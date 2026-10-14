//! DXF import: a tolerant, defensive ASCII reader.
//!
//! The reader accepts any DXF release, because it only looks at the HEADER,
//! TABLES, BLOCKS, and ENTITIES sections, and there at the LAYER, LTYPE,
//! and STYLE tables, at block definitions, and at LINE, CIRCLE, ARC,
//! polyline, TEXT, MTEXT, and INSERT records. Everything else is counted
//! and reported.
//!
//! Tables may come after the entities that use them, in theory, a block
//! may place one defined after it, and a file may name a layer or
//! linetype it never defines; so entities are read with their properties
//! as names, and the names are resolved — rows made for the undefined
//! ones, and reported — once the whole file has been read.

use std::collections::HashMap;

use cad_core::{LengthUnit, Point2, Vec2};
use cad_document::{
    ArcEntity, AttributeDefinitionEntity, Block, BlockId, BlockReferenceEntity, BlockTable,
    CircleEntity, Color, DimStyle, DimStyleId, DimensionEntity, Drawing, DrawingSettings,
    EllipseEntity, Entity, EntityRecord, HatchEntity, HatchStyle, Justification, Layer, LayerId,
    Layout, LayoutId, LineEntity, Linetype, LinetypeDef, LinetypeId, Lineweight, MTextEntity,
    Measure, PointEntity, PointStyle, PolylineEntity, Properties, RawEntity, RawGroup, TextEntity,
    TextStyle, TextStyleId, ViewportEntity, check_name, nearest_index, same_name,
};

use crate::export::insunits_code;
use crate::properties::{
    LAYER_FROZEN, LAYER_LOCKED, LINETYPE_BY_BLOCK, LINETYPE_BY_LAYER, color_from_dxf,
    lineweight_from_dxf, pattern_from_dxf,
};
use crate::text_codes::{plain_mtext, plain_text};
use crate::{DxfError, ImportReport};

/// Start of every binary DXF file.
const BINARY_SENTINEL: &[u8] = b"AutoCAD Binary DXF";
/// Records that belong to the entity before them (POLYLINE vertices, INSERT
/// attributes) and are not counted on their own.
/// Records that follow another and are read with it: none, now that
/// ATTRIB is read for its INSERT (#183).
const SUBORDINATE_RECORDS: [&str; 0] = [];
/// POLYLINE flag bits for 3D polylines, meshes, and polyface meshes.
const POLYLINE_NOT_2D: i64 = 8 | 16 | 32 | 64;
/// Polyline flag bit: closed.
const POLYLINE_CLOSED: i64 = 1;
/// Report key for supported entity types with geometry the drawing cannot hold.
pub(crate) const INVALID: &str = "(invalid geometry)";
/// Report keys for table rows the drawing cannot hold as written.
const BAD_LAYER_NAME: &str = "(layer name)";
const BAD_LINETYPE_NAME: &str = "(linetype name)";
const BAD_LINETYPE_PATTERN: &str = "(linetype pattern)";
const BAD_TEXT_STYLE_NAME: &str = "(text style name)";
const BAD_DIM_STYLE_NAME: &str = "(dimension style name)";
/// Report key for a dimension style with a value a dimension cannot be
/// drawn by: the row is kept with the default for that value.
const BAD_DIM_STYLE: &str = "(dimension style values)";
/// Report keys for the DIMENSION kinds the drawing has no measure for.
const ORDINATE: &str = "(ordinate DIMENSION)";
/// Report key for a pattern too fine for its region at its scale: filled solid.
const TOO_DENSE: &str = "(HATCH pattern too dense)";
const UNKNOWN_DIMENSION: &str = "(DIMENSION of an unknown kind)";
const BAD_BLOCK_NAME: &str = "(block name)";
/// Report key for a layer whose colour is a true colour: the drawing holds
/// the index palette, so the layer takes the nearest index (ADR-0018).
const LAYER_TRUE_COLOUR: &str = "(layer true colour)";
/// Report key for blocks nested deeper than the model follows: what lies
/// below that depth is not drawn.
const NESTING_TOO_DEEP: &str = "(blocks nested too deep)";
/// Report keys for what an INSERT carries that a reference cannot hold:
/// the reference is imported as far as the model holds, the rest said.
const INSERT_NON_UNIFORM: &str = "(INSERT with a non-uniform scale)";
const INSERT_ARRAY: &str = "(INSERT array)";
/// Report key for an INSERT of a block the file never defines: not imported.
const INSERT_UNDEFINED: &str = "(INSERT of an undefined block)";
/// Report key for the text and shapes of a complex linetype: the dashes
/// are kept, the rest is not.
const LINETYPE_SHAPES: &str = "(linetype shapes)";
/// Longest type name kept in the report.
const MAX_KIND_LENGTH: usize = 32;

/// Bounds on what an import may consume (DESIGN §43). Imported files are
/// untrusted: every quantity a file can grow is bounded here, and reaching
/// a bound is a structured error or a report line, never an allocation in
/// proportion to a claimed count.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ImportLimits {
    /// Bytes of file.
    pub max_bytes: usize,
    /// Entities, in the drawing and in block definitions together.
    pub max_entities: usize,
    /// Rows of each table — layers, linetypes, text styles, blocks —
    /// those the file defines and those made for names it uses but never
    /// defines. Past the limit, a defined row is an error and a name that
    /// would make one is treated as a name no row can have.
    pub max_table_rows: usize,
    /// Groups in one record: a polyline's vertices, a paragraph's pieces.
    pub max_record_groups: usize,
    /// Bytes in one group value.
    pub max_value_bytes: usize,
    /// Levels of block nesting the drawing follows; deeper is reported.
    pub max_nesting: usize,
}

impl Default for ImportLimits {
    fn default() -> Self {
        Self {
            max_bytes: 256 * 1024 * 1024,
            max_entities: 2_000_000,
            max_table_rows: 100_000,
            max_record_groups: 1_000_000,
            max_value_bytes: 64 * 1024,
            max_nesting: BlockTable::MAX_NESTING,
        }
    }
}

/// A successfully imported drawing and the account of what was skipped.
#[derive(Debug)]
pub struct Import {
    pub drawing: Drawing,
    pub report: ImportReport,
}

/// Imports an ASCII DXF file with the default limits.
pub fn import(bytes: &[u8]) -> Result<Import, DxfError> {
    import_with_limits(bytes, &ImportLimits::default())
}

/// Imports an ASCII DXF file into a fresh drawing with an empty history.
pub fn import_with_limits(bytes: &[u8], limits: &ImportLimits) -> Result<Import, DxfError> {
    if bytes.len() > limits.max_bytes {
        return Err(DxfError::FileTooLarge {
            bytes: bytes.len(),
            limit: limits.max_bytes,
        });
    }
    if bytes.starts_with(BINARY_SENTINEL) {
        return Err(DxfError::BinaryDxf);
    }
    // Pre-2007 files use a legacy code page. Only text — layer and linetype
    // names — could be affected, and a name that does not decode is still
    // a name, so lossy decoding is safe here.
    let text = String::from_utf8_lossy(bytes);

    let mut reader = Reader {
        groups: Groups::new(&text, limits.max_value_bytes),
        limits,
        entities: Vec::new(),
        tables: Tables::new(limits.max_table_rows),
        header: Header::default(),
        report: ImportReport::default(),
        open_polyline: None,
        open_block: None,
        total: 0,
        paper_blocks: Vec::new(),
        layouts: Vec::new(),
        block_record_names: HashMap::new(),
    };
    reader.read_file()?;
    reader.finish()
}

/// One DXF group: a code and its value, with the line the code was on.
#[derive(Clone, Copy, Debug)]
struct Group<'a> {
    code: i32,
    /// The value with the spaces around it removed: what a number, a name,
    /// or a flag is read from, padded or not.
    value: &'a str,
    /// The value as written, less the line ending: what words are read
    /// from, since the spaces at the ends of a text are its own (#177).
    raw: &'a str,
    line: usize,
}

impl Group<'_> {
    fn is(&self, code: i32, value: &str) -> bool {
        self.code == code && self.value == value
    }
}

/// Iterates over the groups of a file, two lines at a time.
struct Groups<'a> {
    lines: std::str::Lines<'a>,
    line: usize,
    max_value_bytes: usize,
}

impl<'a> Groups<'a> {
    fn new(text: &'a str, max_value_bytes: usize) -> Self {
        Self {
            lines: text.lines(),
            line: 0,
            max_value_bytes,
        }
    }

    /// The next group, or `None` at a clean end of input.
    fn next(&mut self) -> Result<Option<Group<'a>>, DxfError> {
        let Some(code_text) = self.lines.next() else {
            return Ok(None);
        };
        self.line += 1;
        let line = self.line;
        let code = code_text
            .trim()
            .parse::<i32>()
            .map_err(|_| DxfError::InvalidGroupCode { line })?;
        let value = self.lines.next().ok_or(DxfError::UnexpectedEnd { line })?;
        self.line += 1;
        if value.len() > self.max_value_bytes {
            return Err(DxfError::ValueTooLong {
                line: self.line,
                limit: self.max_value_bytes,
            });
        }
        Ok(Some(Group {
            code,
            value: value.trim(),
            raw: value.strip_suffix('\r').unwrap_or(value),
            line,
        }))
    }

    /// The next group; the file must not end here.
    fn expect(&mut self) -> Result<Group<'a>, DxfError> {
        self.next()?.ok_or(DxfError::UnexpectedEnd {
            line: self.line + 1,
        })
    }
}

/// A linetype as an entity, a layer, or the header names it, before the
/// table is known.
#[derive(Clone, Debug, PartialEq)]
enum LinetypeRef {
    ByLayer,
    ByBlock,
    Named(String),
}

impl LinetypeRef {
    fn from_dxf(name: &str) -> Self {
        if name.eq_ignore_ascii_case(LINETYPE_BY_LAYER) {
            Self::ByLayer
        } else if name.eq_ignore_ascii_case(LINETYPE_BY_BLOCK) {
            Self::ByBlock
        } else {
            Self::Named(name.to_owned())
        }
    }
}

/// An entity's properties as the file states them: resolved against the
/// tables once the whole file is read.
#[derive(Clone, Debug)]
struct Pending {
    layer: String,
    color: Color,
    linetype: LinetypeRef,
    lineweight: Lineweight,
    /// A text's style, by name.
    style: Option<String>,
    /// A dimension's style, by name.
    dim_style: Option<String>,
    /// A reference's block, by name.
    block: Option<String>,
    /// It carried something the drawing cannot hold.
    dropped: bool,
    /// Group 67: the entity is in paper space (#191).
    paper: bool,
}

impl Default for Pending {
    fn default() -> Self {
        Self {
            layer: Layer::ZERO.to_owned(),
            color: Color::ByLayer,
            linetype: LinetypeRef::ByLayer,
            lineweight: Lineweight::ByLayer,
            style: None,
            dim_style: None,
            block: None,
            dropped: false,
            paper: false,
        }
    }
}

/// The header variables the drawing has settings for.
#[derive(Debug, Default)]
struct Header {
    current_layer: Option<String>,
    current_dim_style: Option<String>,
    current_color: Option<Color>,
    current_linetype: Option<LinetypeRef>,
    current_lineweight: Option<Lineweight>,
    ltscale: Option<f64>,
    /// The point style (#198): `$PDMODE` and `$PDSIZE`.
    point_mode: Option<i64>,
    point_size: Option<f64>,
}

/// The tables as they are built up: the file's rows, then the rows made
/// for names the file used but never defined.
///
/// A row named like the permanent one — `0`, `Continuous`, `Standard` —
/// restyles it and takes no ID; every other row's ID is its place among
/// the rest, from 1, which is what `Drawing::from_tables` will give it.
/// Each table is indexed by lower-case name, so a file with many rows is
/// read in time proportional to its size.
#[derive(Debug, Default)]
struct Tables {
    /// Each layer with the name of its linetype, resolved at the end.
    layers: Vec<(Layer, LinetypeRef)>,
    linetypes: Vec<LinetypeDef>,
    text_styles: Vec<TextStyle>,
    dim_styles: Vec<DimStyle>,
    /// The file's block definitions, in order; their IDs follow from 1.
    blocks: Vec<BlockBuilder>,
    /// Lower-case name → the ID the row will have, or none for a row that
    /// restyles the permanent one.
    layer_ids: HashMap<String, Option<LayerId>>,
    linetype_ids: HashMap<String, Option<LinetypeId>>,
    text_style_ids: HashMap<String, Option<TextStyleId>>,
    dim_style_ids: HashMap<String, Option<DimStyleId>>,
    block_ids: HashMap<String, BlockId>,
    created_layers: usize,
    created_linetypes: usize,
    created_text_styles: usize,
    created_dim_styles: usize,
    /// Rows a table may have.
    max_rows: usize,
}

/// A block definition as it is read: its entities with their properties
/// as names, like the drawing's own.
#[derive(Debug)]
struct BlockBuilder {
    name: String,
    base: Point2,
    entities: Vec<(Entity, Pending)>,
}

/// A layout's entities as read, before their names are resolved.
type PaperEntities = Vec<(Entity, Pending)>;

/// A paper-space block (#191): the entities of a layout, read like the
/// drawing's own, and the records the drawing has no entity for, kept as
/// the file held them (ADR-0025, narrowed).
#[derive(Debug, Default)]
struct PaperBlock {
    name: String,
    entities: Vec<(Entity, Pending)>,
    kept: Vec<RawEntity>,
}

/// The block definition the reader is inside, in the BLOCKS section.
#[derive(Debug)]
enum OpenBlock {
    /// A definition the drawing will hold.
    User(BlockBuilder),
    /// A paper-space block: a layout's entities (#191).
    Paper(PaperBlock),
    /// Model space, paper space, an anonymous block, one with a name no
    /// block can have, or a second definition of a name: its entities are
    /// not user content and are skipped.
    Skipped,
}

/// The table is full: one more row is one too many.
struct TableFull;

impl Tables {
    fn new(max_rows: usize) -> Self {
        Self {
            max_rows,
            ..Self::default()
        }
    }

    fn key(name: &str) -> String {
        name.to_lowercase()
    }

    fn has_block(&self, name: &str) -> bool {
        self.block_ids.contains_key(&Self::key(name))
    }

    /// The ID the block of that name will have, if the file defined it.
    fn block(&self, name: &str) -> Option<BlockId> {
        self.block_ids.get(&Self::key(name)).copied()
    }

    /// Keeps a definition the file made; its ID is its place, from 1.
    fn add_block(&mut self, block: BlockBuilder) -> Result<(), TableFull> {
        if self.blocks.len() >= self.max_rows {
            return Err(TableFull);
        }
        let id = BlockId::from_raw(self.blocks.len() as u32 + 1);
        self.block_ids.insert(Self::key(&block.name), id);
        self.blocks.push(block);
        Ok(())
    }

    fn has_text_style(&self, name: &str) -> bool {
        self.text_style_ids.contains_key(&Self::key(name))
    }

    /// Keeps a text style the file defined.
    fn add_text_style(&mut self, style: TextStyle) -> Result<(), TableFull> {
        if self.text_styles.len() >= self.max_rows {
            return Err(TableFull);
        }
        let id = (!same_name(&style.name, TextStyle::STANDARD)).then(|| {
            let with_ids = self
                .text_style_ids
                .values()
                .filter(|id| id.is_some())
                .count();
            TextStyleId::from_raw(with_ids as u32 + 1)
        });
        self.text_style_ids.insert(Self::key(&style.name), id);
        self.text_styles.push(style);
        Ok(())
    }

    /// The ID the text style of that name will have, making it if the file
    /// did not define it. `None` for a name no style can have — or for one
    /// there is no room left to make.
    fn text_style(&mut self, name: &str) -> Option<TextStyleId> {
        if same_name(name, TextStyle::STANDARD) {
            return Some(TextStyleId::FIRST);
        }
        if !self.has_text_style(name) {
            check_name(name).ok()?;
            self.add_text_style(TextStyle::named(name)).ok()?;
            self.created_text_styles += 1;
        }
        self.text_style_ids.get(&Self::key(name)).copied().flatten()
    }

    fn has_dim_style(&self, name: &str) -> bool {
        self.dim_style_ids.contains_key(&Self::key(name))
    }

    /// The ID the dimension style of that name will have, making it with
    /// the defaults if the file did not define it. `None` for a name no
    /// style can have, or one there is no room left to make.
    fn dim_style(&mut self, name: &str) -> Option<DimStyleId> {
        if same_name(name, DimStyle::STANDARD) {
            return Some(DimStyleId::FIRST);
        }
        if !self.has_dim_style(name) {
            check_name(name).ok()?;
            self.add_dim_style(DimStyle::named(name)).ok()?;
            self.created_dim_styles += 1;
        }
        self.dim_style_ids.get(&Self::key(name)).copied().flatten()
    }

    /// Keeps a dimension style the file defined.
    fn add_dim_style(&mut self, style: DimStyle) -> Result<(), TableFull> {
        if self.dim_styles.len() >= self.max_rows {
            return Err(TableFull);
        }
        let id = (!same_name(&style.name, DimStyle::STANDARD)).then(|| {
            let with_ids = self
                .dim_style_ids
                .values()
                .filter(|id| id.is_some())
                .count();
            DimStyleId::from_raw(with_ids as u32 + 1)
        });
        self.dim_style_ids.insert(Self::key(&style.name), id);
        self.dim_styles.push(style);
        Ok(())
    }

    fn has_layer(&self, name: &str) -> bool {
        self.layer_ids.contains_key(&Self::key(name))
    }

    /// Keeps a layer the file defined, with the name of its linetype.
    fn add_layer(&mut self, layer: Layer, linetype: LinetypeRef) -> Result<(), TableFull> {
        if self.layers.len() >= self.max_rows {
            return Err(TableFull);
        }
        let id = (!same_name(&layer.name, Layer::ZERO)).then(|| {
            let with_ids = self.layer_ids.values().filter(|id| id.is_some()).count();
            LayerId::from_raw(with_ids as u32 + 1)
        });
        self.layer_ids.insert(Self::key(&layer.name), id);
        self.layers.push((layer, linetype));
        Ok(())
    }

    /// The ID the layer of that name will have, making the layer with a
    /// new layer's settings if the file did not define it. `None` for a
    /// name no layer can have — or for one there is no room left to make.
    fn layer(&mut self, name: &str) -> Option<LayerId> {
        if same_name(name, Layer::ZERO) {
            return Some(LayerId::FIRST);
        }
        if !self.has_layer(name) {
            check_name(name).ok()?;
            self.add_layer(
                Layer::named(name),
                LinetypeRef::Named(LinetypeDef::CONTINUOUS.to_owned()),
            )
            .ok()?;
            self.created_layers += 1;
        }
        self.layer_ids.get(&Self::key(name)).copied().flatten()
    }

    fn has_linetype(&self, name: &str) -> bool {
        self.linetype_ids.contains_key(&Self::key(name))
    }

    /// Keeps a linetype the file defined.
    fn add_linetype(&mut self, def: LinetypeDef) -> Result<(), TableFull> {
        if self.linetypes.len() >= self.max_rows {
            return Err(TableFull);
        }
        let id = (!same_name(&def.name, LinetypeDef::CONTINUOUS)).then(|| {
            let with_ids = self.linetype_ids.values().filter(|id| id.is_some()).count();
            LinetypeId::from_raw(with_ids as u32 + 1)
        });
        self.linetype_ids.insert(Self::key(&def.name), id);
        self.linetypes.push(def);
        Ok(())
    }

    /// The linetype a reference means, making a continuous linetype of the
    /// name if the file did not define it. `None` for a name no linetype
    /// can have — or for one there is no room left to make.
    fn linetype(&mut self, reference: &LinetypeRef) -> Option<Linetype> {
        let name = match reference {
            LinetypeRef::ByLayer => return Some(Linetype::ByLayer),
            LinetypeRef::ByBlock => return Some(Linetype::ByBlock),
            LinetypeRef::Named(name) => name,
        };
        if same_name(name, LinetypeDef::CONTINUOUS) {
            return Some(Linetype::CONTINUOUS);
        }
        if !self.has_linetype(name) {
            check_name(name).ok()?;
            self.add_linetype(LinetypeDef {
                name: name.clone(),
                description: String::new(),
                pattern: Vec::new(),
            })
            .ok()?;
            self.created_linetypes += 1;
        }
        self.linetype_ids
            .get(&Self::key(name))
            .copied()
            .flatten()
            .map(Linetype::Named)
    }
}

struct Reader<'a> {
    groups: Groups<'a>,
    limits: &'a ImportLimits,
    entities: Vec<(Entity, Pending)>,
    tables: Tables,
    header: Header,
    report: ImportReport,
    /// An R12-style POLYLINE whose VERTEX records are still being read.
    open_polyline: Option<PolylineBuilder>,
    /// The block definition being read, in the BLOCKS section.
    open_block: Option<OpenBlock>,
    /// Entities read so far, in the drawing and in its blocks together,
    /// against the limit.
    total: usize,
    /// Paper-space blocks: each layout's entities (#191). The one named
    /// `*Paper_Space` also takes what the ENTITIES section flags as paper
    /// (67), as R12 and some writers put the active layout there.
    paper_blocks: Vec<PaperBlock>,
    /// LAYOUT objects as the file held them.
    layouts: Vec<Vec<RawGroup>>,
    /// Block record handles to their names, to pair a layout with its block.
    block_record_names: HashMap<String, String>,
}

/// Collects a polyline that may span several records.
#[derive(Debug, Default)]
struct PolylineBuilder {
    vertices: Vec<Point2>,
    /// One per vertex: the bulge of the segment that starts there.
    bulges: Vec<f64>,
    closed: bool,
    /// Not a 2D polyline at all (mesh, 3D polyline): already reported; its
    /// vertices are swallowed.
    unsupported: bool,
    properties: Pending,
}

impl<'a> Reader<'a> {
    fn read_file(&mut self) -> Result<(), DxfError> {
        loop {
            let group = self.groups.expect()?;
            if group.is(0, "EOF") {
                return Ok(());
            }
            if group.is(0, "SECTION") {
                let name = self.groups.expect()?;
                match name.value {
                    "HEADER" if name.code == 2 => self.read_header()?,
                    "TABLES" if name.code == 2 => self.read_records(RecordContext::Tables)?,
                    "ENTITIES" if name.code == 2 => self.read_records(RecordContext::Entities)?,
                    "BLOCKS" if name.code == 2 => self.read_records(RecordContext::Blocks)?,
                    "OBJECTS" if name.code == 2 => self.read_records(RecordContext::Objects)?,
                    _ => self.skip_section()?,
                }
            }
            // Anything else between sections (comments, stray groups) is ignored.
        }
    }

    /// Reads the header variables the drawing has settings for: its unit,
    /// `$INSUNITS`; the current layer, colour, linetype, and lineweight;
    /// and `$LTSCALE`. Every other variable is skipped.
    fn read_header(&mut self) -> Result<(), DxfError> {
        let mut variable = "";
        loop {
            let group = self.groups.expect()?;
            if group.is(0, "ENDSEC") {
                return Ok(());
            }
            if group.code == 9 {
                variable = group.value;
                continue;
            }
            let number = group.value.trim().parse::<i64>().ok();
            match (variable, group.code) {
                // Not a whole number: no unit is declared, as if absent.
                ("$INSUNITS", 70) => {
                    if let Some(code) = number {
                        self.declare_unit(code);
                    }
                }
                ("$CLAYER", 8) => self.header.current_layer = Some(group.value.to_owned()),
                ("$DIMSTYLE", 2) => self.header.current_dim_style = Some(group.value.to_owned()),
                ("$CECOLOR", 62) => self.header.current_color = number.and_then(color_from_dxf),
                ("$CELTYPE", 6) => {
                    self.header.current_linetype = Some(LinetypeRef::from_dxf(group.value));
                }
                ("$CELWEIGHT", 370) => {
                    self.header.current_lineweight = number.and_then(lineweight_from_dxf);
                }
                ("$LTSCALE", 40) => self.header.ltscale = group.value.parse::<f64>().ok(),
                ("$PDMODE", 70) => self.header.point_mode = number,
                ("$PDSIZE", 40) => self.header.point_size = group.value.parse::<f64>().ok(),
                _ => continue,
            }
            variable = "";
        }
    }

    /// Takes the file's `$INSUNITS` code: a unit the drawing can be in, no
    /// unit, or one it cannot be in — reported, and the drawing unitless.
    /// The point style the header gave (#198): a mode the drawing can
    /// draw, or the default; a size in drawing units, or the default when
    /// the file's is relative to the screen (zero or negative), which the
    /// drawing does not hold and the report says so.
    fn point_style(&mut self) -> PointStyle {
        let default = PointStyle::default();
        let mode = self
            .header
            .point_mode
            .and_then(|mode| u8::try_from(mode).ok())
            .filter(|mode| PointStyle::new(*mode, 1.0).check().is_ok())
            .unwrap_or(default.mode);
        let size = match self.header.point_size {
            Some(size) if size.is_finite() && size > 0.0 => size,
            Some(size) if size.is_finite() => {
                self.report.point_size_relative = true;
                default.size
            }
            _ => default.size,
        };
        PointStyle::new(mode, size)
    }

    fn declare_unit(&mut self, code: i64) {
        let unit = LengthUnit::ALL
            .into_iter()
            .find(|unit| insunits_code(*unit) == code);
        self.report.unit = unit;
        self.report.unsupported_unit = (unit.is_none() && code != 0).then_some(code);
    }

    fn skip_section(&mut self) -> Result<(), DxfError> {
        while !self.groups.expect()?.is(0, "ENDSEC") {}
        Ok(())
    }

    /// Reads the `0`-delimited records of a section up to its `ENDSEC`.
    fn read_records(&mut self, context: RecordContext) -> Result<(), DxfError> {
        let mut record = self.groups.expect()?;
        loop {
            if record.code != 0 {
                // Groups before the first record of the section: ignore.
                record = self.groups.expect()?;
                continue;
            }
            if record.value == "ENDSEC" {
                return Ok(());
            }
            let mut body = RecordBody::default();
            // Records read by walking their groups, and everything a
            // layout keeps (#184, #191): a paper-space block's records the
            // drawing has no entity for, LAYOUT objects, and BLOCK_RECORD
            // rows, whose handles name the layouts' blocks.
            let keep_raw = matches!(record.value, "HATCH" | "SPLINE" | "LAYOUT" | "BLOCK_RECORD")
                || matches!(self.open_block, Some(OpenBlock::Paper(_)));
            if keep_raw {
                body.raw = Some(Vec::new());
            }
            let mut groups = 0;
            let next = loop {
                let group = self.groups.expect()?;
                if group.code == 0 {
                    break group;
                }
                groups += 1;
                if groups > self.limits.max_record_groups {
                    return Err(DxfError::RecordTooLarge {
                        line: group.line,
                        limit: self.limits.max_record_groups,
                    });
                }
                if let Some(raw) = &mut body.raw {
                    raw.push((group.code, group.value.to_owned()));
                }
                body.absorb(group)?;
            };
            self.finish_record(context, record, &body)?;
            record = next;
        }
    }

    fn finish_record(
        &mut self,
        context: RecordContext,
        record: Group<'_>,
        body: &RecordBody,
    ) -> Result<(), DxfError> {
        // In the BLOCKS section, a user definition's entities and a
        // paper-space block's count (#191); a skipped block's do not.
        if matches!(context, RecordContext::Blocks) {
            match record.value {
                "BLOCK" => {
                    self.open_block(body);
                    return Ok(());
                }
                "ENDBLK" => return self.close_block(),
                _ if !matches!(
                    self.open_block,
                    Some(OpenBlock::User(_) | OpenBlock::Paper(_))
                ) =>
                {
                    return Ok(());
                }
                _ => {}
            }
        }
        if matches!(context, RecordContext::Objects) {
            if record.value == "LAYOUT"
                && let Some(raw) = &body.raw
            {
                self.layouts.push(raw.clone());
            }
            return Ok(());
        }
        if matches!(context, RecordContext::Tables)
            && record.value == "BLOCK_RECORD"
            && let (Some(raw), Some(name)) = (&body.raw, &body.name)
            && let Some((_, handle)) = raw.iter().find(|(code, _)| *code == 5)
        {
            self.block_record_names
                .insert(handle.trim().to_uppercase(), name.clone());
        }
        let entities = matches!(context, RecordContext::Entities | RecordContext::Blocks);
        match (context, record.value) {
            (_, "LINE") if entities => {
                let line = body.line(record.line)?;
                self.import(Entity::Line(line), body)?;
            }
            (_, "POINT") if entities => {
                let point = body.point(record.line)?;
                self.import(Entity::Point(point), body)?;
            }
            (_, "CIRCLE") if entities => match body.circle(record.line)? {
                Some(circle) => self.import(Entity::Circle(circle), body)?,
                None => self.report.count_unsupported(INVALID),
            },
            (_, "ARC") if entities => match body.arc(record.line)? {
                Some(arc) => self.import(Entity::Arc(arc), body)?,
                None => self.report.count_unsupported(INVALID),
            },
            (_, "ELLIPSE") if entities => match body.ellipse(record.line)? {
                Some(ellipse) => self.import(Entity::Ellipse(ellipse), body)?,
                None => self.report.count_unsupported(INVALID),
            },
            (_, "TEXT") if entities => {
                let (text, dropped) = body.text(record.line)?;
                let mut pending = body.pending();
                pending.dropped |= dropped;
                self.push(Entity::Text(text), pending)?;
            }
            // An attribute definition (#183): a text with a tag (2), a
            // prompt (3), and flags (70); its words are the default.
            (_, "ATTDEF") if entities => {
                let (text, dropped) = body.text(record.line)?;
                let mut pending = body.pending();
                pending.dropped |= dropped;
                let mut definition = AttributeDefinitionEntity::new(
                    body.name.clone().unwrap_or_default(),
                    body.description.clone().unwrap_or_default(),
                    text,
                );
                definition.invisible = body.flags & 1 != 0;
                self.push(Entity::AttributeDefinition(definition), pending)?;
            }
            // An attribute (#183): the value (1) its tag (2) has on the
            // INSERT just read, which it follows until SEQEND.
            (_, "ATTRIB") if entities => {
                let tag = body.name.clone().unwrap_or_default();
                let (value, _) = plain_text(body.text.as_deref().unwrap_or(""));
                self.attach_attribute(&tag, &value);
            }
            (_, "MTEXT") if entities => {
                let (text, formatting) = body.mtext(record.line)?;
                if formatting {
                    self.report.formatting_dropped += 1;
                }
                self.push(Entity::MText(text), body.pending())?;
            }
            (_, "INSERT") if entities => self.insert(record, body)?,
            (_, "DIMENSION") if entities => self.dimension(body)?,
            (_, "HATCH") if entities => self.hatch(body)?,
            (_, "SPLINE") if entities => match body.raw.as_deref().and_then(crate::spline::read) {
                Some(spline) => self.import(Entity::Spline(spline), body)?,
                None => self.report.count_unsupported(INVALID),
            },
            (RecordContext::Tables, "STYLE") => self.table_text_style(body)?,
            (RecordContext::Tables, "DIMSTYLE") => self.table_dim_style(body)?,
            (_, "LWPOLYLINE") if entities => {
                let mut properties = body.pending();
                properties.dropped |= body.constant_width.is_some_and(|width| width != 0.0);
                let builder = PolylineBuilder {
                    vertices: body.points(record.line, "LWPOLYLINE")?,
                    closed: body.flags & POLYLINE_CLOSED != 0,
                    bulges: body.bulges.clone(),
                    unsupported: false,
                    properties,
                };
                self.finish_polyline(builder)?;
            }
            (_, "POLYLINE") if entities => {
                let unsupported = body.flags & POLYLINE_NOT_2D != 0;
                if unsupported {
                    self.report.count_unsupported("POLYLINE");
                }
                self.open_polyline = Some(PolylineBuilder {
                    closed: body.flags & POLYLINE_CLOSED != 0,
                    unsupported,
                    properties: body.pending(),
                    ..Default::default()
                });
            }
            (_, "VERTEX") if entities => {
                let open = self
                    .open_polyline
                    .as_mut()
                    .filter(|polyline| !polyline.unsupported);
                if let Some(polyline) = open {
                    polyline
                        .vertices
                        .extend(body.points(record.line, "VERTEX")?);
                    polyline.bulges.extend(&body.bulges);
                }
            }
            (_, "SEQEND") if entities => {
                let finished = self
                    .open_polyline
                    .take()
                    .filter(|polyline| !polyline.unsupported);
                if let Some(polyline) = finished {
                    self.finish_polyline(polyline)?;
                }
            }
            // A VIEWPORT (#192) is an entity of a layout; one in model
            // space — a tiled viewport of an old file — the drawing has
            // no place for.
            (_, "VIEWPORT") if entities => {
                let on_paper = matches!(self.open_block, Some(OpenBlock::Paper(_))) || body.paper;
                match body.viewport(record.line)? {
                    Some(viewport) if on_paper => self.import(Entity::Viewport(viewport), body)?,
                    Some(_) => self.report.count_unsupported("VIEWPORT"),
                    None => self.report.count_unsupported(INVALID),
                }
            }
            // A record of a layout's paper the drawing has no entity for
            // is kept as it is, to be written back (#191).
            (_, kind) if entities && !SUBORDINATE_RECORDS.contains(&kind) => {
                match (&mut self.open_block, &body.raw) {
                    (Some(OpenBlock::Paper(paper)), Some(raw)) => paper.kept.push(RawEntity {
                        kind: kind.to_owned(),
                        groups: raw.clone(),
                    }),
                    _ => self.report.count_unsupported(&kind_name(kind)),
                }
            }
            (RecordContext::Tables, "LAYER") => self.table_layer(body)?,
            (RecordContext::Tables, "LTYPE") => self.table_linetype(body)?,
            _ => {}
        }
        Ok(())
    }

    /// A BLOCK record: the start of a definition. The model- and
    /// paper-space containers every complete file has, anonymous blocks,
    /// and names no block can have are skipped with their contents, as is
    /// a second definition of a name; the first counts.
    fn open_block(&mut self, body: &RecordBody) {
        let name = body.name.as_deref().unwrap_or("");
        if is_paper_space(name) {
            self.open_block = Some(OpenBlock::Paper(PaperBlock {
                name: name.to_owned(),
                ..PaperBlock::default()
            }));
            return;
        }
        if body.name_is_reserved {
            self.open_block = Some(OpenBlock::Skipped);
            return;
        }
        if check_name(name).is_err() {
            self.report.count_unsupported(BAD_BLOCK_NAME);
            self.open_block = Some(OpenBlock::Skipped);
            return;
        }
        if self.tables.has_block(name) {
            self.open_block = Some(OpenBlock::Skipped);
            return;
        }
        let base = Point2::new(body.x0.unwrap_or(0.0), body.y0.unwrap_or(0.0));
        self.open_block = Some(OpenBlock::User(BlockBuilder {
            name: name.to_owned(),
            base,
            entities: Vec::new(),
        }));
    }

    /// An ENDBLK record: the definition is complete. A polyline left open
    /// inside it is finished with it.
    fn close_block(&mut self) -> Result<(), DxfError> {
        self.open_polyline = None;
        if let Some(OpenBlock::Paper(_)) = &self.open_block
            && let Some(OpenBlock::Paper(paper)) = self.open_block.take()
        {
            self.paper_blocks.push(paper);
            return Ok(());
        }
        if let Some(OpenBlock::User(block)) = self.open_block.take() {
            self.tables
                .add_block(block)
                .map_err(|TableFull| self.too_many_rows("BLOCK"))?;
        }
        Ok(())
    }

    /// An INSERT: a reference to a block by name, placed. DXF scales a
    /// reference along X and Y separately; the drawing holds one scale and
    /// a mirror. Equal magnitudes with one sign flipped are a mirror —
    /// across X directly, across Y as a mirror and a half turn — and
    /// unequal ones are imported at the X magnitude and reported. Arrays
    /// (column or row counts above one) and attributes are reported and the
    /// one reference imported.
    fn insert(&mut self, record: Group<'_>, body: &RecordBody) -> Result<(), DxfError> {
        let Some(name) = body.name.clone() else {
            self.report.count_unsupported(INVALID);
            return Ok(());
        };
        let need = |value, group| required(value, record.line, "INSERT", group);
        let insertion = Point2::new(need(body.x0, 10)?, need(body.y0, 20)?);
        let (sx, sy) = (body.width.unwrap_or(1.0), body.scale_y.unwrap_or(1.0));
        if sx == 0.0 || sy == 0.0 {
            self.report.count_unsupported(INVALID);
            return Ok(());
        }
        // Exactly equal magnitudes are one scale; anything else is two.
        #[allow(clippy::float_cmp)]
        if sx.abs() != sy.abs() {
            self.report.count_unsupported(INSERT_NON_UNIFORM);
        }
        if body.flags > 1 || body.attachment.unwrap_or(1) > 1 {
            self.report.count_unsupported(INSERT_ARRAY);
        }
        let mut rotation = body.start_degrees.unwrap_or(0.0).to_radians();
        if sx < 0.0 {
            rotation += std::f64::consts::PI;
        }
        let reference = BlockReferenceEntity {
            block: BlockId::FIRST,
            insertion,
            scale: sx.abs(),
            rotation,
            mirrored: (sx < 0.0) != (sy < 0.0),
            attributes: Vec::new(),
        };
        let mut pending = body.pending();
        pending.block = Some(name);
        self.push(Entity::BlockRef(reference), pending)
    }

    /// A row of the LAYER table. Its name must be one a layer can have; the
    /// first row of a name counts. A negative colour, or the frozen flag,
    /// is a layer that is off.
    fn table_layer(&mut self, body: &RecordBody) -> Result<(), DxfError> {
        let Some(name) = body.name.as_deref().filter(|name| check_name(name).is_ok()) else {
            self.report.count_unsupported(BAD_LAYER_NAME);
            return Ok(());
        };
        if self.tables.has_layer(name) {
            return Ok(());
        }
        let mut layer = Layer::named(name);
        if let Some(code) = body.color {
            layer.on = code >= 0;
        }
        let (color, true_colour_dropped) = body.colour();
        if let Some(Color::Index(index)) = color {
            layer.color = Color::Index(index);
        }
        if true_colour_dropped {
            self.report.count_unsupported(LAYER_TRUE_COLOUR);
        }
        if body.flags & LAYER_FROZEN != 0 {
            layer.on = false;
        }
        layer.locked = body.flags & LAYER_LOCKED != 0;
        if let Some(Lineweight::Hundredths(hundredths)) =
            body.lineweight.and_then(lineweight_from_dxf)
        {
            layer.lineweight = Lineweight::Hundredths(hundredths);
        }
        let linetype = body.linetype.as_deref().map_or(
            LinetypeRef::Named(LinetypeDef::CONTINUOUS.to_owned()),
            |name| {
                match LinetypeRef::from_dxf(name) {
                    // A layer's linetype is what ByLayer means.
                    LinetypeRef::ByLayer | LinetypeRef::ByBlock => {
                        LinetypeRef::Named(LinetypeDef::CONTINUOUS.to_owned())
                    }
                    named => named,
                }
            },
        );
        self.tables
            .add_layer(layer, linetype)
            .map_err(|TableFull| self.too_many_rows("LAYER"))
    }

    fn too_many_rows(&self, table: &'static str) -> DxfError {
        DxfError::TooManyRows {
            table,
            limit: self.limits.max_table_rows,
        }
    }

    /// A row of the LTYPE table. The dashes of a complex linetype are kept
    /// and its shapes reported; a pattern the drawing cannot hold is
    /// reported and the linetype is continuous. The `ByLayer` and
    /// `ByBlock` rows newer files carry are not linetypes.
    fn table_linetype(&mut self, body: &RecordBody) -> Result<(), DxfError> {
        let Some(name) = body.name.as_deref().filter(|name| check_name(name).is_ok()) else {
            self.report.count_unsupported(BAD_LINETYPE_NAME);
            return Ok(());
        };
        if !matches!(LinetypeRef::from_dxf(name), LinetypeRef::Named(_))
            || self.tables.has_linetype(name)
        {
            return Ok(());
        }
        let mut pattern = pattern_from_dxf(&body.elements).unwrap_or_else(|| {
            self.report.count_unsupported(BAD_LINETYPE_PATTERN);
            Vec::new()
        });
        if body.has_shapes {
            self.report.count_unsupported(LINETYPE_SHAPES);
        }
        let description = body.description.clone().unwrap_or_default();
        let name = if same_name(name, LinetypeDef::CONTINUOUS) {
            // Continuous is continuous, whatever a file says of it.
            pattern.clear();
            LinetypeDef::CONTINUOUS.to_owned()
        } else {
            name.to_owned()
        };
        self.tables
            .add_linetype(LinetypeDef {
                name,
                description,
                pattern,
            })
            .map_err(|TableFull| self.too_many_rows("LTYPE"))
    }

    /// A row of the STYLE table: a name is all the drawing keeps of it.
    /// A DIMENSION: its kind from the low bits of 70 — rotated (0) and
    /// aligned (1) from their two points (13, 14) and the dimension line's
    /// (10), rotated with its angle (50); the angle between two lines (2)
    /// from the lines' points (13–14 and 15–10) and the arc's (16); a
    /// diameter (3) from the two points on the circle (10, 15); a radius
    /// (4) from the centre (10) and the point on the circle (15); the
    /// angle at a vertex (5) from the vertex (15), the points (13, 14), and
    /// the arc's (10). Where the text is (11) places a radius or diameter.
    /// The text (1) is the drafter's unless empty or `<>`; the text's angle
    /// (53) is a turn away from where it would read on its own. The style
    /// is named (3) and made if the file did not define it; the anonymous
    /// block is not read, the layout engine drawing the dimension afresh
    /// (ADR-0020). An ordinate dimension (6), one of a kind unknown, and
    /// one that cannot be laid out are reported.
    fn dimension(&mut self, body: &RecordBody) -> Result<(), DxfError> {
        let point = |x: Option<f64>, y: Option<f64>| Some(Point2::new(x?, y?));
        let (p10, p11) = (point(body.x0, body.y0), point(body.x1, body.y1));
        let (p13, p14) = (point(body.x13, body.y13), point(body.x14, body.y14));
        let (p15, p16) = (point(body.x15, body.y15), point(body.x16, body.y16));
        let built = match body.flags & 15 {
            kind @ (0 | 1) => (|| {
                let (a, b, place) = (p13?, p14?, p10?);
                let measure = if kind == 0 {
                    let rotation = body.start_degrees.unwrap_or(0.0).to_radians();
                    Measure::Linear { a, b, rotation }
                } else {
                    Measure::Aligned { a, b }
                };
                Some(DimensionEntity::new(measure, place))
            })(),
            2 => (|| {
                let place = p16?;
                let measure = Measure::between_line_segments([p13?, p14?], [p15?, p10?], place)?;
                Some(DimensionEntity::new(measure, place))
            })(),
            3 => (|| {
                let (far, near) = (p10?, p15?);
                let center = far + (near - far) * 0.5;
                let measure = Measure::Diameter {
                    center,
                    radius: center.distance_to(near),
                };
                Some(DimensionEntity::new(measure, p11.unwrap_or(near)))
            })(),
            4 => (|| {
                let (center, near) = (p10?, p15?);
                let measure = Measure::Radius {
                    center,
                    radius: center.distance_to(near),
                };
                Some(DimensionEntity::new(measure, p11.unwrap_or(near)))
            })(),
            5 => (|| {
                let (vertex, a, b, place) = (p15?, p13?, p14?, p10?);
                Some(DimensionEntity::new(
                    Measure::Angular { vertex, a, b },
                    place,
                ))
            })(),
            6 => {
                self.report.count_unsupported(ORDINATE);
                return Ok(());
            }
            _ => {
                self.report.count_unsupported(UNKNOWN_DIMENSION);
                return Ok(());
            }
        };
        let Some(mut dimension) = built else {
            self.report.count_unsupported(INVALID);
            return Ok(());
        };
        match body.text.as_deref().map(str::trim) {
            None | Some("") | Some("<>") => {}
            Some(words) => dimension.text_override = Some(words.to_owned()),
        }
        if let Some(turn) = body.text_rotation
            && turn != 0.0
        {
            let on_its_own = dimension
                .layout(&DimStyle::named(DimStyle::STANDARD))
                .text
                .rotation;
            dimension.text_angle = Some(on_its_own + turn.to_radians());
        }
        if dimension.check().is_err() {
            self.report.count_unsupported(INVALID);
            return Ok(());
        }
        let mut pending = body.pending();
        pending.dim_style = body.description.clone();
        self.push(Entity::Dimension(dimension), pending)
    }

    /// A HATCH (ADR-0021), read from its groups in order by
    /// `crate::hatch`: its polyline and edge boundary paths — lines and
    /// arcs; a path with an ellipse or a spline is reported and left out —
    /// and its fill, solid or a pattern from the table with its angle,
    /// scale, and origin. A pattern the table lacks is filled solid and
    /// reported; so is a gradient. A hatch with no boundary left, or one
    /// that bounds no region, is reported.
    fn hatch(&mut self, body: &RecordBody) -> Result<(), DxfError> {
        let Some(read) = body.raw.as_deref().and_then(crate::hatch::read) else {
            self.report.count_unsupported(INVALID);
            return Ok(());
        };
        for key in &read.dropped {
            self.report.count_unsupported(key);
        }
        let hatch = HatchEntity::solid(read.loops).styled(read.style.unwrap_or(HatchStyle::Solid));
        match hatch.check() {
            Ok(()) => self.push(Entity::Hatch(hatch), body.pending()),
            // A pattern too fine to draw is filled solid instead.
            Err(cad_document::HatchError::TooDense) => {
                self.report.count_unsupported(TOO_DENSE);
                self.push(
                    Entity::Hatch(hatch.styled(HatchStyle::Solid)),
                    body.pending(),
                )
            }
            Err(_) => {
                if read.dropped.is_empty() {
                    self.report.count_unsupported(INVALID);
                }
                Ok(())
            }
        }
    }

    /// A row of the DIMSTYLE table: the name and the variables the drawing
    /// holds — text height (140), arrow size (41), text gap (147), extension
    /// offset (42) and overshoot (44), decimals (271), and the suffix
    /// (DIMPOST, 3, where `<>` stands for the value). A value a dimension
    /// cannot be drawn by is reported and the default stands.
    fn table_dim_style(&mut self, body: &RecordBody) -> Result<(), DxfError> {
        let Some(name) = body.name.as_deref().filter(|name| check_name(name).is_ok()) else {
            self.report.count_unsupported(BAD_DIM_STYLE_NAME);
            return Ok(());
        };
        if self.tables.has_dim_style(name) {
            return Ok(());
        }
        let name = if same_name(name, DimStyle::STANDARD) {
            DimStyle::STANDARD
        } else {
            name
        };
        let mut style = DimStyle::named(name);
        let defaults = style.clone();
        let mut dropped = false;
        let mut take = |slot: &mut f64, value: Option<f64>, default: f64, positive: bool| {
            if let Some(value) = value {
                if value.is_finite() && (value > 0.0 || (!positive && value == 0.0)) {
                    *slot = value;
                } else {
                    *slot = default;
                    dropped = true;
                }
            }
        };
        take(
            &mut style.text_height,
            body.dim_text_height,
            defaults.text_height,
            true,
        );
        take(&mut style.arrow_size, body.width, defaults.arrow_size, true);
        take(&mut style.text_gap, body.dim_gap, defaults.text_gap, false);
        take(
            &mut style.extension_offset,
            body.scale_y,
            defaults.extension_offset,
            false,
        );
        take(
            &mut style.extension_overshoot,
            body.dim_overshoot,
            defaults.extension_overshoot,
            false,
        );
        if let Some(decimals) = body.dim_decimals {
            match u8::try_from(decimals) {
                Ok(decimals) if decimals <= DimStyle::MAX_DECIMALS => style.decimals = decimals,
                _ => dropped = true,
            }
        }
        if let Some(post) = body.description.as_deref() {
            style.suffix = post
                .split_once("<>")
                .map_or(post, |(_, after)| after)
                .to_owned();
        }
        if dropped {
            self.report.count_unsupported(BAD_DIM_STYLE);
        }
        self.tables
            .add_dim_style(style)
            .map_err(|TableFull| self.too_many_rows("DIMSTYLE"))
    }

    fn table_text_style(&mut self, body: &RecordBody) -> Result<(), DxfError> {
        let Some(name) = body.name.as_deref().filter(|name| check_name(name).is_ok()) else {
            self.report.count_unsupported(BAD_TEXT_STYLE_NAME);
            return Ok(());
        };
        if self.tables.has_text_style(name) {
            return Ok(());
        }
        let name = if same_name(name, TextStyle::STANDARD) {
            TextStyle::STANDARD
        } else {
            name
        };
        self.tables
            .add_text_style(TextStyle::named(name))
            .map_err(|TableFull| self.too_many_rows("STYLE"))
    }

    fn import(&mut self, entity: Entity, body: &RecordBody) -> Result<(), DxfError> {
        self.push(entity, body.pending())
    }

    /// Gives the reference read last — in the drawing, or in the block
    /// being read — the value of one of its attributes (#183). An ATTRIB
    /// after anything else is dropped.
    fn attach_attribute(&mut self, tag: &str, value: &str) {
        let last = match &mut self.open_block {
            Some(OpenBlock::User(block)) => block.entities.last_mut(),
            Some(OpenBlock::Paper(paper)) => paper.entities.last_mut(),
            Some(OpenBlock::Skipped) => None,
            None => self.entities.last_mut(),
        };
        if let Some((Entity::BlockRef(reference), _)) = last {
            *reference = reference.clone().with_attribute(tag, value);
        }
    }

    /// Keeps an entity for the drawing, or for the block definition being
    /// read. The limit counts both; the report counts what reaches the
    /// drawing itself — a definition's entities are part of their block.
    fn push(&mut self, entity: Entity, properties: Pending) -> Result<(), DxfError> {
        if self.total >= self.limits.max_entities {
            return Err(DxfError::TooManyEntities {
                limit: self.limits.max_entities,
            });
        }
        self.total += 1;
        match &mut self.open_block {
            Some(OpenBlock::User(block)) => block.entities.push((entity, properties)),
            Some(OpenBlock::Skipped) => {}
            // A layout's entities count as the drawing's: they are (#191).
            Some(OpenBlock::Paper(_)) | None => {
                match &entity {
                    Entity::Line(_) => self.report.lines += 1,
                    Entity::Circle(_) => self.report.circles += 1,
                    Entity::Arc(_) => self.report.arcs += 1,
                    Entity::Ellipse(_) => self.report.ellipses += 1,
                    Entity::Spline(_) => self.report.splines += 1,
                    Entity::AttributeDefinition(_) => self.report.attribute_definitions += 1,
                    Entity::Polyline(_) => self.report.polylines += 1,
                    Entity::Text(_) | Entity::MText(_) => self.report.texts += 1,
                    // Counted once its block is found.
                    Entity::BlockRef(_) => {}
                    Entity::Dimension(_) => self.report.dimensions += 1,
                    Entity::Hatch(_) => self.report.hatches += 1,
                    Entity::Viewport(_) => self.report.viewports += 1,
                    Entity::Point(_) => self.report.points += 1,
                }
                match &mut self.open_block {
                    Some(OpenBlock::Paper(paper)) => paper.entities.push((entity, properties)),
                    // Flagged as paper (67) in the ENTITIES section: the
                    // active layout's, whose block is `*Paper_Space`.
                    _ if properties.paper => {
                        self.active_paper().entities.push((entity, properties))
                    }
                    _ => self.entities.push((entity, properties)),
                }
            }
        }
        Ok(())
    }

    /// The paper block of the active layout, for what the ENTITIES
    /// section flags as paper: `*Paper_Space`, made if the file has none.
    fn active_paper(&mut self) -> &mut PaperBlock {
        let at = self
            .paper_blocks
            .iter()
            .position(|block| is_active_paper_space(&block.name));
        let at = at.unwrap_or_else(|| {
            self.paper_blocks.push(PaperBlock {
                name: "*Paper_Space".to_owned(),
                ..PaperBlock::default()
            });
            self.paper_blocks.len() - 1
        });
        &mut self.paper_blocks[at]
    }

    fn finish_polyline(&mut self, polyline: PolylineBuilder) -> Result<(), DxfError> {
        if polyline.vertices.len() < 2 {
            self.report.count_unsupported(INVALID);
        } else {
            let entity = Entity::Polyline(PolylineEntity::with_bulges(
                polyline.vertices,
                polyline.bulges,
                polyline.closed,
            ));
            self.push(entity, polyline.properties)?;
        }
        Ok(())
    }

    /// An entity with its properties resolved against the tables: rows
    /// made for the layer, linetype, and text style it names and the file
    /// did not define. A reference to a block the file did not define is
    /// reported and not kept.
    fn resolve(&mut self, mut entity: Entity, pending: Pending) -> Option<(Entity, Properties)> {
        let mut dropped = pending.dropped;
        // A text's style: the file's, or Standard when it names none
        // or one no style can be called.
        let style = pending
            .style
            .as_deref()
            .and_then(|name| self.tables.text_style(name))
            .unwrap_or(TextStyleId::FIRST);
        match &mut entity {
            Entity::Text(text) => text.style = style,
            Entity::MText(text) => text.style = style,
            // A dimension's style likewise: the file's, or Standard.
            Entity::Dimension(dimension) => {
                dimension.style = pending
                    .dim_style
                    .as_deref()
                    .and_then(|name| self.tables.dim_style(name))
                    .unwrap_or(DimStyleId::FIRST);
            }
            Entity::BlockRef(reference) => {
                let block = pending
                    .block
                    .as_deref()
                    .and_then(|name| self.tables.block(name));
                match block {
                    Some(block) => reference.block = block,
                    None => {
                        self.report.count_unsupported(INSERT_UNDEFINED);
                        return None;
                    }
                }
            }
            _ => {}
        }
        let layer = self.tables.layer(&pending.layer).unwrap_or_else(|| {
            dropped = true;
            LayerId::FIRST
        });
        let linetype = self.tables.linetype(&pending.linetype).unwrap_or_else(|| {
            dropped = true;
            Linetype::ByLayer
        });
        if dropped {
            self.report.with_dropped_properties += 1;
        }
        Some((
            entity,
            Properties {
                layer,
                color: pending.color,
                linetype,
                lineweight: pending.lineweight,
            },
        ))
    }

    /// Resolves every name against the tables and builds the drawing.
    /// The layouts the file carried, other than the model's, in tab order
    /// (#184, #191): each LAYOUT object with the paper-space block its
    /// record names, paired by the record's handle — the block's entities
    /// as the layout's, its other records kept — and a layout for a paper
    /// block with something in it that no LAYOUT object names.
    fn layouts(&mut self) -> Vec<(Layout, PaperEntities)> {
        let mut kept: Vec<(i64, Layout, PaperEntities)> = Vec::new();
        let mut paper_blocks = std::mem::take(&mut self.paper_blocks);
        for groups in std::mem::take(&mut self.layouts) {
            // The AcDbLayout subclass holds the name (1), the tab order
            // (71), and the block record (330); the plot settings before
            // it hold a 1 of their own.
            let start = groups
                .iter()
                .position(|(code, value)| *code == 100 && value.trim() == "AcDbLayout")
                .unwrap_or(0);
            let after = |code: i32| {
                groups[start..]
                    .iter()
                    .find(|(c, _)| *c == code)
                    .map(|(_, value)| value.trim().to_owned())
            };
            let name = after(1).unwrap_or_default();
            if name.is_empty() || name.eq_ignore_ascii_case("Model") {
                continue;
            }
            if kept
                .iter()
                .any(|(_, layout, _)| same_name(&layout.name, &name))
                || check_name(&name).is_err()
            {
                continue;
            }
            let block_record_handle = after(330).unwrap_or_default().to_uppercase();
            let block_name = self
                .block_record_names
                .get(&block_record_handle)
                .cloned()
                .unwrap_or_default();
            let at = paper_blocks
                .iter()
                .position(|block| block.name.eq_ignore_ascii_case(&block_name));
            let block = at.map(|at| paper_blocks.remove(at)).unwrap_or_default();
            let order = after(71).and_then(|v| v.parse().ok()).unwrap_or(0);
            kept.push((
                order,
                Layout {
                    name,
                    paper_size: None,
                    kept: groups,
                    block_record_handle,
                    block_name,
                    kept_entities: block.kept,
                },
                block.entities,
            ));
        }
        kept.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.name.cmp(&b.1.name)));
        let mut layouts: Vec<(Layout, PaperEntities)> = kept
            .into_iter()
            .map(|(_, layout, entities)| (layout, entities))
            .collect();
        // A paper block no LAYOUT object names — an R12 file's, or a
        // file without an OBJECTS section — is a layout of its own when
        // something is on it, named like a writer would name it.
        for block in paper_blocks {
            if block.entities.is_empty() && block.kept.is_empty() {
                continue;
            }
            let name = (1..)
                .map(|n| format!("Layout{n}"))
                .find(|candidate| {
                    !layouts
                        .iter()
                        .any(|(layout, _)| same_name(&layout.name, candidate))
                })
                .expect("some number is free");
            layouts.push((
                Layout {
                    name,
                    paper_size: None,
                    kept: Vec::new(),
                    block_record_handle: String::new(),
                    block_name: block.name,
                    kept_entities: block.kept,
                },
                block.entities,
            ));
        }
        layouts
    }

    fn finish(mut self) -> Result<Import, DxfError> {
        // The file's layers first: their linetypes may make rows that
        // entities then use.
        let mut layers: Vec<Layer> = Vec::with_capacity(self.tables.layers.len());
        for index in 0..self.tables.layers.len() {
            let (layer, reference) = self.tables.layers[index].clone();
            let linetype = self
                .tables
                .linetype(&reference)
                .unwrap_or(Linetype::CONTINUOUS);
            layers.push(Layer { linetype, ..layer });
        }

        // Block definitions first: a reference inside one resolves like
        // one in the drawing, against every block the file defined — so
        // the names stay in the table while each is built.
        let mut blocks: Vec<Block> = Vec::with_capacity(self.tables.blocks.len());
        for index in 0..self.tables.blocks.len() {
            let entities = std::mem::take(&mut self.tables.blocks[index].entities);
            let records = entities
                .into_iter()
                .filter_map(|(entity, pending)| self.resolve(entity, pending))
                .map(|(entity, properties)| EntityRecord::new(entity, properties))
                .collect();
            let builder = &self.tables.blocks[index];
            blocks.push(Block::new(builder.name.clone(), builder.base, records));
        }

        let mut entities: Vec<(Entity, Properties, LayoutId)> =
            Vec::with_capacity(self.entities.len());
        for (entity, pending) in std::mem::take(&mut self.entities) {
            if let Some((entity, properties)) = self.resolve(entity, pending) {
                if matches!(entity, Entity::BlockRef(_)) {
                    self.report.block_references += 1;
                }
                entities.push((entity, properties, LayoutId::MODEL));
            }
        }
        // The layouts after the model, in tab order, take the IDs 1, 2, …;
        // each one's entities resolve like the model's (#191).
        let mut layouts = Vec::new();
        for (index, (layout, paper)) in self.layouts().into_iter().enumerate() {
            let space = LayoutId::from_raw(index as u32 + 1);
            let mut on_it = 0;
            for (entity, pending) in paper {
                if let Some((entity, properties)) = self.resolve(entity, pending) {
                    if matches!(entity, Entity::BlockRef(_)) {
                        self.report.block_references += 1;
                    }
                    entities.push((entity, properties, space));
                    on_it += 1;
                }
            }
            if on_it > 0 || !layout.kept_entities.is_empty() {
                self.report.layouts.push(layout.name.clone());
            }
            layouts.push(layout);
        }
        // Layers made for entities come after the file's own.
        layers.extend(
            self.tables
                .layers
                .drain(layers.len()..)
                .map(|(layer, _)| Layer {
                    linetype: Linetype::CONTINUOUS,
                    ..layer
                }),
        );
        let linetypes = std::mem::take(&mut self.tables.linetypes);
        let text_styles = std::mem::take(&mut self.tables.text_styles);
        let dim_styles = std::mem::take(&mut self.tables.dim_styles);

        let drawing = Drawing::from_spaces(
            layers,
            linetypes,
            text_styles,
            dim_styles,
            blocks,
            layouts,
            entities,
        )
        .map_err(|error| DxfError::Tables(error.to_string()))?;
        self.report.layers = drawing.layers().len() - 1;
        self.report.linetypes = drawing.linetypes().len() - 1;
        self.report.text_styles = drawing.text_styles().len() - 1;
        self.report.dim_styles = drawing.dim_styles().len() - 1;
        self.report.blocks = drawing.blocks().len() - 1;
        let deepest = drawing
            .blocks()
            .iter()
            .map(|(id, _)| drawing.blocks().nesting_depth(id))
            .max()
            .unwrap_or(0);
        if deepest > self.limits.max_nesting {
            self.report.count_unsupported(NESTING_TOO_DEEP);
        }
        self.report.created_layers = self.tables.created_layers;
        self.report.created_linetypes = self.tables.created_linetypes;
        self.report.created_text_styles = self.tables.created_text_styles;
        self.report.created_dim_styles = self.tables.created_dim_styles;

        // Settings the header names something for; what it names must be
        // usable, or the default stands.
        let current_layer = self
            .header
            .current_layer
            .as_deref()
            .and_then(|name| drawing.layers().find(name))
            .filter(|id| drawing.layers().get(*id).is_some_and(|layer| layer.on))
            .unwrap_or(LayerId::FIRST);
        let current_linetype = match &self.header.current_linetype {
            Some(LinetypeRef::ByBlock) => Linetype::ByBlock,
            Some(LinetypeRef::Named(name)) => drawing
                .linetypes()
                .find(name)
                .map_or(Linetype::ByLayer, Linetype::Named),
            Some(LinetypeRef::ByLayer) | None => Linetype::ByLayer,
        };
        let settings = DrawingSettings {
            current_layer,
            current_color: self.header.current_color.unwrap_or(Color::ByLayer),
            current_linetype,
            current_dim_style: self
                .header
                .current_dim_style
                .as_deref()
                .and_then(|name| drawing.dim_styles().find(name))
                .unwrap_or(DimStyleId::FIRST),
            current_lineweight: self
                .header
                .current_lineweight
                .unwrap_or(Lineweight::ByLayer),
            ltscale: self
                .header
                .ltscale
                .filter(|scale| scale.is_finite() && *scale > 0.0)
                .unwrap_or(DrawingSettings::DEFAULT_LTSCALE),
            unit: self.report.unit,
            point_style: self.point_style(),
        };
        let drawing = drawing
            .with_settings(settings)
            .map_err(|error| DxfError::Tables(error.to_string()))?;
        Ok(Import {
            drawing,
            report: self.report,
        })
    }
}

#[derive(Clone, Copy, Debug)]
enum RecordContext {
    Entities,
    Blocks,
    Tables,
    /// The OBJECTS section: layouts to keep (#184).
    Objects,
}

/// The groups of one record that matter to this reader.
#[derive(Debug, Default)]
struct RecordBody {
    /// Every group in order, kept only for a record whose groups mean
    /// different things where they stand: a HATCH (`crate::hatch`).
    raw: Option<Vec<(i32, String)>>,
    /// Every group 10 and 20 of the record, in order: one point for most
    /// entities, one per vertex for an LWPOLYLINE.
    xs: Vec<f64>,
    ys: Vec<f64>,
    /// Group 70: entity or table-row flags.
    flags: i64,
    /// Group 42 of each vertex, in step with `xs`: the bulge of the polyline
    /// segment that starts there. Zero where the vertex has none.
    bulges: Vec<f64>,
    x0: Option<f64>,
    y0: Option<f64>,
    x1: Option<f64>,
    y1: Option<f64>,
    radius: Option<f64>,
    start_degrees: Option<f64>,
    end_degrees: Option<f64>,
    /// Group 2: the name of a table row or a block.
    name: Option<String>,
    name_is_reserved: bool,
    /// Group 3: a linetype's description.
    description: Option<String>,
    /// Group 8: the layer.
    layer: Option<String>,
    /// Group 62, as written: a layer's sign says whether it is off.
    color: Option<i64>,
    /// Group 420: a true colour, as `0x00RRGGBB`.
    true_color: Option<i64>,
    /// Group 6: the linetype, by name.
    linetype: Option<String>,
    /// Group 370, as written.
    lineweight: Option<i64>,
    /// Groups 49 of a linetype: dash, gap, dot lengths.
    elements: Vec<f64>,
    /// Any group 74 that is not zero: the linetype has text or shapes in it.
    has_shapes: bool,
    /// Group 1: a text's words, or the last piece of a paragraph's.
    text: Option<String>,
    /// Groups 3 of a paragraph: its words, 250 characters at a time.
    chunks: Vec<String>,
    /// Group 7: a text's style, by name.
    style: Option<String>,
    /// Group 41: a paragraph's width, or a reference's X scale.
    width: Option<f64>,
    /// Group 42 of a record with one point: a reference's Y scale, or a
    /// dimension style's extension-line offset.
    scale_y: Option<f64>,
    /// Groups 13–16 (and 23–26) of a dimension: its further points.
    x13: Option<f64>,
    y13: Option<f64>,
    x14: Option<f64>,
    y14: Option<f64>,
    x15: Option<f64>,
    y15: Option<f64>,
    x16: Option<f64>,
    y16: Option<f64>,
    /// Group 53 of a dimension: its text's turn, in degrees.
    text_rotation: Option<f64>,
    /// Groups 44, 140, 147, 271 of a dimension style: extension overshoot,
    /// text height, text gap, decimals.
    dim_overshoot: Option<f64>,
    dim_text_height: Option<f64>,
    dim_gap: Option<f64>,
    dim_decimals: Option<i64>,
    /// Group 43: an LWPOLYLINE's constant width, which is dropped; a
    /// reference's Z scale, which a plane does without.
    constant_width: Option<f64>,
    /// Group 66: vertices follow a POLYLINE, attributes an INSERT.
    follow: bool,
    /// Groups 71, 72, 73: attachment, and horizontal and vertical justification.
    attachment: Option<i64>,
    halign: Option<i64>,
    valign: Option<i64>,
    has_dropped_properties: bool,
    /// Group 67: the record is in paper space (#191).
    paper: bool,
    /// A VIEWPORT's view (#192): the model point at its centre (12, 22),
    /// the height of its view (45), and whether it is on (68).
    view_center_x: Option<f64>,
    view_center_y: Option<f64>,
    view_height: Option<f64>,
    status: Option<i64>,
}

impl RecordBody {
    /// A VIEWPORT (#192): its frame — centre (10, 20), width (40), height
    /// (41) — the model point at its centre (12, 22), the height of its
    /// view (45), and whether it is on (68, positive). `None` for a frame
    /// or a view with no height.
    fn viewport(&self, at: usize) -> Result<Option<ViewportEntity>, DxfError> {
        let need = |value, group| required(value, at, "VIEWPORT", group);
        let center = Point2::new(need(self.x0, 10)?, need(self.y0, 20)?);
        let width = need(self.radius, 40)?;
        let height = need(self.width, 41)?;
        let view_center = Point2::new(
            self.view_center_x.unwrap_or(0.0),
            self.view_center_y.unwrap_or(0.0),
        );
        let view_height = self.view_height.unwrap_or(height);
        let mut viewport =
            ViewportEntity::new(center, width, height, view_center, height / view_height);
        viewport.on = self.status.is_none_or(|status| status > 0);
        Ok(
            (width > 0.0 && height > 0.0 && view_height > 0.0 && viewport.check().is_ok())
                .then_some(viewport),
        )
    }

    fn absorb(&mut self, group: Group<'_>) -> Result<(), DxfError> {
        match group.code {
            10 => {
                let x = real(group)?;
                self.x0.get_or_insert(x);
                self.xs.push(x);
                self.bulges.push(0.0);
            }
            20 => {
                let y = real(group)?;
                self.y0.get_or_insert(y);
                self.ys.push(y);
            }
            // A bulge belongs to the vertex whose coordinates precede it.
            42 => {
                let bulge = real(group)?;
                self.scale_y = Some(bulge);
                if let Some(last) = self.bulges.last_mut() {
                    *last = bulge;
                }
            }
            43 => self.constant_width = Some(real(group)?),
            44 => self.dim_overshoot = Some(real(group)?),
            140 => self.dim_text_height = Some(real(group)?),
            147 => self.dim_gap = Some(real(group)?),
            271 => self.dim_decimals = group.value.parse().ok(),
            66 => self.follow = group.value.trim() != "0",
            70 => self.flags = group.value.parse().unwrap_or(0),
            11 => self.x1 = Some(real(group)?),
            21 => self.y1 = Some(real(group)?),
            13 => self.x13 = Some(real(group)?),
            23 => self.y13 = Some(real(group)?),
            14 => self.x14 = Some(real(group)?),
            24 => self.y14 = Some(real(group)?),
            15 => self.x15 = Some(real(group)?),
            25 => self.y15 = Some(real(group)?),
            16 => self.x16 = Some(real(group)?),
            26 => self.y16 = Some(real(group)?),
            53 => self.text_rotation = Some(real(group)?),
            40 => self.radius = Some(real(group)?),
            50 => self.start_degrees = Some(real(group)?),
            51 => self.end_degrees = Some(real(group)?),
            2 => {
                self.name = Some(group.value.to_owned());
                self.name_is_reserved = is_structural_block(group.value);
            }
            3 => {
                self.description = Some(group.value.to_owned());
                self.chunks.push(group.raw.to_owned());
            }
            1 => self.text = Some(group.raw.to_owned()),
            7 => self.style = Some(group.value.to_owned()),
            41 => self.width = Some(real(group)?),
            71 => self.attachment = group.value.parse().ok(),
            72 => self.halign = group.value.parse().ok(),
            73 => self.valign = group.value.parse().ok(),
            8 => self.layer = Some(group.value.to_owned()),
            6 => self.linetype = Some(group.value.to_owned()),
            // A colour or lineweight that is not one is dropped.
            62 => match group.value.parse::<i64>() {
                Ok(code) if color_from_dxf(code).is_some() => self.color = Some(code),
                _ => self.has_dropped_properties = true,
            },
            370 => match group.value.parse::<i64>() {
                Ok(code) if lineweight_from_dxf(code).is_some() => self.lineweight = Some(code),
                _ => self.has_dropped_properties = true,
            },
            67 => self.paper = group.value.trim() == "1",
            12 => self.view_center_x = Some(real(group)?),
            22 => self.view_center_y = Some(real(group)?),
            45 => self.view_height = Some(real(group)?),
            68 => self.status = group.value.trim().parse().ok(),
            420 => self.true_color = group.value.parse::<i64>().ok(),
            49 => self.elements.push(real(group)?),
            74 => self.has_shapes |= group.value.parse::<i64>().unwrap_or(0) != 0,
            // A linetype scale of the entity's own: dropped. Exactly one is
            // no scale at all.
            #[allow(clippy::float_cmp)]
            48 => self.has_dropped_properties |= real(group)? != 1.0,
            // Thickness, and elevation: a 2D drawing keeps neither. Zero is
            // the default and loses nothing.
            30 | 31 | 39 => self.has_dropped_properties |= real(group)? != 0.0,
            _ => {}
        }
        Ok(())
    }

    /// The colour the record states, and whether a true colour was dropped
    /// for it: the drawing holds the index palette, so a true colour
    /// becomes the nearest index. The group 62 beside a true colour is not
    /// trusted: some writers put the nearest index there, others a default.
    fn colour(&self) -> (Option<Color>, bool) {
        match self.true_color {
            None => (self.color.and_then(color_from_dxf), false),
            Some(rgb) => {
                let [_, r, g, b] = (rgb as u32).to_be_bytes();
                (Some(Color::Index(nearest_index([r, g, b]))), true)
            }
        }
    }

    /// The entity's properties, as names where the tables are needed.
    fn pending(&self) -> Pending {
        let (color, true_colour_dropped) = self.colour();
        Pending {
            layer: self.layer.clone().unwrap_or_else(|| Layer::ZERO.to_owned()),
            color: color.unwrap_or(Color::ByLayer),
            linetype: self
                .linetype
                .as_deref()
                .map_or(LinetypeRef::ByLayer, LinetypeRef::from_dxf),
            lineweight: self
                .lineweight
                .and_then(lineweight_from_dxf)
                .unwrap_or(Lineweight::ByLayer),
            style: self.style.clone(),
            dim_style: None,
            block: None,
            dropped: self.has_dropped_properties || true_colour_dropped,
            paper: self.paper,
        }
    }

    /// A TEXT: its words at its height. Left, centre, and right
    /// justification along the baseline are what the drawing holds; the
    /// vertical justifications are placed by moving the baseline, so the
    /// words land where they were; aligned and fit text — stretched to a
    /// second point — is read as left text at its first point, and the
    /// stretch is dropped. Whether anything was dropped is the second value.
    fn text(&self, at: usize) -> Result<(TextEntity, bool), DxfError> {
        let need = |value, group| required(value, at, "TEXT", group);
        let first = Point2::new(need(self.x0, 10)?, need(self.y0, 20)?);
        let height = need(self.radius, 40)?;
        let (content, mut dropped) = plain_text(self.text.as_deref().unwrap_or(""));
        let rotation = self.start_degrees.unwrap_or(0.0).to_radians();
        let (halign, valign) = (self.halign.unwrap_or(0), self.valign.unwrap_or(0));
        let alignment = match (self.x1, self.y1) {
            (Some(x), Some(y)) => Point2::new(x, y),
            _ => first,
        };
        // Middle (4) is centred both ways. Aligned (3) and fit (5) stretch
        // the text between the two points: not held.
        let justification = match halign {
            1 | 4 => Justification::Center,
            2 => Justification::Right,
            3 | 5 => {
                dropped = true;
                Justification::Left
            }
            _ => Justification::Left,
        };
        // Fit and aligned text is read at its first point; the rest at the
        // alignment point when there is one.
        let placed = (halign != 0 || valign != 0) && !matches!(halign, 3 | 5);
        let anchor = if placed { alignment } else { first };
        let up = cad_core::Vec2::new(-rotation.sin(), rotation.cos());
        // The vertical justification says where the anchor is on the text:
        // the baseline moves so that it is.
        let drop = match (halign, valign) {
            (4, _) | (_, 2) => 0.5 * height,
            (_, 3) => height,
            // Bottom: the anchor is the descent below the baseline.
            (_, 1) => -TextEntity::new(first, height, rotation, "").frame().descent,
            _ => 0.0,
        };
        let insertion = anchor - up * drop;
        let text = TextEntity::new(insertion, height, rotation, content).justified(justification);
        Ok((text, dropped || !height.is_finite() || height <= 0.0))
    }

    /// An MTEXT: its words, in pieces, with the formatting taken out; its
    /// top left worked out from whichever of the nine attachment points the
    /// insertion is. The second value says whether formatting was dropped.
    fn mtext(&self, at: usize) -> Result<(MTextEntity, bool), DxfError> {
        let need = |value, group| required(value, at, "MTEXT", group);
        let insertion = Point2::new(need(self.x0, 10)?, need(self.y0, 20)?);
        let height = need(self.radius, 40)?;
        let mut raw = self.chunks.concat();
        raw.push_str(self.text.as_deref().unwrap_or(""));
        let (content, dropped) = plain_mtext(&raw);
        // The direction may be given as a vector (11, 21) instead of an angle.
        let rotation = match (self.start_degrees, self.x1, self.y1) {
            (Some(degrees), ..) => degrees.to_radians(),
            (None, Some(x), Some(y)) if x != 0.0 || y != 0.0 => y.atan2(x),
            _ => 0.0,
        };
        let width = self.width.unwrap_or(0.0).max(0.0);
        let mut text = MTextEntity::new(insertion, width, height, rotation, content);
        // Attachment points: 1 2 3 across the top, 4 5 6 the middle, 7 8 9
        // the bottom; the insertion is that point of the box.
        let attachment = self.attachment.unwrap_or(1).clamp(1, 9);
        let frame = text.frame();
        let cap_depth = (text.insertion - frame.start).dot(frame.up);
        let across = match (attachment - 1) % 3 {
            1 => 0.5 * frame.width,
            2 => frame.width,
            _ => 0.0,
        };
        let down = match (attachment - 1) / 3 {
            1 => 0.5 * cap_depth,
            2 => cap_depth,
            _ => 0.0,
        };
        text.insertion = insertion - frame.along * across + frame.up * down;
        Ok((text, dropped))
    }

    /// A POINT (#198): its place (10, 20).
    fn point(&self, at: usize) -> Result<PointEntity, DxfError> {
        let need = |value, group| required(value, at, "POINT", group);
        Ok(PointEntity::new(Point2::new(
            need(self.x0, 10)?,
            need(self.y0, 20)?,
        )))
    }

    fn line(&self, at: usize) -> Result<LineEntity, DxfError> {
        let need = |value, group| required(value, at, "LINE", group);
        Ok(LineEntity::new(
            Point2::new(need(self.x0, 10)?, need(self.y0, 20)?),
            Point2::new(need(self.x1, 11)?, need(self.y1, 21)?),
        ))
    }

    /// Every point of the record. Each X must have its Y.
    fn points(&self, at: usize, entity: &'static str) -> Result<Vec<Point2>, DxfError> {
        if self.ys.len() < self.xs.len() {
            return Err(DxfError::MissingCoordinate {
                line: at,
                entity,
                group: 20,
            });
        }
        if self.xs.len() < self.ys.len() {
            return Err(DxfError::MissingCoordinate {
                line: at,
                entity,
                group: 10,
            });
        }
        Ok(self
            .xs
            .iter()
            .zip(&self.ys)
            .map(|(x, y)| Point2::new(*x, *y))
            .collect())
    }

    /// `None` for a circle the drawing cannot hold: a radius that is not positive.
    fn circle(&self, at: usize) -> Result<Option<CircleEntity>, DxfError> {
        let need = |value, group| required(value, at, "CIRCLE", group);
        let center = Point2::new(need(self.x0, 10)?, need(self.y0, 20)?);
        let radius = need(self.radius, 40)?;
        Ok((radius > 0.0).then(|| CircleEntity::new(center, radius)))
    }

    /// An ELLIPSE (#181): centre (10, 20), the major axis's end relative
    /// to it (11, 21), the minor axis as a ratio (40), and the start and
    /// end parameters (41, 42), radians, 0 and 2π for a whole one. One
    /// with no axis, or a ratio outside (0, 1], is not an ellipse.
    fn ellipse(&self, at: usize) -> Result<Option<EllipseEntity>, DxfError> {
        let need = |value, group| required(value, at, "ELLIPSE", group);
        let center = Point2::new(need(self.x0, 10)?, need(self.y0, 20)?);
        let major = Vec2::new(need(self.x1, 11)?, need(self.y1, 21)?);
        let ratio = need(self.radius, 40)?;
        let start = self.width.unwrap_or(0.0);
        let end = self.scale_y.unwrap_or(std::f64::consts::TAU);
        let ellipse = EllipseEntity::arc(center, major, ratio, start, end);
        Ok(ellipse.ellipse().is_valid().then_some(ellipse))
    }

    /// DXF arcs run counter-clockwise from group 50 to group 51, in degrees.
    fn arc(&self, at: usize) -> Result<Option<ArcEntity>, DxfError> {
        let need = |value, group| required(value, at, "ARC", group);
        let center = Point2::new(need(self.x0, 10)?, need(self.y0, 20)?);
        let radius = need(self.radius, 40)?;
        let start = need(self.start_degrees, 50)?.to_radians();
        let end = need(self.end_degrees, 51)?.to_radians();
        Ok((radius > 0.0).then(|| ArcEntity::new(center, radius, start, end)))
    }
}

fn required(
    value: Option<f64>,
    line: usize,
    entity: &'static str,
    group: i32,
) -> Result<f64, DxfError> {
    value.ok_or(DxfError::MissingCoordinate {
        line,
        entity,
        group,
    })
}

/// A paper-space block: `*Paper_Space`, `*Paper_Space0`, … or the R12
/// spelling `$PAPER_SPACE`.
fn is_paper_space(name: &str) -> bool {
    let upper = name.to_uppercase();
    upper.starts_with("*PAPER_SPACE") || upper == "$PAPER_SPACE"
}

/// The paper block of the active layout, where an entity flagged as paper
/// in the ENTITIES section belongs: `*Paper_Space` itself, not the
/// numbered ones.
fn is_active_paper_space(name: &str) -> bool {
    let upper = name.to_uppercase();
    upper == "*PAPER_SPACE" || upper == "$PAPER_SPACE"
}

/// Block names that are part of the file's structure rather than user
/// content: `*Model_Space`, `*Paper_Space`, anonymous `*…` blocks, and the
/// `$MODEL_SPACE` / `$PAPER_SPACE` spelling used by R12 writers.
fn is_structural_block(name: &str) -> bool {
    name.starts_with('*')
        || name.eq_ignore_ascii_case("$MODEL_SPACE")
        || name.eq_ignore_ascii_case("$PAPER_SPACE")
}

fn real(group: Group<'_>) -> Result<f64, DxfError> {
    match group.value.parse::<f64>() {
        Ok(value) if value.is_finite() => Ok(value),
        _ => Err(DxfError::InvalidNumber {
            line: group.line + 1,
            group: group.code,
        }),
    }
}

/// A report-safe type name: bounded length, printable ASCII only.
fn kind_name(kind: &str) -> String {
    let clean: String = kind
        .chars()
        .filter(|c| c.is_ascii_graphic())
        .take(MAX_KIND_LENGTH)
        .collect();
    if clean.is_empty() {
        "(unnamed)".to_owned()
    } else {
        clean.to_ascii_uppercase()
    }
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;
    use crate::export;

    /// Builds a file from `code value` pairs, one group per pair.
    fn dxf(groups: &[(i32, &str)]) -> String {
        groups
            .iter()
            .map(|(code, value)| format!("{code:>3}\n{value}\n"))
            .collect()
    }

    fn entities(body: &[(i32, &str)]) -> String {
        let mut groups = vec![(0, "SECTION"), (2, "ENTITIES")];
        groups.extend_from_slice(body);
        groups.extend_from_slice(&[(0, "ENDSEC"), (0, "EOF")]);
        dxf(&groups)
    }

    const LINE_0_0_TO_10_5: [(i32, &str); 6] = [
        (0, "LINE"),
        (8, "0"),
        (10, "0.0"),
        (20, "0.0"),
        (11, "10.0"),
        (21, "5.0"),
    ];

    fn lines_of(import: &Import) -> Vec<LineEntity> {
        import
            .drawing
            .entities()
            .filter_map(|(_, entity)| entity.as_line().copied())
            .collect()
    }

    /// Every imported entity, in file order.
    fn entities_of(import: &Import) -> Vec<Entity> {
        let mut entities: Vec<_> = import
            .drawing
            .entities()
            .map(|(id, entity)| (id, entity.clone()))
            .collect();
        entities.sort_by_key(|(id, _)| *id);
        entities.into_iter().map(|(_, entity)| entity).collect()
    }

    fn line(x0: f64, y0: f64, x1: f64, y1: f64) -> LineEntity {
        LineEntity::new(Point2::new(x0, y0), Point2::new(x1, y1))
    }

    #[test]
    fn imports_a_line() {
        let import = import(entities(&LINE_0_0_TO_10_5).as_bytes()).unwrap();
        assert_eq!(lines_of(&import), vec![line(0.0, 0.0, 10.0, 5.0)]);
        assert_eq!(
            import.report,
            ImportReport {
                lines: 1,
                ..Default::default()
            }
        );
        assert!(!import.report.has_unsupported_content());
    }

    #[test]
    fn import_starts_a_fresh_drawing_with_no_history() {
        let import = import(entities(&LINE_0_0_TO_10_5).as_bytes()).unwrap();
        assert!(!import.drawing.can_undo());
        assert_eq!(import.drawing.len(), 1);
    }

    #[test]
    fn tolerates_crlf_padding_exponents_and_missing_sections() {
        let text = "  0\r\nSECTION\r\n  2\r\nENTITIES\r\n  0\r\nLINE\r\n 10\r\n  1.0E+1\r\n 20\r\n-2.5e0\r\n 11\r\n 3\r\n 21\r\n.5\r\n  0\r\nENDSEC\r\n  0\r\nEOF\r\n";
        let import = import(text.as_bytes()).unwrap();
        assert_eq!(lines_of(&import), vec![line(10.0, -2.5, 3.0, 0.5)]);
    }

    #[test]
    fn a_file_with_no_entities_section_is_an_empty_drawing() {
        let import = import(
            dxf(&[
                (0, "SECTION"),
                (2, "HEADER"),
                (9, "$ACADVER"),
                (1, "AC1027"),
                (0, "ENDSEC"),
                (0, "EOF"),
            ])
            .as_bytes(),
        )
        .unwrap();
        assert!(import.drawing.is_empty());
    }

    // ── the drawing unit (#113) ──

    fn with_header(header: &[(i32, &str)]) -> String {
        let mut groups = vec![(0, "SECTION"), (2, "HEADER")];
        groups.extend_from_slice(header);
        groups.extend_from_slice(&[
            (0, "ENDSEC"),
            (0, "SECTION"),
            (2, "ENTITIES"),
            (0, "LINE"),
            (10, "0"),
            (20, "0"),
            (11, "25"),
            (21, "0"),
            (0, "ENDSEC"),
            (0, "EOF"),
        ]);
        dxf(&groups)
    }

    #[test]
    fn the_drawing_unit_is_read_from_the_header() {
        use LengthUnit::{Centimetres, Feet, Inches, Metres, Millimetres};
        for (code, unit) in [
            ("1", Inches),
            ("2", Feet),
            ("4", Millimetres),
            ("5", Centimetres),
            ("     6", Metres),
        ] {
            let text = with_header(&[(9, "$ACADVER"), (1, "AC1015"), (9, "$INSUNITS"), (70, code)]);
            let import = import(text.as_bytes()).unwrap();
            assert_eq!(import.drawing.unit(), Some(unit), "code {code}");
            assert_eq!(import.report.unit, Some(unit));
            assert!(!import.report.has_unsupported_content());
            assert_eq!(
                import.report.summary(),
                vec![
                    "Imported 1 line.".to_owned(),
                    format!("Drawing unit: {}.", unit.name())
                ]
            );
            assert!(!import.drawing.can_undo(), "no history");
        }
    }

    #[test]
    fn a_file_without_a_unit_or_with_unit_zero_is_unitless() {
        for header in [
            &[][..],
            &[(9, "$ACADVER"), (1, "AC1009")],
            &[(9, "$INSUNITS"), (70, "0")],
            // Not a whole number, and a value group that is not the unit's.
            &[(9, "$INSUNITS"), (70, "mm")],
            &[(9, "$INSUNITS"), (40, "4")],
            // The value belongs to the variable just before it only.
            &[(9, "$INSUNITS"), (9, "$LUNITS"), (70, "4")],
        ] {
            let import = import(with_header(header).as_bytes()).unwrap();
            assert_eq!(import.drawing.unit(), None, "{header:?}");
            assert_eq!(import.report.unsupported_unit, None, "{header:?}");
            assert_eq!(import.report.summary(), vec!["Imported 1 line.".to_owned()]);
        }
    }

    #[test]
    fn a_unit_the_drawing_cannot_be_in_is_reported_and_the_drawing_is_unitless() {
        // 3 is miles, 7 kilometres, 13 microns.
        for code in ["3", "7", "13", "-1", "99999"] {
            let text = with_header(&[(9, "$INSUNITS"), (70, code)]);
            let import = import(text.as_bytes()).unwrap();
            assert_eq!(import.drawing.unit(), None);
            assert!(
                import.report.has_unsupported_content(),
                "exporting would lose it"
            );
            assert_eq!(
                import.report.summary().last().unwrap(),
                &format!(
                    "The file's drawing unit (code {code}) is not supported: the drawing is unitless."
                )
            );
        }
    }

    #[test]
    fn the_drawing_unit_survives_a_round_trip_and_a_unitless_file_is_plain_r12() {
        let mut drawing = Drawing::from_entities([Entity::Line(line(0.0, 0.0, 25.0, 0.0))]);
        let plain = crate::export(&drawing);
        assert!(
            !plain.contains("$INSUNITS"),
            "nothing newer than R12 without a unit"
        );

        for unit in LengthUnit::ALL {
            let mut tx = drawing.transaction();
            tx.set_unit(Some(unit));
            tx.commit();
            let text = crate::export(&drawing);
            assert!(text.contains(&format!("  9\n$INSUNITS\n 70\n{}\n", insunits_code(unit))));
            let back = import(text.as_bytes()).unwrap();
            assert_eq!(back.drawing.unit(), Some(unit));
            assert_eq!(
                crate::export(&back.drawing),
                text,
                "export → import → export"
            );
        }
    }

    #[test]
    fn unsupported_entities_are_counted_by_kind_and_supported_ones_still_import() {
        let mut body = vec![(0, "RAY"), (10, "0"), (20, "0"), (40, "5")];
        body.extend_from_slice(&LINE_0_0_TO_10_5);
        body.extend_from_slice(&[(0, "XLINE"), (70, "8"), (0, "RAY"), (40, "1")]);
        // A 3D polyline is unsupported; its vertices belong to it and are not counted separately.
        body.extend_from_slice(&[
            (0, "POLYLINE"),
            (70, "8"),
            (0, "VERTEX"),
            (10, "1"),
            (0, "VERTEX"),
            (0, "SEQEND"),
        ]);

        let import = import(entities(&body).as_bytes()).unwrap();
        assert_eq!(import.report.lines, 1);
        let counts: Vec<(&str, usize)> = import
            .report
            .unsupported
            .iter()
            .map(|(kind, count)| (kind.as_str(), *count))
            .collect();
        assert_eq!(counts, vec![("POLYLINE", 1), ("RAY", 2), ("XLINE", 1)]);
        assert!(import.report.has_unsupported_content());
    }

    #[test]
    fn a_hatch_with_no_boundary_is_invalid_geometry_and_never_panics() {
        let body = [(0, "HATCH"), (2, "SOLID")];
        let empty = import(entities(&body).as_bytes()).unwrap();
        assert_eq!(empty.report.unsupported.get(INVALID), Some(&1));
        // Claimed counts far beyond the groups present read what is there.
        let body = [
            (0, "HATCH"),
            (2, "SOLID"),
            (70, "1"),
            (91, "999999999"),
            (92, "2"),
            (72, "0"),
            (73, "1"),
            (93, "999999999"),
            (10, "0"),
            (20, "0"),
            (10, "10"),
            (20, "0"),
            (10, "10"),
            (20, "10"),
            (97, "0"),
            (75, "0"),
            (76, "1"),
        ];
        let claimed = import(entities(&body).as_bytes()).unwrap();
        assert_eq!(claimed.report.hatches, 1);
    }

    #[test]
    fn imports_circles_and_arcs_with_angles_converted_from_degrees() {
        use std::f64::consts::{FRAC_PI_2, PI};
        let body = [
            (0, "CIRCLE"),
            (8, "0"),
            (10, "1.5"),
            (20, "-2"),
            (40, "3"),
            (0, "ARC"),
            (8, "0"),
            (10, "0"),
            (20, "0"),
            (40, "10"),
            (50, "90"),
            (51, "180"),
        ];
        let import = import(entities(&body).as_bytes()).unwrap();

        assert_eq!(
            (
                import.report.lines,
                import.report.circles,
                import.report.arcs
            ),
            (0, 1, 1)
        );
        assert!(!import.report.has_unsupported_content());
        let imported = entities_of(&import);
        assert_eq!(
            imported[0],
            Entity::Circle(CircleEntity::new(Point2::new(1.5, -2.0), 3.0))
        );
        let Entity::Arc(arc) = &imported[1] else {
            panic!("expected an arc, got {:?}", imported[1])
        };
        assert_eq!((arc.center, arc.radius), (Point2::ORIGIN, 10.0));
        assert!((arc.start_angle - FRAC_PI_2).abs() < 1e-12 && (arc.end_angle - PI).abs() < 1e-12);
    }

    fn polylines_of(import: &Import) -> Vec<PolylineEntity> {
        entities_of(import)
            .into_iter()
            .filter_map(|entity| match entity {
                Entity::Polyline(polyline) => Some(polyline),
                _ => None,
            })
            .collect()
    }

    fn p(x: f64, y: f64) -> Point2 {
        Point2::new(x, y)
    }

    #[test]
    fn imports_r12_polylines_from_their_vertex_records() {
        let body = [
            (0, "POLYLINE"),
            (8, "0"),
            (66, "1"),
            (70, "1"),
            (0, "VERTEX"),
            (8, "0"),
            (10, "0"),
            (20, "0"),
            (0, "VERTEX"),
            (8, "0"),
            (10, "4"),
            (20, "0"),
            (0, "VERTEX"),
            (8, "0"),
            (10, "4"),
            (20, "3"),
            (0, "SEQEND"),
        ];
        let import =
            import(entities(&[body.as_slice(), &LINE_0_0_TO_10_5].concat()).as_bytes()).unwrap();
        assert_eq!((import.report.polylines, import.report.lines), (1, 1));
        assert!(!import.report.has_unsupported_content());
        assert_eq!(
            polylines_of(&import),
            vec![PolylineEntity::new(
                vec![p(0.0, 0.0), p(4.0, 0.0), p(4.0, 3.0)],
                true
            )]
        );
    }

    #[test]
    fn imports_lwpolylines_from_their_repeated_coordinates() {
        let open = [
            (0, "LWPOLYLINE"),
            (90, "3"),
            (70, "0"),
            (10, "0"),
            (20, "0"),
            (10, "1"),
            (20, "0"),
            (10, "1"),
            (20, "1"),
        ];
        let import = import(entities(&open).as_bytes()).unwrap();
        assert_eq!(
            polylines_of(&import),
            vec![PolylineEntity::new(
                vec![p(0.0, 0.0), p(1.0, 0.0), p(1.0, 1.0)],
                false
            )]
        );
    }

    /// #115: group 42 is read in both encodings, on the vertex it follows.
    #[test]
    fn polylines_with_arc_segments_keep_their_bulges() {
        let lightweight = [
            (0, "LWPOLYLINE"),
            (70, "1"),
            (10, "0"),
            (20, "0"),
            (10, "10"),
            (20, "0"),
            (42, "1.0"),
            (10, "10"),
            (20, "4"),
            (10, "0"),
            (20, "4"),
            (42, "-0.5"),
        ];
        let classic = [
            (0, "POLYLINE"),
            (70, "0"),
            (0, "VERTEX"),
            (10, "0"),
            (20, "0"),
            (42, "1.0"),
            (0, "VERTEX"),
            (10, "2"),
            (20, "0"),
            (0, "SEQEND"),
        ];
        let import =
            import(entities(&[lightweight.as_slice(), &classic].concat()).as_bytes()).unwrap();
        assert_eq!(import.report.polylines, 2);
        assert!(import.report.unsupported.is_empty());
        assert_eq!(
            polylines_of(&import),
            vec![
                PolylineEntity::with_bulges(
                    vec![p(0.0, 0.0), p(10.0, 0.0), p(10.0, 4.0), p(0.0, 4.0)],
                    vec![0.0, 1.0, 0.0, -0.5],
                    true
                ),
                PolylineEntity::with_bulges(vec![p(0.0, 0.0), p(2.0, 0.0)], vec![1.0], false),
            ]
        );
    }

    /// Regression: a circle written as a closed polyline of two arcs — a
    /// common encoding — imports as the whole circle and round-trips.
    #[test]
    fn a_circle_written_as_two_arcs_imports_whole() {
        let body = [
            (0, "LWPOLYLINE"),
            (70, "1"),
            (10, "0"),
            (20, "0"),
            (42, "1"),
            (10, "100"),
            (20, "0"),
            (42, "1"),
        ];
        let first = import(entities(&body).as_bytes()).unwrap();
        let circle = polylines_of(&first)[0].clone();
        assert_eq!(circle.bulges().collect::<Vec<_>>(), vec![1.0, 1.0]);
        let shape = Entity::Polyline(circle.clone());
        assert_eq!(shape.primitives(cad_document::Tables::none()).len(), 2);
        assert!(
            shape.distance_to(p(50.0, 50.0), cad_document::Tables::none()) < 1e-12
                && shape.distance_to(p(50.0, -50.0), cad_document::Tables::none()) < 1e-12
        );

        let text = export(&first.drawing);
        // The ENTITIES section only: the layouts after it have 42s of their own.
        let entities = &text[text.find("ENTITIES").unwrap()..];
        let entities = &entities[..entities.find("ENDSEC").unwrap()];
        assert_eq!(entities.matches("\n 42\n").count(), 2);
        assert_eq!(
            polylines_of(&import(text.as_bytes()).unwrap()),
            vec![circle]
        );
    }

    #[test]
    fn an_absurd_bulge_never_becomes_unbounded_geometry() {
        let with_bulge = |bulge: &'static str| {
            let body = [
                (0, "LWPOLYLINE"),
                (10, "0"),
                (20, "0"),
                (42, bulge),
                (10, "1"),
                (20, "0"),
            ];
            import(entities(&body).as_bytes())
        };
        // Finite but far too large to square: imported, as a straight segment.
        let import = with_bulge("1e200").unwrap();
        let bounds =
            Entity::Polyline(polylines_of(&import)[0].clone()).bounds(cad_document::Tables::none());
        assert!(bounds.min().is_finite() && bounds.max().is_finite());
        // Not a finite number, or not a number: a malformed file, as for a coordinate.
        for bulge in ["1e400", "round"] {
            assert!(
                matches!(
                    with_bulge(bulge),
                    Err(DxfError::InvalidNumber { group: 42, .. })
                ),
                "bulge {bulge}"
            );
        }
    }

    /// #115: export → import → export is byte-identical for bulged polylines.
    #[test]
    fn a_bulged_polyline_survives_a_round_trip_exactly() {
        let slot = PolylineEntity::with_bulges(
            vec![p(0.0, 0.0), p(10.0, 0.0), p(10.0, 4.0), p(0.0, 4.0)],
            vec![0.0, 1.0, 0.0, 2f64.sqrt() - 1.0],
            true,
        );
        let open = PolylineEntity::with_bulges(
            vec![p(0.1, 0.2), p(7.3, -9.1), p(20.0, 0.0)],
            vec![-0.333_333_333_333_333_3, 0.0],
            false,
        );
        let drawing = Drawing::from_entities([
            Entity::Polyline(slot.clone()),
            Entity::Polyline(open.clone()),
        ]);
        let text = export(&drawing);
        let reread = import(text.as_bytes()).unwrap();
        assert_eq!(polylines_of(&reread), vec![slot, open]);
        assert_eq!(export(&reread.drawing), text);
    }

    #[test]
    fn a_zero_bulge_is_a_straight_segment() {
        let body = [
            (0, "LWPOLYLINE"),
            (70, "1"),
            (10, "0"),
            (20, "0"),
            (42, "0.0"),
            (10, "1"),
            (20, "0"),
            (10, "1"),
            (20, "1"),
        ];
        let import = import(entities(&body).as_bytes()).unwrap();
        assert_eq!(import.report.polylines, 1);
        assert!(polylines_of(&import)[0].closed);
    }

    #[test]
    fn degenerate_and_malformed_polylines() {
        let one_vertex = [(0, "LWPOLYLINE"), (10, "0"), (20, "0")];
        let import_one = import(entities(&one_vertex).as_bytes()).unwrap();
        assert_eq!(import_one.report.unsupported.get(INVALID), Some(&1));

        let missing_y = entities(&[(0, "LWPOLYLINE"), (10, "0"), (20, "0"), (10, "1")]);
        assert_eq!(
            import(missing_y.as_bytes()).unwrap_err(),
            DxfError::MissingCoordinate {
                line: 5,
                entity: "LWPOLYLINE",
                group: 20
            }
        );
    }

    #[test]
    fn a_polyline_survives_a_round_trip_exactly() {
        let original = PolylineEntity::new(
            vec![p(0.1, 0.2), p(707.1067811865476, -3.5), p(1e6, 2.5e-3)],
            true,
        );
        let text = export(&Drawing::from_entities([Entity::Polyline(
            original.clone(),
        )]));
        let reread = import(text.as_bytes()).unwrap();
        assert_eq!(polylines_of(&reread), vec![original]);
        assert_eq!(export(&reread.drawing), text);
    }

    #[test]
    fn curves_with_a_radius_that_is_not_positive_are_reported_not_imported() {
        let body = [
            (0, "CIRCLE"),
            (10, "0"),
            (20, "0"),
            (40, "0"),
            (0, "ARC"),
            (10, "0"),
            (20, "0"),
            (40, "-1"),
            (50, "0"),
            (51, "90"),
        ];
        let import = import(entities(&body).as_bytes()).unwrap();
        assert!(import.drawing.is_empty());
        assert_eq!(import.report.unsupported.get(INVALID), Some(&2));
    }

    #[test]
    fn curves_missing_required_groups_are_malformed() {
        let no_radius = entities(&[(0, "CIRCLE"), (10, "0"), (20, "0")]);
        assert_eq!(
            import(no_radius.as_bytes()).unwrap_err(),
            DxfError::MissingCoordinate {
                line: 5,
                entity: "CIRCLE",
                group: 40
            }
        );
        let no_end = entities(&[(0, "ARC"), (10, "0"), (20, "0"), (40, "1"), (50, "0")]);
        assert_eq!(
            import(no_end.as_bytes()).unwrap_err(),
            DxfError::MissingCoordinate {
                line: 5,
                entity: "ARC",
                group: 51
            }
        );
    }

    /// Angles pass through degrees, so they come back within floating-point
    /// rounding rather than bit for bit; everything else is exact.
    #[test]
    fn circles_and_arcs_survive_a_round_trip() {
        let original = vec![
            Entity::Circle(CircleEntity::new(Point2::new(12.5, -7.25), 0.1)),
            Entity::Arc(ArcEntity::new(
                Point2::new(-3.0, 4.0),
                707.1067811865476,
                0.3,
                5.9,
            )),
            Entity::Arc(ArcEntity::new(Point2::ORIGIN, 1.0, 6.0, 0.25)),
        ];
        let text = export(&Drawing::from_entities(original.clone()));
        let reread = entities_of(&import(text.as_bytes()).unwrap());

        assert_eq!(reread[0], original[0]);
        for (read, written) in reread.iter().zip(&original).skip(1) {
            let (Entity::Arc(read), Entity::Arc(written)) = (read, written) else {
                panic!("expected arcs")
            };
            assert_eq!((read.center, read.radius), (written.center, written.radius));
            assert!((read.start_angle - written.start_angle).abs() < 1e-12);
            assert!((read.end_angle - written.end_angle).abs() < 1e-12);
        }
    }

    #[test]
    fn properties_the_drawing_cannot_hold_are_reported() {
        let plain = LINE_0_0_TO_10_5;
        // A layer, a colour, a linetype, and a lineweight are held (#139).
        let styled = [
            (0, "LINE"),
            (8, "WALLS"),
            (62, "1"),
            (6, "DASHED"),
            (370, "50"),
            (10, "0"),
            (20, "0"),
            (11, "1"),
            (21, "1"),
        ];
        let elevated = [
            (0, "LINE"),
            (10, "0"),
            (20, "0"),
            (30, "7.5"),
            (11, "1"),
            (21, "1"),
            (31, "0.0"),
        ];
        let flat = [
            (0, "LINE"),
            (10, "0"),
            (20, "0"),
            (30, "0.0"),
            (11, "1"),
            (21, "1"),
            (31, "0"),
        ];
        // A linetype scale of its own, a thickness, a colour that is no
        // colour, a lineweight that is not standard.
        let scaled = [
            (0, "LINE"),
            (48, "2.0"),
            (10, "0"),
            (20, "0"),
            (11, "1"),
            (21, "1"),
        ];
        let thick = [
            (0, "LINE"),
            (39, "1"),
            (10, "0"),
            (20, "0"),
            (11, "1"),
            (21, "1"),
        ];
        let odd_colour = [
            (0, "LINE"),
            (62, "300"),
            (10, "0"),
            (20, "0"),
            (11, "1"),
            (21, "1"),
        ];
        let odd_weight = [
            (0, "LINE"),
            (370, "51"),
            (10, "0"),
            (20, "0"),
            (11, "1"),
            (21, "1"),
        ];
        let body = [
            plain.as_slice(),
            &styled,
            &elevated,
            &flat,
            &scaled,
            &thick,
            &odd_colour,
            &odd_weight,
        ]
        .concat();

        let import = import(entities(&body).as_bytes()).unwrap();
        assert_eq!(import.report.lines, 8);
        assert_eq!(
            import.report.with_dropped_properties, 5,
            "elevated, scaled, thick, two odd"
        );
        let properties: Vec<Properties> = {
            let mut records: Vec<_> = import
                .drawing
                .records()
                .map(|(id, r)| (id, r.properties))
                .collect();
            records.sort_by_key(|(id, _)| *id);
            records.into_iter().map(|(_, p)| p).collect()
        };
        let walls = import.drawing.layers().find("walls").unwrap();
        let dashed = import.drawing.linetypes().find("dashed").unwrap();
        assert_eq!(
            properties[1],
            Properties {
                layer: walls,
                color: Color::Index(1),
                linetype: Linetype::Named(dashed),
                lineweight: Lineweight::Hundredths(50),
            }
        );
        // What could not be read keeps the default, and is counted.
        assert_eq!(properties[6], Properties::default());
        assert_eq!(properties[7], Properties::default());
        assert_eq!(
            import.report.summary(),
            vec![
                "Imported 8 lines.",
                "With 1 layer and 1 linetype.",
                "Named but not defined in the file, so created: 1 layer and 1 linetype.",
                "Properties the drawing cannot hold were dropped from 5 entities.",
            ]
        );
        assert!(import.report.has_unsupported_content());
    }

    // ── layers and linetypes (#139) ──

    /// A file with LAYER and LTYPE tables, as groups.
    fn with_tables(tables: &[(i32, &str)], body: &[(i32, &str)]) -> String {
        let mut groups = vec![(0, "SECTION"), (2, "TABLES")];
        groups.extend_from_slice(tables);
        groups.extend_from_slice(&[(0, "ENDSEC"), (0, "SECTION"), (2, "ENTITIES")]);
        groups.extend_from_slice(body);
        groups.extend_from_slice(&[(0, "ENDSEC"), (0, "EOF")]);
        dxf(&groups)
    }

    const TABLES: [(i32, &str); 46] = [
        (0, "TABLE"),
        (2, "LTYPE"),
        (70, "4"),
        (0, "LTYPE"),
        (2, "ByLayer"),
        (70, "0"),
        (0, "LTYPE"),
        (2, "CONTINUOUS"),
        (70, "0"),
        (3, "Solid line"),
        (72, "65"),
        (73, "0"),
        (40, "0.0"),
        (0, "LTYPE"),
        (2, "DASHED"),
        (70, "0"),
        (3, "Dashed __ __"),
        (72, "65"),
        (73, "2"),
        (40, "0.75"),
        (49, "0.5"),
        (49, "-0.25"),
        (0, "LTYPE"),
        (2, "DOT"),
        (70, "0"),
        (3, "Dot . . ."),
        (72, "65"),
        (73, "2"),
        (40, "0.25"),
        (49, "0.0"),
        (49, "-0.25"),
        (0, "ENDTAB"),
        (0, "TABLE"),
        (2, "LAYER"),
        (70, "3"),
        (0, "LAYER"),
        (2, "0"),
        (70, "0"),
        (62, "7"),
        (6, "CONTINUOUS"),
        (0, "LAYER"),
        (2, "Walls"),
        (70, "4"),
        (62, "-1"),
        (6, "DASHED"),
        (0, "ENDTAB"),
    ];

    #[test]
    fn the_layer_and_linetype_tables_are_read() {
        let import = import(with_tables(&TABLES, &LINE_0_0_TO_10_5).as_bytes()).unwrap();
        let drawing = &import.drawing;
        assert_eq!(drawing.linetypes().len(), 3, "ByLayer is not a linetype");
        let dashed = drawing.linetypes().find("Dashed").unwrap();
        assert_eq!(dashed, LinetypeId::from_raw(1));
        let def = drawing.linetypes().get(dashed).unwrap();
        assert_eq!(def.name, "DASHED");
        assert_eq!(def.description, "Dashed __ __");
        assert_eq!(def.pattern, vec![0.5, 0.25]);
        let dot = drawing.linetypes().get(LinetypeId::from_raw(2)).unwrap();
        assert_eq!(
            (dot.name.as_str(), dot.pattern.as_slice()),
            ("DOT", &[0.0, 0.25][..])
        );

        assert_eq!(drawing.layers().len(), 2);
        let walls = drawing.layers().get(LayerId::from_raw(1)).unwrap();
        assert_eq!(walls.name, "Walls");
        assert_eq!(
            walls.color,
            Color::Index(1),
            "a negative colour is the colour, off"
        );
        assert!(!walls.on);
        assert!(walls.locked, "flag 4");
        assert_eq!(walls.linetype, Linetype::Named(dashed));
        assert_eq!(walls.lineweight, Lineweight::Default);
        let zero = drawing.layers().get(LayerId::FIRST).unwrap();
        assert_eq!(zero.color, Color::Index(7));
        assert!(zero.on);

        assert_eq!((import.report.layers, import.report.linetypes), (1, 2));
        assert_eq!(
            (
                import.report.created_layers,
                import.report.created_linetypes
            ),
            (0, 0)
        );
        assert!(!import.report.has_unsupported_content());
        assert_eq!(
            import.report.summary(),
            vec!["Imported 1 line.", "With 1 layer and 2 linetypes."]
        );
    }

    #[test]
    fn a_frozen_layer_is_off_and_a_lineweight_on_a_layer_is_kept() {
        let tables = [
            (0, "TABLE"),
            (2, "LAYER"),
            (70, "1"),
            (0, "LAYER"),
            (2, "Frozen"),
            (70, "1"),
            (62, "3"),
            (370, "100"),
            (0, "LAYER"),
            (2, "frozen"),
            (70, "0"),
            (62, "4"),
            (0, "LAYER"),
            (2, "Odd"),
            (62, "0"),
            (370, "51"),
            (6, "BYLAYER"),
            (0, "LAYER"),
            (70, "0"),
            (0, "LAYER"),
            (2, "bad,name"),
            (0, "ENDTAB"),
        ];
        let import = import(with_tables(&tables, &[]).as_bytes()).unwrap();
        let layers = import.drawing.layers();
        assert_eq!(layers.len(), 3, "a repeated name counts once");
        let frozen = layers.get(layers.find("FROZEN").unwrap()).unwrap();
        assert!(!frozen.on && !frozen.locked);
        assert_eq!(frozen.color, Color::Index(3), "the first row of a name");
        assert_eq!(frozen.lineweight, Lineweight::Hundredths(100));
        let odd = layers.get(layers.find("odd").unwrap()).unwrap();
        assert_eq!(
            odd.color,
            Layer::DEFAULT_COLOR,
            "ByBlock is no colour for a layer"
        );
        assert_eq!(odd.lineweight, Lineweight::Default, "51 is not a width");
        assert_eq!(
            odd.linetype,
            Linetype::CONTINUOUS,
            "ByLayer is no linetype for a layer"
        );
        assert_eq!(import.report.unsupported.get("(layer name)"), Some(&2));
    }

    #[test]
    fn names_the_file_uses_but_never_defines_are_created_and_reported() {
        let body = [
            (0, "LINE"),
            (8, "Ghost"),
            (6, "PHANTOM"),
            (10, "0"),
            (20, "0"),
            (11, "1"),
            (21, "1"),
            (0, "CIRCLE"),
            (8, "ghost"),
            (6, "Phantom"),
            (10, "0"),
            (20, "0"),
            (40, "1"),
            (0, "ARC"),
            (8, "Ghost"),
            (6, "Continuous"),
            (10, "0"),
            (20, "0"),
            (40, "1"),
            (50, "0"),
            (51, "90"),
        ];
        let tables = [
            (0, "TABLE"),
            (2, "LAYER"),
            (70, "1"),
            (0, "LAYER"),
            (2, "Dashed walls"),
            (62, "7"),
            (6, "DASHED"),
            (0, "ENDTAB"),
        ];
        let import = import(with_tables(&tables, &body).as_bytes()).unwrap();
        let drawing = &import.drawing;
        // The file's layer, then the one made for the entities.
        assert_eq!(
            drawing.layers().find("Dashed walls"),
            Some(LayerId::from_raw(1))
        );
        assert_eq!(drawing.layers().find("ghost"), Some(LayerId::from_raw(2)));
        // The linetype the layer names, then the one the entities name.
        let dashed = drawing.linetypes().find("dashed").unwrap();
        let phantom = drawing.linetypes().find("phantom").unwrap();
        assert_eq!(
            (dashed, phantom),
            (LinetypeId::from_raw(1), LinetypeId::from_raw(2))
        );
        assert!(
            drawing.linetypes().get(phantom).unwrap().is_continuous(),
            "nothing better is known"
        );
        let ghost = drawing.layers().get(LayerId::from_raw(2)).unwrap();
        assert_eq!(
            (ghost.name.as_str(), ghost.linetype),
            ("Ghost", Linetype::CONTINUOUS)
        );
        let mut properties: Vec<_> = drawing
            .records()
            .map(|(id, r)| (id, r.properties))
            .collect();
        properties.sort_by_key(|(id, _)| *id);
        assert!(
            properties
                .iter()
                .all(|(_, p)| p.layer == LayerId::from_raw(2))
        );
        assert_eq!(properties[0].1.linetype, Linetype::Named(phantom));
        assert_eq!(properties[1].1.linetype, Linetype::Named(phantom));
        assert_eq!(properties[2].1.linetype, Linetype::CONTINUOUS);
        assert_eq!(
            import.report.summary(),
            vec![
                "Imported 1 line, 1 circle, 1 arc.",
                "With 2 layers and 2 linetypes.",
                "Named but not defined in the file, so created: 1 layer and 2 linetypes.",
            ]
        );
        assert!(!import.report.has_unsupported_content(), "nothing was lost");
        assert!(!drawing.can_undo());
    }

    #[test]
    fn a_complex_or_overlong_pattern_is_reported_and_the_dashes_kept() {
        let tables = [
            (0, "TABLE"),
            (2, "LTYPE"),
            (70, "2"),
            (0, "LTYPE"),
            (2, "GAS"),
            (3, "Gas line ----GAS----"),
            (72, "65"),
            (73, "3"),
            (40, "0.95"),
            (49, "0.5"),
            (74, "0"),
            (49, "-0.2"),
            (74, "2"),
            (49, "-0.25"),
            (74, "0"),
            (0, "LTYPE"),
            (2, "LONG"),
            (73, "10"),
            (40, "10"),
            (49, "1"),
            (49, "-1"),
            (49, "1"),
            (49, "-1"),
            (49, "1"),
            (49, "-1"),
            (49, "1"),
            (49, "-1"),
            (49, "1"),
            (49, "-1"),
            (0, "LTYPE"),
            (2, "CONTINUOUS"),
            (3, "Said to be dashed"),
            (49, "1"),
            (49, "-1"),
            (0, "ENDTAB"),
        ];
        let import = import(with_tables(&tables, &[]).as_bytes()).unwrap();
        let linetypes = import.drawing.linetypes();
        assert_eq!(linetypes.len(), 3);
        let gas = linetypes.get(linetypes.find("gas").unwrap()).unwrap();
        assert_eq!(
            gas.pattern,
            vec![0.5, 0.45],
            "the text's gap merges with the gap"
        );
        let long = linetypes.get(linetypes.find("long").unwrap()).unwrap();
        assert!(long.is_continuous());
        let continuous = linetypes.get(LinetypeId::FIRST).unwrap();
        assert!(continuous.is_continuous());
        assert_eq!(continuous.description, "Said to be dashed");
        assert_eq!(import.report.unsupported.get("(linetype shapes)"), Some(&1));
        assert_eq!(
            import.report.unsupported.get("(linetype pattern)"),
            Some(&1)
        );
        assert!(import.report.has_unsupported_content());
    }

    #[test]
    fn the_header_sets_the_current_layer_colour_linetype_lineweight_and_scale() {
        let header = [
            (9, "$CLAYER"),
            (8, "Walls"),
            (9, "$CECOLOR"),
            (62, "3"),
            (9, "$CELTYPE"),
            (6, "DASHED"),
            (9, "$CELWEIGHT"),
            (370, "35"),
            (9, "$LTSCALE"),
            (40, "25"),
        ];
        let mut groups = vec![(0, "SECTION"), (2, "HEADER")];
        groups.extend_from_slice(&header);
        groups.push((0, "ENDSEC"));
        let mut tables = TABLES.to_vec();
        // Walls is off in TABLES: switch it on so it can be current.
        let off = tables
            .iter()
            .position(|group| *group == (62, "-1"))
            .unwrap();
        tables[off] = (62, "1");
        let text = dxf(&groups) + &with_tables(&tables, &LINE_0_0_TO_10_5);
        let import = import(text.as_bytes()).unwrap();
        let drawing = &import.drawing;
        assert_eq!(drawing.current_layer(), LayerId::from_raw(1));
        assert_eq!(drawing.current_color(), Color::Index(3));
        assert_eq!(
            drawing.current_linetype(),
            Linetype::Named(LinetypeId::from_raw(1))
        );
        assert_eq!(drawing.current_lineweight(), Lineweight::Hundredths(35));
        assert_eq!(drawing.ltscale(), 25.0);
        assert!(!drawing.can_undo());

        // What the header names but the file does not have, or cannot be
        // current, leaves the default.
        let header = [
            (9, "$CLAYER"),
            (8, "Walls"),
            (9, "$CELTYPE"),
            (6, "ZIGZAG"),
            (9, "$LTSCALE"),
            (40, "-2"),
            (9, "$CECOLOR"),
            (62, "999"),
        ];
        let mut groups = vec![(0, "SECTION"), (2, "HEADER")];
        groups.extend_from_slice(&header);
        groups.push((0, "ENDSEC"));
        let text = dxf(&groups) + &with_tables(&TABLES, &LINE_0_0_TO_10_5);
        let import = super::import(text.as_bytes()).unwrap();
        let drawing = &import.drawing;
        assert_eq!(drawing.current_layer(), LayerId::FIRST, "Walls is off");
        assert_eq!(drawing.current_linetype(), Linetype::ByLayer);
        assert_eq!(drawing.ltscale(), 1.0);
        assert_eq!(drawing.current_color(), Color::ByLayer);
        assert_eq!(
            drawing.linetypes().len(),
            3,
            "the header does not make rows"
        );
    }

    /// #153: a true colour (group 420) becomes the index beside it, or the
    /// nearest one, and is reported as a dropped property.
    #[test]
    fn a_true_colour_becomes_the_nearest_index_and_is_reported() {
        let text = dxf(&[
            (0, "SECTION"),
            (2, "TABLES"),
            (0, "TABLE"),
            (2, "LAYER"),
            (0, "LAYER"),
            (2, "TEAL"),
            (62, "3"),
            (420, "32896"), // 0x008080
            (0, "ENDTAB"),
            (0, "ENDSEC"),
            (0, "SECTION"),
            (2, "ENTITIES"),
            (0, "LINE"),
            (420, "16744448"), // 0xFF8000: orange, nearest 30
            (10, "0"),
            (20, "0"),
            (11, "1"),
            (21, "0"),
            (0, "LINE"),
            (62, "7"),
            (420, "660730"), // 0x0A14FA, nearly blue: the index beside it is a default
            (10, "0"),
            (20, "1"),
            (11, "1"),
            (21, "1"),
            (0, "ENDSEC"),
            (0, "EOF"),
        ]);
        let import = import(text.as_bytes()).unwrap();
        let colours: Vec<Color> = {
            let mut records: Vec<_> = import.drawing.records().collect();
            records.sort_by_key(|(id, _)| *id);
            records.iter().map(|(_, r)| r.properties.color).collect()
        };
        assert_eq!(colours, [Color::Index(30), Color::Index(5)]);
        assert_eq!(import.report.with_dropped_properties, 2);
        let teal = import.drawing.layers().find("TEAL").unwrap();
        assert_eq!(
            import.drawing.layers().get(teal).unwrap().color,
            Color::Index(nearest_index([0, 128, 128])),
            "the nearest index, not the 3 beside it"
        );
        assert_eq!(import.report.unsupported.get(LAYER_TRUE_COLOUR), Some(&1));
        assert!(import.report.has_unsupported_content());
    }

    #[test]
    fn user_blocks_are_imported_and_structural_ones_are_not() {
        let text = dxf(&[
            (0, "SECTION"),
            (2, "BLOCKS"),
            (0, "BLOCK"),
            (2, "*Model_Space"),
            (0, "ENDBLK"),
            (0, "BLOCK"),
            (2, "DOOR"),
            (0, "LINE"),
            (10, "0"),
            (20, "0"),
            (11, "1"),
            (21, "1"),
            (0, "ENDBLK"),
            (0, "ENDSEC"),
            (0, "EOF"),
        ]);
        let import = import(text.as_bytes()).unwrap();
        assert!(
            import.drawing.is_empty(),
            "geometry inside block definitions is not model space"
        );
        assert_eq!(
            import.drawing.blocks().len(),
            2,
            "the door, not model space"
        );
        let door = import.drawing.blocks().find("DOOR").unwrap();
        assert_eq!(import.drawing.blocks().get(door).unwrap().entities.len(), 1);
        assert_eq!(import.report.blocks, 1);
        assert!(import.report.unsupported.is_empty());
    }

    #[test]
    fn structural_block_names_are_recognised_in_every_spelling() {
        for name in [
            "*Model_Space",
            "*Paper_Space",
            "*U12",
            "$MODEL_SPACE",
            "$Paper_Space",
        ] {
            assert!(is_structural_block(name), "{name}");
        }
        for name in ["DOOR", "MODEL_SPACE", "$TITLE"] {
            assert!(!is_structural_block(name), "{name}");
        }
    }

    #[test]
    fn binary_dxf_is_rejected_with_a_clear_message() {
        let error = import(b"AutoCAD Binary DXF\r\n\x1a\0\0\0").unwrap_err();
        assert_eq!(error, DxfError::BinaryDxf);
        assert!(error.to_string().contains("save as ASCII"));
    }

    #[test]
    fn malformed_files_return_structured_errors() {
        let cases: Vec<(String, DxfError)> = vec![
            (String::new(), DxfError::UnexpectedEnd { line: 1 }),
            (
                "hello\nworld\n".into(),
                DxfError::InvalidGroupCode { line: 1 },
            ),
            ("  0\n".into(), DxfError::UnexpectedEnd { line: 1 }),
            // No EOF marker: the file was truncated.
            (
                dxf(&[(0, "SECTION"), (2, "ENTITIES"), (0, "ENDSEC")]),
                DxfError::UnexpectedEnd { line: 7 },
            ),
            (
                dxf(&[(0, "SECTION"), (2, "ENTITIES"), (0, "LINE"), (10, "1")]),
                DxfError::UnexpectedEnd { line: 9 },
            ),
            (
                entities(&[(0, "LINE"), (10, "abc"), (20, "0"), (11, "1"), (21, "1")]),
                DxfError::InvalidNumber { line: 8, group: 10 },
            ),
            (
                entities(&[(0, "LINE"), (10, "inf"), (20, "0"), (11, "1"), (21, "1")]),
                DxfError::InvalidNumber { line: 8, group: 10 },
            ),
            (
                entities(&[(0, "LINE"), (10, "NaN"), (20, "0"), (11, "1"), (21, "1")]),
                DxfError::InvalidNumber { line: 8, group: 10 },
            ),
            (
                entities(&[(0, "LINE"), (10, "0"), (20, "0"), (11, "1")]),
                DxfError::MissingCoordinate {
                    line: 5,
                    entity: "LINE",
                    group: 21,
                },
            ),
        ];
        for (text, expected) in cases {
            assert_eq!(import(text.as_bytes()).unwrap_err(), expected, "{text:?}");
        }
    }

    #[test]
    fn limits_are_enforced() {
        let text = entities(
            &[
                LINE_0_0_TO_10_5.as_slice(),
                &LINE_0_0_TO_10_5,
                &LINE_0_0_TO_10_5,
            ]
            .concat(),
        );

        let too_big = ImportLimits {
            max_bytes: 10,
            ..Default::default()
        };
        assert_eq!(
            import_with_limits(text.as_bytes(), &too_big).unwrap_err(),
            DxfError::FileTooLarge {
                bytes: text.len(),
                limit: 10
            }
        );

        let too_many = ImportLimits {
            max_entities: 2,
            ..Default::default()
        };
        assert_eq!(
            import_with_limits(text.as_bytes(), &too_many).unwrap_err(),
            DxfError::TooManyEntities { limit: 2 }
        );

        let exact = ImportLimits {
            max_entities: 3,
            ..Default::default()
        };
        assert_eq!(
            import_with_limits(text.as_bytes(), &exact)
                .unwrap()
                .report
                .lines,
            3
        );
    }

    #[test]
    fn hostile_type_names_are_sanitised() {
        let long = "X".repeat(10_000);
        let import =
            import(entities(&[(0, long.as_str()), (0, "we\u{202e}ird name")]).as_bytes()).unwrap();
        let kinds: Vec<&String> = import.report.unsupported.keys().collect();
        assert_eq!(kinds, vec!["WEIRDNAME", &"X".repeat(MAX_KIND_LENGTH)]);
    }

    #[test]
    fn non_utf8_bytes_do_not_stop_an_import() {
        let mut bytes = b"999\ncaf\xe9\n".to_vec();
        bytes.extend_from_slice(entities(&LINE_0_0_TO_10_5).as_bytes());
        assert_eq!(import(&bytes).unwrap().report.lines, 1);
    }

    // ── hostile files (§43, #156): every bound is reached by a test ──

    fn layer_rows(names: &[&str]) -> Vec<(i32, &'static str)> {
        let mut groups = vec![(0, "SECTION"), (2, "TABLES"), (0, "TABLE"), (2, "LAYER")];
        for name in names {
            let name: &'static str = Box::leak((*name).to_owned().into_boxed_str());
            groups.extend([(0, "LAYER"), (2, name), (62, "7")]);
        }
        groups.extend([(0, "ENDTAB"), (0, "ENDSEC")]);
        groups
    }

    #[test]
    fn a_table_with_more_rows_than_the_limit_is_refused() {
        let mut groups = layer_rows(&["A", "B", "C"]);
        groups.extend([(0, "SECTION"), (2, "ENTITIES"), (0, "ENDSEC"), (0, "EOF")]);
        let text = dxf(&groups);
        let limits = ImportLimits {
            max_table_rows: 2,
            ..Default::default()
        };
        assert_eq!(
            import_with_limits(text.as_bytes(), &limits).unwrap_err(),
            DxfError::TooManyRows {
                table: "LAYER",
                limit: 2
            }
        );
        assert_eq!(import(text.as_bytes()).unwrap().report.layers, 3);
    }

    #[test]
    fn rows_made_for_undefined_names_stop_at_the_limit_and_are_reported() {
        let mut groups = vec![(0, "SECTION"), (2, "ENTITIES")];
        for layer in ["L1", "L2", "L3", "L4", "L5"] {
            groups.extend([
                (0, "LINE"),
                (8, layer),
                (10, "0"),
                (20, "0"),
                (11, "1"),
                (21, "1"),
            ]);
        }
        groups.extend([(0, "ENDSEC"), (0, "EOF")]);
        let text = dxf(&groups);
        let limits = ImportLimits {
            max_table_rows: 3,
            ..Default::default()
        };
        let imported = import_with_limits(text.as_bytes(), &limits).unwrap();
        assert_eq!(imported.report.lines, 5, "every line is imported");
        assert_eq!(imported.report.created_layers, 3);
        assert_eq!(
            imported.report.with_dropped_properties, 2,
            "two lines fell to layer 0, and it is said"
        );
        let on_zero = imported
            .drawing
            .records()
            .filter(|(_, record)| record.properties.layer == LayerId::FIRST)
            .count();
        assert_eq!(on_zero, 2);
    }

    #[test]
    fn a_record_with_more_groups_than_the_limit_is_refused() {
        let text = entities(&LINE_0_0_TO_10_5);
        let limits = ImportLimits {
            max_record_groups: 3,
            ..Default::default()
        };
        assert!(matches!(
            import_with_limits(text.as_bytes(), &limits).unwrap_err(),
            DxfError::RecordTooLarge { limit: 3, .. }
        ));
    }

    #[test]
    fn a_value_longer_than_the_limit_is_refused() {
        let text = entities(&[
            (0, "LWPOLYLINE"),
            (8, "ABCDEFGHIJ"),
            (10, "0"),
            (20, "0"),
            (10, "1"),
            (20, "1"),
        ]);
        let limits = ImportLimits {
            max_value_bytes: 5,
            ..Default::default()
        };
        assert!(matches!(
            import_with_limits(text.as_bytes(), &limits).unwrap_err(),
            DxfError::ValueTooLong { limit: 5, .. }
        ));
    }

    /// A chain of blocks, each placing the next; the model places the first.
    fn insert_chain(depth: usize) -> String {
        let names: Vec<String> = (0..depth).map(|i| format!("B{i}")).collect();
        let mut groups: Vec<(i32, &str)> = vec![(0, "SECTION"), (2, "BLOCKS")];
        for (i, name) in names.iter().enumerate() {
            groups.extend([(0, "BLOCK"), (2, name.as_str()), (10, "0"), (20, "0")]);
            match names.get(i + 1) {
                Some(next) => {
                    groups.extend([(0, "INSERT"), (2, next.as_str()), (10, "1"), (20, "0")])
                }
                None => groups.extend([(0, "LINE"), (10, "0"), (20, "0"), (11, "1"), (21, "0")]),
            }
            groups.push((0, "ENDBLK"));
        }
        groups.extend([(0, "ENDSEC"), (0, "SECTION"), (2, "ENTITIES")]);
        groups.extend([
            (0, "INSERT"),
            (2, "B0"),
            (10, "0"),
            (20, "0"),
            (0, "ENDSEC"),
            (0, "EOF"),
        ]);
        dxf(&groups)
    }

    #[test]
    fn blocks_nested_deeper_than_the_limit_are_reported() {
        let shallow = import(insert_chain(3).as_bytes()).unwrap();
        assert_eq!(shallow.report.blocks, 3);
        assert!(!shallow.report.unsupported.contains_key(NESTING_TOO_DEEP));

        let limits = ImportLimits {
            max_nesting: 1,
            ..Default::default()
        };
        let tight = import_with_limits(insert_chain(3).as_bytes(), &limits).unwrap();
        assert_eq!(tight.report.unsupported.get(NESTING_TOO_DEEP), Some(&1));

        // Forty deep is more than the model follows: imported, and said.
        let deep = import(insert_chain(40).as_bytes()).unwrap();
        assert_eq!((deep.report.blocks, deep.report.block_references), (40, 1));
        assert_eq!(deep.report.unsupported.get(NESTING_TOO_DEEP), Some(&1));
        assert!(deep.report.has_unsupported_content());
    }

    #[test]
    fn a_flood_of_empty_blocks_stops_at_the_row_limit() {
        let mut text = String::from("  0\nSECTION\n  2\nBLOCKS\n");
        for i in 0..150_000 {
            text.push_str(&format!("  0\nBLOCK\n  2\nB{i}\n  0\nENDBLK\n"));
        }
        text.push_str("  0\nENDSEC\n  0\nEOF\n");
        let started = std::time::Instant::now();
        assert_eq!(
            import(text.as_bytes()).unwrap_err(),
            DxfError::TooManyRows {
                table: "BLOCK",
                limit: ImportLimits::default().max_table_rows
            }
        );
        assert!(started.elapsed().as_secs() < 30, "{:?}", started.elapsed());
    }

    #[test]
    fn a_polyline_claiming_billions_of_vertices_imports_the_two_it_has() {
        let text = entities(&[
            (0, "LWPOLYLINE"),
            (90, "4294967296"),
            (70, "0"),
            (10, "0"),
            (20, "0"),
            (10, "10"),
            (20, "0"),
        ]);
        let imported = import(text.as_bytes()).unwrap();
        assert_eq!(polylines_of(&imported)[0].vertices.len(), 2);
        assert!(!imported.report.has_unsupported_content());
    }

    /// The reader's work is proportional to the bytes read.
    fn groups_of_nothing(megabytes: usize) -> String {
        let group = "999\nx\n";
        let mut text = group.repeat(megabytes * 1024 * 1024 / group.len());
        text.push_str("  0\nEOF\n");
        text
    }

    #[test]
    fn ten_megabytes_of_groups_import_in_bounded_time() {
        let text = groups_of_nothing(10);
        let started = std::time::Instant::now();
        assert!(import(text.as_bytes()).unwrap().drawing.is_empty());
        assert!(started.elapsed().as_secs() < 60, "{:?}", started.elapsed());
    }

    /// Run by `scripts/stress.sh`, in release: a hundred megabytes within
    /// the budget. A minute is generous; the point is that it is bounded.
    #[test]
    #[ignore = "a hundred megabytes: the time budget, run by scripts/stress.sh"]
    fn a_hundred_megabytes_of_groups_import_within_the_budget() {
        let text = groups_of_nothing(100);
        let started = std::time::Instant::now();
        assert!(import(text.as_bytes()).unwrap().drawing.is_empty());
        assert!(started.elapsed().as_secs() < 60, "{:?}", started.elapsed());
    }

    fn coordinate() -> impl Strategy<Value = f64> {
        prop_oneof![-1.0e9..1.0e9_f64, proptest::num::f64::NORMAL]
    }

    proptest! {
        /// Imported files are untrusted: no input may panic.
        #[test]
        fn arbitrary_bytes_never_panic(bytes in proptest::collection::vec(any::<u8>(), 0..512)) {
            let _ = import(&bytes);
        }

        #[test]
        fn arbitrary_dxf_shaped_text_never_panics(
            groups in proptest::collection::vec((0..1100i32, "[A-Z0-9.*$ -]{0,12}"), 0..60)
        ) {
            let text: String = groups.iter().map(|(code, value)| format!("{code}\n{value}\n")).collect();
            let _ = import(text.as_bytes());
        }

        /// #156: files of every record kind with any groups never panic,
        /// and whatever they import survives export → import → export.
        #[test]
        fn arbitrary_records_of_every_kind_never_panic_and_round_trip(
            records in proptest::collection::vec(
                (
                    prop_oneof![
                        Just("SECTION"), Just("ENDSEC"), Just("TABLE"), Just("ENDTAB"),
                        Just("LAYER"), Just("LTYPE"), Just("STYLE"), Just("BLOCK_RECORD"),
                        Just("BLOCK"), Just("ENDBLK"), Just("LINE"), Just("CIRCLE"), Just("ARC"),
                        Just("LWPOLYLINE"), Just("POLYLINE"), Just("VERTEX"), Just("SEQEND"),
                        Just("TEXT"), Just("MTEXT"), Just("INSERT"), Just("ATTRIB"), Just("HATCH"),
                        Just("EOF"), Just("DICTIONARY"), Just("LAYOUT"),
                    ],
                    proptest::collection::vec(
                        (
                            prop_oneof![
                                Just(1), Just(2), Just(3), Just(5), Just(6), Just(7), Just(8), Just(9),
                                Just(10), Just(20), Just(30), Just(11), Just(21), Just(40), Just(41),
                                Just(42), Just(43), Just(48), Just(49), Just(50), Just(51), Just(62),
                                Just(66), Just(70), Just(71), Just(72), Just(73), Just(74), Just(90),
                                Just(100), Just(330), Just(370), Just(420), Just(1001), 0..2000i32,
                            ],
                            prop_oneof![
                                (-1000i64..1000).prop_map(|n| n.to_string()),
                                (-100_000i64..100_000).prop_map(|n| format!("{}.{:03}", n / 1000, (n % 1000).abs())),
                                Just("1e308".to_owned()), Just("-1e308".to_owned()), Just("1e999".to_owned()),
                                Just("inf".to_owned()), Just("nan".to_owned()), Just(String::new()),
                                Just("4294967296".to_owned()),
                                "[A-Z]{1,6}", "[A-Za-z0-9 *$_.-]{0,10}",
                                Just("BYLAYER".to_owned()), Just("ByBlock".to_owned()),
                                Just("Continuous".to_owned()), Just("*Model_Space".to_owned()),
                                Just("ENTITIES".to_owned()), Just("TABLES".to_owned()),
                                Just("BLOCKS".to_owned()), Just("HEADER".to_owned()),
                                Just("\\P{a\\}".to_owned()),
                            ],
                        ),
                        0..10,
                    ),
                ),
                0..40,
            )
        ) {
            let mut text = String::new();
            for (kind, groups) in &records {
                text.push_str(&format!("  0\n{kind}\n"));
                for (code, value) in groups {
                    text.push_str(&format!("{code:>3}\n{value}\n"));
                }
            }
            text.push_str("  0\nEOF\n");
            if let Ok(imported) = import(text.as_bytes()) {
                let first = export(&imported.drawing);
                let again = import(first.as_bytes()).unwrap();
                prop_assert!(!again.report.has_unsupported_content(), "{:?}", again.report.summary());
                prop_assert_eq!(again.drawing.len(), imported.drawing.len());
                prop_assert_eq!(export(&again.drawing), first);
            }
        }

        /// #139: tables and property groups of any shape never panic, and
        /// always yield a drawing whose tables are consistent.
        #[test]
        fn arbitrary_tables_and_properties_never_panic(
            rows in proptest::collection::vec(
                (prop_oneof![Just("LAYER"), Just("LTYPE"), Just("STYLE")],
                 proptest::collection::vec(
                     (prop_oneof![Just(2), Just(3), Just(6), Just(62), Just(70), Just(370), Just(49), Just(74), Just(40), Just(73)],
                      "[A-Za-z0-9 .*,<-]{0,8}"),
                     0..8)),
                0..12),
            lines in proptest::collection::vec(
                proptest::collection::vec(
                    (prop_oneof![Just(8), Just(6), Just(62), Just(370), Just(48)], "[A-Za-z0-9 .<-]{0,8}"),
                    0..5),
                0..6),
            texts in proptest::collection::vec(
                (prop_oneof![Just("TEXT"), Just("MTEXT")],
                 proptest::collection::vec(
                     (prop_oneof![Just(1), Just(3), Just(7), Just(8), Just(40), Just(41), Just(50), Just(71), Just(72), Just(73), Just(11), Just(21)],
                      "[A-Za-z0-9 .^;{}\\\\P~<-]{0,10}"),
                     0..8)),
                0..4),
        ) {
            let mut groups: Vec<(i32, String)> = vec![(0, "SECTION".into()), (2, "TABLES".into())];
            for (kind, body) in &rows {
                groups.push((0, (*kind).into()));
                groups.extend(body.iter().map(|(code, value)| (*code, value.clone())));
            }
            groups.extend([(0, "ENDSEC".into()), (0, "SECTION".into()), (2, "ENTITIES".into())]);
            for body in &lines {
                groups.push((0, "LINE".into()));
                groups.extend(body.iter().map(|(code, value)| (*code, value.clone())));
                groups.extend([(10, "0".into()), (20, "0".into()), (11, "1".into()), (21, "1".into())]);
            }
            for (kind, body) in &texts {
                groups.push((0, (*kind).into()));
                groups.extend([(10, "0".into()), (20, "0".into()), (40, "2.5".into())]);
                groups.extend(body.iter().map(|(code, value)| (*code, value.clone())));
            }
            groups.extend([(0, "ENDSEC".into()), (0, "EOF".into())]);
            let text: String = groups.iter().map(|(code, value)| format!("{code}\n{value}\n")).collect();
            if let Ok(imported) = import(text.as_bytes()) {
                let drawing = &imported.drawing;
                prop_assert_eq!(drawing.len(), lines.len() + texts.len());
                for (_, entity) in drawing.entities() {
                    let style = match entity {
                        Entity::Text(t) => Some(t.style),
                        Entity::MText(t) => Some(t.style),
                        _ => None,
                    };
                    if let Some(style) = style {
                        prop_assert!(drawing.text_styles().contains(style));
                    }
                }
                for (_, record) in drawing.records() {
                    prop_assert!(drawing.layers().contains(record.properties.layer));
                    if let Linetype::Named(id) = record.properties.linetype {
                        prop_assert!(drawing.linetypes().contains(id));
                    }
                }
                for (_, layer) in drawing.layers().iter() {
                    if let Linetype::Named(id) = layer.linetype {
                        prop_assert!(drawing.linetypes().contains(id));
                    }
                }
                // And what came in goes out and comes back the same.
                let text = crate::export(drawing);
                let back = import(text.as_bytes()).unwrap();
                prop_assert_eq!(crate::export(&back.drawing), text);
            }
        }

        /// Export → import reproduces every coordinate bit for bit.
        /// #115: any polyline, bulged or not, survives export and import
        /// exactly, and a second export is byte-identical.
        #[test]
        fn polylines_with_bulges_round_trip_exactly(
            vertices in proptest::collection::vec((coordinate(), coordinate()), 2..8),
            bulges in proptest::collection::vec(prop_oneof![Just(0.0), -4.0..4.0f64], 0..8),
            closed in any::<bool>(),
        ) {
            let vertices: Vec<Point2> = vertices.into_iter().map(|(x, y)| p(x, y)).collect();
            let original = PolylineEntity::with_bulges(vertices, bulges, closed);
            let text = export(&Drawing::from_entities([Entity::Polyline(original.clone())]));
            let imported = import(text.as_bytes()).unwrap();
            prop_assert_eq!(polylines_of(&imported), vec![original]);
            prop_assert!(!imported.report.has_unsupported_content());
            prop_assert_eq!(export(&imported.drawing), text);
        }

        #[test]
        fn export_then_import_is_exact(
            coordinates in proptest::collection::vec((coordinate(), coordinate(), coordinate(), coordinate()), 0..20)
        ) {
            let original: Vec<LineEntity> =
                coordinates.iter().map(|&(x0, y0, x1, y1)| line(x0, y0, x1, y1)).collect();
            let drawing = Drawing::from_entities(original.iter().map(|line| Entity::Line(*line)));

            let text = export(&drawing);
            let imported = import(text.as_bytes()).unwrap();

            let mut reread: Vec<(u64, LineEntity)> =
                imported
                    .drawing
                    .entities()
                    .filter_map(|(id, entity)| Some((id.raw(), *entity.as_line()?)))
                    .collect();
            reread.sort_by_key(|(id, _)| *id);
            let reread: Vec<LineEntity> = reread.into_iter().map(|(_, line)| line).collect();
            prop_assert_eq!(reread, original);
            prop_assert!(!imported.report.has_unsupported_content());
            // And a second trip changes nothing.
            prop_assert_eq!(export(&imported.drawing), text);
        }
    }
}

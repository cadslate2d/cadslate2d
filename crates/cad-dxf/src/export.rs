//! DXF export (ADR-0018: ASCII DXF, release 2000).
//!
//! The file has a HEADER with the drawing's settings and extents, an empty
//! CLASSES section, TABLES with the drawing's linetypes, layers, text
//! styles, and block records (and the viewport, view, UCS, application,
//! and dimension-style tables the release expects, empty or with their one
//! standard row), BLOCKS with model space, paper space, and every block
//! definition, ENTITIES with each entity's layer and — when they are not
//! its layer's — its colour, linetype, and lineweight, and OBJECTS with the
//! dictionary and layouts that tie the spaces together. Every record has a
//! handle and names its owner; handles follow entity IDs, so the output is
//! deterministic.

use std::fmt::Write;

use cad_core::{EntityId, LengthUnit, Point2};
use cad_document::{
    ArcEntity, BlockId, BlockReferenceEntity, BlockTable, CircleEntity, Color, DimStyle,
    DimStyleId, DimensionEntity, Drawing, DrawingSettings, Entity, EntityRecord, HatchEntity,
    Justification, Layer, LineEntity, Linetype, LinetypeDef, LinetypeTable, Lineweight,
    MTextEntity, Measure, PolylineEntity, Properties, TextEntity, TextStyle, TextStyleTable,
};

use crate::properties::{
    LAYER_LOCKED, LINETYPE_ALIGNMENT_A, LINETYPE_BY_BLOCK, LINETYPE_BY_LAYER, color_to_dxf,
    lineweight_to_dxf, pattern_to_dxf,
};

/// The DXF release written, as stored in `$ACADVER`: R2000, the first
/// release with lineweights and MTEXT, and one every reader takes.
pub(crate) const ACADVER_R2000: &str = "AC1015";
/// The code page an ASCII file declares.
const CODEPAGE: &str = "ANSI_1252";
/// A paragraph's words are written in pieces of at most this many
/// characters (group 3), and the rest in group 1.
const MTEXT_PIECE: usize = 250;
/// The width-to-height ratio of the viewport that opens on the drawing.
const VIEW_ASPECT: f64 = 1.5;

/// A record's handle: a hexadecimal number unique in the file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Handle(u64);

impl Handle {
    /// The owner of what has none: the root dictionary and the tables.
    const NONE: Self = Self(0);

    fn text(self) -> String {
        format!("{:X}", self.0)
    }
}

/// Hands out handles in order, so that the output is deterministic.
struct Handles(u64);

impl Handles {
    fn take(&mut self) -> Handle {
        let handle = Handle(self.0);
        self.0 += 1;
        handle
    }

    /// `$HANDSEED`: the first handle a reader may give to something new.
    fn seed(&self) -> Handle {
        Handle(self.0)
    }
}

/// The records everything else points at. Allocated first, so their
/// handles do not depend on the drawing.
struct Structure {
    root: Handle,
    groups: Handle,
    layouts: Handle,
    plot_styles: Handle,
    plot_style: Handle,
    model_layout: Handle,
    paper_layout: Handle,
    model_space: Handle,
    paper_space: Handle,
}

impl Structure {
    fn allocate(handles: &mut Handles) -> Self {
        Self {
            root: handles.take(),
            groups: handles.take(),
            layouts: handles.take(),
            plot_styles: handles.take(),
            plot_style: handles.take(),
            model_layout: handles.take(),
            paper_layout: handles.take(),
            model_space: handles.take(),
            paper_space: handles.take(),
        }
    }
}

/// Serialises the drawing as ASCII DXF.
///
/// Output is deterministic: entities are written in `EntityId` order with
/// handles in that order, and coordinates use the shortest decimal form
/// that reads back to exactly the same `f64`.
pub fn export(drawing: &Drawing) -> String {
    let mut handles = Handles(1);
    let structure = Structure::allocate(&mut handles);
    // One block record per definition, in table order; the record is what
    // a definition's entities name as their owner.
    let block_records: Vec<Handle> = drawing
        .blocks()
        .iter()
        .filter(|(id, _)| *id != BlockId::FIRST)
        .map(|_| handles.take())
        .collect();
    // One anonymous block per dimension, holding its geometry as our
    // layout draws it, for a reader that does not lay dimensions out.
    let dimensions = dimensions_in_write_order(drawing);
    let dimension_records: Vec<Handle> = dimensions.iter().map(|_| handles.take()).collect();
    let mut body = Writer {
        out: String::new(),
        handles,
        dimensions_named: 0,
    };
    body.group(0, "SECTION");
    body.group(2, "CLASSES");
    body.group(0, "ENDSEC");
    body.section("TABLES", |w| {
        w.tables(drawing, &structure, &block_records, &dimension_records);
    });
    body.section("BLOCKS", |w| {
        w.blocks(drawing, &structure, &block_records);
        w.dimension_blocks(drawing, &dimensions, &dimension_records);
    });
    body.section("ENTITIES", |w| {
        let mut records: Vec<(EntityId, &EntityRecord)> = drawing.records().collect();
        records.sort_unstable_by_key(|(id, _)| *id);
        for (_, record) in records {
            w.record(record, structure.model_space, drawing);
        }
    });
    body.section("OBJECTS", |w| w.objects(&structure));
    body.group(0, "EOF");

    // The header names the first free handle, so it is written last.
    let mut header = Writer {
        out: String::new(),
        handles: Handles(0),
        dimensions_named: 0,
    };
    header.section("HEADER", |w| w.header(drawing, body.handles.seed()));
    header.out + &body.out
}

struct Writer {
    out: String,
    handles: Handles,
    /// DIMENSION records written so far: the next one's block is `*D` and
    /// this number plus one, which is the order `dimension_blocks` writes
    /// them in.
    dimensions_named: usize,
}

/// Every dimension, with its properties, in the order their DIMENSION
/// records are written: those inside block definitions first, in table
/// order, then the drawing's own in ID order — so that the n-th DIMENSION
/// written owns the n-th anonymous block.
fn dimensions_in_write_order(drawing: &Drawing) -> Vec<(DimensionEntity, Properties)> {
    let mut out = Vec::new();
    let mut take = |record: &EntityRecord| {
        if let Entity::Dimension(dimension) = &*record.entity {
            out.push((dimension.clone(), record.properties));
        }
    };
    for (id, block) in drawing.blocks().iter() {
        if id != BlockId::FIRST {
            block.entities.iter().for_each(&mut take);
        }
    }
    let mut records: Vec<(EntityId, &EntityRecord)> = drawing.records().collect();
    records.sort_unstable_by_key(|(id, _)| *id);
    records.into_iter().for_each(|(_, record)| take(record));
    out
}

/// The name of the n-th dimension's anonymous block, counting from one.
fn dimension_block_name(n: usize) -> String {
    format!("*D{n}")
}

impl Writer {
    /// One group: the code right-aligned in three columns, then the value.
    fn group(&mut self, code: i32, value: &str) {
        writeln!(self.out, "{code:>3}\n{value}").expect("writing to a String cannot fail");
    }

    fn real(&mut self, code: i32, value: f64) {
        self.group(code, &format_real(value));
    }

    fn int(&mut self, code: i32, value: i64) {
        self.group(code, &value.to_string());
    }

    fn handle(&mut self, code: i32, handle: Handle) {
        self.group(code, &handle.text());
    }

    /// A point in groups `code`, `code + 10`, `code + 20`; the Z is zero.
    fn point(&mut self, code: i32, point: Point2) {
        self.real(code, point.x);
        self.real(code + 10, point.y);
        self.real(code + 20, 0.0);
    }

    fn section(&mut self, name: &str, body: impl FnOnce(&mut Self)) {
        self.group(0, "SECTION");
        self.group(2, name);
        body(self);
        self.group(0, "ENDSEC");
    }

    // ── HEADER ──

    /// `$ACADVER` and the code page; the drawing's unit; the settings for
    /// new objects, when they are not a new drawing's; the extents; and
    /// the first free handle.
    fn header(&mut self, drawing: &Drawing, seed: Handle) {
        let linetypes = drawing.linetypes();
        self.group(9, "$ACADVER");
        self.group(1, ACADVER_R2000);
        self.group(9, "$DWGCODEPAGE");
        self.group(3, CODEPAGE);
        if let Some(unit) = drawing.unit() {
            self.group(9, "$INSUNITS");
            self.int(70, insunits_code(unit));
        }
        let settings = drawing.settings();
        let defaults = DrawingSettings::default();
        if settings.current_layer != defaults.current_layer
            && let Some(layer) = drawing.layers().get(settings.current_layer)
        {
            self.group(9, "$CLAYER");
            self.group(8, &layer.name);
        }
        if settings.current_color != defaults.current_color {
            self.group(9, "$CECOLOR");
            self.int(62, color_to_dxf(settings.current_color));
        }
        if settings.current_linetype != defaults.current_linetype {
            self.group(9, "$CELTYPE");
            self.group(6, &linetype_name(settings.current_linetype, linetypes));
        }
        if settings.current_lineweight != defaults.current_lineweight {
            self.group(9, "$CELWEIGHT");
            self.int(370, lineweight_to_dxf(settings.current_lineweight));
        }
        if settings.current_dim_style != DimStyleId::FIRST
            && let Some(style) = drawing.dim_styles().get(settings.current_dim_style)
        {
            self.group(9, "$DIMSTYLE");
            self.group(2, &style.name);
        }
        // Exactly the default is left out; any other value is written as it is.
        #[allow(clippy::float_cmp)]
        if settings.ltscale != defaults.ltscale {
            self.group(9, "$LTSCALE");
            self.real(40, settings.ltscale);
        }
        if let Some(extents) = drawing.extents() {
            self.group(9, "$EXTMIN");
            self.point(10, extents.min());
            self.group(9, "$EXTMAX");
            self.point(10, extents.max());
        }
        self.group(9, "$HANDSEED");
        self.handle(5, seed);
    }

    // ── TABLES ──

    fn tables(
        &mut self,
        drawing: &Drawing,
        structure: &Structure,
        block_records: &[Handle],
        dimension_records: &[Handle],
    ) {
        let linetypes = drawing.linetypes();
        self.table("VPORT", 1, |w, table| {
            w.row("VPORT", table, "AcDbViewportTableRecord");
            w.viewport(drawing);
        });
        // The ByBlock and ByLayer rows every file of this release carries,
        // then the drawing's own, Continuous first.
        self.table("LTYPE", linetypes.len() + 2, |w, table| {
            for name in ["ByBlock", "ByLayer"] {
                w.row("LTYPE", table, "AcDbLinetypeTableRecord");
                w.linetype(&LinetypeDef {
                    name: name.to_owned(),
                    description: String::new(),
                    pattern: Vec::new(),
                });
            }
            for (_, def) in linetypes.iter() {
                w.row("LTYPE", table, "AcDbLinetypeTableRecord");
                w.linetype(def);
            }
        });
        self.table("LAYER", drawing.layers().len(), |w, table| {
            for (_, layer) in drawing.layers().iter() {
                w.row("LAYER", table, "AcDbLayerTableRecord");
                w.layer(layer, linetypes, structure.plot_style);
            }
        });
        self.table("STYLE", drawing.text_styles().len(), |w, table| {
            for (_, style) in drawing.text_styles().iter() {
                w.row("STYLE", table, "AcDbTextStyleTableRecord");
                w.text_style(style);
            }
        });
        self.table("VIEW", 0, |_, _| {});
        self.table("UCS", 0, |_, _| {});
        self.table("APPID", 1, |w, table| {
            w.row("APPID", table, "AcDbRegAppTableRecord");
            w.group(2, "ACAD");
            w.group(70, "0");
        });
        let dim_styles = drawing.dim_styles();
        self.table("DIMSTYLE", dim_styles.len(), |w, table| {
            w.group(100, "AcDbDimStyleTable");
            w.group(71, "0");
            for (_, style) in dim_styles.iter() {
                w.dim_style(table, style);
            }
        });
        let blocks = drawing.blocks();
        let rows = blocks.len() + 1 + dimension_records.len();
        self.table("BLOCK_RECORD", rows, |w, table| {
            for (name, record, layout) in [
                (
                    "*Model_Space",
                    structure.model_space,
                    structure.model_layout,
                ),
                (
                    "*Paper_Space",
                    structure.paper_space,
                    structure.paper_layout,
                ),
            ] {
                w.block_record(table, record, name, layout);
            }
            let user = blocks.iter().filter(|(id, _)| *id != BlockId::FIRST);
            for ((_, block), record) in user.zip(block_records) {
                w.block_record(table, *record, &block.name, Handle::NONE);
            }
            for (n, record) in dimension_records.iter().enumerate() {
                w.block_record(table, *record, &dimension_block_name(n + 1), Handle::NONE);
            }
        });
    }

    /// One table: its handle, the number of rows, and the rows.
    fn table(&mut self, name: &str, rows: usize, body: impl FnOnce(&mut Self, Handle)) {
        let table = self.handles.take();
        self.group(0, "TABLE");
        self.group(2, name);
        self.handle(5, table);
        self.handle(330, Handle::NONE);
        self.group(100, "AcDbSymbolTable");
        self.int(70, rows as i64);
        body(self, table);
        self.group(0, "ENDTAB");
    }

    /// The groups every table row starts with, up to its own subclass.
    fn row(&mut self, kind: &str, table: Handle, subclass: &str) -> Handle {
        let handle = self.handles.take();
        self.group(0, kind);
        self.handle(5, handle);
        self.handle(330, table);
        self.group(100, "AcDbSymbolTableRecord");
        self.group(100, subclass);
        handle
    }

    /// The one viewport, `*Active`, looking at the whole drawing.
    fn viewport(&mut self, drawing: &Drawing) {
        let fallback = (Point2::ORIGIN, 1000.0);
        let (center, height) = match drawing.extents() {
            Some(extents) => {
                // Halves first: a drawing at the edge of what a number
                // holds must not overflow into a view no reader takes.
                let (min, max) = (extents.min(), extents.max());
                let center = Point2::new(min.x * 0.5 + max.x * 0.5, min.y * 0.5 + max.y * 0.5);
                let width = max.x * 0.5 - min.x * 0.5;
                let height = (max.y * 0.5 - min.y * 0.5).max(width / VIEW_ASPECT) * 2.2;
                if center.x.is_finite() && center.y.is_finite() && height.is_finite() {
                    (center, height.max(1.0))
                } else {
                    fallback
                }
            }
            None => fallback,
        };
        self.group(2, "*Active");
        self.group(70, "0");
        self.real(10, 0.0);
        self.real(20, 0.0);
        self.real(11, 1.0);
        self.real(21, 1.0);
        self.real(12, center.x);
        self.real(22, center.y);
        self.real(13, 0.0);
        self.real(23, 0.0);
        self.real(14, 0.5);
        self.real(24, 0.5);
        self.real(15, 0.5);
        self.real(25, 0.5);
        self.real(16, 0.0);
        self.real(26, 0.0);
        self.real(36, 1.0);
        self.real(17, 0.0);
        self.real(27, 0.0);
        self.real(37, 0.0);
        self.real(40, height);
        self.real(41, VIEW_ASPECT);
        self.real(42, 50.0);
        self.real(43, 0.0);
        self.real(44, 0.0);
        self.real(50, 0.0);
        self.real(51, 0.0);
        self.group(71, "0");
        self.group(72, "1000");
        self.group(73, "1");
        self.group(74, "3");
        self.group(75, "0");
        self.group(76, "0");
        self.group(77, "0");
        self.group(78, "0");
        self.group(281, "0");
        self.group(65, "0");
        self.real(146, 0.0);
    }

    /// An LTYPE row's own groups: flags, description, alignment, then the
    /// pattern as its count, its length, and each element — gaps negative —
    /// with the element type that says it is a plain dash.
    fn linetype(&mut self, def: &LinetypeDef) {
        self.group(2, &def.name);
        self.group(70, "0");
        self.group(3, &def.description);
        self.int(72, LINETYPE_ALIGNMENT_A);
        self.int(73, def.pattern.len() as i64);
        self.real(40, def.period());
        for element in pattern_to_dxf(&def.pattern) {
            self.real(49, element);
            self.group(74, "0");
        }
    }

    /// A LAYER row's own groups: flags, colour — negative when the layer
    /// is off — linetype, lineweight, and the plot style every layer shares.
    fn layer(&mut self, layer: &Layer, table: &LinetypeTable, plot_style: Handle) {
        self.group(2, &layer.name);
        self.int(70, if layer.locked { LAYER_LOCKED } else { 0 });
        let color = match layer.color {
            Color::Index(index) => i64::from(index),
            // A layer's colour is what ByLayer means: an indirection here is
            // written as the colour a new layer has.
            Color::ByLayer | Color::ByBlock => color_to_dxf(Layer::DEFAULT_COLOR),
        };
        self.int(62, if layer.on { color } else { -color });
        let linetype = match layer.linetype {
            Linetype::Named(_) => layer.linetype,
            Linetype::ByLayer | Linetype::ByBlock => Linetype::CONTINUOUS,
        };
        self.group(6, &linetype_name(linetype, table));
        // A layer's lineweight is what ByLayer means: an indirection here
        // is the default width.
        let lineweight = match layer.lineweight {
            Lineweight::Hundredths(_) => layer.lineweight,
            Lineweight::ByLayer | Lineweight::ByBlock | Lineweight::Default => Lineweight::Default,
        };
        self.int(370, lineweight_to_dxf(lineweight));
        self.handle(390, plot_style);
    }

    /// A STYLE row's own groups: flags, no fixed height, a width factor of
    /// one, no oblique angle, the last height used, and the stroke font
    /// every style draws with.
    fn text_style(&mut self, style: &TextStyle) {
        self.group(2, &style.name);
        self.group(70, "0");
        self.real(40, 0.0);
        self.real(41, 1.0);
        self.real(50, 0.0);
        self.group(71, "0");
        self.real(42, 2.5);
        self.group(3, "txt");
        self.group(4, "");
    }

    /// A DIMSTYLE row — whose handle is group 105, alone among the tables —
    /// with the variables the drawing holds: the suffix as DIMPOST (3), with
    /// `<>` standing for the value; arrow size (41); extension offset (42)
    /// and overshoot (44); text height (140); text gap (147); decimals (271).
    fn dim_style(&mut self, table: Handle, style: &DimStyle) {
        let handle = self.handles.take();
        self.group(0, "DIMSTYLE");
        self.handle(105, handle);
        self.handle(330, table);
        self.group(100, "AcDbSymbolTableRecord");
        self.group(100, "AcDbDimStyleTableRecord");
        self.group(2, &style.name);
        self.group(70, "0");
        if !style.suffix.is_empty() {
            self.group(3, &format!("<>{}", style.suffix));
        }
        self.real(41, style.arrow_size);
        self.real(42, style.extension_offset);
        self.real(44, style.extension_overshoot);
        self.real(140, style.text_height);
        self.real(147, style.text_gap);
        self.int(271, i64::from(style.decimals));
    }

    /// A BLOCK_RECORD row: the block by name, and its layout when it is a
    /// space.
    fn block_record(&mut self, table: Handle, record: Handle, name: &str, layout: Handle) {
        self.group(0, "BLOCK_RECORD");
        self.handle(5, record);
        self.handle(330, table);
        self.group(100, "AcDbSymbolTableRecord");
        self.group(100, "AcDbBlockTableRecord");
        self.group(2, name);
        self.handle(340, layout);
    }

    // ── BLOCKS ──

    /// Model space and paper space, empty, then every definition with its
    /// entities, owned by its block record.
    fn blocks(&mut self, drawing: &Drawing, structure: &Structure, block_records: &[Handle]) {
        self.block(
            "*Model_Space",
            Point2::ORIGIN,
            0,
            structure.model_space,
            |_| {},
        );
        self.block(
            "*Paper_Space",
            Point2::ORIGIN,
            0,
            structure.paper_space,
            |_| {},
        );
        let user = drawing
            .blocks()
            .iter()
            .filter(|(id, _)| *id != BlockId::FIRST);
        for ((_, block), owner) in user.zip(block_records) {
            self.block(&block.name, block.base, 0, *owner, |w| {
                for entity in block.entities.iter() {
                    w.record(entity, *owner, drawing);
                }
            });
        }
    }

    /// The anonymous blocks (flag 1) of the dimensions, in the order their
    /// records are written: each holds what the dimension's style lays out
    /// — lines, arrowhead outlines, the arc, the text — with the
    /// dimension's properties, as EXPLODE would make them (ADR-0020).
    fn dimension_blocks(
        &mut self,
        drawing: &Drawing,
        dimensions: &[(DimensionEntity, Properties)],
        dimension_records: &[Handle],
    ) {
        for (n, ((dimension, properties), owner)) in
            dimensions.iter().zip(dimension_records).enumerate()
        {
            let parts = Entity::from(dimension.clone())
                .exploded(*properties, drawing.tables())
                .unwrap_or_default();
            self.block(
                &dimension_block_name(n + 1),
                Point2::ORIGIN,
                1,
                *owner,
                |w| {
                    for (part, properties) in parts {
                        w.record(&EntityRecord::new(part, properties), *owner, drawing);
                    }
                },
            );
        }
    }

    fn block(
        &mut self,
        name: &str,
        base: Point2,
        flags: i64,
        record: Handle,
        body: impl FnOnce(&mut Self),
    ) {
        let begin = self.handles.take();
        self.group(0, "BLOCK");
        self.handle(5, begin);
        self.handle(330, record);
        self.group(100, "AcDbEntity");
        self.group(8, Layer::ZERO);
        self.group(100, "AcDbBlockBegin");
        self.group(2, name);
        self.int(70, flags);
        self.point(10, base);
        self.group(3, name);
        self.group(1, "");
        body(self);
        let end = self.handles.take();
        self.group(0, "ENDBLK");
        self.handle(5, end);
        self.handle(330, record);
        self.group(100, "AcDbEntity");
        self.group(8, Layer::ZERO);
        self.group(100, "AcDbBlockEnd");
    }

    // ── ENTITIES ──

    /// One entity with its properties, as a record of ENTITIES or of a block.
    fn record(&mut self, record: &EntityRecord, owner: Handle, drawing: &Drawing) {
        let layer = drawing
            .layers()
            .get(record.properties.layer)
            .map_or(Layer::ZERO, |layer| layer.name.as_str());
        let kind = match &*record.entity {
            Entity::Line(_) => "LINE",
            Entity::Circle(_) => "CIRCLE",
            Entity::Arc(_) => "ARC",
            Entity::Polyline(_) => "LWPOLYLINE",
            Entity::Text(_) => "TEXT",
            Entity::MText(_) => "MTEXT",
            Entity::BlockRef(_) => "INSERT",
            Entity::Dimension(_) => "DIMENSION",
            Entity::Hatch(_) => "HATCH",
        };
        self.entity(kind, owner, layer, &record.properties, drawing.linetypes());
        match &*record.entity {
            Entity::Line(line) => self.line(line),
            Entity::Circle(circle) => self.circle(circle),
            Entity::Arc(arc) => self.arc(arc),
            Entity::Polyline(polyline) => self.polyline(polyline),
            Entity::Text(text) => self.text(text, drawing.text_styles()),
            Entity::MText(text) => self.mtext(text, drawing.text_styles()),
            Entity::BlockRef(reference) => self.insert(reference, drawing.blocks()),
            Entity::Dimension(dimension) => self.dimension(dimension, drawing),
            Entity::Hatch(hatch) => self.hatch(hatch),
        }
    }

    /// A solid HATCH (ADR-0021): the solid flag (70), associativity off
    /// (71), its loops as polyline boundary paths (92 flag 2, plus 1 for
    /// the outermost) with their bulges (72), closed (73), and their
    /// vertices (93, then 10/20 and 42); the normal-parity style (75), the
    /// predefined pattern kind (76), no seed points (98).
    fn hatch(&mut self, hatch: &HatchEntity) {
        self.group(100, "AcDbHatch");
        self.point(10, Point2::ORIGIN);
        self.real(210, 0.0);
        self.real(220, 0.0);
        self.real(230, 1.0);
        self.group(2, "SOLID");
        self.int(70, 1);
        self.int(71, 0);
        self.int(91, hatch.loops.len() as i64);
        for (index, path) in hatch.loops.iter().enumerate() {
            let flags = 2 | if index == 0 { 1 } else { 16 };
            self.int(92, flags);
            let bulged = path.bulges.iter().any(|bulge| *bulge != 0.0);
            self.int(72, i64::from(bulged));
            self.int(73, 1);
            self.int(93, path.vertices.len() as i64);
            for (vertex, bulge) in path.vertices.iter().zip(&path.bulges) {
                self.real(10, vertex.x);
                self.real(20, vertex.y);
                if bulged {
                    self.real(42, *bulge);
                }
            }
            self.int(97, 0);
        }
        self.int(75, 0);
        self.int(76, 1);
        self.int(98, 0);
    }

    /// A DIMENSION (ADR-0020): its anonymous block (2), its kind (70), its
    /// definition points, its style (3), its text (1) when it has words of
    /// its own, its text's angle (53) as the turn away from where the text
    /// would read on its own, its measured value (42), and where its text
    /// is (11) — the placement point itself for a radius or diameter, which
    /// is where the text goes. A linear dimension is a rotated one: the
    /// aligned subclass, then the rotation (50).
    fn dimension(&mut self, dimension: &DimensionEntity, drawing: &Drawing) {
        let style = drawing
            .dim_styles()
            .get(dimension.style)
            .map_or(DimStyle::STANDARD, |style| style.name.as_str())
            .to_owned();
        let style_row = drawing.tables().dim_style(dimension.style);
        let layout = dimension.layout(style_row);
        self.dimensions_named += 1;
        self.group(100, "AcDbDimension");
        self.group(2, &dimension_block_name(self.dimensions_named));
        let (kind, definition, text_point) = match dimension.measure {
            Measure::Linear { .. } => (0, dimension.line_point, layout.text.insertion),
            Measure::Aligned { .. } => (1, dimension.line_point, layout.text.insertion),
            Measure::Angular { .. } => (5, dimension.line_point, layout.text.insertion),
            Measure::Radius { center, .. } => (4, center, dimension.line_point),
            // The point on the circle opposite the leader.
            Measure::Diameter { .. } => (
                3,
                dimension
                    .circle_points()
                    .map_or(dimension.line_point, |(_, far)| far),
                dimension.line_point,
            ),
        };
        self.point(10, definition);
        self.point(11, text_point);
        // 32: the block is this dimension's own; 128: the text is where the
        // drafter put it.
        self.int(70, kind | 32 | 128);
        self.int(71, 5);
        self.real(42, layout.value);
        if let Some(text) = &dimension.text_override {
            self.group(1, text);
        }
        if let Some(angle) = dimension.text_angle {
            let on_its_own = dimension
                .clone()
                .turned(None)
                .layout(style_row)
                .text
                .rotation;
            self.real(53, (angle - on_its_own).to_degrees());
        }
        self.group(3, &style);
        match dimension.measure {
            Measure::Linear { a, b, rotation } => {
                self.group(100, "AcDbAlignedDimension");
                self.point(13, a);
                self.point(14, b);
                self.group(100, "AcDbRotatedDimension");
                self.real(50, rotation.to_degrees());
            }
            Measure::Aligned { a, b } => {
                self.group(100, "AcDbAlignedDimension");
                self.point(13, a);
                self.point(14, b);
            }
            Measure::Angular { vertex, a, b } => {
                self.group(100, "AcDb3PointAngularDimension");
                self.point(13, a);
                self.point(14, b);
                self.point(15, vertex);
            }
            Measure::Radius { .. } | Measure::Diameter { .. } => {
                self.group(
                    100,
                    if matches!(dimension.measure, Measure::Radius { .. }) {
                        "AcDbRadialDimension"
                    } else {
                        "AcDbDiametricDimension"
                    },
                );
                if let Some((near, _)) = dimension.circle_points() {
                    self.point(15, near);
                }
                self.real(40, 0.0);
            }
        }
    }

    /// The groups every entity starts with: its handle and owner, then —
    /// under `AcDbEntity` — its layer, and its linetype, colour, and
    /// lineweight when they are its own.
    fn entity(
        &mut self,
        kind: &str,
        owner: Handle,
        layer: &str,
        properties: &Properties,
        table: &LinetypeTable,
    ) {
        let handle = self.handles.take();
        self.group(0, kind);
        self.handle(5, handle);
        self.handle(330, owner);
        self.group(100, "AcDbEntity");
        self.group(8, layer);
        if properties.linetype != Linetype::ByLayer {
            self.group(6, &linetype_name(properties.linetype, table));
        }
        if properties.color != Color::ByLayer {
            self.int(62, color_to_dxf(properties.color));
        }
        if properties.lineweight != Lineweight::ByLayer {
            self.int(370, lineweight_to_dxf(properties.lineweight));
        }
    }

    fn line(&mut self, line: &LineEntity) {
        self.group(100, "AcDbLine");
        self.point(10, line.start);
        self.point(11, line.end);
    }

    fn circle(&mut self, circle: &CircleEntity) {
        self.group(100, "AcDbCircle");
        self.point(10, circle.center);
        self.real(40, circle.radius);
    }

    /// DXF arcs run counter-clockwise from the start angle to the end
    /// angle, in degrees — the same convention as the document, in
    /// different units.
    fn arc(&mut self, arc: &ArcEntity) {
        self.group(100, "AcDbCircle");
        self.point(10, arc.center);
        self.real(40, arc.radius);
        self.group(100, "AcDbArc");
        self.real(50, arc.start_angle.to_degrees());
        self.real(51, arc.end_angle.to_degrees());
    }

    /// An LWPOLYLINE: the vertex count, the closed flag, then each vertex
    /// and — when the segment starting there is an arc — its bulge.
    fn polyline(&mut self, polyline: &PolylineEntity) {
        /// Bit 1 of the polyline flags: closed.
        const CLOSED: i64 = 1;
        self.group(100, "AcDbPolyline");
        self.int(90, polyline.vertices.len() as i64);
        self.int(70, if polyline.closed { CLOSED } else { 0 });
        for (vertex, bulge) in polyline.vertices.iter().zip(polyline.bulges()) {
            self.real(10, vertex.x);
            self.real(20, vertex.y);
            if bulge.abs() > 0.0 {
                self.real(42, bulge);
            }
        }
    }

    /// A TEXT: its first point, height, words, rotation, style, and — for
    /// centred and right-justified text — the horizontal justification
    /// (group 72) with the alignment point in 11/21, which is where the
    /// text is justified to. A left-justified text writes neither.
    fn text(&mut self, text: &TextEntity, styles: &TextStyleTable) {
        self.group(100, "AcDbText");
        // For a justified text the first point is the baseline start, as
        // DXF readers expect, and the alignment point is the insertion.
        self.point(10, text.start());
        self.real(40, text.height);
        self.group(1, &text.content);
        if text.rotation != 0.0 {
            self.real(50, text.rotation.to_degrees());
        }
        let style = styles
            .get(text.style)
            .map_or(TextStyle::STANDARD, |style| style.name.as_str());
        if style != TextStyle::STANDARD {
            self.group(7, style);
        }
        let justification = match text.justification {
            Justification::Left => None,
            Justification::Center => Some(1),
            Justification::Right => Some(2),
        };
        if let Some(code) = justification {
            self.int(72, code);
            self.point(11, text.insertion);
        }
        self.group(100, "AcDbText");
    }

    /// An MTEXT: its top left, height, width, the top-left attachment, its
    /// style, and its words — line breaks as `\P`, the characters DXF
    /// reads as codes escaped — in pieces of 250 characters, the last in
    /// group 1; then its rotation when it has one.
    fn mtext(&mut self, text: &MTextEntity, styles: &TextStyleTable) {
        /// Attachment point 1: the insertion is the top left.
        const TOP_LEFT: &str = "1";
        self.group(100, "AcDbMText");
        self.point(10, text.insertion);
        self.real(40, text.height);
        self.real(41, text.width);
        self.group(71, TOP_LEFT);
        let style = styles
            .get(text.style)
            .map_or(TextStyle::STANDARD, |style| style.name.as_str());
        if style != TextStyle::STANDARD {
            self.group(7, style);
        }
        let coded = mtext_codes(&text.content);
        let pieces: Vec<String> = coded
            .chars()
            .collect::<Vec<char>>()
            .chunks(MTEXT_PIECE)
            .map(|piece| piece.iter().collect())
            .collect();
        let (last, first) = pieces
            .split_last()
            .map_or(("", &[][..]), |(last, first)| (last.as_str(), first));
        for piece in first {
            self.group(3, piece);
        }
        self.group(1, last);
        if text.rotation != 0.0 {
            self.real(50, text.rotation.to_degrees());
        }
    }

    /// An INSERT: the block by name, the insertion point, the scale along
    /// X, Y, and Z when it is not one — a mirrored reference has its Y
    /// scale negated, which is how DXF says it — and the rotation in
    /// degrees when it is not zero.
    fn insert(&mut self, reference: &BlockReferenceEntity, blocks: &BlockTable) {
        self.group(100, "AcDbBlockReference");
        let name = blocks
            .get(reference.block)
            .map_or("", |block| block.name.as_str());
        self.group(2, name);
        self.point(10, reference.insertion);
        // Exactly one is no scale at all.
        #[allow(clippy::float_cmp)]
        if reference.scale != 1.0 || reference.mirrored {
            let y = if reference.mirrored {
                -reference.scale
            } else {
                reference.scale
            };
            self.real(41, reference.scale);
            self.real(42, y);
            self.real(43, reference.scale);
        }
        if reference.rotation != 0.0 {
            self.real(50, reference.rotation.to_degrees());
        }
    }

    // ── OBJECTS ──

    /// The root dictionary with the groups, layouts, and plot-style
    /// entries the release expects; the two layouts, one per space; and
    /// the one plot style, a placeholder.
    fn objects(&mut self, structure: &Structure) {
        self.dictionary(structure.root, Handle::NONE);
        self.group(3, "ACAD_GROUP");
        self.handle(350, structure.groups);
        self.group(3, "ACAD_LAYOUT");
        self.handle(350, structure.layouts);
        self.group(3, "ACAD_PLOTSTYLENAME");
        self.handle(350, structure.plot_styles);

        self.dictionary(structure.groups, structure.root);

        self.dictionary(structure.layouts, structure.root);
        self.group(3, "Layout1");
        self.handle(350, structure.paper_layout);
        self.group(3, "Model");
        self.handle(350, structure.model_layout);

        self.group(0, "ACDBDICTIONARYWDFLT");
        self.handle(5, structure.plot_styles);
        self.handle(330, structure.root);
        self.group(100, "AcDbDictionary");
        self.group(281, "1");
        self.group(3, "Normal");
        self.handle(350, structure.plot_style);
        self.group(100, "AcDbDictionaryWithDefault");
        self.handle(340, structure.plot_style);

        self.group(0, "ACDBPLACEHOLDER");
        self.handle(5, structure.plot_style);
        self.handle(330, structure.plot_styles);

        self.layout(
            structure.model_layout,
            structure.layouts,
            "Model",
            structure.model_space,
            true,
        );
        self.layout(
            structure.paper_layout,
            structure.layouts,
            "Layout1",
            structure.paper_space,
            false,
        );
    }

    fn dictionary(&mut self, handle: Handle, owner: Handle) {
        self.group(0, "DICTIONARY");
        self.handle(5, handle);
        self.handle(330, owner);
        self.group(100, "AcDbDictionary");
        // Duplicate record cloning: keep existing.
        self.group(281, "1");
    }

    /// A LAYOUT: A3 plot settings nobody uses, and the layout proper,
    /// which names its block record.
    fn layout(&mut self, handle: Handle, owner: Handle, name: &str, space: Handle, model: bool) {
        self.group(0, "LAYOUT");
        self.handle(5, handle);
        self.handle(330, owner);
        self.group(100, "AcDbPlotSettings");
        self.group(1, "");
        self.group(4, "A3");
        self.group(6, "");
        for (code, value) in [
            (40, 7.5),
            (41, 20.0),
            (42, 7.5),
            (43, 20.0),
            (44, 420.0),
            (45, 297.0),
            (46, 0.0),
            (47, 0.0),
            (48, 0.0),
            (49, 0.0),
            (140, 0.0),
            (141, 0.0),
            (142, 1.0),
            (143, 1.0),
        ] {
            self.real(code, value);
        }
        // Plot layout flags: the model layout is "model type".
        self.group(70, if model { "1024" } else { "0" });
        self.group(72, "1");
        self.group(73, "0");
        self.group(74, "5");
        self.group(7, "");
        self.group(75, "16");
        self.group(76, "0");
        self.group(77, "2");
        self.group(78, "300");
        self.real(147, 1.0);
        self.real(148, 0.0);
        self.real(149, 0.0);
        self.group(100, "AcDbLayout");
        self.group(1, name);
        self.group(70, "1");
        self.group(71, if model { "0" } else { "1" });
        self.real(10, 0.0);
        self.real(20, 0.0);
        self.real(11, 420.0);
        self.real(21, 297.0);
        self.point(12, Point2::ORIGIN);
        // Extents not yet computed, as every writer leaves them.
        for code in [14, 24, 34] {
            self.real(code, 1e20);
        }
        for code in [15, 25, 35] {
            self.real(code, -1e20);
        }
        self.real(146, 0.0);
        self.point(13, Point2::ORIGIN);
        self.point(16, Point2::new(1.0, 0.0));
        self.point(17, Point2::new(0.0, 1.0));
        self.group(76, "1");
        self.handle(330, space);
    }
}

/// Group 6 for a linetype: `BYLAYER`, `BYBLOCK`, or the row's name.
fn linetype_name(linetype: Linetype, table: &LinetypeTable) -> String {
    match linetype {
        Linetype::ByLayer => LINETYPE_BY_LAYER.to_owned(),
        Linetype::ByBlock => LINETYPE_BY_BLOCK.to_owned(),
        Linetype::Named(id) => table.get(id).map_or_else(
            || LinetypeDef::CONTINUOUS.to_owned(),
            |def| def.name.clone(),
        ),
    }
}

/// A paragraph's words as MTEXT codes: a line break is `\P`, and the
/// characters MTEXT reads as the start of a code are escaped.
fn mtext_codes(content: &str) -> String {
    let mut out = String::with_capacity(content.len());
    for c in content.chars() {
        match c {
            '\n' => out.push_str("\\P"),
            '\\' => out.push_str("\\\\"),
            '{' => out.push_str("\\{"),
            '}' => out.push_str("\\}"),
            other => out.push(other),
        }
    }
    out
}

/// The `$INSUNITS` code of a unit.
pub(crate) const fn insunits_code(unit: LengthUnit) -> i64 {
    match unit {
        LengthUnit::Inches => 1,
        LengthUnit::Feet => 2,
        LengthUnit::Millimetres => 4,
        LengthUnit::Centimetres => 5,
        LengthUnit::Metres => 6,
    }
}

/// The shortest decimal that parses back to exactly `value`, always with a
/// decimal point so readers treat it as a real.
fn format_real(value: f64) -> String {
    let mut text = value.to_string();
    if !text.contains('.') {
        text.push_str(".0");
    }
    text
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use cad_core::Point2;
    use cad_document::{Block, Layer, LayerId, LinetypeId, TextStyleId};
    use proptest::prelude::*;

    use super::*;

    fn line(x0: f64, y0: f64, x1: f64, y1: f64) -> Entity {
        Entity::Line(LineEntity::new(Point2::new(x0, y0), Point2::new(x1, y1)))
    }

    // ── reading the output back as groups ──

    type Record<'a> = Vec<(i32, &'a str)>;

    fn groups(text: &str) -> Vec<(i32, &str)> {
        let lines: Vec<&str> = text.lines().collect();
        lines
            .chunks(2)
            .map(|pair| (pair[0].trim().parse().unwrap(), pair[1]))
            .collect()
    }

    /// The records of one type, each from its `0` group to the next.
    fn records<'a>(text: &'a str, kind: &str) -> Vec<Record<'a>> {
        let mut out = Vec::new();
        let mut current: Option<Record<'a>> = None;
        for (code, value) in groups(text) {
            if code == 0 {
                if let Some(record) = current.take() {
                    out.push(record);
                }
                if value == kind {
                    current = Some(Vec::new());
                }
            } else if let Some(record) = &mut current {
                record.push((code, value));
            }
        }
        out.extend(current);
        out
    }

    fn value<'a>(record: &Record<'a>, code: i32) -> Option<&'a str> {
        record.iter().find(|(c, _)| *c == code).map(|(_, v)| *v)
    }

    fn values<'a>(record: &Record<'a>, code: i32) -> Vec<&'a str> {
        record
            .iter()
            .filter(|(c, _)| *c == code)
            .map(|(_, v)| *v)
            .collect()
    }

    /// The text of one section.
    fn section<'a>(text: &'a str, name: &str) -> &'a str {
        let start = text
            .find(&format!("  0\nSECTION\n  2\n{name}\n"))
            .unwrap_or_else(|| panic!("no {name} section"));
        let rest = &text[start..];
        let end = rest.find("  0\nENDSEC\n").unwrap();
        &rest[..end]
    }

    /// The record of `kind` whose group 2 is `name`.
    fn named<'a>(text: &'a str, kind: &str, name: &str) -> Record<'a> {
        records(text, kind)
            .into_iter()
            .find(|record| value(record, 2) == Some(name))
            .unwrap_or_else(|| panic!("no {kind} {name}"))
    }

    #[test]
    fn an_empty_drawing_is_a_complete_file_with_its_permanent_rows() {
        let dxf = export(&Drawing::new());
        let sections: Vec<&str> = groups(&dxf)
            .windows(2)
            .filter(|pair| pair[0] == (0, "SECTION"))
            .map(|pair| pair[1].1)
            .collect();
        assert_eq!(
            sections,
            [
                "HEADER", "CLASSES", "TABLES", "BLOCKS", "ENTITIES", "OBJECTS"
            ]
        );
        assert!(dxf.ends_with("  0\nEOF\n"));
        let header = section(&dxf, "HEADER");
        assert!(header.contains("  9\n$ACADVER\n  1\nAC1015\n"));
        assert!(header.contains("  9\n$HANDSEED\n  5\n"));
        assert!(!header.contains("$EXTMIN"), "nothing to have extents");

        let tables: Vec<&str> = records(&dxf, "TABLE")
            .iter()
            .map(|table| value(table, 2).unwrap())
            .collect();
        assert_eq!(
            tables,
            [
                "VPORT",
                "LTYPE",
                "LAYER",
                "STYLE",
                "VIEW",
                "UCS",
                "APPID",
                "DIMSTYLE",
                "BLOCK_RECORD"
            ]
        );
        let names = |kind: &str| -> Vec<&str> {
            records(&dxf, kind)
                .iter()
                .map(|row| value(row, 2).unwrap())
                .collect()
        };
        assert_eq!(names("LTYPE"), ["ByBlock", "ByLayer", "Continuous"]);
        assert_eq!(names("LAYER"), ["0"]);
        assert_eq!(names("STYLE"), ["Standard"]);
        assert_eq!(names("BLOCK_RECORD"), ["*Model_Space", "*Paper_Space"]);
        assert_eq!(names("BLOCK"), ["*Model_Space", "*Paper_Space"]);
        assert_eq!(value(&named(&dxf, "TABLE", "LTYPE"), 70), Some("3"));
        assert!(section(&dxf, "ENTITIES").ends_with("ENTITIES\n"));

        // Every handle is unique and every owner is a handle, or none. (The
        // header's one group 5 is the seed, not a handle.)
        let all = groups(&dxf[dxf.find("  0\nSECTION\n  2\nCLASSES\n").unwrap()..]);
        let handles: Vec<&str> = all
            .iter()
            .filter(|(code, _)| *code == 5)
            .map(|(_, v)| *v)
            .collect();
        let unique: HashSet<&str> = handles.iter().copied().collect();
        assert_eq!(handles.len(), unique.len());
        for (code, owner) in &all {
            if matches!(code, 330 | 340 | 350 | 390) {
                assert!(*owner == "0" || unique.contains(owner), "owner {owner}");
            }
        }
        let seed = value(&groups(section(&dxf, "HEADER")), 5).unwrap();
        assert!(!unique.contains(seed), "the seed is the first free handle");
        // The root dictionary names the groups, layouts, and plot styles;
        // the layouts dictionary names one layout per space.
        let dictionaries = records(&dxf, "DICTIONARY");
        assert_eq!(
            values(&dictionaries[0], 3),
            ["ACAD_GROUP", "ACAD_LAYOUT", "ACAD_PLOTSTYLENAME"]
        );
        assert_eq!(values(&dictionaries[2], 3), ["Layout1", "Model"]);
        let layouts = records(&dxf, "LAYOUT");
        assert_eq!(
            layouts
                .iter()
                // The first group 1 is the plot settings' style sheet, empty.
                .map(|l| *values(l, 1).last().unwrap())
                .collect::<Vec<_>>(),
            ["Model", "Layout1"]
        );
        let model = named(&dxf, "BLOCK_RECORD", "*Model_Space");
        // A layout's first 330 is its owner, the dictionary; its last names
        // its block record, which points back.
        assert_eq!(values(&layouts[0], 330).last(), value(&model, 5).as_ref());
        assert_eq!(value(&model, 340), value(&layouts[0], 5));
    }

    #[test]
    fn a_line_is_written_with_its_handle_owner_layer_and_coordinates() {
        let dxf = export(&Drawing::from_entities([line(0.0, 0.0, 100.0, 50.5)]));
        let lines = records(&dxf, "LINE");
        assert_eq!(lines.len(), 1);
        let codes: Vec<i32> = lines[0].iter().map(|(code, _)| *code).collect();
        assert_eq!(codes, [5, 330, 100, 8, 100, 10, 20, 30, 11, 21, 31]);
        assert_eq!(value(&lines[0], 8), Some("0"));
        assert_eq!(values(&lines[0], 100), ["AcDbEntity", "AcDbLine"]);
        assert_eq!(
            (
                value(&lines[0], 10),
                value(&lines[0], 20),
                value(&lines[0], 11),
                value(&lines[0], 21)
            ),
            (Some("0.0"), Some("0.0"), Some("100.0"), Some("50.5"))
        );
        // Its owner is model space's block record.
        let model = named(&dxf, "BLOCK_RECORD", "*Model_Space");
        assert_eq!(value(&lines[0], 330), value(&model, 5));
        // And the header has the extents of the one line.
        let header = section(&dxf, "HEADER");
        assert!(header.contains("  9\n$EXTMIN\n 10\n0.0\n 20\n0.0\n 30\n0.0\n"));
        assert!(header.contains("  9\n$EXTMAX\n 10\n100.0\n 20\n50.5\n 30\n0.0\n"));
    }

    #[test]
    fn circles_and_arcs_are_written_with_angles_in_degrees() {
        use std::f64::consts::{FRAC_PI_2, PI};
        let dxf = export(&Drawing::from_entities([
            Entity::Circle(CircleEntity::new(Point2::new(1.0, 2.0), 3.5)),
            Entity::Arc(ArcEntity::new(Point2::new(0.0, 0.0), 10.0, FRAC_PI_2, PI)),
        ]));
        let circle = &records(&dxf, "CIRCLE")[0];
        assert_eq!(
            (value(circle, 10), value(circle, 20), value(circle, 40)),
            (Some("1.0"), Some("2.0"), Some("3.5"))
        );
        let arc = &records(&dxf, "ARC")[0];
        assert_eq!(values(arc, 100), ["AcDbEntity", "AcDbCircle", "AcDbArc"]);
        assert_eq!(
            (value(arc, 40), value(arc, 50), value(arc, 51)),
            (Some("10.0"), Some("90.0"), Some("180.0"))
        );
    }

    /// #115: a bulge is written after the vertex that starts the arc
    /// segment, and nowhere else.
    #[test]
    fn bulges_are_written_as_group_42_after_their_vertices() {
        let slot = PolylineEntity::with_bulges(
            vec![
                Point2::new(0.0, 0.0),
                Point2::new(10.0, 0.0),
                Point2::new(10.0, 4.0),
                Point2::new(0.0, 4.0),
            ],
            vec![0.0, 1.0, 0.0, -0.5],
            true,
        );
        let dxf = export(&Drawing::from_entities([Entity::Polyline(slot)]));
        let polyline = &records(&dxf, "LWPOLYLINE")[0];
        let data: Vec<(i32, &str)> = polyline
            .iter()
            .copied()
            .filter(|(code, _)| matches!(code, 10 | 20 | 42 | 70 | 90))
            .collect();
        assert_eq!(
            data,
            [
                (90, "4"),
                (70, "1"),
                (10, "0.0"),
                (20, "0.0"),
                (10, "10.0"),
                (20, "0.0"),
                (42, "1.0"),
                (10, "10.0"),
                (20, "4.0"),
                (10, "0.0"),
                (20, "4.0"),
                (42, "-0.5"),
            ]
        );
    }

    #[test]
    fn a_polyline_is_written_as_an_lwpolyline() {
        let triangle = PolylineEntity::new(
            vec![
                Point2::new(0.0, 0.0),
                Point2::new(4.0, 0.0),
                Point2::new(4.0, 3.0),
            ],
            false,
        );
        let dxf = export(&Drawing::from_entities([Entity::Polyline(triangle)]));
        assert!(records(&dxf, "POLYLINE").is_empty() && records(&dxf, "VERTEX").is_empty());
        let polyline = &records(&dxf, "LWPOLYLINE")[0];
        assert_eq!(values(polyline, 100), ["AcDbEntity", "AcDbPolyline"]);
        assert_eq!(
            (value(polyline, 90), value(polyline, 70)),
            (Some("3"), Some("0"))
        );
        assert_eq!(values(polyline, 10), ["0.0", "4.0", "4.0"]);
        assert_eq!(values(polyline, 20), ["0.0", "0.0", "3.0"]);
    }

    #[test]
    fn output_is_deterministic_and_ordered_by_entity_id() {
        let mut drawing = Drawing::new();
        let mut ids = Vec::new();
        for x in [1.0, 2.0, 3.0, 4.0] {
            let mut tx = drawing.transaction();
            ids.push(
                tx.insert(LineEntity::new(Point2::new(x, 0.0), Point2::new(x, 1.0)))
                    .unwrap(),
            );
            tx.commit();
        }
        // Removing from the middle reorders dense storage; export must not care.
        let mut tx = drawing.transaction();
        tx.remove(ids[1]).unwrap();
        tx.commit();

        let dxf = export(&drawing);
        let lines = records(section(&dxf, "ENTITIES"), "LINE");
        let xs: Vec<&str> = lines.iter().map(|l| value(l, 10).unwrap()).collect();
        assert_eq!(xs, ["1.0", "3.0", "4.0"]);
        // Handles follow: one more each time.
        let handles: Vec<u64> = lines
            .iter()
            .map(|l| u64::from_str_radix(value(l, 5).unwrap(), 16).unwrap())
            .collect();
        assert_eq!(handles[1], handles[0] + 1);
        assert_eq!(handles[2], handles[0] + 2);
        assert_eq!(export(&drawing), dxf);
    }

    #[test]
    fn reals_always_carry_a_decimal_point() {
        assert_eq!(format_real(100.0), "100.0");
        assert_eq!(format_real(-0.5), "-0.5");
        assert_eq!(format_real(707.1067811865476), "707.1067811865476");
        assert_eq!(format_real(1e21), "1000000000000000000000.0");
    }

    #[test]
    fn the_viewport_looks_at_the_whole_drawing() {
        let dxf = export(&Drawing::from_entities([line(-100.0, 0.0, 100.0, 50.0)]));
        let viewport = named(&dxf, "VPORT", "*Active");
        assert_eq!(
            (value(&viewport, 12), value(&viewport, 22)),
            (Some("0.0"), Some("25.0"))
        );
        // 200 across at an aspect of 1.5 wants 133.3 of height; with margin.
        let height: f64 = value(&viewport, 40).unwrap().parse().unwrap();
        assert!((height - 200.0 / VIEW_ASPECT * 1.1).abs() < 1e-9);
        let empty = export(&Drawing::new());
        assert_eq!(
            value(&named(&empty, "VPORT", "*Active"), 40),
            Some("1000.0")
        );
    }

    // ── layers, colours, linetypes, lineweights (#139, #152) ──

    /// Layers `walls` (dashed, colour 1, locked) and `hidden` (off, a
    /// lineweight), the linetypes Dashed and Dot, and lines with every
    /// kind of property.
    fn styled() -> Drawing {
        let (walls, hidden) = (LayerId::from_raw(1), LayerId::from_raw(2));
        let (dashed, dot) = (LinetypeId::from_raw(1), LinetypeId::from_raw(2));
        let layers = [
            Layer {
                color: Color::Index(1),
                linetype: Linetype::Named(dashed),
                locked: true,
                ..Layer::named("walls")
            },
            Layer {
                on: false,
                lineweight: Lineweight::Hundredths(50),
                ..Layer::named("hidden")
            },
        ];
        let linetypes = [
            LinetypeDef::built_in_named("Dashed").unwrap(),
            LinetypeDef::built_in_named("Dot").unwrap(),
        ];
        let at = |y: f64| Entity::Line(LineEntity::new(Point2::new(0.0, y), Point2::new(10.0, y)));
        let entities = [
            (at(0.0), Properties::default()),
            (at(1.0), Properties::on(walls)),
            (
                at(2.0),
                Properties {
                    color: Color::Index(5),
                    linetype: Linetype::Named(dot),
                    lineweight: Lineweight::Hundredths(35),
                    ..Properties::on(hidden)
                },
            ),
            (
                at(3.0),
                Properties {
                    color: Color::ByBlock,
                    linetype: Linetype::ByBlock,
                    lineweight: Lineweight::ByBlock,
                    ..Properties::default()
                },
            ),
            (
                at(4.0),
                Properties {
                    linetype: Linetype::CONTINUOUS,
                    lineweight: Lineweight::Default,
                    ..Properties::on(walls)
                },
            ),
        ];
        Drawing::from_parts(layers, linetypes, entities)
            .unwrap()
            .with_settings(DrawingSettings {
                current_layer: walls,
                current_color: Color::Index(3),
                current_linetype: Linetype::Named(dot),
                current_lineweight: Lineweight::Hundredths(25),
                current_dim_style: cad_document::DimStyleId::FIRST,
                ltscale: 20.0,
                unit: None,
            })
            .unwrap()
    }

    #[test]
    fn the_tables_are_written_with_patterns_switches_lineweights_and_linetypes() {
        let dxf = export(&styled());
        let dashed = named(&dxf, "LTYPE", "Dashed");
        assert_eq!(value(&dashed, 3), Some("__ __ __ __ __ __ __"));
        assert_eq!(
            (value(&dashed, 72), value(&dashed, 73)),
            (Some("65"), Some("2"))
        );
        assert_eq!(value(&dashed, 40), Some("0.75"));
        assert_eq!(values(&dashed, 49), ["0.5", "-0.25"]);
        assert_eq!(values(&dashed, 74), ["0", "0"], "a plain dash per element");
        assert_eq!(values(&named(&dxf, "LTYPE", "Dot"), 49), ["0.0", "-0.25"]);
        assert_eq!(value(&named(&dxf, "TABLE", "LTYPE"), 70), Some("5"));

        let walls = named(&dxf, "LAYER", "walls");
        assert_eq!(value(&walls, 70), Some("4"), "locked");
        assert_eq!(value(&walls, 62), Some("1"));
        assert_eq!(value(&walls, 6), Some("Dashed"));
        assert_eq!(value(&walls, 370), Some("-3"), "the default width");
        let hidden = named(&dxf, "LAYER", "hidden");
        assert_eq!(value(&hidden, 62), Some("-7"), "off: a negative colour");
        assert_eq!(value(&hidden, 370), Some("50"));
        let zero = named(&dxf, "LAYER", "0");
        assert_eq!(value(&zero, 6), Some("Continuous"));
        // Every layer shares the one plot style.
        let placeholder = &records(&dxf, "ACDBPLACEHOLDER")[0];
        assert_eq!(value(&walls, 390), value(placeholder, 5));
    }

    #[test]
    fn an_entity_carries_its_layer_and_its_own_colour_linetype_and_lineweight_only() {
        let dxf = export(&styled());
        let own: Vec<Vec<(i32, &str)>> = records(section(&dxf, "ENTITIES"), "LINE")
            .iter()
            .map(|line| {
                line.iter()
                    .copied()
                    .filter(|(code, _)| matches!(code, 8 | 6 | 62 | 370))
                    .collect()
            })
            .collect();
        assert_eq!(
            own,
            vec![
                vec![(8, "0")],
                vec![(8, "walls")],
                vec![(8, "hidden"), (6, "Dot"), (62, "5"), (370, "35")],
                vec![(8, "0"), (6, "BYBLOCK"), (62, "0"), (370, "-2")],
                vec![(8, "walls"), (6, "Continuous"), (370, "-3")],
            ]
        );
    }

    #[test]
    fn the_header_carries_the_settings_that_are_not_a_new_drawings() {
        let header = section(&export(&styled()), "HEADER").to_owned();
        assert!(header.contains("  9\n$CLAYER\n  8\nwalls\n"));
        assert!(header.contains("  9\n$CECOLOR\n 62\n3\n"));
        assert!(header.contains("  9\n$CELTYPE\n  6\nDot\n"));
        assert!(header.contains("  9\n$CELWEIGHT\n370\n25\n"));
        assert!(header.contains("  9\n$LTSCALE\n 40\n20.0\n"));
        assert!(!header.contains("$INSUNITS"));

        let header = section(&export(&Drawing::new()), "HEADER").to_owned();
        for variable in ["$CLAYER", "$CECOLOR", "$CELTYPE", "$CELWEIGHT", "$LTSCALE"] {
            assert!(!header.contains(variable), "{variable}");
        }
    }

    #[test]
    fn everything_the_drawing_holds_survives_export_import_export_byte_for_byte() {
        let first = export(&styled());
        let back = crate::import(first.as_bytes()).unwrap();
        assert_eq!(back.report.layers, 2);
        assert_eq!(back.report.linetypes, 2);
        assert_eq!(
            back.report.created_layers + back.report.created_linetypes,
            0
        );
        assert!(!back.report.has_unsupported_content());
        let drawing = &back.drawing;
        let walls = drawing.layers().find("walls").unwrap();
        let walls_row = drawing.layers().get(walls).unwrap();
        assert!(walls_row.locked && walls_row.color == Color::Index(1));
        let hidden = drawing.layers().find("hidden").unwrap();
        assert_eq!(
            drawing.layers().get(hidden).unwrap().lineweight,
            Lineweight::Hundredths(50),
            "a layer's lineweight is written now"
        );
        let settings = drawing.settings();
        assert_eq!(settings.current_layer, walls);
        assert_eq!(settings.current_color, Color::Index(3));
        assert_eq!(settings.current_lineweight, Lineweight::Hundredths(25));
        assert_eq!(settings.ltscale, 20.0);
        let lineweights: Vec<Lineweight> = {
            let mut records: Vec<_> = drawing.records().collect();
            records.sort_by_key(|(id, _)| *id);
            records
                .iter()
                .map(|(_, record)| record.properties.lineweight)
                .collect()
        };
        assert_eq!(
            lineweights,
            [
                Lineweight::ByLayer,
                Lineweight::ByLayer,
                Lineweight::Hundredths(35),
                Lineweight::ByBlock,
                Lineweight::Default
            ]
        );
        assert_eq!(export(drawing), first);
    }

    #[test]
    fn blocks_are_written_as_records_definitions_and_inserts_and_survive_a_round_trip() {
        let mut drawing = Drawing::new();
        let mut tx = drawing.transaction();
        let chair = tx
            .define_block(Block::new(
                "Chair",
                Point2::new(1.0, 2.0),
                vec![EntityRecord::new(
                    LineEntity::new(Point2::ORIGIN, Point2::new(10.0, 0.0)),
                    Properties {
                        color: Color::ByBlock,
                        ..Properties::default()
                    },
                )],
            ))
            .unwrap();
        let pair = tx
            .define_block(Block::new(
                "Pair",
                Point2::ORIGIN,
                vec![
                    EntityRecord::new(
                        BlockReferenceEntity::new(chair, Point2::ORIGIN, 1.0, 0.0),
                        Properties::default(),
                    ),
                    EntityRecord::new(
                        BlockReferenceEntity::new(chair, Point2::new(20.0, 0.0), 1.0, 0.0),
                        Properties::default(),
                    ),
                ],
            ))
            .unwrap();
        tx.insert(BlockReferenceEntity::new(
            pair,
            Point2::new(100.0, 0.0),
            1.0,
            0.0,
        ))
        .unwrap();
        // Mirrored, twice the size, a quarter turn, in red.
        tx.insert_with(
            BlockReferenceEntity {
                mirrored: true,
                ..BlockReferenceEntity::new(
                    chair,
                    Point2::new(0.0, 50.0),
                    2.0,
                    std::f64::consts::FRAC_PI_2,
                )
            },
            Properties {
                color: Color::Index(1),
                ..Properties::default()
            },
        )
        .unwrap();
        tx.commit();

        let first = export(&drawing);
        let names: Vec<&str> = records(&first, "BLOCK_RECORD")
            .iter()
            .map(|row| value(row, 2).unwrap())
            .collect();
        assert_eq!(names, ["*Model_Space", "*Paper_Space", "Chair", "Pair"]);
        let chair_record = named(&first, "BLOCK_RECORD", "Chair");
        let chair_block = named(&first, "BLOCK", "Chair");
        assert_eq!(
            value(&chair_block, 330),
            value(&chair_record, 5),
            "the block names its record"
        );
        assert_eq!(
            (value(&chair_block, 10), value(&chair_block, 20)),
            (Some("1.0"), Some("2.0"))
        );
        // The chair's line is owned by the chair's record; the pair holds two inserts.
        let blocks = section(&first, "BLOCKS");
        let seat = &records(blocks, "LINE")[0];
        assert_eq!(value(seat, 330), value(&chair_record, 5));
        assert_eq!(value(seat, 62), Some("0"));
        let inserts = records(blocks, "INSERT");
        assert_eq!(inserts.len(), 2);
        assert_eq!(value(&inserts[1], 10), Some("20.0"));
        // The mirrored reference in the model: scales, angle, colour.
        let placed = records(section(&first, "ENTITIES"), "INSERT");
        assert_eq!(placed.len(), 2);
        let mirrored = &placed[1];
        assert_eq!(values(mirrored, 100), ["AcDbEntity", "AcDbBlockReference"]);
        assert_eq!(
            (
                value(mirrored, 41),
                value(mirrored, 42),
                value(mirrored, 43),
                value(mirrored, 50)
            ),
            (Some("2.0"), Some("-2.0"), Some("2.0"), Some("90.0"))
        );
        assert_eq!(value(mirrored, 62), Some("1"));
        assert!(
            value(&placed[0], 41).is_none(),
            "a scale of one is not written"
        );

        let back = crate::import(first.as_bytes()).unwrap();
        assert!(!back.report.has_unsupported_content());
        assert_eq!((back.report.blocks, back.report.block_references), (2, 2));
        let mirrored = back
            .drawing
            .entities()
            .filter_map(|(_, entity)| match entity {
                Entity::BlockRef(reference) if reference.mirrored => Some(reference.clone()),
                _ => None,
            })
            .next()
            .expect("the mirrored chair");
        assert_eq!(mirrored.scale, 2.0);
        assert!((mirrored.rotation - std::f64::consts::FRAC_PI_2).abs() < 1e-12);
        assert_eq!(export(&back.drawing), first, "byte-identical");
    }

    #[test]
    fn a_text_is_written_with_its_insertion_height_words_angle_and_justification() {
        let plain = TextEntity::new(Point2::new(1.0, 2.0), 2.5, 0.0, "Hello world");
        let turned = TextEntity::new(
            Point2::new(10.0, 0.0),
            5.0,
            std::f64::consts::FRAC_PI_2,
            "Up",
        )
        .justified(Justification::Center);
        let right = TextEntity::new(Point2::new(50.0, 0.0), 5.0, 0.0, "End")
            .justified(Justification::Right);
        let start = right.start();
        let dxf = export(&Drawing::from_entities([
            Entity::Text(plain),
            Entity::Text(turned.clone()),
            Entity::Text(right),
        ]));
        let texts = records(&dxf, "TEXT");
        assert_eq!(texts.len(), 3);
        assert_eq!(
            values(&texts[0], 100),
            ["AcDbEntity", "AcDbText", "AcDbText"]
        );
        assert_eq!(
            (
                value(&texts[0], 10),
                value(&texts[0], 20),
                value(&texts[0], 40),
                value(&texts[0], 1)
            ),
            (Some("1.0"), Some("2.0"), Some("2.5"), Some("Hello world"))
        );
        assert!(value(&texts[0], 72).is_none() && value(&texts[0], 7).is_none());
        // A centred text: the alignment point is the insertion, group 72 is 1,
        // and the first point is where the baseline starts.
        let centred_start = turned.start();
        assert_eq!(
            value(&texts[1], 10),
            Some(format_real(centred_start.x).as_str())
        );
        assert_eq!(
            value(&texts[1], 20),
            Some(format_real(centred_start.y).as_str())
        );
        assert_eq!(
            (
                value(&texts[1], 50),
                value(&texts[1], 72),
                value(&texts[1], 11),
                value(&texts[1], 21)
            ),
            (Some("90.0"), Some("1"), Some("10.0"), Some("0.0"))
        );
        assert_eq!(value(&texts[2], 10), Some(format_real(start.x).as_str()));
        assert_eq!(
            (value(&texts[2], 72), value(&texts[2], 11)),
            (Some("2"), Some("50.0"))
        );
    }

    #[test]
    fn a_paragraph_is_written_as_mtext_with_breaks_and_codes_escaped_and_survives() {
        let long: String = (0..300)
            .map(|i| char::from(b'a' + (i % 26) as u8))
            .collect();
        let mut drawing = Drawing::new();
        let mut tx = drawing.transaction();
        tx.insert(MTextEntity::new(
            Point2::new(0.0, 10.0),
            40.0,
            2.5,
            0.3,
            "Site plan\nscale 1:50 {a\\b}",
        ))
        .unwrap();
        tx.insert(MTextEntity::new(
            Point2::ORIGIN,
            400.0,
            2.5,
            0.0,
            long.clone(),
        ))
        .unwrap();
        tx.commit();

        let dxf = export(&drawing);
        let texts = records(&dxf, "MTEXT");
        assert_eq!(texts.len(), 2);
        assert_eq!(values(&texts[0], 100), ["AcDbEntity", "AcDbMText"]);
        assert_eq!(
            (
                value(&texts[0], 10),
                value(&texts[0], 20),
                value(&texts[0], 40),
                value(&texts[0], 41),
                value(&texts[0], 71)
            ),
            (
                Some("0.0"),
                Some("10.0"),
                Some("2.5"),
                Some("40.0"),
                Some("1")
            )
        );
        assert_eq!(
            value(&texts[0], 1),
            Some("Site plan\\Pscale 1:50 \\{a\\\\b\\}")
        );
        assert_eq!(
            value(&texts[0], 50),
            Some(format_real(0.3f64.to_degrees()).as_str())
        );
        // Three hundred characters: 250 in group 3, the rest in group 1.
        assert_eq!(values(&texts[1], 3), [&long[..250]]);
        assert_eq!(value(&texts[1], 1), Some(&long[250..]));
        assert!(value(&texts[1], 50).is_none());

        let back = crate::import(dxf.as_bytes()).unwrap();
        assert!(!back.report.has_unsupported_content());
        assert_eq!(back.report.formatting_dropped, 0);
        let paragraphs: Vec<&MTextEntity> = back
            .drawing
            .entities()
            .filter_map(|(_, entity)| match entity {
                Entity::MText(text) => Some(text),
                _ => None,
            })
            .collect();
        assert_eq!(paragraphs.len(), 2);
        assert!(
            paragraphs
                .iter()
                .any(|text| text.content == "Site plan\nscale 1:50 {a\\b}" && text.width == 40.0)
        );
        assert!(paragraphs.iter().any(|text| text.content == long));
        assert_eq!(export(&back.drawing), dxf, "byte-identical");
    }

    #[test]
    fn a_text_style_other_than_standard_is_named_on_its_texts() {
        let roman = TextStyleId::from_raw(1);
        let drawing = Drawing::from_tables(
            [],
            [],
            [TextStyle::named("Roman")],
            [],
            [],
            [
                (
                    Entity::Text(TextEntity {
                        style: roman,
                        ..TextEntity::new(Point2::ORIGIN, 2.5, 0.0, "Hi")
                    }),
                    Properties::default(),
                ),
                (
                    Entity::MText(MTextEntity {
                        style: roman,
                        ..MTextEntity::new(Point2::ORIGIN, 40.0, 2.5, 0.0, "Para")
                    }),
                    Properties::default(),
                ),
            ],
        )
        .unwrap();
        let dxf = export(&drawing);
        assert_eq!(value(&records(&dxf, "TEXT")[0], 7), Some("Roman"));
        assert_eq!(value(&records(&dxf, "MTEXT")[0], 7), Some("Roman"));
        assert_eq!(value(&named(&dxf, "STYLE", "Roman"), 3), Some("txt"));
    }

    proptest! {
        #[test]
        fn formatted_reals_parse_back_exactly(value in proptest::num::f64::NORMAL | proptest::num::f64::ZERO) {
            prop_assert_eq!(format_real(value).parse::<f64>().unwrap(), value);
        }
    }
}

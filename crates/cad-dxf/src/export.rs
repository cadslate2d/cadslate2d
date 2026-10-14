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

use std::collections::HashMap;
use std::fmt::Write;

use cad_core::{Aabb, EntityId, LengthUnit, Point2, Vec2};
use cad_document::{
    ArcEntity, AttributeDefinitionEntity, BlockId, BlockReferenceEntity, BlockTable, CircleEntity,
    Color, DimStyle, DimStyleId, DimensionEntity, Drawing, DrawingSettings, EllipseEntity, Entity,
    EntityRecord, HatchEntity, HatchStyle, Justification, Layer, Layout, LayoutArea, LayoutId,
    LayoutPlot, LayoutTable, LeaderEntity, LineEntity, Linetype, LinetypeDef, LinetypeTable,
    Lineweight, MTextEntity, Measure, PointEntity, PolylineEntity, Properties, Revision,
    SplineEntity, TextEntity, TextStyle, TextStyleTable, ViewportEntity,
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

    /// The handle taken last: the record being written.
    fn last(&self) -> Handle {
        Handle(self.0 - 1)
    }
}

/// A layout's records in the file being written (#184, #191): its block
/// record, its LAYOUT object, the name of its paper block, and one
/// handle per record kept on it.
struct SheetHandles {
    id: LayoutId,
    record: Handle,
    layout: Handle,
    block_name: String,
    kept: Vec<Handle>,
}

/// Group codes that hold a handle of another record: pointers and
/// owners. A kept record's are mapped to the file being written, or to
/// nothing when what they named was not kept.
fn is_handle_reference(code: i32) -> bool {
    (330..=369).contains(&code)
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
    /// The layouts' records, in tab order; when there are any, they stand
    /// in for the one `Layout1` of a drawing with none.
    sheets: Vec<SheetHandles>,
    /// Handles the file's kept records had, uppercase, to the handles
    /// they have now.
    remap: HashMap<String, Handle>,
}

impl Structure {
    fn allocate(handles: &mut Handles, layouts: &LayoutTable) -> Self {
        let mut structure = Self {
            root: handles.take(),
            groups: handles.take(),
            layouts: handles.take(),
            plot_styles: handles.take(),
            plot_style: handles.take(),
            model_layout: handles.take(),
            paper_layout: handles.take(),
            model_space: handles.take(),
            paper_space: handles.take(),
            sheets: Vec::new(),
            remap: HashMap::new(),
        };
        // A layout's paper block keeps the name the file gave it; one
        // made here takes the next name a writer would give it.
        let mut block_names: Vec<String> = layouts
            .iter()
            .filter(|(id, _)| *id != LayoutId::MODEL)
            .map(|(_, layout)| layout.block_name.to_uppercase())
            .filter(|name| !name.is_empty())
            .collect();
        let sheets = layouts.iter().filter(|(id, _)| *id != LayoutId::MODEL);
        for (index, (id, layout)) in sheets.enumerate() {
            // The first layout takes the handles a drawing without any
            // gives its one `Layout1`, so a file of ours read back writes
            // the same bytes again.
            let (record, layout_handle) = if index == 0 {
                (structure.paper_space, structure.paper_layout)
            } else {
                (handles.take(), handles.take())
            };
            let block_name = if layout.block_name.is_empty() {
                let name = std::iter::once("*Paper_Space".to_owned())
                    .chain((0..).map(|n| format!("*Paper_Space{n}")))
                    .find(|name| !block_names.contains(&name.to_uppercase()))
                    .expect("some name is free");
                block_names.push(name.to_uppercase());
                name
            } else {
                layout.block_name.clone()
            };
            let kept: Vec<Handle> = layout
                .kept_entities
                .iter()
                .map(|_| handles.take())
                .collect();
            if !layout.block_record_handle.is_empty() {
                structure
                    .remap
                    .insert(layout.block_record_handle.clone(), record);
            }
            if let Some((_, old)) = layout.kept.iter().find(|(code, _)| *code == 5) {
                structure
                    .remap
                    .insert(old.trim().to_uppercase(), layout_handle);
            }
            for (entity, handle) in layout.kept_entities.iter().zip(&kept) {
                if let Some((_, old)) = entity.groups.iter().find(|(code, _)| *code == 5) {
                    structure.remap.insert(old.trim().to_uppercase(), *handle);
                }
            }
            structure.sheets.push(SheetHandles {
                id,
                record,
                layout: layout_handle,
                block_name,
                kept,
            });
        }
        structure
    }
}

/// The entities of `space`, in ID order: how a space's records are written.
fn records_in(drawing: &Drawing, space: LayoutId) -> Vec<EntityId> {
    let mut ids: Vec<EntityId> = drawing
        .records()
        .filter(|(_, record)| record.space == space)
        .map(|(id, _)| id)
        .collect();
    ids.sort_unstable();
    ids
}

/// Serialises the drawing as ASCII DXF.
///
/// Output is deterministic: entities are written in `EntityId` order with
/// handles in that order, and coordinates use the shortest decimal form
/// that reads back to exactly the same `f64`.
pub fn export(drawing: &Drawing) -> String {
    Export::begin(drawing)
        .finish(drawing)
        .expect("a borrowed drawing cannot change")
}

/// The drawing changed while an export in slices was under way: what was
/// written no longer describes it, and the job is over.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DrawingChanged;

/// Where an export in slices has got to.
enum Phase {
    /// The next dimension's anonymous block to write, in BLOCKS.
    DimensionBlocks(usize),
    /// The next record to write, in ENTITIES.
    Entities(usize),
    Done,
}

/// An export in slices (#178, ADR-0024): begun at one revision of the
/// drawing, it writes as many records per step as it is given, so an
/// autosave can share the main thread with the frames between steps. A
/// change to the drawing meanwhile ends it. Stepped to the end in one go,
/// it is [`export`].
pub struct Export {
    body: Writer,
    revision: Revision,
    structure: Structure,
    dimensions: Vec<(DimensionEntity, Properties)>,
    dimension_records: Vec<Handle>,
    /// Every entity's ID, in the order their records are written.
    ids: Vec<EntityId>,
    phase: Phase,
}

impl std::fmt::Debug for Export {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let (phase, at) = match self.phase {
            Phase::DimensionBlocks(n) => ("dimension blocks", n),
            Phase::Entities(n) => ("entities", n),
            Phase::Done => ("done", 0),
        };
        f.debug_struct("Export")
            .field("revision", &self.revision)
            .field("phase", &phase)
            .field("at", &at)
            .field("written", &self.body.out.len())
            .finish()
    }
}

impl Export {
    /// Writes everything before the first record: the classes, the tables,
    /// and the block definitions.
    pub fn begin(drawing: &Drawing) -> Self {
        let mut handles = Handles(1);
        let structure = Structure::allocate(&mut handles, drawing.layouts());
        // One block record per definition, in table order; the record is
        // what a definition's entities name as their owner.
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
            paper: false,
            viewports_written: 0,
            active_viewport: None,
            viewport_links: HashMap::new(),
        };
        body.group(0, "SECTION");
        body.group(2, "CLASSES");
        body.group(0, "ENDSEC");
        body.section("TABLES", |w| {
            w.tables(drawing, &structure, &block_records, &dimension_records);
        });
        body.group(0, "SECTION");
        body.group(2, "BLOCKS");
        body.blocks(drawing, &structure, &block_records);
        // The ENTITIES section is the model's; a layout's entities are in
        // its paper block (#191).
        let ids = records_in(drawing, LayoutId::MODEL);
        Self {
            body,
            revision: drawing.revision(),
            structure,
            dimensions,
            dimension_records,
            ids,
            phase: Phase::DimensionBlocks(0),
        }
    }

    /// Writes up to `budget` more records — a dimension's block counts as
    /// one — and says whether the file is complete.
    pub fn step(&mut self, drawing: &Drawing, budget: usize) -> Result<bool, DrawingChanged> {
        if drawing.revision() != self.revision {
            return Err(DrawingChanged);
        }
        let mut left = budget;
        loop {
            match self.phase {
                Phase::DimensionBlocks(n) if n < self.dimensions.len() => {
                    if left == 0 {
                        return Ok(false);
                    }
                    let (dimension, properties) = &self.dimensions[n];
                    self.body.dimension_block(
                        drawing,
                        n,
                        dimension,
                        *properties,
                        self.dimension_records[n],
                    );
                    self.phase = Phase::DimensionBlocks(n + 1);
                    left -= 1;
                }
                Phase::DimensionBlocks(_) => {
                    self.body.group(0, "ENDSEC");
                    self.body.group(0, "SECTION");
                    self.body.group(2, "ENTITIES");
                    self.phase = Phase::Entities(0);
                }
                Phase::Entities(n) if n < self.ids.len() => {
                    if left == 0 {
                        return Ok(false);
                    }
                    if let Some(record) = drawing.record(self.ids[n]) {
                        self.body
                            .record(record, self.structure.model_space, drawing);
                    }
                    self.phase = Phase::Entities(n + 1);
                    left -= 1;
                }
                Phase::Entities(_) => {
                    self.body.group(0, "ENDSEC");
                    let structure = &self.structure;
                    self.body
                        .section("OBJECTS", |w| w.objects(structure, drawing));
                    self.body.group(0, "EOF");
                    self.phase = Phase::Done;
                }
                Phase::Done => return Ok(true),
            }
        }
    }

    /// Writes whatever is left and returns the file: the header, which
    /// names the first free handle, in front of the body.
    pub fn finish(mut self, drawing: &Drawing) -> Result<String, DrawingChanged> {
        self.step(drawing, usize::MAX)?;
        let mut header = Writer {
            out: String::new(),
            handles: Handles(0),
            dimensions_named: 0,
            paper: false,
            viewports_written: 0,
            active_viewport: None,
            viewport_links: HashMap::new(),
        };
        header.section("HEADER", |w| w.header(drawing, self.body.handles.seed()));
        Ok(header.out + &self.body.out)
    }
}

struct Writer {
    out: String,
    handles: Handles,
    /// DIMENSION records written so far: the next one's block is `*D` and
    /// this number plus one, which is the order `dimension_blocks` writes
    /// them in.
    dimensions_named: usize,
    /// Inside a layout's paper block: every entity is flagged as paper (67).
    paper: bool,
    /// VIEWPORT records written in the current paper block: the next
    /// one's number (69), and the handle of the first, which the LAYOUT
    /// names as its active viewport (331).
    viewports_written: usize,
    active_viewport: Option<Handle>,
    /// The viewport a kept LAYOUT names as active (331), by the handle
    /// the file gave it, to the handle of the first viewport written in
    /// the layout's block (#192).
    viewport_links: HashMap<String, Handle>,
}

/// Every dimension, with its properties, in the order their DIMENSION
/// records are written: those inside block definitions first, in table
/// order, then each layout's in ID order, then the model's in ID order —
/// the order their records are written — so that the n-th DIMENSION
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
    let spaces = drawing
        .layouts()
        .iter()
        .map(|(id, _)| id)
        .filter(|id| *id != LayoutId::MODEL)
        .chain(std::iter::once(LayoutId::MODEL));
    for space in spaces {
        for id in records_in(drawing, space) {
            if let Some(record) = drawing.record(id) {
                take(record);
            }
        }
    }
    out
}

/// The name of the n-th dimension's anonymous block, counting from one.
fn dimension_block_name(n: usize) -> String {
    format!("*D{n}")
}

/// A kept LAYOUT's groups with the name the layout has now: the first
/// group 1 after the `AcDbLayout` marker.
/// The plot settings' groups for `plot` (#208): the rotation (73), the
/// plot type (74), the window's corners (48, 49, 140, 141), the scale
/// (142 over 143), and the flags (70, bit 16) with the standard scale (75)
/// that say a fit — the flags as `groups` had them otherwise.
fn plot_groups(plot: &LayoutPlot, groups: &[(i32, String)]) -> Vec<(i32, String)> {
    let flags = groups
        .iter()
        .find(|(code, _)| *code == 70)
        .and_then(|(_, value)| value.trim().parse::<i64>().ok())
        .unwrap_or(0);
    let (kind, window) = match plot.area {
        LayoutArea::Layout => (5, None),
        LayoutArea::Extents => (1, None),
        LayoutArea::Window(window) => (4, Some(window)),
    };
    let window = window.unwrap_or_else(|| Aabb::from_points(Point2::ORIGIN, Point2::ORIGIN));
    let (flags, standard, paper, drawing) = match plot.scale {
        None => (flags | 16, 0, 1.0, 1.0),
        Some(scale) => (flags & !16, 16, scale, 1.0),
    };
    vec![
        (48, format_real(window.min().x)),
        (49, format_real(window.min().y)),
        (140, format_real(window.max().x)),
        (141, format_real(window.max().y)),
        (142, format_real(paper)),
        (143, format_real(drawing)),
        (70, flags.to_string()),
        (73, plot.turns.to_string()),
        (74, kind.to_string()),
        (75, standard.to_string()),
    ]
}

fn layout_groups_as_now(groups: &[(i32, String)], layout: &Layout) -> Vec<(i32, String)> {
    let mut out = groups.to_vec();
    let start = out
        .iter()
        .position(|(code, value)| *code == 100 && value.trim() == "AcDbLayout")
        .unwrap_or(0);
    if let Some(slot) = out[start..].iter_mut().find(|(code, _)| *code == 1) {
        slot.1 = layout.name.clone();
    }
    // Plot settings set here (#208) replace the file's.
    if let Some(plot) = layout.plot.as_deref() {
        for (code, value) in plot_groups(plot, &out) {
            match out.iter_mut().find(|(c, _)| *c == code) {
                Some(slot) => slot.1 = value,
                None => out.push((code, value)),
            }
        }
    }
    // A sheet set here (#194) replaces the plot settings' paper size.
    if layout.paper_size.is_some() {
        let size = layout.sheet_size();
        for (code, value) in [(44, size.x), (45, size.y)] {
            match out.iter_mut().find(|(c, _)| *c == code) {
                Some(slot) => slot.1 = format_real(value),
                None => out.push((code, format_real(value))),
            }
        }
    }
    out
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
        // Attributes are shown as their definitions say (#183).
        self.group(9, "$ATTMODE");
        self.int(70, 1);
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
        // The point style (#198): the mode and the size, in drawing units.
        if settings.point_style.mode != defaults.point_style.mode {
            self.group(9, "$PDMODE");
            self.int(70, i64::from(settings.point_style.mode));
        }
        #[allow(clippy::float_cmp)]
        if settings.point_style.size != defaults.point_style.size {
            self.group(9, "$PDSIZE");
            self.real(40, settings.point_style.size);
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
        let sheets = &structure.sheets;
        let spaces = if sheets.is_empty() {
            2
        } else {
            1 + sheets.len()
        };
        let rows = blocks.len() - 1 + spaces + dimension_records.len();
        self.table("BLOCK_RECORD", rows, |w, table| {
            w.block_record(
                table,
                structure.model_space,
                "*Model_Space",
                structure.model_layout,
            );
            if sheets.is_empty() {
                w.block_record(
                    table,
                    structure.paper_space,
                    "*Paper_Space",
                    structure.paper_layout,
                );
            }
            for sheet in sheets {
                w.block_record(table, sheet.record, &sheet.block_name, sheet.layout);
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
        if structure.sheets.is_empty() {
            self.block(
                "*Paper_Space",
                Point2::ORIGIN,
                0,
                structure.paper_space,
                |_| {},
            );
        }
        // Each layout's paper block (#191): its entities, flagged as
        // paper (67), then the records kept on it as the file held them.
        for sheet in &structure.sheets {
            let Some(layout) = drawing.layouts().get(sheet.id) else {
                continue;
            };
            self.paper = true;
            self.viewports_written = 0;
            self.active_viewport = None;
            self.block(&sheet.block_name, Point2::ORIGIN, 0, sheet.record, |w| {
                for id in records_in(drawing, sheet.id) {
                    if let Some(record) = drawing.record(id) {
                        w.record(record, sheet.record, drawing);
                    }
                }
                for (entity, handle) in layout.kept_entities.iter().zip(&sheet.kept) {
                    w.group(0, &entity.kind);
                    w.kept_groups(&entity.groups, *handle, sheet.record, &structure.remap);
                }
            });
            self.paper = false;
            let active = layout
                .kept
                .iter()
                .find(|(code, _)| *code == 331)
                .map(|(_, old)| old.trim().to_uppercase());
            if let (Some(old), Some(handle)) = (active, self.active_viewport.take()) {
                self.viewport_links.insert(old, handle);
            }
        }
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

    /// The anonymous block (flag 1) of the n-th dimension, in the order
    /// their records are written: it holds what the dimension's style lays
    /// out — lines, arrowhead outlines, the arc, the text — with the
    /// dimension's properties, as EXPLODE would make them (ADR-0020).
    fn dimension_block(
        &mut self,
        drawing: &Drawing,
        n: usize,
        dimension: &DimensionEntity,
        properties: Properties,
        owner: Handle,
    ) {
        let parts = Entity::from(dimension.clone())
            .exploded(properties, drawing.tables())
            .unwrap_or_default();
        self.block(
            &dimension_block_name(n + 1),
            Point2::ORIGIN,
            1,
            owner,
            |w| {
                for (part, properties) in parts {
                    w.record(&EntityRecord::new(part, properties), owner, drawing);
                }
            },
        );
    }

    fn record(&mut self, record: &EntityRecord, owner: Handle, drawing: &Drawing) {
        let layer = drawing
            .layers()
            .get(record.properties.layer)
            .map_or(Layer::ZERO, |layer| layer.name.as_str());
        let kind = match &*record.entity {
            Entity::Line(_) => "LINE",
            Entity::Circle(_) => "CIRCLE",
            Entity::Arc(_) => "ARC",
            Entity::Ellipse(_) => "ELLIPSE",
            Entity::Spline(_) => "SPLINE",
            Entity::AttributeDefinition(_) => "ATTDEF",
            Entity::Polyline(_) => "LWPOLYLINE",
            Entity::Text(_) => "TEXT",
            Entity::MText(_) => "MTEXT",
            Entity::BlockRef(_) => "INSERT",
            Entity::Dimension(_) => "DIMENSION",
            Entity::Hatch(_) => "HATCH",
            Entity::Viewport(_) => "VIEWPORT",
            Entity::Point(_) => "POINT",
            Entity::Leader(_) => "LEADER",
            Entity::XLine(_) => "XLINE",
            Entity::Ray(_) => "RAY",
        };
        self.entity(kind, owner, layer, &record.properties, drawing.linetypes());
        match &*record.entity {
            Entity::Line(line) => self.line(line),
            Entity::Circle(circle) => self.circle(circle),
            Entity::Arc(arc) => self.arc(arc),
            Entity::Ellipse(ellipse) => self.ellipse(ellipse),
            Entity::Spline(spline) => self.spline(spline),
            Entity::Polyline(polyline) => self.polyline(polyline),
            Entity::Text(text) => self.text(text, drawing.text_styles()),
            Entity::MText(text) => self.mtext(text, drawing.text_styles()),
            Entity::BlockRef(reference) => {
                self.insert(reference, drawing.blocks());
                self.attributes(reference, owner, layer, &record.properties, drawing);
            }
            Entity::AttributeDefinition(definition) => {
                self.attribute_definition(definition, drawing.text_styles());
            }
            Entity::Dimension(dimension) => self.dimension(dimension, drawing),
            Entity::Hatch(hatch) => self.hatch(hatch),
            Entity::Viewport(viewport) => self.viewport_entity(viewport),
            Entity::Point(point) => self.point_entity(point),
            Entity::Leader(leader) => {
                self.leader(leader, owner, layer, &record.properties, drawing)
            }
            // An XLINE or a RAY (#206): its base and its unit direction.
            Entity::XLine(line) => {
                self.group(100, "AcDbXline");
                self.point(10, line.base);
                self.point(11, Point2::ORIGIN + line.direction);
            }
            Entity::Ray(ray) => {
                self.group(100, "AcDbRay");
                self.point(10, ray.start);
                self.point(11, Point2::ORIGIN + ray.direction);
            }
        }
    }

    /// A LEADER (#205): its style (3), whether it has a note (73: 0 for
    /// an MTEXT, 3 for none), its vertices (76, then 10, 20, 30 each),
    /// and the handle of its note (340); then the note as an MTEXT of
    /// its own, laid out by the style, under that handle.
    fn leader(
        &mut self,
        leader: &LeaderEntity,
        owner: Handle,
        layer: &str,
        properties: &Properties,
        drawing: &Drawing,
    ) {
        let style = drawing.tables().dim_style(leader.style);
        let layout = leader.layout(style);
        let note = layout
            .text
            .as_ref()
            .map(|text| (self.handles.take(), text.clone()));
        self.group(100, "AcDbLeader");
        self.group(3, &style.name);
        self.int(71, 1);
        self.int(72, 0);
        self.int(73, if note.is_some() { 0 } else { 3 });
        self.int(74, 0);
        self.int(75, 0);
        self.real(40, style.text_height);
        self.real(41, layout.text.as_ref().map_or(0.0, TextEntity::width));
        self.int(76, leader.vertices.len() as i64);
        for vertex in &leader.vertices {
            self.point(10, *vertex);
        }
        if let Some((handle, text)) = note {
            self.handle(340, handle);
            // The note's top left: the text's insertion is its baseline.
            let top_left = text.start() + Vec2::new(0.0, text.height);
            let paragraph = MTextEntity::new(
                top_left,
                text.width(),
                text.height,
                0.0,
                text.content.clone(),
            );
            self.entity_with(
                handle,
                "MTEXT",
                owner,
                layer,
                properties,
                drawing.linetypes(),
            );
            self.mtext(&paragraph, drawing.text_styles());
        }
    }

    /// A POINT (#198): its place.
    fn point_entity(&mut self, point: &PointEntity) {
        self.group(100, "AcDbPoint");
        self.point(10, point.position);
    }

    /// A VIEWPORT (#192): its frame on paper — centre (10, 20), width
    /// (40), height (41) — on or off (68), its number (69) in the order
    /// written, the model point at its centre (12, 22), and the height of
    /// the model's view (45); the rest as every writer lays it out: a view
    /// straight down, no twist, no lens, no UCS.
    fn viewport_entity(&mut self, viewport: &ViewportEntity) {
        self.viewports_written += 1;
        if self.active_viewport.is_none() {
            self.active_viewport = Some(self.handles.last());
        }
        self.group(100, "AcDbViewport");
        self.point(10, viewport.center);
        self.real(30, 0.0);
        self.real(40, viewport.width);
        self.real(41, viewport.height);
        self.int(68, if viewport.on { 1 } else { 0 });
        self.int(69, self.viewports_written as i64);
        self.point(12, viewport.view_center);
        self.point(13, Point2::ORIGIN);
        self.point(14, Point2::new(0.5, 0.5));
        self.point(15, Point2::new(0.5, 0.5));
        self.point(16, Point2::ORIGIN);
        self.real(36, 1.0);
        self.point(17, Point2::ORIGIN);
        self.real(37, 0.0);
        self.real(42, 50.0);
        self.real(43, 0.0);
        self.real(44, 0.0);
        self.real(45, viewport.view_height());
        self.real(50, 0.0);
        self.real(51, 0.0);
        self.int(72, 1000);
        self.int(90, 0);
        self.group(1, "");
        self.int(281, 0);
        self.int(71, 1);
        self.int(74, 0);
        self.point(110, Point2::ORIGIN);
        self.real(130, 0.0);
        self.point(111, Point2::new(1.0, 0.0));
        self.real(131, 0.0);
        self.point(112, Point2::new(0.0, 1.0));
        self.real(132, 0.0);
        self.int(79, 0);
        self.real(146, 0.0);
    }

    /// A HATCH (ADR-0021): its pattern's name (2), the solid flag (70),
    /// associativity off (71), its loops as polyline boundary paths (92
    /// flag 2, plus 1 for the outermost) with their bulges (72), closed
    /// (73), and their vertices (93, then 10/20 and 42); the normal-parity
    /// style (75), the predefined pattern kind (76); for a pattern, its
    /// angle (52) and scale (41) and each family as placed — angle (53),
    /// a point a line passes through (43, 44), the offset to the next line
    /// in the drawing (45, 46), and the dashes (79, then 49); no seed
    /// points (98).
    fn hatch(&mut self, hatch: &HatchEntity) {
        self.group(100, "AcDbHatch");
        self.point(10, Point2::ORIGIN);
        self.real(210, 0.0);
        self.real(220, 0.0);
        self.real(230, 1.0);
        self.group(2, hatch.style.name());
        let solid = matches!(hatch.style, HatchStyle::Solid);
        self.int(70, i64::from(solid));
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
        if let HatchStyle::Pattern { angle, scale, .. } = &hatch.style {
            self.real(52, angle.to_degrees());
            self.real(41, *scale);
            self.int(77, 0);
            let families = hatch.families();
            self.int(78, families.len() as i64);
            for family in families {
                self.real(53, family.angle.to_degrees());
                self.real(43, family.origin.x);
                self.real(44, family.origin.y);
                let step = family.step();
                self.real(45, step.x);
                self.real(46, step.y);
                self.int(79, family.dashes.len() as i64);
                for dash in &family.dashes {
                    self.real(49, *dash);
                }
            }
        }
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
        self.entity_with(handle, kind, owner, layer, properties, table);
    }

    /// An entity record under a handle taken earlier: a LEADER's note,
    /// which the leader names before the note is written (#205).
    fn entity_with(
        &mut self,
        handle: Handle,
        kind: &str,
        owner: Handle,
        layer: &str,
        properties: &Properties,
        table: &LinetypeTable,
    ) {
        self.group(0, kind);
        self.handle(5, handle);
        self.handle(330, owner);
        self.group(100, "AcDbEntity");
        if self.paper {
            self.group(67, "1");
        }
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

    /// An ELLIPSE (#181): the centre, the major axis's end relative to it,
    /// the ratio, and the parameters — 0 and 2π for a whole one.
    fn ellipse(&mut self, ellipse: &EllipseEntity) {
        self.group(100, "AcDbEllipse");
        self.point(10, ellipse.center);
        self.point(11, Point2::ORIGIN + ellipse.major);
        self.real(40, ellipse.ratio);
        let geometry = ellipse.ellipse();
        if geometry.is_full() {
            self.real(41, 0.0);
            self.real(42, std::f64::consts::TAU);
        } else {
            self.real(41, geometry.start);
            self.real(42, geometry.start + geometry.sweep);
        }
    }

    /// A SPLINE (#182): flags (70: planar, and closed when it ends where
    /// it began), the degree (71), the counts (72, 73, 74), the knots (40),
    /// the weights (41) when rational, the control points (10), and the
    /// fit points (11) it was drawn through, when it was.
    fn spline(&mut self, spline: &SplineEntity) {
        self.group(100, "AcDbSpline");
        let flags = 8 | if spline.is_closed() { 1 } else { 0 };
        self.int(70, flags);
        self.int(71, spline.degree as i64);
        self.int(72, spline.knots.len() as i64);
        self.int(73, spline.control.len() as i64);
        self.int(74, spline.fit.len() as i64);
        for knot in &spline.knots {
            self.real(40, *knot);
        }
        for weight in &spline.weights {
            self.real(41, *weight);
        }
        for point in &spline.control {
            self.point(10, *point);
        }
        for point in &spline.fit {
            self.point(11, *point);
        }
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
        if polyline.width() > 0.0 {
            self.real(43, polyline.width());
        }
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
        self.text_groups(text, styles);
        self.group(100, "AcDbText");
    }

    /// The text groups shared by TEXT, ATTDEF, and ATTRIB: placement,
    /// height, words, rotation, style, justification.
    fn text_groups(&mut self, text: &TextEntity, styles: &TextStyleTable) {
        self.point(10, text.start());
        self.real(40, text.height);
        self.group(1, &text_codes(&text.content));
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
    }

    /// An ATTDEF (#183): a text whose words are the default, then its
    /// prompt (3), tag (2), and flags (70).
    fn attribute_definition(
        &mut self,
        definition: &AttributeDefinitionEntity,
        styles: &TextStyleTable,
    ) {
        self.group(100, "AcDbText");
        self.text_groups(&definition.text, styles);
        self.group(100, "AcDbAttributeDefinition");
        self.group(3, &definition.prompt);
        self.group(2, &definition.tag);
        self.int(70, if definition.invisible { 1 } else { 0 });
    }

    /// The ATTRIBs of a reference whose block defines attributes (#183),
    /// one per definition in definition order, each placed where the
    /// reference puts it, then the SEQEND that ends them. The INSERT said
    /// `66 1` before.
    fn attributes(
        &mut self,
        reference: &BlockReferenceEntity,
        owner: Handle,
        layer: &str,
        properties: &Properties,
        drawing: &Drawing,
    ) {
        let Some(block) = drawing.blocks().get(reference.block) else {
            return;
        };
        let definitions: Vec<AttributeDefinitionEntity> =
            block.attribute_definitions().cloned().collect();
        if definitions.is_empty() {
            return;
        }
        for definition in &definitions {
            let value = reference
                .attribute(&definition.tag)
                .unwrap_or(&definition.text.content);
            let placed = match reference.place(&Entity::Text(definition.text.clone()), block.base) {
                Entity::Text(text) => text.saying(value),
                _ => unreachable!("a text placed is a text"),
            };
            self.entity("ATTRIB", owner, layer, properties, drawing.linetypes());
            self.group(100, "AcDbText");
            self.text_groups(&placed, drawing.text_styles());
            self.group(100, "AcDbAttribute");
            self.group(2, &definition.tag);
            self.int(70, if definition.invisible { 1 } else { 0 });
        }
        self.entity("SEQEND", owner, layer, properties, drawing.linetypes());
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
        if blocks
            .get(reference.block)
            .is_some_and(|block| block.attribute_definitions().next().is_some())
        {
            self.int(66, 1);
        }
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
    fn objects(&mut self, structure: &Structure, drawing: &Drawing) {
        self.dictionary(structure.root, Handle::NONE);
        self.group(3, "ACAD_GROUP");
        self.handle(350, structure.groups);
        self.group(3, "ACAD_LAYOUT");
        self.handle(350, structure.layouts);
        self.group(3, "ACAD_PLOTSTYLENAME");
        self.handle(350, structure.plot_styles);

        self.dictionary(structure.groups, structure.root);

        self.dictionary(structure.layouts, structure.root);
        if structure.sheets.is_empty() {
            self.group(3, "Layout1");
            self.handle(350, structure.paper_layout);
        }
        for sheet in &structure.sheets {
            if let Some(layout) = drawing.layouts().get(sheet.id) {
                self.group(3, &layout.name);
                self.handle(350, sheet.layout);
            }
        }
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
            0,
            (Vec2::new(420.0, 297.0), LayoutPlot::default()),
        );
        if structure.sheets.is_empty() {
            self.layout(
                structure.paper_layout,
                structure.layouts,
                "Layout1",
                structure.paper_space,
                1,
                (Vec2::new(420.0, 297.0), LayoutPlot::default()),
            );
        }
        // A layout the file gave its LAYOUT object keeps it, under the
        // name it has now; one made here gets a sheet of its own (#191).
        for (index, sheet) in structure.sheets.iter().enumerate() {
            let Some(layout) = drawing.layouts().get(sheet.id) else {
                continue;
            };
            if layout.kept.is_empty() {
                self.layout(
                    sheet.layout,
                    structure.layouts,
                    &layout.name,
                    sheet.record,
                    index as i64 + 1,
                    (layout.sheet_size(), layout.plot_settings()),
                );
            } else {
                self.group(0, "LAYOUT");
                let groups = layout_groups_as_now(&layout.kept, layout);
                self.kept_groups(&groups, sheet.layout, structure.layouts, &structure.remap);
            }
        }
    }

    /// A kept record's groups (#184), as the file held them, with its own
    /// handle (5) and owner (the first 330) made those of this file and
    /// every other handle it names mapped to the record it names here —
    /// or to nothing, when that was not kept.
    fn kept_groups(
        &mut self,
        groups: &[(i32, String)],
        own: Handle,
        owner: Handle,
        remap: &HashMap<String, Handle>,
    ) {
        let mut owner_written = false;
        for (code, value) in groups {
            match *code {
                5 => self.handle(5, own),
                330 if !owner_written => {
                    owner_written = true;
                    self.handle(330, owner);
                }
                code if is_handle_reference(code) => {
                    let old = value.trim().to_uppercase();
                    match self.viewport_links.get(&old).or_else(|| remap.get(&old)) {
                        Some(handle) => self.handle(code, *handle),
                        None => self.group(code, "0"),
                    }
                }
                code => self.group(code, value),
            }
        }
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
    /// which names its block record. `order` is the tab's place; 0 is
    /// the model's.
    /// A LAYOUT object of this file's own: a sheet of `sheet` millimetres
    /// with the margins a layout made here has.
    fn layout(
        &mut self,
        handle: Handle,
        owner: Handle,
        name: &str,
        space: Handle,
        order: i64,
        (sheet, plot): (Vec2, LayoutPlot),
    ) {
        let model = order == 0;
        let groups = plot_groups(&plot, &[(70, if model { "1024" } else { "0" }.to_owned())]);
        let group_of = |code: i32| {
            groups
                .iter()
                .find(|(c, _)| *c == code)
                .map(|(_, value)| value.clone())
                .unwrap_or_default()
        };
        self.group(0, "LAYOUT");
        self.handle(5, handle);
        self.handle(330, owner);
        self.group(100, "AcDbPlotSettings");
        self.group(1, "");
        let paper = if sheet == Vec2::new(420.0, 297.0) {
            "A3".to_owned()
        } else {
            format!("{}_x_{}_mm", format_real(sheet.x), format_real(sheet.y))
        };
        self.group(4, &paper);
        self.group(6, "");
        for (code, value) in [
            (40, 7.5),
            (41, 20.0),
            (42, 7.5),
            (43, 20.0),
            (44, sheet.x),
            (45, sheet.y),
            (46, 0.0),
            (47, 0.0),
        ] {
            self.real(code, value);
        }
        for code in [48, 49, 140, 141, 142, 143] {
            self.group(code, &group_of(code));
        }
        // Plot layout flags: the model layout is "model type".
        self.group(70, &group_of(70));
        self.group(72, "1");
        self.group(73, &group_of(73));
        self.group(74, &group_of(74));
        self.group(7, "");
        self.group(75, &group_of(75));
        self.group(76, "0");
        self.group(77, "2");
        self.group(78, "300");
        self.real(147, 1.0);
        self.real(148, 0.0);
        self.real(149, 0.0);
        self.group(100, "AcDbLayout");
        self.group(1, name);
        self.group(70, "1");
        self.int(71, order);
        self.real(10, 0.0);
        self.real(20, 0.0);
        self.real(11, sheet.x);
        self.real(21, sheet.y);
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

/// A text's words as TEXT codes: a `%`, which a reader takes as the start
/// of a `%%` code, is written `%%%`, the code for itself (a fuzzer finding,
/// #177: a run of them read back shorter).
fn text_codes(content: &str) -> String {
    content.replace('%', "%%%")
}

/// A paragraph's words as MTEXT codes: a line break is `\P`, and the
/// characters MTEXT reads as the start of a code — a backslash, a brace,
/// a `%` — are escaped.
fn mtext_codes(content: &str) -> String {
    let mut out = String::with_capacity(content.len());
    for c in content.chars() {
        match c {
            '\n' => out.push_str("\\P"),
            '\\' => out.push_str("\\\\"),
            '{' => out.push_str("\\{"),
            '}' => out.push_str("\\}"),
            '%' => out.push_str("%%%"),
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
                point_style: cad_document::PointStyle::new(35, 2.5),
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

    /// A fuzzer finding (#177): a `%` written as itself starts a `%%` code
    /// on reading, so a run of them came back shorter and a text ending in
    /// `%%` lost it.
    #[test]
    fn a_percent_sign_in_a_text_or_a_paragraph_survives() {
        let mut drawing = Drawing::new();
        let mut tx = drawing.transaction();
        tx.insert(TextEntity::new(
            Point2::ORIGIN,
            2.5,
            0.0,
            "100% sure, 50%% more, %%%%%%%%%%%",
        ))
        .unwrap();
        tx.insert(MTextEntity::new(
            Point2::new(0.0, 10.0),
            60.0,
            2.5,
            0.0,
            "rate 5%\nends %%".to_owned(),
        ))
        .unwrap();
        tx.commit();
        let text = export(&drawing);
        assert!(text.contains("\n100%%% sure, 50%%%%%% more, %%%%%%%%%%%%%%%%%%%%%%%%%%%%%%%%%\n"));
        let back = crate::import(text.as_bytes()).unwrap();
        let mut contents: Vec<String> = back
            .drawing
            .entities()
            .map(|(_, entity)| match entity {
                Entity::Text(text) => text.content.clone(),
                Entity::MText(text) => text.content.clone(),
                other => panic!("{other:?}"),
            })
            .collect();
        contents.sort();
        assert_eq!(
            contents,
            vec!["100% sure, 50%% more, %%%%%%%%%%%", "rate 5%\nends %%"]
        );
        assert!(!back.report.has_unsupported_content());
        assert_eq!(back.report.formatting_dropped, 0);
        assert_eq!(export(&back.drawing), text);
    }

    /// #181: an ellipse and an arc of one are written as DXF holds them and
    /// read back the same, byte for byte on the second pass.
    #[test]
    fn ellipses_are_written_with_their_axis_ratio_and_parameters_and_survive() {
        let mut drawing = Drawing::new();
        let mut tx = drawing.transaction();
        tx.insert(EllipseEntity::new(
            Point2::new(10.0, 20.0),
            cad_core::Vec2::new(30.0, 40.0),
            0.6,
        ))
        .unwrap();
        tx.insert(EllipseEntity::arc(
            Point2::ORIGIN,
            cad_core::Vec2::new(25.0, 0.0),
            0.4,
            0.5,
            2.0,
        ))
        .unwrap();
        tx.commit();
        let text = export(&drawing);
        let records = records(&text, "ELLIPSE");
        assert_eq!(records.len(), 2);
        assert_eq!(value(&records[0], 11), Some("30.0"));
        assert_eq!(value(&records[0], 21), Some("40.0"));
        assert_eq!(value(&records[0], 40), Some("0.6"));
        assert_eq!(value(&records[0], 41), Some("0.0"));
        assert_eq!(value(&records[0], 42), Some("6.283185307179586"));
        assert_eq!(value(&records[1], 41), Some("0.5"));
        assert_eq!(value(&records[1], 42), Some("2.0"));
        let back = crate::import(text.as_bytes()).unwrap();
        assert_eq!(back.report.ellipses, 2);
        assert_eq!(export(&back.drawing), text);
    }

    /// #182: a spline is written with its degree, counts, knots, weights,
    /// control points, and fit points, and reads back the same.
    #[test]
    fn splines_are_written_whole_and_survive() {
        let mut drawing = Drawing::new();
        let mut tx = drawing.transaction();
        let through = cad_document::SplineEntity::through(&[
            Point2::new(0.0, 0.0),
            Point2::new(30.0, 60.0),
            Point2::new(70.0, 60.0),
            Point2::new(100.0, 0.0),
        ])
        .unwrap();
        tx.insert(through).unwrap();
        tx.commit();
        let text = export(&drawing);
        let records = records(&text, "SPLINE");
        assert_eq!(records.len(), 1);
        assert_eq!(value(&records[0], 71), Some("3"));
        assert_eq!(value(&records[0], 72), Some("8"));
        assert_eq!(value(&records[0], 73), Some("4"));
        assert_eq!(value(&records[0], 74), Some("4"));
        let back = crate::import(text.as_bytes()).unwrap();
        assert_eq!(back.report.splines, 1);
        assert_eq!(export(&back.drawing), text);
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

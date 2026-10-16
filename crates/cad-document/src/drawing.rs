//! The drawing: the authoritative CAD state.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque};

mod merge;
use std::fmt;
use std::sync::Arc;

use cad_core::{Aabb, EntityId, LengthUnit, Vec2};

use crate::block::{Block, BlockId, BlockTable, initial_blocks};
use crate::dim_style::{DimStyle, DimStyleError, DimStyleId, DimStyleTable, initial_dim_styles};
use crate::dimension::DimensionError;
use crate::group::{Group, GroupId, GroupTable, initial_groups};
use crate::history::History;
use crate::id_alloc::EntityIdAllocator;
use crate::layer::{Layer, LayerId, LayerTable, initial_layers};
use crate::layer_state::{
    LayerState, LayerStateId, LayerStateTable, SavedLayer, initial_layer_states,
};
use crate::layout::{Layout, LayoutId, LayoutPlot, LayoutTable, initial_layouts};
use crate::leader::LeaderError;
use crate::linetype::{LinetypeDef, LinetypeError, LinetypeId, LinetypeTable, initial_linetypes};
use crate::mleader::{
    MLeaderError, MLeaderStyle, MLeaderStyleError, MLeaderStyleId, MLeaderStyleTable,
    initial_mleader_styles,
};
use crate::named_view::{NamedView, NamedViewId, NamedViewTable, initial_named_views};
use crate::names::{NameError, check_name, same_name};
use crate::point::{PointStyle, PointStyleError};
use crate::spatial::RTreeIndex;
use crate::table::TableChange;
use crate::tables::Tables;
use crate::text_style::{
    TextStyle, TextStyleError, TextStyleId, TextStyleTable, initial_text_styles,
};
use crate::viewport::ViewportError;
use crate::{
    Change, Color, DrawingSettings, Entity, EntityChange, EntityRecord, EntityStore, Linetype,
    Lineweight, MTextEntity, Properties, Revision, SettingsChange, SpatialIndex, TextEntity,
    Transaction,
};

/// A 2D drawing.
///
/// Reads are open. Writes go through exactly three doors — [`transaction`],
/// [`undo`], and [`redo`] — each of which keeps the entity store, the tables,
/// the spatial index, the history, and the revision consistent.
///
/// [`transaction`]: Drawing::transaction
/// [`undo`]: Drawing::undo
/// [`redo`]: Drawing::redo
#[derive(Debug)]
pub struct Drawing {
    entities: EntityStore,
    layers: LayerTable,
    linetypes: LinetypeTable,
    text_styles: TextStyleTable,
    dim_styles: DimStyleTable,
    blocks: BlockTable,
    settings: DrawingSettings,
    ids: EntityIdAllocator,
    index: Box<dyn SpatialIndex>,
    /// The entities without bounds — xlines and rays (ADR-0031) — kept out
    /// of the index's tree and tested one by one beside it.
    unbounded: BTreeSet<EntityId>,
    /// Every entity with a circle or an arc in it, by its largest radius
    /// (#210): what can be too large for the arc shader at a zoom
    /// (ADR-0009) is found without reading every entity.
    radii: BTreeSet<(Radius, EntityId)>,
    radius_of: HashMap<EntityId, f64>,
    /// How many entities are in each tier of the drawing order (#226):
    /// the top tier is where new objects go.
    orders: BTreeMap<i16, usize>,
    history: History,
    revision: Revision,
    shown_revision: Revision,
    /// What each change to what is shown touched, newest last: the shown
    /// revision before it, the one after, and the entities — or `None`
    /// when everything did, as when a layer is switched off. Derived state
    /// such as the render scene asks [`Drawing::shown_changes_since`] what
    /// to redo instead of redoing all of it (ADR-0022).
    journal: VecDeque<(Revision, Revision, Option<Vec<EntityId>>)>,
    /// The spaces: the model first, then the layouts (#191).
    layouts: LayoutTable,
    /// Named sets of objects picked together (#214).
    groups: GroupTable,
    /// Saved layer states (#228).
    layer_states: LayerStateTable,
    /// Named views (#235).
    views: NamedViewTable,
    /// Multileader styles (#240).
    mleader_styles: MLeaderStyleTable,
}

/// How many changes the journal remembers; a scene further behind than
/// this is rebuilt whole.
const JOURNAL_LENGTH: usize = 64;

/// What a commit, undo, or redo did, so interaction state such as the
/// selection can follow along without being stored in history (DESIGN §19.7).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ApplyReport {
    /// Entities that now exist and did not before.
    pub inserted: Vec<EntityId>,
    /// Entities that no longer exist.
    pub removed: Vec<EntityId>,
    /// Entities whose geometry or properties changed.
    pub replaced: Vec<EntityId>,
    /// Whether a row of any table changed: a layer made, renamed, removed.
    pub tables_changed: bool,
}

/// Everything a drawing read from a file is made of (#240): the tables'
/// rows, the layouts, and the entities, each in the space it names. See
/// [`Drawing::assemble`].
#[derive(Clone, Debug, Default)]
pub struct DrawingParts {
    pub layers: Vec<Layer>,
    pub linetypes: Vec<LinetypeDef>,
    pub text_styles: Vec<TextStyle>,
    pub dim_styles: Vec<DimStyle>,
    pub mleader_styles: Vec<MLeaderStyle>,
    pub blocks: Vec<Block>,
    pub layouts: Vec<Layout>,
    pub entities: Vec<(Entity, Properties, LayoutId)>,
}

/// Why a change could not be added to a transaction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransactionError {
    /// The entity does not exist in the drawing.
    UnknownEntity(EntityId),
    /// The multileader style does not exist (#240).
    UnknownMLeaderStyle(MLeaderStyleId),
    /// A multileader no layout can be drawn from.
    BadMLeader(MLeaderError),
    /// The name cannot be a multileader style's name.
    BadMLeaderStyleName(NameError),
    /// This multileader style already has the name.
    DuplicateMLeaderStyleName(MLeaderStyleId),
    /// A multileader style's values no multileader can be drawn by.
    BadMLeaderStyle(MLeaderStyleError),
    /// `Standard` keeps its name and stays.
    PermanentMLeaderStyle,
    /// A multileader is drawn by the style, or it is the current one.
    MLeaderStyleInUse(MLeaderStyleId),
    /// A layer cannot be merged into itself (#236).
    MergedIntoItself(LayerId),
    /// The layer state does not exist, in the drawing or after this
    /// transaction (#228).
    UnknownLayerState(LayerStateId),
    /// The named view does not exist, in the drawing or after this
    /// transaction (#235).
    UnknownView(NamedViewId),
    /// The name cannot be a view's name.
    BadViewName(NameError),
    /// This view already has the name, give or take its case.
    DuplicateViewName(NamedViewId),
    /// A view must show something: a finite centre and a positive size.
    BadView,
    /// The name cannot be a layer state's name.
    BadLayerStateName(NameError),
    /// This layer state already has the name, give or take its case.
    DuplicateLayerStateName(LayerStateId),
    /// The group does not exist, in the drawing or after this transaction.
    UnknownGroup(GroupId),
    /// The name cannot be a group's name.
    BadGroupName(NameError),
    /// This group already has the name, give or take its case.
    DuplicateGroupName(GroupId),
    /// A group must have at least one member.
    EmptyGroup,
    /// A group's member must be an entity of the drawing, given once.
    BadGroupMember(EntityId),
    /// The entity was already changed earlier in this transaction.
    AlreadyChanged(EntityId),
    /// The layer does not exist, in the drawing or earlier in this transaction.
    UnknownLayer(LayerId),
    /// The layer was already changed earlier in this transaction.
    LayerAlreadyChanged(LayerId),
    /// Layer `0` is always there: it cannot be removed or renamed.
    PermanentLayer,
    /// An entity is on the layer, so it cannot be removed.
    LayerInUse(LayerId),
    /// The name cannot be a layer's name.
    BadLayerName(NameError),
    /// This layer already has the name, give or take its case.
    DuplicateLayerName(LayerId),
    /// The layer is the current one, so it cannot be removed, switched
    /// off, or frozen.
    CurrentLayer(LayerId),
    /// The layer is off, so it cannot be made the current one.
    LayerIsOff(LayerId),
    /// The layer is frozen, so it cannot be made the current one (#213).
    LayerIsFrozen(LayerId),
    /// The linetype does not exist, in the drawing or earlier in this
    /// transaction.
    UnknownLinetype(LinetypeId),
    /// The linetype was already changed earlier in this transaction.
    LinetypeAlreadyChanged(LinetypeId),
    /// `Continuous` is always there: it cannot be removed.
    PermanentLinetype,
    /// An entity or a layer has the linetype, or new objects are given
    /// it, so it cannot be removed.
    LinetypeInUse(LinetypeId),
    /// The row cannot be a linetype.
    BadLinetype(LinetypeError),
    /// This linetype already has the name, give or take its case.
    DuplicateLinetypeName(LinetypeId),
    /// `LTSCALE` must be a positive number.
    BadLtscale,
    /// The point style is not one the drawing can draw.
    BadPointStyle(PointStyleError),
    /// A leader with too few points, or one that is not finite.
    BadLeader(LeaderError),
    /// The dimension style does not exist, in the drawing or earlier in
    /// this transaction.
    UnknownDimStyle(DimStyleId),
    /// `Standard` is always there: it cannot be removed.
    PermanentDimStyle,
    /// The style is the one new dimensions take, or a dimension has it: it
    /// cannot be removed.
    DimStyleInUse(DimStyleId),
    /// The name cannot be a dimension style's name.
    BadDimStyleName(NameError),
    /// This dimension style already has the name, give or take its case.
    DuplicateDimStyleName(DimStyleId),
    /// A value of the style is not one a dimension can be drawn by.
    BadDimStyle(DimStyleError),
    /// A value of the style is not one a text can be drawn by (#221).
    BadTextStyle(TextStyleError),
    /// A dimension that cannot be laid out.
    BadDimension(DimensionError),
    /// The text style does not exist, in the drawing or earlier in this
    /// transaction.
    UnknownTextStyle(TextStyleId),
    /// `Standard` is always there: it cannot be removed.
    PermanentTextStyle,
    /// A text has the style, so it cannot be removed.
    TextStyleInUse(TextStyleId),
    /// The name cannot be a text style's name.
    BadTextStyleName(NameError),
    /// This text style already has the name, give or take its case.
    DuplicateTextStyleName(TextStyleId),
    /// The block does not exist, in the drawing or earlier in this
    /// transaction.
    UnknownBlock(BlockId),
    /// The block was already changed earlier in this transaction.
    BlockAlreadyChanged(BlockId),
    /// `*Model_Space` is the drawing: it cannot be referenced, redefined,
    /// or removed.
    PermanentBlock,
    /// A reference places the block, so it cannot be removed.
    BlockInUse(BlockId),
    /// The name cannot be a block's name.
    BadBlockName(NameError),
    /// This block already has the name, give or take its case.
    DuplicateBlockName(BlockId),
    /// The definition would hold a reference to itself, directly or
    /// through other blocks.
    BlockCycle(BlockId),
    /// A reference's scale must be a positive number.
    BadBlockScale,
    /// A viewport that cannot be as given (#192).
    BadViewport(ViewportError),
    /// A viewport is a window of a layout's paper: it cannot be in the
    /// model, nor in a block.
    ViewportOffPaper,
    /// The layout does not exist, in the drawing or earlier in this
    /// transaction.
    UnknownLayout(LayoutId),
    /// The model is always there: it cannot be removed or renamed.
    PermanentLayout,
    /// This transaction already touched an entity of the layout, so it
    /// cannot remove it: do that in a transaction of its own.
    LayoutInUse(LayoutId),
    /// The name cannot be a layout's name.
    BadLayoutName(NameError),
    /// A sheet is finite and positive both ways.
    BadSheet,
    /// This layout already has the name, give or take its case.
    DuplicateLayoutName(LayoutId),
}

impl fmt::Display for TransactionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownEntity(id) => write!(f, "entity {id} does not exist"),
            Self::MergedIntoItself(id) => write!(f, "layer {id} cannot be merged into itself"),
            Self::UnknownMLeaderStyle(id) => write!(f, "multileader style {id} does not exist"),
            Self::BadMLeader(error) => write!(f, "{error}"),
            Self::BadMLeaderStyleName(error) => write!(f, "bad multileader style name: {error:?}"),
            Self::DuplicateMLeaderStyleName(id) => {
                write!(f, "multileader style {id} already has that name")
            }
            Self::BadMLeaderStyle(error) => write!(f, "{}", error.explain()),
            Self::PermanentMLeaderStyle => write!(f, "Standard keeps its name and stays"),
            Self::MLeaderStyleInUse(id) => write!(f, "multileader style {id} is in use"),
            Self::UnknownLayerState(id) => write!(f, "layer state {id} does not exist"),
            Self::UnknownView(id) => write!(f, "view {id} does not exist"),
            Self::BadViewName(error) => write!(f, "bad view name: {error:?}"),
            Self::DuplicateViewName(id) => write!(f, "view {id} already has that name"),
            Self::BadView => write!(f, "a view must show something"),
            Self::BadLayerStateName(error) => write!(f, "bad layer state name: {error:?}"),
            Self::DuplicateLayerStateName(id) => {
                write!(f, "layer state {id} already has that name")
            }
            Self::UnknownGroup(id) => write!(f, "group {id} does not exist"),
            Self::BadGroupName(error) => write!(f, "bad group name: {error:?}"),
            Self::DuplicateGroupName(id) => write!(f, "group {id} already has that name"),
            Self::EmptyGroup => write!(f, "a group needs a member"),
            Self::BadGroupMember(id) => {
                write!(
                    f,
                    "entity {id} cannot be a member: not there, or given twice"
                )
            }
            Self::AlreadyChanged(id) => {
                write!(f, "entity {id} was already changed in this transaction")
            }
            Self::UnknownLayer(id) => write!(f, "layer {id} does not exist"),
            Self::LayerAlreadyChanged(id) => {
                write!(f, "layer {id} was already changed in this transaction")
            }
            Self::PermanentLayer => write!(f, "layer 0 cannot be removed or renamed"),
            Self::LayerInUse(id) => write!(f, "layer {id} has entities on it"),
            Self::BadLayerName(error) => write!(f, "{}", error.explain("layer")),
            Self::DuplicateLayerName(id) => write!(f, "layer {id} already has that name"),
            Self::CurrentLayer(id) => write!(f, "layer {id} is the current layer"),
            Self::LayerIsOff(id) => write!(f, "layer {id} is off"),
            Self::LayerIsFrozen(id) => write!(f, "layer {id} is frozen"),
            Self::UnknownLinetype(id) => write!(f, "linetype {id} does not exist"),
            Self::LinetypeAlreadyChanged(id) => {
                write!(f, "linetype {id} was already changed in this transaction")
            }
            Self::PermanentLinetype => write!(f, "linetype Continuous cannot be removed"),
            Self::LinetypeInUse(id) => write!(f, "linetype {id} is in use"),
            Self::BadLinetype(error) => write!(f, "{error}"),
            Self::DuplicateLinetypeName(id) => {
                write!(f, "linetype {id} already has that name")
            }
            Self::BadLtscale => write!(f, "LTSCALE must be a positive number"),
            Self::BadPointStyle(error) => write!(f, "{error}"),
            Self::BadLeader(error) => write!(f, "{error}"),
            Self::UnknownDimStyle(id) => write!(f, "dimension style {id} does not exist"),
            Self::PermanentDimStyle => write!(f, "dimension style Standard cannot be removed"),
            Self::DimStyleInUse(id) => write!(f, "dimension style {id} is in use"),
            Self::BadDimStyleName(error) => write!(f, "{}", error.explain("dimension style")),
            Self::DuplicateDimStyleName(id) => {
                write!(f, "dimension style {id} already has that name")
            }
            Self::BadDimStyle(error) => write!(f, "{}", error.explain()),
            Self::BadTextStyle(error) => write!(f, "{}", error.explain()),
            Self::BadDimension(error) => write!(f, "{}", error.explain()),
            Self::UnknownTextStyle(id) => write!(f, "text style {id} does not exist"),
            Self::PermanentTextStyle => write!(f, "text style Standard cannot be removed"),
            Self::TextStyleInUse(id) => write!(f, "text style {id} has texts"),
            Self::BadTextStyleName(error) => write!(f, "{}", error.explain("text style")),
            Self::DuplicateTextStyleName(id) => {
                write!(f, "text style {id} already has that name")
            }
            Self::UnknownBlock(id) => write!(f, "block {id} does not exist"),
            Self::BlockAlreadyChanged(id) => {
                write!(f, "block {id} was already changed in this transaction")
            }
            Self::PermanentBlock => write!(f, "*Model_Space is the drawing itself"),
            Self::BlockInUse(id) => write!(f, "block {id} is placed by a reference"),
            Self::BadBlockName(error) => write!(f, "{}", error.explain("block")),
            Self::DuplicateBlockName(id) => write!(f, "block {id} already has that name"),
            Self::BlockCycle(id) => write!(f, "block {id} would contain itself"),
            Self::BadBlockScale => write!(f, "a block reference's scale must be positive"),
            Self::BadViewport(error) => write!(f, "{error}"),
            Self::ViewportOffPaper => write!(f, "a viewport belongs on a layout's paper"),
            Self::UnknownLayout(id) => write!(f, "layout {id} does not exist"),
            Self::PermanentLayout => write!(f, "the model cannot be removed or renamed"),
            Self::LayoutInUse(id) => write!(
                f,
                "layout {id} has an entity this transaction already changed"
            ),
            Self::BadLayoutName(error) => write!(f, "{}", error.explain("layout")),
            Self::BadSheet => write!(f, "a sheet's width and height must be positive"),
            Self::DuplicateLayoutName(id) => write!(f, "layout {id} already has that name"),
        }
    }
}

impl std::error::Error for TransactionError {}

/// A radius ordered by its value, for the drawing's list of curves by
/// size (#210). Radii are finite and positive, so the order is total.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Radius(f64);

impl Eq for Radius {}

impl PartialOrd for Radius {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Radius {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.0.total_cmp(&other.0)
    }
}

/// The largest radius of a circle or an arc among an entity's primitives,
/// as the scene draws them — a block reference's through its definition,
/// a polyline's arc segments — or `None` when it has none.
///
/// It agrees with [`Entity::primitives`] but reads the common kinds
/// directly: a drawing opens with every entity noted, and chording an
/// ellipse or a spline only to find no arc in it doubled the cost.
fn largest_radius(entity: &Entity, tables: Tables<'_>) -> Option<f64> {
    fn largest(primitives: impl Iterator<Item = cad_geometry::Primitive>) -> Option<f64> {
        primitives
            .filter_map(|primitive| match primitive {
                cad_geometry::Primitive::Arc(arc) => Some(arc.radius),
                cad_geometry::Primitive::Circle(circle) => Some(circle.radius),
                cad_geometry::Primitive::Segment(_) => None,
            })
            .filter(|radius| radius.is_finite())
            .reduce(f64::max)
    }
    match entity {
        Entity::Circle(circle) => Some(circle.radius).filter(|radius| radius.is_finite()),
        Entity::Arc(arc) => Some(arc.radius).filter(|radius| radius.is_finite()),
        Entity::Polyline(polyline) if polyline.has_arcs() => largest(polyline.primitives()),
        Entity::BlockRef(_) => largest(entity.primitives(tables).into_iter()),
        // Chords, segments, or no primitives at all.
        Entity::Polyline(_)
        | Entity::Line(_)
        | Entity::Ellipse(_)
        | Entity::Spline(_)
        | Entity::AttributeDefinition(_)
        | Entity::Text(_)
        | Entity::MText(_)
        | Entity::Dimension(_)
        | Entity::Hatch(_)
        | Entity::Viewport(_)
        | Entity::Wipeout(_)
        | Entity::Point(_)
        | Entity::Leader(_)
        | Entity::MLeader(_)
        | Entity::XLine(_)
        | Entity::Ray(_) => None,
    }
}

/// Which tables `PURGE` sweeps (#200).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PurgeScope {
    pub blocks: bool,
    pub dim_styles: bool,
    pub layers: bool,
    pub linetypes: bool,
    pub text_styles: bool,
}

impl PurgeScope {
    pub const ALL: Self = Self {
        blocks: true,
        dim_styles: true,
        layers: true,
        linetypes: true,
        text_styles: true,
    };
    pub const NONE: Self = Self {
        blocks: false,
        dim_styles: false,
        layers: false,
        linetypes: false,
        text_styles: false,
    };
}

/// What a purge removed, by name, table by table.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Purged {
    pub blocks: Vec<String>,
    pub dim_styles: Vec<String>,
    pub layers: Vec<String>,
    pub linetypes: Vec<String>,
    pub text_styles: Vec<String>,
}

impl Purged {
    pub fn total(&self) -> usize {
        self.blocks.len()
            + self.dim_styles.len()
            + self.layers.len()
            + self.linetypes.len()
            + self.text_styles.len()
    }

    pub fn is_empty(&self) -> bool {
        self.total() == 0
    }
}

/// A viewport (#192) must be one the paper can hold, and is a window of a
/// layout's paper: in the model, or in a block, it has nothing to be a
/// window of.
fn viewport_placeable(entity: &Entity, space: LayoutId) -> Result<(), TransactionError> {
    match entity {
        Entity::Viewport(viewport) => {
            viewport.check().map_err(TransactionError::BadViewport)?;
            if space == LayoutId::MODEL {
                return Err(TransactionError::ViewportOffPaper);
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

impl Default for Drawing {
    fn default() -> Self {
        Self::new()
    }
}

impl Drawing {
    /// An empty drawing using the default spatial index.
    pub fn new() -> Self {
        Self::with_index(Box::new(RTreeIndex::new()))
    }

    /// An empty drawing using the given spatial index implementation.
    pub fn with_index(index: Box<dyn SpatialIndex>) -> Self {
        Self {
            entities: EntityStore::new(),
            layers: initial_layers(),
            linetypes: initial_linetypes(),
            text_styles: initial_text_styles(),
            dim_styles: initial_dim_styles(),
            blocks: initial_blocks(),
            settings: DrawingSettings::default(),
            ids: EntityIdAllocator::new(),
            index,
            unbounded: BTreeSet::new(),
            radii: BTreeSet::new(),
            radius_of: HashMap::new(),
            orders: BTreeMap::new(),
            history: History::default(),
            revision: Revision::next(),
            shown_revision: Revision::next(),
            journal: VecDeque::new(),
            layouts: initial_layouts(),
            groups: initial_groups(),
            layer_states: initial_layer_states(),
            views: initial_named_views(),
            mleader_styles: initial_mleader_styles(),
        }
    }

    /// A drawing pre-populated with `entities` — on layer `0`, with default
    /// properties — and an empty history.
    ///
    /// This is the import path: opening a file starts a fresh drawing rather
    /// than recording an undoable edit.
    pub fn from_entities(entities: impl IntoIterator<Item = Entity>) -> Self {
        let entities = entities
            .into_iter()
            .map(|entity| (entity, Properties::default()));
        Self::from_parts([], [], entities).expect("layer 0 always exists")
    }

    /// A drawing pre-populated with layers, linetypes, and entities that
    /// have properties, and an empty history.
    ///
    /// `layers` come after layer `0`, which is always there, and take the
    /// IDs 1, 2, … in the order given; `linetypes` likewise after
    /// `Continuous`. A row with the permanent row's name — `0`,
    /// `Continuous` — replaces that row's settings instead of adding one,
    /// and takes no ID. A row with a name that is not allowed or is already
    /// taken, a layer with a linetype that is not among them, and an entity
    /// on a layer or with a linetype that is not among them, are refused.
    pub fn from_parts(
        layers: impl IntoIterator<Item = Layer>,
        linetypes: impl IntoIterator<Item = LinetypeDef>,
        entities: impl IntoIterator<Item = (Entity, Properties)>,
    ) -> Result<Self, TransactionError> {
        Self::from_tables(layers, linetypes, [], [], [], entities)
    }

    /// [`from_tables`](Self::from_tables) with layouts as well (#191):
    /// after the model, which is always there, they take the IDs 1, 2, …
    /// in the order given, and each entity names the space it is in. An
    /// entity in a space that is not among them is refused.
    pub fn from_spaces(
        layers: impl IntoIterator<Item = Layer>,
        linetypes: impl IntoIterator<Item = LinetypeDef>,
        text_styles: impl IntoIterator<Item = TextStyle>,
        dim_styles: impl IntoIterator<Item = DimStyle>,
        blocks: impl IntoIterator<Item = Block>,
        layouts: impl IntoIterator<Item = Layout>,
        entities: impl IntoIterator<Item = (Entity, Properties, LayoutId)>,
    ) -> Result<Self, TransactionError> {
        let mut drawing = Self::with_rows(layers, linetypes, text_styles, dim_styles, [], blocks)?;
        drawing.place(layouts, entities)?;
        Ok(drawing)
    }

    /// [`from_spaces`](Self::from_spaces) with every part a file brings,
    /// multileader styles too (#240): they take the IDs 1, 2, … after
    /// `Standard` in the order given, and one called `Standard` replaces
    /// its values.
    pub fn assemble(parts: DrawingParts) -> Result<Self, TransactionError> {
        let DrawingParts {
            layers,
            linetypes,
            text_styles,
            dim_styles,
            mleader_styles,
            blocks,
            layouts,
            entities,
        } = parts;
        let mut drawing = Self::with_rows(
            layers,
            linetypes,
            text_styles,
            dim_styles,
            mleader_styles,
            blocks,
        )?;
        drawing.place(layouts, entities)?;
        Ok(drawing)
    }

    /// The layouts, then the entities of every space, of a drawing being
    /// put together.
    fn place(
        &mut self,
        layouts: impl IntoIterator<Item = Layout>,
        entities: impl IntoIterator<Item = (Entity, Properties, LayoutId)>,
    ) -> Result<(), TransactionError> {
        let drawing = self;
        for layout in layouts {
            check_name(&layout.name).map_err(TransactionError::BadLayoutName)?;
            if let Some(taken) = drawing.layouts.find(&layout.name) {
                return Err(TransactionError::DuplicateLayoutName(taken));
            }
            let id = drawing.layouts.allocate();
            drawing
                .layouts
                .apply(&TableChange::Insert { id, row: layout });
        }
        for (entity, properties, space) in entities {
            if !drawing.layouts.contains(space) {
                return Err(TransactionError::UnknownLayout(space));
            }
            Self::check_record(drawing, &entity, &properties)?;
            viewport_placeable(&entity, space)?;
            let id = drawing.ids.allocate();
            drawing
                .entities
                .insert(id, EntityRecord::new(entity, properties).in_space(space))
                .expect("freshly allocated IDs are unique");
        }
        drawing.rebuild_index();
        Ok(())
    }

    /// [`from_parts`](Self::from_parts) with text styles and blocks as
    /// well, after `Standard` and `*Model_Space`, which take the IDs 1, 2, …
    /// in the order given. A text with a style that is not among them, a
    /// reference to a block that is not, and a block that would hold
    /// itself through any chain of references are refused.
    pub fn from_tables(
        layers: impl IntoIterator<Item = Layer>,
        linetypes: impl IntoIterator<Item = LinetypeDef>,
        text_styles: impl IntoIterator<Item = TextStyle>,
        dim_styles: impl IntoIterator<Item = DimStyle>,
        blocks: impl IntoIterator<Item = Block>,
        entities: impl IntoIterator<Item = (Entity, Properties)>,
    ) -> Result<Self, TransactionError> {
        let mut drawing = Self::with_rows(layers, linetypes, text_styles, dim_styles, [], blocks)?;
        for (entity, properties) in entities {
            Self::check_record(&drawing, &entity, &properties)?;
            let id = drawing.ids.allocate();
            drawing
                .entities
                .insert(id, EntityRecord::new(entity, properties))
                .expect("freshly allocated IDs are unique");
        }
        drawing.rebuild_index();
        Ok(drawing)
    }

    /// A drawing with the tables' rows and no entity, each row checked.
    fn with_rows(
        layers: impl IntoIterator<Item = Layer>,
        linetypes: impl IntoIterator<Item = LinetypeDef>,
        text_styles: impl IntoIterator<Item = TextStyle>,
        dim_styles: impl IntoIterator<Item = DimStyle>,
        mleader_styles: impl IntoIterator<Item = MLeaderStyle>,
        blocks: impl IntoIterator<Item = Block>,
    ) -> Result<Self, TransactionError> {
        let mut drawing = Self::new();
        for style in mleader_styles {
            check_name(&style.name).map_err(TransactionError::BadMLeaderStyleName)?;
            style.check().map_err(TransactionError::BadMLeaderStyle)?;
            match drawing.mleader_styles.find(&style.name) {
                Some(id) if id == MLeaderStyleId::FIRST => {
                    let before = drawing.mleader_styles.get(MLeaderStyleId::FIRST).cloned();
                    drawing.mleader_styles.apply(&TableChange::Replace {
                        id: MLeaderStyleId::FIRST,
                        before: before.expect("Standard is always there"),
                        after: style,
                    });
                    continue;
                }
                Some(taken) => return Err(TransactionError::DuplicateMLeaderStyleName(taken)),
                None => {}
            }
            let id = drawing.mleader_styles.allocate();
            drawing
                .mleader_styles
                .apply(&TableChange::Insert { id, row: style });
        }
        for style in dim_styles {
            check_name(&style.name).map_err(TransactionError::BadDimStyleName)?;
            style.check().map_err(TransactionError::BadDimStyle)?;
            match drawing.dim_styles.find(&style.name) {
                Some(id) if id == DimStyleId::FIRST => {
                    let before = drawing.dim_styles.get(DimStyleId::FIRST).cloned();
                    drawing.dim_styles.apply(&TableChange::Replace {
                        id: DimStyleId::FIRST,
                        before: before.expect("Standard is always there"),
                        after: style,
                    });
                    continue;
                }
                Some(taken) => return Err(TransactionError::DuplicateDimStyleName(taken)),
                None => {}
            }
            let id = drawing.dim_styles.allocate();
            drawing
                .dim_styles
                .apply(&TableChange::Insert { id, row: style });
        }
        for style in text_styles {
            check_name(&style.name).map_err(TransactionError::BadTextStyleName)?;
            style.check().map_err(TransactionError::BadTextStyle)?;
            match drawing.text_styles.find(&style.name) {
                Some(id) if id == TextStyleId::FIRST => {
                    let before = drawing.text_styles.get(TextStyleId::FIRST).cloned();
                    drawing.text_styles.apply(&TableChange::Replace {
                        id: TextStyleId::FIRST,
                        before: before.expect("Standard is always there"),
                        after: style,
                    });
                    continue;
                }
                Some(taken) => return Err(TransactionError::DuplicateTextStyleName(taken)),
                None => {}
            }
            let id = drawing.text_styles.allocate();
            drawing
                .text_styles
                .apply(&TableChange::Insert { id, row: style });
        }
        for linetype in linetypes {
            linetype.check().map_err(TransactionError::BadLinetype)?;
            match drawing.linetypes.find(&linetype.name) {
                Some(id) if id == LinetypeId::FIRST => {
                    let before = drawing.linetypes.get(LinetypeId::FIRST).cloned();
                    drawing.linetypes.apply(&TableChange::Replace {
                        id: LinetypeId::FIRST,
                        before: before.expect("Continuous is always there"),
                        after: linetype,
                    });
                    continue;
                }
                Some(taken) => return Err(TransactionError::DuplicateLinetypeName(taken)),
                None => {}
            }
            let id = drawing.linetypes.allocate();
            drawing
                .linetypes
                .apply(&TableChange::Insert { id, row: linetype });
        }
        let known_linetype = |drawing: &Self, linetype: Linetype| match linetype {
            Linetype::Named(id) if !drawing.linetypes.contains(id) => {
                Err(TransactionError::UnknownLinetype(id))
            }
            _ => Ok(()),
        };
        for layer in layers {
            check_name(&layer.name).map_err(TransactionError::BadLayerName)?;
            known_linetype(&drawing, layer.linetype)?;
            match drawing.layers.find(&layer.name) {
                Some(id) if id == LayerId::FIRST => {
                    let before = drawing.layers.get(LayerId::FIRST).cloned();
                    drawing.layers.apply(&TableChange::Replace {
                        id: LayerId::FIRST,
                        before: before.expect("layer 0 is always there"),
                        after: layer,
                    });
                    continue;
                }
                Some(taken) => return Err(TransactionError::DuplicateLayerName(taken)),
                None => {}
            }
            let id = drawing.layers.allocate();
            drawing
                .layers
                .apply(&TableChange::Insert { id, row: layer });
        }
        // Every block goes in before any is checked: a definition may
        // place a block the file defines after it.
        for block in blocks {
            check_name(&block.name).map_err(TransactionError::BadBlockName)?;
            if let Some(taken) = drawing.blocks.find(&block.name) {
                return Err(TransactionError::DuplicateBlockName(taken));
            }
            let id = drawing.blocks.allocate();
            drawing
                .blocks
                .apply(&TableChange::Insert { id, row: block });
        }
        let check_record = Self::check_record;
        for (id, block) in drawing.blocks.iter() {
            if drawing.blocks.would_cycle(id, block) {
                return Err(TransactionError::BlockCycle(id));
            }
            for record in block.entities.iter() {
                check_record(&drawing, &record.entity, &record.properties)?;
                if matches!(*record.entity, Entity::Viewport(_)) {
                    return Err(TransactionError::ViewportOffPaper);
                }
            }
        }
        Ok(drawing)
    }

    /// What every record of the drawing, or of a definition, must satisfy.
    fn check_record(
        drawing: &Self,
        entity: &Entity,
        properties: &Properties,
    ) -> Result<(), TransactionError> {
        if !drawing.layers.contains(properties.layer) {
            return Err(TransactionError::UnknownLayer(properties.layer));
        }
        if let Linetype::Named(id) = properties.linetype
            && !drawing.linetypes.contains(id)
        {
            return Err(TransactionError::UnknownLinetype(id));
        }
        match entity {
            Entity::Text(TextEntity { style, .. }) | Entity::MText(MTextEntity { style, .. })
                if !drawing.text_styles.contains(*style) =>
            {
                Err(TransactionError::UnknownTextStyle(*style))
            }
            Entity::BlockRef(reference) if !drawing.blocks.contains(reference.block) => {
                Err(TransactionError::UnknownBlock(reference.block))
            }
            Entity::Dimension(dimension) if !drawing.dim_styles.contains(dimension.style) => {
                Err(TransactionError::UnknownDimStyle(dimension.style))
            }
            Entity::Dimension(dimension) if dimension.check().is_err() => Err(
                TransactionError::BadDimension(dimension.check().unwrap_err()),
            ),
            Entity::Leader(leader) if !drawing.dim_styles.contains(leader.style) => {
                Err(TransactionError::UnknownDimStyle(leader.style))
            }
            Entity::Leader(leader) if leader.check().is_err() => {
                Err(TransactionError::BadLeader(leader.check().unwrap_err()))
            }
            Entity::MLeader(mleader) if !drawing.mleader_styles.contains(mleader.style) => {
                Err(TransactionError::UnknownMLeaderStyle(mleader.style))
            }
            Entity::MLeader(mleader) => mleader.check().map_err(TransactionError::BadMLeader),
            Entity::BlockRef(reference)
                if !(reference.scale.is_finite() && reference.scale > 0.0) =>
            {
                Err(TransactionError::BadBlockScale)
            }
            Entity::Viewport(viewport) => viewport.check().map_err(TransactionError::BadViewport),
            _ => Ok(()),
        }
    }

    /// The spatial index afresh from every entity, for a drawing being
    /// put together.
    fn rebuild_index(&mut self) {
        let tables = Tables {
            blocks: &self.blocks,
            dim_styles: &self.dim_styles,
            mleader_styles: &self.mleader_styles,
            point_style: self.settings.point_style,
            wipeout_frame: self.settings.wipeout_frame,
        };
        self.unbounded = self
            .entities
            .iter()
            .filter(|(_, entity)| entity.is_unbounded())
            .map(|(id, _)| id)
            .collect();
        let bounds: Vec<(EntityId, Aabb)> = self
            .entities
            .iter()
            .filter(|(_, entity)| !entity.is_unbounded())
            .map(|(id, entity)| (id, entity.bounds(tables)))
            .collect();
        self.index.rebuild(&mut bounds.into_iter());
        // In bulk: a set built from a sorted list is not built one insert
        // at a time, and a drawing opens through here.
        let mut radii: Vec<(Radius, EntityId)> = self
            .entities
            .iter()
            .filter_map(|(id, entity)| {
                largest_radius(entity, tables).map(|radius| (Radius(radius), id))
            })
            .collect();
        radii.sort_unstable();
        self.radius_of = radii.iter().map(|(radius, id)| (*id, radius.0)).collect();
        self.radii = radii.into_iter().collect();
        self.orders.clear();
        for (_, record) in self.entities.records() {
            *self.orders.entry(record.order).or_default() += 1;
        }
    }

    /// Counts one more entity in tier `order`, or one fewer.
    fn note_order(&mut self, order: i16, more: bool) {
        if more {
            *self.orders.entry(order).or_default() += 1;
        } else if let Some(count) = self.orders.get_mut(&order) {
            *count -= 1;
            if *count == 0 {
                self.orders.remove(&order);
            }
        }
    }

    /// The highest tier of the drawing order any entity is in, or 0 in an
    /// empty drawing (#226): where a new object goes.
    pub fn top_order(&self) -> i16 {
        self.orders.keys().next_back().copied().unwrap_or(0)
    }

    /// The lowest tier any entity is in, or 0 in an empty drawing.
    pub fn bottom_order(&self) -> i16 {
        self.orders.keys().next().copied().unwrap_or(0)
    }

    /// Notes the largest radius of a circle or arc in entity `id`, or that
    /// it has none — or that it is gone (#210).
    fn note_radius(&mut self, id: EntityId) {
        let radius = self
            .entities
            .get(id)
            .and_then(|entity| largest_radius(entity, self.tables()));
        if let Some(old) = self.radius_of.remove(&id) {
            self.radii.remove(&(Radius(old), id));
        }
        if let Some(radius) = radius {
            self.radius_of.insert(id, radius);
            self.radii.insert((Radius(radius), id));
        }
    }

    /// The entities with a circle or an arc whose radius is over `radius`,
    /// largest first (#210): those that may hold a curve too large for the
    /// arc shader at a zoom, whatever their extent.
    pub fn curves_larger_than(&self, radius: f64) -> impl Iterator<Item = EntityId> + '_ {
        self.radii
            .iter()
            .rev()
            .take_while(move |(largest, _)| largest.0 > radius)
            .map(|(_, id)| *id)
    }

    /// Changes on every commit, undo, and redo, and at no other time.
    pub fn revision(&self) -> Revision {
        self.revision
    }

    /// Changes whenever what [`shown`](Self::shown) yields could have
    /// changed — an entity inserted, removed, or replaced (its properties
    /// count), or a layer switched on or off — and at no other time.
    ///
    /// A layer renamed, recoloured, or locked leaves it alone: the same
    /// entities are shown, in the same places, with the same properties.
    /// Derived state that depends on the entities but not on what their
    /// layers look like can tell by this that it is still good.
    pub fn shown_revision(&self) -> Revision {
        self.shown_revision
    }

    /// The entities whose presence, geometry, or properties changed since
    /// the shown revision `since` — `Some(empty)` when nothing did — or
    /// `None` when everything may have: a layer switched on or off, a
    /// drawing this journal does not know, or more changes than it
    /// remembers (ADR-0022).
    pub fn shown_changes_since(&self, since: Revision) -> Option<Vec<EntityId>> {
        if since == self.shown_revision {
            return Some(Vec::new());
        }
        let mut ids = Vec::new();
        for (before, after, changed) in self.journal.iter().rev() {
            ids.extend(changed.as_ref()?.iter().copied());
            if *before == since {
                return Some(ids);
            }
            let _ = after;
        }
        None
    }

    /// One more than the largest entity ID ever allocated: every entity's
    /// ID is below it.
    pub fn id_bound(&self) -> u64 {
        self.ids.peek()
    }

    pub fn len(&self) -> usize {
        self.entities.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entities.is_empty()
    }

    pub fn entity(&self, id: EntityId) -> Option<&Entity> {
        self.entities.get(id).map(|entity| &**entity)
    }

    /// Every entity, in dense storage order.
    pub fn entities(&self) -> impl Iterator<Item = (EntityId, &Entity)> {
        self.entities.iter().map(|(id, entity)| (id, &**entity))
    }

    /// The layer, colour, linetype, and lineweight of an entity.
    pub fn properties(&self, id: EntityId) -> Option<Properties> {
        self.entities.record(id).map(|record| record.properties)
    }

    /// The spaces of the drawing: the model, row [`LayoutId::MODEL`], and
    /// its layouts in tab order (#191).
    pub fn layouts(&self) -> &LayoutTable {
        &self.layouts
    }

    /// The drawing's groups (#214), in the order they were made.
    pub fn groups(&self) -> &GroupTable {
        &self.groups
    }

    /// The drawing's multileader styles (#240).
    pub fn mleader_styles(&self) -> &MLeaderStyleTable {
        &self.mleader_styles
    }

    /// The drawing's named views (#235), in the order they were saved.
    pub fn views(&self) -> &NamedViewTable {
        &self.views
    }

    /// Adds a named view without history: the import path. A refused one
    /// leaves the drawing as it was.
    pub fn import_view(&mut self, view: NamedView) -> Result<NamedViewId, TransactionError> {
        let mut tx = self.transaction();
        let id = tx.add_view(view)?;
        let changes = std::mem::take(&mut tx.changes);
        drop(tx);
        for change in &changes {
            if let Change::View(change) = change {
                self.views.apply(change);
            }
        }
        Ok(id)
    }

    /// The drawing's saved layer states (#228), in the order they were made.
    pub fn layer_states(&self) -> &LayerStateTable {
        &self.layer_states
    }

    /// The layers as they are now, as a state called `name` would keep
    /// them (#228).
    pub fn layer_state_now(&self, name: impl Into<String>) -> LayerState {
        let layers = self
            .layers
            .iter()
            .map(|(_, layer)| {
                let linetype = match layer.linetype {
                    Linetype::Named(id) => self
                        .linetypes
                        .get(id)
                        .map_or(String::new(), |def| def.name.clone()),
                    _ => String::new(),
                };
                SavedLayer::of(layer, &linetype)
            })
            .collect();
        LayerState::new(name, layers)
    }

    /// Adds a layer state without history: the import path, as
    /// [`import_group`](Self::import_group) is. A refused one leaves the
    /// drawing as it was.
    pub fn import_layer_state(
        &mut self,
        state: LayerState,
    ) -> Result<LayerStateId, TransactionError> {
        let mut tx = self.transaction();
        let id = tx.add_layer_state(state)?;
        let changes = std::mem::take(&mut tx.changes);
        drop(tx);
        for change in &changes {
            if let Change::LayerState(change) = change {
                self.layer_states.apply(change);
            }
        }
        Ok(id)
    }

    /// The groups `id` is a member of.
    pub fn groups_of(&self, id: EntityId) -> impl Iterator<Item = (GroupId, &Group)> {
        self.groups
            .iter()
            .filter(move |(_, group)| group.contains(id))
    }

    /// Adds a group without history: the import path, after
    /// [`from_spaces`](Self::from_spaces), whose entities take the IDs 1,
    /// 2, … in the order given. The group is checked as
    /// [`TransactionBuilder::add_group`] checks it; a refused one leaves
    /// the drawing as it was.
    pub fn import_group(&mut self, group: Group) -> Result<GroupId, TransactionError> {
        let mut tx = self.transaction();
        let id = tx.add_group(group)?;
        let changes = std::mem::take(&mut tx.changes);
        drop(tx);
        for change in &changes {
            if let Change::Group(change) = change {
                self.groups.apply(change);
            }
        }
        Ok(id)
    }

    /// Puts entities in tiers of the drawing order without history (#226):
    /// the import path, after [`from_spaces`](Self::from_spaces), from the
    /// order a file kept. An ID the drawing does not hold is passed over.
    pub fn import_orders(&mut self, orders: impl IntoIterator<Item = (EntityId, i16)>) {
        for (id, order) in orders {
            let Some(record) = self.entities.record(id) else {
                continue;
            };
            let before = record.order;
            let after = record.in_order(order);
            self.entities.replace(id, after);
            self.note_order(before, false);
            self.note_order(order, true);
        }
    }

    /// The space an entity is in.
    pub fn space_of(&self, id: EntityId) -> Option<LayoutId> {
        self.entities.record(id).map(|record| record.space)
    }

    /// Whether the entity is shown and in `space`.
    pub fn is_shown_in(&self, id: EntityId, space: LayoutId) -> bool {
        self.space_of(id) == Some(space) && self.is_shown(id)
    }

    /// Every entity of `space`, in dense storage order.
    pub fn entities_in(&self, space: LayoutId) -> impl Iterator<Item = (EntityId, &Entity)> {
        self.entities
            .records()
            .filter(move |(_, record)| record.space == space)
            .map(|(id, record)| (id, &*record.entity))
    }

    /// Every entity of `space` that is shown, in dense storage order.
    pub fn shown_in(&self, space: LayoutId) -> impl Iterator<Item = (EntityId, &Entity)> {
        self.entities
            .records()
            .filter(move |(_, record)| {
                record.space == space
                    && self
                        .layers
                        .get(record.properties.layer)
                        .is_some_and(Layer::is_shown)
            })
            .map(|(id, record)| (id, &*record.entity))
    }

    /// [`query_shown`](Self::query_shown) in `space`: what the pointer
    /// does there.
    pub fn query_shown_in(&self, space: LayoutId, bounds: &Aabb) -> Vec<EntityId> {
        let mut ids = self.query(bounds);
        ids.retain(|id| self.is_shown_in(*id, space));
        ids
    }

    /// The bounding box of the entities of `space`, or `None` when it
    /// has none.
    pub fn extents_in(&self, space: LayoutId) -> Option<Aabb> {
        self.entities_in(space)
            .filter(|(_, entity)| !entity.is_unbounded())
            .map(|(_, entity)| entity.bounds(self.tables()))
            .reduce(|a, b| a.union(&b))
    }

    /// The stored record of an entity: its geometry and its properties.
    pub fn record(&self, id: EntityId) -> Option<&EntityRecord> {
        self.entities.record(id)
    }

    /// Every entity with its properties, in dense storage order.
    pub fn records(&self) -> impl Iterator<Item = (EntityId, &EntityRecord)> {
        self.entities.records()
    }

    /// The layer table. It always holds layer `0`.
    pub fn layers(&self) -> &LayerTable {
        &self.layers
    }

    /// The linetype table. It always holds `Continuous`.
    pub fn linetypes(&self) -> &LinetypeTable {
        &self.linetypes
    }

    /// The text style table. It always holds `Standard`.
    pub fn text_styles(&self) -> &TextStyleTable {
        &self.text_styles
    }

    /// The dimension style table. It always holds `Standard`.
    pub fn dim_styles(&self) -> &DimStyleTable {
        &self.dim_styles
    }

    /// The style new dimensions are drawn by.
    pub fn current_dim_style(&self) -> DimStyleId {
        self.settings.current_dim_style
    }

    /// The style new texts take (#221).
    pub fn current_text_style(&self) -> TextStyleId {
        self.settings.current_text_style
    }

    /// The block table. It always holds `*Model_Space`, the drawing itself.
    /// The tables an entity's geometry is read through: the blocks a
    /// reference places and the styles a dimension is drawn by.
    pub fn tables(&self) -> Tables<'_> {
        Tables {
            blocks: &self.blocks,
            dim_styles: &self.dim_styles,
            mleader_styles: &self.mleader_styles,
            point_style: self.settings.point_style,
            wipeout_frame: self.settings.wipeout_frame,
        }
    }

    pub fn blocks(&self) -> &BlockTable {
        &self.blocks
    }

    /// The bounds of an entity of this drawing, as the spatial index holds
    /// them: a block reference's come from its definition.
    pub fn bounds_of(&self, entity: &Entity) -> Aabb {
        entity.bounds(self.tables())
    }

    /// The drawing's settings.
    pub fn settings(&self) -> DrawingSettings {
        self.settings
    }

    /// The layer new objects are put on. It always exists and is on.
    pub fn current_layer(&self) -> LayerId {
        self.settings.current_layer
    }

    /// The colour new objects are given.
    pub fn current_color(&self) -> Color {
        self.settings.current_color
    }

    /// The linetype new objects are given.
    pub fn current_linetype(&self) -> Linetype {
        self.settings.current_linetype
    }

    /// The lineweight new objects are given.
    pub fn current_lineweight(&self) -> Lineweight {
        self.settings.current_lineweight
    }

    /// What every linetype pattern is multiplied by.
    pub fn ltscale(&self) -> f64 {
        self.settings.ltscale
    }

    /// The drawing's unit, or `None` if it is unitless.
    pub fn unit(&self) -> Option<LengthUnit> {
        self.settings.unit
    }

    /// The same drawing with its unit set, and still no history: for a
    /// drawing being put together from a file.
    #[must_use]
    pub fn with_unit(mut self, unit: Option<LengthUnit>) -> Self {
        self.settings.unit = unit;
        self
    }

    /// The same drawing with all its settings set, and still no history:
    /// for a drawing being put together from a file. The settings are
    /// checked as a transaction would check them: the current layer must
    /// exist and be on, a named linetype must exist, `LTSCALE` must be
    /// positive.
    pub fn with_settings(mut self, settings: DrawingSettings) -> Result<Self, TransactionError> {
        let mut tx = self.transaction();
        tx.set_current_layer(settings.current_layer)?;
        tx.set_current_linetype(settings.current_linetype)?;
        tx.set_ltscale(settings.ltscale)?;
        tx.set_current_dim_style(settings.current_dim_style)?;
        tx.set_current_text_style(settings.current_text_style)?;
        tx.set_current_color(settings.current_color);
        tx.set_current_lineweight(settings.current_lineweight);
        tx.set_unit(settings.unit);
        tx.set_point_style(settings.point_style)?;
        tx.set_wipeout_frame(settings.wipeout_frame);
        tx.set_current_mleader_style(settings.current_mleader_style)?;
        tx.commit();
        self.history = History::default();
        Ok(self)
    }

    /// The layer an entity is on.
    pub fn layer_of(&self, id: EntityId) -> Option<&Layer> {
        self.layers.get(self.properties(id)?.layer)
    }

    /// Whether the entity is shown: it exists and its layer is on and not
    /// frozen. What is not shown is not there for the pointer either — not
    /// picked, not selected, not snapped to.
    pub fn is_shown(&self, id: EntityId) -> bool {
        self.layer_of(id).is_some_and(Layer::is_shown)
    }

    /// Whether the entity is protected from change: its layer is locked.
    pub fn is_locked(&self, id: EntityId) -> bool {
        self.layer_of(id).is_some_and(|layer| layer.locked)
    }

    /// Every entity of the model that is shown, in dense storage order.
    /// The model's, like [`query_shown`](Self::query_shown) and
    /// [`extents`](Self::extents): a layout's are read with
    /// [`shown_in`](Self::shown_in) (#191).
    pub fn shown(&self) -> impl Iterator<Item = (EntityId, &Entity)> {
        self.shown_in(LayoutId::MODEL)
    }

    /// [`query`](Self::query), keeping only entities of the model that are
    /// shown. This is the query for anything the pointer does there.
    pub fn query_shown(&self, bounds: &Aabb) -> Vec<EntityId> {
        self.query_shown_in(LayoutId::MODEL, bounds)
    }

    /// Candidate entities whose bounds intersect `bounds`. Callers follow up
    /// with an exact geometric test.
    pub fn query(&self, bounds: &Aabb) -> Vec<EntityId> {
        let mut ids = self.index.query(bounds);
        // The construction lines that cross the box (ADR-0031).
        ids.extend(self.unbounded.iter().copied().filter(|id| {
            self.entities
                .get(*id)
                .is_some_and(|entity| entity.crosses(bounds, self.tables()))
        }));
        ids
    }

    /// The construction lines of `space` that are shown (ADR-0031): what
    /// the overlay draws clipped to the view.
    pub fn unbounded_shown_in(&self, space: LayoutId) -> impl Iterator<Item = (EntityId, &Entity)> {
        self.unbounded.iter().filter_map(move |id| {
            self.is_shown_in(*id, space)
                .then(|| self.entities.get(*id).map(|entity| (*id, &**entity)))
                .flatten()
        })
    }

    /// The bounding box of the model's entities, or `None` when it has
    /// none.
    pub fn extents(&self) -> Option<Aabb> {
        self.extents_in(LayoutId::MODEL)
    }

    /// The raw value of the next entity ID. Monotonic for the lifetime of the
    /// drawing; never decreases, including through undo (DESIGN §10.2).
    pub fn next_entity_id(&self) -> u64 {
        self.ids.peek()
    }

    pub fn can_undo(&self) -> bool {
        self.history.can_undo()
    }

    /// How many transactions can be undone: for tests of what is one step.
    #[cfg(test)]
    fn history_len_for_test(&self) -> Option<usize> {
        Some(self.history.undo_len())
    }

    pub fn can_redo(&self) -> bool {
        self.history.can_redo()
    }

    /// Starts a transaction. Nothing changes until [`TransactionBuilder::commit`];
    /// dropping the builder abandons it.
    pub fn transaction(&mut self) -> TransactionBuilder<'_> {
        TransactionBuilder {
            drawing: self,
            changes: Vec::new(),
            touched: HashSet::new(),
            others: Vec::new(),
            space: LayoutId::MODEL,
        }
    }

    /// Reverts the most recent transaction. Returns `None` when there is
    /// nothing to undo.
    pub fn undo(&mut self) -> Option<ApplyReport> {
        let inverse = self.history.step_back()?.inverse();
        Some(self.apply(&inverse))
    }

    /// Re-applies the most recently undone transaction. Returns `None` when
    /// there is nothing to redo.
    pub fn redo(&mut self) -> Option<ApplyReport> {
        let transaction = self.history.step_forward()?.clone();
        Some(self.apply(&transaction))
    }

    /// Starts an undo group. Transactions committed until [`Drawing::end_group`]
    /// are undone and redone together, as one step. Commands use this so that
    /// one invocation is one undo step however many transactions it commits.
    pub fn begin_group(&mut self) {
        self.history.begin_group();
    }

    /// Ends the open undo group. A no-op when none is open.
    pub fn end_group(&mut self) {
        self.history.end_group();
    }

    /// Reverts the latest transaction committed inside the open undo group
    /// and forgets it: unlike [`Drawing::undo`] it cannot be redone. This is
    /// the "Undo" option *inside* a command. Returns `None` when the group
    /// holds nothing to roll back.
    pub fn rollback_last_in_group(&mut self) -> Option<ApplyReport> {
        let inverse = self.history.take_last_in_group()?.inverse();
        Some(self.apply(&inverse))
    }

    /// The single place where canonical state changes. Keeps the store, the
    /// tables, and the derived spatial index in step, then takes a new
    /// revision.
    fn apply(&mut self, transaction: &Transaction) -> ApplyReport {
        let mut report = ApplyReport::default();
        let mut shown_changed = false;
        // A layer switched on or off, frozen or thawed, changes what is
        // shown without naming an entity: everything may have changed.
        let mut everything = false;
        for change in transaction.changes() {
            shown_changed |= match change {
                Change::Entity(_) => true,
                Change::Layer(TableChange::Replace { before, after, .. }) => {
                    let switched = before.is_shown() != after.is_shown();
                    everything |= switched;
                    switched
                }
                // A layer that is made or removed has nothing on it.
                // A block redefined changes the geometry of every reference;
                // a dimension style changed, of every dimension drawn by it.
                Change::Block(TableChange::Replace { .. })
                | Change::DimStyle(TableChange::Replace { .. }) => true,
                // The point style drawn changes every point (#198), and
                // whether frames are shown every wipeout (#226).
                Change::Settings(change) => {
                    let restyled = change.before.point_style != change.after.point_style
                        || change.before.wipeout_frame != change.after.wipeout_frame;
                    everything |= restyled;
                    restyled
                }
                Change::Layer(_)
                | Change::Linetype(_)
                | Change::TextStyle(_)
                | Change::DimStyle(_)
                | Change::Block(_)
                | Change::Layout(_)
                // A group draws nothing (#214).
                | Change::Group(_)
                // Nor does a saved layer state (#228): restoring one
                // changes layers, which say so for themselves.
                | Change::LayerState(_)
                // Nor does a named view (#235).
                | Change::View(_) => false,
                // A multileader style changed, every multileader drawn by
                // it (#240).
                Change::MLeaderStyle(TableChange::Replace { .. }) => true,
                Change::MLeaderStyle(_) => false,
            };
            match change {
                Change::Entity(EntityChange::Insert { id, record }) => {
                    if record.entity.is_unbounded() {
                        self.unbounded.insert(*id);
                    } else {
                        self.index.insert(*id, record.entity.bounds(self.tables()));
                    }
                    self.entities
                        .insert(*id, record.clone())
                        .expect("transactions never insert a live ID");
                    self.note_radius(*id);
                    self.note_order(record.order, true);
                    report.inserted.push(*id);
                }
                Change::Entity(EntityChange::Remove { id, record }) => {
                    self.index.remove(*id);
                    self.unbounded.remove(id);
                    self.entities
                        .remove(*id)
                        .expect("transactions only remove live entities");
                    self.note_radius(*id);
                    self.note_order(record.order, false);
                    report.removed.push(*id);
                }
                Change::Entity(EntityChange::Replace { id, before, after }) => {
                    // A change of properties alone leaves the index as it is.
                    if !Arc::ptr_eq(&before.entity, &after.entity) {
                        // A trim can turn a ray into a line: the entry
                        // moves between the tree and the side list.
                        self.index.remove(*id);
                        self.unbounded.remove(id);
                        if after.entity.is_unbounded() {
                            self.unbounded.insert(*id);
                        } else {
                            self.index.insert(*id, after.entity.bounds(self.tables()));
                        }
                    }
                    self.entities
                        .replace(*id, after.clone())
                        .expect("transactions only replace live entities");
                    self.note_radius(*id);
                    self.note_order(before.order, false);
                    self.note_order(after.order, true);
                    report.replaced.push(*id);
                }
                Change::Layer(change) => {
                    self.layers.apply(change);
                    report.tables_changed = true;
                }
                Change::Linetype(change) => {
                    self.linetypes.apply(change);
                    report.tables_changed = true;
                }
                Change::TextStyle(change) => {
                    self.text_styles.apply(change);
                    report.tables_changed = true;
                }
                Change::Layout(change) => {
                    self.layouts.apply(change);
                    report.tables_changed = true;
                }
                Change::Group(change) => {
                    self.groups.apply(change);
                    report.tables_changed = true;
                }
                Change::LayerState(change) => {
                    self.layer_states.apply(change);
                    report.tables_changed = true;
                }
                Change::View(change) => {
                    self.views.apply(change);
                    report.tables_changed = true;
                }
                Change::MLeaderStyle(change) => {
                    self.mleader_styles.apply(change);
                    report.tables_changed = true;
                    // One row changed; every multileader drawn by it is
                    // laid out afresh, and its index entry with it.
                    if let TableChange::Replace { id, .. } = change {
                        let dependents: Vec<EntityId> = self
                            .entities
                            .iter()
                            .filter_map(|(entity_id, entity)| match &**entity {
                                Entity::MLeader(mleader) if mleader.style == *id => Some(entity_id),
                                _ => None,
                            })
                            .collect();
                        for entity_id in dependents {
                            if let Some(entity) = self.entities.get(entity_id) {
                                let bounds = entity.bounds(self.tables());
                                self.index.update(entity_id, bounds);
                            }
                            report.replaced.push(entity_id);
                        }
                    }
                }
                Change::DimStyle(change) => {
                    self.dim_styles.apply(change);
                    report.tables_changed = true;
                    // One change to the row; every dimension drawn by it is
                    // laid out afresh, so its index entry is (§19.6).
                    if let TableChange::Replace { id, .. } = change {
                        let dependents: Vec<EntityId> = self
                            .entities
                            .iter()
                            .filter_map(|(entity_id, entity)| match &**entity {
                                Entity::Dimension(dimension) if dimension.style == *id => {
                                    Some(entity_id)
                                }
                                Entity::Leader(leader) if leader.style == *id => Some(entity_id),
                                _ => None,
                            })
                            .collect();
                        for entity_id in dependents {
                            if let Some(entity) = self.entities.get(entity_id) {
                                let tables = Tables {
                                    blocks: &self.blocks,
                                    dim_styles: &self.dim_styles,
                                    mleader_styles: &self.mleader_styles,
                                    point_style: self.settings.point_style,
                                    wipeout_frame: self.settings.wipeout_frame,
                                };
                                self.index.update(entity_id, entity.bounds(tables));
                            }
                            self.note_radius(entity_id);
                            report.replaced.push(entity_id);
                        }
                    }
                }
                Change::Block(change) => {
                    self.blocks.apply(change);
                    report.tables_changed = true;
                    // One change to the row; derived state for every
                    // reference that places it, directly or through other
                    // blocks, is recomputed here (§19.6).
                    if let TableChange::Replace { id, .. } = change {
                        let dependents: Vec<EntityId> = self
                            .entities
                            .iter()
                            .filter_map(|(entity_id, entity)| match &**entity {
                                Entity::BlockRef(reference)
                                    if reference.block == *id
                                        || self.blocks.reaches(reference.block, *id) =>
                                {
                                    Some(entity_id)
                                }
                                _ => None,
                            })
                            .collect();
                        for entity_id in dependents {
                            if let Some(entity) = self.entities.get(entity_id) {
                                self.index.update(entity_id, entity.bounds(self.tables()));
                            }
                            self.note_radius(entity_id);
                            report.replaced.push(entity_id);
                        }
                    }
                }
                Change::Settings(change) => self.settings = change.after,
            }
        }
        self.revision = Revision::next();
        if shown_changed {
            let before = self.shown_revision;
            self.shown_revision = Revision::next();
            let changed = (!everything).then(|| {
                let mut ids: Vec<EntityId> = Vec::with_capacity(
                    report.inserted.len() + report.removed.len() + report.replaced.len(),
                );
                ids.extend(&report.inserted);
                ids.extend(&report.removed);
                ids.extend(&report.replaced);
                ids
            });
            self.journal
                .push_back((before, self.shown_revision, changed));
            if self.journal.len() > JOURNAL_LENGTH {
                self.journal.pop_front();
            }
        }
        report
    }
}

/// Collects changes for one atomic, undoable edit.
///
/// Each entity may be changed at most once per transaction, which keeps every
/// recorded change valid against the drawing as it was when the transaction
/// began — so commit cannot fail.
#[derive(Debug)]
pub struct TransactionBuilder<'d> {
    drawing: &'d mut Drawing,
    changes: Vec<Change>,
    /// The entities `changes` already touch, so that a transaction over
    /// many entities checks each in constant time rather than by scanning.
    touched: HashSet<EntityId>,
    /// Where in `changes` the table and settings changes are: the few a
    /// transaction over many entities has to look at for each of them.
    others: Vec<usize>,
    /// The space new entities go into (#191): the model, unless the
    /// transaction was made [`in_space`](Self::in_space).
    space: LayoutId,
}

impl TransactionBuilder<'_> {
    /// Adds a new entity and returns its ID. The ID is consumed even if the
    /// transaction is abandoned; sparse IDs are fine (DESIGN §10.2).
    ///
    /// The entity gets the properties new objects are given: the current
    /// layer, colour, linetype, and lineweight. An object made *from*
    /// another — a copy, an offset, the second piece of a break — takes
    /// that object's properties with [`insert_like`](Self::insert_like)
    /// instead.
    pub fn insert(&mut self, entity: impl Into<Entity>) -> Result<EntityId, TransactionError> {
        self.insert_in(self.space, entity)
    }

    /// The same transaction, with new entities going into `space` (#191):
    /// the session's current space, which the drawing does not know.
    #[must_use]
    pub fn in_space(mut self, space: LayoutId) -> Self {
        self.space = space;
        self
    }

    /// Adds a new entity with the given properties.
    pub fn insert_with(
        &mut self,
        entity: impl Into<Entity>,
        properties: Properties,
    ) -> Result<EntityId, TransactionError> {
        self.insert_with_in(self.space, entity, properties)
    }

    /// Adds a new entity with the properties of `source`, an entity of the
    /// drawing as it was when the transaction began.
    pub fn insert_like(
        &mut self,
        source: EntityId,
        entity: impl Into<Entity>,
    ) -> Result<EntityId, TransactionError> {
        let record = self
            .drawing
            .record(source)
            .ok_or(TransactionError::UnknownEntity(source))?;
        let (properties, space, order) = (record.properties, record.space, record.order);
        let entity = entity.into();
        self.known_text_style(&entity)?;
        self.known_block(&entity)?;
        viewport_placeable(&entity, space)?;
        let record = EntityRecord::new(entity, properties).in_space(space);
        Ok(self.insert_record(record.in_order(order)))
    }

    /// Adds a new entity to `space` — the model, or a layout — with the
    /// properties new objects take (#191).
    pub fn insert_in(
        &mut self,
        space: LayoutId,
        entity: impl Into<Entity>,
    ) -> Result<EntityId, TransactionError> {
        self.known_layout(space)?;
        let settings = self.settings_now();
        let properties = Properties {
            layer: settings.current_layer,
            color: settings.current_color,
            linetype: settings.current_linetype,
            lineweight: settings.current_lineweight,
        };
        let entity = entity.into();
        self.known_text_style(&entity)?;
        self.known_block(&entity)?;
        viewport_placeable(&entity, space)?;
        let record = EntityRecord::new(entity, properties).in_space(space);
        Ok(self.insert_record(record.in_order(self.drawing.top_order())))
    }

    /// Adds a new entity to `space` with the given properties.
    pub fn insert_with_in(
        &mut self,
        space: LayoutId,
        entity: impl Into<Entity>,
        properties: Properties,
    ) -> Result<EntityId, TransactionError> {
        self.known_layout(space)?;
        self.known_layer(properties.layer)?;
        self.known_linetype(properties.linetype)?;
        let entity = entity.into();
        self.known_text_style(&entity)?;
        self.known_block(&entity)?;
        viewport_placeable(&entity, space)?;
        let record = EntityRecord::new(entity, properties).in_space(space);
        Ok(self.insert_record(record.in_order(self.drawing.top_order())))
    }

    /// Adds a new entity over everything the drawing holds (#226): in a
    /// tier of the drawing order of its own above the top one, as a
    /// wipeout is made. The properties are those new objects take.
    pub fn insert_on_top(
        &mut self,
        entity: impl Into<Entity>,
    ) -> Result<EntityId, TransactionError> {
        let id = self.insert(entity)?;
        let Some(Change::Entity(EntityChange::Insert { record, .. })) = self.changes.last_mut()
        else {
            unreachable!("an insert was just recorded");
        };
        record.order = self.drawing.top_order().saturating_add(1);
        Ok(id)
    }

    /// Puts an existing entity in tier `order` of the drawing order (#226),
    /// keeping everything else about it.
    pub fn set_order(&mut self, id: EntityId, order: i16) -> Result<(), TransactionError> {
        let before = self.existing(id)?;
        let after = before.in_order(order);
        self.push(EntityChange::Replace { id, before, after });
        Ok(())
    }

    /// Moves an existing entity to `space`, keeping its ID, its geometry,
    /// and its properties: one change, undone as one.
    pub fn set_space(&mut self, id: EntityId, space: LayoutId) -> Result<(), TransactionError> {
        self.known_layout(space)?;
        let before = self.existing(id)?;
        viewport_placeable(&before.entity, space)?;
        let after = before.in_space(space);
        self.push(EntityChange::Replace { id, before, after });
        Ok(())
    }

    /// Adds a layout and returns its ID. Its name must be allowed and not
    /// taken.
    pub fn add_layout(&mut self, layout: Layout) -> Result<LayoutId, TransactionError> {
        check_name(&layout.name).map_err(TransactionError::BadLayoutName)?;
        self.free_layout_name(&layout.name, None)?;
        let id = self.drawing.layouts.allocate();
        self.push_other(Change::Layout(TableChange::Insert { id, row: layout }));
        Ok(id)
    }

    /// Renames a layout. The model keeps its name.
    pub fn rename_layout(
        &mut self,
        id: LayoutId,
        name: impl Into<String>,
    ) -> Result<(), TransactionError> {
        if id == LayoutId::MODEL {
            return Err(TransactionError::PermanentLayout);
        }
        let name = name.into();
        check_name(&name).map_err(TransactionError::BadLayoutName)?;
        let before = self
            .layout_now(id)
            .cloned()
            .ok_or(TransactionError::UnknownLayout(id))?;
        if !same_name(&before.name, &name) {
            self.free_layout_name(&name, Some(id))?;
        }
        let after = Layout {
            name,
            ..before.clone()
        };
        self.push_other(Change::Layout(TableChange::Replace { id, before, after }));
        Ok(())
    }

    /// Sets a layout's sheet, in millimetres (#194): what its view fits
    /// to and the page it plots on; the writer puts it in the layout's
    /// plot settings. The model has no sheet.
    pub fn set_layout_sheet(&mut self, id: LayoutId, size: Vec2) -> Result<(), TransactionError> {
        if id == LayoutId::MODEL {
            return Err(TransactionError::PermanentLayout);
        }
        if !Layout::sheet_size_allowed(size) {
            return Err(TransactionError::BadSheet);
        }
        let before = self
            .layout_now(id)
            .cloned()
            .ok_or(TransactionError::UnknownLayout(id))?;
        let after = before.clone().with_sheet(size);
        self.push_other(Change::Layout(TableChange::Replace { id, before, after }));
        Ok(())
    }

    /// Sets what a plot of a layout takes (#208): its area, scale, and
    /// turn, kept on the row and written into its plot settings. The model
    /// has none; settings a plot cannot follow are refused.
    pub fn set_layout_plot(
        &mut self,
        id: LayoutId,
        plot: LayoutPlot,
    ) -> Result<(), TransactionError> {
        if id == LayoutId::MODEL {
            return Err(TransactionError::PermanentLayout);
        }
        if !plot.is_valid() {
            return Err(TransactionError::BadSheet);
        }
        let before = self
            .layout_now(id)
            .cloned()
            .ok_or(TransactionError::UnknownLayout(id))?;
        let after = before.clone().with_plot(plot);
        self.push_other(Change::Layout(TableChange::Replace { id, before, after }));
        Ok(())
    }

    /// Removes a layout and everything on it, as one step. The model
    /// stays. An entity of the layout this transaction already touched
    /// is refused: remove the layout in a transaction of its own.
    pub fn remove_layout(&mut self, id: LayoutId) -> Result<(), TransactionError> {
        if id == LayoutId::MODEL {
            return Err(TransactionError::PermanentLayout);
        }
        let row = self
            .layout_now(id)
            .cloned()
            .ok_or(TransactionError::UnknownLayout(id))?;
        let on_it: Vec<EntityId> = self
            .drawing
            .records()
            .filter(|(_, record)| record.space == id)
            .map(|(id, _)| id)
            .collect();
        let pending = self.changes.iter().any(|change| match change {
            Change::Entity(EntityChange::Insert { record, .. }) => record.space == id,
            Change::Entity(EntityChange::Replace { after, .. }) => after.space == id,
            _ => false,
        });
        if pending || on_it.iter().any(|entity| self.touched.contains(entity)) {
            return Err(TransactionError::LayoutInUse(id));
        }
        for entity in on_it {
            self.remove(entity)?;
        }
        // Its named views go with it (#235).
        let views: Vec<NamedViewId> = self
            .drawing
            .views
            .iter()
            .filter(|(_, view)| view.space == id)
            .map(|(view, _)| view)
            .collect();
        for view in views {
            self.remove_view(view)?;
        }
        self.push_other(Change::Layout(TableChange::Remove { id, row }));
        Ok(())
    }

    /// Removes an existing entity. It leaves its groups in the same
    /// transaction, and a group it leaves empty goes too (#214).
    pub fn remove(&mut self, id: EntityId) -> Result<(), TransactionError> {
        let record = self.existing(id)?;
        self.push(EntityChange::Remove { id, record });
        for (group_id, group) in self.groups_now() {
            if group.contains(id) {
                let mut after = group.clone();
                after.members.retain(|member| *member != id);
                self.push_other(Change::Group(if after.members.is_empty() {
                    TableChange::Remove {
                        id: group_id,
                        row: group,
                    }
                } else {
                    TableChange::Replace {
                        id: group_id,
                        before: group,
                        after,
                    }
                }));
            }
        }
        Ok(())
    }

    /// Adds a group of existing entities and returns its ID (#214). A
    /// named group's name must be allowed and not taken; an unnamed one
    /// ([`Group::unnamed`]) is named `*A` and the first number no group
    /// has. Its members must be entities the drawing has and keeps, each
    /// given once, and at least one.
    pub fn add_group(&mut self, mut group: Group) -> Result<GroupId, TransactionError> {
        if group.name.is_empty() {
            group.name = self.free_unnamed_group_name();
        } else {
            self.check_group_name(&group.name, None)?;
        }
        self.check_members(&group.members)?;
        let id = self.drawing.groups.allocate();
        self.push_other(Change::Group(TableChange::Insert { id, row: group }));
        Ok(id)
    }

    /// Replaces a group by itself with something different: a name, a
    /// description, its members, whether it is selectable. The same rules
    /// hold as for [`add_group`](Self::add_group).
    pub fn replace_group(&mut self, id: GroupId, group: Group) -> Result<(), TransactionError> {
        let before = self
            .group_now(id)
            .cloned()
            .ok_or(TransactionError::UnknownGroup(id))?;
        if group.name != before.name {
            self.check_group_name(&group.name, Some(id))?;
        }
        self.check_members(&group.members)?;
        self.push_other(Change::Group(TableChange::Replace {
            id,
            before,
            after: group,
        }));
        Ok(())
    }

    /// Adds a multileader style (#240). Its name must be allowed and not
    /// taken, and its values ones a multileader can be drawn by.
    pub fn add_mleader_style(
        &mut self,
        style: MLeaderStyle,
    ) -> Result<MLeaderStyleId, TransactionError> {
        style.check().map_err(TransactionError::BadMLeaderStyle)?;
        self.check_mleader_style_name(&style.name, None)?;
        let id = self.drawing.mleader_styles.allocate();
        self.push_other(Change::MLeaderStyle(TableChange::Insert { id, row: style }));
        Ok(id)
    }

    /// Replaces a multileader style's values or name, keeping its ID and
    /// so every multileader drawn by it. `Standard` keeps its name.
    pub fn replace_mleader_style(
        &mut self,
        id: MLeaderStyleId,
        style: MLeaderStyle,
    ) -> Result<(), TransactionError> {
        style.check().map_err(TransactionError::BadMLeaderStyle)?;
        let before = self.existing_mleader_style(id)?;
        if style.name != before.name {
            if id == MLeaderStyleId::FIRST {
                return Err(TransactionError::PermanentMLeaderStyle);
            }
            self.check_mleader_style_name(&style.name, Some(id))?;
        }
        self.push_other(Change::MLeaderStyle(TableChange::Replace {
            id,
            before,
            after: style,
        }));
        Ok(())
    }

    /// Removes a multileader style nothing is drawn by and that is not the
    /// current one. `Standard` stays.
    pub fn remove_mleader_style(&mut self, id: MLeaderStyleId) -> Result<(), TransactionError> {
        if id == MLeaderStyleId::FIRST {
            return Err(TransactionError::PermanentMLeaderStyle);
        }
        let row = self.existing_mleader_style(id)?;
        let uses =
            |entity: &Entity| matches!(entity, Entity::MLeader(mleader) if mleader.style == id);
        let pending = self.changes.iter().any(|change| match change {
            Change::Entity(EntityChange::Insert { record, .. })
            | Change::Entity(EntityChange::Replace { after: record, .. }) => uses(&record.entity),
            _ => false,
        });
        if pending
            || self.settings_now().current_mleader_style == id
            || self.drawing.entities().any(|(_, entity)| uses(entity))
            || self.blocks_use(|record| uses(&record.entity))
        {
            return Err(TransactionError::MLeaderStyleInUse(id));
        }
        self.push_other(Change::MLeaderStyle(TableChange::Remove { id, row }));
        Ok(())
    }

    /// Makes `id` the style new multileaders are drawn by.
    pub fn set_current_mleader_style(
        &mut self,
        id: MLeaderStyleId,
    ) -> Result<(), TransactionError> {
        self.mleader_style_now(id)
            .ok_or(TransactionError::UnknownMLeaderStyle(id))?;
        self.set_settings(DrawingSettings {
            current_mleader_style: id,
            ..self.settings_now()
        });
        Ok(())
    }

    /// Saves a named view (#235). Its name must be allowed and not taken,
    /// its space must exist, and it must show something.
    pub fn add_view(&mut self, view: NamedView) -> Result<NamedViewId, TransactionError> {
        self.check_view(&view, None)?;
        let id = self.drawing.views.allocate();
        self.push_other(Change::View(TableChange::Insert { id, row: view }));
        Ok(id)
    }

    /// Replaces a named view: saved again, or renamed.
    pub fn replace_view(
        &mut self,
        id: NamedViewId,
        view: NamedView,
    ) -> Result<(), TransactionError> {
        let before = self.existing_view(id)?;
        self.check_view(&view, Some(id))?;
        self.push_other(Change::View(TableChange::Replace {
            id,
            before,
            after: view,
        }));
        Ok(())
    }

    /// Deletes a named view.
    pub fn remove_view(&mut self, id: NamedViewId) -> Result<(), TransactionError> {
        let row = self.existing_view(id)?;
        self.push_other(Change::View(TableChange::Remove { id, row }));
        Ok(())
    }

    /// Saves a layer state (#228). Its name must be allowed and not taken.
    pub fn add_layer_state(&mut self, state: LayerState) -> Result<LayerStateId, TransactionError> {
        self.check_layer_state_name(&state.name, None)?;
        let id = self.drawing.layer_states.allocate();
        self.push_other(Change::LayerState(TableChange::Insert { id, row: state }));
        Ok(id)
    }

    /// Replaces a layer state: renamed, described, or saved again.
    pub fn replace_layer_state(
        &mut self,
        id: LayerStateId,
        state: LayerState,
    ) -> Result<(), TransactionError> {
        let before = self.existing_layer_state(id)?;
        if state.name != before.name {
            self.check_layer_state_name(&state.name, Some(id))?;
        }
        self.push_other(Change::LayerState(TableChange::Replace {
            id,
            before,
            after: state,
        }));
        Ok(())
    }

    /// Removes a layer state; the layers stay as they are.
    pub fn remove_layer_state(&mut self, id: LayerStateId) -> Result<(), TransactionError> {
        let row = self.existing_layer_state(id)?;
        self.push_other(Change::LayerState(TableChange::Remove { id, row }));
        Ok(())
    }

    /// Sets every layer the state `id` names, and the drawing has, as the
    /// state keeps it; the rest stay as they are. A linetype the drawing
    /// no longer has leaves the layer's own. The current layer is never
    /// switched off or frozen: it keeps being shown. Returns how many
    /// layers changed.
    pub fn restore_layer_state(&mut self, id: LayerStateId) -> Result<usize, TransactionError> {
        let state = self
            .drawing
            .layer_states
            .get(id)
            .cloned()
            .ok_or(TransactionError::UnknownLayerState(id))?;
        let current = self.settings_now().current_layer;
        let layers: Vec<(LayerId, Layer)> = self
            .drawing
            .layers
            .iter()
            .map(|(id, layer)| (id, layer.clone()))
            .collect();
        let mut changed = 0;
        for (layer_id, layer) in layers {
            let Some(saved) = state.layer(&layer.name) else {
                continue;
            };
            let linetype = self
                .drawing
                .linetypes
                .find(&saved.linetype)
                .map_or(layer.linetype, Linetype::Named);
            let shown = layer_id == current;
            let after = Layer {
                on: saved.on || shown,
                frozen: saved.frozen && !shown,
                locked: saved.locked,
                color: saved.color,
                linetype,
                lineweight: saved.lineweight,
                ..layer.clone()
            };
            if after != layer {
                self.replace_layer(layer_id, after)?;
                changed += 1;
            }
        }
        Ok(changed)
    }

    /// Removes a group; its members stay.
    pub fn remove_group(&mut self, id: GroupId) -> Result<(), TransactionError> {
        let row = self
            .group_now(id)
            .cloned()
            .ok_or(TransactionError::UnknownGroup(id))?;
        self.push_other(Change::Group(TableChange::Remove { id, row }));
        Ok(())
    }

    /// Replaces an existing entity's geometry, keeping its ID and its
    /// properties.
    pub fn replace(
        &mut self,
        id: EntityId,
        entity: impl Into<Entity>,
    ) -> Result<(), TransactionError> {
        let entity = entity.into();
        self.known_text_style(&entity)?;
        self.known_block(&entity)?;
        let before = self.existing(id)?;
        viewport_placeable(&entity, before.space)?;
        let after = before.with_entity(entity);
        self.push(EntityChange::Replace { id, before, after });
        Ok(())
    }

    /// Changes an existing entity's properties, keeping its ID and sharing
    /// its geometry.
    pub fn set_properties(
        &mut self,
        id: EntityId,
        properties: Properties,
    ) -> Result<(), TransactionError> {
        self.known_layer(properties.layer)?;
        self.known_linetype(properties.linetype)?;
        let before = self.existing(id)?;
        let after = before.with_properties(properties);
        self.push(EntityChange::Replace { id, before, after });
        Ok(())
    }

    /// Replaces an existing entity's geometry and its properties at once
    /// (`MATCHPROP`, #200), keeping its ID.
    pub fn replace_with(
        &mut self,
        id: EntityId,
        entity: impl Into<Entity>,
        properties: Properties,
    ) -> Result<(), TransactionError> {
        let entity = entity.into();
        self.known_text_style(&entity)?;
        self.known_block(&entity)?;
        self.known_layer(properties.layer)?;
        self.known_linetype(properties.linetype)?;
        let before = self.existing(id)?;
        viewport_placeable(&entity, before.space)?;
        let after = before.with_entity(entity).with_properties(properties);
        self.push(EntityChange::Replace { id, before, after });
        Ok(())
    }

    /// Renames a layer (`RENAME`, #200): its row with the new name, every
    /// entity on it untouched. Layer `0` keeps its name.
    pub fn rename_layer(
        &mut self,
        id: LayerId,
        name: impl Into<String>,
    ) -> Result<(), TransactionError> {
        let layer = Layer {
            name: name.into(),
            ..self.existing_layer(id)?
        };
        self.replace_layer(id, layer)
    }

    /// Renames a linetype: its row with the new name, which must be allowed
    /// and not taken. `Continuous` keeps its name.
    pub fn rename_linetype(
        &mut self,
        id: LinetypeId,
        name: impl Into<String>,
    ) -> Result<(), TransactionError> {
        if id == LinetypeId::FIRST {
            return Err(TransactionError::PermanentLinetype);
        }
        if self.linetype_change(id).is_some() {
            return Err(TransactionError::LinetypeAlreadyChanged(id));
        }
        let before = self
            .drawing
            .linetypes
            .get(id)
            .cloned()
            .ok_or(TransactionError::UnknownLinetype(id))?;
        let after = LinetypeDef {
            name: name.into(),
            ..before.clone()
        };
        after.check().map_err(TransactionError::BadLinetype)?;
        if !same_name(&after.name, &before.name) {
            let added = self.others().filter_map(|change| match change {
                Change::Linetype(TableChange::Insert { id, row }) => Some((*id, row)),
                _ => None,
            });
            let taken = self
                .drawing
                .linetypes
                .iter()
                .filter(|(other, _)| *other != id)
                .filter_map(|(other, _)| self.linetype_now(other).map(|row| (other, row)))
                .chain(added)
                .find(|(_, row)| same_name(&row.name, &after.name));
            if let Some((other, _)) = taken {
                return Err(TransactionError::DuplicateLinetypeName(other));
            }
        }
        self.push_other(Change::Linetype(TableChange::Replace { id, before, after }));
        Ok(())
    }

    /// Renames a text style: its row with the new name, which must be
    /// allowed and not taken. `Standard` keeps its name.
    pub fn rename_text_style(
        &mut self,
        id: TextStyleId,
        name: impl Into<String>,
    ) -> Result<(), TransactionError> {
        if id == TextStyleId::FIRST {
            return Err(TransactionError::PermanentTextStyle);
        }
        let before = self
            .text_style_now(id)
            .cloned()
            .ok_or(TransactionError::UnknownTextStyle(id))?;
        let name = name.into();
        check_name(&name).map_err(TransactionError::BadTextStyleName)?;
        if !same_name(&name, &before.name) {
            let added = self.others().filter_map(|change| match change {
                Change::TextStyle(TableChange::Insert { id, row }) => Some((*id, row)),
                _ => None,
            });
            let taken = self
                .drawing
                .text_styles
                .iter()
                .filter(|(other, _)| *other != id)
                .filter_map(|(other, _)| self.text_style_now(other).map(|row| (other, row)))
                .chain(added)
                .find(|(_, row)| same_name(&row.name, &name));
            if let Some((other, _)) = taken {
                return Err(TransactionError::DuplicateTextStyleName(other));
            }
        }
        let after = TextStyle {
            name,
            ..before.clone()
        };
        self.push_other(Change::TextStyle(TableChange::Replace {
            id,
            before,
            after,
        }));
        Ok(())
    }

    /// Renames a block: its definition with the new name, every reference
    /// untouched.
    pub fn rename_block(
        &mut self,
        id: BlockId,
        name: impl Into<String>,
    ) -> Result<(), TransactionError> {
        let block = Block {
            name: name.into(),
            ..self
                .block_now(id)
                .cloned()
                .ok_or(TransactionError::UnknownBlock(id))?
        };
        self.redefine_block(id, block)
    }

    /// Renames a dimension style: its row with the new name, every
    /// dimension drawn by it untouched. `Standard` keeps its name.
    pub fn rename_dim_style(
        &mut self,
        id: DimStyleId,
        name: impl Into<String>,
    ) -> Result<(), TransactionError> {
        let style = DimStyle {
            name: name.into(),
            ..self
                .dim_style_now(id)
                .cloned()
                .ok_or(TransactionError::UnknownDimStyle(id))?
        };
        self.replace_dim_style(id, style)
    }

    /// Removes every row of the tables in `scope` that nothing uses
    /// (`PURGE`, #200) — not the permanent rows, not the current ones, not
    /// one an entity, a layer, a block's entity, or another block refers
    /// to — and says which went, by name. Blocks go first, and again while
    /// removing one frees another it placed; layers before linetypes,
    /// since a layer may hold a linetype's only use.
    pub fn purge(&mut self, scope: PurgeScope) -> Purged {
        let mut purged = Purged::default();
        if scope.blocks {
            loop {
                let candidates: Vec<(BlockId, String)> = self
                    .drawing
                    .blocks
                    .iter()
                    .filter_map(|(id, _)| self.block_now(id).map(|block| (id, block.name.clone())))
                    .collect();
                let before = purged.blocks.len();
                for (id, name) in candidates {
                    if self.remove_block(id).is_ok() {
                        purged.blocks.push(name);
                    }
                }
                if purged.blocks.len() == before {
                    break;
                }
            }
        }
        if scope.text_styles {
            let candidates: Vec<(TextStyleId, String)> = self
                .drawing
                .text_styles
                .iter()
                .filter_map(|(id, _)| self.text_style_now(id).map(|row| (id, row.name.clone())))
                .collect();
            for (id, name) in candidates {
                if self.remove_text_style(id).is_ok() {
                    purged.text_styles.push(name);
                }
            }
        }
        if scope.dim_styles {
            let candidates: Vec<(DimStyleId, String)> = self
                .drawing
                .dim_styles
                .iter()
                .filter_map(|(id, _)| self.dim_style_now(id).map(|row| (id, row.name.clone())))
                .collect();
            for (id, name) in candidates {
                if self.remove_dim_style(id).is_ok() {
                    purged.dim_styles.push(name);
                }
            }
        }
        if scope.layers {
            let candidates: Vec<(LayerId, String)> = self
                .drawing
                .layers
                .iter()
                .filter_map(|(id, _)| self.layer_now(id).map(|row| (id, row.name.clone())))
                .collect();
            for (id, name) in candidates {
                if self.remove_layer(id).is_ok() {
                    purged.layers.push(name);
                }
            }
        }
        if scope.linetypes {
            let candidates: Vec<(LinetypeId, String)> = self
                .drawing
                .linetypes
                .iter()
                .filter_map(|(id, _)| self.linetype_now(id).map(|row| (id, row.name.clone())))
                .collect();
            for (id, name) in candidates {
                if self.remove_linetype(id).is_ok() {
                    purged.linetypes.push(name);
                }
            }
        }
        purged
    }

    /// Whether an entity inside a block — as the drawing holds the blocks,
    /// less those this transaction removes, plus those it adds or
    /// redefines — satisfies `uses`.
    fn blocks_use(&self, uses: impl Fn(&EntityRecord) -> bool) -> bool {
        let added = self.others().filter_map(|change| match change {
            Change::Block(TableChange::Insert { row, .. }) => Some(row),
            _ => None,
        });
        self.drawing
            .blocks
            .iter()
            .filter_map(|(id, _)| self.block_now(id))
            .chain(added)
            .any(|block| block.entities.iter().any(&uses))
    }

    /// Adds a layer and returns its ID. Its name must be allowed and not
    /// taken, and its linetype must exist. Like an entity's, the ID is
    /// consumed even if the transaction is abandoned.
    pub fn add_layer(&mut self, layer: Layer) -> Result<LayerId, TransactionError> {
        self.free_name(&layer.name, None)?;
        self.known_linetype(layer.linetype)?;
        let id = self.drawing.layers.allocate();
        self.push_other(Change::Layer(TableChange::Insert { id, row: layer }));
        Ok(id)
    }

    /// Replaces an existing layer's row — its name or any of its settings —
    /// keeping its ID, and so every entity on it.
    ///
    /// Layer `0` keeps its name, the current layer stays on, and the
    /// layer's linetype must exist.
    pub fn replace_layer(&mut self, id: LayerId, layer: Layer) -> Result<(), TransactionError> {
        let before = self.existing_layer(id)?;
        if layer.name != before.name {
            if id == LayerId::FIRST {
                return Err(TransactionError::PermanentLayer);
            }
            self.free_name(&layer.name, Some(id))?;
        }
        self.known_linetype(layer.linetype)?;
        if !layer.is_shown() && self.settings_now().current_layer == id {
            return Err(TransactionError::CurrentLayer(id));
        }
        self.push_other(Change::Layer(TableChange::Replace {
            id,
            before,
            after: layer,
        }));
        Ok(())
    }

    /// Moves every object on the layers `from` onto `into` — in the
    /// drawing and in block definitions, keeping their other properties —
    /// and removes those layers (`LAYMRG`, #236). Layer `0` and the
    /// current layer cannot be merged away, nor a layer into itself.
    /// Returns how many objects moved, in the drawing and in blocks.
    pub fn merge_layers(
        &mut self,
        from: &[LayerId],
        into: LayerId,
    ) -> Result<usize, TransactionError> {
        self.known_layer(into)?;
        self.check_layers_going(from)?;
        if from.contains(&into) {
            return Err(TransactionError::MergedIntoItself(into));
        }
        let moved = self.rework_layers(from, |record| {
            Some(record.with_properties(Properties {
                layer: into,
                ..record.properties
            }))
        })?;
        self.drop_layers(from)?;
        Ok(moved)
    }

    /// Erases every object on the layers `layers` — in the drawing and in
    /// block definitions — and removes those layers (`LAYDEL`, #236).
    /// Layer `0` and the current layer cannot be deleted. Returns how many
    /// objects were erased.
    pub fn delete_layers(&mut self, layers: &[LayerId]) -> Result<usize, TransactionError> {
        self.check_layers_going(layers)?;
        let erased = self.rework_layers(layers, |_| None)?;
        self.drop_layers(layers)?;
        Ok(erased)
    }

    /// Refuses layer `0`, the current layer, and one that is not there.
    fn check_layers_going(&self, layers: &[LayerId]) -> Result<(), TransactionError> {
        for id in layers {
            if *id == LayerId::FIRST {
                return Err(TransactionError::PermanentLayer);
            }
            self.existing_layer(*id)?;
            if self.settings_now().current_layer == *id {
                return Err(TransactionError::CurrentLayer(*id));
            }
        }
        Ok(())
    }

    /// Gives every object on `layers`, in the drawing and in block
    /// definitions, what `rework` makes of it — another record, or `None`
    /// to erase it. Returns how many objects it touched.
    fn rework_layers(
        &mut self,
        layers: &[LayerId],
        rework: impl Fn(&EntityRecord) -> Option<EntityRecord>,
    ) -> Result<usize, TransactionError> {
        let on = |record: &EntityRecord| layers.contains(&record.properties.layer);
        let ids: Vec<EntityId> = self
            .drawing
            .records()
            .filter(|(_, record)| on(record))
            .map(|(id, _)| id)
            .collect();
        let mut count = ids.len();
        for id in ids {
            let before = self.existing(id)?;
            match rework(&before) {
                Some(after) => self.push(EntityChange::Replace { id, before, after }),
                None => self.remove(id)?,
            }
        }
        let blocks: Vec<(BlockId, Block)> = self
            .drawing
            .blocks
            .iter()
            .filter(|(id, block)| *id != BlockId::FIRST && block.entities.iter().any(on))
            .map(|(id, block)| (id, block.clone()))
            .collect();
        for (id, block) in blocks {
            count += block.entities.iter().filter(|record| on(record)).count();
            let entities: Vec<EntityRecord> = block
                .entities
                .iter()
                .filter_map(|record| {
                    if on(record) {
                        rework(record)
                    } else {
                        Some(record.clone())
                    }
                })
                .collect();
            self.redefine_block(
                id,
                Block {
                    entities: Arc::new(entities),
                    ..block
                },
            )?;
        }
        Ok(count)
    }

    /// Removes layers this transaction has emptied: none of the drawing's
    /// entities, as it will be, nor any block's, is on them.
    fn drop_layers(&mut self, layers: &[LayerId]) -> Result<(), TransactionError> {
        for id in layers {
            let row = self.existing_layer(*id)?;
            self.push_other(Change::Layer(TableChange::Remove { id: *id, row }));
        }
        Ok(())
    }

    /// Removes a layer that has no entities on it and is not the current
    /// one. Layer `0` stays.
    pub fn remove_layer(&mut self, id: LayerId) -> Result<(), TransactionError> {
        if id == LayerId::FIRST {
            return Err(TransactionError::PermanentLayer);
        }
        let row = self.existing_layer(id)?;
        if self.settings_now().current_layer == id {
            return Err(TransactionError::CurrentLayer(id));
        }
        // In use by an entity of the drawing, or by one this transaction
        // puts on it. (An entity this transaction takes *off* it still
        // counts: remove the layer in a transaction of its own.)
        let on_layer = |record: &EntityRecord| record.properties.layer == id;
        let pending = self.changes.iter().any(|change| match change {
            Change::Entity(EntityChange::Insert { record, .. }) => on_layer(record),
            Change::Entity(EntityChange::Replace { after, .. }) => on_layer(after),
            Change::Entity(EntityChange::Remove { .. })
            | Change::Layer(_)
            | Change::Linetype(_)
            | Change::TextStyle(_)
            | Change::DimStyle(_)
            | Change::Block(_)
            | Change::Layout(_)
            | Change::Group(_)
            | Change::LayerState(_)
            | Change::View(_)
            | Change::MLeaderStyle(_)
            | Change::Settings(_) => false,
        });
        if pending
            || self.drawing.records().any(|(_, record)| on_layer(record))
            || self.blocks_use(on_layer)
        {
            return Err(TransactionError::LayerInUse(id));
        }
        self.push_other(Change::Layer(TableChange::Remove { id, row }));
        Ok(())
    }

    /// Makes `id` the layer new objects are put on. It must exist, be on,
    /// and not be frozen.
    pub fn set_current_layer(&mut self, id: LayerId) -> Result<(), TransactionError> {
        let layer = self
            .layer_now(id)
            .ok_or(TransactionError::UnknownLayer(id))?;
        if !layer.on {
            return Err(TransactionError::LayerIsOff(id));
        }
        if layer.frozen {
            return Err(TransactionError::LayerIsFrozen(id));
        }
        self.set_settings(DrawingSettings {
            current_layer: id,
            ..self.settings_now()
        });
        Ok(())
    }

    /// Sets the colour new objects are given. `ByLayer`, the default, means
    /// each takes its layer's.
    pub fn set_current_color(&mut self, color: Color) {
        self.set_settings(DrawingSettings {
            current_color: color,
            ..self.settings_now()
        });
    }

    /// Sets the lineweight new objects are given. `ByLayer`, the default,
    /// means each takes its layer's.
    pub fn set_current_lineweight(&mut self, lineweight: Lineweight) {
        self.set_settings(DrawingSettings {
            current_lineweight: lineweight,
            ..self.settings_now()
        });
    }

    /// Loads a linetype and returns its ID. It must be valid and its name
    /// not taken. Like a layer's, the ID is consumed even if the
    /// transaction is abandoned.
    pub fn add_linetype(&mut self, linetype: LinetypeDef) -> Result<LinetypeId, TransactionError> {
        linetype.check().map_err(TransactionError::BadLinetype)?;
        let added = self.others().filter_map(|change| match change {
            Change::Linetype(TableChange::Insert { row, .. }) => Some(row),
            _ => None,
        });
        let taken = self
            .drawing
            .linetypes
            .iter()
            .filter(|(id, _)| self.linetype_now(*id).is_some())
            .map(|(id, row)| (Some(id), row))
            .chain(added.map(|row| (None, row)))
            .find(|(_, row)| same_name(&row.name, &linetype.name));
        if let Some((id, _)) = taken {
            // A row added earlier in this transaction has no ID to report
            // that the drawing knows; its name will do.
            return Err(TransactionError::DuplicateLinetypeName(
                id.unwrap_or(LinetypeId::FIRST),
            ));
        }
        let id = self.drawing.linetypes.allocate();
        self.push_other(Change::Linetype(TableChange::Insert { id, row: linetype }));
        Ok(id)
    }

    /// Removes a linetype that nothing refers to: no entity, no layer, and
    /// not the setting for new objects. `Continuous` stays.
    pub fn remove_linetype(&mut self, id: LinetypeId) -> Result<(), TransactionError> {
        if id == LinetypeId::FIRST {
            return Err(TransactionError::PermanentLinetype);
        }
        if self.linetype_change(id).is_some() {
            return Err(TransactionError::LinetypeAlreadyChanged(id));
        }
        let row = self
            .drawing
            .linetypes
            .get(id)
            .cloned()
            .ok_or(TransactionError::UnknownLinetype(id))?;
        let named = Linetype::Named(id);
        // In use in the drawing, or by what this transaction adds or
        // changes. (What this transaction takes *off* it still counts:
        // remove the linetype in a transaction of its own.)
        let pending = self.changes.iter().any(|change| match change {
            Change::Entity(EntityChange::Insert { record, .. }) => {
                record.properties.linetype == named
            }
            Change::Entity(EntityChange::Replace { after, .. }) => {
                after.properties.linetype == named
            }
            Change::Layer(TableChange::Insert { row, .. }) => row.linetype == named,
            Change::Layer(TableChange::Replace { after, .. }) => after.linetype == named,
            _ => false,
        });
        let in_use = pending
            || self.settings_now().current_linetype == named
            || self
                .drawing
                .records()
                .any(|(_, record)| record.properties.linetype == named)
            || self
                .drawing
                .layers
                .iter()
                .filter_map(|(id, _)| self.layer_now(id))
                .any(|layer| layer.linetype == named)
            || self.blocks_use(|record| record.properties.linetype == named);
        if in_use {
            return Err(TransactionError::LinetypeInUse(id));
        }
        self.push_other(Change::Linetype(TableChange::Remove { id, row }));
        Ok(())
    }

    /// Sets the linetype new objects are given. `ByLayer`, the default,
    /// means each takes its layer's. A named linetype must exist.
    pub fn set_current_linetype(&mut self, linetype: Linetype) -> Result<(), TransactionError> {
        self.known_linetype(linetype)?;
        self.set_settings(DrawingSettings {
            current_linetype: linetype,
            ..self.settings_now()
        });
        Ok(())
    }

    /// Sets what every linetype pattern is multiplied by. It must be a
    /// positive number.
    pub fn set_ltscale(&mut self, ltscale: f64) -> Result<(), TransactionError> {
        if !(ltscale.is_finite() && ltscale > 0.0) {
            return Err(TransactionError::BadLtscale);
        }
        self.set_settings(DrawingSettings {
            ltscale,
            ..self.settings_now()
        });
        Ok(())
    }

    /// Sets how every point is drawn (#198): one change, however many
    /// points there are.
    pub fn set_point_style(&mut self, style: PointStyle) -> Result<(), TransactionError> {
        style.check().map_err(TransactionError::BadPointStyle)?;
        self.set_settings(DrawingSettings {
            point_style: style,
            ..self.settings_now()
        });
        Ok(())
    }

    /// Shows or hides every wipeout's frame (#226): one change, however
    /// many wipeouts there are.
    pub fn set_wipeout_frame(&mut self, shown: bool) {
        self.set_settings(DrawingSettings {
            wipeout_frame: shown,
            ..self.settings_now()
        });
    }

    /// Says what the drawing's numbers are numbers of; `None` makes it
    /// unitless. No geometry changes: the unit is what the numbers mean.
    pub fn set_unit(&mut self, unit: Option<LengthUnit>) {
        self.set_settings(DrawingSettings {
            unit,
            ..self.settings_now()
        });
    }

    /// Adds a text style and returns its ID. Its name must be allowed and
    /// not taken.
    pub fn add_text_style(&mut self, style: TextStyle) -> Result<TextStyleId, TransactionError> {
        check_name(&style.name).map_err(TransactionError::BadTextStyleName)?;
        style.check().map_err(TransactionError::BadTextStyle)?;
        let added = self.others().filter_map(|change| match change {
            Change::TextStyle(TableChange::Insert { id, row }) => Some((*id, row)),
            _ => None,
        });
        let taken = self
            .drawing
            .text_styles
            .iter()
            .filter(|(id, _)| self.text_style_now(*id).is_some())
            .chain(added)
            .find(|(_, row)| same_name(&row.name, &style.name));
        if let Some((id, _)) = taken {
            return Err(TransactionError::DuplicateTextStyleName(id));
        }
        let id = self.drawing.text_styles.allocate();
        self.push_other(Change::TextStyle(TableChange::Insert { id, row: style }));
        Ok(id)
    }

    /// Changes a text style's values — the height, width factor, and
    /// oblique angle new texts take (#221) — keeping its name. No text
    /// already made changes: a text keeps its own width and slant.
    pub fn set_text_style_values(
        &mut self,
        id: TextStyleId,
        values: TextStyle,
    ) -> Result<(), TransactionError> {
        let before = self
            .text_style_now(id)
            .cloned()
            .ok_or(TransactionError::UnknownTextStyle(id))?;
        let after = TextStyle {
            name: before.name.clone(),
            ..values
        };
        after.check().map_err(TransactionError::BadTextStyle)?;
        self.push_other(Change::TextStyle(TableChange::Replace {
            id,
            before,
            after,
        }));
        Ok(())
    }

    /// Sets the style new texts take. It must exist.
    pub fn set_current_text_style(&mut self, id: TextStyleId) -> Result<(), TransactionError> {
        if self.text_style_now(id).is_none() {
            return Err(TransactionError::UnknownTextStyle(id));
        }
        self.set_settings(DrawingSettings {
            current_text_style: id,
            ..self.settings_now()
        });
        Ok(())
    }

    /// Removes a text style no text has, and new texts do not take.
    /// `Standard` stays.
    pub fn remove_text_style(&mut self, id: TextStyleId) -> Result<(), TransactionError> {
        if id == TextStyleId::FIRST {
            return Err(TransactionError::PermanentTextStyle);
        }
        let row = self
            .text_style_now(id)
            .cloned()
            .ok_or(TransactionError::UnknownTextStyle(id))?;
        if self.settings_now().current_text_style == id {
            return Err(TransactionError::TextStyleInUse(id));
        }
        let uses = |entity: &Entity| match entity {
            Entity::Text(text) => text.style == id,
            Entity::MText(text) => text.style == id,
            Entity::AttributeDefinition(definition) => definition.text.style == id,
            _ => false,
        };
        let pending = self.changes.iter().any(|change| match change {
            Change::Entity(EntityChange::Insert { record, .. }) => uses(&record.entity),
            Change::Entity(EntityChange::Replace { after, .. }) => uses(&after.entity),
            _ => false,
        });
        if pending
            || self.drawing.entities().any(|(_, entity)| uses(entity))
            || self.blocks_use(|record| uses(&record.entity))
        {
            return Err(TransactionError::TextStyleInUse(id));
        }
        self.push_other(Change::TextStyle(TableChange::Remove { id, row }));
        Ok(())
    }

    /// Adds a dimension style and returns its ID. Its name must be allowed
    /// and not taken, and its values ones a dimension can be drawn by.
    pub fn add_dim_style(&mut self, style: DimStyle) -> Result<DimStyleId, TransactionError> {
        check_name(&style.name).map_err(TransactionError::BadDimStyleName)?;
        style.check().map_err(TransactionError::BadDimStyle)?;
        self.free_dim_style_name(&style.name, None)?;
        let id = self.drawing.dim_styles.allocate();
        self.push_other(Change::DimStyle(TableChange::Insert { id, row: style }));
        Ok(id)
    }

    /// Replaces a dimension style's row — its values, or its name —
    /// keeping its ID, and so every dimension drawn by it: one change,
    /// however many (§19.6). `Standard` may change its values and not its
    /// name.
    pub fn replace_dim_style(
        &mut self,
        id: DimStyleId,
        style: DimStyle,
    ) -> Result<(), TransactionError> {
        let before = self
            .dim_style_now(id)
            .cloned()
            .ok_or(TransactionError::UnknownDimStyle(id))?;
        style.check().map_err(TransactionError::BadDimStyle)?;
        if !same_name(&style.name, &before.name) {
            if id == DimStyleId::FIRST {
                return Err(TransactionError::PermanentDimStyle);
            }
            check_name(&style.name).map_err(TransactionError::BadDimStyleName)?;
            self.free_dim_style_name(&style.name, Some(id))?;
        }
        self.push_other(Change::DimStyle(TableChange::Replace {
            id,
            before,
            after: style,
        }));
        Ok(())
    }

    /// Removes a dimension style that is not `Standard`, not the one new
    /// dimensions take, and not one any dimension is drawn by.
    pub fn remove_dim_style(&mut self, id: DimStyleId) -> Result<(), TransactionError> {
        if id == DimStyleId::FIRST {
            return Err(TransactionError::PermanentDimStyle);
        }
        let row = self
            .dim_style_now(id)
            .cloned()
            .ok_or(TransactionError::UnknownDimStyle(id))?;
        if self.settings_now().current_dim_style == id {
            return Err(TransactionError::DimStyleInUse(id));
        }
        let uses = |entity: &Entity| match entity {
            Entity::Dimension(dimension) => dimension.style == id,
            Entity::Leader(leader) => leader.style == id,
            _ => false,
        };
        let pending = self.changes.iter().any(|change| match change {
            Change::Entity(EntityChange::Insert { record, .. }) => uses(&record.entity),
            Change::Entity(EntityChange::Replace { after, .. }) => uses(&after.entity),
            _ => false,
        });
        if pending
            || self
                .drawing
                .records()
                .any(|(_, record)| uses(&record.entity))
            || self.blocks_use(|record| uses(&record.entity))
        {
            return Err(TransactionError::DimStyleInUse(id));
        }
        self.push_other(Change::DimStyle(TableChange::Remove { id, row }));
        Ok(())
    }

    /// Sets the style new dimensions are drawn by. It must exist.
    pub fn set_current_dim_style(&mut self, id: DimStyleId) -> Result<(), TransactionError> {
        if self.dim_style_now(id).is_none() {
            return Err(TransactionError::UnknownDimStyle(id));
        }
        self.set_settings(DrawingSettings {
            current_dim_style: id,
            ..self.settings_now()
        });
        Ok(())
    }

    /// Refuses a dimension style name another row has, `except` the row
    /// being renamed.
    fn free_dim_style_name(
        &self,
        name: &str,
        except: Option<DimStyleId>,
    ) -> Result<(), TransactionError> {
        let taken = self
            .drawing
            .dim_styles
            .iter()
            .map(|(id, _)| id)
            .chain(self.others().filter_map(|change| match change {
                Change::DimStyle(TableChange::Insert { id, .. }) => Some(*id),
                _ => None,
            }))
            .filter(|id| Some(*id) != except)
            .find(|id| {
                self.dim_style_now(*id)
                    .is_some_and(|row| same_name(&row.name, name))
            });
        match taken {
            Some(id) => Err(TransactionError::DuplicateDimStyleName(id)),
            None => Ok(()),
        }
    }

    /// The dimension style `id` as it will be once this transaction commits.
    fn dim_style_now(&self, id: DimStyleId) -> Option<&DimStyle> {
        let change = self.others().rev().find_map(|change| match change {
            Change::DimStyle(change) if change.id() == id => Some(change),
            _ => None,
        });
        match change {
            Some(TableChange::Remove { .. }) => None,
            Some(TableChange::Insert { row, .. }) => Some(row),
            Some(TableChange::Replace { after, .. }) => Some(after),
            None => self.drawing.dim_styles.get(id),
        }
    }

    /// Defines a block and returns its ID. Its name must be allowed and
    /// not taken, and its entities must be ones the drawing could hold:
    /// on layers that exist, with linetypes, text styles, and blocks that
    /// exist — and no block that would hold this one.
    pub fn define_block(&mut self, block: Block) -> Result<BlockId, TransactionError> {
        self.free_block_name(&block.name, None)?;
        let id = self.drawing.blocks.allocate();
        self.check_block_contents(id, &block)?;
        self.push_other(Change::Block(TableChange::Insert { id, row: block }));
        Ok(id)
    }

    /// Replaces a block's row — its entities, base point, or name —
    /// keeping its ID, and so every reference to it. One change, however
    /// many references there are (§19.6). `*Model_Space` stays as it is.
    pub fn redefine_block(&mut self, id: BlockId, block: Block) -> Result<(), TransactionError> {
        if id == BlockId::FIRST {
            return Err(TransactionError::PermanentBlock);
        }
        if self.block_change(id).is_some() {
            return Err(TransactionError::BlockAlreadyChanged(id));
        }
        let before = self
            .drawing
            .blocks
            .get(id)
            .cloned()
            .ok_or(TransactionError::UnknownBlock(id))?;
        if block.name != before.name {
            self.free_block_name(&block.name, Some(id))?;
        }
        self.check_block_contents(id, &block)?;
        self.push_other(Change::Block(TableChange::Replace {
            id,
            before,
            after: block,
        }));
        Ok(())
    }

    /// Removes a block nothing places: no reference in the drawing, none
    /// inside another block, none this transaction adds.
    pub fn remove_block(&mut self, id: BlockId) -> Result<(), TransactionError> {
        if id == BlockId::FIRST {
            return Err(TransactionError::PermanentBlock);
        }
        if self.block_change(id).is_some() {
            return Err(TransactionError::BlockAlreadyChanged(id));
        }
        let row = self
            .drawing
            .blocks
            .get(id)
            .cloned()
            .ok_or(TransactionError::UnknownBlock(id))?;
        let places = |entity: &Entity| matches!(entity, Entity::BlockRef(r) if r.block == id);
        let pending = self.changes.iter().any(|change| match change {
            Change::Entity(EntityChange::Insert { record, .. }) => places(&record.entity),
            Change::Entity(EntityChange::Replace { after, .. }) => places(&after.entity),
            Change::Block(TableChange::Insert { row, .. })
            | Change::Block(TableChange::Replace { after: row, .. }) => {
                row.references().any(|inner| inner == id)
            }
            _ => false,
        });
        // Placed by another block as the drawing holds it — one this
        // transaction removes places nothing any more (PURGE sweeps a
        // block and then what only it placed); one it redefines still
        // counts until commit, like a layer an entity is taken off.
        let removed_here =
            |other: BlockId| matches!(self.block_change(other), Some(TableChange::Remove { .. }));
        let in_use = pending
            || self.drawing.entities().any(|(_, entity)| places(entity))
            || self.drawing.blocks.iter().any(|(other, block)| {
                other != id && !removed_here(other) && block.references().any(|inner| inner == id)
            });
        if in_use {
            return Err(TransactionError::BlockInUse(id));
        }
        self.push_other(Change::Block(TableChange::Remove { id, row }));
        Ok(())
    }

    /// Applies the changes as one undo step and discards any redo branch.
    /// An empty transaction is a no-op: no history entry, no new revision.
    pub fn commit(self) -> ApplyReport {
        if self.changes.is_empty() {
            return ApplyReport::default();
        }
        let transaction = Transaction::new(self.changes);
        let report = self.drawing.apply(&transaction);
        self.drawing.history.record(transaction);
        report
    }

    fn insert_record(&mut self, record: EntityRecord) -> EntityId {
        let id = self.drawing.ids.allocate();
        self.push(EntityChange::Insert { id, record });
        id
    }

    fn push(&mut self, change: EntityChange) {
        self.touched.insert(change.id());
        self.changes.push(Change::Entity(change));
    }

    /// Records a table or settings change.
    fn push_other(&mut self, change: Change) {
        debug_assert!(!matches!(change, Change::Entity(_)));
        self.others.push(self.changes.len());
        self.changes.push(change);
    }

    /// The table and settings changes recorded so far, in order.
    fn others(&self) -> impl DoubleEndedIterator<Item = &Change> {
        self.others.iter().map(|index| &self.changes[*index])
    }

    fn existing(&self, id: EntityId) -> Result<EntityRecord, TransactionError> {
        if self.touched.contains(&id) {
            return Err(TransactionError::AlreadyChanged(id));
        }
        self.drawing
            .entities
            .record(id)
            .cloned()
            .ok_or(TransactionError::UnknownEntity(id))
    }

    /// How this transaction has changed the layer `id` so far, if it has.
    fn layer_change(&self, id: LayerId) -> Option<&TableChange<Layer>> {
        self.others().find_map(|change| match change {
            Change::Layer(change) if change.id() == id => Some(change),
            _ => None,
        })
    }

    /// The layer `id` as it will be once this transaction commits.
    fn layer_now(&self, id: LayerId) -> Option<&Layer> {
        match self.layer_change(id) {
            Some(TableChange::Remove { .. }) => None,
            Some(TableChange::Insert { row, .. }) => Some(row),
            Some(TableChange::Replace { after, .. }) => Some(after),
            None => self.drawing.layers.get(id),
        }
    }

    /// How this transaction has changed the linetype `id` so far, if it has.
    fn linetype_change(&self, id: LinetypeId) -> Option<&TableChange<LinetypeDef>> {
        self.others().find_map(|change| match change {
            Change::Linetype(change) if change.id() == id => Some(change),
            _ => None,
        })
    }

    /// The linetype `id` as it will be once this transaction commits.
    fn linetype_now(&self, id: LinetypeId) -> Option<&LinetypeDef> {
        match self.linetype_change(id) {
            Some(TableChange::Remove { .. }) => None,
            Some(TableChange::Insert { row, .. }) => Some(row),
            Some(TableChange::Replace { after, .. }) => Some(after),
            None => self.drawing.linetypes.get(id),
        }
    }

    /// How this transaction has changed the block `id` so far, if it has.
    fn block_change(&self, id: BlockId) -> Option<&TableChange<Block>> {
        self.others().find_map(|change| match change {
            Change::Block(change) if change.id() == id => Some(change),
            _ => None,
        })
    }

    /// The block `id` as it will be once this transaction commits.
    fn block_now(&self, id: BlockId) -> Option<&Block> {
        match self.block_change(id) {
            Some(TableChange::Remove { .. }) => None,
            Some(TableChange::Insert { row, .. }) => Some(row),
            Some(TableChange::Replace { after, .. }) => Some(after),
            None => self.drawing.blocks.get(id),
        }
    }

    /// Whether `entity` may be stored: a block reference must place a
    /// block that exists — never the drawing itself — at a positive scale.
    fn known_block(&self, entity: &Entity) -> Result<(), TransactionError> {
        let Entity::BlockRef(reference) = entity else {
            return Ok(());
        };
        if reference.block == BlockId::FIRST {
            return Err(TransactionError::PermanentBlock);
        }
        if self.block_now(reference.block).is_none() {
            return Err(TransactionError::UnknownBlock(reference.block));
        }
        if !(reference.scale.is_finite() && reference.scale > 0.0) {
            return Err(TransactionError::BadBlockScale);
        }
        Ok(())
    }

    /// Whether a definition's entities are ones the drawing could hold,
    /// and none of them places `id` itself, directly or through others.
    fn check_block_contents(&self, id: BlockId, block: &Block) -> Result<(), TransactionError> {
        if block
            .entities
            .iter()
            .any(|record| matches!(*record.entity, Entity::Viewport(_)))
        {
            return Err(TransactionError::ViewportOffPaper);
        }
        for record in block.entities.iter() {
            self.known_layer(record.properties.layer)?;
            self.known_linetype(record.properties.linetype)?;
            self.known_text_style(&record.entity)?;
            self.known_block(&record.entity)?;
        }
        if self.drawing.blocks.would_cycle(id, block) {
            return Err(TransactionError::BlockCycle(id));
        }
        Ok(())
    }

    /// Whether `name` can be given to a block — to `renamed`, or to a new
    /// one.
    fn free_block_name(
        &self,
        name: &str,
        renamed: Option<BlockId>,
    ) -> Result<(), TransactionError> {
        check_name(name).map_err(TransactionError::BadBlockName)?;
        let added = self.others().filter_map(|change| match change {
            Change::Block(TableChange::Insert { id, .. }) => Some(*id),
            _ => None,
        });
        let taken = self
            .drawing
            .blocks
            .iter()
            .map(|(id, _)| id)
            .chain(added)
            .filter(|id| Some(*id) != renamed)
            .find(|id| {
                self.block_now(*id)
                    .is_some_and(|block| same_name(&block.name, name))
            });
        match taken {
            Some(id) => Err(TransactionError::DuplicateBlockName(id)),
            None => Ok(()),
        }
    }

    /// The group `id` as it will be once this transaction commits. Unlike
    /// the other tables, a group may change more than once in one
    /// transaction — erasing two of its members — each change from the
    /// state the one before left: the newest is what it is.
    fn group_now(&self, id: GroupId) -> Option<&Group> {
        let change = self.others().rev().find_map(|change| match change {
            Change::Group(change) if change.id() == id => Some(change),
            _ => None,
        });
        match change {
            Some(TableChange::Remove { .. }) => None,
            Some(TableChange::Insert { row, .. }) => Some(row),
            Some(TableChange::Replace { after, .. }) => Some(after),
            None => self.drawing.groups.get(id),
        }
    }

    /// Every group as it will be once this transaction commits.
    fn groups_now(&self) -> Vec<(GroupId, Group)> {
        let mut ids: BTreeSet<GroupId> = self.drawing.groups.iter().map(|(id, _)| id).collect();
        ids.extend(self.others().filter_map(|change| match change {
            Change::Group(change) => Some(change.id()),
            _ => None,
        }));
        ids.into_iter()
            .filter_map(|id| self.group_now(id).map(|group| (id, group.clone())))
            .collect()
    }

    /// `*A` and the first number no group has.
    fn free_unnamed_group_name(&self) -> String {
        let taken: BTreeSet<u32> = self
            .groups_now()
            .iter()
            .filter_map(|(_, group)| crate::group::unnamed_number(&group.name))
            .collect();
        let number = (1..).find(|n| !taken.contains(n)).expect("a free number");
        format!("{}{number}", Group::UNNAMED_PREFIX)
    }

    /// Refuses a name a group cannot have, or another group has.
    /// The multileader style `id` as it will be once this transaction
    /// commits.
    fn mleader_style_now(&self, id: MLeaderStyleId) -> Option<&MLeaderStyle> {
        let change = self.others().rev().find_map(|change| match change {
            Change::MLeaderStyle(change) if change.id() == id => Some(change),
            _ => None,
        });
        match change {
            Some(TableChange::Remove { .. }) => None,
            Some(TableChange::Insert { row, .. }) => Some(row),
            Some(TableChange::Replace { after, .. }) => Some(after),
            None => self.drawing.mleader_styles.get(id),
        }
    }

    fn existing_mleader_style(&self, id: MLeaderStyleId) -> Result<MLeaderStyle, TransactionError> {
        let changed = self
            .others()
            .any(|change| matches!(change, Change::MLeaderStyle(change) if change.id() == id));
        if changed {
            return Err(TransactionError::UnknownMLeaderStyle(id));
        }
        self.drawing
            .mleader_styles
            .get(id)
            .cloned()
            .ok_or(TransactionError::UnknownMLeaderStyle(id))
    }

    fn check_mleader_style_name(
        &self,
        name: &str,
        except: Option<MLeaderStyleId>,
    ) -> Result<(), TransactionError> {
        check_name(name).map_err(TransactionError::BadMLeaderStyleName)?;
        let added = self.others().filter_map(|change| match change {
            Change::MLeaderStyle(TableChange::Insert { id, row }) => Some((*id, row)),
            Change::MLeaderStyle(TableChange::Replace { id, after, .. }) => Some((*id, after)),
            _ => None,
        });
        let taken = self
            .drawing
            .mleader_styles
            .iter()
            .chain(added)
            .find(|(id, other)| Some(*id) != except && same_name(&other.name, name));
        match taken {
            Some((id, _)) => Err(TransactionError::DuplicateMLeaderStyleName(id)),
            None => Ok(()),
        }
    }

    fn existing_view(&self, id: NamedViewId) -> Result<NamedView, TransactionError> {
        let changed = self
            .others()
            .any(|change| matches!(change, Change::View(change) if change.id() == id));
        if changed {
            return Err(TransactionError::UnknownView(id));
        }
        self.drawing
            .views
            .get(id)
            .cloned()
            .ok_or(TransactionError::UnknownView(id))
    }

    /// Refuses a view that shows nothing, one of a space that will not
    /// be, and a name not allowed or another view's.
    fn check_view(
        &self,
        view: &NamedView,
        except: Option<NamedViewId>,
    ) -> Result<(), TransactionError> {
        if !view.is_valid() {
            return Err(TransactionError::BadView);
        }
        self.known_layout(view.space)?;
        check_name(&view.name).map_err(TransactionError::BadViewName)?;
        let added = self.others().filter_map(|change| match change {
            Change::View(TableChange::Insert { id, row }) => Some((*id, row)),
            Change::View(TableChange::Replace { id, after, .. }) => Some((*id, after)),
            _ => None,
        });
        let taken = self
            .drawing
            .views
            .iter()
            .chain(added)
            .find(|(id, other)| Some(*id) != except && same_name(&other.name, &view.name));
        match taken {
            Some((id, _)) => Err(TransactionError::DuplicateViewName(id)),
            None => Ok(()),
        }
    }

    fn existing_layer_state(&self, id: LayerStateId) -> Result<LayerState, TransactionError> {
        let changed = self
            .others()
            .any(|change| matches!(change, Change::LayerState(change) if change.id() == id));
        if changed {
            return Err(TransactionError::UnknownLayerState(id));
        }
        self.drawing
            .layer_states
            .get(id)
            .cloned()
            .ok_or(TransactionError::UnknownLayerState(id))
    }

    /// Refuses a name not allowed, or one another state has — in the
    /// drawing or added by this transaction.
    fn check_layer_state_name(
        &self,
        name: &str,
        except: Option<LayerStateId>,
    ) -> Result<(), TransactionError> {
        check_name(name).map_err(TransactionError::BadLayerStateName)?;
        let added = self.others().filter_map(|change| match change {
            Change::LayerState(TableChange::Insert { id, row }) => Some((*id, row)),
            Change::LayerState(TableChange::Replace { id, after, .. }) => Some((*id, after)),
            _ => None,
        });
        let taken = self
            .drawing
            .layer_states
            .iter()
            .chain(added)
            .find(|(id, state)| Some(*id) != except && same_name(&state.name, name));
        match taken {
            Some((id, _)) => Err(TransactionError::DuplicateLayerStateName(id)),
            None => Ok(()),
        }
    }

    fn check_group_name(
        &self,
        name: &str,
        except: Option<GroupId>,
    ) -> Result<(), TransactionError> {
        if crate::group::unnamed_number(name).is_none() {
            check_name(name).map_err(TransactionError::BadGroupName)?;
        }
        match self
            .groups_now()
            .into_iter()
            .find(|(id, group)| Some(*id) != except && same_name(&group.name, name))
        {
            Some((id, _)) => Err(TransactionError::DuplicateGroupName(id)),
            None => Ok(()),
        }
    }

    /// Refuses no members, a member given twice, and one that is not an
    /// entity the drawing will have.
    fn check_members(&self, members: &[EntityId]) -> Result<(), TransactionError> {
        if members.is_empty() {
            return Err(TransactionError::EmptyGroup);
        }
        let mut seen = HashSet::new();
        for member in members {
            // What this transaction touched is looked up in it; the rest
            // in the drawing.
            let there = if self.touched.contains(member) {
                !self.changes.iter().any(|change| {
                    matches!(change, Change::Entity(EntityChange::Remove { id, .. }) if id == member)
                })
            } else {
                self.drawing.entities.record(*member).is_some()
            };
            if !there || !seen.insert(*member) {
                return Err(TransactionError::BadGroupMember(*member));
            }
        }
        Ok(())
    }

    /// The layout `id` as it will be once this transaction commits.
    fn layout_now(&self, id: LayoutId) -> Option<&Layout> {
        let change = self.others().find_map(|change| match change {
            Change::Layout(change) if change.id() == id => Some(change),
            _ => None,
        });
        match change {
            Some(TableChange::Remove { .. }) => None,
            Some(TableChange::Insert { row, .. }) => Some(row),
            Some(TableChange::Replace { after, .. }) => Some(after),
            None => self.drawing.layouts.get(id),
        }
    }

    fn known_layout(&self, id: LayoutId) -> Result<(), TransactionError> {
        self.layout_now(id)
            .map(|_| ())
            .ok_or(TransactionError::UnknownLayout(id))
    }

    /// Refuses a layout name another layout has, `except` the one given.
    fn free_layout_name(
        &self,
        name: &str,
        except: Option<LayoutId>,
    ) -> Result<(), TransactionError> {
        let added = self.others().filter_map(|change| match change {
            Change::Layout(TableChange::Insert { id, row })
            | Change::Layout(TableChange::Replace { id, after: row, .. }) => Some((*id, row)),
            _ => None,
        });
        let taken = self
            .drawing
            .layouts
            .iter()
            .filter(|(id, _)| self.layout_now(*id).is_some())
            .chain(added)
            .find(|(id, row)| Some(*id) != except && same_name(&row.name, name));
        match taken {
            Some((id, _)) => Err(TransactionError::DuplicateLayoutName(id)),
            None => Ok(()),
        }
    }

    /// The text style `id` as it will be once this transaction commits.
    fn text_style_now(&self, id: TextStyleId) -> Option<&TextStyle> {
        let change = self.others().find_map(|change| match change {
            Change::TextStyle(change) if change.id() == id => Some(change),
            _ => None,
        });
        match change {
            Some(TableChange::Remove { .. }) => None,
            Some(TableChange::Insert { row, .. }) => Some(row),
            Some(TableChange::Replace { after, .. }) => Some(after),
            None => self.drawing.text_styles.get(id),
        }
    }

    /// Whether the entity may be stored: a text's style must exist once
    /// this transaction commits; a dimension's too, and the dimension must
    /// be one that can be laid out.
    fn known_text_style(&self, entity: &Entity) -> Result<(), TransactionError> {
        let style = match entity {
            Entity::Text(text) => text.style,
            Entity::MText(text) => text.style,
            Entity::Dimension(dimension) => {
                dimension.check().map_err(TransactionError::BadDimension)?;
                if self.dim_style_now(dimension.style).is_none() {
                    return Err(TransactionError::UnknownDimStyle(dimension.style));
                }
                return Ok(());
            }
            _ => return Ok(()),
        };
        if self.text_style_now(style).is_none() {
            return Err(TransactionError::UnknownTextStyle(style));
        }
        Ok(())
    }

    /// Whether `linetype` may be given to an entity, a layer, or new
    /// objects: it is an indirection, or a row that exists once this
    /// transaction commits.
    fn known_linetype(&self, linetype: Linetype) -> Result<(), TransactionError> {
        match linetype {
            Linetype::Named(id) if self.linetype_now(id).is_none() => {
                Err(TransactionError::UnknownLinetype(id))
            }
            _ => Ok(()),
        }
    }

    /// The settings as they will be once this transaction commits.
    fn settings_now(&self) -> DrawingSettings {
        self.others()
            .rev()
            .find_map(|change| match change {
                Change::Settings(change) => Some(change.after),
                _ => None,
            })
            .unwrap_or(self.drawing.settings)
    }

    /// Records the settings as `after`. A transaction holds at most one
    /// settings change: a second one moves the first one's end.
    fn set_settings(&mut self, after: DrawingSettings) {
        let pending = self
            .others
            .iter()
            .find_map(|index| match &self.changes[*index] {
                Change::Settings(_) => Some(*index),
                _ => None,
            });
        match pending {
            Some(index) => {
                if let Change::Settings(change) = &mut self.changes[index] {
                    change.after = after;
                }
            }
            None => self.push_other(Change::Settings(SettingsChange {
                before: self.drawing.settings,
                after,
            })),
        }
    }

    /// A layer of the drawing that this transaction has not touched.
    fn existing_layer(&self, id: LayerId) -> Result<Layer, TransactionError> {
        if self.layer_change(id).is_some() {
            return Err(TransactionError::LayerAlreadyChanged(id));
        }
        self.drawing
            .layers
            .get(id)
            .cloned()
            .ok_or(TransactionError::UnknownLayer(id))
    }

    /// Whether an entity may be put on layer `id`: it exists and this
    /// transaction does not remove it, or this transaction adds it.
    fn known_layer(&self, id: LayerId) -> Result<(), TransactionError> {
        match self.layer_now(id) {
            Some(_) => Ok(()),
            None => Err(TransactionError::UnknownLayer(id)),
        }
    }

    /// Whether `name` can be given to a layer — to `renamed`, or to a new
    /// one: it is allowed, and no other layer has it once this transaction
    /// commits.
    fn free_name(&self, name: &str, renamed: Option<LayerId>) -> Result<(), TransactionError> {
        check_name(name).map_err(TransactionError::BadLayerName)?;
        let added = self.others().filter_map(|change| match change {
            Change::Layer(TableChange::Insert { id, .. }) => Some(*id),
            _ => None,
        });
        let taken = self
            .drawing
            .layers
            .iter()
            .map(|(id, _)| id)
            .chain(added)
            .filter(|id| Some(*id) != renamed)
            .find(|id| {
                self.layer_now(*id)
                    .is_some_and(|layer| same_name(&layer.name, name))
            });
        match taken {
            Some(id) => Err(TransactionError::DuplicateLayerName(id)),
            None => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use cad_core::Point2;
    use proptest::prelude::*;

    use super::*;
    use crate::spatial::LinearIndex;
    use crate::{
        AttributeDefinitionEntity, BlockReferenceEntity, DimensionEntity, LineEntity, Measure,
        TextEntity,
    };

    fn line(x0: f64, y0: f64, x1: f64, y1: f64) -> LineEntity {
        LineEntity::new(Point2::new(x0, y0), Point2::new(x1, y1))
    }

    fn create(drawing: &mut Drawing, entity: LineEntity) -> EntityId {
        let mut tx = drawing.transaction();
        let id = tx.insert(entity).unwrap();
        tx.commit();
        id
    }

    fn everything() -> Aabb {
        Aabb::from_points(Point2::new(-1e9, -1e9), Point2::new(1e9, 1e9))
    }

    fn sorted(mut ids: Vec<EntityId>) -> Vec<EntityId> {
        ids.sort_unstable();
        ids
    }

    fn live_ids(drawing: &Drawing) -> Vec<EntityId> {
        sorted(drawing.entities().map(|(id, _)| id).collect())
    }

    #[test]
    fn a_new_drawing_is_empty() {
        let drawing = Drawing::new();
        assert!(drawing.is_empty());
        assert_eq!(drawing.extents(), None);
        assert!(!drawing.can_undo());
        assert!(!drawing.can_redo());
    }

    #[test]
    fn committed_entities_are_stored_indexed_and_reported() {
        let mut drawing = Drawing::new();
        let mut tx = drawing.transaction();
        let id = tx.insert(line(0.0, 0.0, 10.0, 0.0)).unwrap();
        let report = tx.commit();

        assert_eq!(
            report,
            ApplyReport {
                inserted: vec![id],
                ..Default::default()
            }
        );
        assert_eq!(
            drawing.entity(id),
            Some(&Entity::Line(line(0.0, 0.0, 10.0, 0.0)))
        );
        assert_eq!(drawing.query(&everything()), vec![id]);
        assert_eq!(drawing.len(), 1);
    }

    #[test]
    fn nothing_changes_until_commit() {
        let mut drawing = Drawing::new();
        let before = drawing.revision();
        let mut tx = drawing.transaction();
        tx.insert(line(0.0, 0.0, 1.0, 1.0)).unwrap();
        drop(tx);

        assert!(drawing.is_empty());
        assert_eq!(drawing.revision(), before);
        assert!(!drawing.can_undo());
    }

    #[test]
    fn an_empty_transaction_is_a_no_op() {
        let mut drawing = Drawing::new();
        let before = drawing.revision();
        assert_eq!(drawing.transaction().commit(), ApplyReport::default());
        assert_eq!(drawing.revision(), before);
        assert!(!drawing.can_undo());
    }

    #[test]
    fn undo_removes_a_created_entity_and_redo_restores_the_same_id() {
        let mut drawing = Drawing::new();
        let id = create(&mut drawing, line(0.0, 0.0, 10.0, 0.0));

        let undone = drawing.undo().unwrap();
        assert_eq!(
            undone,
            ApplyReport {
                removed: vec![id],
                ..Default::default()
            }
        );
        assert_eq!(drawing.entity(id), None);
        assert_eq!(drawing.query(&everything()), vec![]);

        let redone = drawing.redo().unwrap();
        assert_eq!(
            redone,
            ApplyReport {
                inserted: vec![id],
                ..Default::default()
            }
        );
        assert_eq!(
            drawing.entity(id),
            Some(&Entity::Line(line(0.0, 0.0, 10.0, 0.0)))
        );
        assert_eq!(drawing.query(&everything()), vec![id]);
    }

    /// DESIGN §10.2: create A, create B, undo B, create C → C gets a new ID.
    #[test]
    fn the_id_allocator_never_rolls_back_through_undo() {
        let mut drawing = Drawing::new();
        let a = create(&mut drawing, line(0.0, 0.0, 1.0, 0.0));
        let b = create(&mut drawing, line(0.0, 1.0, 1.0, 1.0));
        assert_eq!(b.raw(), a.raw() + 1);

        let before_undo = drawing.next_entity_id();
        drawing.undo().unwrap();
        assert_eq!(
            drawing.next_entity_id(),
            before_undo,
            "undo must not lower the counter"
        );

        let c = create(&mut drawing, line(0.0, 2.0, 1.0, 2.0));
        assert_eq!(c.raw(), b.raw() + 1, "B's ID must not be reused for C");
    }

    /// DESIGN §19.3: A, B, C, undo C, undo B, create D → B/C are gone for good.
    #[test]
    fn new_work_after_undo_discards_the_redo_branch_without_reusing_ids() {
        let mut drawing = Drawing::new();
        let a = create(&mut drawing, line(0.0, 0.0, 1.0, 0.0));
        let b = create(&mut drawing, line(0.0, 1.0, 1.0, 1.0));
        let c = create(&mut drawing, line(0.0, 2.0, 1.0, 2.0));
        drawing.undo().unwrap();
        drawing.undo().unwrap();
        assert!(drawing.can_redo());

        let d = create(&mut drawing, line(0.0, 3.0, 1.0, 3.0));

        assert!(!drawing.can_redo());
        assert_eq!(drawing.redo(), None);
        assert_eq!(live_ids(&drawing), vec![a, d]);
        assert!(d > c && d != b);
    }

    #[test]
    fn remove_and_replace_are_undoable() {
        let mut drawing = Drawing::new();
        let kept = create(&mut drawing, line(0.0, 0.0, 1.0, 0.0));
        let erased = create(&mut drawing, line(5.0, 5.0, 6.0, 5.0));

        let mut tx = drawing.transaction();
        tx.replace(kept, line(0.0, 0.0, 100.0, 0.0)).unwrap();
        tx.remove(erased).unwrap();
        let report = tx.commit();
        assert_eq!(report.replaced, vec![kept]);
        assert_eq!(report.removed, vec![erased]);
        assert_eq!(
            drawing.entity(kept),
            Some(&Entity::Line(line(0.0, 0.0, 100.0, 0.0)))
        );
        assert_eq!(drawing.entity(erased), None);

        // One gesture, one undo step: both changes revert together.
        let undone = drawing.undo().unwrap();
        assert_eq!(undone.inserted, vec![erased]);
        assert_eq!(undone.replaced, vec![kept]);
        assert_eq!(
            drawing.entity(kept),
            Some(&Entity::Line(line(0.0, 0.0, 1.0, 0.0)))
        );
        assert_eq!(
            drawing.entity(erased),
            Some(&Entity::Line(line(5.0, 5.0, 6.0, 5.0)))
        );
    }

    #[test]
    fn the_index_follows_replaced_geometry() {
        let mut drawing = Drawing::new();
        let id = create(&mut drawing, line(0.0, 0.0, 1.0, 0.0));
        let far = Aabb::from_points(Point2::new(900.0, 900.0), Point2::new(1100.0, 1100.0));
        assert_eq!(drawing.query(&far), vec![]);

        let mut tx = drawing.transaction();
        tx.replace(id, line(1000.0, 1000.0, 1001.0, 1000.0))
            .unwrap();
        tx.commit();
        assert_eq!(drawing.query(&far), vec![id]);

        drawing.undo().unwrap();
        assert_eq!(drawing.query(&far), vec![]);
    }

    #[test]
    fn changing_a_missing_entity_is_rejected() {
        let mut drawing = Drawing::new();
        let ghost = EntityId::from_raw(999);
        let mut tx = drawing.transaction();
        assert_eq!(
            tx.remove(ghost),
            Err(TransactionError::UnknownEntity(ghost))
        );
        assert_eq!(
            tx.replace(ghost, line(0.0, 0.0, 1.0, 1.0)),
            Err(TransactionError::UnknownEntity(ghost))
        );
    }

    #[test]
    fn an_entity_can_change_only_once_per_transaction() {
        let mut drawing = Drawing::new();
        let id = create(&mut drawing, line(0.0, 0.0, 1.0, 0.0));
        let mut tx = drawing.transaction();
        tx.remove(id).unwrap();
        assert_eq!(tx.remove(id), Err(TransactionError::AlreadyChanged(id)));
        assert_eq!(
            tx.replace(id, line(0.0, 0.0, 2.0, 0.0)),
            Err(TransactionError::AlreadyChanged(id))
        );
    }

    #[test]
    fn revision_changes_on_commit_undo_and_redo_only() {
        let mut drawing = Drawing::new();
        let r0 = drawing.revision();

        let id = create(&mut drawing, line(0.0, 0.0, 1.0, 0.0));
        let r1 = drawing.revision();
        assert_ne!(r1, r0);

        // Reads leave the revision alone.
        let _ = (
            drawing.entity(id),
            drawing.query(&everything()),
            drawing.extents(),
            drawing.len(),
        );
        assert_eq!(drawing.revision(), r1);

        drawing.undo().unwrap();
        let r2 = drawing.revision();
        assert_ne!(r2, r1);

        drawing.redo().unwrap();
        assert_ne!(drawing.revision(), r2);

        // Nothing left to redo: no change, no new revision.
        let r3 = drawing.revision();
        assert_eq!(drawing.redo(), None);
        assert_eq!(drawing.revision(), r3);
    }

    #[test]
    fn undo_and_redo_on_an_empty_history_do_nothing() {
        let mut drawing = Drawing::new();
        assert_eq!(drawing.undo(), None);
        assert_eq!(drawing.redo(), None);
    }

    #[test]
    fn imported_entities_are_indexed_and_not_undoable() {
        let drawing = Drawing::from_entities([
            Entity::Line(line(0.0, 0.0, 1.0, 0.0)),
            Entity::Line(line(10.0, 10.0, 11.0, 10.0)),
        ]);
        assert_eq!(drawing.len(), 2);
        assert_eq!(drawing.query(&everything()).len(), 2);
        assert!(!drawing.can_undo());
        assert_eq!(
            drawing.extents(),
            Some(Aabb::from_points(
                Point2::new(0.0, 0.0),
                Point2::new(11.0, 10.0)
            ))
        );
    }

    #[test]
    fn extents_cover_every_entity() {
        let mut drawing = Drawing::new();
        create(&mut drawing, line(-5.0, 0.0, 1.0, 0.0));
        create(&mut drawing, line(0.0, -3.0, 0.0, 8.0));
        assert_eq!(
            drawing.extents(),
            Some(Aabb::from_points(
                Point2::new(-5.0, -3.0),
                Point2::new(1.0, 8.0)
            ))
        );
    }

    #[test]
    fn a_group_is_undone_and_redone_as_one_step() {
        let mut drawing = Drawing::new();
        let before = create(&mut drawing, line(0.0, 0.0, 1.0, 0.0));

        drawing.begin_group();
        let a = create(&mut drawing, line(0.0, 1.0, 1.0, 1.0));
        let b = create(&mut drawing, line(0.0, 2.0, 1.0, 2.0));
        let mut tx = drawing.transaction();
        tx.replace(a, line(0.0, 1.0, 9.0, 1.0)).unwrap();
        tx.commit();
        drawing.end_group();

        let undone = drawing.undo().unwrap();
        assert_eq!(live_ids(&drawing), vec![before]);
        assert_eq!(sorted(undone.removed), vec![a, b]);

        drawing.redo().unwrap();
        assert_eq!(live_ids(&drawing), vec![before, a, b]);
        assert_eq!(
            drawing.entity(a),
            Some(&Entity::Line(line(0.0, 1.0, 9.0, 1.0)))
        );

        drawing.undo().unwrap();
        drawing.undo().unwrap();
        assert!(drawing.is_empty() && !drawing.can_undo());
    }

    #[test]
    fn a_group_with_one_or_no_transactions_changes_nothing() {
        let mut drawing = Drawing::new();
        drawing.begin_group();
        drawing.end_group();
        assert!(!drawing.can_undo());

        drawing.begin_group();
        let id = create(&mut drawing, line(0.0, 0.0, 1.0, 0.0));
        drawing.end_group();
        drawing.end_group(); // closing twice is harmless
        drawing.undo().unwrap();
        assert_eq!(drawing.entity(id), None);
        assert!(!drawing.can_undo());
    }

    #[test]
    fn rolling_back_inside_a_group_is_not_redoable() {
        let mut drawing = Drawing::new();
        let outside = create(&mut drawing, line(0.0, 0.0, 1.0, 0.0));
        assert_eq!(drawing.rollback_last_in_group(), None, "no group is open");

        drawing.begin_group();
        assert_eq!(drawing.rollback_last_in_group(), None, "the group is empty");
        let kept = create(&mut drawing, line(0.0, 1.0, 1.0, 1.0));
        let dropped = create(&mut drawing, line(0.0, 2.0, 1.0, 2.0));

        let report = drawing.rollback_last_in_group().unwrap();
        assert_eq!(report.removed, vec![dropped]);
        assert!(
            !drawing.can_redo(),
            "a rolled-back step is forgotten, not redoable"
        );
        drawing.end_group();

        assert_eq!(live_ids(&drawing), vec![outside, kept]);
        // Rolling back never reaches transactions from before the group.
        drawing.begin_group();
        assert_eq!(drawing.rollback_last_in_group(), None);
        assert_eq!(live_ids(&drawing), vec![outside, kept]);
    }

    #[test]
    fn undo_and_redo_inside_a_group_do_not_confuse_it() {
        let mut drawing = Drawing::new();
        let a = create(&mut drawing, line(0.0, 0.0, 1.0, 0.0));
        let b = create(&mut drawing, line(0.0, 1.0, 1.0, 1.0));

        // What the UNDO command does: it runs inside its own group.
        drawing.begin_group();
        drawing.undo().unwrap();
        drawing.end_group();
        assert_eq!(live_ids(&drawing), vec![a]);

        drawing.begin_group();
        drawing.redo().unwrap();
        drawing.end_group();
        assert_eq!(live_ids(&drawing), vec![a, b]);

        // Still two separate steps.
        drawing.undo().unwrap();
        assert_eq!(live_ids(&drawing), vec![a]);
    }

    /// DESIGN §19.4: editing a very large entity must not produce copies of it.
    /// History shares the payload, so undo puts back the very same allocation.
    #[test]
    fn undoing_an_edit_to_a_large_polyline_restores_the_original_payload_without_copying() {
        use crate::PolylineEntity;
        let vertices: Vec<Point2> = (0..10_000)
            .map(|i| Point2::new(f64::from(i), f64::from(i % 7)))
            .collect();
        let mut drawing = Drawing::new();
        let mut tx = drawing.transaction();
        let id = tx
            .insert(PolylineEntity::new(vertices.clone(), false))
            .unwrap();
        tx.commit();
        let original: *const Entity = drawing.entity(id).unwrap();

        let mut moved = vertices;
        moved[5000] = Point2::new(-1.0, -1.0);
        let mut tx = drawing.transaction();
        tx.replace(id, PolylineEntity::new(moved, false)).unwrap();
        tx.commit();
        let edited: *const Entity = drawing.entity(id).unwrap();
        assert!(!std::ptr::eq(original, edited));

        drawing.undo().unwrap();
        assert!(
            std::ptr::eq(original, drawing.entity(id).unwrap()),
            "undo must reuse the stored payload"
        );
        drawing.redo().unwrap();
        assert!(
            std::ptr::eq(edited, drawing.entity(id).unwrap()),
            "redo must reuse the stored payload"
        );
    }

    // ── properties and layers (#131, ADR-0013) ──

    fn add_layer(drawing: &mut Drawing, name: &str) -> LayerId {
        let mut tx = drawing.transaction();
        let id = tx.add_layer(Layer::named(name)).unwrap();
        tx.commit();
        id
    }

    fn red_on(layer: LayerId) -> Properties {
        Properties {
            color: crate::Color::Index(1),
            ..Properties::on(layer)
        }
    }

    #[test]
    fn a_new_drawing_has_layer_zero_and_new_entities_are_on_it() {
        let mut drawing = Drawing::new();
        let layers: Vec<(LayerId, &str)> = drawing
            .layers()
            .iter()
            .map(|(id, layer)| (id, layer.name.as_str()))
            .collect();
        assert_eq!(layers, vec![(LayerId::FIRST, "0")]);

        let id = create(&mut drawing, line(0.0, 0.0, 10.0, 0.0));
        assert_eq!(drawing.properties(id), Some(Properties::default()));
        assert_eq!(drawing.properties(EntityId::from_raw(999)), None);
        let imported = Drawing::from_entities([Entity::Line(line(0.0, 0.0, 1.0, 1.0))]);
        assert!(
            imported
                .records()
                .all(|(_, record)| record.properties == Properties::default())
        );
    }

    #[test]
    fn a_drawing_can_start_with_layers_and_styled_entities_and_no_history() {
        let walls = LayerId::from_raw(1);
        let drawing = Drawing::from_parts(
            [Layer::named("walls")],
            [],
            [
                (Entity::Line(line(0.0, 0.0, 10.0, 0.0)), red_on(walls)),
                (
                    Entity::Line(line(0.0, 5.0, 10.0, 5.0)),
                    Properties::default(),
                ),
            ],
        )
        .unwrap();
        assert_eq!(drawing.layers().get(walls), Some(&Layer::named("walls")));
        let styles: Vec<Properties> = live_ids(&drawing)
            .into_iter()
            .filter_map(|id| drawing.properties(id))
            .collect();
        assert_eq!(styles, vec![red_on(walls), Properties::default()]);
        assert!(!drawing.can_undo());
        assert_eq!(drawing.query(&everything()).len(), 2, "indexed");

        let nowhere = LayerId::from_raw(7);
        let refused = Drawing::from_parts(
            [],
            [],
            [(
                Entity::Line(line(0.0, 0.0, 1.0, 0.0)),
                Properties::on(nowhere),
            )],
        );
        assert_eq!(refused.err(), Some(TransactionError::UnknownLayer(nowhere)));
    }

    #[test]
    fn replacing_geometry_keeps_properties_and_restyling_keeps_geometry() {
        let mut drawing = Drawing::new();
        let walls = add_layer(&mut drawing, "walls");
        let mut tx = drawing.transaction();
        let id = tx
            .insert_with(line(0.0, 0.0, 10.0, 0.0), red_on(walls))
            .unwrap();
        tx.commit();
        assert_eq!(drawing.properties(id), Some(red_on(walls)));

        let mut tx = drawing.transaction();
        tx.replace(id, line(5.0, 5.0, 9.0, 9.0)).unwrap();
        tx.commit();
        assert_eq!(
            drawing.entity(id),
            Some(&Entity::Line(line(5.0, 5.0, 9.0, 9.0)))
        );
        assert_eq!(
            drawing.properties(id),
            Some(red_on(walls)),
            "kept through the edit"
        );

        let geometry: *const Entity = drawing.entity(id).unwrap();
        let mut tx = drawing.transaction();
        tx.set_properties(id, Properties::default()).unwrap();
        let report = tx.commit();
        assert_eq!(report.replaced, vec![id]);
        assert_eq!(drawing.properties(id), Some(Properties::default()));
        assert!(
            std::ptr::eq(geometry, drawing.entity(id).unwrap()),
            "a change of properties shares the geometry"
        );

        drawing.undo().unwrap();
        assert_eq!(drawing.properties(id), Some(red_on(walls)));
        drawing.undo().unwrap();
        assert_eq!(
            drawing.entity(id),
            Some(&Entity::Line(line(0.0, 0.0, 10.0, 0.0)))
        );
        assert_eq!(drawing.properties(id), Some(red_on(walls)));
    }

    #[test]
    fn an_entity_made_from_another_takes_its_properties() {
        let mut drawing = Drawing::new();
        let walls = add_layer(&mut drawing, "walls");
        let mut tx = drawing.transaction();
        let source = tx
            .insert_with(line(0.0, 0.0, 10.0, 0.0), red_on(walls))
            .unwrap();
        tx.commit();

        let mut tx = drawing.transaction();
        let copy = tx.insert_like(source, line(0.0, 5.0, 10.0, 5.0)).unwrap();
        assert_eq!(
            tx.insert_like(EntityId::from_raw(999), line(0.0, 0.0, 1.0, 0.0)),
            Err(TransactionError::UnknownEntity(EntityId::from_raw(999)))
        );
        tx.commit();
        assert_eq!(drawing.properties(copy), Some(red_on(walls)));

        // An erased entity comes back with the properties it had.
        let mut tx = drawing.transaction();
        tx.remove(copy).unwrap();
        tx.commit();
        drawing.undo().unwrap();
        assert_eq!(drawing.properties(copy), Some(red_on(walls)));
    }

    #[test]
    fn layers_are_added_renamed_and_removed_in_the_same_history_as_entities() {
        let mut drawing = Drawing::new();
        let walls = add_layer(&mut drawing, "walls");
        assert_eq!(walls.raw(), 1);
        let mut tx = drawing.transaction();
        let id = tx
            .insert_with(line(0.0, 0.0, 10.0, 0.0), Properties::on(walls))
            .unwrap();
        tx.commit();

        // A rename changes one row; the entity still refers to the same layer.
        let mut tx = drawing.transaction();
        tx.replace_layer(walls, Layer::named("partitions")).unwrap();
        let report = tx.commit();
        assert!(report.tables_changed);
        assert!(report.replaced.is_empty(), "no entity was touched");
        assert_eq!(
            drawing.layers().get(walls),
            Some(&Layer::named("partitions"))
        );
        assert_eq!(drawing.properties(id).unwrap().layer, walls);

        // Undo walks back through layer and entity changes alike.
        drawing.undo().unwrap();
        assert_eq!(drawing.layers().get(walls), Some(&Layer::named("walls")));
        drawing.undo().unwrap();
        assert_eq!(drawing.entity(id), None);
        drawing.undo().unwrap();
        assert_eq!(drawing.layers().len(), 1);
        assert!(!drawing.can_undo());

        drawing.redo().unwrap();
        drawing.redo().unwrap();
        drawing.redo().unwrap();
        assert_eq!(
            drawing.layers().get(walls),
            Some(&Layer::named("partitions"))
        );
        assert_eq!(drawing.properties(id).unwrap().layer, walls);

        // A layer made after an undone one never takes its ID.
        drawing.undo().unwrap();
        drawing.undo().unwrap();
        drawing.undo().unwrap();
        assert_eq!(add_layer(&mut drawing, "doors").raw(), 2);
    }

    #[test]
    fn a_layer_and_what_is_on_it_can_be_made_in_one_transaction() {
        let mut drawing = Drawing::new();
        let mut tx = drawing.transaction();
        let layer = tx.add_layer(Layer::named("walls")).unwrap();
        let id = tx
            .insert_with(line(0.0, 0.0, 10.0, 0.0), Properties::on(layer))
            .unwrap();
        tx.commit();
        assert_eq!(drawing.properties(id).unwrap().layer, layer);
        drawing.undo().unwrap();
        assert_eq!((drawing.len(), drawing.layers().len()), (0, 1), "one step");
        drawing.redo().unwrap();
        assert_eq!(drawing.properties(id).unwrap().layer, layer);
    }

    #[test]
    fn layer_changes_that_would_break_the_drawing_are_refused() {
        let mut drawing = Drawing::new();
        let walls = add_layer(&mut drawing, "walls");
        let empty = add_layer(&mut drawing, "empty");
        let nowhere = LayerId::from_raw(99);
        let mut tx = drawing.transaction();
        let id = tx
            .insert_with(line(0.0, 0.0, 10.0, 0.0), Properties::on(walls))
            .unwrap();
        tx.commit();
        let revision = drawing.revision();

        let mut tx = drawing.transaction();
        assert_eq!(
            tx.remove_layer(LayerId::FIRST),
            Err(TransactionError::PermanentLayer)
        );
        assert_eq!(
            tx.remove_layer(walls),
            Err(TransactionError::LayerInUse(walls))
        );
        assert_eq!(
            tx.remove_layer(nowhere),
            Err(TransactionError::UnknownLayer(nowhere))
        );
        assert_eq!(
            tx.replace_layer(nowhere, Layer::named("x")),
            Err(TransactionError::UnknownLayer(nowhere))
        );
        assert_eq!(
            tx.insert_with(line(0.0, 0.0, 1.0, 0.0), Properties::on(nowhere)),
            Err(TransactionError::UnknownLayer(nowhere))
        );
        assert_eq!(
            tx.set_properties(id, Properties::on(nowhere)),
            Err(TransactionError::UnknownLayer(nowhere))
        );
        // Nothing was recorded by any refusal.
        assert_eq!(tx.commit(), ApplyReport::default());
        assert_eq!(drawing.revision(), revision);

        // Within one transaction: a layer is changed once, a removed layer
        // takes no entities, and a layer an entity was just put on stays.
        let mut tx = drawing.transaction();
        tx.replace_layer(empty, Layer::named("spare")).unwrap();
        assert_eq!(
            tx.replace_layer(empty, Layer::named("again")),
            Err(TransactionError::LayerAlreadyChanged(empty))
        );
        assert_eq!(
            tx.remove_layer(empty),
            Err(TransactionError::LayerAlreadyChanged(empty))
        );
        drop(tx);

        let mut tx = drawing.transaction();
        tx.remove_layer(empty).unwrap();
        assert_eq!(
            tx.set_properties(id, Properties::on(empty)),
            Err(TransactionError::UnknownLayer(empty))
        );
        drop(tx);

        let mut tx = drawing.transaction();
        tx.set_properties(id, Properties::on(empty)).unwrap();
        assert_eq!(
            tx.remove_layer(empty),
            Err(TransactionError::LayerInUse(empty))
        );
        tx.commit();
        // Now `walls` is free and can go.
        let mut tx = drawing.transaction();
        tx.remove_layer(walls).unwrap();
        tx.commit();
        assert_eq!(drawing.layers().get(walls), None);
        drawing.undo().unwrap();
        assert_eq!(drawing.layers().get(walls), Some(&Layer::named("walls")));
    }

    // ── layer settings, names, and the current layer (#132) ──

    fn change_layer(
        drawing: &mut Drawing,
        id: LayerId,
        edit: impl FnOnce(&mut Layer),
    ) -> Result<(), TransactionError> {
        let mut layer = drawing.layers().get(id).cloned().expect("the layer exists");
        edit(&mut layer);
        let mut tx = drawing.transaction();
        let result = tx.replace_layer(id, layer);
        tx.commit();
        result
    }

    #[test]
    fn each_setting_of_a_layer_is_one_undoable_change() {
        let mut drawing = Drawing::new();
        let walls = add_layer(&mut drawing, "walls");
        let fresh = Layer::named("walls");

        type Edit = fn(&mut Layer);
        let edits: [(&str, Edit); 6] = [
            ("rename", |layer| layer.name = "partitions".to_owned()),
            ("colour", |layer| layer.color = crate::Color::Index(3)),
            ("linetype", |layer| {
                layer.linetype = crate::Linetype::ByBlock
            }),
            ("lineweight", |layer| {
                layer.lineweight = crate::Lineweight::ByBlock
            }),
            ("off", |layer| layer.on = false),
            ("lock", |layer| layer.locked = true),
        ];
        for (what, edit) in edits {
            change_layer(&mut drawing, walls, edit).unwrap();
            let mut expected = fresh.clone();
            edit(&mut expected);
            assert_eq!(drawing.layers().get(walls), Some(&expected), "{what}");
            drawing.undo().unwrap();
            assert_eq!(drawing.layers().get(walls), Some(&fresh), "{what} undone");
            drawing.redo().unwrap();
            assert_eq!(
                drawing.layers().get(walls),
                Some(&expected),
                "{what} redone"
            );
            drawing.undo().unwrap();
        }
    }

    #[test]
    fn layer_zero_keeps_its_name_but_takes_every_setting() {
        let mut drawing = Drawing::new();
        assert_eq!(
            change_layer(&mut drawing, LayerId::FIRST, |layer| layer.name =
                "base".to_owned()),
            Err(TransactionError::PermanentLayer)
        );
        change_layer(&mut drawing, LayerId::FIRST, |layer| {
            layer.color = crate::Color::Index(2);
            layer.locked = true;
        })
        .unwrap();
        let zero = drawing.layers().get(LayerId::FIRST).unwrap();
        assert_eq!(
            (zero.name.as_str(), zero.color, zero.locked),
            ("0", crate::Color::Index(2), true)
        );
    }

    #[test]
    fn layer_names_must_be_allowed_and_unique_whatever_their_case() {
        let mut drawing = Drawing::new();
        let walls = add_layer(&mut drawing, "Walls");
        let doors = add_layer(&mut drawing, "doors");

        let mut tx = drawing.transaction();
        assert_eq!(
            tx.add_layer(Layer::named("WALLS")),
            Err(TransactionError::DuplicateLayerName(walls))
        );
        assert_eq!(
            tx.add_layer(Layer::named("0")),
            Err(TransactionError::DuplicateLayerName(LayerId::FIRST))
        );
        assert_eq!(
            tx.add_layer(Layer::named("a*b")),
            Err(TransactionError::BadLayerName(NameError::Forbidden('*')))
        );
        assert_eq!(
            tx.add_layer(Layer::named("")),
            Err(TransactionError::BadLayerName(NameError::Empty))
        );
        // Two new layers in one transaction cannot share a name either.
        let windows = tx.add_layer(Layer::named("windows")).unwrap();
        assert_eq!(
            tx.add_layer(Layer::named("Windows")),
            Err(TransactionError::DuplicateLayerName(windows))
        );
        tx.commit();

        assert_eq!(
            change_layer(&mut drawing, doors, |layer| layer.name = "walls".to_owned()),
            Err(TransactionError::DuplicateLayerName(walls))
        );
        assert_eq!(
            change_layer(&mut drawing, doors, |layer| layer.name =
                " doors".to_owned()),
            Err(TransactionError::BadLayerName(NameError::Padded))
        );
        // Changing only the case of a layer's own name is a rename like any other.
        change_layer(&mut drawing, doors, |layer| layer.name = "Doors".to_owned()).unwrap();
        assert_eq!(drawing.layers().find("DOORS"), Some(doors));

        // A name freed in a transaction can be taken in the same one.
        let mut tx = drawing.transaction();
        tx.replace_layer(walls, Layer::named("partitions")).unwrap();
        let new_walls = tx.add_layer(Layer::named("walls")).unwrap();
        tx.commit();
        assert_eq!(drawing.layers().find("walls"), Some(new_walls));
        assert_eq!(drawing.layers().find("partitions"), Some(walls));
    }

    #[test]
    fn new_entities_go_on_the_current_layer_and_changing_it_is_undoable() {
        let mut drawing = Drawing::new();
        let walls = add_layer(&mut drawing, "walls");
        assert_eq!(drawing.current_layer(), LayerId::FIRST);

        let mut tx = drawing.transaction();
        tx.set_current_layer(walls).unwrap();
        tx.commit();
        assert_eq!(drawing.current_layer(), walls);
        assert_eq!(
            drawing.settings(),
            DrawingSettings {
                current_layer: walls,
                ..DrawingSettings::default()
            }
        );
        let on_walls = create(&mut drawing, line(0.0, 0.0, 10.0, 0.0));
        assert_eq!(drawing.properties(on_walls), Some(Properties::on(walls)));

        drawing.undo().unwrap(); // the line
        drawing.undo().unwrap(); // the setting
        assert_eq!(drawing.current_layer(), LayerId::FIRST);
        let on_zero = create(&mut drawing, line(0.0, 5.0, 10.0, 5.0));
        assert_eq!(drawing.properties(on_zero), Some(Properties::default()));

        // Set and draw in one transaction: the entity is on the new layer,
        // and setting twice records one change from the first value to the last.
        let doors = add_layer(&mut drawing, "doors");
        let mut tx = drawing.transaction();
        tx.set_current_layer(walls).unwrap();
        tx.set_current_layer(doors).unwrap();
        let id = tx.insert(line(0.0, 9.0, 10.0, 9.0)).unwrap();
        tx.commit();
        assert_eq!(drawing.properties(id), Some(Properties::on(doors)));
        drawing.undo().unwrap();
        assert_eq!(
            drawing.current_layer(),
            LayerId::FIRST,
            "one step back to where it was"
        );
    }

    /// #135: new objects take the current colour; it is a setting, undone
    /// like any other, and an object keeps what it was given.
    #[test]
    fn new_entities_take_the_current_colour_which_is_an_undoable_setting() {
        let mut drawing = Drawing::new();
        assert_eq!(drawing.current_color(), Color::ByLayer);
        let walls = add_layer(&mut drawing, "walls");
        let by_layer = create(&mut drawing, line(0.0, 0.0, 10.0, 0.0));

        let mut tx = drawing.transaction();
        tx.set_current_color(Color::Index(1));
        tx.set_current_layer(walls).unwrap();
        tx.commit();
        assert_eq!(drawing.current_color(), Color::Index(1));
        let red = create(&mut drawing, line(0.0, 5.0, 10.0, 5.0));
        assert_eq!(
            drawing.properties(red),
            Some(Properties {
                color: Color::Index(1),
                ..Properties::on(walls)
            })
        );
        assert_eq!(
            drawing.properties(by_layer),
            Some(Properties::default()),
            "untouched"
        );

        // An object made from another takes that one's colour, not the current one.
        let mut tx = drawing.transaction();
        let copy = tx.insert_like(by_layer, line(0.0, 9.0, 10.0, 9.0)).unwrap();
        tx.commit();
        assert_eq!(drawing.properties(copy).unwrap().color, Color::ByLayer);

        drawing.undo().unwrap(); // the copy
        drawing.undo().unwrap(); // the red line
        drawing.undo().unwrap(); // the settings: colour and layer together
        assert_eq!(
            (drawing.current_color(), drawing.current_layer()),
            (Color::ByLayer, LayerId::FIRST)
        );
        drawing.redo().unwrap();
        assert_eq!(drawing.current_color(), Color::Index(1));

        // Set and draw in one transaction.
        let mut tx = drawing.transaction();
        tx.set_current_color(Color::ByBlock);
        let id = tx.insert(line(0.0, 20.0, 10.0, 20.0)).unwrap();
        tx.commit();
        assert_eq!(drawing.properties(id).unwrap().color, Color::ByBlock);
    }

    /// #136: the same for the lineweight new objects are given.
    #[test]
    fn new_entities_take_the_current_lineweight_which_is_an_undoable_setting() {
        let mut drawing = Drawing::new();
        assert_eq!(drawing.current_lineweight(), Lineweight::ByLayer);
        let thin = create(&mut drawing, line(0.0, 0.0, 10.0, 0.0));

        let heavy = Lineweight::Hundredths(100);
        let mut tx = drawing.transaction();
        tx.set_current_lineweight(heavy);
        tx.commit();
        let wide = create(&mut drawing, line(0.0, 5.0, 10.0, 5.0));
        assert_eq!(drawing.properties(wide).unwrap().lineweight, heavy);
        assert_eq!(
            drawing.properties(thin).unwrap().lineweight,
            Lineweight::ByLayer
        );
        assert_eq!(
            drawing.current_color(),
            Color::ByLayer,
            "the other settings are kept"
        );

        drawing.undo().unwrap();
        drawing.undo().unwrap();
        assert_eq!(drawing.current_lineweight(), Lineweight::ByLayer);
        drawing.redo().unwrap();
        assert_eq!(drawing.current_lineweight(), heavy);
    }

    /// #113: changing the drawing unit is undoable, and changes no geometry.
    #[test]
    fn the_drawing_unit_is_a_setting_changed_and_undone_like_any_other() {
        let mut drawing = Drawing::new();
        assert_eq!(drawing.unit(), None, "a new drawing is unitless");
        let id = create(&mut drawing, line(0.0, 0.0, 25.0, 0.0));
        let walls = add_layer(&mut drawing, "walls");

        let mut tx = drawing.transaction();
        tx.set_unit(Some(LengthUnit::Millimetres));
        tx.commit();
        assert_eq!(drawing.unit(), Some(LengthUnit::Millimetres));
        assert_eq!(
            drawing.entity(id),
            Some(&Entity::Line(line(0.0, 0.0, 25.0, 0.0))),
            "the unit says what the numbers mean; it does not change them"
        );

        // The unit and the current layer are one settings record: each
        // change keeps the other.
        let mut tx = drawing.transaction();
        tx.set_current_layer(walls).unwrap();
        tx.commit();
        assert_eq!(drawing.unit(), Some(LengthUnit::Millimetres));
        let mut tx = drawing.transaction();
        tx.set_unit(Some(LengthUnit::Metres));
        tx.commit();
        assert_eq!(drawing.current_layer(), walls);

        drawing.undo().unwrap();
        assert_eq!(drawing.unit(), Some(LengthUnit::Millimetres));
        drawing.undo().unwrap();
        drawing.undo().unwrap();
        assert_eq!(drawing.unit(), None);
        drawing.redo().unwrap();
        assert_eq!(drawing.unit(), Some(LengthUnit::Millimetres));

        // Both in one transaction: one change, one step.
        let mut tx = drawing.transaction();
        tx.set_unit(Some(LengthUnit::Feet));
        tx.set_current_layer(walls).unwrap();
        tx.set_unit(None);
        tx.commit();
        assert_eq!((drawing.unit(), drawing.current_layer()), (None, walls));
        drawing.undo().unwrap();
        assert_eq!(
            (drawing.unit(), drawing.current_layer()),
            (Some(LengthUnit::Millimetres), LayerId::FIRST)
        );

        let imported = Drawing::new().with_unit(Some(LengthUnit::Inches));
        assert_eq!(imported.unit(), Some(LengthUnit::Inches));
        assert!(!imported.can_undo());
    }

    #[test]
    fn groups_hold_existing_entities_and_lose_them_as_they_are_removed() {
        let mut drawing = Drawing::new();
        let ids: Vec<EntityId> = (0..4)
            .map(|i| create(&mut drawing, line(f64::from(i), 0.0, f64::from(i), 10.0)))
            .collect();
        let mut tx = drawing.transaction();
        let door = tx
            .add_group(Group::named("Door", vec![ids[0], ids[1]]))
            .unwrap();
        let first = tx.add_group(Group::unnamed(vec![ids[2]])).unwrap();
        let second = tx.add_group(Group::unnamed(vec![ids[3], ids[2]])).unwrap();
        tx.commit();
        let names: Vec<&str> = drawing
            .groups()
            .iter()
            .map(|(_, g)| g.name.as_str())
            .collect();
        assert_eq!(names, ["Door", "*A1", "*A2"]);
        assert!(drawing.groups().get(first).unwrap().is_unnamed());
        assert_eq!(
            drawing
                .groups_of(ids[2])
                .map(|(id, _)| id)
                .collect::<Vec<_>>(),
            vec![first, second]
        );

        // The refusals.
        let mut tx = drawing.transaction();
        assert_eq!(
            tx.add_group(Group::named("DOOR", vec![ids[2]])),
            Err(TransactionError::DuplicateGroupName(door))
        );
        assert_eq!(
            tx.add_group(Group::named("a*b", vec![ids[2]])),
            Err(TransactionError::BadGroupName(NameError::Forbidden('*')))
        );
        assert_eq!(
            tx.add_group(Group::named("Empty", vec![])),
            Err(TransactionError::EmptyGroup)
        );
        let gone = EntityId::from_raw(99);
        assert_eq!(
            tx.add_group(Group::named("Ghost", vec![gone])),
            Err(TransactionError::BadGroupMember(gone))
        );
        assert_eq!(
            tx.add_group(Group::named("Twice", vec![ids[0], ids[0]])),
            Err(TransactionError::BadGroupMember(ids[0]))
        );
        tx.remove(ids[3]).unwrap();
        assert_eq!(
            tx.add_group(Group::named("Erased", vec![ids[3]])),
            Err(TransactionError::BadGroupMember(ids[3])),
            "a member removed in the same transaction"
        );
        drop(tx);

        // Both members of the door erased in one transaction: the group goes.
        let mut tx = drawing.transaction();
        tx.remove(ids[0]).unwrap();
        tx.remove(ids[1]).unwrap();
        tx.remove(ids[2]).unwrap();
        tx.commit();
        assert!(!drawing.groups().contains(door));
        assert!(!drawing.groups().contains(first));
        assert_eq!(drawing.groups().get(second).unwrap().members, vec![ids[3]]);
        drawing.undo();
        assert_eq!(
            drawing.groups().get(door).unwrap().members,
            vec![ids[0], ids[1]]
        );
        assert_eq!(drawing.groups().len(), 3);

        // Renamed, given a member back, made unselectable: one change.
        let mut tx = drawing.transaction();
        let mut renamed = drawing_group(&tx, door);
        renamed.name = "Front door".to_owned();
        renamed.selectable = false;
        tx.replace_group(door, renamed).unwrap();
        tx.commit();
        assert_eq!(drawing.groups().find("front door"), Some(door));
        assert!(!drawing.groups().get(door).unwrap().selectable);

        // From a file: by position, with the same checks, and no history.
        let mut imported = Drawing::from_entities([
            line(0.0, 0.0, 1.0, 1.0).into(),
            line(1.0, 1.0, 2.0, 0.0).into(),
        ]);
        let roof = Group::named("Roof", vec![EntityId::from_raw(1), EntityId::from_raw(2)]);
        imported.import_group(roof.clone()).unwrap();
        assert_eq!(imported.groups().len(), 1);
        assert!(!imported.can_undo());
        assert_eq!(
            imported.import_group(roof),
            Err(TransactionError::DuplicateGroupName(GroupId::from_raw(1)))
        );
        let ghost = EntityId::from_raw(7);
        assert_eq!(
            imported.import_group(Group::named("Shed", vec![ghost])),
            Err(TransactionError::BadGroupMember(ghost))
        );
        assert_eq!(imported.groups().len(), 1, "a refused group leaves nothing");
    }

    /// A group as the transaction will leave it, for a test to edit.
    fn drawing_group(tx: &TransactionBuilder<'_>, id: GroupId) -> Group {
        tx.group_now(id).cloned().unwrap()
    }

    #[test]
    fn a_frozen_layer_is_not_shown_and_the_current_layer_cannot_be_frozen() {
        let mut drawing = Drawing::new();
        let walls = add_layer(&mut drawing, "walls");
        let mut tx = drawing.transaction();
        let wall = tx
            .insert_with(
                LineEntity::new(Point2::new(0.0, 0.0), Point2::new(10.0, 0.0)),
                Properties::on(walls),
            )
            .unwrap();
        tx.commit();
        let shown = drawing.shown_revision();

        change_layer(&mut drawing, walls, |layer| layer.frozen = true).unwrap();
        assert!(!drawing.is_shown(wall));
        assert_eq!(drawing.shown().count(), 0);
        assert!(drawing.query_shown(&everything()).is_empty());
        assert_ne!(
            drawing.shown_revision(),
            shown,
            "freezing changes what is shown"
        );
        let mut tx = drawing.transaction();
        assert_eq!(
            tx.set_current_layer(walls),
            Err(TransactionError::LayerIsFrozen(walls))
        );
        drop(tx);

        // Thawed, it is shown again; off and thawed, it stays hidden.
        change_layer(&mut drawing, walls, |layer| {
            layer.on = false;
            layer.frozen = false;
        })
        .unwrap();
        assert!(!drawing.is_shown(wall));
        change_layer(&mut drawing, walls, |layer| layer.on = true).unwrap();
        assert!(drawing.is_shown(wall));

        assert_eq!(
            change_layer(&mut drawing, LayerId::FIRST, |layer| layer.frozen = true),
            Err(TransactionError::CurrentLayer(LayerId::FIRST))
        );
        drawing.undo();
        assert!(!drawing.is_shown(wall), "undone, it is off again");
    }

    #[test]
    fn the_current_layer_cannot_be_removed_switched_off_or_be_an_off_layer() {
        let mut drawing = Drawing::new();
        let walls = add_layer(&mut drawing, "walls");
        let hidden = add_layer(&mut drawing, "hidden");
        change_layer(&mut drawing, hidden, |layer| layer.on = false).unwrap();

        let mut tx = drawing.transaction();
        assert_eq!(
            tx.set_current_layer(hidden),
            Err(TransactionError::LayerIsOff(hidden))
        );
        assert_eq!(
            tx.set_current_layer(LayerId::from_raw(50)),
            Err(TransactionError::UnknownLayer(LayerId::from_raw(50)))
        );
        tx.set_current_layer(walls).unwrap();
        // Already current within this transaction.
        assert_eq!(
            tx.remove_layer(walls),
            Err(TransactionError::CurrentLayer(walls))
        );
        tx.commit();

        assert_eq!(
            change_layer(&mut drawing, walls, |layer| layer.on = false),
            Err(TransactionError::CurrentLayer(walls))
        );
        let mut tx = drawing.transaction();
        assert_eq!(
            tx.remove_layer(walls),
            Err(TransactionError::CurrentLayer(walls))
        );
        drop(tx);
        // Locking the current layer is allowed: new objects can still be drawn on it.
        change_layer(&mut drawing, walls, |layer| layer.locked = true).unwrap();
        assert_eq!(drawing.current_layer(), walls);
    }

    /// #134: what is shown has its own revision, which a change to how a
    /// layer looks does not touch.
    #[test]
    fn the_shown_revision_changes_with_entities_and_layer_visibility_only() {
        let mut drawing = Drawing::new();
        let walls = add_layer(&mut drawing, "walls");
        let id = create(&mut drawing, line(0.0, 0.0, 10.0, 0.0));

        let expect = |drawing: &mut Drawing,
                      changes: bool,
                      what: &str,
                      edit: &mut dyn FnMut(&mut Drawing)| {
            let (shown, all) = (drawing.shown_revision(), drawing.revision());
            edit(drawing);
            assert_ne!(drawing.revision(), all, "{what}: the drawing changed");
            assert_eq!(drawing.shown_revision() != shown, changes, "{what}");
        };

        expect(&mut drawing, false, "a new layer", &mut |d| {
            add_layer(d, "doors");
        });
        expect(&mut drawing, false, "a layer's colour", &mut |d| {
            change_layer(d, walls, |layer| layer.color = crate::Color::Index(1)).unwrap();
        });
        expect(&mut drawing, false, "a layer's name", &mut |d| {
            change_layer(d, walls, |layer| layer.name = "partitions".to_owned()).unwrap();
        });
        expect(&mut drawing, false, "a layer locked", &mut |d| {
            change_layer(d, walls, |layer| layer.locked = true).unwrap();
        });
        expect(&mut drawing, false, "the current layer", &mut |d| {
            let mut tx = d.transaction();
            tx.set_unit(Some(LengthUnit::Metres));
            tx.commit();
        });
        expect(&mut drawing, false, "undoing a setting", &mut |d| {
            d.undo().unwrap();
        });

        expect(&mut drawing, true, "a layer switched off", &mut |d| {
            change_layer(d, walls, |layer| layer.on = false).unwrap();
        });
        expect(&mut drawing, true, "undoing that", &mut |d| {
            d.undo().unwrap();
        });
        expect(&mut drawing, true, "an entity drawn", &mut |d| {
            create(d, line(0.0, 5.0, 10.0, 5.0));
        });
        expect(&mut drawing, true, "an entity moved", &mut |d| {
            let mut tx = d.transaction();
            tx.replace(id, line(1.0, 1.0, 9.0, 9.0)).unwrap();
            tx.commit();
        });
        expect(&mut drawing, true, "an entity's properties", &mut |d| {
            let mut tx = d.transaction();
            tx.set_properties(id, Properties::on(walls)).unwrap();
            tx.commit();
        });
        expect(&mut drawing, true, "an entity erased", &mut |d| {
            let mut tx = d.transaction();
            tx.remove(id).unwrap();
            tx.commit();
        });
        expect(&mut drawing, true, "redo after undo", &mut |d| {
            d.undo().unwrap();
        });
    }

    // ── linetypes (#138) ──

    fn load(drawing: &mut Drawing, name: &str) -> LinetypeId {
        let mut tx = drawing.transaction();
        let id = tx
            .add_linetype(LinetypeDef::built_in_named(name).expect("a built-in linetype"))
            .unwrap();
        tx.commit();
        id
    }

    #[test]
    fn linetypes_are_loaded_once_each_and_continuous_stays() {
        let mut drawing = Drawing::new();
        assert_eq!(drawing.linetypes().len(), 1);
        let dashed = load(&mut drawing, "Dashed");
        assert_eq!(dashed, LinetypeId::from_raw(1));
        assert_eq!(drawing.linetypes().find("dashed"), Some(dashed));

        let mut tx = drawing.transaction();
        assert_eq!(
            tx.add_linetype(LinetypeDef::built_in_named("DASHED").unwrap()),
            Err(TransactionError::DuplicateLinetypeName(dashed))
        );
        assert_eq!(
            tx.add_linetype(LinetypeDef {
                name: "continuous".to_owned(),
                ..LinetypeDef::built_in_named("Hidden").unwrap()
            }),
            Err(TransactionError::DuplicateLinetypeName(LinetypeId::FIRST))
        );
        assert_eq!(
            tx.add_linetype(LinetypeDef {
                pattern: vec![0.5],
                ..LinetypeDef::built_in_named("Hidden").unwrap()
            }),
            Err(TransactionError::BadLinetype(LinetypeError::Odd))
        );
        // Two of the same name in one transaction: the second is refused.
        tx.add_linetype(LinetypeDef::built_in_named("Hidden").unwrap())
            .unwrap();
        assert!(matches!(
            tx.add_linetype(LinetypeDef::built_in_named("hidden").unwrap()),
            Err(TransactionError::DuplicateLinetypeName(_))
        ));
        assert_eq!(
            tx.remove_linetype(LinetypeId::FIRST),
            Err(TransactionError::PermanentLinetype)
        );
        tx.commit();
        assert_eq!(drawing.linetypes().len(), 3);

        drawing.undo();
        assert_eq!(drawing.linetypes().len(), 2, "one transaction, one step");
        drawing.undo();
        assert_eq!(drawing.linetypes().len(), 1);
        drawing.redo();
        drawing.redo();
        assert_eq!(
            drawing.linetypes().find("hidden"),
            Some(LinetypeId::from_raw(2))
        );
        // IDs are never reused, even after an undo.
        drawing.undo();
        drawing.undo();
        assert_eq!(load(&mut drawing, "Center"), LinetypeId::from_raw(3));
    }

    #[test]
    fn a_linetype_in_use_cannot_be_removed() {
        let mut drawing = Drawing::new();
        let dashed = load(&mut drawing, "Dashed");
        let hidden = load(&mut drawing, "Hidden");
        let center = load(&mut drawing, "Center");
        let phantom = load(&mut drawing, "Phantom");

        // By an entity.
        let mut tx = drawing.transaction();
        let id = tx
            .insert_with(
                line(0.0, 0.0, 1.0, 0.0),
                Properties {
                    linetype: Linetype::Named(dashed),
                    ..Properties::default()
                },
            )
            .unwrap();
        tx.commit();
        // By a layer.
        let mut tx = drawing.transaction();
        let walls = tx
            .add_layer(Layer {
                linetype: Linetype::Named(hidden),
                ..Layer::named("walls")
            })
            .unwrap();
        tx.commit();
        // As the setting for new objects.
        let mut tx = drawing.transaction();
        tx.set_current_linetype(Linetype::Named(center)).unwrap();
        tx.commit();

        let mut tx = drawing.transaction();
        assert_eq!(
            tx.remove_linetype(dashed),
            Err(TransactionError::LinetypeInUse(dashed))
        );
        assert_eq!(
            tx.remove_linetype(hidden),
            Err(TransactionError::LinetypeInUse(hidden))
        );
        assert_eq!(
            tx.remove_linetype(center),
            Err(TransactionError::LinetypeInUse(center))
        );
        assert_eq!(
            tx.remove_linetype(LinetypeId::from_raw(40)),
            Err(TransactionError::UnknownLinetype(LinetypeId::from_raw(40)))
        );
        // Given to something in this very transaction counts too.
        tx.set_properties(
            id,
            Properties {
                linetype: Linetype::Named(phantom),
                ..Properties::default()
            },
        )
        .unwrap();
        assert_eq!(
            tx.remove_linetype(phantom),
            Err(TransactionError::LinetypeInUse(phantom))
        );
        drop(tx);

        // Taken off everything, it goes — in a transaction of its own.
        let mut tx = drawing.transaction();
        tx.set_properties(id, Properties::default()).unwrap();
        tx.replace_layer(walls, Layer::named("walls")).unwrap();
        tx.set_current_linetype(Linetype::ByLayer).unwrap();
        tx.commit();
        let mut tx = drawing.transaction();
        tx.remove_linetype(dashed).unwrap();
        tx.remove_linetype(hidden).unwrap();
        tx.remove_linetype(center).unwrap();
        assert_eq!(
            tx.remove_linetype(dashed),
            Err(TransactionError::LinetypeAlreadyChanged(dashed))
        );
        tx.commit();
        assert_eq!(drawing.linetypes().len(), 2);
        drawing.undo();
        assert_eq!(drawing.linetypes().len(), 5);
    }

    #[test]
    fn nothing_can_refer_to_a_linetype_that_does_not_exist() {
        let mut drawing = Drawing::new();
        let gone = LinetypeId::from_raw(9);
        let unknown = TransactionError::UnknownLinetype(gone);
        let mut tx = drawing.transaction();
        assert_eq!(tx.set_current_linetype(Linetype::Named(gone)), Err(unknown));
        assert_eq!(
            tx.add_layer(Layer {
                linetype: Linetype::Named(gone),
                ..Layer::named("walls")
            }),
            Err(unknown)
        );
        assert_eq!(
            tx.replace_layer(
                LayerId::FIRST,
                Layer {
                    linetype: Linetype::Named(gone),
                    ..Layer::named("0")
                }
            ),
            Err(unknown)
        );
        let dotted = Properties {
            linetype: Linetype::Named(gone),
            ..Properties::default()
        };
        assert_eq!(
            tx.insert_with(line(0.0, 0.0, 1.0, 0.0), dotted),
            Err(unknown)
        );
        let id = tx.insert(line(0.0, 0.0, 1.0, 0.0)).unwrap();
        tx.commit();
        let mut tx = drawing.transaction();
        assert_eq!(tx.set_properties(id, dotted), Err(unknown));
        // A linetype loaded in this transaction can be used in it.
        let dashed = tx
            .add_linetype(LinetypeDef::built_in_named("Dashed").unwrap())
            .unwrap();
        tx.set_current_linetype(Linetype::Named(dashed)).unwrap();
        let second = tx.insert(line(0.0, 1.0, 1.0, 1.0)).unwrap();
        tx.commit();
        assert_eq!(
            drawing.properties(second).unwrap().linetype,
            Linetype::Named(dashed),
            "new objects are given the current linetype"
        );
        assert_eq!(drawing.properties(id).unwrap().linetype, Linetype::ByLayer);

        assert_eq!(
            Drawing::from_parts(
                [Layer {
                    linetype: Linetype::Named(gone),
                    ..Layer::named("walls")
                }],
                [],
                []
            )
            .err(),
            Some(TransactionError::UnknownLinetype(gone))
        );
        assert_eq!(
            Drawing::from_parts([], [], [(Entity::Line(line(0.0, 0.0, 1.0, 0.0)), dotted)]).err(),
            Some(TransactionError::UnknownLinetype(gone))
        );
        let loaded = Drawing::from_parts(
            [],
            [LinetypeDef::built_in_named("Hidden").unwrap()],
            [(
                Entity::Line(line(0.0, 0.0, 1.0, 0.0)),
                Properties {
                    linetype: Linetype::Named(LinetypeId::from_raw(1)),
                    ..Properties::default()
                },
            )],
        )
        .unwrap();
        assert_eq!(
            loaded.linetypes().find("hidden"),
            Some(LinetypeId::from_raw(1))
        );
        assert!(!loaded.can_undo());
    }

    #[test]
    fn a_drawing_from_a_file_can_restyle_its_permanent_rows() {
        let drawing = Drawing::from_parts(
            [
                Layer {
                    color: Color::Index(3),
                    locked: true,
                    ..Layer::named("0")
                },
                Layer::named("walls"),
            ],
            [
                LinetypeDef {
                    description: "Unbroken".to_owned(),
                    ..LinetypeDef::continuous()
                },
                LinetypeDef::built_in_named("Dashed").unwrap(),
            ],
            [],
        )
        .unwrap();
        assert_eq!(drawing.layers().len(), 2);
        let zero = drawing.layers().get(LayerId::FIRST).unwrap();
        assert_eq!((zero.color, zero.locked), (Color::Index(3), true));
        assert_eq!(drawing.layers().find("walls"), Some(LayerId::from_raw(1)));
        assert_eq!(drawing.linetypes().len(), 2);
        assert_eq!(
            drawing
                .linetypes()
                .get(LinetypeId::FIRST)
                .unwrap()
                .description,
            "Unbroken"
        );
        assert_eq!(
            drawing.linetypes().find("dashed"),
            Some(LinetypeId::from_raw(1))
        );
    }

    #[test]
    fn a_drawing_from_a_file_can_have_text_styles_that_its_texts_refer_to() {
        let roman = TextStyleId::from_raw(1);
        let text = |style| {
            Entity::Text(TextEntity {
                style,
                ..TextEntity::new(Point2::ORIGIN, 2.5, 0.0, "Hi")
            })
        };
        let drawing = Drawing::from_tables(
            [],
            [],
            [TextStyle::named("standard"), TextStyle::named("Roman")],
            [],
            [],
            [(text(roman), Properties::default())],
        )
        .unwrap();
        assert_eq!(
            drawing.text_styles().len(),
            2,
            "standard restyled the permanent row"
        );
        assert_eq!(
            drawing.text_styles().get(TextStyleId::FIRST).unwrap().name,
            "standard"
        );
        assert_eq!(drawing.text_styles().find("roman"), Some(roman));
        assert_eq!(
            Drawing::from_tables([], [], [], [], [], [(text(roman), Properties::default())]).err(),
            Some(TransactionError::UnknownTextStyle(roman))
        );
        assert_eq!(
            Drawing::from_tables(
                [],
                [],
                [TextStyle::named("a"), TextStyle::named("A")],
                [],
                [],
                []
            )
            .err(),
            Some(TransactionError::DuplicateTextStyleName(
                TextStyleId::from_raw(1)
            ))
        );
    }

    #[test]
    fn a_drawing_from_a_file_can_have_blocks_that_place_each_other_in_any_order() {
        // IDs follow the order given: the pair first, then the chair.
        let pair = BlockId::from_raw(1);
        let chair = BlockId::from_raw(2);
        let place = |block, x| {
            EntityRecord::new(
                BlockReferenceEntity::new(block, Point2::new(x, 0.0), 1.0, 0.0),
                Properties::default(),
            )
        };
        let seat = || {
            EntityRecord::new(
                LineEntity::new(Point2::ORIGIN, Point2::new(10.0, 0.0)),
                Properties::default(),
            )
        };
        // The pair comes first in the file and places the chair defined after it.
        let drawing = Drawing::from_tables(
            [],
            [],
            [],
            [],
            [
                Block::new(
                    "Pair",
                    Point2::ORIGIN,
                    vec![place(chair, 0.0), place(chair, 20.0)],
                ),
                Block::new("Chair", Point2::ORIGIN, vec![seat()]),
            ],
            [(
                BlockReferenceEntity::new(pair, Point2::new(100.0, 0.0), 1.0, 0.0).into(),
                Properties::default(),
            )],
        )
        .unwrap();
        assert_eq!(drawing.blocks().find("pair"), Some(pair));
        assert_eq!(
            drawing
                .bounds_of(drawing.entities().next().unwrap().1)
                .max(),
            Point2::new(130.0, 0.0)
        );

        // A cycle through two blocks is refused, whichever is checked first.
        let cyclic = Drawing::from_tables(
            [],
            [],
            [],
            [],
            [
                Block::new("A", Point2::ORIGIN, vec![place(BlockId::from_raw(2), 0.0)]),
                Block::new("B", Point2::ORIGIN, vec![place(BlockId::from_raw(1), 0.0)]),
            ],
            [],
        );
        assert!(
            matches!(cyclic, Err(TransactionError::BlockCycle(_))),
            "{cyclic:?}"
        );
        // So are a reference to a block the file lacks, a bad scale, a
        // record on a layer the file lacks, and a name taken twice.
        assert_eq!(
            Drawing::from_tables(
                [],
                [],
                [],
                [],
                [],
                [(
                    BlockReferenceEntity::new(BlockId::from_raw(3), Point2::ORIGIN, 1.0, 0.0)
                        .into(),
                    Properties::default()
                )],
            )
            .err(),
            Some(TransactionError::UnknownBlock(BlockId::from_raw(3)))
        );
        assert_eq!(
            Drawing::from_tables(
                [],
                [],
                [],
                [],
                [Block::new("Chair", Point2::ORIGIN, vec![seat()])],
                [(
                    BlockReferenceEntity::new(BlockId::from_raw(1), Point2::ORIGIN, 0.0, 0.0)
                        .into(),
                    Properties::default()
                )],
            )
            .err(),
            Some(TransactionError::BadBlockScale)
        );
        assert_eq!(
            Drawing::from_tables(
                [],
                [],
                [],
                [],
                [Block::new(
                    "Chair",
                    Point2::ORIGIN,
                    vec![EntityRecord::new(
                        LineEntity::new(Point2::ORIGIN, Point2::new(1.0, 0.0)),
                        Properties::on(LayerId::from_raw(4)),
                    )]
                )],
                [],
            )
            .err(),
            Some(TransactionError::UnknownLayer(LayerId::from_raw(4)))
        );
        assert_eq!(
            Drawing::from_tables(
                [],
                [],
                [],
                [],
                [
                    Block::new("Chair", Point2::ORIGIN, vec![]),
                    Block::new("CHAIR", Point2::ORIGIN, vec![])
                ],
                [],
            )
            .err(),
            Some(TransactionError::DuplicateBlockName(BlockId::from_raw(1)))
        );
    }

    #[test]
    fn a_drawing_from_a_file_takes_its_settings_without_history() {
        let drawing = Drawing::from_parts(
            [Layer::named("walls")],
            [LinetypeDef::built_in_named("Dashed").unwrap()],
            [],
        )
        .unwrap();
        let settings = DrawingSettings {
            current_layer: LayerId::from_raw(1),
            current_color: Color::Index(3),
            current_linetype: Linetype::Named(LinetypeId::from_raw(1)),
            current_lineweight: Lineweight::Hundredths(50),
            current_dim_style: DimStyleId::FIRST,
            current_text_style: TextStyleId::FIRST,
            current_mleader_style: crate::mleader::MLeaderStyleId::FIRST,
            ltscale: 25.0,
            unit: Some(LengthUnit::Millimetres),
            point_style: PointStyle::new(3, 2.0),
            wipeout_frame: false,
        };
        let drawing = drawing.with_settings(settings).unwrap();
        assert_eq!(drawing.settings(), settings);
        assert!(!drawing.can_undo(), "a file is not an edit");

        let fresh = || Drawing::from_parts([], [], []).unwrap();
        assert_eq!(
            fresh().with_settings(settings).err(),
            Some(TransactionError::UnknownLayer(LayerId::from_raw(1)))
        );
        assert_eq!(
            fresh()
                .with_settings(DrawingSettings {
                    ltscale: 0.0,
                    ..DrawingSettings::default()
                })
                .err(),
            Some(TransactionError::BadLtscale)
        );
    }

    #[test]
    fn dimension_styles_are_a_table_with_standard_that_new_dimensions_take() {
        let mut drawing = Drawing::new();
        assert_eq!(drawing.dim_styles().len(), 1);
        assert_eq!(drawing.current_dim_style(), DimStyleId::FIRST);

        let mut tx = drawing.transaction();
        let arch = tx.add_dim_style(DimStyle::named("Arch")).unwrap();
        assert_eq!(arch, DimStyleId::from_raw(1));
        assert_eq!(
            tx.add_dim_style(DimStyle::named("ARCH")),
            Err(TransactionError::DuplicateDimStyleName(arch))
        );
        assert_eq!(
            tx.add_dim_style(DimStyle::named("standard")),
            Err(TransactionError::DuplicateDimStyleName(DimStyleId::FIRST))
        );
        assert_eq!(
            tx.add_dim_style(DimStyle::named("a/b")),
            Err(TransactionError::BadDimStyleName(NameError::Forbidden('/')))
        );
        let mut flat = DimStyle::named("Flat");
        flat.text_height = 0.0;
        assert_eq!(
            tx.add_dim_style(flat),
            Err(TransactionError::BadDimStyle(DimStyleError::NotPositive(
                "text height"
            )))
        );
        tx.set_current_dim_style(arch).unwrap();
        assert_eq!(
            tx.set_current_dim_style(DimStyleId::from_raw(9)),
            Err(TransactionError::UnknownDimStyle(DimStyleId::from_raw(9)))
        );
        tx.commit();
        assert_eq!(drawing.current_dim_style(), arch);

        // The current style cannot go; Standard never can; the values of
        // either may change, and a style other than Standard may be renamed.
        let arch_row = drawing.dim_styles().get(arch).unwrap().clone();
        let standard_row = drawing.dim_styles().get(DimStyleId::FIRST).unwrap().clone();
        let mut tx = drawing.transaction();
        assert_eq!(
            tx.remove_dim_style(arch),
            Err(TransactionError::DimStyleInUse(arch))
        );
        assert_eq!(
            tx.remove_dim_style(DimStyleId::FIRST),
            Err(TransactionError::PermanentDimStyle)
        );
        let mut taller = arch_row;
        taller.text_height = 5.0;
        tx.replace_dim_style(arch, taller.clone()).unwrap();
        assert_eq!(
            tx.replace_dim_style(arch, taller.renamed("Standard")),
            Err(TransactionError::DuplicateDimStyleName(DimStyleId::FIRST))
        );
        assert_eq!(
            tx.replace_dim_style(DimStyleId::FIRST, standard_row.renamed("Std")),
            Err(TransactionError::PermanentDimStyle)
        );
        tx.replace_dim_style(arch, taller.renamed("Architectural"))
            .unwrap();
        tx.commit();
        assert_eq!(
            drawing.dim_styles().get(arch).unwrap().name,
            "Architectural"
        );
        assert_eq!(drawing.dim_styles().get(arch).unwrap().text_height, 5.0);

        // Back to Standard, then the other style can go; undo brings it back.
        let mut tx = drawing.transaction();
        tx.set_current_dim_style(DimStyleId::FIRST).unwrap();
        tx.remove_dim_style(arch).unwrap();
        tx.commit();
        assert_eq!(drawing.dim_styles().len(), 1);
        drawing.undo().unwrap();
        assert_eq!(drawing.dim_styles().len(), 2, "one history for all tables");
        assert_eq!(drawing.current_dim_style(), arch);
    }

    #[test]
    fn text_styles_are_a_table_with_standard_that_texts_refer_to() {
        let mut drawing = Drawing::new();
        assert_eq!(drawing.text_styles().len(), 1);
        let mut tx = drawing.transaction();
        let roman = tx.add_text_style(TextStyle::named("Roman")).unwrap();
        assert_eq!(roman, TextStyleId::from_raw(1));
        assert_eq!(
            tx.add_text_style(TextStyle::named("ROMAN")),
            Err(TransactionError::DuplicateTextStyleName(roman))
        );
        assert_eq!(
            tx.add_text_style(TextStyle::named("standard")),
            Err(TransactionError::DuplicateTextStyleName(TextStyleId::FIRST))
        );
        assert_eq!(
            tx.add_text_style(TextStyle::named("a,b")),
            Err(TransactionError::BadTextStyleName(NameError::Forbidden(
                ','
            )))
        );
        assert_eq!(
            tx.remove_text_style(TextStyleId::FIRST),
            Err(TransactionError::PermanentTextStyle)
        );
        // A text in the new style, in the same transaction.
        let text = TextEntity {
            style: roman,
            ..TextEntity::new(Point2::ORIGIN, 2.5, 0.0, "Hello")
        };
        let id = tx.insert(text.clone()).unwrap();
        assert_eq!(
            tx.remove_text_style(roman),
            Err(TransactionError::TextStyleInUse(roman))
        );
        let ghost = TextEntity {
            style: TextStyleId::from_raw(9),
            ..text.clone()
        };
        assert_eq!(
            tx.insert(ghost.clone()),
            Err(TransactionError::UnknownTextStyle(TextStyleId::from_raw(9)))
        );
        tx.commit();

        let mut tx = drawing.transaction();
        assert_eq!(
            tx.replace(id, ghost),
            Err(TransactionError::UnknownTextStyle(TextStyleId::from_raw(9)))
        );
        tx.replace(id, text.saying("Changed")).unwrap();
        assert_eq!(
            tx.remove_text_style(roman),
            Err(TransactionError::TextStyleInUse(roman))
        );
        tx.commit();
        let mut tx = drawing.transaction();
        tx.remove(id).unwrap();
        tx.commit();
        let mut tx = drawing.transaction();
        tx.remove_text_style(roman).unwrap();
        tx.commit();
        assert_eq!(drawing.text_styles().len(), 1);
        drawing.undo();
        assert_eq!(drawing.text_styles().len(), 2, "one history for all tables");
    }

    // ── blocks (#146, ADR-0017) ──

    fn chair_block() -> Block {
        Block::new(
            "Chair",
            Point2::ORIGIN,
            vec![
                EntityRecord::new(line(0.0, 0.0, 10.0, 0.0), Properties::default()),
                EntityRecord::new(line(0.0, 0.0, 0.0, 10.0), Properties::default()),
            ],
        )
    }

    fn define(drawing: &mut Drawing, block: Block) -> BlockId {
        let mut tx = drawing.transaction();
        let id = tx.define_block(block).unwrap();
        tx.commit();
        id
    }

    fn place(drawing: &mut Drawing, block: BlockId, at: Point2) -> EntityId {
        let mut tx = drawing.transaction();
        let id = tx
            .insert(BlockReferenceEntity::new(block, at, 1.0, 0.0))
            .unwrap();
        tx.commit();
        id
    }

    #[test]
    fn the_curves_larger_than_a_radius_follow_every_edit_and_redefinition() {
        use crate::{ArcEntity, CircleEntity, PolylineEntity};
        let mut drawing = Drawing::new();
        let larger = |drawing: &Drawing, radius: f64| -> Vec<EntityId> {
            drawing.curves_larger_than(radius).collect()
        };
        let mut tx = drawing.transaction();
        let small = tx.insert(CircleEntity::new(Point2::ORIGIN, 5.0)).unwrap();
        let big = tx
            .insert(ArcEntity::new(Point2::ORIGIN, 500.0, 0.0, 0.1))
            .unwrap();
        tx.insert(line(0.0, 0.0, 1e6, 0.0)).unwrap();
        // A half-circle segment of a polyline: a radius of 50.
        let bulged = tx
            .insert(PolylineEntity::with_bulges(
                vec![Point2::ORIGIN, Point2::new(100.0, 0.0)],
                vec![1.0, 0.0],
                false,
            ))
            .unwrap();
        tx.commit();
        assert_eq!(
            larger(&drawing, 1.0),
            vec![big, bulged, small],
            "largest first"
        );
        assert_eq!(larger(&drawing, 50.0), vec![big], "over, not at");
        assert!(larger(&drawing, 1000.0).is_empty());

        // A curve made a line is no longer one; one removed is gone.
        let mut tx = drawing.transaction();
        tx.replace(small, line(0.0, 0.0, 1.0, 1.0)).unwrap();
        tx.remove(big).unwrap();
        tx.commit();
        assert_eq!(larger(&drawing, 1.0), vec![bulged]);
        drawing.undo();
        assert_eq!(
            larger(&drawing, 1.0),
            vec![big, bulged, small],
            "and undone"
        );

        // A reference has its definition's curves, and follows a redefinition.
        let wheel = define(
            &mut drawing,
            Block::new(
                "Wheel",
                Point2::ORIGIN,
                vec![EntityRecord::new(
                    CircleEntity::new(Point2::ORIGIN, 10.0),
                    Properties::default(),
                )],
            ),
        );
        let placed = place(&mut drawing, wheel, Point2::new(3.0, 3.0));
        assert_eq!(larger(&drawing, 6.0), vec![big, bulged, placed]);
        let mut tx = drawing.transaction();
        tx.redefine_block(wheel, chair_block()).unwrap();
        tx.commit();
        assert_eq!(larger(&drawing, 1.0), vec![big, bulged, small]);
    }

    #[test]
    fn a_block_is_defined_placed_and_its_references_have_its_bounds() {
        let mut drawing = Drawing::new();
        let chair = define(&mut drawing, chair_block());
        assert_eq!(chair, BlockId::from_raw(1));
        assert_eq!(drawing.blocks().find("CHAIR"), Some(chair));
        let a = place(&mut drawing, chair, Point2::new(100.0, 100.0));
        let bounds = drawing.bounds_of(drawing.entity(a).unwrap());
        assert_eq!(bounds.min(), Point2::new(100.0, 100.0));
        assert_eq!(bounds.max(), Point2::new(110.0, 110.0));
        // The index knows the reference by its definition's extent.
        let hit = Aabb::from_points(Point2::new(108.0, 100.5), Point2::new(109.0, 101.0));
        assert_eq!(drawing.query(&hit), vec![a]);
        assert!(
            drawing
                .query(&Aabb::from_points(
                    Point2::new(120.0, 120.0),
                    Point2::new(130.0, 130.0)
                ))
                .is_empty()
        );
        assert_eq!(drawing.extents(), Some(bounds));
    }

    #[test]
    fn redefining_a_block_is_one_change_that_moves_every_reference_and_undoes_as_one() {
        let mut drawing = Drawing::new();
        let chair = define(&mut drawing, chair_block());
        let ids: Vec<EntityId> = (0..10_000)
            .map(|i| place(&mut drawing, chair, Point2::new(f64::from(i) * 20.0, 0.0)))
            .collect();
        let far = Aabb::from_points(Point2::new(-5.0, 12.0), Point2::new(5.0, 40.0));
        assert!(drawing.query(&far).is_empty());

        // A taller chair: the one change in the transaction.
        let mut tx = drawing.transaction();
        tx.redefine_block(
            chair,
            Block::new(
                "Chair",
                Point2::ORIGIN,
                vec![EntityRecord::new(
                    line(0.0, 0.0, 0.0, 30.0),
                    Properties::default(),
                )],
            ),
        )
        .unwrap();
        let report = tx.commit();
        assert_eq!(
            report.replaced.len(),
            10_000,
            "every reference is derived state that changed"
        );
        let history_entry = drawing.history_len_for_test();
        assert_eq!(
            history_entry,
            Some(10_002),
            "a definition, ten thousand inserts, one redefinition"
        );
        assert_eq!(drawing.query(&far), vec![ids[0]], "the index follows");
        let bounds = drawing.bounds_of(drawing.entity(ids[9_999]).unwrap());
        assert_eq!(bounds.max(), Point2::new(199_980.0, 30.0));

        drawing.undo();
        assert!(
            drawing.query(&far).is_empty(),
            "undone, the chairs are short again"
        );
        drawing.redo();
        assert_eq!(drawing.query(&far), vec![ids[0]]);
        // Fresh index equals the maintained one.
        let mut fresh = LinearIndex::new();
        fresh.rebuild(&mut drawing.entities().map(|(id, e)| (id, drawing.bounds_of(e))));
        assert_eq!(
            sorted(drawing.query(&everything())),
            sorted(fresh.query(&everything()))
        );
    }

    #[test]
    fn a_block_cannot_hold_itself_directly_or_through_others() {
        let mut drawing = Drawing::new();
        let chair = define(&mut drawing, chair_block());
        let pair = define(
            &mut drawing,
            Block::new(
                "Pair",
                Point2::ORIGIN,
                vec![EntityRecord::new(
                    BlockReferenceEntity::new(chair, Point2::new(20.0, 0.0), 1.0, 0.0),
                    Properties::default(),
                )],
            ),
        );
        let holding = |block: BlockId| {
            Block::new(
                "Chair",
                Point2::ORIGIN,
                vec![EntityRecord::new(
                    BlockReferenceEntity::new(block, Point2::ORIGIN, 1.0, 0.0),
                    Properties::default(),
                )],
            )
        };
        let mut tx = drawing.transaction();
        assert_eq!(
            tx.redefine_block(chair, holding(chair)),
            Err(TransactionError::BlockCycle(chair))
        );
        assert_eq!(
            tx.redefine_block(chair, holding(pair)),
            Err(TransactionError::BlockCycle(chair))
        );
        // A definition may hold the drawing itself no more than a reference may.
        assert_eq!(
            tx.define_block(Block::new(
                "Odd",
                Point2::ORIGIN,
                vec![EntityRecord::new(
                    BlockReferenceEntity::new(BlockId::FIRST, Point2::ORIGIN, 1.0, 0.0),
                    Properties::default(),
                )]
            )),
            Err(TransactionError::PermanentBlock)
        );
        assert_eq!(
            tx.define_block(Block::new(
                "Odd",
                Point2::ORIGIN,
                vec![EntityRecord::new(
                    BlockReferenceEntity::new(BlockId::from_raw(40), Point2::ORIGIN, 1.0, 0.0),
                    Properties::default(),
                )]
            )),
            Err(TransactionError::UnknownBlock(BlockId::from_raw(40)))
        );
        assert_eq!(
            tx.define_block(Block::new("chair", Point2::ORIGIN, vec![])),
            Err(TransactionError::DuplicateBlockName(chair))
        );
        assert_eq!(
            tx.define_block(Block::new("a*b", Point2::ORIGIN, vec![])),
            Err(TransactionError::BadBlockName(NameError::Forbidden('*')))
        );
        assert_eq!(
            tx.redefine_block(BlockId::FIRST, chair_block()),
            Err(TransactionError::PermanentBlock)
        );
        // A reference must place something that exists, at a positive scale.
        assert_eq!(
            tx.insert(BlockReferenceEntity::new(
                BlockId::from_raw(40),
                Point2::ORIGIN,
                1.0,
                0.0
            )),
            Err(TransactionError::UnknownBlock(BlockId::from_raw(40)))
        );
        assert_eq!(
            tx.insert(BlockReferenceEntity::new(chair, Point2::ORIGIN, 0.0, 0.0)),
            Err(TransactionError::BadBlockScale)
        );
        assert_eq!(
            tx.insert(BlockReferenceEntity::new(
                BlockId::FIRST,
                Point2::ORIGIN,
                1.0,
                0.0
            )),
            Err(TransactionError::PermanentBlock)
        );
        // Rename through redefinition, and a block defined and placed in one go.
        tx.redefine_block(
            chair,
            Block {
                name: "Seat".to_owned(),
                ..chair_block()
            },
        )
        .unwrap();
        let table = tx
            .define_block(Block::new("Table", Point2::ORIGIN, vec![]))
            .unwrap();
        tx.insert(BlockReferenceEntity::new(table, Point2::ORIGIN, 2.0, 0.0))
            .unwrap();
        tx.commit();
        assert_eq!(drawing.blocks().find("seat"), Some(chair));
        assert_eq!(drawing.blocks().len(), 4);
    }

    #[test]
    fn a_block_in_use_cannot_be_removed_and_model_space_never_is() {
        let mut drawing = Drawing::new();
        let chair = define(&mut drawing, chair_block());
        let pair = define(
            &mut drawing,
            Block::new(
                "Pair",
                Point2::ORIGIN,
                vec![EntityRecord::new(
                    BlockReferenceEntity::new(chair, Point2::new(20.0, 0.0), 1.0, 0.0),
                    Properties::default(),
                )],
            ),
        );
        let placed = place(&mut drawing, pair, Point2::ORIGIN);
        let mut tx = drawing.transaction();
        assert_eq!(
            tx.remove_block(BlockId::FIRST),
            Err(TransactionError::PermanentBlock)
        );
        assert_eq!(
            tx.remove_block(chair),
            Err(TransactionError::BlockInUse(chair)),
            "held by Pair"
        );
        assert_eq!(
            tx.remove_block(pair),
            Err(TransactionError::BlockInUse(pair)),
            "placed"
        );
        assert_eq!(
            tx.remove_block(BlockId::from_raw(40)),
            Err(TransactionError::UnknownBlock(BlockId::from_raw(40)))
        );
        drop(tx);
        let mut tx = drawing.transaction();
        tx.remove(placed).unwrap();
        tx.commit();
        let mut tx = drawing.transaction();
        // Adding a reference in this transaction counts too.
        tx.insert(BlockReferenceEntity::new(pair, Point2::ORIGIN, 1.0, 0.0))
            .unwrap();
        assert_eq!(
            tx.remove_block(pair),
            Err(TransactionError::BlockInUse(pair))
        );
        drop(tx);
        let mut tx = drawing.transaction();
        assert_eq!(
            tx.remove_block(chair),
            Err(TransactionError::BlockInUse(chair)),
            "Pair places it"
        );
        tx.remove_block(pair).unwrap();
        assert_eq!(
            tx.remove_block(pair),
            Err(TransactionError::BlockAlreadyChanged(pair))
        );
        // Pair gone in this transaction, nothing places the chair: PURGE
        // sweeps a block and then what only it placed, as one change.
        tx.remove_block(chair).unwrap();
        tx.commit();
        assert_eq!(drawing.blocks().len(), 1);
        drawing.undo();
        assert_eq!(drawing.blocks().len(), 3, "one history for every table");
    }

    #[test]
    fn ltscale_is_a_positive_setting_with_its_own_undo() {
        let mut drawing = Drawing::new();
        assert_eq!(drawing.ltscale(), 1.0);
        let mut tx = drawing.transaction();
        assert_eq!(tx.set_ltscale(0.0), Err(TransactionError::BadLtscale));
        assert_eq!(tx.set_ltscale(-2.0), Err(TransactionError::BadLtscale));
        assert_eq!(tx.set_ltscale(f64::NAN), Err(TransactionError::BadLtscale));
        assert_eq!(
            tx.set_ltscale(f64::INFINITY),
            Err(TransactionError::BadLtscale)
        );
        tx.set_ltscale(2.5).unwrap();
        tx.commit();
        assert_eq!(drawing.ltscale(), 2.5);
        let shown = drawing.shown_revision();
        drawing.undo();
        assert_eq!(drawing.ltscale(), 1.0);
        assert_eq!(
            drawing.shown_revision(),
            shown,
            "a setting changes no geometry"
        );
        drawing.redo();
        assert_eq!(drawing.ltscale(), 2.5);
    }

    #[test]
    fn what_is_on_an_off_layer_is_not_shown_and_what_is_on_a_locked_one_is_locked() {
        let (off, locked) = (LayerId::from_raw(1), LayerId::from_raw(2));
        let drawing = Drawing::from_parts(
            [
                Layer {
                    on: false,
                    ..Layer::named("off")
                },
                Layer {
                    locked: true,
                    ..Layer::named("locked")
                },
            ],
            [],
            [
                (
                    Entity::Line(line(0.0, 0.0, 10.0, 0.0)),
                    Properties::default(),
                ),
                (Entity::Line(line(0.0, 1.0, 10.0, 1.0)), Properties::on(off)),
                (
                    Entity::Line(line(0.0, 2.0, 10.0, 2.0)),
                    Properties::on(locked),
                ),
            ],
        )
        .unwrap();
        let ids = live_ids(&drawing);
        assert_eq!(
            ids.iter()
                .map(|id| drawing.is_shown(*id))
                .collect::<Vec<_>>(),
            [true, false, true]
        );
        assert_eq!(
            ids.iter()
                .map(|id| drawing.is_locked(*id))
                .collect::<Vec<_>>(),
            [false, false, true]
        );
        assert_eq!(
            drawing.layer_of(ids[1]).map(|layer| layer.name.as_str()),
            Some("off")
        );
        assert_eq!(
            sorted(drawing.query_shown(&everything())),
            vec![ids[0], ids[2]]
        );
        assert_eq!(
            sorted(drawing.query(&everything())),
            ids,
            "the index itself holds everything"
        );
        assert_eq!(
            sorted(drawing.shown().map(|(id, _)| id).collect()),
            vec![ids[0], ids[2]]
        );
        let ghost = EntityId::from_raw(999);
        assert!(!drawing.is_shown(ghost) && !drawing.is_locked(ghost));

        let duplicate = Drawing::from_parts([Layer::named("a"), Layer::named("A")], [], []);
        assert_eq!(
            duplicate.err(),
            Some(TransactionError::DuplicateLayerName(LayerId::from_raw(1)))
        );
        let bad = Drawing::from_parts([Layer::named("a,b")], [], []);
        assert_eq!(
            bad.err(),
            Some(TransactionError::BadLayerName(NameError::Forbidden(',')))
        );
    }

    // ── spaces (#191) ──

    #[test]
    fn a_new_drawing_has_the_model_and_puts_entities_in_it() {
        let mut drawing = Drawing::new();
        assert_eq!(drawing.layouts().len(), 1);
        assert_eq!(
            drawing.layouts().get(LayoutId::MODEL).unwrap().name,
            "Model"
        );
        let mut tx = drawing.transaction();
        let id = tx.insert(line(0.0, 0.0, 10.0, 0.0)).unwrap();
        tx.commit();
        assert_eq!(drawing.space_of(id), Some(LayoutId::MODEL));
        assert!(drawing.is_shown_in(id, LayoutId::MODEL));
    }

    #[test]
    fn the_models_reads_see_nothing_of_a_layout_and_the_reverse() {
        let mut drawing = Drawing::new();
        let mut tx = drawing.transaction();
        let sheet = tx.add_layout(Layout::named("Sheet")).unwrap();
        let model = tx.insert(line(0.0, 0.0, 10.0, 0.0)).unwrap();
        let paper = tx.insert_in(sheet, line(0.0, 0.0, 20.0, 0.0)).unwrap();
        tx.commit();
        let ids = |iter: Vec<EntityId>| sorted(iter);
        assert_eq!(
            ids(drawing.shown().map(|(id, _)| id).collect()),
            vec![model]
        );
        assert_eq!(
            ids(drawing.shown_in(sheet).map(|(id, _)| id).collect()),
            vec![paper]
        );
        assert_eq!(ids(drawing.query_shown(&everything())), vec![model]);
        assert_eq!(
            ids(drawing.query_shown_in(sheet, &everything())),
            vec![paper]
        );
        assert_eq!(drawing.extents().unwrap().max().x, 10.0);
        assert_eq!(drawing.extents_in(sheet).unwrap().max().x, 20.0);
        assert_eq!(
            ids(drawing.query(&everything())),
            vec![model, paper],
            "the index holds every space"
        );
        assert_eq!(drawing.len(), 2);
        assert!(!drawing.is_shown_in(paper, LayoutId::MODEL));
        assert!(drawing.is_shown(paper), "shown, on its own paper");
    }

    #[test]
    fn a_removed_layout_takes_its_entities_and_undo_brings_both_back() {
        let mut drawing = Drawing::new();
        let mut tx = drawing.transaction();
        let sheet = tx.add_layout(Layout::named("Sheet")).unwrap();
        let paper = tx.insert_in(sheet, line(0.0, 0.0, 20.0, 0.0)).unwrap();
        let model = tx.insert(line(0.0, 0.0, 10.0, 0.0)).unwrap();
        tx.commit();
        let mut tx = drawing.transaction();
        tx.remove_layout(sheet).unwrap();
        tx.commit();
        assert!(!drawing.layouts().contains(sheet));
        assert!(drawing.entity(paper).is_none());
        assert!(drawing.entity(model).is_some());
        assert_eq!(drawing.query(&everything()), vec![model]);
        drawing.undo();
        assert!(drawing.layouts().contains(sheet));
        assert_eq!(drawing.space_of(paper), Some(sheet));
        assert_eq!(sorted(drawing.query(&everything())), vec![paper, model]);
        drawing.redo();
        assert!(drawing.entity(paper).is_none());
    }

    /// #200: PURGE removes every row nothing uses — not the permanent
    /// rows, not the current ones, not a row an entity, a layer, a
    /// block's entity, or another block refers to — as one change, and
    /// says which went; a scope purges one table alone.
    #[test]
    fn purge_removes_what_nothing_uses_and_keeps_the_permanent_the_current_and_the_used() {
        let mut drawing = Drawing::new();
        let mut tx = drawing.transaction();
        let dashed = tx
            .add_linetype(LinetypeDef::built_in_named("DASHED").unwrap())
            .unwrap();
        let dot = tx
            .add_linetype(LinetypeDef::built_in_named("DOT").unwrap())
            .unwrap();
        let center = tx
            .add_linetype(LinetypeDef::built_in_named("CENTER").unwrap())
            .unwrap();
        let hidden = tx
            .add_linetype(LinetypeDef::built_in_named("HIDDEN").unwrap())
            .unwrap();
        let used = tx
            .add_layer(Layer {
                linetype: Linetype::Named(dashed),
                ..Layer::named("USED")
            })
            .unwrap();
        let current = tx.add_layer(Layer::named("CURRENT")).unwrap();
        let in_block = tx.add_layer(Layer::named("INBLOCK")).unwrap();
        let free = tx.add_layer(Layer::named("FREE")).unwrap();
        tx.set_current_layer(current).unwrap();
        let titles = tx.add_text_style(TextStyle::named("TITLES")).unwrap();
        let notes = tx.add_text_style(TextStyle::named("NOTES")).unwrap();
        let tags = tx.add_text_style(TextStyle::named("TAGS")).unwrap();
        let arch = tx.add_dim_style(DimStyle::named("ARCH")).unwrap();
        let civil = tx.add_dim_style(DimStyle::named("CIVIL")).unwrap();
        let current_dim = tx.add_dim_style(DimStyle::named("CURRENTDS")).unwrap();
        tx.set_current_dim_style(current_dim).unwrap();
        let part = tx
            .define_block(Block::new(
                "PART",
                Point2::ORIGIN,
                vec![
                    EntityRecord::new(
                        Entity::Line(line(0.0, 0.0, 1.0, 0.0)),
                        Properties {
                            layer: in_block,
                            linetype: Linetype::Named(hidden),
                            ..Properties::default()
                        },
                    ),
                    EntityRecord::new(
                        Entity::AttributeDefinition(AttributeDefinitionEntity::new(
                            "TAG",
                            "Tag",
                            TextEntity {
                                style: tags,
                                ..TextEntity::new(Point2::ORIGIN, 1.0, 0.0, "x")
                            },
                        )),
                        Properties::default(),
                    ),
                ],
            ))
            .unwrap();
        let inner = tx
            .define_block(Block::new("INNER", Point2::ORIGIN, vec![]))
            .unwrap();
        let outer = tx
            .define_block(Block::new(
                "OUTER",
                Point2::ORIGIN,
                vec![EntityRecord::new(
                    Entity::BlockRef(BlockReferenceEntity::new(inner, Point2::ORIGIN, 1.0, 0.0)),
                    Properties::default(),
                )],
            ))
            .unwrap();
        tx.insert_with(
            line(0.0, 0.0, 10.0, 0.0),
            Properties {
                layer: used,
                linetype: Linetype::Named(dot),
                ..Properties::default()
            },
        )
        .unwrap();
        tx.insert(TextEntity {
            style: titles,
            ..TextEntity::new(Point2::ORIGIN, 2.0, 0.0, "Title")
        })
        .unwrap();
        tx.insert(DimensionEntity {
            style: arch,
            ..DimensionEntity::new(
                Measure::Aligned {
                    a: Point2::ORIGIN,
                    b: Point2::new(10.0, 0.0),
                },
                Point2::new(5.0, 5.0),
            )
        })
        .unwrap();
        tx.insert(BlockReferenceEntity::new(part, Point2::ORIGIN, 1.0, 0.0))
            .unwrap();
        tx.commit();
        let rows = (
            drawing.layers().len(),
            drawing.linetypes().len(),
            drawing.text_styles().len(),
            drawing.dim_styles().len(),
            drawing.blocks().len(),
        );

        // What would go, without going: an abandoned transaction.
        let mut dry = drawing.transaction();
        let would = dry.purge(PurgeScope::ALL);
        drop(dry);
        assert_eq!(drawing.layers().len(), rows.0);

        let mut tx = drawing.transaction();
        let purged = tx.purge(PurgeScope::ALL);
        tx.commit();
        assert_eq!(purged, would);
        assert_eq!(purged.layers, vec!["FREE"]);
        assert_eq!(
            purged.linetypes,
            vec!["Center"],
            "as the built-in table names it"
        );
        assert_eq!(purged.text_styles, vec!["NOTES"]);
        assert_eq!(purged.dim_styles, vec!["CIVIL"]);
        assert_eq!(
            purged.blocks,
            vec!["OUTER", "INNER"],
            "the outer first frees the inner"
        );
        assert_eq!(purged.total(), 6);
        assert!(!drawing.layers().contains(free));
        assert!(drawing.layers().contains(current) && drawing.layers().contains(in_block));
        assert!(drawing.linetypes().contains(hidden), "used inside a block");
        assert!(drawing.linetypes().contains(dashed), "used by a layer");
        assert!(
            drawing.text_styles().contains(tags),
            "an attribute definition's, in a block"
        );
        assert!(drawing.dim_styles().contains(current_dim) && drawing.dim_styles().contains(arch));
        assert!(!drawing.blocks().contains(outer) && !drawing.blocks().contains(inner));
        assert!(drawing.blocks().contains(part));
        let _ = (center, notes, civil);

        // One undo brings every row back; one redo takes them again.
        assert!(drawing.undo().is_some());
        assert_eq!(
            (
                drawing.layers().len(),
                drawing.linetypes().len(),
                drawing.text_styles().len(),
                drawing.dim_styles().len(),
                drawing.blocks().len(),
            ),
            rows
        );
        assert!(drawing.redo().is_some());
        assert!(!drawing.layers().contains(free));
        drawing.undo();

        // A scope of one table.
        let mut tx = drawing.transaction();
        let purged = tx.purge(PurgeScope {
            layers: true,
            ..PurgeScope::NONE
        });
        tx.commit();
        assert_eq!(purged.layers, vec!["FREE"]);
        assert!(purged.linetypes.is_empty() && purged.blocks.is_empty());
        assert!(drawing.linetypes().contains(center));
        // Nothing left to purge in that table.
        let mut tx = drawing.transaction();
        assert!(
            tx.purge(PurgeScope {
                layers: true,
                ..PurgeScope::NONE
            })
            .is_empty()
        );
        drop(tx);
    }

    /// #200: a row of any table is renamed through its builder, which
    /// refuses a taken or bad name and the permanent rows; references by
    /// ID are untouched.
    #[test]
    fn rows_of_every_table_are_renamed_with_the_refusals_and_references_hold() {
        let mut drawing = Drawing::new();
        let mut tx = drawing.transaction();
        let walls = tx.add_layer(Layer::named("WALLS")).unwrap();
        let other = tx.add_layer(Layer::named("OTHER")).unwrap();
        let dashed = tx
            .add_linetype(LinetypeDef::built_in_named("DASHED").unwrap())
            .unwrap();
        let dot = tx
            .add_linetype(LinetypeDef::built_in_named("DOT").unwrap())
            .unwrap();
        let titles = tx.add_text_style(TextStyle::named("TITLES")).unwrap();
        let notes = tx.add_text_style(TextStyle::named("NOTES")).unwrap();
        let arch = tx.add_dim_style(DimStyle::named("ARCH")).unwrap();
        let part = tx
            .define_block(Block::new("PART", Point2::ORIGIN, vec![]))
            .unwrap();
        let on_walls = tx
            .insert_with(
                line(0.0, 0.0, 1.0, 0.0),
                Properties {
                    layer: walls,
                    linetype: Linetype::Named(dashed),
                    ..Properties::default()
                },
            )
            .unwrap();
        tx.commit();
        let mut tx = drawing.transaction();
        tx.rename_layer(walls, "Partitions").unwrap();
        tx.rename_linetype(dashed, "Broken").unwrap();
        tx.rename_text_style(titles, "Headings").unwrap();
        tx.rename_block(part, "Widget").unwrap();
        tx.rename_dim_style(arch, "Building").unwrap();
        tx.commit();
        assert_eq!(drawing.layers().get(walls).unwrap().name, "Partitions");
        assert_eq!(drawing.linetypes().get(dashed).unwrap().name, "Broken");
        assert_eq!(drawing.text_styles().get(titles).unwrap().name, "Headings");
        assert_eq!(drawing.blocks().get(part).unwrap().name, "Widget");
        assert_eq!(drawing.dim_styles().get(arch).unwrap().name, "Building");
        let properties = drawing.properties(on_walls).unwrap();
        assert_eq!(
            (properties.layer, properties.linetype),
            (walls, Linetype::Named(dashed))
        );
        assert!(drawing.undo().is_some(), "one change");
        assert_eq!(drawing.layers().get(walls).unwrap().name, "WALLS");
        assert_eq!(drawing.blocks().get(part).unwrap().name, "PART");
        drawing.redo();

        let mut tx = drawing.transaction();
        assert_eq!(
            tx.rename_layer(LayerId::FIRST, "Zero"),
            Err(TransactionError::PermanentLayer)
        );
        assert_eq!(
            tx.rename_layer(walls, "other"),
            Err(TransactionError::DuplicateLayerName(other)),
            "give or take its case"
        );
        assert!(matches!(
            tx.rename_layer(walls, "a,b"),
            Err(TransactionError::BadLayerName(_))
        ));
        assert_eq!(
            tx.rename_linetype(LinetypeId::FIRST, "Solid"),
            Err(TransactionError::PermanentLinetype)
        );
        assert_eq!(
            tx.rename_linetype(dashed, "dot"),
            Err(TransactionError::DuplicateLinetypeName(dot))
        );
        assert!(matches!(
            tx.rename_linetype(dashed, ""),
            Err(TransactionError::BadLinetype(_))
        ));
        assert_eq!(
            tx.rename_text_style(TextStyleId::FIRST, "Plain"),
            Err(TransactionError::PermanentTextStyle)
        );
        assert_eq!(
            tx.rename_text_style(titles, "NOTES"),
            Err(TransactionError::DuplicateTextStyleName(notes))
        );
        assert_eq!(
            tx.rename_block(BlockId::FIRST, "Model"),
            Err(TransactionError::PermanentBlock)
        );
        assert_eq!(
            tx.rename_dim_style(DimStyleId::FIRST, "Plain"),
            Err(TransactionError::PermanentDimStyle)
        );
        // The same name, in another case, is allowed: it is not taken by another.
        tx.rename_layer(walls, "PARTITIONS").unwrap();
        tx.commit();
        assert_eq!(drawing.layers().get(walls).unwrap().name, "PARTITIONS");
    }

    /// #200: geometry and properties replaced as one (`MATCHPROP`).
    #[test]
    fn replace_with_changes_geometry_and_properties_in_one_change() {
        let mut drawing = Drawing::new();
        let mut tx = drawing.transaction();
        let walls = tx.add_layer(Layer::named("WALLS")).unwrap();
        let id = tx.insert(line(0.0, 0.0, 1.0, 0.0)).unwrap();
        tx.commit();
        let mut tx = drawing.transaction();
        tx.replace_with(
            id,
            line(0.0, 0.0, 5.0, 0.0),
            Properties {
                layer: walls,
                color: Color::Index(1),
                ..Properties::default()
            },
        )
        .unwrap();
        assert_eq!(
            tx.set_properties(id, Properties::default()),
            Err(TransactionError::AlreadyChanged(id))
        );
        tx.commit();
        assert_eq!(drawing.properties(id).unwrap().layer, walls);
        assert_eq!(drawing.entity(id).unwrap().as_line().unwrap().end.x, 5.0);
        drawing.undo();
        assert_eq!(drawing.properties(id).unwrap().layer, LayerId::FIRST);
        assert_eq!(drawing.entity(id).unwrap().as_line().unwrap().end.x, 1.0);
    }

    /// #206 (ADR-0031): construction lines are kept out of the index's
    /// tree and the extents, found by any query whose box they cross, and
    /// move into the tree when an edit makes one a line.
    #[test]
    fn construction_lines_are_queried_beside_the_tree_and_left_out_of_the_extents() {
        let mut drawing = Drawing::new();
        let mut tx = drawing.transaction();
        let line_id = tx.insert(line(0.0, 0.0, 10.0, 10.0)).unwrap();
        let xline = tx
            .insert(
                crate::XLineEntity::through(Point2::new(0.0, 50.0), Point2::new(1.0, 50.0))
                    .unwrap(),
            )
            .unwrap();
        let ray = tx
            .insert(
                crate::RayEntity::through(Point2::new(100.0, 0.0), Point2::new(100.0, 1.0))
                    .unwrap(),
            )
            .unwrap();
        tx.commit();
        let extents = drawing.extents().unwrap();
        assert_eq!(
            extents.max(),
            Point2::new(10.0, 10.0),
            "only the line counts"
        );
        let far = Aabb::from_points(Point2::new(5000.0, 45.0), Point2::new(5010.0, 55.0));
        assert_eq!(
            drawing.query(&far),
            vec![xline],
            "the xline crosses it, far from its base"
        );
        let above = Aabb::from_points(Point2::new(95.0, 500.0), Point2::new(105.0, 510.0));
        assert_eq!(drawing.query(&above), vec![ray]);
        let below = Aabb::from_points(Point2::new(95.0, -510.0), Point2::new(105.0, -500.0));
        assert!(drawing.query(&below).is_empty(), "behind the ray's start");
        let mut near = drawing.query(&Aabb::from_points(
            Point2::new(0.0, 0.0),
            Point2::new(10.0, 60.0),
        ));
        near.sort();
        assert_eq!(near, vec![line_id, xline]);
        assert_eq!(drawing.unbounded_shown_in(LayoutId::MODEL).count(), 2);
        // A ray replaced by a line moves into the tree, and back on undo.
        let mut tx = drawing.transaction();
        tx.replace(ray, line(100.0, 0.0, 100.0, 20.0)).unwrap();
        tx.commit();
        assert_eq!(drawing.unbounded_shown_in(LayoutId::MODEL).count(), 1);
        assert_eq!(drawing.extents().unwrap().max(), Point2::new(100.0, 20.0));
        assert!(drawing.query(&above).is_empty());
        drawing.undo();
        assert_eq!(drawing.query(&above), vec![ray]);
        assert_eq!(drawing.extents().unwrap().max(), Point2::new(10.0, 10.0));
        // Removed, gone from the side list too.
        let mut tx = drawing.transaction();
        tx.remove(xline).unwrap();
        tx.commit();
        assert!(drawing.query(&far).is_empty());
    }

    #[test]
    fn a_layouts_sheet_is_set_as_one_change_and_the_model_has_none() {
        let mut drawing = Drawing::new();
        let mut tx = drawing.transaction();
        let sheet = tx.add_layout(Layout::named("Sheet")).unwrap();
        tx.commit();
        assert_eq!(
            drawing.layouts().get(sheet).unwrap().sheet_size(),
            Vec2::new(420.0, 297.0),
            "A3 landscape until set"
        );
        let mut tx = drawing.transaction();
        assert_eq!(
            tx.set_layout_sheet(LayoutId::MODEL, Vec2::new(210.0, 297.0)),
            Err(TransactionError::PermanentLayout)
        );
        assert_eq!(
            tx.set_layout_sheet(sheet, Vec2::new(0.0, 297.0)),
            Err(TransactionError::BadSheet)
        );
        assert_eq!(
            tx.set_layout_sheet(sheet, Vec2::new(210.0, f64::NAN)),
            Err(TransactionError::BadSheet)
        );
        tx.set_layout_sheet(sheet, Vec2::new(297.0, 210.0)).unwrap();
        tx.commit();
        let row = drawing.layouts().get(sheet).unwrap();
        assert_eq!(row.sheet_size(), Vec2::new(297.0, 210.0));
        assert_eq!(row.sheet().max(), Point2::new(297.0, 210.0));
        assert!(drawing.undo().is_some());
        assert_eq!(
            drawing.layouts().get(sheet).unwrap().sheet_size(),
            Vec2::new(420.0, 297.0)
        );
        assert!(drawing.redo().is_some());
        assert_eq!(
            drawing.layouts().get(sheet).unwrap().sheet_size(),
            Vec2::new(297.0, 210.0)
        );
    }

    #[test]
    fn new_objects_take_the_top_tier_and_pieces_their_source_s() {
        let mut drawing = Drawing::new();
        assert_eq!((drawing.bottom_order(), drawing.top_order()), (0, 0));
        let mut tx = drawing.transaction();
        let back = tx.insert(line(0.0, 0.0, 10.0, 0.0)).unwrap();
        let front = tx.insert(line(0.0, 1.0, 10.0, 1.0)).unwrap();
        tx.commit();
        let mut tx = drawing.transaction();
        tx.set_order(back, -2).unwrap();
        tx.set_order(front, 3).unwrap();
        tx.commit();
        assert_eq!((drawing.bottom_order(), drawing.top_order()), (-2, 3));

        let mut tx = drawing.transaction();
        let new = tx.insert(line(0.0, 2.0, 10.0, 2.0)).unwrap();
        let piece = tx.insert_like(back, line(0.0, 3.0, 10.0, 3.0)).unwrap();
        let mask = tx.insert_on_top(line(0.0, 4.0, 10.0, 4.0)).unwrap();
        tx.commit();
        let order = |drawing: &Drawing, id| drawing.record(id).map(|record| record.order);
        assert_eq!(order(&drawing, new), Some(3), "a new object goes on top");
        assert_eq!(
            order(&drawing, piece),
            Some(-2),
            "a piece stays where its source is"
        );
        assert_eq!(order(&drawing, mask), Some(4), "above everything");
        assert_eq!(drawing.top_order(), 4);

        assert!(drawing.undo().is_some());
        assert_eq!(drawing.top_order(), 3);
        assert!(drawing.undo().is_some());
        assert_eq!(order(&drawing, back), Some(0), "one reorder, undone as one");
        assert_eq!(order(&drawing, front), Some(0));
        assert_eq!((drawing.bottom_order(), drawing.top_order()), (0, 0));
    }

    #[test]
    fn a_change_of_tier_is_a_change_to_what_is_shown() {
        let mut drawing = Drawing::new();
        let mut tx = drawing.transaction();
        let id = tx.insert(line(0.0, 0.0, 10.0, 0.0)).unwrap();
        tx.commit();
        let before = drawing.shown_revision();
        let mut tx = drawing.transaction();
        tx.set_order(id, 1).unwrap();
        tx.commit();
        assert_ne!(drawing.shown_revision(), before);
        assert_eq!(
            drawing.shown_changes_since(before),
            Some(vec![id]),
            "only the entity reordered is repacked"
        );
    }

    #[test]
    fn named_views_are_checked_and_go_with_their_layout() {
        let mut drawing = Drawing::new();
        let view = |name: &str, space, width| NamedView {
            name: name.to_owned(),
            space,
            center: Point2::new(10.0, 10.0),
            width,
            height: 20.0,
        };
        let mut tx = drawing.transaction();
        let sheet = tx.add_layout(Layout::named("Sheet")).unwrap();
        let detail = tx.add_view(view("Detail", LayoutId::MODEL, 40.0)).unwrap();
        let title = tx.add_view(view("Title", sheet, 40.0)).unwrap();
        assert_eq!(
            tx.add_view(view("DETAIL", LayoutId::MODEL, 40.0)),
            Err(TransactionError::DuplicateViewName(detail))
        );
        assert_eq!(
            tx.add_view(view("Flat", LayoutId::MODEL, 0.0)),
            Err(TransactionError::BadView)
        );
        assert!(matches!(
            tx.add_view(view("a<b", LayoutId::MODEL, 1.0)),
            Err(TransactionError::BadViewName(_))
        ));
        let nowhere = LayoutId::from_raw(99);
        assert_eq!(
            tx.add_view(view("Nowhere", nowhere, 1.0)),
            Err(TransactionError::UnknownLayout(nowhere))
        );
        tx.commit();
        assert_eq!(
            drawing.views().get(title).unwrap().frame().min(),
            Point2::new(-10.0, 0.0)
        );

        let mut tx = drawing.transaction();
        tx.remove_layout(sheet).unwrap();
        tx.commit();
        assert!(!drawing.views().contains(title), "gone with its layout");
        assert!(drawing.views().contains(detail));
        assert!(drawing.undo().is_some());
        assert!(drawing.views().contains(title), "back with it");
        let gone = NamedViewId::from_raw(99);
        let mut tx = drawing.transaction();
        assert_eq!(
            tx.remove_view(gone),
            Err(TransactionError::UnknownView(gone))
        );
    }

    #[test]
    fn a_layer_state_restores_the_layers_it_names_as_one_undo() {
        let mut drawing = Drawing::new();
        let mut tx = drawing.transaction();
        let dashed = tx
            .add_linetype(LinetypeDef::built_in_named("DASHED").unwrap())
            .unwrap();
        let walls = tx
            .add_layer(Layer {
                linetype: Linetype::Named(dashed),
                ..Layer::named("walls")
            })
            .unwrap();
        let notes = tx.add_layer(Layer::named("notes")).unwrap();
        tx.commit();
        let state = drawing.layer_state_now("Plan");
        let mut tx = drawing.transaction();
        let plan = tx.add_layer_state(state).unwrap();
        tx.commit();

        // The layers changed, and one made since.
        let walls_now = drawing.layers().get(walls).unwrap().clone();
        let mut tx = drawing.transaction();
        tx.replace_layer(
            walls,
            Layer {
                on: false,
                color: Color::Index(1),
                linetype: Linetype::CONTINUOUS,
                ..walls_now
            },
        )
        .unwrap();
        tx.replace_layer(
            notes,
            Layer {
                locked: true,
                ..Layer::named("notes")
            },
        )
        .unwrap();
        let later = tx.add_layer(Layer::named("later")).unwrap();
        tx.set_current_layer(later).unwrap();
        tx.commit();
        let changed = drawing.layers().clone();

        let mut tx = drawing.transaction();
        assert_eq!(tx.restore_layer_state(plan), Ok(2));
        tx.commit();
        let layer = |id| drawing.layers().get(id).unwrap().clone();
        assert!(layer(walls).on);
        assert_eq!(layer(walls).color, Layer::DEFAULT_COLOR);
        assert_eq!(layer(walls).linetype, Linetype::Named(dashed));
        assert!(!layer(notes).locked);
        assert_eq!(
            layer(later),
            Layer::named("later"),
            "not in the state: as it was"
        );
        assert!(drawing.undo().is_some());
        assert_eq!(drawing.layers(), &changed, "one undo");
    }

    #[test]
    fn a_restore_keeps_the_current_layer_shown_and_a_linetype_gone_leaves_its_own() {
        let mut drawing = Drawing::new();
        let mut tx = drawing.transaction();
        let dashed = tx
            .add_linetype(LinetypeDef::built_in_named("DASHED").unwrap())
            .unwrap();
        let walls = tx
            .add_layer(Layer {
                on: false,
                linetype: Linetype::Named(dashed),
                ..Layer::named("walls")
            })
            .unwrap();
        tx.commit();
        let state = drawing.layer_state_now("Off");
        let mut tx = drawing.transaction();
        let off = tx.add_layer_state(state).unwrap();
        tx.commit();
        let mut tx = drawing.transaction();
        tx.replace_layer(
            walls,
            Layer {
                on: true,
                linetype: Linetype::CONTINUOUS,
                ..Layer::named("walls")
            },
        )
        .unwrap();
        tx.set_current_layer(walls).unwrap();
        tx.commit();
        let mut tx = drawing.transaction();
        tx.remove_linetype(dashed).unwrap();
        tx.commit();

        let mut tx = drawing.transaction();
        assert_eq!(tx.restore_layer_state(off), Ok(0), "nothing it may change");
        tx.commit();
        let walls = drawing.layers().get(walls).unwrap();
        assert!(walls.on, "the current layer stays on");
        assert_eq!(walls.linetype, Linetype::CONTINUOUS);
    }

    #[test]
    fn layer_state_names_are_checked_and_unknown_states_refused() {
        let mut drawing = Drawing::new();
        let state = |name: &str| LayerState {
            name: name.to_owned(),
            ..Drawing::new().layer_state_now("")
        };
        let mut tx = drawing.transaction();
        let plan = tx.add_layer_state(state("Plan")).unwrap();
        assert_eq!(
            tx.add_layer_state(state("PLAN")),
            Err(TransactionError::DuplicateLayerStateName(plan))
        );
        assert!(matches!(
            tx.add_layer_state(state("a<b")),
            Err(TransactionError::BadLayerStateName(_))
        ));
        tx.commit();
        let gone = LayerStateId::from_raw(99);
        let mut tx = drawing.transaction();
        assert_eq!(
            tx.restore_layer_state(gone),
            Err(TransactionError::UnknownLayerState(gone))
        );
        assert_eq!(
            tx.remove_layer_state(gone),
            Err(TransactionError::UnknownLayerState(gone))
        );
        tx.replace_layer_state(plan, state("Ceiling")).unwrap();
        tx.commit();
        assert_eq!(drawing.layer_states().find("ceiling"), Some(plan));
        let mut tx = drawing.transaction();
        tx.remove_layer_state(plan).unwrap();
        tx.commit();
        assert!(drawing.layer_states().is_empty());
        assert!(drawing.undo().is_some());
        assert_eq!(drawing.layer_states().find("Ceiling"), Some(plan));
    }

    #[test]
    fn moving_an_entity_between_spaces_is_one_change() {
        let mut drawing = Drawing::new();
        let mut tx = drawing.transaction();
        let sheet = tx.add_layout(Layout::named("Sheet")).unwrap();
        let id = tx.insert(line(0.0, 0.0, 10.0, 0.0)).unwrap();
        tx.commit();
        let mut tx = drawing.transaction();
        tx.set_space(id, sheet).unwrap();
        tx.commit();
        assert_eq!(drawing.space_of(id), Some(sheet));
        assert!(drawing.query_shown(&everything()).is_empty());
        assert_eq!(drawing.properties(id), Some(Properties::default()));
        assert!(drawing.undo().is_some());
        assert_eq!(drawing.space_of(id), Some(LayoutId::MODEL));
        assert_eq!(drawing.query_shown(&everything()), vec![id]);
        assert!(drawing.redo().is_some());
        assert_eq!(
            drawing.space_of(id),
            Some(sheet),
            "one change, redone as one"
        );
    }

    #[test]
    fn the_model_cannot_be_renamed_or_removed_and_layout_names_are_unique() {
        let mut drawing = Drawing::new();
        let mut tx = drawing.transaction();
        assert_eq!(
            tx.rename_layout(LayoutId::MODEL, "Paper"),
            Err(TransactionError::PermanentLayout)
        );
        assert_eq!(
            tx.remove_layout(LayoutId::MODEL),
            Err(TransactionError::PermanentLayout)
        );
        let a = tx.add_layout(Layout::named("Sheet")).unwrap();
        assert_eq!(
            tx.add_layout(Layout::named("sheet")),
            Err(TransactionError::DuplicateLayoutName(a))
        );
        assert_eq!(
            tx.add_layout(Layout::named("Model")),
            Err(TransactionError::DuplicateLayoutName(LayoutId::MODEL))
        );
        assert!(matches!(
            tx.add_layout(Layout::named("a,b")),
            Err(TransactionError::BadLayoutName(_))
        ));
        let b = tx.add_layout(Layout::named("Other")).unwrap();
        assert_eq!(
            tx.rename_layout(b, "SHEET"),
            Err(TransactionError::DuplicateLayoutName(a))
        );
        tx.rename_layout(b, "Sheet 2").unwrap();
        tx.commit();
        assert_eq!(drawing.layouts().get(b).unwrap().name, "Sheet 2");
        let mut tx = drawing.transaction();
        let on_it = tx.insert_in(b, line(0.0, 0.0, 1.0, 1.0)).unwrap();
        assert_eq!(tx.remove_layout(b), Err(TransactionError::LayoutInUse(b)));
        tx.commit();
        let mut tx = drawing.transaction();
        tx.replace(on_it, line(0.0, 0.0, 2.0, 2.0)).unwrap();
        assert_eq!(tx.remove_layout(b), Err(TransactionError::LayoutInUse(b)));
        tx.commit();
        let mut tx = drawing.transaction();
        assert_eq!(
            tx.insert_in(LayoutId::from_raw(9), line(0.0, 0.0, 1.0, 1.0))
                .err(),
            Some(TransactionError::UnknownLayout(LayoutId::from_raw(9)))
        );
        tx.commit();
    }

    #[test]
    fn a_drawing_from_spaces_holds_each_entity_in_its_space_and_refuses_an_unknown_one() {
        let sheet = LayoutId::from_raw(1);
        let drawing = Drawing::from_spaces(
            [],
            [],
            [],
            [],
            [],
            [Layout::named("Sheet")],
            [
                (
                    line(0.0, 0.0, 10.0, 0.0).into(),
                    Properties::default(),
                    LayoutId::MODEL,
                ),
                (
                    line(0.0, 0.0, 20.0, 0.0).into(),
                    Properties::default(),
                    sheet,
                ),
            ],
        )
        .unwrap();
        assert_eq!(drawing.layouts().get(sheet).unwrap().name, "Sheet");
        assert_eq!(drawing.shown().count(), 1);
        assert_eq!(drawing.shown_in(sheet).count(), 1);
        assert!(!drawing.can_undo());
        let refused = Drawing::from_spaces(
            [],
            [],
            [],
            [],
            [],
            [],
            [(
                line(0.0, 0.0, 1.0, 0.0).into(),
                Properties::default(),
                sheet,
            )],
        );
        assert_eq!(refused.err(), Some(TransactionError::UnknownLayout(sheet)));
        let duplicate = Drawing::from_spaces([], [], [], [], [], [Layout::named("Model")], []);
        assert_eq!(
            duplicate.err(),
            Some(TransactionError::DuplicateLayoutName(LayoutId::MODEL))
        );
    }

    #[derive(Clone, Debug)]
    enum Op {
        Create(f64, f64, f64, f64),
        CreateOn(f64, usize),
        Erase(usize),
        Modify(usize, f64, f64),
        Restyle(usize, usize, u8),
        AddLayer(u8),
        RenameLayer(usize, u8),
        SwitchLayer(usize, bool, bool, bool),
        RemoveLayer(usize),
        SetCurrent(usize),
        SetUnit(usize),
        /// A built-in linetype, by index.
        LoadLinetype(usize),
        RemoveLinetype(usize),
        /// An entity given a linetype; a layer given one; the setting.
        Dash(usize, usize),
        DashLayer(usize, usize),
        SetCurrentLinetype(usize),
        SetLtscale(f64),
        SetPointStyle(u8, f64),
        /// A text style by name; a text with a style by index (one past
        /// the table is a style that does not exist); a style removed.
        AddTextStyle(u8),
        RestyleText(usize, f64, f64, f64),
        SetTextStyle(usize),
        CreateText(f64, usize),
        RemoveTextStyle(usize),
        /// A dimension style by name; one restyled (a text height, which
        /// may be refused) or renamed; one removed; one made current.
        AddDimStyle(u8),
        RestyleDimStyle(usize, f64, u8),
        RemoveDimStyle(usize),
        SetDimStyle(usize),
        /// A block of one line by name; a reference to a block by index
        /// (one past the table is a block that does not exist); a block
        /// redefined with a line of another length, or to hold another
        /// block; a block removed.
        DefineBlock(u8, f64),
        PlaceBlock(usize, f64, f64),
        RedefineBlock(usize, f64),
        NestBlock(usize, usize),
        RemoveBlock(usize),
        /// Every unused row swept away (#200).
        Purge,
        /// A layer, an entity on it, and an edit of another, as one step.
        Together(usize, f64),
        Group(usize, usize, u8),
        GroupAdd(usize, usize),
        GroupRemove(usize, usize),
        GroupRename(usize, u8),
        /// A layout by name; one renamed; one removed with what is on it;
        /// a line drawn in a space by index; an entity moved to a space.
        AddLayout(u8),
        RenameLayout(usize, u8),
        SetLayoutSheet(usize, f64, f64),
        SetLayoutPlot(usize, f64, u8),
        RemoveLayout(usize),
        CreateIn(usize, f64),
        MoveTo(usize, usize),
        /// An entity put in a tier of the drawing order; a line drawn over
        /// everything (#226).
        Reorder(usize, i16),
        CreateOnTop(f64),
        WipeoutFrame(bool),
        /// A layer state saved by name, one restored, one removed (#228).
        SaveState(u8),
        RestoreState(usize),
        DeleteState(usize),
        /// A view saved by name in a space by index, one deleted (#235).
        SaveView(u8, usize, f64),
        /// A layer merged into another, one deleted with what is on it
        /// (#236).
        MergeLayer(usize, usize),
        /// A multileader style made, restyled (an arrow size, which may be
        /// refused), removed, or set current; a multileader drawn (#240).
        AddMLeaderStyle(u8),
        RestyleMLeader(usize, f64),
        RemoveMLeaderStyle(usize),
        SetMLeaderStyle(usize),
        CreateMLeader(f64, usize),
        DeleteLayer(usize),
        DeleteView(usize),
        Undo,
        Redo,
    }

    fn coordinate() -> impl Strategy<Value = f64> {
        -500.0..500.0_f64
    }

    /// The drawing's list of curves by size (#210), as it keeps it.
    fn curves_by_size(drawing: &Drawing) -> Vec<(EntityId, f64)> {
        drawing
            .radii
            .iter()
            .rev()
            .map(|(radius, id)| (*id, radius.0))
            .collect()
    }

    /// How many entities are in each tier, as the drawing keeps it and as
    /// its records say.
    type TierCounts = Vec<(i16, usize)>;

    fn tiers(drawing: &Drawing) -> (TierCounts, TierCounts) {
        let mut afresh: BTreeMap<i16, usize> = BTreeMap::new();
        for (_, record) in drawing.records() {
            *afresh.entry(record.order).or_default() += 1;
        }
        (
            drawing
                .orders
                .iter()
                .map(|(order, n)| (*order, *n))
                .collect(),
            afresh.into_iter().collect(),
        )
    }

    /// The same list read from every entity's primitives, largest first.
    fn radii_read_afresh(drawing: &Drawing) -> Vec<(EntityId, f64)> {
        let mut radii: Vec<(EntityId, f64)> = drawing
            .entities()
            .filter_map(|(id, entity)| {
                entity
                    .primitives(drawing.tables())
                    .iter()
                    .filter_map(|primitive| match primitive {
                        cad_geometry::Primitive::Arc(arc) => Some(arc.radius),
                        cad_geometry::Primitive::Circle(circle) => Some(circle.radius),
                        cad_geometry::Primitive::Segment(_) => None,
                    })
                    .reduce(f64::max)
                    .map(|radius| (id, radius))
            })
            .collect();
        radii.sort_by(|a, b| b.1.total_cmp(&a.1).then(b.0.cmp(&a.0)));
        radii
    }

    fn op() -> impl Strategy<Value = Op> {
        prop_oneof![
            3 => (coordinate(), coordinate(), coordinate(), coordinate())
                .prop_map(|(a, b, c, d)| Op::Create(a, b, c, d)),
            2 => (coordinate(), any::<usize>()).prop_map(|(x, layer)| Op::CreateOn(x, layer)),
            1 => any::<usize>().prop_map(Op::Erase),
            2 => (any::<usize>(), coordinate(), coordinate()).prop_map(|(i, x, y)| Op::Modify(i, x, y)),
            2 => (any::<usize>(), any::<usize>(), 1..8u8).prop_map(|(i, layer, color)| Op::Restyle(i, layer, color)),
            2 => (0..12u8).prop_map(Op::AddLayer),
            1 => (any::<usize>(), 0..12u8).prop_map(|(i, name)| Op::RenameLayer(i, name)),
            1 => (any::<usize>(), any::<bool>(), prop::bool::weighted(0.25), any::<bool>())
                .prop_map(|(i, on, frozen, locked)| Op::SwitchLayer(i, on, frozen, locked)),
            1 => any::<usize>().prop_map(Op::RemoveLayer),
            1 => any::<usize>().prop_map(Op::SetCurrent),
            1 => any::<usize>().prop_map(Op::SetUnit),
            2 => any::<usize>().prop_map(Op::LoadLinetype),
            1 => any::<usize>().prop_map(Op::RemoveLinetype),
            1 => (any::<usize>(), any::<usize>()).prop_map(|(i, lt)| Op::Dash(i, lt)),
            1 => (any::<usize>(), any::<usize>()).prop_map(|(i, lt)| Op::DashLayer(i, lt)),
            1 => any::<usize>().prop_map(Op::SetCurrentLinetype),
            1 => (-1.0..5.0_f64).prop_map(Op::SetLtscale),
            1 => (0..100u8, -1.0..5.0_f64).prop_map(|(mode, size)| Op::SetPointStyle(mode, size)),
            1 => (0..6u8).prop_map(Op::AddTextStyle),
            1 => (any::<usize>(), -1.0..5.0f64, -0.5..3.0f64, -1.7..1.7f64)
                .prop_map(|(i, height, width, oblique)| Op::RestyleText(i, height, width, oblique)),
            1 => any::<usize>().prop_map(Op::SetTextStyle),
            2 => (0..5u8, 1.0..50.0f64).prop_map(|(n, len)| Op::DefineBlock(n, len)),
            3 => (any::<usize>(), coordinate(), 0.1..4.0f64).prop_map(|(b, x, s)| Op::PlaceBlock(b, x, s)),
            2 => (any::<usize>(), 1.0..50.0f64).prop_map(|(b, len)| Op::RedefineBlock(b, len)),
            1 => (any::<usize>(), any::<usize>()).prop_map(|(b, inner)| Op::NestBlock(b, inner)),
            1 => any::<usize>().prop_map(Op::RemoveBlock),
            1 => Just(Op::Purge),
            2 => (coordinate(), any::<usize>()).prop_map(|(x, style)| Op::CreateText(x, style)),
            1 => any::<usize>().prop_map(Op::RemoveTextStyle),
            1 => (0..4u8).prop_map(Op::AddLayout),
            1 => (any::<usize>(), 0..4u8).prop_map(|(i, name)| Op::RenameLayout(i, name)),
            1 => any::<usize>().prop_map(Op::RemoveLayout),
            1 => (any::<usize>(), -10.0..500.0_f64, -10.0..500.0_f64)
                .prop_map(|(i, w, h)| Op::SetLayoutSheet(i, w, h)),
            1 => (any::<usize>(), -1.0..5.0_f64, 0..5u8).prop_map(|(i, scale, turns)| Op::SetLayoutPlot(i, scale, turns)),
            2 => (any::<usize>(), coordinate()).prop_map(|(i, x)| Op::CreateIn(i, x)),
            1 => (any::<usize>(), any::<usize>()).prop_map(|(i, space)| Op::MoveTo(i, space)),
            1 => (any::<usize>(), -3..4i16).prop_map(|(i, order)| Op::Reorder(i, order)),
            1 => coordinate().prop_map(Op::CreateOnTop),
            1 => any::<bool>().prop_map(Op::WipeoutFrame),
            1 => (0..4u8).prop_map(Op::SaveState),
            2 => any::<usize>().prop_map(Op::RestoreState),
            1 => any::<usize>().prop_map(Op::DeleteState),
            1 => (0..4u8, any::<usize>(), -1.0..50.0f64).prop_map(|(n, space, size)| Op::SaveView(n, space, size)),
            1 => any::<usize>().prop_map(Op::DeleteView),
            1 => (any::<usize>(), any::<usize>()).prop_map(|(from, into)| Op::MergeLayer(from, into)),
            1 => any::<usize>().prop_map(Op::DeleteLayer),
            1 => (0..4u8).prop_map(Op::AddMLeaderStyle),
            1 => (any::<usize>(), -1.0..8.0f64).prop_map(|(i, size)| Op::RestyleMLeader(i, size)),
            1 => any::<usize>().prop_map(Op::RemoveMLeaderStyle),
            1 => any::<usize>().prop_map(Op::SetMLeaderStyle),
            2 => (coordinate(), any::<usize>()).prop_map(|(x, style)| Op::CreateMLeader(x, style)),
            1 => (0..5u8).prop_map(Op::AddDimStyle),
            1 => (any::<usize>(), -1.0..5.0f64, 0..5u8).prop_map(|(i, h, n)| Op::RestyleDimStyle(i, h, n)),
            1 => any::<usize>().prop_map(Op::RemoveDimStyle),
            1 => any::<usize>().prop_map(Op::SetDimStyle),
            1 => (any::<usize>(), coordinate()).prop_map(|(i, x)| Op::Together(i, x)),
            2 => (any::<usize>(), any::<usize>(), 0..6u8).prop_map(|(a, b, name)| Op::Group(a, b, name)),
            1 => (any::<usize>(), any::<usize>()).prop_map(|(g, e)| Op::GroupAdd(g, e)),
            1 => (any::<usize>(), any::<usize>()).prop_map(|(g, e)| Op::GroupRemove(g, e)),
            1 => (any::<usize>(), 0..6u8).prop_map(|(g, name)| Op::GroupRename(g, name)),
            3 => Just(Op::Undo),
            2 => Just(Op::Redo),
        ]
    }

    /// Everything that is saved: every entity with its geometry and
    /// properties, every layer, every linetype, and the settings.
    type Saved = (
        Vec<(EntityId, Entity, Properties, LayoutId, i16)>,
        Vec<(LayerId, Layer)>,
        Vec<(LinetypeId, LinetypeDef)>,
        Vec<(TextStyleId, TextStyle)>,
        Vec<(DimStyleId, DimStyle)>,
        Vec<(BlockId, Block)>,
        Vec<(LayoutId, Layout)>,
        Vec<(GroupId, Group)>,
        Vec<(LayerStateId, LayerState)>,
        Vec<(NamedViewId, NamedView)>,
        Vec<(MLeaderStyleId, MLeaderStyle)>,
        DrawingSettings,
    );

    fn saved(drawing: &Drawing) -> Saved {
        let mut entities: Vec<(EntityId, Entity, Properties, LayoutId, i16)> = drawing
            .records()
            .map(|(id, record)| {
                (
                    id,
                    (*record.entity).clone(),
                    record.properties,
                    record.space,
                    record.order,
                )
            })
            .collect();
        entities.sort_by_key(|(id, ..)| *id);
        let layers = drawing
            .layers()
            .iter()
            .map(|(id, layer)| (id, layer.clone()))
            .collect();
        let linetypes = drawing
            .linetypes()
            .iter()
            .map(|(id, linetype)| (id, linetype.clone()))
            .collect();
        let text_styles = drawing
            .text_styles()
            .iter()
            .map(|(id, style)| (id, style.clone()))
            .collect();
        let dim_styles = drawing
            .dim_styles()
            .iter()
            .map(|(id, style)| (id, style.clone()))
            .collect();
        let blocks = drawing
            .blocks()
            .iter()
            .map(|(id, block)| (id, block.clone()))
            .collect();
        let layouts = drawing
            .layouts()
            .iter()
            .map(|(id, layout)| (id, layout.clone()))
            .collect();
        let groups = drawing
            .groups()
            .iter()
            .map(|(id, group)| (id, group.clone()))
            .collect();
        (
            entities,
            layers,
            linetypes,
            text_styles,
            dim_styles,
            blocks,
            layouts,
            groups,
            drawing
                .layer_states()
                .iter()
                .map(|(id, state)| (id, state.clone()))
                .collect(),
            drawing
                .views()
                .iter()
                .map(|(id, view)| (id, view.clone()))
                .collect(),
            drawing
                .mleader_styles()
                .iter()
                .map(|(id, style)| (id, style.clone()))
                .collect(),
            drawing.settings(),
        )
    }

    proptest! {
        /// After any sequence of edits to entities, layers, and settings,
        /// undos, and redos — refused changes included:
        ///  - the spatial index equals one rebuilt from scratch (§19.5);
        ///  - no entity or layer ID is ever handed out twice (§10.2);
        ///  - the allocator never decreases;
        ///  - every entity is on a layer that exists; layer names are
        ///    unique; the current layer exists and is on;
        ///  - every linetype an entity, a layer, or the settings name
        ///    exists; linetype names are unique;
        ///  - undoing everything gives back the empty drawing, and redoing
        ///    everything gives back exactly the drawing there was (§19.2):
        ///    one history for entities, tables, and settings.
        #[test]
        fn derived_state_and_identity_survive_any_edit_sequence(
            ops in prop::collection::vec(op(), 0..80),
            queries in prop::collection::vec(
                (coordinate(), coordinate(), 0.0..300.0_f64, 0.0..300.0_f64), 1..6),
        ) {
            let mut drawing = Drawing::new();
            let empty = saved(&drawing);
            let mut allocated: Vec<EntityId> = Vec::new();
            let mut layers_made: Vec<LayerId> = Vec::new();
            let mut linetypes_made: Vec<LinetypeId> = Vec::new();
            let mut high_water = drawing.next_entity_id();
            let mut together = 0;

            for op in ops {
                let live = live_ids(&drawing);
                let layers: Vec<LayerId> = drawing.layers().iter().map(|(id, _)| id).collect();
                let layer = |pick: usize| layers[pick % layers.len()];
                let linetypes: Vec<LinetypeId> = drawing.linetypes().iter().map(|(id, _)| id).collect();
                // One past the table: a linetype that does not exist.
                let linetype = |pick: usize| {
                    let index = pick % (linetypes.len() + 1);
                    linetypes.get(index).copied().unwrap_or(LinetypeId::from_raw(u32::MAX))
                };
                let in_use = |drawing: &Drawing, id: LinetypeId| {
                    let named = Linetype::Named(id);
                    drawing.records().any(|(_, record)| record.properties.linetype == named)
                        || drawing.layers().iter().any(|(_, layer)| layer.linetype == named)
                        || drawing.current_linetype() == named
                };
                match op {
                    Op::Create(x0, y0, x1, y1) => {
                        let id = create(&mut drawing, line(x0, y0, x1, y1));
                        prop_assert!(!allocated.contains(&id), "ID {id} was reused");
                        prop_assert_eq!(drawing.properties(id).unwrap().layer, drawing.current_layer());
                        allocated.push(id);
                    }
                    Op::CreateOn(x, on) => {
                        let mut tx = drawing.transaction();
                        // A circle, so the list of curves by size (#210) has something in it.
                        let circle = crate::CircleEntity::new(Point2::new(x, 0.0), 9.0);
                        allocated.push(tx.insert_with(circle, Properties::on(layer(on))).unwrap());
                        tx.commit();
                    }
                    Op::Erase(pick) if !live.is_empty() => {
                        let mut tx = drawing.transaction();
                        tx.remove(live[pick % live.len()]).unwrap();
                        tx.commit();
                    }
                    Op::Modify(pick, x, y) if !live.is_empty() => {
                        let id = live[pick % live.len()];
                        let before = drawing.properties(id);
                        let mut tx = drawing.transaction();
                        tx.replace(id, line(x, y, x + 10.0, y - 10.0)).unwrap();
                        tx.commit();
                        prop_assert_eq!(drawing.properties(id), before, "an edit keeps properties");
                    }
                    Op::Restyle(pick, on, color) if !live.is_empty() => {
                        let mut tx = drawing.transaction();
                        let properties = Properties { color: crate::Color::Index(color), ..Properties::on(layer(on)) };
                        tx.set_properties(live[pick % live.len()], properties).unwrap();
                        tx.commit();
                    }
                    Op::AddLayer(name) => {
                        // Few names: collisions are common and must be refused.
                        let name = format!("Layer {name}");
                        let taken = drawing.layers().find(&name);
                        let mut tx = drawing.transaction();
                        let added = tx.add_layer(Layer::named(name.to_uppercase()));
                        tx.commit();
                        match (added, taken) {
                            (Ok(id), None) => {
                                prop_assert!(!layers_made.contains(&id), "layer ID {} was reused", id);
                                layers_made.push(id);
                            }
                            (Err(TransactionError::DuplicateLayerName(id)), Some(existing)) => {
                                prop_assert_eq!(id, existing);
                            }
                            other => prop_assert!(false, "adding a layer: {:?}", other),
                        }
                    }
                    Op::RenameLayer(pick, name) => {
                        let id = layer(pick);
                        let name = format!("layer {name}");
                        let taken = drawing.layers().find(&name).filter(|other| *other != id);
                        let renamed = change_layer(&mut drawing, id, |layer| layer.name = name);
                        prop_assert_eq!(renamed.is_ok(), id != LayerId::FIRST && taken.is_none(), "{:?}", renamed);
                    }
                    Op::SwitchLayer(pick, on, frozen, locked) => {
                        let id = layer(pick);
                        let switched = change_layer(&mut drawing, id, |layer| {
                            layer.on = on;
                            layer.frozen = frozen;
                            layer.locked = locked;
                        });
                        prop_assert_eq!(switched.is_ok(), (on && !frozen) || id != drawing.current_layer(), "{:?}", switched);
                    }
                    Op::AddMLeaderStyle(name) => {
                        let name = format!("ml{name}");
                        let taken = drawing.mleader_styles().find(&name).is_some();
                        let mut tx = drawing.transaction();
                        let made = tx.add_mleader_style(MLeaderStyle::named(name));
                        prop_assert_eq!(made.is_err(), taken);
                        tx.commit();
                    }
                    Op::RestyleMLeader(pick, size) => {
                        let styles: Vec<MLeaderStyleId> = drawing.mleader_styles().iter().map(|(id, _)| id).collect();
                        let id = styles[pick % styles.len()];
                        let style = MLeaderStyle { arrow_size: size, ..drawing.mleader_styles().get(id).unwrap().clone() };
                        let mut tx = drawing.transaction();
                        let changed = tx.replace_mleader_style(id, style);
                        prop_assert_eq!(changed.is_ok(), size > 0.0);
                        tx.commit();
                    }
                    Op::RemoveMLeaderStyle(pick) => {
                        let styles: Vec<MLeaderStyleId> = drawing.mleader_styles().iter().map(|(id, _)| id).collect();
                        let id = styles[pick % styles.len()];
                        let used = drawing.entities().any(|(_, entity)| matches!(entity, Entity::MLeader(m) if m.style == id))
                            || drawing.settings().current_mleader_style == id;
                        let mut tx = drawing.transaction();
                        let removed = tx.remove_mleader_style(id);
                        prop_assert_eq!(removed.is_ok(), id != MLeaderStyleId::FIRST && !used);
                        tx.commit();
                    }
                    Op::SetMLeaderStyle(pick) => {
                        let styles: Vec<MLeaderStyleId> = drawing.mleader_styles().iter().map(|(id, _)| id).collect();
                        let id = styles[pick % styles.len()];
                        let mut tx = drawing.transaction();
                        tx.set_current_mleader_style(id).unwrap();
                        tx.commit();
                    }
                    Op::CreateMLeader(x, pick) => {
                        let styles: Vec<MLeaderStyleId> = drawing.mleader_styles().iter().map(|(id, _)| id).collect();
                        let style = styles[pick % styles.len()];
                        let mleader = crate::MLeaderEntity::new(
                            Point2::new(x, 0.0),
                            Point2::new(x + 10.0, 10.0),
                            crate::MLeaderContent::Text("Note".to_owned()),
                        )
                        .in_style(style);
                        let mut tx = drawing.transaction();
                        let id = tx.insert(mleader).unwrap();
                        tx.commit();
                        allocated.push(id);
                    }
                    Op::MergeLayer(from, into) => {
                        let (from, into) = (layer(from), layer(into));
                        let refused = from == LayerId::FIRST || from == into || drawing.current_layer() == from;
                        let mut tx = drawing.transaction();
                        let merged = tx.merge_layers(&[from], into);
                        tx.commit();
                        prop_assert_eq!(merged.is_ok(), !refused);
                        if merged.is_ok() {
                            prop_assert!(!drawing.layers().contains(from));
                            let on_it = |record: &EntityRecord| record.properties.layer == from;
                            prop_assert!(!drawing.records().any(|(_, record)| on_it(record)));
                            prop_assert!(!drawing.blocks().iter().any(|(_, block)| block.entities.iter().any(on_it)));
                        }
                    }
                    Op::DeleteLayer(pick) => {
                        let id = layer(pick);
                        let refused = id == LayerId::FIRST || drawing.current_layer() == id;
                        let before = drawing.len();
                        let on_it = drawing.records().filter(|(_, record)| record.properties.layer == id).count();
                        let mut tx = drawing.transaction();
                        let deleted = tx.delete_layers(&[id]);
                        tx.commit();
                        prop_assert_eq!(deleted.is_ok(), !refused);
                        if deleted.is_ok() {
                            prop_assert!(!drawing.layers().contains(id));
                            prop_assert_eq!(drawing.len(), before - on_it);
                        }
                    }
                    Op::RemoveLayer(pick) => {
                        let id = layer(pick);
                        let in_use = drawing.records().any(|(_, record)| record.properties.layer == id);
                        let current = drawing.current_layer() == id;
                        let mut tx = drawing.transaction();
                        let removed = tx.remove_layer(id);
                        tx.commit();
                        prop_assert_eq!(removed.is_ok(), id != LayerId::FIRST && !in_use && !current);
                    }
                    Op::SetCurrent(pick) => {
                        let id = layer(pick);
                        let shown = drawing.layers().get(id).unwrap().is_shown();
                        let mut tx = drawing.transaction();
                        let set = tx.set_current_layer(id);
                        tx.commit();
                        prop_assert_eq!(set.is_ok(), shown);
                    }
                    Op::SetUnit(pick) => {
                        let units = LengthUnit::ALL;
                        let unit = (pick % (units.len() + 1)).checked_sub(1).map(|index| units[index]);
                        let layer_before = drawing.current_layer();
                        let mut tx = drawing.transaction();
                        tx.set_unit(unit);
                        tx.commit();
                        prop_assert_eq!(drawing.unit(), unit);
                        prop_assert_eq!(drawing.current_layer(), layer_before);
                    }
                    Op::LoadLinetype(pick) => {
                        let built_in = LinetypeDef::built_in();
                        let def = built_in[pick % built_in.len()].clone();
                        let taken = drawing.linetypes().find(&def.name);
                        let mut tx = drawing.transaction();
                        let added = tx.add_linetype(def);
                        tx.commit();
                        match (added, taken) {
                            (Ok(id), None) => {
                                prop_assert!(!linetypes_made.contains(&id), "linetype ID {} was reused", id);
                                linetypes_made.push(id);
                            }
                            (Err(TransactionError::DuplicateLinetypeName(id)), Some(existing)) => {
                                prop_assert_eq!(id, existing);
                            }
                            other => prop_assert!(false, "loading a linetype: {:?}", other),
                        }
                    }
                    Op::RemoveLinetype(pick) => {
                        let id = linetype(pick);
                        let exists = drawing.linetypes().contains(id);
                        let used = in_use(&drawing, id);
                        let mut tx = drawing.transaction();
                        let removed = tx.remove_linetype(id);
                        tx.commit();
                        prop_assert_eq!(removed.is_ok(), exists && id != LinetypeId::FIRST && !used, "{:?}", removed);
                    }
                    Op::Dash(pick, lt) if !live.is_empty() => {
                        let id = live[pick % live.len()];
                        let named = Linetype::Named(linetype(lt));
                        let before = drawing.properties(id).unwrap();
                        let mut tx = drawing.transaction();
                        let set = tx.set_properties(id, Properties { linetype: named, ..before });
                        tx.commit();
                        prop_assert_eq!(set.is_ok(), drawing.linetypes().contains(linetype(lt)));
                    }
                    Op::DashLayer(pick, lt) => {
                        let named = Linetype::Named(linetype(lt));
                        let set = change_layer(&mut drawing, layer(pick), |layer| layer.linetype = named);
                        prop_assert_eq!(set.is_ok(), drawing.linetypes().contains(linetype(lt)), "{:?}", set);
                    }
                    Op::SetCurrentLinetype(pick) => {
                        let named = Linetype::Named(linetype(pick));
                        let mut tx = drawing.transaction();
                        let set = tx.set_current_linetype(named);
                        tx.commit();
                        prop_assert_eq!(set.is_ok(), drawing.linetypes().contains(linetype(pick)));
                    }
                    Op::SetLtscale(scale) => {
                        let mut tx = drawing.transaction();
                        let set = tx.set_ltscale(scale);
                        tx.commit();
                        prop_assert_eq!(set.is_ok(), scale > 0.0);
                        prop_assert!(drawing.ltscale() > 0.0);
                    }
                    Op::SetPointStyle(mode, size) => {
                        let style = PointStyle::new(mode, size);
                        let previous = drawing.settings().point_style;
                        let shown_before = drawing.shown_revision();
                        let mut tx = drawing.transaction();
                        let set = tx.set_point_style(style);
                        tx.commit();
                        prop_assert_eq!(set.is_ok(), style.check().is_ok());
                        if set.is_ok() {
                            prop_assert_eq!(drawing.settings().point_style, style);
                            prop_assert!(drawing.settings().point_style.check().is_ok());
                            prop_assert_eq!(drawing.shown_revision() != shown_before, style != previous);
                        }
                    }
                    Op::AddTextStyle(name) => {
                        let name = format!("Style {name}");
                        let taken = drawing.text_styles().find(&name);
                        let mut tx = drawing.transaction();
                        let added = tx.add_text_style(TextStyle::named(name.to_uppercase()));
                        tx.commit();
                        match (added, taken) {
                            (Ok(_), None) => {}
                            (Err(TransactionError::DuplicateTextStyleName(id)), Some(existing)) => {
                                prop_assert_eq!(id, existing);
                            }
                            other => prop_assert!(false, "adding a text style: {:?}", other),
                        }
                    }
                    Op::CreateText(x, pick) => {
                        let styles: Vec<TextStyleId> = drawing.text_styles().iter().map(|(id, _)| id).collect();
                        let index = pick % (styles.len() + 1);
                        let style = styles.get(index).copied().unwrap_or(TextStyleId::from_raw(u32::MAX));
                        let text = TextEntity { style, ..TextEntity::new(Point2::new(x, 0.0), 2.5, 0.0, "Ab") };
                        let mut tx = drawing.transaction();
                        let inserted = tx.insert(text);
                        tx.commit();
                        prop_assert_eq!(inserted.is_ok(), index < styles.len(), "{:?}", inserted);
                        if let Ok(id) = inserted {
                            allocated.push(id);
                        }
                    }
                    Op::AddLayout(name) => {
                        let name = format!("Sheet {name}");
                        let taken = drawing.layouts().find(&name);
                        let mut tx = drawing.transaction();
                        let added = tx.add_layout(Layout::named(name.to_uppercase()));
                        tx.commit();
                        match (added, taken) {
                            (Ok(_), None) => {}
                            (Err(TransactionError::DuplicateLayoutName(id)), Some(existing)) => {
                                prop_assert_eq!(id, existing);
                            }
                            other => prop_assert!(false, "adding a layout: {:?}", other),
                        }
                    }
                    Op::RenameLayout(pick, name) => {
                        let layouts: Vec<LayoutId> = drawing.layouts().iter().map(|(id, _)| id).collect();
                        let id = layouts[pick % layouts.len()];
                        let name = format!("Sheet {name}");
                        let taken = drawing.layouts().find(&name);
                        let mut tx = drawing.transaction();
                        let renamed = tx.rename_layout(id, &name);
                        tx.commit();
                        let allowed = id != LayoutId::MODEL && taken.is_none_or(|other| other == id);
                        prop_assert_eq!(renamed.is_ok(), allowed, "{:?}", renamed);
                        if allowed {
                            prop_assert_eq!(drawing.layouts().find(&name), Some(id));
                        }
                    }
                    Op::SetLayoutPlot(pick, scale, turns) => {
                        let layouts: Vec<LayoutId> = drawing.layouts().iter().map(|(id, _)| id).collect();
                        let id = layouts[pick % layouts.len()];
                        let plot = LayoutPlot {
                            scale: (scale != 0.0).then_some(scale),
                            turns,
                            ..LayoutPlot::default()
                        };
                        let mut tx = drawing.transaction();
                        let set = tx.set_layout_plot(id, plot);
                        tx.commit();
                        let allowed = id != LayoutId::MODEL && plot.is_valid();
                        prop_assert_eq!(set.is_ok(), allowed, "{:?}", set);
                        if allowed {
                            prop_assert_eq!(drawing.layouts().get(id).map(Layout::plot_settings), Some(plot));
                        }
                    }
                    Op::SetLayoutSheet(pick, width, height) => {
                        let layouts: Vec<LayoutId> = drawing.layouts().iter().map(|(id, _)| id).collect();
                        let id = layouts[pick % layouts.len()];
                        let size = Vec2::new(width, height);
                        let mut tx = drawing.transaction();
                        let set = tx.set_layout_sheet(id, size);
                        tx.commit();
                        let allowed = id != LayoutId::MODEL && Layout::sheet_size_allowed(size);
                        prop_assert_eq!(set.is_ok(), allowed, "{:?}", set);
                        if allowed {
                            prop_assert_eq!(drawing.layouts().get(id).map(Layout::sheet_size), Some(size));
                        }
                    }
                    Op::RemoveLayout(pick) => {
                        let layouts: Vec<LayoutId> = drawing.layouts().iter().map(|(id, _)| id).collect();
                        let id = layouts[pick % layouts.len()];
                        let on_it: Vec<EntityId> = drawing.records().filter(|(_, r)| r.space == id).map(|(id, _)| id).collect();
                        let mut tx = drawing.transaction();
                        let removed = tx.remove_layout(id);
                        tx.commit();
                        prop_assert_eq!(removed.is_ok(), id != LayoutId::MODEL, "{:?}", removed);
                        if removed.is_ok() {
                            prop_assert!(!drawing.layouts().contains(id));
                            for entity in on_it {
                                prop_assert!(drawing.entity(entity).is_none(), "entity {} outlived its layout", entity);
                            }
                        }
                    }
                    Op::CreateIn(pick, x) => {
                        let layouts: Vec<LayoutId> = drawing.layouts().iter().map(|(id, _)| id).collect();
                        let space = layouts[pick % layouts.len()];
                        let mut tx = drawing.transaction();
                        let id = tx.insert_in(space, line(x, 0.0, x + 10.0, 5.0)).unwrap();
                        tx.commit();
                        prop_assert_eq!(drawing.space_of(id), Some(space));
                        allocated.push(id);
                    }
                    Op::Reorder(pick, order) => {
                        let live = live_ids(&drawing);
                        if let Some(&id) = live.get(pick % live.len().max(1)) {
                            let mut tx = drawing.transaction();
                            tx.set_order(id, order).unwrap();
                            tx.commit();
                            prop_assert_eq!(drawing.record(id).map(|r| r.order), Some(order));
                        }
                    }
                    Op::SaveState(name) => {
                        let name = format!("state{name}");
                        let taken = drawing.layer_states().find(&name).is_some();
                        let state = drawing.layer_state_now(name.clone());
                        let mut tx = drawing.transaction();
                        let made = tx.add_layer_state(state.clone());
                        prop_assert_eq!(made.is_err(), taken, "a name is taken once");
                        tx.commit();
                        if !taken {
                            let id = drawing.layer_states().find(&name).unwrap();
                            prop_assert_eq!(drawing.layer_states().get(id), Some(&state));
                        }
                    }
                    Op::RestoreState(pick) if !drawing.layer_states().is_empty() => {
                        let states: Vec<LayerStateId> = drawing.layer_states().iter().map(|(id, _)| id).collect();
                        let id = states[pick % states.len()];
                        let state = drawing.layer_states().get(id).unwrap().clone();
                        let current = drawing.current_layer();
                        let mut tx = drawing.transaction();
                        tx.restore_layer_state(id).unwrap();
                        tx.commit();
                        // Every layer it names is as it keeps it, but that the
                        // current one stays shown.
                        for (layer_id, layer) in drawing.layers().iter() {
                            if let Some(saved) = state.layer(&layer.name) {
                                prop_assert_eq!(layer.locked, saved.locked);
                                prop_assert_eq!(layer.color, saved.color);
                                prop_assert_eq!(layer.lineweight, saved.lineweight);
                                if layer_id == current {
                                    prop_assert!(layer.is_shown());
                                } else {
                                    prop_assert_eq!((layer.on, layer.frozen), (saved.on, saved.frozen));
                                }
                            }
                        }
                    }
                    Op::DeleteState(pick) if !drawing.layer_states().is_empty() => {
                        let states: Vec<LayerStateId> = drawing.layer_states().iter().map(|(id, _)| id).collect();
                        let id = states[pick % states.len()];
                        let mut tx = drawing.transaction();
                        tx.remove_layer_state(id).unwrap();
                        tx.commit();
                        prop_assert!(!drawing.layer_states().contains(id));
                    }
                    Op::RestoreState(_) | Op::DeleteState(_) => {}
                    Op::SaveView(name, space, size) => {
                        let name = format!("view{name}");
                        let spaces: Vec<LayoutId> = drawing.layouts().iter().map(|(id, _)| id).collect();
                        let space = spaces[space % spaces.len()];
                        let view = NamedView {
                            name: name.clone(),
                            space,
                            center: Point2::new(size, -size),
                            width: size,
                            height: size / 2.0,
                        };
                        let taken = drawing.views().find(&name).is_some();
                        let mut tx = drawing.transaction();
                        let made = tx.add_view(view);
                        prop_assert_eq!(made.is_err(), taken || size <= 0.0);
                        tx.commit();
                    }
                    Op::DeleteView(pick) if !drawing.views().is_empty() => {
                        let views: Vec<NamedViewId> = drawing.views().iter().map(|(id, _)| id).collect();
                        let id = views[pick % views.len()];
                        let mut tx = drawing.transaction();
                        tx.remove_view(id).unwrap();
                        tx.commit();
                    }
                    Op::DeleteView(_) => {}
                    Op::WipeoutFrame(shown) => {
                        let mut tx = drawing.transaction();
                        tx.set_wipeout_frame(shown);
                        tx.commit();
                        prop_assert_eq!(drawing.settings().wipeout_frame, shown);
                    }
                    Op::CreateOnTop(x) => {
                        let top = drawing.top_order();
                        let mut tx = drawing.transaction();
                        let id = tx
                            .insert_on_top(LineEntity::new(Point2::new(x, 0.0), Point2::new(x, 1.0)))
                            .unwrap();
                        tx.commit();
                        allocated.push(id);
                        prop_assert_eq!(drawing.record(id).map(|r| r.order), Some(top + 1));
                        prop_assert_eq!(drawing.top_order(), top + 1);
                    }
                    Op::MoveTo(pick, space) => {
                        let live = live_ids(&drawing);
                        let layouts: Vec<LayoutId> = drawing.layouts().iter().map(|(id, _)| id).collect();
                        if let Some(&id) = live.get(pick % live.len().max(1)) {
                            let space = layouts[space % layouts.len()];
                            let mut tx = drawing.transaction();
                            tx.set_space(id, space).unwrap();
                            tx.commit();
                            prop_assert_eq!(drawing.space_of(id), Some(space));
                        }
                    }
                    Op::RemoveTextStyle(pick) => {
                        let styles: Vec<TextStyleId> = drawing.text_styles().iter().map(|(id, _)| id).collect();
                        let id = styles[pick % styles.len()];
                        let used = drawing.entities().any(|(_, entity)| matches!(entity, Entity::Text(text) if text.style == id))
                            || drawing.current_text_style() == id;
                        let mut tx = drawing.transaction();
                        let removed = tx.remove_text_style(id);
                        tx.commit();
                        prop_assert_eq!(removed.is_ok(), id != TextStyleId::FIRST && !used, "{:?}", removed);
                    }
                    // A style's values change, or are refused, and no text
                    // already made changes with them (#221).
                    Op::RestyleText(pick, height, width_factor, oblique) => {
                        let styles: Vec<TextStyleId> = drawing.text_styles().iter().map(|(id, _)| id).collect();
                        let id = styles[pick % styles.len()];
                        let texts_before: Vec<Entity> = drawing.entities().map(|(_, e)| e.clone()).collect();
                        let values = TextStyle { height, width_factor, oblique, ..TextStyle::named("ignored") };
                        let fine = values.check().is_ok();
                        let mut tx = drawing.transaction();
                        let set = tx.set_text_style_values(id, values);
                        tx.commit();
                        prop_assert_eq!(set.is_ok(), fine, "{:?}", set);
                        let texts_after: Vec<Entity> = drawing.entities().map(|(_, e)| e.clone()).collect();
                        prop_assert_eq!(texts_after, texts_before);
                    }
                    Op::SetTextStyle(pick) => {
                        let styles: Vec<TextStyleId> = drawing.text_styles().iter().map(|(id, _)| id).collect();
                        let id = styles[pick % styles.len()];
                        let mut tx = drawing.transaction();
                        tx.set_current_text_style(id).unwrap();
                        tx.commit();
                        prop_assert_eq!(drawing.current_text_style(), id);
                    }
                    Op::AddDimStyle(name) => {
                        let name = format!("Dim {name}");
                        let taken = drawing.dim_styles().find(&name);
                        let mut tx = drawing.transaction();
                        let added = tx.add_dim_style(DimStyle::named(name.to_uppercase()));
                        tx.commit();
                        match (added, taken) {
                            (Ok(_), None) => {}
                            (Err(TransactionError::DuplicateDimStyleName(id)), Some(existing)) => {
                                prop_assert_eq!(id, existing);
                            }
                            other => prop_assert!(false, "adding a dimension style: {:?}", other),
                        }
                    }
                    Op::RestyleDimStyle(pick, height, name) => {
                        let styles: Vec<DimStyleId> = drawing.dim_styles().iter().map(|(id, _)| id).collect();
                        let id = styles[pick % styles.len()];
                        let mut style = drawing.dim_styles().get(id).unwrap().clone();
                        style.text_height = height;
                        let rename = name % 2 == 0;
                        let new_name = format!("DIM {name}");
                        if rename {
                            style = style.renamed(new_name.clone());
                        }
                        let name_taken = drawing.dim_styles().find(&new_name).is_some_and(|other| other != id);
                        let mut tx = drawing.transaction();
                        let replaced = tx.replace_dim_style(id, style);
                        tx.commit();
                        let allowed = height > 0.0
                            && !(rename && id == DimStyleId::FIRST)
                            && !(rename && name_taken);
                        prop_assert_eq!(replaced.is_ok(), allowed, "{:?}", replaced);
                        prop_assert!(drawing.dim_styles().iter().all(|(_, s)| s.check().is_ok()));
                    }
                    Op::RemoveDimStyle(pick) => {
                        let styles: Vec<DimStyleId> = drawing.dim_styles().iter().map(|(id, _)| id).collect();
                        let id = styles[pick % styles.len()];
                        let current = drawing.current_dim_style() == id;
                        let mut tx = drawing.transaction();
                        let removed = tx.remove_dim_style(id);
                        tx.commit();
                        prop_assert_eq!(removed.is_ok(), id != DimStyleId::FIRST && !current, "{:?}", removed);
                    }
                    Op::SetDimStyle(pick) => {
                        let styles: Vec<DimStyleId> = drawing.dim_styles().iter().map(|(id, _)| id).collect();
                        let index = pick % (styles.len() + 1);
                        let id = styles.get(index).copied().unwrap_or(DimStyleId::from_raw(u32::MAX));
                        let mut tx = drawing.transaction();
                        let set = tx.set_current_dim_style(id);
                        tx.commit();
                        prop_assert_eq!(set.is_ok(), index < styles.len());
                        prop_assert!(drawing.dim_styles().contains(drawing.current_dim_style()));
                    }
                    Op::DefineBlock(name, len) => {
                        let name = format!("Block {name}");
                        let taken = drawing.blocks().find(&name);
                        let block = Block::new(name.to_uppercase(), Point2::ORIGIN, vec![
                            EntityRecord::new(line(0.0, 0.0, len, 0.0), Properties::default()),
                        ]);
                        let mut tx = drawing.transaction();
                        let defined = tx.define_block(block);
                        tx.commit();
                        match (defined, taken) {
                            (Ok(_), None) => {}
                            (Err(TransactionError::DuplicateBlockName(id)), Some(existing)) => prop_assert_eq!(id, existing),
                            other => prop_assert!(false, "defining a block: {:?}", other),
                        }
                    }
                    Op::PlaceBlock(pick, x, scale) => {
                        let blocks: Vec<BlockId> = drawing.blocks().iter().map(|(id, _)| id).collect();
                        let index = pick % (blocks.len() + 1);
                        let block = blocks.get(index).copied().unwrap_or(BlockId::from_raw(u32::MAX));
                        let mut tx = drawing.transaction();
                        let placed = tx.insert(BlockReferenceEntity::new(block, Point2::new(x, 0.0), scale, 0.3));
                        tx.commit();
                        prop_assert_eq!(placed.is_ok(), index < blocks.len() && block != BlockId::FIRST, "{:?}", placed);
                        if let Ok(id) = placed {
                            allocated.push(id);
                        }
                    }
                    Op::RedefineBlock(pick, len) => {
                        let blocks: Vec<BlockId> = drawing.blocks().iter().map(|(id, _)| id).collect();
                        let id = blocks[pick % blocks.len()];
                        let name = drawing.blocks().get(id).unwrap().name.clone();
                        let block = Block::new(name, Point2::new(1.0, 1.0), vec![
                            EntityRecord::new(line(0.0, 0.0, 0.0, len), Properties::default()),
                            EntityRecord::new(crate::CircleEntity::new(Point2::ORIGIN, len), Properties::default()),
                        ]);
                        let mut tx = drawing.transaction();
                        let redefined = tx.redefine_block(id, block);
                        tx.commit();
                        prop_assert_eq!(redefined.is_ok(), id != BlockId::FIRST, "{:?}", redefined);
                    }
                    Op::NestBlock(pick, inner) => {
                        let blocks: Vec<BlockId> = drawing.blocks().iter().map(|(id, _)| id).collect();
                        let id = blocks[pick % blocks.len()];
                        let inner = blocks[inner % blocks.len()];
                        let name = drawing.blocks().get(id).unwrap().name.clone();
                        let block = Block::new(name, Point2::ORIGIN, vec![EntityRecord::new(
                            BlockReferenceEntity::new(inner, Point2::new(5.0, 5.0), 1.0, 0.0),
                            Properties::default(),
                        )]);
                        let cycle = inner == id || drawing.blocks().reaches(inner, id);
                        let mut tx = drawing.transaction();
                        let nested = tx.redefine_block(id, block);
                        tx.commit();
                        let allowed = id != BlockId::FIRST && inner != BlockId::FIRST && !cycle;
                        prop_assert_eq!(nested.is_ok(), allowed, "{:?}", nested);
                    }
                    Op::RemoveBlock(pick) => {
                        let blocks: Vec<BlockId> = drawing.blocks().iter().map(|(id, _)| id).collect();
                        let id = blocks[pick % blocks.len()];
                        let used = drawing.entities().any(|(_, e)| matches!(e, Entity::BlockRef(r) if r.block == id))
                            || drawing.blocks().iter().any(|(other, b)| other != id && b.references().any(|i| i == id));
                        let mut tx = drawing.transaction();
                        let removed = tx.remove_block(id);
                        tx.commit();
                        prop_assert_eq!(removed.is_ok(), id != BlockId::FIRST && !used, "{:?}", removed);
                    }
                    Op::Purge => {
                        let layers_before: Vec<(LayerId, String)> = drawing.layers().iter().map(|(id, l)| (id, l.name.clone())).collect();
                        let layer_used = |drawing: &Drawing, id: LayerId| {
                            drawing.records().any(|(_, r)| r.properties.layer == id)
                                || drawing.blocks().iter().any(|(_, b)| b.entities.iter().any(|r| r.properties.layer == id))
                                || drawing.current_layer() == id
                                || id == LayerId::FIRST
                        };
                        let block_used = |drawing: &Drawing, id: BlockId| {
                            drawing.entities().any(|(_, e)| matches!(e, Entity::BlockRef(r) if r.block == id))
                                || drawing.blocks().iter().any(|(other, b)| other != id && b.references().any(|i| i == id))
                                || id == BlockId::FIRST
                        };
                        // A block stays when an entity places it, or a block
                        // that stays does; a layer, when an entity or a kept
                        // block's entity is on it.
                        let mut kept_blocks: Vec<BlockId> = drawing.blocks().iter().map(|(id, _)| id)
                            .filter(|id| *id == BlockId::FIRST || drawing.entities().any(|(_, e)| matches!(e, Entity::BlockRef(r) if r.block == *id)))
                            .collect();
                        loop {
                            let more: Vec<BlockId> = drawing.blocks().iter()
                                .filter(|(id, _)| !kept_blocks.contains(id))
                                .filter(|(id, _)| kept_blocks.iter().any(|k| drawing.blocks().get(*k).is_some_and(|b| b.references().any(|i| i == *id))))
                                .map(|(id, _)| id)
                                .collect();
                            if more.is_empty() { break; }
                            kept_blocks.extend(more);
                        }
                        let in_kept_block = |uses: &dyn Fn(&EntityRecord) -> bool| kept_blocks.iter().any(|k| drawing.blocks().get(*k).is_some_and(|b| b.entities.iter().any(uses)));
                        let kept_layers: Vec<LayerId> = layers_before.iter()
                            .filter(|(id, _)| *id == LayerId::FIRST || drawing.current_layer() == *id
                                || drawing.records().any(|(_, r)| r.properties.layer == *id)
                                || in_kept_block(&|r| r.properties.layer == *id))
                            .map(|(id, _)| *id).collect();
                        let kept_linetypes: Vec<LinetypeId> = drawing.linetypes().iter().map(|(id, _)| id)
                            .filter(|id| {
                                let named = Linetype::Named(*id);
                                *id == LinetypeId::FIRST
                                    || drawing.current_linetype() == named
                                    || drawing.records().any(|(_, r)| r.properties.linetype == named)
                                    || kept_layers.iter().any(|l| drawing.layers().get(*l).is_some_and(|layer| layer.linetype == named))
                                    || in_kept_block(&|r| r.properties.linetype == named)
                            })
                            .collect();
                        let _ = (&layer_used, &block_used);
                        let mut tx = drawing.transaction();
                        let purged = tx.purge(PurgeScope::ALL);
                        tx.commit();
                        for id in &kept_layers {
                            prop_assert!(drawing.layers().contains(*id), "a used layer was purged");
                        }
                        for id in &kept_blocks {
                            prop_assert!(drawing.blocks().contains(*id), "a used block was purged");
                        }
                        for id in &kept_linetypes {
                            prop_assert!(drawing.linetypes().contains(*id), "a used linetype was purged");
                        }
                        // What is left and not permanent or current is in use;
                        // nothing else stayed.
                        prop_assert_eq!(drawing.blocks().len(), kept_blocks.len(), "an unused block stayed");
                        prop_assert_eq!(drawing.layers().len(), kept_layers.len(), "an unused layer stayed");
                        for (id, _) in drawing.layers().iter() {
                            prop_assert!(layer_used(&drawing, id));
                        }
                        for (id, _) in drawing.blocks().iter() {
                            prop_assert!(block_used(&drawing, id));
                        }
                        prop_assert_eq!(purged.layers.len(), layers_before.len() - kept_layers.len());
                    }
                    Op::Together(pick, x) => {
                        together += 1;
                        let mut tx = drawing.transaction();
                        let new_layer = tx.add_layer(Layer::named(format!("together {together}"))).unwrap();
                        tx.set_current_layer(new_layer).unwrap();
                        allocated.push(tx.insert(line(x, 0.0, x, 5.0)).unwrap());
                        if !live.is_empty() {
                            tx.set_properties(live[pick % live.len()], Properties::on(new_layer)).unwrap();
                        }
                        tx.commit();
                        layers_made.push(new_layer);
                    }
                    // A group of two (one when both picks are the same
                    // entity), named or not: a name already taken is refused.
                    Op::Group(a, b, name) if !live.is_empty() => {
                        let mut members = vec![live[a % live.len()], live[b % live.len()]];
                        members.dedup();
                        let group = if name == 0 {
                            Group::unnamed(members)
                        } else {
                            Group::named(format!("g{name}"), members)
                        };
                        let taken = name != 0 && drawing.groups().find(&group.name).is_some();
                        let mut tx = drawing.transaction();
                        let added = tx.add_group(group);
                        tx.commit();
                        prop_assert_eq!(added.is_ok(), !taken, "{:?}", added);
                    }
                    Op::GroupAdd(g, e) | Op::GroupRemove(g, e)
                        if !drawing.groups().is_empty() && !live.is_empty() =>
                    {
                        let groups: Vec<GroupId> = drawing.groups().iter().map(|(id, _)| id).collect();
                        let id = groups[g % groups.len()];
                        let entity = live[e % live.len()];
                        let mut group = drawing.groups().get(id).unwrap().clone();
                        let adding = matches!(op, Op::GroupAdd(..));
                        if adding {
                            group.members.push(entity);
                        } else {
                            group.members.retain(|member| *member != entity);
                        }
                        let mut tx = drawing.transaction();
                        let replaced = tx.replace_group(id, group.clone());
                        tx.commit();
                        let fine = !group.members.is_empty()
                            && sorted(group.members.clone()).windows(2).all(|pair| pair[0] != pair[1]);
                        prop_assert_eq!(replaced.is_ok(), fine, "{:?}", replaced);
                    }
                    Op::GroupRename(g, name) if !drawing.groups().is_empty() => {
                        let groups: Vec<GroupId> = drawing.groups().iter().map(|(id, _)| id).collect();
                        let id = groups[g % groups.len()];
                        let mut group = drawing.groups().get(id).unwrap().clone();
                        group.name = format!("g{name}");
                        let taken = drawing.groups().find(&group.name).is_some_and(|other| other != id);
                        let mut tx = drawing.transaction();
                        let renamed = tx.replace_group(id, group);
                        tx.commit();
                        prop_assert_eq!(renamed.is_ok(), !taken, "{:?}", renamed);
                    }
                    Op::Erase(_) | Op::Modify(..) | Op::Restyle(..) | Op::Dash(..) | Op::Group(..)
                    | Op::GroupAdd(..) | Op::GroupRemove(..) | Op::GroupRename(..) => {}
                    Op::Undo => { drawing.undo(); }
                    Op::Redo => { drawing.redo(); }
                }
                prop_assert!(drawing.next_entity_id() >= high_water);
                high_water = drawing.next_entity_id();
                // Every multileader is drawn by a style the drawing has (#240).
                for (id, entity) in drawing.entities() {
                    if let Entity::MLeader(mleader) = entity {
                        prop_assert!(drawing.mleader_styles().contains(mleader.style), "multileader {} has no style", id);
                    }
                }
                prop_assert!(drawing.mleader_styles().contains(drawing.settings().current_mleader_style));
                // Every named view is of a space the drawing has (#235).
                for (id, view) in drawing.views().iter() {
                    prop_assert!(drawing.layouts().contains(view.space), "view {} is of a space that is gone", id);
                }
                // Every group has members, each an entity of the drawing,
                // once; no two groups share a name (#214).
                let mut group_names = HashSet::new();
                for (id, group) in drawing.groups().iter() {
                    prop_assert!(!group.members.is_empty(), "group {} is empty", id);
                    let mut seen = HashSet::new();
                    for member in &group.members {
                        prop_assert!(drawing.entity(*member).is_some(), "group {} holds {}, which is gone", id, member);
                        prop_assert!(seen.insert(*member), "group {} holds {} twice", id, member);
                    }
                    prop_assert!(group_names.insert(group.name.to_lowercase()), "two groups are called {}", group.name);
                }
                for (id, record) in drawing.records() {
                    prop_assert!(
                        drawing.layers().contains(record.properties.layer),
                        "entity {} is on a layer that does not exist", id
                    );
                }
                let current = drawing.layers().get(drawing.current_layer());
                prop_assert!(current.is_some_and(Layer::is_shown), "the current layer is {:?}", current);
                let known = |linetype: Linetype| match linetype {
                    Linetype::Named(id) => drawing.linetypes().contains(id),
                    Linetype::ByLayer | Linetype::ByBlock => true,
                };
                for (id, record) in drawing.records() {
                    prop_assert!(known(record.properties.linetype), "entity {} has a linetype that does not exist", id);
                }
                for (id, layer) in drawing.layers().iter() {
                    prop_assert!(known(layer.linetype), "layer {} has a linetype that does not exist", id);
                }
                prop_assert!(known(drawing.current_linetype()));
                prop_assert!(drawing.text_styles().contains(drawing.current_text_style()), "the current text style is gone");
                for (id, entity) in drawing.entities() {
                    if let Entity::Text(text) = entity {
                        prop_assert!(drawing.text_styles().contains(text.style), "text {} has a style that does not exist", id);
                    }
                }
                let mut linetype_names: Vec<String> = drawing.linetypes().iter().map(|(_, lt)| lt.name.to_lowercase()).collect();
                linetype_names.sort();
                let count = linetype_names.len();
                linetype_names.dedup();
                prop_assert_eq!(linetype_names.len(), count, "two linetypes share a name");
                let mut names: Vec<String> = drawing.layers().iter().map(|(_, layer)| layer.name.to_lowercase()).collect();
                names.sort();
                let count = names.len();
                names.dedup();
                prop_assert_eq!(names.len(), count, "two layers share a name");
                // Spaces (#191): every entity is in a space that exists, the
                // model is always there, names are unique, and the model's
                // reads see nothing of a layout, nor a layout's of another.
                prop_assert!(drawing.layouts().contains(LayoutId::MODEL));
                for (id, record) in drawing.records() {
                    prop_assert!(drawing.layouts().contains(record.space), "entity {} is in a space that does not exist", id);
                }
                let mut layout_names: Vec<String> = drawing.layouts().iter().map(|(_, l)| l.name.to_lowercase()).collect();
                layout_names.sort();
                let count = layout_names.len();
                layout_names.dedup();
                prop_assert_eq!(layout_names.len(), count, "two layouts share a name");
                for (space, _) in drawing.layouts().iter() {
                    let expected: Vec<EntityId> = sorted(
                        drawing.records().filter(|(id, r)| r.space == space && drawing.is_shown(*id)).map(|(id, _)| id).collect(),
                    );
                    prop_assert_eq!(sorted(drawing.shown_in(space).map(|(id, _)| id).collect()), expected.clone());
                    prop_assert_eq!(sorted(drawing.query_shown_in(space, &everything())), expected);
                }
                prop_assert_eq!(
                    sorted(drawing.shown().map(|(id, _)| id).collect()),
                    sorted(drawing.shown_in(LayoutId::MODEL).map(|(id, _)| id).collect())
                );
            }

            let mut reference = LinearIndex::new();
            reference.rebuild(&mut drawing.entities().map(|(id, entity)| (id, drawing.bounds_of(entity))));
            for (id, entity) in drawing.entities() {
                if let Entity::BlockRef(r) = entity {
                    prop_assert!(drawing.blocks().contains(r.block) && r.block != BlockId::FIRST, "reference {} places nothing", id);
                }
            }
            for (id, _) in drawing.blocks().iter() {
                prop_assert!(!drawing.blocks().reaches(id, id), "block {} holds itself", id);
            }

            prop_assert_eq!(sorted(drawing.query(&everything())), live_ids(&drawing));
            prop_assert_eq!(curves_by_size(&drawing), radii_read_afresh(&drawing));
            let (kept, afresh) = tiers(&drawing);
            prop_assert_eq!(kept, afresh);
            for (x, y, w, h) in queries {
                let query = Aabb::from_points(Point2::new(x, y), Point2::new(x + w, y + h));
                prop_assert_eq!(sorted(drawing.query(&query)), sorted(reference.query(&query)));
            }

            // All the way back, and all the way forward again.
            let now = saved(&drawing);
            let mut steps = 0;
            while drawing.undo().is_some() {
                steps += 1;
            }
            prop_assert_eq!(saved(&drawing), empty);
            for _ in 0..steps {
                prop_assert!(drawing.redo().is_some());
            }
            prop_assert_eq!(saved(&drawing), now);
            prop_assert_eq!(sorted(drawing.query(&everything())), live_ids(&drawing));
            prop_assert_eq!(curves_by_size(&drawing), radii_read_afresh(&drawing));
            let (kept, afresh) = tiers(&drawing);
            prop_assert_eq!(kept, afresh);
        }
    }
}

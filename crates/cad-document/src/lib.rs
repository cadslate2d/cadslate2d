//! The semantic drawing: entities, storage, spatial index, transactions, history.
//!
//! The document preserves CAD semantics — a line is a line, never a list of
//! vertices for the GPU (DESIGN §11). It is mutated only through
//! [`Drawing::transaction`], [`Drawing::undo`], and [`Drawing::redo`]
//! (DESIGN §19.1, ADR-0001). Nothing here knows about commands, selection,
//! the view, or rendering.

#![cfg_attr(test, allow(clippy::float_cmp))]

mod aci;
mod block;
mod construction;
mod dim_style;
mod dimension;
mod drawing;
mod editing;
mod entity;
mod exploding;
mod grips;
mod hatch;
mod hatch_pattern;
mod history;
mod id_alloc;
mod joining;
pub mod kept;
mod layer;
mod layout;
mod leader;
mod linetype;
mod lineweight;
mod mtext;
mod names;
mod point;
mod properties;
mod revision;
mod settings;
pub mod spatial;
mod store;
mod table;
mod tables;
mod text;
mod text_style;
mod transaction;
mod viewport;

pub use aci::{FOREGROUND_INDEX, Paint, index_paint, nearest_index};
pub use block::{Block, BlockChange, BlockId, BlockReferenceEntity, BlockTable, empty_blocks};
/// A hatch's boundary loop: a closed path of straight and arc segments.
pub use cad_geometry::{Path, Spline};
pub use construction::{CONSTRUCTION_REACH, RayEntity, XLineEntity};
pub use dim_style::{DimStyle, DimStyleChange, DimStyleError, DimStyleId, DimStyleTable};
pub use dimension::{DimensionEntity, DimensionError, DimensionLayout, Measure};
pub use drawing::{ApplyReport, Drawing, PurgeScope, Purged, TransactionBuilder, TransactionError};
pub use editing::{
    BreakError, CornerError, Cornered, ExtendError, LengthenError, Lengthening, OffsetError,
    StretchHold, TrimError, TrimParts,
};
pub use entity::{
    ArcEntity, AttributeDefinitionEntity, CircleEntity, EllipseEntity, Entity, LineEntity,
    MTextEntity, PolylineEntity, SharedEntity, SplineEntity, TextEntity,
};
pub use exploding::ExplodeError;
pub use grips::{EntityGrip, GripKind};
pub use hatch::{HatchEntity, HatchError, HatchStyle, MAX_PATTERN_PIECES};
pub use hatch_pattern::{
    HatchPattern, PatternSyntax, hatch_pattern, hatch_patterns, parse_patterns,
};
pub use joining::{JoinError, Joined, close_into_circle, join};
pub use kept::{RawEntity, RawGroup};
pub use layer::{Layer, LayerChange, LayerId, LayerTable};
pub use layout::{Layout, LayoutArea, LayoutChange, LayoutId, LayoutPlot, LayoutTable};
pub use leader::{LeaderEntity, LeaderError, LeaderLayout};
pub use linetype::{LinetypeChange, LinetypeDef, LinetypeError, LinetypeId, LinetypeTable};
pub use lineweight::Weight;
pub use mtext::LINE_SPACING;
pub use names::{NameError, Named, check_name, same_name};
pub use point::{PointEntity, PointStyle, PointStyleError};
pub use properties::{Color, EntityRecord, Linetype, Lineweight, Properties};
pub use revision::Revision;
pub use settings::{DrawingSettings, SettingsChange};
pub use spatial::SpatialIndex;
pub use store::{DuplicateEntityId, EntityStore};
pub use table::{Table, TableChange, TableId};
pub use tables::Tables;
pub use text::{Justification, TextFrame};
pub use text_style::{TextStyle, TextStyleChange, TextStyleId, TextStyleTable};
pub use transaction::{Change, EntityChange, Transaction};
pub use viewport::{ViewportEntity, ViewportError};

//! The semantic drawing: entities, storage, spatial index, transactions, history.
//!
//! The document preserves CAD semantics — a line is a line, never a list of
//! vertices for the GPU (DESIGN §11). It is mutated only through
//! [`Drawing::transaction`], [`Drawing::undo`], and [`Drawing::redo`]
//! (DESIGN §19.1, ADR-0001). Nothing here knows about commands, selection,
//! the view, or rendering.

#![cfg_attr(test, allow(clippy::float_cmp))]

mod aci;
mod drawing;
mod editing;
mod entity;
mod grips;
mod history;
mod id_alloc;
mod joining;
mod layer;
mod lineweight;
mod properties;
mod revision;
mod settings;
pub mod spatial;
mod store;
mod table;
mod transaction;

pub use aci::{FOREGROUND_INDEX, Paint, index_paint};
pub use drawing::{ApplyReport, Drawing, TransactionBuilder, TransactionError};
pub use editing::{
    BreakError, CornerError, Cornered, ExtendError, OffsetError, StretchHold, TrimError, TrimParts,
};
pub use entity::{ArcEntity, CircleEntity, Entity, LineEntity, PolylineEntity, SharedEntity};
pub use grips::{EntityGrip, GripKind};
pub use joining::{JoinError, Joined, close_into_circle, join};
pub use layer::{Layer, LayerChange, LayerId, LayerNameError, LayerTable};
pub use lineweight::Weight;
pub use properties::{Color, EntityRecord, Linetype, Lineweight, Properties};
pub use revision::Revision;
pub use settings::{DrawingSettings, SettingsChange};
pub use spatial::SpatialIndex;
pub use store::{DuplicateEntityId, EntityStore};
pub use table::{Table, TableChange, TableId};
pub use transaction::{Change, EntityChange, Transaction};

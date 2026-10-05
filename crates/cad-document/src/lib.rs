//! The semantic drawing: entities, storage, spatial index, transactions, history.
//!
//! The document preserves CAD semantics — a line is a line, never a list of
//! vertices for the GPU (DESIGN §11). It is mutated only through
//! [`Drawing::transaction`], [`Drawing::undo`], and [`Drawing::redo`]
//! (DESIGN §19.1, ADR-0001). Nothing here knows about commands, selection,
//! the view, or rendering.

#![cfg_attr(test, allow(clippy::float_cmp))]

mod drawing;
mod editing;
mod entity;
mod grips;
mod history;
mod id_alloc;
mod joining;
mod revision;
pub mod spatial;
mod store;
mod transaction;

pub use drawing::{ApplyReport, Drawing, TransactionBuilder, TransactionError};
pub use editing::{
    BreakError, CornerError, Cornered, ExtendError, OffsetError, StretchHold, TrimError, TrimParts,
};
pub use entity::{ArcEntity, CircleEntity, Entity, LineEntity, PolylineEntity, SharedEntity};
pub use grips::{EntityGrip, GripKind};
pub use joining::{JoinError, Joined, close_into_circle, join};
pub use revision::Revision;
pub use spatial::SpatialIndex;
pub use store::{DuplicateEntityId, EntityStore};
pub use transaction::{Change, EntityChange, Transaction};

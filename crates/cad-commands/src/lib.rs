//! The interactive session: command state machines, selection, transient edits.
//!
//! [`Session`] is the single interactive root (ADR-0001). Every user action —
//! pointer, keyboard, typed text — arrives as a method call on it, and the
//! same methods are driven by headless tests and by `cad-wasm`. Nothing here
//! knows about packed GPU geometry or JavaScript.

#![cfg_attr(test, allow(clippy::float_cmp))]

pub mod aids;
pub mod bsection;
mod command;
mod commands;
pub mod constraint_display;
mod dirty;
pub mod elevation;
#[cfg(any(test, feature = "testing"))]
pub mod evaluation;
mod events;
pub mod floorplan;
pub mod grips;
mod history;
pub mod intent;
mod log;
mod overlay;
pub mod proposal;
mod registry;
mod selection;
mod session;
mod settings;
pub mod snap;
pub mod stair;
mod summary;
#[cfg(any(test, feature = "testing"))]
pub mod testing;
#[cfg(test)]
mod tests;
mod toggles;
mod transient;
mod ui_state;
pub mod views;

pub use aids::{Track, TrackKind};
pub use command::{
    Command, CommandContext, CommandInput, FileDrawing, HoveredObject, Prompt, SpaceRequest, Step,
};
pub use dirty::Dirty;
pub use events::{FileFormat, HostRequest, KeyAction, Modifiers, PointerButton};
pub use grips::{Grip, GripId};
pub use intent::{Ran, RunError};
pub use overlay::OverlayItem;
pub use proposal::{Proposal, ProposalSummary};
pub use registry::CommandRegistry;
pub use selection::{Selection, SelectionMethod};
pub use session::{Aim, Session};
pub use settings::{DraftingSettings, Paper, PlotArea, PlotScale, PlotSettings};
pub use snap::{SnapMode, SnapModes, SnapPoint};
pub use summary::{AI_CONTEXT_LIMIT, AiContext, AiGroup, AiObject, SelectionSummary};
pub use toggles::{Toggle, Toggles};
pub use transient::TransientEdit;
pub use ui_state::{ImportReport, LayerRow, UiState};

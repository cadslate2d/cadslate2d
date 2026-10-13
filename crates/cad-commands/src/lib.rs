//! The interactive session: command state machines, selection, transient edits.
//!
//! [`Session`] is the single interactive root (ADR-0001). Every user action —
//! pointer, keyboard, typed text — arrives as a method call on it, and the
//! same methods are driven by headless tests and by `cad-wasm`. Nothing here
//! knows about packed GPU geometry or JavaScript.

#![cfg_attr(test, allow(clippy::float_cmp))]

pub mod aids;
mod command;
mod commands;
mod dirty;
mod events;
pub mod grips;
mod history;
mod log;
mod overlay;
mod registry;
mod selection;
mod session;
mod settings;
pub mod snap;
mod summary;
#[cfg(any(test, feature = "testing"))]
pub mod testing;
#[cfg(test)]
mod tests;
mod toggles;
mod transient;
mod ui_state;

pub use aids::{Track, TrackKind};
pub use command::{Command, CommandContext, CommandInput, HoveredObject, Prompt, Step};
pub use dirty::Dirty;
pub use events::{HostRequest, KeyAction, Modifiers, PointerButton};
pub use grips::{Grip, GripId};
pub use overlay::OverlayItem;
pub use registry::CommandRegistry;
pub use selection::{Selection, SelectionMethod};
pub use session::{Aim, Session};
pub use settings::{DraftingSettings, Paper, PlotArea, PlotScale, PlotSettings};
pub use snap::{SnapMode, SnapModes, SnapPoint};
pub use summary::SelectionSummary;
pub use toggles::{Toggle, Toggles};
pub use transient::TransientEdit;
pub use ui_state::{ImportReport, LayerRow, UiState};

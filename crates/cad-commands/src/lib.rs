//! The interactive session: command state machines, selection, transient edits.
//!
//! [`Session`] is the single interactive root (ADR-0001). Every user action —
//! pointer, keyboard, typed text — arrives as a method call on it, and the
//! same methods are driven by headless tests and by `cad-wasm`. Nothing here
//! knows about packed GPU geometry or JavaScript.

#![cfg_attr(test, allow(clippy::float_cmp))]

mod command;
mod commands;
mod dirty;
mod events;
mod history;
mod log;
mod overlay;
mod registry;
mod selection;
mod session;
pub mod snap;
#[cfg(any(test, feature = "testing"))]
pub mod testing;
#[cfg(test)]
mod tests;
mod toggles;
mod transient;
mod ui_state;

pub use command::{Command, CommandContext, CommandInput, Prompt, Step};
pub use dirty::Dirty;
pub use events::{HostRequest, KeyAction, Modifiers, PointerButton};
pub use overlay::OverlayItem;
pub use registry::CommandRegistry;
pub use selection::Selection;
pub use session::Session;
pub use snap::{SnapMode, SnapModes, SnapPoint};
pub use toggles::{Toggle, Toggles};
pub use transient::TransientEdit;
pub use ui_state::UiState;

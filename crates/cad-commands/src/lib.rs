//! The interactive session: command state machines, selection, transient edits.
//!
//! [`Session`] is the single interactive root (ADR-0001). Every user action —
//! pointer, keyboard, typed text — arrives as a method call on it, and the
//! same methods are driven by headless tests and by `cad-wasm`. Nothing here
//! knows about packed GPU geometry or JavaScript.

#![cfg_attr(test, allow(clippy::float_cmp))]

mod dirty;
mod events;
mod session;

pub use dirty::Dirty;
pub use events::{Modifiers, PointerButton};
pub use session::Session;

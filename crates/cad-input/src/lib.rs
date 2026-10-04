//! Command-line tokenizing and parsing into typed input (DESIGN §13–14).
//!
//! This crate turns text into [`Input`] values and nothing more. It does not
//! know about the drawing or about any specific command: the command names it
//! recognises are registered by the caller, and what a piece of text *means*
//! is decided by the [`ExpectedInput`] the active prompt supplies (§13.3).
//!
//! Every function here is pure and never panics on any input.

#![cfg_attr(test, allow(clippy::float_cmp))]

mod aliases;
mod diagnostic;
mod keyword;
mod number;
mod point;
mod resolve;

pub use aliases::{CommandAliases, DuplicateCommandName};
pub use diagnostic::Diagnostic;
pub use keyword::Keyword;
pub use point::{PointInput, parse_point};
pub use resolve::{ExpectedInput, Input, InputKind, resolve};

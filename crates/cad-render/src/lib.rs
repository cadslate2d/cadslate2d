//! Render preparation (DESIGN §12, ADR-0003).
//!
//! Rust decides *what* is drawn; TypeScript decides how prepared buffers reach
//! the GPU. Two lifetimes are kept strictly apart:
//!
//! - [`RenderScene`]: committed geometry, a function of the document alone.
//!   It is rebuilt only when the drawing's revision changes — never because
//!   the pointer moved or the view changed.
//! - `OverlayFrame`: transient interaction geometry, rebuilt freely.
//!
//! Nothing here mutates the document or the session.

#![cfg_attr(test, allow(clippy::float_cmp))]

mod frame;
mod scene;

pub use frame::{DrawBatch, PackedFrame, StyleRole};
pub use scene::{RenderScene, SceneCache};

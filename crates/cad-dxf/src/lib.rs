//! DXF import and export for the semantic drawing (DESIGN §24, ADR-0005).
//!
//! DXF is an interchange format here, not the document model: group codes
//! never leave this crate and `cad-document` knows nothing about DXF.
//!
//! Imported files are untrusted (§43). The reader is iterative, allocates in
//! proportion to what it actually imports, enforces size limits, and returns
//! a structured error — it never panics on any input.
//!
//! Unsupported content is counted and reported, never silently dropped
//! (§24.2, LOCKED). Round-tripping is *not* lossless when the report says
//! content was skipped.

#![cfg_attr(test, allow(clippy::float_cmp))]

mod error;
mod export;
mod import;
mod report;

pub use error::DxfError;
pub use export::export;
pub use import::{Import, ImportLimits, import, import_with_limits};
pub use report::ImportReport;

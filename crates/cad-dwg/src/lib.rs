//! DWG import and export for the semantic drawing (DESIGN §24, §40;
//! ADR-0036): AutoCAD 2000 (`AC1015`), 2004 (`AC1018`), and 2007/2008
//! (`AC1021`), read and written from the Open Design Alliance's published
//! specification.
//!
//! DWG is a second encoding of the records the DXF reader finishes, never a
//! second model: this crate decodes the containers and the bit-coded
//! objects, and what a record means is `cad-dxf`'s business.
//!
//! Imported files are untrusted (§43). Every count a file can grow is
//! bounded by [`ImportLimits`]; reaching a bound is a structured
//! [`DwgError`], and no input panics.
//!
//! What this crate reads of each section and object, and where the
//! specification is wrong, is in `docs/dwg-matrix.md`.

#![cfg_attr(test, allow(clippy::float_cmp))]

pub mod bits;
mod classes;
mod crc;
pub mod entities;
mod error;
mod groups;
pub mod header;
mod import;
mod limits;
mod lz77;
pub mod objects;
mod r2000;
mod r2004;
mod r2007;
pub mod records;
mod sections;
pub mod tables;
mod version;

pub use classes::Class;
pub use error::DwgError;
pub use import::{Import, import, import_with_limits, records};
pub use limits::ImportLimits;
pub use sections::{File, MapEntry, Section, SectionName};
pub use version::Version;

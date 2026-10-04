//! Pure geometric functions: distances, intersections, predicates.
//!
//! Everything here is stateless and knows nothing about the drawing
//! (ADR-0001). Comparisons go through `cad_core::tolerance` (DESIGN §21).

#![cfg_attr(test, allow(clippy::float_cmp))]

mod segment;

pub use segment::Segment;

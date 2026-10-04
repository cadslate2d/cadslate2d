//! Pure geometric functions: distances, intersections, predicates.
//!
//! Everything here is stateless and knows nothing about the drawing
//! (ADR-0001). Comparisons go through `cad_core::tolerance` (DESIGN §21).

#![cfg_attr(test, allow(clippy::float_cmp))]

mod arc;
mod crossing;
pub mod intersect;
mod polyline;
mod segment;
mod transform;

pub use arc::{Arc, Circle, normalize_angle};
pub use intersect::{Hits, Orientation, Primitive, orientation};
pub use polyline::{bounds_of, polyline_distance_to_point, polyline_segments};
pub use segment::Segment;
pub use transform::{Transform, unit_at_degrees};

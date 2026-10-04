//! Pure geometric functions: distances, intersections, predicates.
//!
//! Everything here is stateless and knows nothing about the drawing
//! (ADR-0001). Comparisons go through `cad_core::tolerance` (DESIGN §21).

#![cfg_attr(test, allow(clippy::float_cmp))]

mod arc;
mod corner;
mod crossing;
mod extend;
pub mod intersect;
mod offset;
mod polyline;
mod segment;
mod transform;
mod trim;

pub use arc::{Arc, Circle, normalize_angle};
pub use corner::{Chamfer, Corner, CornerError, Fillet};
pub use extend::{extend_arc, extend_segment};
pub use intersect::{Hits, Orientation, Primitive, orientation};
pub use offset::{offset_circle, offset_segment};
pub use polyline::{
    arc_from_bulge, bulge_from_arc, bulge_primitive, is_straight, polyline_primitives,
    polyline_spans,
};
pub use segment::Segment;
pub use transform::{Transform, unit_at_degrees};
pub use trim::{trim_arc, trim_circle, trim_segment};

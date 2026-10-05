//! Pure geometric functions: distances, intersections, predicates.
//!
//! Everything here is stateless and knows nothing about the drawing
//! (ADR-0001). Comparisons go through `cad_core::tolerance` (DESIGN §21).

#![cfg_attr(test, allow(clippy::float_cmp))]

mod arc;
mod construct;
mod corner;
mod crossing;
mod curve_fillet;
mod extend;
pub mod intersect;
mod join;
mod offset;
mod path;
mod path_corner;
mod path_offset;
mod polyline;
mod segment;
mod transform;
mod trim;

pub use arc::{Arc, Circle, normalize_angle};
pub use corner::{Chamfer, Corner, CornerError, Fillet};
pub use curve_fillet::{CurveFillet, fillet_curves};
pub use extend::{extend_arc, extend_segment};
pub use intersect::{Hits, Orientation, Primitive, orientation};
pub use join::{
    ArcJoinError, Chain, JoinedArcs, chain_paths, cocircular, join_arcs, join_segments,
};
pub use offset::{distance_to_line, offset_arc, offset_circle, offset_segment};
pub use path::{Location, Path, PathTrimError};
pub use path_corner::{CornerJoin, PathCornerError};
pub use path_offset::{
    PathOffsetError, PathSpan, offset_path, path_spans, raw_offset, stitch_spans,
};
pub use polyline::{
    arc_from_bulge, bulge_end_tangent, bulge_from_arc, bulge_from_tangent, bulge_primitive,
    is_straight, polyline_closes, polyline_primitives, polyline_spans,
};
pub use segment::Segment;
pub use transform::{Transform, unit_at_degrees};
pub use trim::{
    Trimmed, break_arc, break_circle, break_segment, trim_arc, trim_arc_parts, trim_circle,
    trim_circle_parts, trim_segment, trim_segment_parts,
};

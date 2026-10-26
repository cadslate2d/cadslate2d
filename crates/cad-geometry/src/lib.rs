//! Pure geometric functions: distances, intersections, predicates.
//!
//! Everything here is stateless and knows nothing about the drawing
//! (ADR-0001). Comparisons go through `cad_core::tolerance` (DESIGN §21).

#![cfg_attr(test, allow(clippy::float_cmp))]

mod arc;
mod band;
mod cloud;
mod collinear;
pub mod constraint;
mod construct;
mod corner;
mod crossing;
mod curve_fillet;
mod dimension;
pub mod ellipse;
mod extend;
pub mod intersect;
mod join;
mod lengthen;
pub mod members;
mod offset;
pub mod opening;
mod path;
mod path_corner;
mod path_offset;
mod pattern;
mod polygon;
mod polyline;
mod segment;
mod select;
pub mod spline;
pub mod tangent;
mod transform;
mod trim;

pub use arc::{Arc, Circle, normalize_angle};
pub use cloud::{CLOUD_BULGE, MAX_CLOUD_ARCS};
pub use collinear::{CollinearRun, collinear_runs};
pub use corner::{Chamfer, Corner, CornerError, Fillet};
pub use curve_fillet::{CurveFillet, fillet_curves};
pub use dimension::{
    Arrowhead, foot_on_line, normalized, ordinate_leader, perpendicular, readable_angle,
};
pub use ellipse::{CHORDS_PER_TURN, Ellipse};
pub use extend::{extend_arc, extend_segment};
pub use intersect::{Hits, Orientation, Primitive, orientation};
pub use join::{
    ArcJoinError, Chain, JoinedArcs, chain_paths, cocircular, join_arcs, join_segments,
};
pub use lengthen::{lengthen_arc, lengthen_segment, polygon_from_edge, regular_polygon};
pub use members::{Member, MembersError, Outline};
pub use offset::{distance_to_line, offset_arc, offset_circle, offset_segment};
pub use opening::{Leaf, Opening, OpeningError};
pub use path::{Location, Path, PathTrimError};
pub use path_corner::{CornerJoin, PathCornerError};
pub use path_offset::{
    PathOffsetError, PathSpan, offset_path, path_spans, raw_offset, stitch_spans,
};
pub use pattern::{LineFamily, pattern_lines};
pub use polygon::{arc_chords, contains as ring_contains, flatten_path, signed_area, tessellate};
pub use polyline::{
    arc_from_bulge, bulge_end_tangent, bulge_from_arc, bulge_from_tangent, bulge_primitive,
    is_straight, polyline_closes, polyline_primitives, polyline_spans,
};
pub use segment::Segment;
pub use select::SelectionShape;
pub use spline::{CHORD_TOLERANCE, MAX_POINTS, Spline, fit_parameters};
pub use transform::{Transform, unit_at_degrees};
pub use trim::{
    Trimmed, break_arc, break_circle, break_ellipse, break_segment, trim_arc, trim_arc_parts,
    trim_circle, trim_circle_parts, trim_ellipse_parts, trim_segment, trim_segment_parts,
};

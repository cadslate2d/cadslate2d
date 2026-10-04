//! Semantic entity types (DESIGN §11).

use std::sync::Arc;

use cad_core::{Aabb, Point2};
use cad_geometry::Segment;

/// A straight line between two points.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LineEntity {
    pub start: Point2,
    pub end: Point2,
}

impl LineEntity {
    pub const fn new(start: Point2, end: Point2) -> Self {
        Self { start, end }
    }

    pub fn segment(&self) -> Segment {
        Segment::new(self.start, self.end)
    }
}

/// Every kind of entity a drawing can hold.
///
/// Deliberately *not* `#[non_exhaustive]`: when a later milestone adds a
/// variant (circle, arc, polyline, …) the compiler must point at every match
/// in the workspace — rendering, picking, DXF — rather than letting a wildcard
/// arm silently ignore the new entity.
#[derive(Clone, Debug, PartialEq)]
pub enum Entity {
    Line(LineEntity),
}

impl Entity {
    /// World-space bounding box, used by the spatial index.
    pub fn bounds(&self) -> Aabb {
        match self {
            Entity::Line(line) => line.segment().bounds(),
        }
    }

    /// Exact distance from `point` to the entity's geometry, used for picking.
    pub fn distance_to(&self, point: Point2) -> f64 {
        match self {
            Entity::Line(line) => line.segment().distance_to_point(point),
        }
    }
}

impl From<LineEntity> for Entity {
    fn from(line: LineEntity) -> Self {
        Entity::Line(line)
    }
}

/// An immutable entity payload shared between the store and undo history, so
/// history never deep-copies large entities (DESIGN §19.4).
pub type SharedEntity = Arc<Entity>;

#[cfg(test)]
mod tests {
    use super::*;

    fn line() -> Entity {
        LineEntity::new(Point2::new(0.0, 0.0), Point2::new(10.0, 0.0)).into()
    }

    #[test]
    fn line_bounds_cover_its_endpoints() {
        let bounds = line().bounds();
        assert_eq!(bounds.min(), Point2::new(0.0, 0.0));
        assert_eq!(bounds.max(), Point2::new(10.0, 0.0));
    }

    #[test]
    fn line_distance_is_point_to_segment_distance() {
        assert_eq!(line().distance_to(Point2::new(5.0, 2.0)), 2.0);
    }
}

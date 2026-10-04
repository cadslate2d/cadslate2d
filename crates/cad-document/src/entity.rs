//! Semantic entity types (DESIGN §11).

use cad_core::{Aabb, Point2};
use cad_geometry::{Arc, Circle, Segment};

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

/// A full circle. It stays a circle in the document; only the renderer may
/// approximate it (DESIGN §11).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CircleEntity {
    pub center: Point2,
    pub radius: f64,
}

impl CircleEntity {
    pub const fn new(center: Point2, radius: f64) -> Self {
        Self { center, radius }
    }

    pub fn circle(&self) -> Circle {
        Circle::new(self.center, self.radius)
    }
}

/// A circular arc, counter-clockwise from `start_angle` to `end_angle`
/// (radians from +X).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ArcEntity {
    pub center: Point2,
    pub radius: f64,
    pub start_angle: f64,
    pub end_angle: f64,
}

impl ArcEntity {
    pub const fn new(center: Point2, radius: f64, start_angle: f64, end_angle: f64) -> Self {
        Self {
            center,
            radius,
            start_angle,
            end_angle,
        }
    }

    pub fn arc(&self) -> Arc {
        Arc::from_angles(self.center, self.radius, self.start_angle, self.end_angle)
    }
}

impl From<Arc> for ArcEntity {
    fn from(arc: Arc) -> Self {
        Self::new(arc.center, arc.radius, arc.start, arc.end())
    }
}

/// Every kind of entity a drawing can hold.
///
/// Deliberately *not* `#[non_exhaustive]`: when a later milestone adds a
/// variant (polyline, text, …) the compiler must point at every match in the
/// workspace — rendering, picking, DXF — rather than letting a wildcard arm
/// silently ignore the new entity.
#[derive(Clone, Debug, PartialEq)]
pub enum Entity {
    Line(LineEntity),
    Circle(CircleEntity),
    Arc(ArcEntity),
}

impl Entity {
    /// World-space bounding box, used by the spatial index.
    pub fn bounds(&self) -> Aabb {
        match self {
            Entity::Line(line) => line.segment().bounds(),
            Entity::Circle(circle) => circle.circle().bounds(),
            Entity::Arc(arc) => arc.arc().bounds(),
        }
    }

    /// Exact distance from `point` to the entity's geometry, used for picking.
    pub fn distance_to(&self, point: Point2) -> f64 {
        match self {
            Entity::Line(line) => line.segment().distance_to_point(point),
            Entity::Circle(circle) => circle.circle().distance_to_point(point),
            Entity::Arc(arc) => arc.arc().distance_to_point(point),
        }
    }

    pub fn as_line(&self) -> Option<&LineEntity> {
        match self {
            Entity::Line(line) => Some(line),
            _ => None,
        }
    }
}

impl From<LineEntity> for Entity {
    fn from(line: LineEntity) -> Self {
        Entity::Line(line)
    }
}

impl From<CircleEntity> for Entity {
    fn from(circle: CircleEntity) -> Self {
        Entity::Circle(circle)
    }
}

impl From<ArcEntity> for Entity {
    fn from(arc: ArcEntity) -> Self {
        Entity::Arc(arc)
    }
}

/// An immutable entity payload shared between the store and undo history, so
/// history never deep-copies large entities (DESIGN §19.4).
pub type SharedEntity = std::sync::Arc<Entity>;

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

    #[test]
    fn a_circle_is_bounded_and_picked_by_its_curve() {
        let circle: Entity = CircleEntity::new(Point2::new(10.0, 10.0), 4.0).into();
        assert_eq!(
            circle.bounds(),
            Aabb::from_points(Point2::new(6.0, 6.0), Point2::new(14.0, 14.0))
        );
        assert_eq!(
            circle.distance_to(Point2::new(10.0, 10.0)),
            4.0,
            "the interior is not the circle"
        );
        assert_eq!(circle.distance_to(Point2::new(15.0, 10.0)), 1.0);
    }

    #[test]
    fn an_arc_runs_counter_clockwise_between_its_angles() {
        use std::f64::consts::{FRAC_PI_2, PI};
        let arc = ArcEntity::new(Point2::ORIGIN, 10.0, 0.0, FRAC_PI_2);
        let entity: Entity = arc.into();
        assert!(entity.distance_to(Point2::new(0.0, 12.0)) - 2.0 < 1e-12);
        // Opposite the arc: far from it, although on its circle.
        assert!(entity.distance_to(Point2::new(-10.0, 0.0)) > 14.0);
        assert!((arc.arc().sweep - FRAC_PI_2).abs() < 1e-12);

        // The geometry type and the entity agree in both directions.
        let back: ArcEntity = Arc::new(Point2::ORIGIN, 10.0, PI, FRAC_PI_2).into();
        assert!((back.start_angle - PI).abs() < 1e-12 && (back.end_angle - 1.5 * PI).abs() < 1e-12);
    }

    #[test]
    fn as_line_distinguishes_entity_kinds() {
        assert!(line().as_line().is_some());
        assert!(
            Entity::from(CircleEntity::new(Point2::ORIGIN, 1.0))
                .as_line()
                .is_none()
        );
    }
}

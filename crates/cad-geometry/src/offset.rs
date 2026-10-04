//! Offsets of the simple primitives (DESIGN §22): a line to a parallel line,
//! a circle to a concentric circle, an arc to a concentric arc. Which way is
//! decided by a point on the wanted side.
//!
//! Offsetting polylines — where pieces can vanish or need joining — is
//! dedicated robust-geometry work and not here.

use cad_core::tolerance::LinearTolerance;
use cad_core::{Point2, Vec2};

use crate::{Arc, Circle, Orientation, Segment, orientation};

/// The parallel to `segment`, `distance` away on the side `side` is on.
/// `None` when `side` is on the line itself — there is no side to choose —
/// or the segment has no direction.
pub fn offset_segment(segment: &Segment, distance: f64, side: Point2) -> Option<Segment> {
    let along = segment.end - segment.start;
    let length = along.length();
    if !(length > 0.0 && distance.is_finite()) {
        return None;
    }
    // The left normal; the exact orientation test says whether to use it.
    let left = Vec2::new(-along.y, along.x) * (distance / length);
    let shift = match orientation(segment.start, segment.end, side) {
        Orientation::Left => left,
        Orientation::Right => -left,
        Orientation::Collinear => return None,
    };
    Some(Segment::new(segment.start + shift, segment.end + shift))
}

/// The concentric circle `distance` away: larger when `side` is outside the
/// circle, smaller when inside. `None` when `side` is on the circle, or when
/// offsetting inwards would leave no circle.
pub fn offset_circle(
    circle: &Circle,
    distance: f64,
    side: Point2,
    tolerance: LinearTolerance,
) -> Option<Circle> {
    let from_center = circle.center.distance_to(side);
    if tolerance.eq(from_center, circle.radius) || !distance.is_finite() {
        return None;
    }
    let radius = if from_center > circle.radius {
        circle.radius + distance
    } else {
        circle.radius - distance
    };
    (radius > 0.0 && !tolerance.is_zero(radius)).then(|| Circle::new(circle.center, radius))
}

/// The concentric arc `distance` away, with the same angles: larger when
/// `side` is outside the arc's circle, smaller when inside. `None` when
/// `side` is on the circle, or when offsetting inwards would leave no arc.
pub fn offset_arc(
    arc: &Arc,
    distance: f64,
    side: Point2,
    tolerance: LinearTolerance,
) -> Option<Arc> {
    offset_circle(&arc.circle(), distance, side, tolerance)
        .map(|circle| Arc::new(arc.center, circle.radius, arc.start, arc.sweep))
}

/// How far `point` is from the line through `segment`, carried on past its
/// ends: the distance at which a parallel passes through the point.
pub fn distance_to_line(segment: &Segment, point: Point2) -> Option<f64> {
    let along = segment.end - segment.start;
    let length = along.length();
    let offset = point - segment.start;
    (length > 0.0).then(|| (along.x * offset.y - along.y * offset.x).abs() / length)
}

#[cfg(test)]
mod tests {
    use super::*;

    const TOLERANCE: LinearTolerance = LinearTolerance::DEFAULT;

    fn p(x: f64, y: f64) -> Point2 {
        Point2::new(x, y)
    }

    #[test]
    fn a_segment_offsets_to_the_side_of_the_point() {
        let segment = Segment::new(p(0.0, 0.0), p(10.0, 0.0));
        assert_eq!(
            offset_segment(&segment, 3.0, p(5.0, 100.0)),
            Some(Segment::new(p(0.0, 3.0), p(10.0, 3.0)))
        );
        assert_eq!(
            offset_segment(&segment, 3.0, p(-50.0, -0.001)),
            Some(Segment::new(p(0.0, -3.0), p(10.0, -3.0))),
            "the side counts, not where along the line the point is"
        );
    }

    #[test]
    fn the_offset_of_a_slanted_segment_is_parallel_and_at_the_distance() {
        let segment = Segment::new(p(1.0, 1.0), p(4.0, 5.0)); // length 5
        let parallel = offset_segment(&segment, 5.0, p(0.0, 10.0)).unwrap();
        // The left normal of (3,4) is (-4,3).
        assert!(parallel.start.distance_to(p(-3.0, 4.0)) < 1e-12);
        assert!(parallel.end.distance_to(p(0.0, 8.0)) < 1e-12);
        assert!((parallel.length() - segment.length()).abs() < 1e-12);
    }

    #[test]
    fn a_point_on_the_line_chooses_no_side() {
        let segment = Segment::new(p(0.0, 0.0), p(10.0, 10.0));
        assert_eq!(offset_segment(&segment, 1.0, p(5.0, 5.0)), None);
        assert_eq!(
            offset_segment(&segment, 1.0, p(20.0, 20.0)),
            None,
            "beyond the end, still on the line"
        );
        let dot = Segment::new(p(1.0, 1.0), p(1.0, 1.0));
        assert_eq!(offset_segment(&dot, 1.0, p(5.0, 5.0)), None);
    }

    #[test]
    fn a_circle_offsets_outwards_or_inwards_by_where_the_point_is() {
        let circle = Circle::new(p(10.0, 10.0), 5.0);
        assert_eq!(
            offset_circle(&circle, 2.0, p(30.0, 10.0), TOLERANCE),
            Some(Circle::new(p(10.0, 10.0), 7.0))
        );
        assert_eq!(
            offset_circle(&circle, 2.0, p(11.0, 10.0), TOLERANCE),
            Some(Circle::new(p(10.0, 10.0), 3.0))
        );
        assert_eq!(
            offset_circle(&circle, 2.0, p(10.0, 10.0), TOLERANCE),
            Some(Circle::new(p(10.0, 10.0), 3.0)),
            "the centre is inside"
        );
    }

    #[test]
    fn an_inward_offset_that_leaves_no_circle_is_refused() {
        let circle = Circle::new(p(0.0, 0.0), 5.0);
        assert_eq!(
            offset_circle(&circle, 5.0, p(1.0, 0.0), TOLERANCE),
            None,
            "radius zero"
        );
        assert_eq!(
            offset_circle(&circle, 8.0, p(1.0, 0.0), TOLERANCE),
            None,
            "radius negative"
        );
        assert!(
            offset_circle(&circle, 8.0, p(9.0, 0.0), TOLERANCE).is_some(),
            "outwards is fine"
        );
    }

    #[test]
    fn a_point_on_the_circle_chooses_no_side() {
        let circle = Circle::new(p(0.0, 0.0), 5.0);
        assert_eq!(offset_circle(&circle, 1.0, p(3.0, 4.0), TOLERANCE), None);
    }

    #[test]
    fn an_arc_offsets_to_a_concentric_arc_with_the_same_angles() {
        let arc = Arc::new(p(10.0, 10.0), 5.0, 0.25, 1.5);
        assert_eq!(
            offset_arc(&arc, 2.0, p(30.0, 30.0), TOLERANCE),
            Some(Arc::new(p(10.0, 10.0), 7.0, 0.25, 1.5))
        );
        assert_eq!(
            offset_arc(&arc, 2.0, p(10.5, 10.5), TOLERANCE),
            Some(Arc::new(p(10.0, 10.0), 3.0, 0.25, 1.5))
        );
        // The side is judged against the whole circle, not just the arc.
        assert_eq!(
            offset_arc(&arc, 2.0, p(10.0, -30.0), TOLERANCE),
            Some(Arc::new(p(10.0, 10.0), 7.0, 0.25, 1.5))
        );
    }

    #[test]
    fn an_arc_cannot_be_offset_inwards_to_nothing_or_from_a_point_on_its_circle() {
        let arc = Arc::new(p(0.0, 0.0), 5.0, 0.0, 1.0);
        assert_eq!(offset_arc(&arc, 5.0, p(1.0, 0.0), TOLERANCE), None);
        assert_eq!(offset_arc(&arc, 9.0, p(1.0, 0.0), TOLERANCE), None);
        assert_eq!(
            offset_arc(&arc, 1.0, p(0.0, -5.0), TOLERANCE),
            None,
            "on the circle"
        );
    }

    #[test]
    fn the_distance_to_a_line_ignores_where_the_segment_ends() {
        let segment = Segment::new(p(0.0, 0.0), p(10.0, 0.0));
        assert_eq!(distance_to_line(&segment, p(5.0, 3.0)), Some(3.0));
        assert_eq!(distance_to_line(&segment, p(500.0, -3.0)), Some(3.0));
        assert_eq!(distance_to_line(&segment, p(7.0, 0.0)), Some(0.0));
        assert_eq!(
            distance_to_line(&Segment::new(p(1.0, 1.0), p(1.0, 1.0)), p(5.0, 5.0)),
            None
        );
    }
}

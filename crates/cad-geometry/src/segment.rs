//! Straight line segments.

use cad_core::tolerance::LinearTolerance;
use cad_core::{Aabb, Point2};

/// A straight segment between two points. May be degenerate (zero length).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Segment {
    pub start: Point2,
    pub end: Point2,
}

impl Segment {
    pub const fn new(start: Point2, end: Point2) -> Self {
        Self { start, end }
    }

    pub fn bounds(&self) -> Aabb {
        Aabb::from_points(self.start, self.end)
    }

    pub fn length(&self) -> f64 {
        self.start.distance_to(self.end)
    }

    /// True when the endpoints coincide within the geometric linear tolerance.
    pub fn is_degenerate(&self, tolerance: LinearTolerance) -> bool {
        tolerance.coincident(self.start, self.end)
    }

    /// The point on the segment nearest to `point`.
    pub fn closest_point(&self, point: Point2) -> Point2 {
        let direction = self.end - self.start;
        let length_squared = direction.length_squared();
        if length_squared > 0.0 {
            let t = ((point - self.start).dot(direction) / length_squared).clamp(0.0, 1.0);
            self.start + direction * t
        } else {
            // Exactly zero length: every parameter maps to the same point.
            self.start
        }
    }

    pub fn distance_to_point(&self, point: Point2) -> f64 {
        point.distance_to(self.closest_point(point))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn seg(x0: f64, y0: f64, x1: f64, y1: f64) -> Segment {
        Segment::new(Point2::new(x0, y0), Point2::new(x1, y1))
    }

    #[test]
    fn bounds_cover_both_endpoints_in_any_order() {
        let b = seg(10.0, -5.0, -2.0, 7.0).bounds();
        assert_eq!(b.min(), Point2::new(-2.0, -5.0));
        assert_eq!(b.max(), Point2::new(10.0, 7.0));
    }

    #[test]
    fn distance_to_interior_is_perpendicular() {
        let s = seg(0.0, 0.0, 10.0, 0.0);
        assert_eq!(s.distance_to_point(Point2::new(4.0, 3.0)), 3.0);
        assert_eq!(
            s.closest_point(Point2::new(4.0, 3.0)),
            Point2::new(4.0, 0.0)
        );
    }

    #[test]
    fn distance_beyond_an_endpoint_is_to_that_endpoint() {
        let s = seg(0.0, 0.0, 10.0, 0.0);
        assert_eq!(s.distance_to_point(Point2::new(13.0, 4.0)), 5.0);
        assert_eq!(s.distance_to_point(Point2::new(-3.0, -4.0)), 5.0);
    }

    #[test]
    fn zero_length_segment_behaves_like_a_point() {
        let s = seg(2.0, 2.0, 2.0, 2.0);
        assert_eq!(s.distance_to_point(Point2::new(5.0, 6.0)), 5.0);
        assert_eq!(
            s.closest_point(Point2::new(5.0, 6.0)),
            Point2::new(2.0, 2.0)
        );
        assert!(s.is_degenerate(LinearTolerance::DEFAULT));
    }

    #[test]
    fn degeneracy_uses_the_linear_tolerance() {
        let nearly = seg(0.0, 0.0, 1e-12, 0.0);
        assert!(nearly.is_degenerate(LinearTolerance::DEFAULT));
        assert!(!seg(0.0, 0.0, 1e-6, 0.0).is_degenerate(LinearTolerance::DEFAULT));
    }

    #[test]
    fn length_is_distance_between_endpoints() {
        assert_eq!(seg(0.0, 0.0, 3.0, 4.0).length(), 5.0);
    }

    fn coordinate() -> impl Strategy<Value = f64> {
        -1.0e4..1.0e4_f64
    }

    fn point() -> impl Strategy<Value = Point2> {
        (coordinate(), coordinate()).prop_map(|(x, y)| Point2::new(x, y))
    }

    proptest! {
        #[test]
        fn distance_is_never_negative(a in point(), b in point(), p in point()) {
            prop_assert!(Segment::new(a, b).distance_to_point(p) >= 0.0);
        }

        #[test]
        fn distance_is_symmetric_in_the_endpoints(a in point(), b in point(), p in point()) {
            let forward = Segment::new(a, b).distance_to_point(p);
            let backward = Segment::new(b, a).distance_to_point(p);
            prop_assert!((forward - backward).abs() <= 1e-7, "{forward} vs {backward}");
        }

        #[test]
        fn points_on_the_segment_have_zero_distance(a in point(), b in point(), t in 0.0..=1.0_f64) {
            let on_segment = a + (b - a) * t;
            prop_assert!(Segment::new(a, b).distance_to_point(on_segment) <= 1e-7);
        }

        #[test]
        fn distance_never_exceeds_distance_to_either_endpoint(a in point(), b in point(), p in point()) {
            let d = Segment::new(a, b).distance_to_point(p);
            prop_assert!(d <= p.distance_to(a) + 1e-9);
            prop_assert!(d <= p.distance_to(b) + 1e-9);
        }

        #[test]
        fn closest_point_lies_within_the_bounds(a in point(), b in point(), p in point()) {
            let s = Segment::new(a, b);
            prop_assert!(s.bounds().expanded(1e-7).contains_point(s.closest_point(p)));
        }
    }
}

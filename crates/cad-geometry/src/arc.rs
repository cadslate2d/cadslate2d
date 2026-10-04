//! Circles and circular arcs.
//!
//! Angles are radians, measured counter-clockwise from +X. An [`Arc`] is
//! stored as a start angle and a positive sweep, which has one representation
//! per arc and no wrap-around cases to get wrong.

use std::f64::consts::{FRAC_PI_2, TAU};

use cad_core::tolerance::LinearTolerance;
use cad_core::{Aabb, Point2, Vec2};

/// `angle` wrapped into `[0, 2π)`.
pub fn normalize_angle(angle: f64) -> f64 {
    let wrapped = angle.rem_euclid(TAU);
    // rem_euclid can return exactly TAU for tiny negative inputs.
    if wrapped >= TAU { 0.0 } else { wrapped }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Circle {
    pub center: Point2,
    pub radius: f64,
}

impl Circle {
    pub const fn new(center: Point2, radius: f64) -> Self {
        Self { center, radius }
    }

    pub fn point_at(&self, angle: f64) -> Point2 {
        self.center + Vec2::from_polar(self.radius, angle)
    }

    pub fn bounds(&self) -> Aabb {
        let reach = Vec2::new(self.radius, self.radius);
        Aabb::from_points(self.center - reach, self.center + reach)
    }

    /// Distance from `point` to the circle itself (not to the disc).
    pub fn distance_to_point(&self, point: Point2) -> f64 {
        (self.center.distance_to(point) - self.radius).abs()
    }

    /// The circle through three points, or `None` when they are collinear or
    /// coincide within the linear tolerance.
    pub fn through_points(
        a: Point2,
        b: Point2,
        c: Point2,
        tolerance: LinearTolerance,
    ) -> Option<Self> {
        let (ab, ac) = (b - a, c - a);
        let cross = ab.x * ac.y - ab.y * ac.x;
        let longest = ab.length().max(ac.length()).max((c - b).length());
        // Height of the triangle over its longest side: zero when collinear.
        if longest <= 0.0 || tolerance.is_zero(cross / longest) {
            return None;
        }
        let (ab2, ac2) = (ab.length_squared(), ac.length_squared());
        let offset = Vec2::new(ac.y * ab2 - ab.y * ac2, ab.x * ac2 - ac.x * ab2) * (0.5 / cross);
        let center = a + offset;
        Some(Self::new(center, center.distance_to(a)))
    }
}

/// A circular arc traversed counter-clockwise from `start` through `sweep`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Arc {
    pub center: Point2,
    pub radius: f64,
    /// Start angle in `[0, 2π)`.
    pub start: f64,
    /// Angle covered, in `(0, 2π]`.
    pub sweep: f64,
}

impl Arc {
    /// An arc from `start`, counter-clockwise through `sweep` radians. The
    /// start is normalised; the sweep is clamped to one full turn.
    pub fn new(center: Point2, radius: f64, start: f64, sweep: f64) -> Self {
        Self {
            center,
            radius,
            start: normalize_angle(start),
            sweep: sweep.clamp(0.0, TAU),
        }
    }

    /// An arc counter-clockwise from `start` to `end`. Equal angles give a
    /// full turn.
    pub fn from_angles(center: Point2, radius: f64, start: f64, end: f64) -> Self {
        let sweep = normalize_angle(end - start);
        Self::new(center, radius, start, if sweep > 0.0 { sweep } else { TAU })
    }

    /// The arc from `start` through `through` to `end`, or `None` when the
    /// points are collinear. The result is always stored counter-clockwise,
    /// so its start may be the given `end`.
    pub fn through_points(
        start: Point2,
        through: Point2,
        end: Point2,
        tolerance: LinearTolerance,
    ) -> Option<Self> {
        let circle = Circle::through_points(start, through, end, tolerance)?;
        let angle = |point: Point2| {
            let offset = point - circle.center;
            offset.y.atan2(offset.x)
        };
        let forwards = Self::from_angles(circle.center, circle.radius, angle(start), angle(end));
        if forwards.contains_angle(angle(through)) {
            Some(forwards)
        } else {
            Some(Self::from_angles(
                circle.center,
                circle.radius,
                angle(end),
                angle(start),
            ))
        }
    }

    /// End angle in `[0, 2π)`.
    pub fn end(&self) -> f64 {
        normalize_angle(self.start + self.sweep)
    }

    pub fn point_at(&self, angle: f64) -> Point2 {
        self.center + Vec2::from_polar(self.radius, angle)
    }

    pub fn start_point(&self) -> Point2 {
        self.point_at(self.start)
    }

    pub fn end_point(&self) -> Point2 {
        self.point_at(self.start + self.sweep)
    }

    pub fn mid_point(&self) -> Point2 {
        self.point_at(self.start + self.sweep / 2.0)
    }

    /// True when the direction `angle` falls within the arc's sweep.
    pub fn contains_angle(&self, angle: f64) -> bool {
        normalize_angle(angle - self.start) <= self.sweep
    }

    /// The exact bounding box: the endpoints plus every axis extreme the arc passes.
    pub fn bounds(&self) -> Aabb {
        let ends = Aabb::from_points(self.start_point(), self.end_point());
        (0..4)
            .map(|quadrant| f64::from(quadrant) * FRAC_PI_2)
            .filter(|angle| self.contains_angle(*angle))
            .map(|angle| self.point_at(angle))
            .fold(ends, |bounds, point| {
                bounds.union(&Aabb::from_points(point, point))
            })
    }

    /// Distance from `point` to the arc: radial where the point faces the arc,
    /// otherwise to the nearer endpoint.
    pub fn distance_to_point(&self, point: Point2) -> f64 {
        let offset = point - self.center;
        if self.contains_angle(offset.y.atan2(offset.x)) {
            (offset.length() - self.radius).abs()
        } else {
            point
                .distance_to(self.start_point())
                .min(point.distance_to(self.end_point()))
        }
    }
}

#[cfg(test)]
mod tests {
    use std::f64::consts::PI;

    use proptest::prelude::*;

    use super::*;

    const TOLERANCE: LinearTolerance = LinearTolerance::DEFAULT;

    fn p(x: f64, y: f64) -> Point2 {
        Point2::new(x, y)
    }

    fn close(a: Point2, b: Point2) -> bool {
        a.distance_to(b) < 1e-9
    }

    #[test]
    fn angles_normalise_into_one_turn() {
        assert_eq!(normalize_angle(0.0), 0.0);
        assert!((normalize_angle(-FRAC_PI_2) - 1.5 * PI).abs() < 1e-12);
        assert!((normalize_angle(5.0 * PI) - PI).abs() < 1e-12);
        assert!(normalize_angle(-1e-20) < TAU);
        assert_eq!(normalize_angle(TAU), 0.0);
    }

    #[test]
    fn circle_bounds_and_distance() {
        let circle = Circle::new(p(10.0, 20.0), 5.0);
        assert_eq!(
            circle.bounds(),
            Aabb::from_points(p(5.0, 15.0), p(15.0, 25.0))
        );
        assert_eq!(
            circle.distance_to_point(p(10.0, 20.0)),
            5.0,
            "the centre is a radius away from the curve"
        );
        assert_eq!(circle.distance_to_point(p(18.0, 20.0)), 3.0);
        assert_eq!(circle.distance_to_point(p(10.0, 23.0)), 2.0);
        assert!(close(circle.point_at(FRAC_PI_2), p(10.0, 25.0)));
    }

    #[test]
    fn circle_through_three_points() {
        let circle =
            Circle::through_points(p(1.0, 0.0), p(0.0, 1.0), p(-1.0, 0.0), TOLERANCE).unwrap();
        assert!(close(circle.center, Point2::ORIGIN));
        assert!((circle.radius - 1.0).abs() < 1e-12);
    }

    #[test]
    fn collinear_or_coincident_points_define_no_circle() {
        assert_eq!(
            Circle::through_points(p(0.0, 0.0), p(1.0, 1.0), p(2.0, 2.0), TOLERANCE),
            None
        );
        assert_eq!(
            Circle::through_points(p(0.0, 0.0), p(0.0, 0.0), p(2.0, 2.0), TOLERANCE),
            None
        );
        assert_eq!(
            Circle::through_points(p(3.0, 3.0), p(3.0, 3.0), p(3.0, 3.0), TOLERANCE),
            None
        );
        assert_eq!(
            Arc::through_points(p(0.0, 0.0), p(5.0, 0.0), p(9.0, 0.0), TOLERANCE),
            None
        );
    }

    #[test]
    fn an_arc_knows_its_endpoints_and_midpoint() {
        let arc = Arc::new(Point2::ORIGIN, 2.0, 0.0, PI);
        assert!(close(arc.start_point(), p(2.0, 0.0)));
        assert!(close(arc.mid_point(), p(0.0, 2.0)));
        assert!(close(arc.end_point(), p(-2.0, 0.0)));
        assert!((arc.end() - PI).abs() < 1e-12);
    }

    #[test]
    fn from_angles_always_runs_counter_clockwise() {
        // From 270° to 90° passes through 0°, not 180°.
        let arc = Arc::from_angles(Point2::ORIGIN, 1.0, 1.5 * PI, FRAC_PI_2);
        assert!((arc.sweep - PI).abs() < 1e-12);
        assert!(arc.contains_angle(0.0));
        assert!(!arc.contains_angle(PI));
        // Equal angles are a full turn.
        assert_eq!(Arc::from_angles(Point2::ORIGIN, 1.0, 1.0, 1.0).sweep, TAU);
    }

    #[test]
    fn three_point_arcs_pass_through_the_middle_point_in_either_direction() {
        // Counter-clockwise: east, north, west.
        let ccw = Arc::through_points(p(1.0, 0.0), p(0.0, 1.0), p(-1.0, 0.0), TOLERANCE).unwrap();
        assert!((ccw.start - 0.0).abs() < 1e-12 && (ccw.sweep - PI).abs() < 1e-12);

        // The same ends through south: stored counter-clockwise from west to east.
        let cw = Arc::through_points(p(1.0, 0.0), p(0.0, -1.0), p(-1.0, 0.0), TOLERANCE).unwrap();
        assert!((cw.start - PI).abs() < 1e-12 && (cw.sweep - PI).abs() < 1e-12);
        assert!(cw.contains_angle(1.5 * PI) && !cw.contains_angle(FRAC_PI_2));
    }

    #[test]
    fn arc_bounds_include_the_axis_extremes_it_passes() {
        // A quarter arc in the first quadrant: just its endpoints.
        let quarter = Arc::new(Point2::ORIGIN, 10.0, 0.0, FRAC_PI_2);
        let bounds = quarter.bounds();
        assert!(close(bounds.min(), p(0.0, 0.0)) && close(bounds.max(), p(10.0, 10.0)));

        // From 45° to 135° the arc bulges up to y = 10 at 90°.
        let top = Arc::new(Point2::ORIGIN, 10.0, PI / 4.0, FRAC_PI_2);
        let half = 10.0 * std::f64::consts::FRAC_1_SQRT_2;
        assert!(close(top.bounds().min(), p(-half, half)));
        assert!(close(top.bounds().max(), p(half, 10.0)));

        // An arc crossing 0°.
        let east = Arc::from_angles(Point2::ORIGIN, 10.0, -PI / 4.0, PI / 4.0);
        assert!((east.bounds().max().x - 10.0).abs() < 1e-12);
    }

    #[test]
    fn arc_distance_is_radial_inside_the_sweep_and_to_an_endpoint_outside_it() {
        let arc = Arc::new(Point2::ORIGIN, 10.0, 0.0, FRAC_PI_2);
        assert!((arc.distance_to_point(p(5.0, 5.0)) - (10.0 - 50f64.sqrt())).abs() < 1e-12);
        assert!((arc.distance_to_point(p(12.0, 12.0)) - (288f64.sqrt() - 10.0)).abs() < 1e-12);
        // Below the start point, outside the sweep: distance to (10, 0).
        assert!((arc.distance_to_point(p(10.0, -3.0)) - 3.0).abs() < 1e-12);
        // Opposite the arc entirely.
        assert!((arc.distance_to_point(p(-10.0, 0.0)) - 200f64.sqrt()).abs() < 1e-12);
    }

    fn coordinate() -> impl Strategy<Value = f64> {
        -1.0e3..1.0e3_f64
    }

    fn any_arc() -> impl Strategy<Value = Arc> {
        (
            coordinate(),
            coordinate(),
            0.1..500.0_f64,
            0.0..TAU,
            0.01..TAU,
        )
            .prop_map(|(x, y, radius, start, sweep)| Arc::new(p(x, y), radius, start, sweep))
    }

    proptest! {
        #[test]
        fn arc_bounds_contain_every_point_on_the_arc(arc in any_arc(), t in 0.0..=1.0_f64) {
            let on_arc = arc.point_at(arc.start + arc.sweep * t);
            prop_assert!(arc.bounds().expanded(1e-7).contains_point(on_arc));
        }

        #[test]
        fn points_on_the_arc_are_at_distance_zero(arc in any_arc(), t in 0.0..=1.0_f64) {
            let on_arc = arc.point_at(arc.start + arc.sweep * t);
            prop_assert!(arc.distance_to_point(on_arc) < 1e-7);
        }

        #[test]
        fn arc_distance_is_never_less_than_distance_to_the_full_circle(
            arc in any_arc(), x in coordinate(), y in coordinate()
        ) {
            let point = p(x, y);
            let circle = Circle::new(arc.center, arc.radius);
            prop_assert!(arc.distance_to_point(point) >= circle.distance_to_point(point) - 1e-9);
        }

        #[test]
        fn circle_bounds_contain_every_point_on_the_circle(
            x in coordinate(), y in coordinate(), radius in 0.1..500.0_f64, angle in 0.0..TAU
        ) {
            let circle = Circle::new(p(x, y), radius);
            prop_assert!(circle.bounds().expanded(1e-7).contains_point(circle.point_at(angle)));
        }

        #[test]
        fn a_three_point_circle_passes_through_its_points(
            ax in coordinate(), ay in coordinate(), bx in coordinate(), by in coordinate(),
            cx in coordinate(), cy in coordinate()
        ) {
            let (a, b, c) = (p(ax, ay), p(bx, by), p(cx, cy));
            if let Some(circle) = Circle::through_points(a, b, c, LinearTolerance::new(1e-3)) {
                // Nearly collinear points give huge circles; scale the check to the radius.
                let slack = 1e-9 * circle.radius.max(1.0) * circle.radius.max(1.0);
                for point in [a, b, c] {
                    prop_assert!(circle.distance_to_point(point) <= slack, "{} > {slack}", circle.distance_to_point(point));
                }
            }
        }

        #[test]
        fn a_three_point_arc_runs_between_its_ends_through_the_middle(
            start in 0.0..TAU, first in 0.05..3.0_f64, second in 0.05..3.0_f64, clockwise in any::<bool>()
        ) {
            let direction = if clockwise { -1.0 } else { 1.0 };
            let unit = Circle::new(p(3.0, -2.0), 7.0);
            let (a, b, c) = (
                unit.point_at(start),
                unit.point_at(start + direction * first),
                unit.point_at(start + direction * (first + second)),
            );
            let arc = Arc::through_points(a, b, c, TOLERANCE).unwrap();
            prop_assert!((arc.sweep - (first + second)).abs() < 1e-6, "sweep {}", arc.sweep);
            prop_assert!(arc.distance_to_point(b) < 1e-7);
            let ends = [arc.start_point(), arc.end_point()];
            prop_assert!(ends.iter().any(|end| end.distance_to(a) < 1e-6));
            prop_assert!(ends.iter().any(|end| end.distance_to(c) < 1e-6));
        }
    }
}

//! Constructions from a point to a curve (DESIGN §18): the foot of the
//! perpendicular, and the points of tangency. These are what the
//! perpendicular and tangent object snaps offer.
//!
//! Both are exact constructions, not searches. Where none exists — the point
//! is the circle's centre, or inside the circle, or already on the curve —
//! there are no results, rather than a nearby guess.

use cad_core::Point2;
use cad_core::tolerance::LinearTolerance;

use crate::intersect::Hits;
use crate::{Circle, Primitive, Segment};

const LINEAR: LinearTolerance = LinearTolerance::DEFAULT;

impl Segment {
    /// Where the perpendicular dropped from `from` meets the segment.
    ///
    /// `None` when the perpendicular misses the segment — it would meet the
    /// line beyond one of the ends — or when `from` is already on the
    /// segment, so there is no perpendicular to drop.
    pub fn perpendicular_foot(&self, from: Point2) -> Option<Point2> {
        let along = self.end - self.start;
        let length_squared = along.length_squared();
        if length_squared <= 0.0 {
            return None;
        }
        let parameter = (from - self.start).dot(along) / length_squared;
        let foot = self.start + along * parameter;
        let on_segment = (0.0..=1.0).contains(&parameter)
            || LINEAR.coincident(foot, self.start)
            || LINEAR.coincident(foot, self.end);
        (on_segment && !LINEAR.coincident(foot, from)).then_some(foot)
    }
}

impl Circle {
    /// Where the line from `from` through the centre meets the circle: the
    /// nearest and the farthest point of the circle, both of which a line
    /// from `from` meets at a right angle.
    ///
    /// None from the centre itself, where every direction qualifies; and a
    /// point on the circle is not its own foot.
    pub fn perpendicular_feet(&self, from: Point2) -> Hits {
        let outward = from - self.center;
        let distance = outward.length();
        if LINEAR.is_zero(distance) {
            return Hits::NONE;
        }
        let radial = outward * (self.radius / distance);
        Hits::pair(self.center + radial, self.center - radial)
            .filtered(|foot| !LINEAR.coincident(foot, from))
    }

    /// The two points where a line from `from` touches the circle.
    ///
    /// None when `from` is inside the circle or on it.
    pub fn tangent_points(&self, from: Point2) -> Hits {
        let outward = from - self.center;
        let distance = outward.length();
        if distance <= self.radius || LINEAR.eq(distance, self.radius) {
            return Hits::NONE;
        }
        // The radius to a tangent point makes the angle acos(r/d) with the
        // line to `from`: cos = r/d, sin = √(d² − r²)/d.
        let cosine = self.radius / distance;
        let sine = (1.0 - cosine * cosine).max(0.0).sqrt();
        let unit = outward * (1.0 / distance);
        let turned = |sine: f64| {
            cad_core::Vec2::new(
                unit.x * cosine - unit.y * sine,
                unit.x * sine + unit.y * cosine,
            ) * self.radius
        };
        Hits::pair(self.center + turned(sine), self.center + turned(-sine))
    }
}

impl Primitive {
    /// The feet of perpendiculars from `from` that land on the primitive.
    pub fn perpendicular_feet(&self, from: Point2) -> Hits {
        match self {
            Primitive::Segment(segment) => segment
                .perpendicular_foot(from)
                .map_or(Hits::NONE, Hits::one),
            Primitive::Circle(circle) => circle.perpendicular_feet(from),
            Primitive::Arc(arc) => arc
                .circle()
                .perpendicular_feet(from)
                .filtered(|foot| LINEAR.is_zero(arc.distance_to_point(foot))),
        }
    }

    /// The points of the primitive at which a line from `from` is tangent.
    /// Straight segments have none.
    pub fn tangent_points(&self, from: Point2) -> Hits {
        match self {
            Primitive::Segment(_) => Hits::NONE,
            Primitive::Circle(circle) => circle.tangent_points(from),
            Primitive::Arc(arc) => arc
                .circle()
                .tangent_points(from)
                .filtered(|point| LINEAR.is_zero(arc.distance_to_point(point))),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::f64::consts::{FRAC_PI_2, PI};

    use proptest::prelude::*;

    use super::*;
    use crate::Arc;

    fn p(x: f64, y: f64) -> Point2 {
        Point2::new(x, y)
    }

    fn close(a: Point2, b: Point2) -> bool {
        a.distance_to(b) < 1e-9 * (1.0 + a.distance_to(Point2::ORIGIN))
    }

    #[test]
    fn the_foot_on_a_segment_is_the_projection() {
        let segment = Segment::new(p(0.0, 0.0), p(10.0, 0.0));
        assert_eq!(segment.perpendicular_foot(p(4.0, 7.0)), Some(p(4.0, 0.0)));
        assert_eq!(segment.perpendicular_foot(p(4.0, -0.5)), Some(p(4.0, 0.0)));
        let slanted = Segment::new(p(0.0, 0.0), p(10.0, 10.0));
        assert!(close(
            slanted.perpendicular_foot(p(10.0, 0.0)).unwrap(),
            p(5.0, 5.0)
        ));
    }

    #[test]
    fn a_perpendicular_that_misses_the_segment_has_no_foot() {
        let segment = Segment::new(p(0.0, 0.0), p(10.0, 0.0));
        assert_eq!(
            segment.perpendicular_foot(p(15.0, 3.0)),
            None,
            "beyond the end"
        );
        assert_eq!(
            segment.perpendicular_foot(p(-0.1, 3.0)),
            None,
            "before the start"
        );
        // Exactly above an end is still on the segment.
        assert_eq!(segment.perpendicular_foot(p(10.0, 3.0)), Some(p(10.0, 0.0)));
        assert_eq!(segment.perpendicular_foot(p(0.0, -3.0)), Some(p(0.0, 0.0)));
    }

    /// Regression: a base point on the entity has no perpendicular to drop.
    #[test]
    fn a_point_on_the_segment_is_not_its_own_foot() {
        let segment = Segment::new(p(0.0, 0.0), p(10.0, 0.0));
        assert_eq!(segment.perpendicular_foot(p(4.0, 0.0)), None);
        assert_eq!(segment.perpendicular_foot(p(0.0, 0.0)), None);
        let dot = Segment::new(p(1.0, 1.0), p(1.0, 1.0));
        assert_eq!(dot.perpendicular_foot(p(5.0, 5.0)), None);
    }

    #[test]
    fn a_circle_has_a_near_and_a_far_foot() {
        let circle = Circle::new(p(0.0, 0.0), 5.0);
        let feet = circle.perpendicular_feet(p(13.0, 0.0));
        assert_eq!(feet.as_slice(), &[p(5.0, 0.0), p(-5.0, 0.0)]);
        // From inside as well.
        let feet = circle.perpendicular_feet(p(0.0, 2.0));
        assert_eq!(feet.as_slice(), &[p(0.0, 5.0), p(0.0, -5.0)]);
    }

    /// Regression: the centre, and a point on the circle.
    #[test]
    fn the_centre_has_no_feet_and_a_point_on_the_circle_only_the_far_one() {
        let circle = Circle::new(p(0.0, 0.0), 5.0);
        assert!(circle.perpendicular_feet(p(0.0, 0.0)).is_empty());
        assert_eq!(
            circle.perpendicular_feet(p(5.0, 0.0)).as_slice(),
            &[p(-5.0, 0.0)]
        );
    }

    #[test]
    fn tangent_points_from_outside_a_circle() {
        // A 3-4-5 triangle: from (5, 0) to the circle of radius 3, the
        // tangent length is 4 and the points are at (9/5, ±12/5).
        let circle = Circle::new(p(0.0, 0.0), 3.0);
        let points = circle.tangent_points(p(5.0, 0.0));
        assert_eq!(points.len(), 2);
        assert!(close(points.as_slice()[0], p(1.8, 2.4)));
        assert!(close(points.as_slice()[1], p(1.8, -2.4)));
    }

    /// Regression: no tangent from inside a circle, or from a point on it.
    #[test]
    fn there_is_no_tangent_from_inside_or_on_the_circle() {
        let circle = Circle::new(p(0.0, 0.0), 3.0);
        assert!(circle.tangent_points(p(1.0, 1.0)).is_empty());
        assert!(circle.tangent_points(p(0.0, 0.0)).is_empty());
        assert!(circle.tangent_points(p(3.0, 0.0)).is_empty());
        assert!(
            circle.tangent_points(p(3.0 + 1e-12, 0.0)).is_empty(),
            "on it, within tolerance"
        );
    }

    #[test]
    fn an_arc_offers_only_the_feet_and_tangent_points_that_lie_on_it() {
        // The upper half of the circle of radius 5.
        let upper = Primitive::Arc(Arc::new(p(0.0, 0.0), 5.0, 0.0, PI));
        assert_eq!(
            upper.perpendicular_feet(p(0.0, 13.0)).as_slice(),
            &[p(0.0, 5.0)]
        );
        assert!(
            upper.perpendicular_feet(p(3.0, -13.0)).len() == 1,
            "only the upper one"
        );

        // Tangents from far to the right touch at top and bottom; only the top is on the arc.
        let points = upper.tangent_points(p(13.0, 0.0));
        assert_eq!(points.len(), 1);
        assert!(points.as_slice()[0].y > 0.0);
        // A quarter arc in the second quadrant has neither.
        let quarter = Primitive::Arc(Arc::new(p(0.0, 0.0), 5.0, FRAC_PI_2, FRAC_PI_2));
        assert!(quarter.tangent_points(p(13.0, 0.0)).is_empty());
    }

    #[test]
    fn a_segment_has_no_tangent_points() {
        let segment = Primitive::Segment(Segment::new(p(0.0, 0.0), p(10.0, 0.0)));
        assert!(segment.tangent_points(p(4.0, 7.0)).is_empty());
        assert_eq!(
            segment.perpendicular_feet(p(4.0, 7.0)).as_slice(),
            &[p(4.0, 0.0)]
        );
    }

    fn coordinate() -> impl Strategy<Value = f64> {
        -1e3..1e3f64
    }

    proptest! {
        /// The foot really is perpendicular, and no point of the segment is nearer.
        #[test]
        fn the_foot_is_perpendicular_and_nearest(
            ax in coordinate(), ay in coordinate(), bx in coordinate(), by in coordinate(),
            fx in coordinate(), fy in coordinate(),
        ) {
            let segment = Segment::new(p(ax, ay), p(bx, by));
            prop_assume!(segment.length() > 1e-3);
            let from = p(fx, fy);
            if let Some(foot) = segment.perpendicular_foot(from) {
                let along = segment.end - segment.start;
                let drop = foot - from;
                prop_assert!(drop.dot(along).abs() < 1e-6 * along.length() * (1.0 + drop.length()));
                prop_assert!((segment.distance_to_point(from) - drop.length()).abs() < 1e-6);
            }
        }

        /// At a tangent point the radius is perpendicular to the line from
        /// the base point, and the point is on the circle.
        #[test]
        fn tangent_lines_meet_the_radius_at_a_right_angle(
            cx in coordinate(), cy in coordinate(), radius in 0.1..500f64,
            fx in coordinate(), fy in coordinate(),
        ) {
            let circle = Circle::new(p(cx, cy), radius);
            let from = p(fx, fy);
            let points = circle.tangent_points(from);
            let outside = from.distance_to(circle.center) > radius * (1.0 + 1e-6);
            prop_assert_eq!(points.len() == 2, outside);
            for point in points.as_slice() {
                let radial = *point - circle.center;
                let line = *point - from;
                prop_assert!((radial.length() - radius).abs() < 1e-7 * (1.0 + radius));
                prop_assert!(radial.dot(line).abs() < 1e-6 * radius * (1.0 + line.length()));
            }
        }

        /// The feet on a circle are on the circle and in line with the centre.
        #[test]
        fn circle_feet_are_collinear_with_the_centre(
            cx in coordinate(), cy in coordinate(), radius in 0.1..500f64,
            fx in coordinate(), fy in coordinate(),
        ) {
            let circle = Circle::new(p(cx, cy), radius);
            let from = p(fx, fy);
            prop_assume!(from.distance_to(circle.center) > 1e-3);
            for foot in circle.perpendicular_feet(from).as_slice() {
                let radial = *foot - circle.center;
                let line = from - circle.center;
                prop_assert!((radial.length() - radius).abs() < 1e-7 * (1.0 + radius));
                let cross = radial.x * line.y - radial.y * line.x;
                prop_assert!(cross.abs() < 1e-6 * radius * line.length());
            }
        }
    }
}

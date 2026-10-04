//! Does a curve touch an axis-aligned box?
//!
//! These answer crossing selection (DESIGN §16.1): an entity is selected when
//! any part of its *curve* lies in the box. Interiors do not count — a box
//! drawn entirely inside a circle does not touch the circle.

use cad_core::{Aabb, Point2};

use crate::{Arc, Circle, Segment};

impl Segment {
    /// True when any point of the segment lies in `bounds` (edges included).
    pub fn intersects_aabb(&self, bounds: &Aabb) -> bool {
        // Liang–Barsky: clip the parameter range [0, 1] against each side.
        let delta = self.end - self.start;
        let (mut enter, mut leave) = (0.0_f64, 1.0_f64);
        let sides = [
            (-delta.x, self.start.x - bounds.min().x),
            (delta.x, bounds.max().x - self.start.x),
            (-delta.y, self.start.y - bounds.min().y),
            (delta.y, bounds.max().y - self.start.y),
        ];
        for (direction, distance) in sides {
            if direction == 0.0 {
                // Parallel to this side: outside it means outside the box.
                if distance < 0.0 {
                    return false;
                }
            } else {
                let t = distance / direction;
                if direction < 0.0 {
                    enter = enter.max(t);
                } else {
                    leave = leave.min(t);
                }
            }
        }
        enter <= leave
    }

    /// Parameters in `[0, 1]` where the segment meets `circle`.
    fn circle_crossings(&self, circle: &Circle) -> impl Iterator<Item = f64> {
        let delta = self.end - self.start;
        let from_center = self.start - circle.center;
        let a = delta.length_squared();
        let b = 2.0 * from_center.dot(delta);
        let c = from_center.length_squared() - circle.radius * circle.radius;
        let discriminant = b * b - 4.0 * a * c;
        let roots = if a > 0.0 && discriminant >= 0.0 {
            let root = discriminant.sqrt();
            [Some((-b - root) / (2.0 * a)), Some((-b + root) / (2.0 * a))]
        } else {
            [None, None]
        };
        roots
            .into_iter()
            .flatten()
            .filter(|t| (0.0..=1.0).contains(t))
    }

    fn point_at(&self, t: f64) -> Point2 {
        self.start + (self.end - self.start) * t
    }
}

impl Circle {
    /// True when the circle's curve touches `bounds`: the box reaches the
    /// curve from inside or outside, without lying wholly within the disc or
    /// wholly outside it.
    pub fn intersects_aabb(&self, bounds: &Aabb) -> bool {
        let nearest = Point2::new(
            self.center.x.clamp(bounds.min().x, bounds.max().x),
            self.center.y.clamp(bounds.min().y, bounds.max().y),
        );
        let farthest = bounds
            .corners()
            .into_iter()
            .map(|corner| self.center.distance_to(corner))
            .fold(0.0, f64::max);
        self.center.distance_to(nearest) <= self.radius && self.radius <= farthest
    }
}

impl Arc {
    /// True when any point of the arc lies in `bounds`.
    pub fn intersects_aabb(&self, bounds: &Aabb) -> bool {
        // Either an endpoint is inside, or the arc crosses the boundary.
        if bounds.contains_point(self.start_point()) || bounds.contains_point(self.end_point()) {
            return true;
        }
        let circle = Circle::new(self.center, self.radius);
        let corners = bounds.corners();
        (0..corners.len()).any(|index| {
            let edge = Segment::new(corners[index], corners[(index + 1) % corners.len()]);
            edge.circle_crossings(&circle).any(|t| {
                let offset = edge.point_at(t) - self.center;
                self.contains_angle(offset.y.atan2(offset.x))
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use std::f64::consts::{FRAC_PI_2, PI};

    use proptest::prelude::*;

    use super::*;

    fn p(x: f64, y: f64) -> Point2 {
        Point2::new(x, y)
    }

    fn boxed(x0: f64, y0: f64, x1: f64, y1: f64) -> Aabb {
        Aabb::from_points(p(x0, y0), p(x1, y1))
    }

    fn seg(x0: f64, y0: f64, x1: f64, y1: f64) -> Segment {
        Segment::new(p(x0, y0), p(x1, y1))
    }

    #[test]
    fn segments_inside_across_and_outside_a_box() {
        let b = boxed(0.0, 0.0, 10.0, 10.0);
        assert!(seg(2.0, 2.0, 8.0, 8.0).intersects_aabb(&b), "inside");
        assert!(
            seg(-5.0, 5.0, 15.0, 5.0).intersects_aabb(&b),
            "straight through"
        );
        assert!(
            seg(-5.0, 5.0, 5.0, 5.0).intersects_aabb(&b),
            "one end inside"
        );
        assert!(
            seg(-1.0, 9.0, 1.0, 11.0).intersects_aabb(&b),
            "clips a corner"
        );
        assert!(
            seg(10.0, 12.0, 10.0, 10.0).intersects_aabb(&b),
            "touches a corner"
        );
        assert!(
            !seg(-5.0, -5.0, -1.0, 20.0).intersects_aabb(&b),
            "passes to the left"
        );
        assert!(
            !seg(11.0, 0.0, 20.0, 9.0).intersects_aabb(&b),
            "its bounding box overlaps but it misses"
        );
        assert!(
            !seg(0.0, 11.0, 10.0, 11.0).intersects_aabb(&b),
            "parallel, above"
        );
    }

    #[test]
    fn a_zero_length_segment_behaves_like_a_point() {
        let b = boxed(0.0, 0.0, 10.0, 10.0);
        assert!(seg(5.0, 5.0, 5.0, 5.0).intersects_aabb(&b));
        assert!(!seg(15.0, 5.0, 15.0, 5.0).intersects_aabb(&b));
    }

    #[test]
    fn a_circle_is_its_curve_not_its_disc() {
        let circle = Circle::new(p(0.0, 0.0), 10.0);
        assert!(
            circle.intersects_aabb(&boxed(8.0, -1.0, 12.0, 1.0)),
            "straddles the curve"
        );
        assert!(
            circle.intersects_aabb(&boxed(-20.0, -20.0, 20.0, 20.0)),
            "contains the whole circle"
        );
        assert!(
            !circle.intersects_aabb(&boxed(-2.0, -2.0, 2.0, 2.0)),
            "wholly inside the disc"
        );
        assert!(
            !circle.intersects_aabb(&boxed(11.0, 11.0, 15.0, 15.0)),
            "wholly outside"
        );
        assert!(
            circle.intersects_aabb(&boxed(10.0, -1.0, 12.0, 1.0)),
            "touches the curve"
        );
    }

    #[test]
    fn an_arc_touches_a_box_only_along_its_sweep() {
        // The upper-right quarter of a circle of radius 10.
        let arc = Arc::new(p(0.0, 0.0), 10.0, 0.0, FRAC_PI_2);
        assert!(
            arc.intersects_aabb(&boxed(6.0, 6.0, 8.0, 8.0)),
            "crosses the arc"
        );
        assert!(
            arc.intersects_aabb(&boxed(9.0, -1.0, 11.0, 1.0)),
            "contains the start point"
        );
        assert!(
            arc.intersects_aabb(&boxed(-20.0, -20.0, 20.0, 20.0)),
            "contains the whole arc"
        );
        assert!(
            !arc.intersects_aabb(&boxed(-8.0, -8.0, -6.0, -6.0)),
            "crosses the circle, but opposite the arc"
        );
        assert!(
            !arc.intersects_aabb(&boxed(1.0, 1.0, 3.0, 3.0)),
            "inside the circle"
        );

        // An arc that enters and leaves through the same edge, endpoints outside.
        let cap = Arc::new(p(0.0, 0.0), 10.0, PI / 4.0, FRAC_PI_2);
        assert!(cap.intersects_aabb(&boxed(-3.0, 9.0, 3.0, 20.0)));
    }

    fn coordinate() -> impl Strategy<Value = f64> {
        -100.0..100.0_f64
    }

    fn any_box() -> impl Strategy<Value = Aabb> {
        (coordinate(), coordinate(), 0.1..80.0_f64, 0.1..80.0_f64)
            .prop_map(|(x, y, w, h)| boxed(x, y, x + w, y + h))
    }

    proptest! {
        /// Sampling the curve can only find points the exact test already accounts for.
        #[test]
        fn a_segment_with_a_sample_point_in_the_box_intersects_it(
            x0 in coordinate(), y0 in coordinate(), x1 in coordinate(), y1 in coordinate(), b in any_box()
        ) {
            let segment = seg(x0, y0, x1, y1);
            let sampled = (0..=64).any(|i| b.contains_point(segment.point_at(f64::from(i) / 64.0)));
            if sampled {
                prop_assert!(segment.intersects_aabb(&b));
            }
            // And a segment whose bounding box misses the box cannot intersect it.
            if !segment.bounds().intersects(&b) {
                prop_assert!(!segment.intersects_aabb(&b));
            }
        }

        #[test]
        fn an_arc_with_a_sample_point_in_the_box_intersects_it(
            cx in coordinate(), cy in coordinate(), radius in 0.5..60.0_f64,
            start in 0.0..std::f64::consts::TAU, sweep in 0.05..std::f64::consts::TAU, b in any_box()
        ) {
            let arc = Arc::new(p(cx, cy), radius, start, sweep);
            let sampled = (0..=256).any(|i| {
                b.contains_point(arc.point_at(arc.start + arc.sweep * f64::from(i) / 256.0))
            });
            if sampled {
                prop_assert!(arc.intersects_aabb(&b));
            }
            if !arc.bounds().expanded(1e-9).intersects(&b) {
                prop_assert!(!arc.intersects_aabb(&b));
            }
        }

        #[test]
        fn a_circle_with_a_sample_point_in_the_box_intersects_it(
            cx in coordinate(), cy in coordinate(), radius in 0.5..60.0_f64, b in any_box()
        ) {
            let circle = Circle::new(p(cx, cy), radius);
            let sampled = (0..256).any(|i| {
                b.contains_point(circle.point_at(std::f64::consts::TAU * f64::from(i) / 256.0))
            });
            if sampled {
                prop_assert!(circle.intersects_aabb(&b));
            }
        }
    }
}

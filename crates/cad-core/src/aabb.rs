//! Axis-aligned bounding boxes, used for spatial indexing and picking.

use crate::Point2;

/// An axis-aligned box with `min <= max` on both axes. A box may be degenerate
/// (zero width or height): a horizontal line has a zero-height box.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Aabb {
    min: Point2,
    max: Point2,
}

impl Aabb {
    /// The smallest box containing both points, given in any order.
    pub fn from_points(a: Point2, b: Point2) -> Self {
        Self {
            min: Point2::new(a.x.min(b.x), a.y.min(b.y)),
            max: Point2::new(a.x.max(b.x), a.y.max(b.y)),
        }
    }

    pub fn min(&self) -> Point2 {
        self.min
    }

    pub fn max(&self) -> Point2 {
        self.max
    }

    pub fn center(&self) -> Point2 {
        Point2::new(
            (self.min.x + self.max.x) / 2.0,
            (self.min.y + self.max.y) / 2.0,
        )
    }

    /// The four corners, counter-clockwise from the minimum.
    pub fn corners(&self) -> [Point2; 4] {
        [
            self.min,
            Point2::new(self.max.x, self.min.y),
            self.max,
            Point2::new(self.min.x, self.max.y),
        ]
    }

    /// The smallest box containing both boxes.
    pub fn union(&self, other: &Self) -> Self {
        Self {
            min: Point2::new(self.min.x.min(other.min.x), self.min.y.min(other.min.y)),
            max: Point2::new(self.max.x.max(other.max.x), self.max.y.max(other.max.y)),
        }
    }

    /// True when the boxes share at least one point. Touching edges count.
    pub fn intersects(&self, other: &Self) -> bool {
        self.min.x <= other.max.x
            && other.min.x <= self.max.x
            && self.min.y <= other.max.y
            && other.min.y <= self.max.y
    }

    /// True when `point` is inside or on the boundary.
    pub fn contains_point(&self, point: Point2) -> bool {
        self.min.x <= point.x
            && point.x <= self.max.x
            && self.min.y <= point.y
            && point.y <= self.max.y
    }

    /// True when `other` lies entirely inside or on the boundary.
    pub fn contains(&self, other: &Self) -> bool {
        self.contains_point(other.min) && self.contains_point(other.max)
    }

    /// The box grown by `margin` on every side. `margin` must not be negative.
    pub fn expanded(&self, margin: f64) -> Self {
        debug_assert!(margin >= 0.0, "margin must not be negative");
        Self {
            min: Point2::new(self.min.x - margin, self.min.y - margin),
            max: Point2::new(self.max.x + margin, self.max.y + margin),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn point() -> impl Strategy<Value = Point2> {
        (-1.0e6..1.0e6_f64, -1.0e6..1.0e6_f64).prop_map(|(x, y)| Point2::new(x, y))
    }

    fn aabb() -> impl Strategy<Value = Aabb> {
        (point(), point()).prop_map(|(a, b)| Aabb::from_points(a, b))
    }

    #[test]
    fn from_points_orders_corners() {
        let b = Aabb::from_points(Point2::new(5.0, -1.0), Point2::new(-2.0, 3.0));
        assert_eq!(b.min(), Point2::new(-2.0, -1.0));
        assert_eq!(b.max(), Point2::new(5.0, 3.0));
        assert_eq!(b.center(), Point2::new(1.5, 1.0));
    }

    #[test]
    fn corners_run_counter_clockwise_from_the_minimum() {
        let b = Aabb::from_points(Point2::new(0.0, 0.0), Point2::new(2.0, 1.0));
        assert_eq!(
            b.corners(),
            [
                Point2::new(0.0, 0.0),
                Point2::new(2.0, 0.0),
                Point2::new(2.0, 1.0),
                Point2::new(0.0, 1.0)
            ]
        );
    }

    #[test]
    fn touching_boxes_intersect() {
        let a = Aabb::from_points(Point2::new(0.0, 0.0), Point2::new(1.0, 1.0));
        let b = Aabb::from_points(Point2::new(1.0, 1.0), Point2::new(2.0, 2.0));
        assert!(a.intersects(&b));
    }

    #[test]
    fn separated_boxes_do_not_intersect() {
        let a = Aabb::from_points(Point2::new(0.0, 0.0), Point2::new(1.0, 1.0));
        let b = Aabb::from_points(Point2::new(1.5, 0.0), Point2::new(2.0, 1.0));
        assert!(!a.intersects(&b));
    }

    #[test]
    fn degenerate_box_still_contains_its_points() {
        let b = Aabb::from_points(Point2::new(0.0, 2.0), Point2::new(10.0, 2.0));
        assert!(b.contains_point(Point2::new(5.0, 2.0)));
        assert!(!b.contains_point(Point2::new(5.0, 2.1)));
    }

    proptest! {
        #[test]
        fn union_is_commutative(a in aabb(), b in aabb()) {
            prop_assert_eq!(a.union(&b), b.union(&a));
        }

        #[test]
        fn union_contains_both_operands(a in aabb(), b in aabb()) {
            let u = a.union(&b);
            prop_assert!(u.contains(&a));
            prop_assert!(u.contains(&b));
        }

        #[test]
        fn union_is_idempotent(a in aabb()) {
            prop_assert_eq!(a.union(&a), a);
        }

        #[test]
        fn intersection_is_symmetric(a in aabb(), b in aabb()) {
            prop_assert_eq!(a.intersects(&b), b.intersects(&a));
        }

        #[test]
        fn containment_implies_intersection(a in aabb(), b in aabb()) {
            if a.contains(&b) {
                prop_assert!(a.intersects(&b));
            }
        }

        #[test]
        fn expanded_box_contains_the_original(a in aabb(), margin in 0.0..1.0e3_f64) {
            prop_assert!(a.expanded(margin).contains(&a));
        }

        #[test]
        fn box_contains_the_points_it_was_built_from(p in point(), q in point()) {
            let b = Aabb::from_points(p, q);
            prop_assert!(b.contains_point(p));
            prop_assert!(b.contains_point(q));
        }
    }
}

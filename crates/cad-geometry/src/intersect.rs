//! Intersection points between lines, circles, and arcs, and the robust
//! orientation predicate (DESIGN §21).
//!
//! These are the primitives under intersection snap, trim, extend, fillet,
//! and chamfer. Each tolerance class is used for what it is for:
//!
//! - **angular** decides whether two directions are parallel;
//! - **linear** decides whether a computed point lies on a bounded curve
//!   (within reach of a segment's end or an arc's end) and whether two
//!   solutions are the same point (tangency);
//! - **orientation** — which side of a line a point is on — uses an exact
//!   predicate, not a tolerance (§21.2).
//!
//! Degenerate configurations have defined results: parallel or collinear
//! lines, concentric or identical circles, and zero-length segments yield no
//! intersection points, because no finite set of points describes them.

use cad_core::tolerance::{AngularTolerance, LinearTolerance};
use cad_core::{Aabb, Point2};

use crate::{Arc, Circle, Segment};

const LINEAR: LinearTolerance = LinearTolerance::DEFAULT;
const ANGULAR: AngularTolerance = AngularTolerance::DEFAULT;

/// Up to two intersection points, without allocating.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Hits {
    points: [Point2; 2],
    len: usize,
}

impl Hits {
    pub const NONE: Self = Self {
        points: [Point2::ORIGIN; 2],
        len: 0,
    };

    pub(crate) fn one(point: Point2) -> Self {
        Self {
            points: [point, Point2::ORIGIN],
            len: 1,
        }
    }

    /// Two solutions, merged into one when they coincide (tangency).
    pub(crate) fn pair(a: Point2, b: Point2) -> Self {
        if LINEAR.coincident(a, b) {
            Self::one(a)
        } else {
            Self {
                points: [a, b],
                len: 2,
            }
        }
    }

    pub fn as_slice(&self) -> &[Point2] {
        &self.points[..self.len]
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// The hits for which `keep` holds.
    pub(crate) fn filtered(self, keep: impl Fn(Point2) -> bool) -> Self {
        let mut kept = Self::NONE;
        for point in self.as_slice().iter().copied().filter(|point| keep(*point)) {
            kept.points[kept.len] = point;
            kept.len += 1;
        }
        kept
    }
}

/// Which side of the directed line `a → b` the point `c` lies on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Orientation {
    /// `c` is to the left: `a`, `b`, `c` turn counter-clockwise.
    Left,
    /// `c` is to the right: `a`, `b`, `c` turn clockwise.
    Right,
    /// The three points are exactly collinear.
    Collinear,
}

/// The exact orientation of three points, computed with adaptive-precision
/// arithmetic. Unlike a cross product compared against an epsilon, the answer
/// is never wrong because of rounding.
pub fn orientation(a: Point2, b: Point2, c: Point2) -> Orientation {
    let coord = |point: Point2| robust::Coord {
        x: point.x,
        y: point.y,
    };
    let determinant = robust::orient2d(coord(a), coord(b), coord(c));
    if determinant > 0.0 {
        Orientation::Left
    } else if determinant < 0.0 {
        Orientation::Right
    } else {
        Orientation::Collinear
    }
}

/// Where the infinite lines through `a` and `b` meet. `None` when they are
/// parallel (or the same line), or when either segment has no direction.
pub fn line_line(a: &Segment, b: &Segment) -> Option<Point2> {
    let (da, db) = (a.end - a.start, b.end - b.start);
    let cross = da.x * db.y - da.y * db.x;
    // |cross| = |da||db| sin θ: parallel when the angle between them is negligible.
    if cross.abs() <= ANGULAR.value() * da.length() * db.length() {
        return None;
    }
    let between = b.start - a.start;
    let t = (between.x * db.y - between.y * db.x) / cross;
    Some(a.start + da * t)
}

/// Where two bounded segments meet, if they do.
pub fn segment_segment(a: &Segment, b: &Segment) -> Option<Point2> {
    line_line(a, b).filter(|point| on_segment(a, *point) && on_segment(b, *point))
}

/// Where the infinite line through `line` meets `circle`.
pub fn line_circle(line: &Segment, circle: &Circle) -> Hits {
    let direction = line.end - line.start;
    let length = direction.length();
    if LINEAR.is_zero(length) {
        return Hits::NONE;
    }
    let unit = direction * (1.0 / length);
    // Project the centre onto the line; the hits sit symmetrically around the foot.
    let along = (circle.center - line.start).dot(unit);
    let foot = line.start + unit * along;
    let offset = circle.center.distance_to(foot);
    let half_chord_squared = circle.radius * circle.radius - offset * offset;
    if half_chord_squared < 0.0 {
        // Just outside, within tolerance, still counts as tangent.
        return if LINEAR.eq(offset, circle.radius) {
            Hits::one(foot)
        } else {
            Hits::NONE
        };
    }
    let half_chord = half_chord_squared.sqrt();
    Hits::pair(foot - unit * half_chord, foot + unit * half_chord)
}

pub fn segment_circle(segment: &Segment, circle: &Circle) -> Hits {
    line_circle(segment, circle).filtered(|point| on_segment(segment, point))
}

pub fn segment_arc(segment: &Segment, arc: &Arc) -> Hits {
    line_circle(segment, &arc.circle())
        .filtered(|point| on_segment(segment, point) && on_arc(arc, point))
}

/// Where two circles meet. Concentric circles — including identical ones —
/// have no intersection points.
pub fn circle_circle(a: &Circle, b: &Circle) -> Hits {
    let between = b.center - a.center;
    let distance = between.length();
    if LINEAR.is_zero(distance) {
        return Hits::NONE;
    }
    let unit = between * (1.0 / distance);
    // Distance from a's centre to the radical line, along the line of centres.
    let along =
        (distance * distance + a.radius * a.radius - b.radius * b.radius) / (2.0 * distance);
    let foot = a.center + unit * along;
    let half_chord_squared = a.radius * a.radius - along * along;
    if half_chord_squared < 0.0 {
        let touching = LINEAR.eq(distance, a.radius + b.radius)
            || LINEAR.eq(distance, (a.radius - b.radius).abs());
        return if touching {
            Hits::one(foot)
        } else {
            Hits::NONE
        };
    }
    let half_chord = half_chord_squared.sqrt();
    let normal = cad_core::Vec2::new(-unit.y, unit.x);
    Hits::pair(foot - normal * half_chord, foot + normal * half_chord)
}

pub fn circle_arc(circle: &Circle, arc: &Arc) -> Hits {
    circle_circle(circle, &arc.circle()).filtered(|point| on_arc(arc, point))
}

pub fn arc_arc(a: &Arc, b: &Arc) -> Hits {
    circle_circle(&a.circle(), &b.circle()).filtered(|point| on_arc(a, point) && on_arc(b, point))
}

/// One piece of an entity's geometry, for code that treats lines, circles,
/// and arcs uniformly.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Primitive {
    Segment(Segment),
    Circle(Circle),
    Arc(Arc),
}

impl Primitive {
    /// Every point where the two primitives meet.
    pub fn intersect(&self, other: &Primitive) -> Hits {
        use Primitive::{Arc as A, Circle as C, Segment as S};
        match (self, other) {
            (S(a), S(b)) => segment_segment(a, b).map_or(Hits::NONE, Hits::one),
            (S(segment), C(circle)) | (C(circle), S(segment)) => segment_circle(segment, circle),
            (S(segment), A(arc)) | (A(arc), S(segment)) => segment_arc(segment, arc),
            (C(a), C(b)) => circle_circle(a, b),
            (C(circle), A(arc)) | (A(arc), C(circle)) => circle_arc(circle, arc),
            (A(a), A(b)) => arc_arc(a, b),
        }
    }

    /// The primitive's bounding box.
    pub fn bounds(&self) -> Aabb {
        match self {
            Primitive::Segment(segment) => segment.bounds(),
            Primitive::Circle(circle) => circle.bounds(),
            Primitive::Arc(arc) => arc.bounds(),
        }
    }

    /// True when any part of the primitive lies within `bounds`.
    pub fn intersects_aabb(&self, bounds: &Aabb) -> bool {
        match self {
            Primitive::Segment(segment) => segment.intersects_aabb(bounds),
            Primitive::Circle(circle) => circle.intersects_aabb(bounds),
            Primitive::Arc(arc) => arc.intersects_aabb(bounds),
        }
    }

    /// The point of the primitive nearest to `point`.
    pub fn closest_point(&self, point: Point2) -> Point2 {
        match self {
            Primitive::Segment(segment) => segment.closest_point(point),
            Primitive::Circle(circle) => circle.closest_point(point),
            Primitive::Arc(arc) => arc.closest_point(point),
        }
    }

    pub fn distance_to_point(&self, point: Point2) -> f64 {
        match self {
            Primitive::Segment(segment) => segment.distance_to_point(point),
            Primitive::Circle(circle) => circle.distance_to_point(point),
            Primitive::Arc(arc) => arc.distance_to_point(point),
        }
    }
}

fn on_segment(segment: &Segment, point: Point2) -> bool {
    LINEAR.is_zero(segment.distance_to_point(point))
}

fn on_arc(arc: &Arc, point: Point2) -> bool {
    LINEAR.is_zero(arc.distance_to_point(point))
}

#[cfg(test)]
mod tests {
    use std::f64::consts::{FRAC_PI_2, PI, TAU};

    use proptest::prelude::*;

    use super::*;

    fn p(x: f64, y: f64) -> Point2 {
        Point2::new(x, y)
    }

    fn seg(x0: f64, y0: f64, x1: f64, y1: f64) -> Segment {
        Segment::new(p(x0, y0), p(x1, y1))
    }

    fn circle(x: f64, y: f64, radius: f64) -> Circle {
        Circle::new(p(x, y), radius)
    }

    fn close(a: Point2, b: Point2) -> bool {
        a.distance_to(b) < 1e-9
    }

    /// The hits, in any order, match `expected`.
    fn assert_hits(hits: Hits, expected: &[Point2]) {
        assert_eq!(hits.len(), expected.len(), "{hits:?}");
        for want in expected {
            assert!(
                hits.as_slice().iter().any(|got| close(*got, *want)),
                "missing {want:?} in {hits:?}"
            );
        }
    }

    // ── orientation ──

    #[test]
    fn orientation_names_the_side() {
        assert_eq!(
            orientation(p(0.0, 0.0), p(1.0, 0.0), p(0.5, 1.0)),
            Orientation::Left
        );
        assert_eq!(
            orientation(p(0.0, 0.0), p(1.0, 0.0), p(0.5, -1.0)),
            Orientation::Right
        );
        assert_eq!(
            orientation(p(0.0, 0.0), p(1.0, 0.0), p(7.0, 0.0)),
            Orientation::Collinear
        );
    }

    /// The case a naive cross product gets wrong: points almost on a line far
    /// from the origin, where rounding swamps the true sign.
    #[test]
    fn orientation_is_exact_where_a_cross_product_is_not() {
        let (a, b) = (p(12.0, 12.0), p(24.0, 24.0));
        let mut naive_disagreements = 0;
        for i in 0..64 {
            for j in 0..64 {
                let c = p(
                    0.5 + f64::from(i) * f64::EPSILON,
                    0.5 + f64::from(j) * f64::EPSILON,
                );
                let exact = orientation(a, b, c);
                // On the line y = x exactly when i == j.
                let expected = match i.cmp(&j) {
                    std::cmp::Ordering::Less => Orientation::Left,
                    std::cmp::Ordering::Greater => Orientation::Right,
                    std::cmp::Ordering::Equal => Orientation::Collinear,
                };
                assert_eq!(exact, expected, "i={i} j={j}");

                let cross = (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x);
                let naive = if cross > 0.0 {
                    Orientation::Left
                } else if cross < 0.0 {
                    Orientation::Right
                } else {
                    Orientation::Collinear
                };
                naive_disagreements += usize::from(naive != expected);
            }
        }
        assert!(
            naive_disagreements > 0,
            "the test should exercise cases where rounding matters"
        );
    }

    // ── lines ──

    #[test]
    fn crossing_segments_meet_at_one_point() {
        assert!(close(
            segment_segment(&seg(0.0, 0.0, 10.0, 10.0), &seg(0.0, 10.0, 10.0, 0.0)).unwrap(),
            p(5.0, 5.0)
        ));
    }

    #[test]
    fn segments_that_would_meet_only_if_extended_do_not_intersect() {
        let (a, b) = (seg(0.0, 0.0, 1.0, 1.0), seg(0.0, 10.0, 10.0, 0.0));
        assert_eq!(segment_segment(&a, &b), None);
        assert!(
            close(line_line(&a, &b).unwrap(), p(5.0, 5.0)),
            "the infinite lines do"
        );
    }

    #[test]
    fn segments_touching_at_an_endpoint_intersect_there() {
        assert!(close(
            segment_segment(&seg(0.0, 0.0, 5.0, 0.0), &seg(5.0, 0.0, 5.0, 5.0)).unwrap(),
            p(5.0, 0.0)
        ));
        // A T-junction: one segment ends on the other's interior.
        assert!(close(
            segment_segment(&seg(0.0, 0.0, 10.0, 0.0), &seg(4.0, 0.0, 4.0, 3.0)).unwrap(),
            p(4.0, 0.0)
        ));
    }

    #[test]
    fn parallel_collinear_and_degenerate_lines_have_no_intersection_point() {
        assert_eq!(
            line_line(&seg(0.0, 0.0, 10.0, 0.0), &seg(0.0, 1.0, 10.0, 1.0)),
            None,
            "parallel"
        );
        assert_eq!(
            line_line(&seg(0.0, 0.0, 10.0, 0.0), &seg(5.0, 0.0, 20.0, 0.0)),
            None,
            "collinear, overlapping"
        );
        assert_eq!(
            line_line(&seg(0.0, 0.0, 10.0, 0.0), &seg(3.0, 3.0, 3.0, 3.0)),
            None,
            "zero length"
        );
        // Nearly parallel: the angle is below the angular tolerance.
        assert_eq!(
            line_line(&seg(0.0, 0.0, 1.0, 0.0), &seg(0.0, 1.0, 1.0, 1.0 + 1e-12)),
            None
        );
    }

    // ── line and circle ──

    #[test]
    fn a_line_through_a_circle_hits_twice() {
        assert_hits(
            line_circle(&seg(-20.0, 0.0, 20.0, 0.0), &circle(0.0, 0.0, 5.0)),
            &[p(-5.0, 0.0), p(5.0, 0.0)],
        );
        assert_hits(
            segment_circle(&seg(0.0, 0.0, 20.0, 0.0), &circle(0.0, 0.0, 5.0)),
            &[p(5.0, 0.0)],
        );
        assert_hits(
            segment_circle(&seg(-1.0, 0.0, 1.0, 0.0), &circle(0.0, 0.0, 5.0)),
            &[],
        );
    }

    #[test]
    fn a_tangent_line_hits_once() {
        assert_hits(
            line_circle(&seg(-10.0, 5.0, 10.0, 5.0), &circle(0.0, 0.0, 5.0)),
            &[p(0.0, 5.0)],
        );
        // Within tolerance outside the circle still counts as tangent.
        assert_hits(
            line_circle(
                &seg(-10.0, 5.0 + 1e-11, 10.0, 5.0 + 1e-11),
                &circle(0.0, 0.0, 5.0),
            ),
            &[p(0.0, 5.0)],
        );
        assert_hits(
            line_circle(&seg(-10.0, 5.1, 10.0, 5.1), &circle(0.0, 0.0, 5.0)),
            &[],
        );
    }

    #[test]
    fn a_segment_meets_an_arc_only_on_its_sweep() {
        let upper_right = Arc::new(p(0.0, 0.0), 10.0, 0.0, FRAC_PI_2);
        assert_hits(
            segment_arc(&seg(0.0, 0.0, 20.0, 20.0), &upper_right),
            &[p(50f64.sqrt(), 50f64.sqrt())],
        );
        assert_hits(segment_arc(&seg(0.0, 0.0, -20.0, -20.0), &upper_right), &[]);
        // Through both ends of a half circle.
        let upper = Arc::new(p(0.0, 0.0), 10.0, 0.0, PI);
        assert_hits(
            segment_arc(&seg(-20.0, 0.0, 20.0, 0.0), &upper),
            &[p(-10.0, 0.0), p(10.0, 0.0)],
        );
    }

    // ── circles ──

    #[test]
    fn overlapping_circles_meet_twice() {
        assert_hits(
            circle_circle(&circle(0.0, 0.0, 5.0), &circle(6.0, 0.0, 5.0)),
            &[p(3.0, 4.0), p(3.0, -4.0)],
        );
    }

    #[test]
    fn tangent_circles_meet_once_inside_or_outside() {
        assert_hits(
            circle_circle(&circle(0.0, 0.0, 5.0), &circle(8.0, 0.0, 3.0)),
            &[p(5.0, 0.0)],
        );
        assert_hits(
            circle_circle(&circle(0.0, 0.0, 5.0), &circle(2.0, 0.0, 3.0)),
            &[p(5.0, 0.0)],
        );
    }

    #[test]
    fn separate_nested_and_concentric_circles_do_not_meet() {
        assert_hits(
            circle_circle(&circle(0.0, 0.0, 5.0), &circle(20.0, 0.0, 5.0)),
            &[],
        );
        assert_hits(
            circle_circle(&circle(0.0, 0.0, 5.0), &circle(1.0, 0.0, 1.0)),
            &[],
        );
        assert_hits(
            circle_circle(&circle(0.0, 0.0, 5.0), &circle(0.0, 0.0, 3.0)),
            &[],
        );
        // Identical circles: no finite answer.
        assert_hits(
            circle_circle(&circle(0.0, 0.0, 5.0), &circle(0.0, 0.0, 5.0)),
            &[],
        );
    }

    #[test]
    fn arcs_meet_only_where_both_sweeps_reach() {
        let a = Arc::new(p(0.0, 0.0), 5.0, 0.0, PI); // upper half
        let b = Arc::new(p(6.0, 0.0), 5.0, FRAC_PI_2, PI); // left half
        assert_hits(arc_arc(&a, &b), &[p(3.0, 4.0)]);
        assert_hits(circle_arc(&circle(6.0, 0.0, 5.0), &a), &[p(3.0, 4.0)]);
    }

    #[test]
    fn primitives_dispatch_symmetrically() {
        let pieces = [
            Primitive::Segment(seg(-20.0, 1.0, 20.0, 1.0)),
            Primitive::Circle(circle(0.0, 0.0, 5.0)),
            Primitive::Arc(Arc::new(p(3.0, 0.0), 5.0, 0.0, TAU * 0.75)),
        ];
        for a in &pieces {
            for b in &pieces {
                let (forward, backward) = (a.intersect(b), b.intersect(a));
                assert_hits(forward, backward.as_slice());
            }
        }
    }

    fn coordinate() -> impl Strategy<Value = f64> {
        -100.0..100.0_f64
    }

    fn any_segment() -> impl Strategy<Value = Segment> {
        (coordinate(), coordinate(), coordinate(), coordinate())
            .prop_map(|(a, b, c, d)| seg(a, b, c, d))
    }

    fn any_circle() -> impl Strategy<Value = Circle> {
        (coordinate(), coordinate(), 0.5..80.0_f64).prop_map(|(x, y, r)| circle(x, y, r))
    }

    fn any_arc() -> impl Strategy<Value = Arc> {
        (any_circle(), 0.0..TAU, 0.1..TAU)
            .prop_map(|(c, start, sweep)| Arc::new(c.center, c.radius, start, sweep))
    }

    fn any_primitive() -> impl Strategy<Value = Primitive> {
        prop_oneof![
            any_segment().prop_map(Primitive::Segment),
            any_circle().prop_map(Primitive::Circle),
            any_arc().prop_map(Primitive::Arc),
        ]
    }

    proptest! {
        /// Every reported intersection lies on both inputs.
        #[test]
        fn hits_lie_on_both_primitives(a in any_primitive(), b in any_primitive()) {
            for point in a.intersect(&b).as_slice() {
                prop_assert!(a.distance_to_point(*point) < 1e-6, "{point:?} is off {a:?}");
                prop_assert!(b.distance_to_point(*point) < 1e-6, "{point:?} is off {b:?}");
            }
        }

        #[test]
        fn intersection_is_symmetric_in_count(a in any_primitive(), b in any_primitive()) {
            prop_assert_eq!(a.intersect(&b).len(), b.intersect(&a).len());
        }

        /// Two segments built to cross at a known point are found to cross there.
        #[test]
        fn constructed_crossings_are_found(
            x in coordinate(), y in coordinate(),
            first in 0.1..TAU, turn in 0.2..(PI - 0.2), reach in 1.0..50.0_f64
        ) {
            let at = p(x, y);
            let arm = |angle: f64| cad_core::Vec2::from_polar(reach, angle);
            let a = Segment::new(at - arm(first), at + arm(first));
            let b = Segment::new(at - arm(first + turn), at + arm(first + turn));
            let found = segment_segment(&a, &b);
            prop_assert!(found.is_some_and(|point| point.distance_to(at) < 1e-7), "{found:?} vs {at:?}");
        }

        #[test]
        fn orientation_flips_when_the_line_is_reversed(
            ax in coordinate(), ay in coordinate(), bx in coordinate(), by in coordinate(),
            cx in coordinate(), cy in coordinate()
        ) {
            let (a, b, c) = (p(ax, ay), p(bx, by), p(cx, cy));
            let flipped = match orientation(a, b, c) {
                Orientation::Left => Orientation::Right,
                Orientation::Right => Orientation::Left,
                Orientation::Collinear => Orientation::Collinear,
            };
            prop_assert_eq!(orientation(b, a, c), flipped);
        }
    }
}

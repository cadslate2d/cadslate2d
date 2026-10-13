//! Intersection points between lines, circles, and arcs, and the robust
//! orientation predicate (DESIGN §21).
//!
//! These are the primitives under intersection snap, trim, extend, fillet,
//! and chamfer. Each tolerance class is used for what it is for:
//!
//! - **angular** decides whether two directions are parallel;
//! - **linear** decides whether a computed point lies on a bounded curve
//!   (within reach of a segment's end or an arc's end) and whether two
//!   solutions are the same point (tangency). It is taken at the magnitude
//!   of the coordinates at hand (`LinearTolerance::around`, ADR-0012);
//! - **orientation** — which side of a line a point is on — uses an exact
//!   predicate, not a tolerance (§21.2).
//!
//! A hit is computed *on* the line or circle it belongs to, so whether it is
//! on the bounded segment or arc is a question of where along — a parameter
//! or an angle — never of measuring its distance from the curve again. Far
//! from the origin that distance cannot be resolved to the tolerance, and
//! measuring it would throw good hits away.
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
        if LINEAR.around(&[a, b]).coincident(a, b) {
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

/// Where the infinite lines through `a` and `b` meet, as a parameter along
/// each: 0 at the segment's start, 1 at its end. `None` when they are
/// parallel (or the same line), or when either segment has no direction.
fn crossing_parameters(a: &Segment, b: &Segment) -> Option<(f64, f64)> {
    let (da, db) = (a.end - a.start, b.end - b.start);
    let cross = da.x * db.y - da.y * db.x;
    // |cross| = |da||db| sin θ: parallel when the angle between them is negligible.
    if cross.abs() <= ANGULAR.value() * da.length() * db.length() {
        return None;
    }
    let between = b.start - a.start;
    Some((
        (between.x * db.y - between.y * db.x) / cross,
        (between.x * da.y - between.y * da.x) / cross,
    ))
}

/// Whether a parameter along a segment of this length — 0 at its start, 1
/// at its end — is on the segment, give or take the tolerance at each end.
fn within_span(parameter: f64, length: f64, tolerance: LinearTolerance) -> bool {
    let slack = tolerance.value() / length;
    parameter >= -slack && parameter <= 1.0 + slack
}

/// Where the infinite lines through `a` and `b` meet. `None` when they are
/// parallel (or the same line), or when either segment has no direction.
pub fn line_line(a: &Segment, b: &Segment) -> Option<Point2> {
    crossing_parameters(a, b).map(|(along_a, _)| a.start + (a.end - a.start) * along_a)
}

/// Where two bounded segments meet, if they do.
pub fn segment_segment(a: &Segment, b: &Segment) -> Option<Point2> {
    let (along_a, along_b) = crossing_parameters(a, b)?;
    let tolerance = LINEAR.around(&[a.start, a.end, b.start, b.end]);
    (within_span(along_a, a.length(), tolerance) && within_span(along_b, b.length(), tolerance))
        .then(|| a.start + (a.end - a.start) * along_a)
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
    // The difference first, then the product: exact enough for a line
    // grazing a very large circle.
    let half_chord_squared = (circle.radius - offset) * (circle.radius + offset);
    if half_chord_squared < 0.0 {
        // Just outside, within tolerance, still counts as tangent.
        let tolerance = LINEAR
            .around(&[line.start, line.end, circle.center])
            .at_magnitude(circle.radius);
        return if tolerance.eq(offset, circle.radius) {
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
///
/// Everything is measured from the smaller circle, and differences of nearly
/// equal lengths are taken before squaring, so the error of a hit scales
/// with the smaller radius. Measured from the larger one, a unit circle
/// crossing one of radius 40 000 — a nearly straight polyline arc — lands
/// micrometres off, far outside the linear tolerance, and differently
/// depending on which circle is named first.
pub fn circle_circle(a: &Circle, b: &Circle) -> Hits {
    let swapped = a.radius > b.radius;
    let (small, large) = if swapped { (b, a) } else { (a, b) };
    let between = large.center - small.center;
    let distance = between.length();
    if LINEAR.is_zero(distance) {
        return Hits::NONE;
    }
    let unit = between * (1.0 / distance);
    // Distance from the smaller centre to the radical line, along the line
    // of centres: (d² + r² − R²) / 2d.
    let along = ((distance - large.radius) * (distance + large.radius)
        + small.radius * small.radius)
        / (2.0 * distance);
    let foot = small.center + unit * along;
    let half_chord_squared = (small.radius - along) * (small.radius + along);
    if half_chord_squared < 0.0 {
        // Not quite reaching, but within tolerance: they touch. The point
        // is taken on the smaller circle — towards the larger one's centre
        // when they touch from outside, away from it when one is inside
        // the other. (The foot of the radical line will not do: for a
        // circle nearly as large as the one it is inside, it overshoots by
        // several times the gap.)
        let tolerance = LINEAR
            .around(&[a.center, b.center])
            .at_magnitude(large.radius);
        return if tolerance.eq(distance, a.radius + b.radius) {
            Hits::one(small.center + unit * small.radius)
        } else if tolerance.eq(distance, (a.radius - b.radius).abs()) {
            Hits::one(small.center - unit * small.radius)
        } else {
            Hits::NONE
        };
    }
    let half_chord = half_chord_squared.sqrt();
    // Across the line of centres, to the left going from `a` to `b`, so the
    // order of the hits does not depend on which circle is the smaller.
    let toward_b = if swapped { unit * -1.0 } else { unit };
    let normal = cad_core::Vec2::new(-toward_b.y, toward_b.x);
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

    /// Some point of the primitive: a segment's start, an arc's start, a
    /// point of a circle. For tests that need one point on a curve, such as
    /// whether the curve lies in a region it does not cross.
    pub fn a_point(&self) -> Point2 {
        match self {
            Primitive::Segment(segment) => segment.start,
            Primitive::Circle(circle) => circle.point_at(0.0),
            Primitive::Arc(arc) => arc.start_point(),
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

/// Whether `point` — already known to be on the segment's line, because
/// that is how it was computed — is on the segment: between its ends, or
/// within the tolerance of one.
fn on_segment(segment: &Segment, point: Point2) -> bool {
    let direction = segment.end - segment.start;
    let length = direction.length();
    let along = (point - segment.start).dot(direction) / (length * length);
    let tolerance = LINEAR.around(&[segment.start, segment.end]);
    within_span(along, length, tolerance)
}

/// Whether `point` — already known to be on the arc's circle, because that is
/// how it was computed — is on the arc: within its sweep, or at an end.
///
/// The distance from the centre is deliberately not measured again. For a
/// nearly straight arc the centre is millions of units away, and at that
/// range a coordinate cannot be told apart from its neighbour a nanometre
/// off: the check would reject points for the arithmetic's noise.
fn on_arc(arc: &Arc, point: Point2) -> bool {
    let offset = point - arc.center;
    let tolerance = LINEAR.around(&[point, arc.center]).at_magnitude(arc.radius);
    arc.contains_angle(offset.y.atan2(offset.x))
        || tolerance.coincident(point, arc.start_point())
        || tolerance.coincident(point, arc.end_point())
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

    /// Regression: found by the polyline-offset property test. A nearly
    /// straight arc (radius 44 083) and an ordinary one were found to meet
    /// when asked one way round and not the other, and a unit circle on the
    /// big arc was placed 3.4 µm from where it is.
    #[test]
    fn a_very_large_circle_meets_a_small_one_exactly_and_either_way_round() {
        let flat = Arc::new(
            p(-7719.7602150052735, -43402.37521451124),
            44082.566916010765,
            1.3947722252302803,
            0.003993887177161923,
        );
        let round = Arc::new(
            p(-30.529016862021834, -2.191951976973318),
            145.56553127466626,
            2.9167476104082644,
            3.609608031445091,
        );
        let (forward, backward) = (arc_arc(&flat, &round), arc_arc(&round, &flat));
        assert_eq!(forward.len(), 1, "{forward:?}");
        assert_hits(backward, forward.as_slice());

        // A unit circle centred on the big one: both hits are on both.
        let big = flat.circle();
        let centre = flat.end_point();
        let unit = Circle::new(centre, 1.0);
        for hits in [circle_circle(&big, &unit), circle_circle(&unit, &big)] {
            assert_eq!(hits.len(), 2);
            for hit in hits.as_slice() {
                assert!((hit.distance_to(centre) - 1.0).abs() < 1e-12, "{hit:?}");
                assert!((hit.distance_to(big.center) - big.radius).abs() < 1e-9);
            }
        }
    }

    /// Regression: found by the stress tests. A circle of radius 49.8 inside
    /// one of radius 57.6, short of touching it by 10⁻⁹: the touching point
    /// came back 6 × 10⁻⁹ off the larger circle, outside the tolerance that
    /// had just said they touch.
    #[test]
    fn circles_that_touch_within_tolerance_touch_at_a_point_on_both() {
        let outer = circle(0.0, 0.0, 57.5512954750725);
        let gap = 9.5e-10;
        let inner = circle(
            57.5512954750725 - 49.83724509716322 - gap,
            0.0,
            49.83724509716322,
        );
        for hits in [circle_circle(&outer, &inner), circle_circle(&inner, &outer)] {
            assert_eq!(hits.len(), 1, "{hits:?}");
            let touch = hits.as_slice()[0];
            assert!(
                outer.distance_to_point(touch) <= LINEAR.value(),
                "{touch:?}"
            );
            assert!(
                inner.distance_to_point(touch) <= LINEAR.value(),
                "{touch:?}"
            );
            assert!(touch.x > 57.0, "on the side where they touch");
        }
        // From outside, the same gap apart.
        let beside = circle(57.5512954750725 + 20.0 + gap, 0.0, 20.0);
        for hits in [
            circle_circle(&outer, &beside),
            circle_circle(&beside, &outer),
        ] {
            assert_eq!(hits.len(), 1);
            let touch = hits.as_slice()[0];
            assert!(outer.distance_to_point(touch) <= LINEAR.value());
            assert!(beside.distance_to_point(touch) <= LINEAR.value());
        }
    }

    /// Regression (ADR-0012): two segments crossing ten million units out,
    /// and one crossing a circle there, are found — a computed point is
    /// placed by where it is along the segment, not by measuring how far
    /// off the segment the arithmetic left it.
    #[test]
    fn crossings_far_from_the_origin_are_found() {
        let mut missed = 0;
        for step in 0..500 {
            let turn = f64::from(step) * 0.618_033_988_75;
            let at = p(1e10 + turn.sin() * 137.0, 7e9 + turn.cos() * 91.0);
            let (a, b) = ((turn * 1.3).cos(), (turn * 1.3).sin());
            let one = Segment::new(
                p(at.x - 50.0 * a, at.y - 50.0 * b),
                p(at.x + 70.0 * a, at.y + 70.0 * b),
            );
            let other = Segment::new(
                p(at.x + 60.0 * b, at.y - 60.0 * a),
                p(at.x - 40.0 * b, at.y + 40.0 * a),
            );
            if segment_segment(&one, &other).is_none() {
                missed += 1;
            }
            if segment_circle(&one, &Circle::new(at, 30.0)).len() != 2 {
                missed += 1;
            }
        }
        assert_eq!(missed, 0);
    }

    #[test]
    fn the_order_of_the_hits_does_not_depend_on_which_circle_is_larger() {
        let (small, large) = (circle(0.0, 0.0, 5.0), circle(20.0, 0.0, 18.0));
        let hits = circle_circle(&small, &large);
        assert!(hits.as_slice()[0].y < 0.0 && hits.as_slice()[1].y > 0.0);
        let (small, large) = (circle(20.0, 0.0, 5.0), circle(0.0, 0.0, 18.0));
        let hits = circle_circle(&large, &small);
        assert!(hits.as_slice()[0].y < 0.0 && hits.as_slice()[1].y > 0.0);
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

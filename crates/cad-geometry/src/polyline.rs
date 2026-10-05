//! Polylines: chains of segments through a list of vertices, where a
//! segment is straight or — when its starting vertex carries a *bulge* — a
//! circular arc.
//!
//! The bulge is the DXF convention, and the one drafters' polylines have
//! always used: for the segment from one vertex to the next,
//!
//! ```text
//! bulge = tan(included angle / 4)
//! ```
//!
//! positive when the arc turns counter-clockwise on the way from the first
//! vertex to the second, negative when clockwise, zero for a straight
//! segment. A bulge of ±1 is a semicircle. The sagitta — how far the arc
//! stands off the chord — is `|bulge| × chord / 2`.

use cad_core::tolerance::{AngularTolerance, LinearTolerance, ParametricTolerance};
use cad_core::{Point2, Vec2};

use crate::{Arc, Primitive, Segment};

/// A bulge this small is a straight segment: the arc's radius would exceed
/// 10¹¹ chord lengths.
const STRAIGHT: ParametricTolerance = ParametricTolerance::DEFAULT;

/// True when a segment with this bulge is straight.
pub fn is_straight(bulge: f64) -> bool {
    !bulge.is_finite() || STRAIGHT.is_zero(bulge)
}

/// The arc a bulged segment from `start` to `end` runs along, as a
/// counter-clockwise [`Arc`]. A clockwise segment (negative bulge) is the
/// same set of points as the counter-clockwise arc from `end` to `start`.
///
/// `None` when the segment is straight, or has no length.
pub fn arc_from_bulge(start: Point2, end: Point2, bulge: f64) -> Option<Arc> {
    let chord = end - start;
    let length = chord.length();
    if is_straight(bulge) || LinearTolerance::DEFAULT.is_zero(length) {
        return None;
    }
    // The centre lies on the chord's perpendicular bisector, to the left of
    // the direction of travel for a counter-clockwise minor arc.
    let middle = start + chord * 0.5;
    let left = Vec2::new(-chord.y, chord.x) * (1.0 / length);
    let to_center = (length / 2.0) * (1.0 - bulge * bulge) / (2.0 * bulge);
    let center = middle + left * to_center;
    let radius = length * (1.0 + bulge * bulge) / (4.0 * bulge.abs());

    // Stored counter-clockwise: a clockwise segment starts at its far end.
    let from = if bulge > 0.0 { start } else { end } - center;
    let sweep = 4.0 * bulge.abs().atan();
    radius
        .is_finite()
        .then(|| Arc::new(center, radius, from.y.atan2(from.x), sweep))
}

/// The bulge of a segment that runs along `arc`: from the arc's start to its
/// end when `forwards` (counter-clockwise, positive), from its end back to
/// its start otherwise (clockwise, negative).
pub fn bulge_from_arc(arc: &Arc, forwards: bool) -> f64 {
    let bulge = (arc.sweep / 4.0).tan();
    if forwards { bulge } else { -bulge }
}

/// The direction of travel on arriving at `end` along a segment from `start`
/// with this bulge: the chord's direction for a straight segment, turned by
/// half the included angle for an arc.
pub fn bulge_end_tangent(start: Point2, end: Point2, bulge: f64) -> Vec2 {
    let chord = end - start;
    if is_straight(bulge) {
        return chord;
    }
    // The tangent–chord angle is half the included angle: 2·atan(bulge).
    let (sine, cosine) = (2.0 * bulge.atan()).sin_cos();
    Vec2::new(
        chord.x * cosine - chord.y * sine,
        chord.x * sine + chord.y * cosine,
    )
}

/// The bulge of the arc from `start` to `end` that leaves `start` in the
/// direction `tangent` — how an arc segment continues smoothly from the
/// segment before it. Zero when `end` lies straight ahead.
///
/// `None` when `end` lies straight behind (no arc turns back on its own
/// tangent), when it is `start` itself, or when `tangent` has no direction.
pub fn bulge_from_tangent(start: Point2, tangent: Vec2, end: Point2) -> Option<f64> {
    let chord = end - start;
    let usable = LinearTolerance::DEFAULT.is_zero(chord.length())
        || LinearTolerance::DEFAULT.is_zero(tangent.length());
    if usable {
        return None;
    }
    // The signed angle from the tangent to the chord is half the included angle.
    let turn = (tangent.x * chord.y - tangent.y * chord.x).atan2(tangent.dot(chord));
    let straight_back = AngularTolerance::DEFAULT.is_zero(std::f64::consts::PI - turn.abs());
    (!straight_back).then(|| (turn / 2.0).tan())
}

/// One segment of a polyline from `start` to `end`: a straight segment, or
/// the arc its bulge describes.
pub fn bulge_primitive(start: Point2, end: Point2, bulge: f64) -> Primitive {
    arc_from_bulge(start, end, bulge).map_or_else(
        || Primitive::Segment(Segment::new(start, end)),
        Primitive::Arc,
    )
}

/// Whether a closed polyline of `count` vertices has a segment from its last
/// vertex back to its first. With three or more it always does. With two it
/// does only if one of the two segments is an arc — a circle drawn as two
/// arcs, a lens. Two straight segments between the same two points would be
/// one line drawn twice.
pub fn polyline_closes(count: usize, closed: bool, bulge_at: impl Fn(usize) -> f64) -> bool {
    closed && (count > 2 || (count == 2 && !(is_straight(bulge_at(0)) && is_straight(bulge_at(1)))))
}

/// The index pairs `(from, to)` of a polyline's segments, including the
/// closing segment when there is one (see [`polyline_closes`]). Fewer than
/// two vertices yield none.
pub fn polyline_spans(
    count: usize,
    closed: bool,
    bulge_at: impl Fn(usize) -> f64,
) -> impl Iterator<Item = (usize, usize)> {
    let closing = polyline_closes(count, closed, bulge_at).then(|| (count - 1, 0));
    (1..count).map(|to| (to - 1, to)).chain(closing)
}

/// The geometry of a polyline through `vertices`: one primitive per segment,
/// in order. `bulge_at(i)` is the bulge of the segment that starts at vertex
/// `i`.
pub fn polyline_primitives<'a>(
    vertices: &'a [Point2],
    bulge_at: impl Fn(usize) -> f64 + 'a,
    closed: bool,
) -> impl Iterator<Item = Primitive> + 'a {
    polyline_spans(vertices.len(), closed, &bulge_at)
        .collect::<Vec<_>>()
        .into_iter()
        .map(move |(from, to)| bulge_primitive(vertices[from], vertices[to], bulge_at(from)))
}

#[cfg(test)]
mod tests {
    use std::f64::consts::{FRAC_PI_2, PI};

    use proptest::prelude::*;

    use super::*;

    fn p(x: f64, y: f64) -> Point2 {
        Point2::new(x, y)
    }

    fn close(a: Point2, b: Point2) -> bool {
        a.distance_to(b) < 1e-9 * (1.0 + a.distance_to(Point2::ORIGIN))
    }

    fn square() -> Vec<Point2> {
        vec![p(0.0, 0.0), p(10.0, 0.0), p(10.0, 10.0), p(0.0, 10.0)]
    }

    fn straight(vertices: &[Point2], closed: bool) -> Vec<Primitive> {
        polyline_primitives(vertices, |_| 0.0, closed).collect()
    }

    #[test]
    fn an_open_polyline_has_one_segment_fewer_than_vertices() {
        assert_eq!(straight(&square(), false).len(), 3);
    }

    #[test]
    fn a_closed_polyline_adds_the_segment_back_to_the_start() {
        let segments = straight(&square(), true);
        assert_eq!(segments.len(), 4);
        assert_eq!(
            segments[3],
            Primitive::Segment(Segment::new(p(0.0, 10.0), p(0.0, 0.0)))
        );
    }

    #[test]
    fn degenerate_polylines_have_no_spurious_segments() {
        assert_eq!(straight(&[], true).len(), 0);
        assert_eq!(straight(&[p(1.0, 1.0)], true).len(), 0);
        // Two vertices closed is still one segment, not a doubled one.
        assert_eq!(straight(&[p(0.0, 0.0), p(1.0, 0.0)], true).len(), 1);
    }

    /// Regression (found by the offset corpus): a circle drawn as two
    /// semicircles is a closed polyline of two vertices. Its second arc is
    /// the closing segment, and it must not be dropped as a doubled line.
    #[test]
    fn a_closed_polyline_of_two_vertices_closes_when_it_has_an_arc() {
        let ends = [p(0.0, 0.0), p(100.0, 0.0)];
        let circle: Vec<Primitive> = polyline_primitives(&ends, |_| 1.0, true).collect();
        assert_eq!(circle.len(), 2, "both semicircles");
        let (Primitive::Arc(lower), Primitive::Arc(upper)) = (circle[0], circle[1]) else {
            panic!("two arcs");
        };
        assert!(
            close(lower.mid_point(), p(50.0, -50.0)) && close(upper.mid_point(), p(50.0, 50.0))
        );

        // A lens: one arc and its chord.
        let bulges = [0.5, 0.0];
        assert_eq!(
            polyline_primitives(&ends, |index| bulges[index], true).count(),
            2
        );
        // Two straight segments would be the same line twice: still one.
        assert_eq!(polyline_primitives(&ends, |_| 0.0, true).count(), 1);
        assert!(polyline_closes(2, true, |_| 1.0) && !polyline_closes(2, true, |_| 0.0));
        assert!(polyline_closes(3, true, |_| 0.0) && !polyline_closes(3, false, |_| 1.0));
    }

    #[test]
    fn a_zero_bulge_is_a_straight_segment() {
        for bulge in [0.0, -0.0, 1e-15, f64::NAN, f64::INFINITY] {
            assert_eq!(
                bulge_primitive(p(1.0, 2.0), p(4.0, 6.0), bulge),
                Primitive::Segment(Segment::new(p(1.0, 2.0), p(4.0, 6.0))),
                "bulge {bulge}"
            );
        }
        assert_eq!(
            arc_from_bulge(p(1.0, 2.0), p(1.0, 2.0), 1.0),
            None,
            "no chord"
        );
    }

    #[test]
    fn a_bulge_of_one_is_a_semicircle_on_the_right_of_the_way_travelled() {
        // From (0,0) to (2,0), counter-clockwise: round the centre (1,0),
        // dipping below the chord.
        let arc = arc_from_bulge(p(0.0, 0.0), p(2.0, 0.0), 1.0).unwrap();
        assert!(close(arc.center, p(1.0, 0.0)));
        assert!((arc.radius - 1.0).abs() < 1e-12 && (arc.sweep - PI).abs() < 1e-12);
        assert!(close(arc.start_point(), p(0.0, 0.0)) && close(arc.end_point(), p(2.0, 0.0)));
        assert!(close(arc.mid_point(), p(1.0, -1.0)));
    }

    #[test]
    fn a_negative_bulge_turns_clockwise_and_bulges_to_the_left() {
        let arc = arc_from_bulge(p(0.0, 0.0), p(2.0, 0.0), -1.0).unwrap();
        assert!(close(arc.mid_point(), p(1.0, 1.0)), "above the chord");
        // Stored counter-clockwise, so it starts where the segment ends.
        assert!(close(arc.start_point(), p(2.0, 0.0)) && close(arc.end_point(), p(0.0, 0.0)));
    }

    #[test]
    fn a_quarter_arc_has_the_familiar_bulge() {
        // tan(90° / 4) = √2 − 1.
        let bulge = 2f64.sqrt() - 1.0;
        let arc = arc_from_bulge(p(10.0, 0.0), p(0.0, 10.0), bulge).unwrap();
        assert!(close(arc.center, p(0.0, 0.0)));
        assert!((arc.radius - 10.0).abs() < 1e-9 && (arc.sweep - FRAC_PI_2).abs() < 1e-12);
        assert!((bulge_from_arc(&arc, true) - bulge).abs() < 1e-12);
        assert!((bulge_from_arc(&arc, false) + bulge).abs() < 1e-12);
    }

    #[test]
    fn a_bulge_above_one_is_more_than_a_semicircle() {
        // Three quarters of a circle: tan(270° / 4) = 1 + √2.
        let arc = arc_from_bulge(p(10.0, 0.0), p(0.0, -10.0), 1.0 + 2f64.sqrt()).unwrap();
        assert!(close(arc.center, p(0.0, 0.0)));
        assert!((arc.sweep - 3.0 * FRAC_PI_2).abs() < 1e-12);
        assert!(close(arc.mid_point(), p(-50f64.sqrt(), 50f64.sqrt())));
    }

    #[test]
    fn a_polyline_mixes_straight_and_arc_segments_in_order() {
        // A slot: two straight sides joined by two semicircular ends.
        let vertices = [p(0.0, 0.0), p(10.0, 0.0), p(10.0, 4.0), p(0.0, 4.0)];
        let bulges = [0.0, 1.0, 0.0, 1.0];
        let slot: Vec<Primitive> =
            polyline_primitives(&vertices, |index| bulges[index], true).collect();
        assert_eq!(slot.len(), 4);
        assert!(
            matches!(slot[0], Primitive::Segment(_)) && matches!(slot[2], Primitive::Segment(_))
        );
        let (Primitive::Arc(right), Primitive::Arc(left)) = (slot[1], slot[3]) else {
            panic!("the ends are arcs");
        };
        assert!(close(right.center, p(10.0, 2.0)) && close(right.mid_point(), p(12.0, 2.0)));
        assert!(close(left.center, p(0.0, 2.0)) && close(left.mid_point(), p(-2.0, 2.0)));
    }

    #[test]
    fn an_arc_continues_from_a_tangent_direction() {
        let east = Vec2::new(1.0, 0.0);
        // Up and to the right from the origin, leaving eastwards: a quarter
        // turn counter-clockwise about (0, 10).
        let bulge = bulge_from_tangent(p(0.0, 0.0), east, p(10.0, 10.0)).unwrap();
        assert!((bulge - (2f64.sqrt() - 1.0)).abs() < 1e-12);
        let arc = arc_from_bulge(p(0.0, 0.0), p(10.0, 10.0), bulge).unwrap();
        assert!(close(arc.center, p(0.0, 10.0)));

        // Down and to the right: the mirror image, clockwise.
        let bulge = bulge_from_tangent(p(0.0, 0.0), east, p(10.0, -10.0)).unwrap();
        assert!((bulge + (2f64.sqrt() - 1.0)).abs() < 1e-12);
        // Straight to the side: a semicircle.
        assert!((bulge_from_tangent(p(0.0, 0.0), east, p(0.0, 6.0)).unwrap() - 1.0).abs() < 1e-12);
        // Straight ahead: no arc needed.
        assert_eq!(
            bulge_from_tangent(p(0.0, 0.0), east, p(7.0, 0.0)),
            Some(0.0)
        );
    }

    #[test]
    fn no_arc_leaves_a_point_and_comes_straight_back() {
        let east = Vec2::new(1.0, 0.0);
        assert_eq!(bulge_from_tangent(p(0.0, 0.0), east, p(-5.0, 0.0)), None);
        assert_eq!(bulge_from_tangent(p(0.0, 0.0), east, p(0.0, 0.0)), None);
        assert_eq!(
            bulge_from_tangent(p(0.0, 0.0), Vec2::new(0.0, 0.0), p(1.0, 1.0)),
            None
        );
    }

    #[test]
    fn the_end_tangent_of_a_straight_segment_is_its_direction() {
        assert_eq!(
            bulge_end_tangent(p(1.0, 1.0), p(4.0, 5.0), 0.0),
            Vec2::new(3.0, 4.0)
        );
        // A counter-clockwise semicircle from (0,0) to (2,0) arrives heading north.
        let arriving = bulge_end_tangent(p(0.0, 0.0), p(2.0, 0.0), 1.0);
        assert!(arriving.x.abs() < 1e-12 && arriving.y > 0.0);
    }

    fn coordinate() -> impl Strategy<Value = f64> {
        -1e3..1e3f64
    }

    fn bulge() -> impl Strategy<Value = f64> {
        prop_oneof![0.01..6.0f64, -6.0..-0.01f64]
    }

    proptest! {
        /// #114: bulge → arc → bulge round-trips, and the arc is the one the
        /// bulge describes.
        #[test]
        fn a_bulge_and_its_arc_describe_each_other(
            ax in coordinate(), ay in coordinate(), bx in coordinate(), by in coordinate(), bulge in bulge(),
        ) {
            let (start, end) = (p(ax, ay), p(bx, by));
            let chord = start.distance_to(end);
            prop_assume!(chord > 1e-3);
            let arc = arc_from_bulge(start, end, bulge).unwrap();
            let scale = 1.0 + arc.radius;

            // It runs between the two vertices, in the direction of the sign.
            let (from, to) = if bulge > 0.0 { (start, end) } else { (end, start) };
            prop_assert!(arc.start_point().distance_to(from) < 1e-9 * scale);
            prop_assert!(arc.end_point().distance_to(to) < 1e-7 * scale);
            // The included angle and the sagitta are what the definition says.
            prop_assert!((arc.sweep - 4.0 * bulge.abs().atan()).abs() < 1e-12);
            let middle = start + (end - start) * 0.5;
            let sagitta = arc.mid_point().distance_to(middle);
            prop_assert!((sagitta - bulge.abs() * chord / 2.0).abs() < 1e-7 * scale);
            // And back again.
            let back = bulge_from_arc(&arc, bulge > 0.0);
            prop_assert!((back - bulge).abs() < 1e-9 * (1.0 + bulge.abs()));
        }

        /// The arc bulges to the right of the way travelled when it turns
        /// counter-clockwise, to the left when clockwise.
        #[test]
        fn the_sign_of_the_bulge_chooses_the_side(
            ax in coordinate(), ay in coordinate(), bx in coordinate(), by in coordinate(), bulge in bulge(),
        ) {
            let (start, end) = (p(ax, ay), p(bx, by));
            prop_assume!(start.distance_to(end) > 1e-3);
            let arc = arc_from_bulge(start, end, bulge).unwrap();
            let along = end - start;
            let to_middle = arc.mid_point() - start;
            let side = along.x * to_middle.y - along.y * to_middle.x;
            prop_assert!(side * bulge < 0.0, "side {side}, bulge {bulge}");
        }

        /// The arc that continues from a tangent really leaves along it, and
        /// the tangent it arrives with is the one a further arc would
        /// continue from: chaining the two functions is consistent.
        #[test]
        fn tangent_continuation_is_smooth(
            ax in coordinate(), ay in coordinate(), bx in coordinate(), by in coordinate(),
            degrees in 0.0..360f64,
        ) {
            let (start, end) = (p(ax, ay), p(bx, by));
            prop_assume!(start.distance_to(end) > 1e-2);
            let tangent = crate::unit_at_degrees(degrees);
            let Some(bulge) = bulge_from_tangent(start, tangent, end) else {
                return Ok(()); // straight behind
            };
            prop_assume!(bulge.abs() > 1e-3 && bulge.abs() < 50.0);
            let arc = arc_from_bulge(start, end, bulge).unwrap();

            // At the start, the radius is perpendicular to the tangent…
            let radius = start - arc.center;
            prop_assert!(radius.dot(tangent).abs() < 1e-6 * arc.radius);
            // …and travel is along it, not against it: counter-clockwise for
            // a positive bulge means turning left of the radius.
            let travel = if bulge > 0.0 {
                Vec2::new(-radius.y, radius.x)
            } else {
                Vec2::new(radius.y, -radius.x)
            };
            prop_assert!(travel.dot(tangent) > 0.0);

            // Arriving, the tangent is perpendicular to the radius at the end.
            let arriving = bulge_end_tangent(start, end, bulge);
            let end_radius = end - arc.center;
            let scale = arc.radius * start.distance_to(end);
            prop_assert!(end_radius.dot(arriving).abs() < 1e-6 * scale);
            let end_travel = if bulge > 0.0 {
                Vec2::new(-end_radius.y, end_radius.x)
            } else {
                Vec2::new(end_radius.y, -end_radius.x)
            };
            prop_assert!(end_travel.dot(arriving) > 0.0);
        }
    }
}

//! Property-based stress tests on near-degenerate input (DESIGN §21, §31,
//! §41): nearly parallel lines, nearly tangent curves, cuts a hair apart,
//! very large and very small coordinates.
//!
//! Each test states an invariant of one operation and holds it over input
//! chosen to sit where the arithmetic is weakest. None may panic.
//!
//! **What "holds" means far from the origin.** The geometric tolerance is
//! absolute (10⁻⁹), but a coordinate of size 10⁷ cannot be told from its
//! neighbour 2 × 10⁻⁹ away. So invariants are checked to the tolerance *or*
//! to the resolution of the numbers involved, whichever is coarser — see
//! [`Frame::slack`]. Within about 10⁶ units of the origin that is the
//! tolerance itself; beyond, results degrade in proportion and never fail
//! outright.
//!
//! The default run is quick. `scripts/stress.sh` runs the same tests with
//! many more cases.

use std::f64::consts::{PI, TAU};

use cad_core::tolerance::LinearTolerance;
use cad_core::{Point2, Vec2};
use cad_geometry::intersect::{circle_circle, line_circle, line_line, segment_segment};
use cad_geometry::{
    Arc, ArcJoinError, Circle, Corner, JoinedArcs, Path, PathSpan, Primitive, Segment, break_arc,
    break_segment, distance_to_line, extend_segment, fillet_curves, join_arcs, join_segments,
    offset_arc, offset_circle, offset_path, offset_segment, path_spans, trim_arc_parts,
    trim_circle_parts, trim_segment_parts,
};
use proptest::prelude::*;

const TOLERANCE: LinearTolerance = LinearTolerance::DEFAULT;

/// Where a case sits and how big it is. Cases are described in a local
/// frame — features tens of units across, near the origin — and placed.
#[derive(Clone, Copy, Debug)]
struct Frame {
    origin: Point2,
    scale: f64,
}

impl Frame {
    fn place(&self, x: f64, y: f64) -> Point2 {
        self.origin + Vec2::new(x, y) * self.scale
    }

    fn length(&self, local: f64) -> f64 {
        local * self.scale
    }

    /// How exactly anything can be known in this frame, for features up to
    /// `extent` local units from its origin: the linear tolerance, or a few
    /// thousand spacings of the floating-point numbers at that range if
    /// that is coarser.
    fn slack(&self, extent: f64) -> f64 {
        let reach = self.origin.x.abs().max(self.origin.y.abs()) + extent.abs() * self.scale;
        TOLERANCE.value().max(reach * f64::EPSILON * 4096.0)
    }
}

/// Frames from the comfortable to the extreme: at the origin or up to ten
/// million units from it, at a thousandth of the usual size or a thousand
/// times it.
fn frame() -> impl Strategy<Value = Frame> {
    let offset = || {
        prop_oneof![
            2 => Just(0.0),
            1 => -1e3..1e3f64,
            1 => -1e7..1e7f64,
        ]
    };
    let scale = prop_oneof![
        2 => Just(1.0),
        1 => 1e-3..1.0f64,
        1 => 1.0..1e3f64,
    ];
    (offset(), offset(), scale).prop_map(|(x, y, scale)| Frame {
        origin: Point2::new(x, y),
        scale,
    })
}

/// A quantity that is almost nothing: its size spread evenly over many
/// orders of magnitude, with either sign.
fn hair() -> impl Strategy<Value = f64> {
    (-15.0..-5.0f64, any::<bool>()).prop_map(|(exponent, negative)| {
        let size = 10f64.powf(exponent);
        if negative { -size } else { size }
    })
}

fn unit(angle: f64) -> Vec2 {
    Vec2::new(angle.cos(), angle.sin())
}

fn length_of(primitive: &Primitive) -> f64 {
    match primitive {
        Primitive::Segment(segment) => segment.length(),
        Primitive::Arc(arc) => arc.radius * arc.sweep,
        Primitive::Circle(circle) => TAU * circle.radius,
    }
}

proptest! {
    // ── intersections ──

    /// Two lines a hair from parallel either have no intersection or one
    /// that is on both of them.
    #[test]
    fn nearly_parallel_lines_meet_on_both_or_not_at_all(
        frame in frame(), direction in 0.0..TAU, tilt in hair(), shift in -1.0..1.0f64, gap in hair(),
    ) {
        let along = unit(direction);
        let across = Vec2::new(-along.y, along.x);
        let middle = frame.place(0.0, 0.0);
        let a = Segment::new(middle - along * frame.length(100.0), middle + along * frame.length(100.0));
        let other = unit(direction + tilt);
        let through = middle + along * frame.length(100.0 * shift) + across * frame.length(gap);
        let b = Segment::new(through - other * frame.length(100.0), through + other * frame.length(100.0));

        let slack = frame.slack(200.0);
        if let Some(point) = segment_segment(&a, &b) {
            prop_assert!(point.is_finite());
            prop_assert!(a.distance_to_point(point) <= slack, "off a by {}", a.distance_to_point(point));
            prop_assert!(b.distance_to_point(point) <= slack, "off b by {}", b.distance_to_point(point));
        }
        // The infinite lines may meet far away; wherever it is, it is a point.
        if let Some(point) = line_line(&a, &b) {
            prop_assert!(point.is_finite());
        }
    }

    /// Plain crossings are found however far from the origin they are —
    /// out to ten thousand million units, where neighbouring numbers are
    /// two micrometres apart. (Regression family, ADR-0012: checked against
    /// an absolute tolerance, one line–circle crossing in ten was missed at
    /// 10⁷ and one in three at 10¹⁰.)
    #[test]
    fn crossings_are_found_however_far_from_the_origin(
        x in -1e10..1e10f64, y in -1e10..1e10f64,
        first in 0.0..TAU, opening in 0.3..(PI - 0.3),
        reach in proptest::array::uniform4(20.0..100f64), radius in 5.0..19f64,
    ) {
        let crossing = Point2::new(x, y);
        let (a, b) = (unit(first), unit(first + opening));
        let one = Segment::new(crossing - a * reach[0], crossing + a * reach[1]);
        let other = Segment::new(crossing - b * reach[2], crossing + b * reach[3]);
        let resolution = TOLERANCE.around(&[crossing]).value() * 64.0;

        let met = segment_segment(&one, &other);
        prop_assert!(met.is_some(), "two segments that cross were not found to");
        prop_assert!(met.unwrap().distance_to(crossing) <= resolution);

        // Each segment runs right through a circle about the crossing.
        let circle = Primitive::Circle(Circle::new(crossing, radius));
        for segment in [one, other] {
            let hits = Primitive::Segment(segment).intersect(&circle);
            prop_assert_eq!(hits.len(), 2, "a segment through a circle crosses it twice");
            for hit in hits.as_slice() {
                prop_assert!((hit.distance_to(crossing) - radius).abs() <= resolution);
            }
            // And through an arc that is all of that circle but a sliver.
            let arc = Primitive::Arc(Arc::new(crossing, radius, first + opening / 2.0 + 0.05, TAU - 0.1));
            prop_assert_eq!(Primitive::Segment(segment).intersect(&arc).len(), 2);
        }
    }

    /// A line a hair from tangent to a circle: every hit is on both.
    #[test]
    fn a_nearly_tangent_line_hits_the_circle_on_the_circle(
        frame in frame(), radius in 1.0..100f64, direction in 0.0..TAU, miss in hair(),
    ) {
        let centre = frame.place(0.0, 0.0);
        let radius = frame.length(radius);
        let outwards = unit(direction);
        let along = Vec2::new(-outwards.y, outwards.x);
        // Relative: the line passes at `radius × (1 + miss)` from the centre.
        let foot = centre + outwards * (radius * (1.0 + miss));
        let line = Segment::new(foot - along * (3.0 * radius), foot + along * (3.0 * radius));
        let circle = Circle::new(centre, radius);

        let slack = frame.slack(400.0);
        let hits = line_circle(&line, &circle);
        prop_assert!(hits.len() <= 2);
        for hit in hits.as_slice() {
            prop_assert!(hit.is_finite());
            prop_assert!(circle.distance_to_point(*hit) <= slack, "off the circle by {}", circle.distance_to_point(*hit));
            prop_assert!(distance_to_line(&line, *hit).unwrap_or(f64::NAN) <= slack);
        }
        // Clearly crossing or clearly missing is never in doubt.
        if miss < -1e-6 {
            prop_assert_eq!(hits.len(), 2);
        }
        if miss > 1e-6 && radius * miss > slack {
            prop_assert_eq!(hits.len(), 0);
        }
    }

    /// Two circles a hair from touching, outside or inside: every hit is on
    /// both, and it does not matter which circle is named first.
    #[test]
    fn nearly_tangent_circles_meet_on_both_whichever_is_first(
        frame in frame(), first in 1.0..100f64, second in 1.0..100f64,
        direction in 0.0..TAU, miss in hair(), inside in any::<bool>(),
    ) {
        let a = Circle::new(frame.place(0.0, 0.0), frame.length(first));
        let radius = frame.length(second);
        let touching = if inside { (a.radius - radius).abs() } else { a.radius + radius };
        prop_assume!(touching > frame.length(0.5));
        let b = Circle::new(a.center + unit(direction) * (touching * (1.0 + miss)), radius);

        let slack = frame.slack(400.0);
        let (forward, backward) = (circle_circle(&a, &b), circle_circle(&b, &a));
        prop_assert_eq!(forward.len(), backward.len());
        for hit in forward.as_slice().iter().chain(backward.as_slice()) {
            prop_assert!(hit.is_finite());
            prop_assert!(a.distance_to_point(*hit) <= slack, "off a by {}", a.distance_to_point(*hit));
            prop_assert!(b.distance_to_point(*hit) <= slack, "off b by {}", b.distance_to_point(*hit));
        }
    }

    /// A small circle centred on a very large one — a nearly straight arc —
    /// always crosses it twice, and each crossing is exactly the small
    /// radius from the small centre. (Regression family: this is where
    /// `circle_circle` used to lose micrometres.)
    #[test]
    fn a_small_circle_on_a_huge_one_crosses_it_twice(
        large in 1e3..1e8f64, small in 0.1..100f64, at in 0.0..TAU, x in -1e3..1e3f64, y in -1e3..1e3f64,
    ) {
        let big = Circle::new(Point2::new(x, y), large);
        let centre = big.point_at(at);
        let little = Circle::new(centre, small);
        // The big circle's own points are only known to its float spacing.
        let slack = TOLERANCE.value().max(large * f64::EPSILON * 64.0);
        for hits in [circle_circle(&big, &little), circle_circle(&little, &big)] {
            prop_assert_eq!(hits.len(), 2);
            for hit in hits.as_slice() {
                prop_assert!((hit.distance_to(centre) - small).abs() <= slack);
                prop_assert!(big.distance_to_point(*hit) <= slack);
            }
        }
    }

    // ── OFFSET ──

    /// Offsetting a line, circle, or arc and then offsetting the result
    /// back gives the object it started from.
    #[test]
    fn offsetting_there_and_back_is_the_identity(
        frame in frame(), direction in 0.0..TAU, radius in 5.0..100f64,
        distance in 0.01..50f64, start in 0.0..TAU, sweep in 0.1..6.0f64, left in any::<bool>(),
    ) {
        let slack = frame.slack(400.0);
        let centre = frame.place(0.0, 0.0);
        let distance = frame.length(distance);

        let along = unit(direction);
        let across = Vec2::new(-along.y, along.x) * if left { 1.0 } else { -1.0 };
        let line = Segment::new(centre - along * frame.length(50.0), centre + along * frame.length(50.0));
        let there = offset_segment(&line, distance, centre + across * frame.length(10.0));
        let there = there.expect("a side was chosen");
        prop_assert!((distance_to_line(&line, there.start).unwrap() - distance).abs() <= slack);
        prop_assert!((there.length() - line.length()).abs() <= slack);
        let back = offset_segment(&there, distance, centre).expect("the line is on one side");
        prop_assert!(back.start.distance_to(line.start) <= slack && back.end.distance_to(line.end) <= slack);

        let radius = frame.length(radius);
        let circle = Circle::new(centre, radius);
        let outside = centre + along * (radius + distance + frame.length(1.0));
        let outer = offset_circle(&circle, distance, outside, TOLERANCE).expect("outwards always works");
        prop_assert!((outer.radius - radius - distance).abs() <= slack);
        let back = offset_circle(&outer, distance, centre, TOLERANCE).expect("it is larger than the distance");
        prop_assert!((back.radius - radius).abs() <= slack && back.center.distance_to(centre) <= slack);

        let arc = Arc::new(centre, radius, start, sweep);
        let outer = offset_arc(&arc, distance, outside, TOLERANCE).expect("outwards always works");
        let back = offset_arc(&outer, distance, centre, TOLERANCE).expect("it is larger than the distance");
        prop_assert!((back.radius - radius).abs() <= slack);
        prop_assert_eq!((back.start, back.sweep), (arc.start, arc.sweep), "the angles are untouched");
    }

    /// A polyline whose arcs are nearly straight — radii up to ten million
    /// — offsets like any other: every point of the result is the offset
    /// distance from it. (Regression family: both polyline-offset bugs
    /// found so far were intersections with arcs like these.)
    #[test]
    fn a_polyline_of_nearly_straight_arcs_offsets_true(
        points in proptest::collection::vec((-200i32..200, -200i32..200), 3..7),
        bulges in proptest::collection::vec(
            prop_oneof![
                2 => Just(0.0),
                3 => (-5.0..-2.0f64, any::<bool>())
                    .prop_map(|(exponent, negative)| 10f64.powf(exponent) * if negative { -1.0 } else { 1.0 }),
                1 => -1.5..1.5f64,
            ],
            7,
        ),
        closed in any::<bool>(), distance in 1.0..40f64, left in any::<bool>(),
    ) {
        let mut vertices: Vec<Point2> = Vec::new();
        for (x, y) in points {
            let point = Point2::new(f64::from(x), f64::from(y));
            if vertices.iter().all(|other| other.distance_to(point) > 5.0) {
                vertices.push(point);
            }
        }
        prop_assume!(vertices.len() >= 3);
        let path = Path {
            bulges: bulges[..vertices.len()].to_vec(),
            vertices,
            closed,
        };
        let source: Vec<Primitive> = path_spans(&path).iter().map(PathSpan::primitive).collect();

        let offsets = offset_path(&path, if left { distance } else { -distance });
        for span in offsets.iter().flat_map(path_spans) {
            prop_assert!(span.start.is_finite() && span.end.is_finite() && span.bulge.is_finite());
            for step in 0..=10 {
                let at = span.point_at(f64::from(step) / 10.0);
                let nearest = source
                    .iter()
                    .map(|primitive| primitive.distance_to_point(at))
                    .fold(f64::INFINITY, f64::min);
                prop_assert!(
                    (nearest - distance).abs() < 1e-5,
                    "{:?} is {} from the source, not {}", at, nearest, distance
                );
            }
        }
    }

    // ── TRIM ──

    /// However close together the cuts are, and however close to the ends,
    /// a trimmed line is the original with exactly one stretch removed: the
    /// stretch nearest the pick.
    #[test]
    fn a_trimmed_line_is_the_line_less_the_stretch_at_the_pick(
        frame in frame(), direction in 0.0..TAU,
        cuts in proptest::collection::vec((0.0..1.0f64, proptest::option::of(hair())), 1..5),
        pick in 0.0..1.0f64,
    ) {
        let along = unit(direction);
        let start = frame.place(0.0, 0.0);
        let line = Segment::new(start, start + along * frame.length(100.0));
        let at = |t: f64| start + along * (frame.length(100.0) * t.clamp(0.0, 1.0));
        // Some cuts sit a hair from another, or from an end.
        let cut_points: Vec<Point2> = cuts
            .iter()
            .flat_map(|(t, twin)| std::iter::once(at(*t)).chain(twin.map(|hair| at(*t + hair))))
            .collect();
        let slack = frame.slack(200.0);

        if let Some(parts) = trim_segment_parts(&line, &cut_points, at(pick), TOLERANCE) {
            let kept: f64 = parts.kept.iter().map(Segment::length).sum();
            prop_assert!((kept + parts.removed.length() - line.length()).abs() <= slack);
            prop_assert!(parts.removed.length() > 0.0);
            prop_assert!(parts.removed.distance_to_point(at(pick)) <= slack, "the pick is in what was removed");
            for piece in &parts.kept {
                prop_assert!(piece.length() > 0.0, "no empty pieces");
                for end in [piece.start, piece.end] {
                    prop_assert!(line.distance_to_point(end) <= slack);
                }
            }
        }
    }

    /// The same for an arc and a circle, with cuts given as angles.
    #[test]
    fn a_trimmed_arc_or_circle_is_itself_less_the_stretch_at_the_pick(
        frame in frame(), radius in 1.0..100f64, start in 0.0..TAU, sweep in 0.2..6.0f64,
        cuts in proptest::collection::vec((0.0..1.0f64, proptest::option::of(hair())), 1..5),
        pick in 0.0..1.0f64,
    ) {
        let centre = frame.place(0.0, 0.0);
        let radius = frame.length(radius);
        let slack = frame.slack(200.0);

        let arc = Arc::new(centre, radius, start, sweep);
        let on_arc = |t: f64| arc.point_at(start + sweep * t.clamp(0.0, 1.0));
        let cut_points: Vec<Point2> = cuts
            .iter()
            .flat_map(|(t, twin)| std::iter::once(on_arc(*t)).chain(twin.map(|hair| on_arc(*t + hair))))
            .collect();
        if let Some(parts) = trim_arc_parts(&arc, &cut_points, on_arc(pick), TOLERANCE) {
            let kept: f64 = parts.kept.iter().map(|piece| piece.radius * piece.sweep).sum();
            let removed = parts.removed.radius * parts.removed.sweep;
            prop_assert!((kept + removed - radius * sweep).abs() <= slack * 4.0, "{} + {} of {}", kept, removed, radius * sweep);
            prop_assert!(parts.removed.distance_to_point(on_arc(pick)) <= slack);
            for piece in &parts.kept {
                prop_assert!(piece.sweep > 0.0 && piece.sweep <= sweep + 1e-9);
                prop_assert!(arc.distance_to_point(piece.start_point()) <= slack);
                prop_assert!(arc.distance_to_point(piece.end_point()) <= slack);
            }
        }

        let circle = Circle::new(centre, radius);
        let on_circle = |t: f64| circle.point_at(TAU * t);
        let cut_points: Vec<Point2> = cuts
            .iter()
            .flat_map(|(t, twin)| std::iter::once(on_circle(*t)).chain(twin.map(|hair| on_circle(*t + hair))))
            .collect();
        if let Some(parts) = trim_circle_parts(&circle, &cut_points, on_circle(pick), TOLERANCE) {
            let kept: f64 = parts.kept.iter().map(|piece| piece.sweep).sum();
            prop_assert_eq!(parts.kept.len(), 1, "an opened circle is one arc");
            prop_assert!((kept + parts.removed.sweep - TAU).abs() <= 1e-9, "{} + {}", kept, parts.removed.sweep);
            prop_assert!(parts.removed.distance_to_point(on_circle(pick)) <= slack);
        }
    }

    // ── EXTEND ──

    /// A line extended to a boundary — at any angle, down to a hair from
    /// parallel — keeps its other end, still passes through its old end,
    /// and ends on the boundary.
    #[test]
    fn an_extended_line_ends_on_the_boundary_and_keeps_its_other_end(
        frame in frame(), direction in 0.0..TAU, ahead in 1.0..200f64,
        slant in prop_oneof![0.05..(PI - 0.05), hair().prop_map(f64::abs), hair().prop_map(|h| PI - h.abs())],
    ) {
        let along = unit(direction);
        let start = frame.place(0.0, 0.0);
        let line = Segment::new(start, start + along * frame.length(50.0));
        // A long boundary crossing the line's path `ahead` beyond its end.
        let crossing = line.end + along * frame.length(ahead);
        let across = unit(direction + slant);
        let boundary = Primitive::Segment(Segment::new(
            crossing - across * frame.length(1e4),
            crossing + across * frame.length(1e4),
        ));
        let slack = frame.slack(2e4);

        if let Some(extended) = extend_segment(&line, true, &[boundary], TOLERANCE) {
            prop_assert_eq!(extended.start, line.start, "the other end is untouched");
            prop_assert!(extended.end.is_finite());
            prop_assert!(extended.distance_to_point(line.end) <= slack, "it still passes through its old end");
            prop_assert!(extended.length() >= line.length() - slack, "it never gets shorter");
            // On the boundary — to the precision a nearly parallel crossing allows.
            let off = boundary.distance_to_point(extended.end);
            prop_assert!(off <= slack / slant.sin().abs().max(1e-12), "{} off the boundary", off);
        }
    }

    // ── FILLET and CHAMFER ──

    /// A chamfer cuts each line back by its own distance from the corner,
    /// and the bevel joins exactly those two points.
    #[test]
    fn a_chamfer_sets_each_line_back_by_its_own_distance(
        frame in frame(), direction in 0.0..TAU, opening in 0.2..(PI - 0.2),
        first in 0.0..40f64, second in 0.0..40f64,
    ) {
        let apex = frame.place(0.0, 0.0);
        let (a, b) = (unit(direction), unit(direction + opening));
        let leg = frame.length(100.0);
        let lines = [Segment::new(apex, apex + a * leg), Segment::new(apex, apex + b * leg)];
        let picks = [apex + a * (leg * 0.9), apex + b * (leg * 0.9)];
        let distances = [frame.length(first), frame.length(second)];
        let slack = frame.slack(200.0);

        let corner = Corner::new(&lines[0], picks[0], &lines[1], picks[1], TOLERANCE).expect("the lines meet");
        prop_assert!(corner.apex().distance_to(apex) <= slack);
        let chamfer = corner.chamfer(distances, TOLERANCE).expect("the lines are long enough");
        for (index, line) in chamfer.lines.iter().enumerate() {
            let joint = if line.start.distance_to(picks[index]) > line.end.distance_to(picks[index]) {
                line.start
            } else {
                line.end
            };
            prop_assert!((joint.distance_to(apex) - distances[index]).abs() <= slack);
            prop_assert!(lines[index].distance_to_point(joint) <= slack);
        }
        if let Some(bevel) = chamfer.bevel {
            prop_assert!((bevel.start.distance_to(apex) - distances[0]).abs() <= slack);
            prop_assert!((bevel.end.distance_to(apex) - distances[1]).abs() <= slack);
        }
    }

    /// Lines a hair from parallel are either refused or filleted with an
    /// arc that really is tangent to both. Nothing in between.
    #[test]
    fn nearly_parallel_lines_are_refused_or_properly_filleted(
        frame in frame(), direction in 0.0..TAU, tilt in hair(), gap in 0.5..20f64, radius in 0.0..30f64,
    ) {
        let along = unit(direction);
        let across = Vec2::new(-along.y, along.x);
        let base = frame.place(0.0, 0.0);
        let leg = frame.length(100.0);
        let a = Segment::new(base, base + along * leg);
        let other = unit(direction + tilt);
        let lifted = base + across * frame.length(gap);
        let b = Segment::new(lifted, lifted + other * leg);
        let slack = frame.slack(1e3);

        let corner = Corner::new(&a, a.end, &b, b.end, TOLERANCE);
        if let Ok(fillet) = corner.and_then(|corner| corner.fillet(frame.length(radius), TOLERANCE)) {
            for line in fillet.lines {
                prop_assert!(line.start.is_finite() && line.end.is_finite());
            }
            if let Some(arc) = fillet.arc {
                prop_assert!(arc.center.is_finite() && arc.radius.is_finite());
                // Lines this close to parallel meet a long way off, and the
                // arc is out there with them: it is judged at that range.
                let there = TOLERANCE.around(&[arc.center]).value() * 512.0;
                for line in [&a, &b] {
                    let away = distance_to_line(line, arc.center).unwrap_or(f64::NAN);
                    prop_assert!(
                        (away - arc.radius).abs() <= slack.max(there).max(arc.radius * 1e-9),
                        "{} from a line, radius {}", away, arc.radius
                    );
                }
            }
        }
    }

    /// A fillet between a line and a circle it nearly touches: refused, or
    /// an arc of the radius asked for, tangent to both.
    #[test]
    fn a_fillet_at_a_near_tangency_is_tangent_or_refused(
        frame in frame(), circle_radius in 5.0..100f64, miss in hair(), radius in 0.5..20f64, side in -0.9..0.9f64,
    ) {
        let centre = frame.place(0.0, 0.0);
        let big = frame.length(circle_radius);
        let radius = frame.length(radius);
        // A horizontal line passing a hair above or below the top of the circle.
        let height = big * (1.0 + miss);
        let line = Segment::new(
            centre + Vec2::new(-3.0 * big, height),
            centre + Vec2::new(3.0 * big, height),
        );
        let circle = Circle::new(centre, big);
        let slack = frame.slack(400.0);

        let fillet = fillet_curves(
            &Primitive::Segment(line),
            centre + Vec2::new(2.5 * big * side.signum(), height),
            &Primitive::Circle(circle),
            circle.point_at(PI / 2.0 - side),
            radius,
            TOLERANCE,
        );
        if let Ok(fillet) = fillet
            && let Some(arc) = fillet.arc
        {
            prop_assert!((arc.radius - radius).abs() <= slack);
            let to_line = distance_to_line(&line, arc.center).unwrap_or(f64::NAN);
            prop_assert!((to_line - radius).abs() <= slack * 4.0, "{} from the line", to_line);
            let between = arc.center.distance_to(centre);
            let tangent = (between - (big + radius)).abs().min((between - (big - radius).abs()).abs());
            prop_assert!(tangent <= slack * 4.0, "{} from tangent to the circle", tangent);
        }
    }

    // ── BREAK and JOIN ──

    /// Breaking removes exactly the stretch between the two points, however
    /// close they are to each other or to an end — and joining the pieces
    /// of a break at one point gives the object back.
    #[test]
    fn breaking_removes_the_stretch_between_and_joining_undoes_a_split(
        frame in frame(), direction in 0.0..TAU, radius in 1.0..100f64, sweep in 0.2..6.0f64,
        first in 0.0..1.0f64, second in prop_oneof![(0.0..1.0f64).prop_map(Some), Just(None)], nudge in proptest::option::of(hair()),
    ) {
        let slack = frame.slack(400.0);
        // The second point: somewhere else, the same point, or a hair away.
        let second_at = match (second, nudge) {
            (Some(t), _) => t,
            (None, Some(hair)) => (first + hair).clamp(0.0, 1.0),
            (None, None) => first,
        };

        let start = frame.place(0.0, 0.0);
        let along = unit(direction);
        let line = Segment::new(start, start + along * frame.length(100.0));
        let on_line = |t: f64| start + along * (frame.length(100.0) * t);
        // The tolerance a caller uses out here: the usual one near the
        // origin, wider where the numbers themselves are coarser (ADR-0012).
        let tolerance = TOLERANCE.around(&[line.start, line.end]);
        if let Some(pieces) = break_segment(&line, on_line(first), on_line(second_at), tolerance) {
            let kept: f64 = pieces.iter().map(Segment::length).sum();
            let gap = on_line(first).distance_to(on_line(second_at));
            prop_assert!((kept + gap - line.length()).abs() <= slack, "{} + {} of {}", kept, gap, line.length());
            prop_assert!(pieces.iter().all(|piece| piece.length() > 0.0));
            if pieces.len() == 2 && gap <= TOLERANCE.value() {
                let joined = join_segments(&pieces, tolerance).expect("two halves of one line");
                prop_assert_eq!((joined.start, joined.end), (line.start, line.end));
            }
        }

        let arc = Arc::new(start, frame.length(radius), direction, sweep);
        let on_arc = |t: f64| arc.point_at(arc.start + arc.sweep * t);
        if let Some(pieces) = break_arc(&arc, on_arc(first), on_arc(second_at), tolerance) {
            let kept: f64 = pieces.iter().map(|piece| piece.sweep).sum();
            let gap = (first - second_at).abs() * sweep;
            prop_assert!((kept + gap - sweep).abs() <= 1e-9 + slack / arc.radius, "{} + {} of {}", kept, gap, sweep);
            if pieces.len() == 2 && gap * arc.radius <= TOLERANCE.value() {
                match join_arcs(&pieces, tolerance) {
                    Ok(JoinedArcs::Arc(joined)) => {
                        prop_assert!((joined.sweep - sweep).abs() <= 1e-9 + slack / arc.radius);
                        prop_assert!(joined.start_point().distance_to(arc.start_point()) <= slack);
                    }
                    other => prop_assert!(false, "the halves did not join: {:?}", other),
                }
            }
        }
    }

    /// Arcs of one circle join into an arc that covers each of them, or
    /// into the circle, or are refused as apart — and the answer does not
    /// depend on the order they are given in.
    #[test]
    fn joined_arcs_cover_every_arc_whatever_the_order(
        frame in frame(), radius in 1.0..100f64,
        spans in proptest::collection::vec((0.0..TAU, 0.05..3.0f64), 2..5),
    ) {
        let centre = frame.place(0.0, 0.0);
        let arcs: Vec<Arc> = spans
            .iter()
            .map(|(start, sweep)| Arc::new(centre, frame.length(radius), *start, *sweep))
            .collect();
        let mut reversed = arcs.clone();
        reversed.reverse();
        let (forward, backward) = (join_arcs(&arcs, TOLERANCE), join_arcs(&reversed, TOLERANCE));
        match (forward, backward) {
            (Ok(JoinedArcs::Arc(a)), Ok(JoinedArcs::Arc(b))) => {
                prop_assert!((a.sweep - b.sweep).abs() <= 1e-9);
                for arc in &arcs {
                    prop_assert!(a.distance_to_point(arc.mid_point()) <= frame.slack(200.0));
                    prop_assert!(a.sweep >= arc.sweep - 1e-9);
                }
            }
            (Ok(JoinedArcs::Circle(_)), Ok(JoinedArcs::Circle(_)))
            | (Err(ArcJoinError::Apart), Err(ArcJoinError::Apart)) => {}
            other => prop_assert!(false, "the order changed the answer: {:?}", other),
        }
    }
}

// ── nothing panics, whatever the numbers ──

/// Coordinates from every corner of the number line that is still a number:
/// zero, the smallest and largest magnitudes, and ordinary values.
fn wild() -> impl Strategy<Value = f64> {
    prop_oneof![
        Just(0.0),
        Just(-0.0),
        Just(f64::MIN_POSITIVE),
        Just(1e-300),
        Just(-1e-300),
        Just(1e-9),
        Just(1e150),
        Just(-1e150),
        Just(1e300),
        Just(-1e300),
        Just(f64::MAX),
        -1e3..1e3f64,
        -1e12..1e12f64,
    ]
}

fn wild_point() -> impl Strategy<Value = Point2> {
    (wild(), wild()).prop_map(|(x, y)| Point2::new(x, y))
}

fn wild_primitive() -> impl Strategy<Value = Primitive> {
    prop_oneof![
        (wild_point(), wild_point()).prop_map(|(a, b)| Primitive::Segment(Segment::new(a, b))),
        (wild_point(), wild()).prop_map(|(c, r)| Primitive::Circle(Circle::new(c, r.abs()))),
        (wild_point(), wild(), -10.0..10f64, 0.0..TAU)
            .prop_map(|(c, r, start, sweep)| Primitive::Arc(Arc::new(c, r.abs(), start, sweep))),
    ]
}

proptest! {
    /// No operation panics on finite input, however extreme. What comes
    /// back may be a refusal, or numbers too large to mean anything; it is
    /// never a crash.
    #[test]
    fn no_operation_panics_on_any_finite_input(
        a in wild_primitive(), b in wild_primitive(),
        p in wild_point(), q in wild_point(), amount in wild(), other in wild(),
    ) {
        let _ = a.intersect(&b);
        let _ = (a.bounds(), a.closest_point(p), a.distance_to_point(p));
        let cuts: Vec<Point2> = a.intersect(&b).as_slice().iter().copied().chain([p, q]).collect();
        let amount = amount.abs();
        match &a {
            Primitive::Segment(segment) => {
                let _ = offset_segment(segment, amount, p);
                let _ = trim_segment_parts(segment, &cuts, q, TOLERANCE);
                let _ = extend_segment(segment, true, &[b], TOLERANCE);
                let _ = extend_segment(segment, false, &[b], TOLERANCE);
                let _ = break_segment(segment, p, q, TOLERANCE);
                if let Primitive::Segment(second) = &b {
                    let _ = join_segments(&[*segment, *second], TOLERANCE);
                    if let Ok(corner) = Corner::new(segment, p, second, q, TOLERANCE) {
                        let _ = corner.fillet(amount, TOLERANCE);
                        let _ = corner.chamfer([amount, other.abs()], TOLERANCE);
                    }
                }
            }
            Primitive::Circle(circle) => {
                let _ = offset_circle(circle, amount, p, TOLERANCE);
                let _ = trim_circle_parts(circle, &cuts, q, TOLERANCE);
            }
            Primitive::Arc(arc) => {
                let _ = offset_arc(arc, amount, p, TOLERANCE);
                let _ = trim_arc_parts(arc, &cuts, q, TOLERANCE);
                let _ = break_arc(arc, p, q, TOLERANCE);
                if let Primitive::Arc(second) = &b {
                    let _ = join_arcs(&[*arc, *second], TOLERANCE);
                }
            }
        }
        let _ = fillet_curves(&a, p, &b, q, amount, TOLERANCE);
        let _ = length_of(&a);
    }
}

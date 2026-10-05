//! Cutting a curve where other geometry crosses it, and removing the part
//! that was picked (DESIGN §22). The simple cases: a segment or an arc cut
//! into at most two remaining pieces, and a circle opened into an arc.

use cad_core::Point2;
use cad_core::tolerance::LinearTolerance;

use crate::{Arc, Circle, Segment, normalize_angle};

/// A curve with two ends, which is what trimming works on.
pub(crate) trait OpenCurve: Sized {
    fn start_point(&self) -> Point2;
    fn end_point(&self) -> Point2;
    /// How far along the curve the closest point to `point` is, from 0 at
    /// the start to 1 at the end.
    fn parameter_of(&self, point: Point2) -> f64;
    /// The part of the curve between two points on it, in curve order.
    fn between(&self, from: Point2, to: Point2) -> Self;
    /// The point of the curve nearest to `point`.
    fn nearest(&self, point: Point2) -> Point2;
}

impl OpenCurve for Segment {
    fn start_point(&self) -> Point2 {
        self.start
    }

    fn end_point(&self) -> Point2 {
        self.end
    }

    fn parameter_of(&self, point: Point2) -> f64 {
        let along = self.end - self.start;
        let length_squared = along.length_squared();
        if length_squared > 0.0 {
            ((point - self.start).dot(along) / length_squared).clamp(0.0, 1.0)
        } else {
            0.0
        }
    }

    fn between(&self, from: Point2, to: Point2) -> Self {
        Segment::new(from, to)
    }

    fn nearest(&self, point: Point2) -> Point2 {
        self.closest_point(point)
    }
}

impl OpenCurve for Arc {
    fn start_point(&self) -> Point2 {
        Arc::start_point(self)
    }

    fn end_point(&self) -> Point2 {
        Arc::end_point(self)
    }

    fn parameter_of(&self, point: Point2) -> f64 {
        let nearest = self.closest_point(point);
        (normalize_angle(angle_about(self.center, nearest) - self.start) / self.sweep)
            .clamp(0.0, 1.0)
    }

    fn between(&self, from: Point2, to: Point2) -> Self {
        // The curve's own ends keep their exact angles.
        let angle = |point: Point2, own: f64, own_point: Point2| {
            if point == own_point {
                own
            } else {
                angle_about(self.center, point)
            }
        };
        let start = angle(from, self.start, Arc::start_point(self));
        let end = angle(to, self.end(), Arc::end_point(self));
        Arc::from_angles(self.center, self.radius, start, end)
    }

    fn nearest(&self, point: Point2) -> Point2 {
        self.closest_point(point)
    }
}

fn angle_about(center: Point2, point: Point2) -> f64 {
    let offset = point - center;
    offset.y.atan2(offset.x)
}

/// What is left of an open curve when the stretch containing `pick` —
/// from the nearest cut before it to the nearest cut after it — is removed.
///
/// `cuts` are points on the curve where a cutting edge meets it. A cut at
/// one of the curve's own ends divides nothing and is ignored. Returns one
/// or two pieces, or `None` when no cut divides the curve.
fn trim_open<C: OpenCurve>(
    curve: &C,
    cuts: &[Point2],
    pick: Point2,
    tolerance: LinearTolerance,
) -> Option<Trimmed<C>> {
    let (start, end) = (curve.start_point(), curve.end_point());
    let at = curve.parameter_of(pick);
    let dividing = cuts
        .iter()
        .filter(|cut| !tolerance.coincident(**cut, start) && !tolerance.coincident(**cut, end))
        .map(|cut| (curve.parameter_of(*cut), *cut));

    // The nearest cut on each side of the pick.
    let mut before: Option<(f64, Point2)> = None;
    let mut after: Option<(f64, Point2)> = None;
    for (parameter, cut) in dividing {
        if parameter < at {
            if before.is_none_or(|(nearest, _)| parameter > nearest) {
                before = Some((parameter, cut));
            }
        } else if after.is_none_or(|(nearest, _)| parameter < nearest) {
            after = Some((parameter, cut));
        }
    }
    if before.is_none() && after.is_none() {
        return None;
    }
    let from = before.map_or(start, |(_, cut)| cut);
    let to = after.map_or(end, |(_, cut)| cut);
    let head = before.map(|_| curve.between(start, from));
    let tail = after.map(|_| curve.between(to, end));
    Some(Trimmed {
        kept: head.into_iter().chain(tail).collect(),
        removed: curve.between(from, to),
    })
}

/// What is left of an open curve broken between two points on it: up to
/// two pieces, one before the earlier point and one after the later. With
/// the two points the same, the curve is split there and nothing is removed.
///
/// `None` when nothing would change — a split at one of the curve's own
/// ends — or when nothing would be left.
fn break_open<C: OpenCurve>(
    curve: &C,
    first: Point2,
    second: Point2,
    tolerance: LinearTolerance,
) -> Option<Vec<C>> {
    let (start, end) = (curve.start_point(), curve.end_point());
    let (a, b) = (curve.nearest(first), curve.nearest(second));
    let (from, to) = if curve.parameter_of(a) <= curve.parameter_of(b) {
        (a, b)
    } else {
        (b, a)
    };
    let head = (!tolerance.coincident(from, start)).then(|| curve.between(start, from));
    let tail = (!tolerance.coincident(to, end)).then(|| curve.between(to, end));
    let split_only = tolerance.coincident(from, to);
    match (head, tail) {
        (None, None) => None,
        // One break point, at an end: there is nothing to split off.
        (None, Some(_)) | (Some(_), None) if split_only => None,
        (head, tail) => Some(head.into_iter().chain(tail).collect()),
    }
}

/// See [`break_open`]: a segment broken between two points.
pub fn break_segment(
    segment: &Segment,
    first: Point2,
    second: Point2,
    tolerance: LinearTolerance,
) -> Option<Vec<Segment>> {
    break_open(segment, first, second, tolerance)
}

/// See [`break_open`]: an arc broken between two points.
pub fn break_arc(
    arc: &Arc,
    first: Point2,
    second: Point2,
    tolerance: LinearTolerance,
) -> Option<Vec<Arc>> {
    break_open(arc, first, second, tolerance)
}

/// A circle broken between two points on it: the part counter-clockwise
/// from the first to the second goes, and the rest stays as one arc. `None`
/// when the two points are the same: a circle has no ends to split at.
pub fn break_circle(
    circle: &Circle,
    first: Point2,
    second: Point2,
    tolerance: LinearTolerance,
) -> Option<Arc> {
    let (a, b) = (circle.closest_point(first), circle.closest_point(second));
    (!tolerance.coincident(a, b)).then(|| {
        Arc::from_angles(
            circle.center,
            circle.radius,
            angle_about(circle.center, b),
            angle_about(circle.center, a),
        )
    })
}

/// A curve after trimming: the one or two pieces that stay, and the piece
/// that goes — which is what a preview shows.
#[derive(Clone, Debug, PartialEq)]
pub struct Trimmed<C> {
    pub kept: Vec<C>,
    pub removed: C,
}

/// See [`trim_open`]: a segment trimmed at `pick`, as what stays and what goes.
pub fn trim_segment_parts(
    segment: &Segment,
    cuts: &[Point2],
    pick: Point2,
    tolerance: LinearTolerance,
) -> Option<Trimmed<Segment>> {
    trim_open(segment, cuts, pick, tolerance)
}

/// The pieces of a segment left after trimming at `pick`.
pub fn trim_segment(
    segment: &Segment,
    cuts: &[Point2],
    pick: Point2,
    tolerance: LinearTolerance,
) -> Option<Vec<Segment>> {
    trim_segment_parts(segment, cuts, pick, tolerance).map(|parts| parts.kept)
}

/// See [`trim_open`]: an arc trimmed at `pick`, as what stays and what goes.
pub fn trim_arc_parts(
    arc: &Arc,
    cuts: &[Point2],
    pick: Point2,
    tolerance: LinearTolerance,
) -> Option<Trimmed<Arc>> {
    trim_open(arc, cuts, pick, tolerance)
}

/// The pieces of an arc left after trimming at `pick`.
pub fn trim_arc(
    arc: &Arc,
    cuts: &[Point2],
    pick: Point2,
    tolerance: LinearTolerance,
) -> Option<Vec<Arc>> {
    trim_arc_parts(arc, cuts, pick, tolerance).map(|parts| parts.kept)
}

/// A circle trimmed at `pick`: the stretch containing the pick, between two
/// neighbouring cuts, goes; the rest stays as one arc. A circle has no ends,
/// so it takes two distinct cuts to remove anything: `None` with fewer.
pub fn trim_circle_parts(
    circle: &Circle,
    cuts: &[Point2],
    pick: Point2,
    tolerance: LinearTolerance,
) -> Option<Trimmed<Arc>> {
    let mut distinct: Vec<Point2> = Vec::new();
    for cut in cuts {
        if !distinct
            .iter()
            .any(|seen| tolerance.coincident(*seen, *cut))
        {
            distinct.push(*cut);
        }
    }
    if distinct.len() < 2 {
        return None;
    }
    let mut angles: Vec<f64> = distinct
        .iter()
        .map(|cut| normalize_angle(angle_about(circle.center, *cut)))
        .collect();
    angles.sort_by(f64::total_cmp);

    // The removed gap runs counter-clockwise from the last cut at or before
    // the pick to the next one; what stays runs on from there back round.
    let at = normalize_angle(angle_about(circle.center, pick));
    let next = angles.iter().position(|angle| *angle > at).unwrap_or(0);
    let previous = (next + angles.len() - 1) % angles.len();
    let arc = |from: f64, to: f64| Arc::from_angles(circle.center, circle.radius, from, to);
    Some(Trimmed {
        kept: vec![arc(angles[next], angles[previous])],
        removed: arc(angles[previous], angles[next]),
    })
}

/// The arc left of a circle after trimming at `pick`.
pub fn trim_circle(
    circle: &Circle,
    cuts: &[Point2],
    pick: Point2,
    tolerance: LinearTolerance,
) -> Option<Arc> {
    trim_circle_parts(circle, cuts, pick, tolerance).and_then(|parts| parts.kept.into_iter().next())
}

#[cfg(test)]
mod tests {
    use std::f64::consts::{FRAC_PI_2, PI};

    use super::*;

    const TOLERANCE: LinearTolerance = LinearTolerance::DEFAULT;

    fn p(x: f64, y: f64) -> Point2 {
        Point2::new(x, y)
    }

    fn segment() -> Segment {
        Segment::new(p(0.0, 0.0), p(100.0, 0.0))
    }

    #[test]
    fn one_cut_removes_the_side_that_was_picked() {
        let cuts = [p(40.0, 0.0)];
        assert_eq!(
            trim_segment(&segment(), &cuts, p(90.0, 3.0), TOLERANCE),
            Some(vec![Segment::new(p(0.0, 0.0), p(40.0, 0.0))])
        );
        assert_eq!(
            trim_segment(&segment(), &cuts, p(10.0, -3.0), TOLERANCE),
            Some(vec![Segment::new(p(40.0, 0.0), p(100.0, 0.0))])
        );
    }

    #[test]
    fn a_pick_between_two_cuts_removes_the_middle_and_leaves_two_pieces() {
        let cuts = [p(70.0, 0.0), p(30.0, 0.0)];
        assert_eq!(
            trim_segment(&segment(), &cuts, p(50.0, 0.0), TOLERANCE),
            Some(vec![
                Segment::new(p(0.0, 0.0), p(30.0, 0.0)),
                Segment::new(p(70.0, 0.0), p(100.0, 0.0))
            ])
        );
        // Outside the two cuts, only up to the nearer one goes.
        assert_eq!(
            trim_segment(&segment(), &cuts, p(95.0, 0.0), TOLERANCE),
            Some(vec![Segment::new(p(0.0, 0.0), p(70.0, 0.0))])
        );
    }

    /// What goes and what stays always add up to the original.
    #[test]
    fn the_removed_part_is_the_complement_of_what_is_kept() {
        let cuts = [p(70.0, 0.0), p(30.0, 0.0)];
        let middle = trim_segment_parts(&segment(), &cuts, p(50.0, 0.0), TOLERANCE).unwrap();
        assert_eq!(middle.removed, Segment::new(p(30.0, 0.0), p(70.0, 0.0)));
        assert_eq!(middle.kept.len(), 2);

        let tail = trim_segment_parts(&segment(), &cuts, p(95.0, 0.0), TOLERANCE).unwrap();
        assert_eq!(tail.removed, Segment::new(p(70.0, 0.0), p(100.0, 0.0)));
        let head = trim_segment_parts(&segment(), &cuts, p(5.0, 0.0), TOLERANCE).unwrap();
        assert_eq!(head.removed, Segment::new(p(0.0, 0.0), p(30.0, 0.0)));

        // An arc: the two sweeps add up.
        let parts = trim_arc_parts(&arc(), &[p(0.0, 10.0)], p(-7.0, 7.0), TOLERANCE).unwrap();
        assert!((parts.kept[0].sweep + parts.removed.sweep - PI).abs() < 1e-12);
        assert!(parts.removed.end_point().distance_to(p(-10.0, 0.0)) < 1e-12);

        // A circle: the kept arc and the removed arc make the full turn.
        let circle = Circle::new(p(0.0, 0.0), 10.0);
        let cuts = [p(10.0, 0.0), p(0.0, 10.0)];
        let parts = trim_circle_parts(&circle, &cuts, p(7.0, 7.0), TOLERANCE).unwrap();
        assert!((parts.removed.sweep - FRAC_PI_2).abs() < 1e-12);
        assert!((parts.kept[0].sweep + parts.removed.sweep - 2.0 * PI).abs() < 1e-12);
        assert!(
            parts
                .removed
                .mid_point()
                .distance_to(p(50f64.sqrt(), 50f64.sqrt()))
                < 1e-9
        );
    }

    // ── BREAK (#126) ──

    #[test]
    fn a_segment_broken_between_two_points_loses_what_lies_between() {
        // The points need not be on the segment: their nearest points count.
        assert_eq!(
            break_segment(&segment(), p(30.0, 4.0), p(70.0, -2.0), TOLERANCE),
            Some(vec![
                Segment::new(p(0.0, 0.0), p(30.0, 0.0)),
                Segment::new(p(70.0, 0.0), p(100.0, 0.0))
            ])
        );
        // The order of the two points does not matter.
        assert_eq!(
            break_segment(&segment(), p(70.0, 0.0), p(30.0, 0.0), TOLERANCE),
            break_segment(&segment(), p(30.0, 0.0), p(70.0, 0.0), TOLERANCE)
        );
        // A second point beyond the end takes the end off.
        assert_eq!(
            break_segment(&segment(), p(60.0, 0.0), p(500.0, 0.0), TOLERANCE),
            Some(vec![Segment::new(p(0.0, 0.0), p(60.0, 0.0))])
        );
    }

    #[test]
    fn the_same_point_twice_splits_without_removing_anything() {
        assert_eq!(
            break_segment(&segment(), p(40.0, 0.0), p(40.0, 0.0), TOLERANCE),
            Some(vec![
                Segment::new(p(0.0, 0.0), p(40.0, 0.0)),
                Segment::new(p(40.0, 0.0), p(100.0, 0.0))
            ])
        );
    }

    #[test]
    fn breaking_at_an_end_or_away_everything_is_refused() {
        // A split at an endpoint splits nothing off.
        assert_eq!(
            break_segment(&segment(), p(0.0, 0.0), p(0.0, 0.0), TOLERANCE),
            None
        );
        assert_eq!(
            break_segment(&segment(), p(100.0, 0.0), p(100.0, 0.0), TOLERANCE),
            None
        );
        // From end to end, nothing would be left.
        assert_eq!(
            break_segment(&segment(), p(0.0, 0.0), p(100.0, 0.0), TOLERANCE),
            None
        );
        assert_eq!(
            break_segment(&segment(), p(-50.0, 0.0), p(500.0, 0.0), TOLERANCE),
            None
        );
    }

    #[test]
    fn an_arc_is_broken_along_its_sweep() {
        let diagonal = 10.0 / 2f64.sqrt();
        let pieces = break_arc(
            &arc(),
            p(diagonal, diagonal),
            p(-diagonal, diagonal),
            TOLERANCE,
        )
        .unwrap();
        assert_eq!(pieces.len(), 2);
        assert!((pieces[0].sweep - PI / 4.0).abs() < 1e-12 && pieces[0].start == 0.0);
        assert!((pieces[1].sweep - PI / 4.0).abs() < 1e-12);
        assert!(pieces[1].end_point().distance_to(p(-10.0, 0.0)) < 1e-12);

        let split = break_arc(&arc(), p(0.0, 10.0), p(0.0, 10.0), TOLERANCE).unwrap();
        assert!(
            (split[0].sweep - FRAC_PI_2).abs() < 1e-12
                && (split[1].sweep - FRAC_PI_2).abs() < 1e-12
        );
    }

    #[test]
    fn a_circle_loses_the_part_counter_clockwise_from_the_first_point_to_the_second() {
        let circle = Circle::new(p(0.0, 0.0), 10.0);
        // From east to north: the first quadrant goes, three quarters stay.
        let kept = break_circle(&circle, p(10.0, 0.0), p(0.0, 10.0), TOLERANCE).unwrap();
        assert!(
            (kept.start - FRAC_PI_2).abs() < 1e-12 && (kept.sweep - 3.0 * FRAC_PI_2).abs() < 1e-12
        );
        // From north to east: the other way round, only the first quadrant stays.
        let kept = break_circle(&circle, p(0.0, 10.0), p(10.0, 0.0), TOLERANCE).unwrap();
        assert!(kept.start.abs() < 1e-12 && (kept.sweep - FRAC_PI_2).abs() < 1e-12);
        // One point cannot open a circle.
        assert_eq!(
            break_circle(&circle, p(10.0, 0.0), p(20.0, 0.0), TOLERANCE),
            None
        );
    }

    #[test]
    fn with_no_cut_on_the_curve_nothing_can_be_trimmed() {
        assert_eq!(trim_segment(&segment(), &[], p(50.0, 0.0), TOLERANCE), None);
    }

    /// Endpoint-coincident: a cutting edge that only touches the end of the
    /// target divides nothing.
    #[test]
    fn a_cut_at_the_curves_own_end_is_not_a_cut() {
        for end in [p(0.0, 0.0), p(100.0, 0.0)] {
            assert_eq!(
                trim_segment(&segment(), &[end], p(50.0, 0.0), TOLERANCE),
                None
            );
        }
        // Together with a real cut, the real one decides.
        assert_eq!(
            trim_segment(
                &segment(),
                &[p(100.0, 0.0), p(60.0, 0.0)],
                p(80.0, 0.0),
                TOLERANCE
            ),
            Some(vec![Segment::new(p(0.0, 0.0), p(60.0, 0.0))])
        );
    }

    #[test]
    fn pieces_end_exactly_on_the_cut_points() {
        let slanted = Segment::new(p(0.1, 0.2), p(7.3, 9.1));
        let cut = p(0.1 + 7.2 / 3.0, 0.2 + 8.9 / 3.0);
        let pieces = trim_segment(&slanted, &[cut], p(7.0, 9.0), TOLERANCE).unwrap();
        assert_eq!(pieces, vec![Segment::new(p(0.1, 0.2), cut)]);
    }

    /// The upper half of a circle of radius 10: from (10,0) over (0,10) to (-10,0).
    fn arc() -> Arc {
        Arc::new(p(0.0, 0.0), 10.0, 0.0, PI)
    }

    #[test]
    fn an_arc_is_trimmed_like_a_segment_along_its_sweep() {
        let top = [p(0.0, 10.0)];
        let right = trim_arc(&arc(), &top, p(-7.0, 7.0), TOLERANCE).unwrap();
        assert_eq!(right.len(), 1);
        assert_eq!(right[0].start, 0.0, "its own start angle is kept exactly");
        assert!((right[0].sweep - FRAC_PI_2).abs() < 1e-12);

        let left = trim_arc(&arc(), &top, p(7.0, 7.0), TOLERANCE).unwrap();
        assert!((left[0].start - FRAC_PI_2).abs() < 1e-12);
        assert!((left[0].sweep - FRAC_PI_2).abs() < 1e-12);
        assert!(left[0].end_point().distance_to(p(-10.0, 0.0)) < 1e-12);
    }

    #[test]
    fn an_arc_cut_twice_loses_its_middle() {
        let diagonal = 10.0 / 2f64.sqrt();
        let cuts = [p(diagonal, diagonal), p(-diagonal, diagonal)];
        let pieces = trim_arc(&arc(), &cuts, p(0.0, 10.0), TOLERANCE).unwrap();
        assert_eq!(pieces.len(), 2);
        assert!((pieces[0].sweep - PI / 4.0).abs() < 1e-12);
        assert!((pieces[1].sweep - PI / 4.0).abs() < 1e-12);
        assert!(pieces[1].end_point().distance_to(p(-10.0, 0.0)) < 1e-12);
    }

    #[test]
    fn a_circle_cut_twice_opens_into_the_arc_away_from_the_pick() {
        let circle = Circle::new(p(0.0, 0.0), 10.0);
        let cuts = [p(10.0, 0.0), p(-10.0, 0.0)];

        // Pick the top: the bottom half stays, from 180° round to 360°.
        let bottom = trim_circle(&circle, &cuts, p(0.0, 10.0), TOLERANCE).unwrap();
        assert!((bottom.start - PI).abs() < 1e-12 && (bottom.sweep - PI).abs() < 1e-12);
        assert!(bottom.mid_point().distance_to(p(0.0, -10.0)) < 1e-9);

        let top = trim_circle(&circle, &cuts, p(1.0, -9.0), TOLERANCE).unwrap();
        assert!(top.mid_point().distance_to(p(0.0, 10.0)) < 1e-9);
    }

    #[test]
    fn a_circle_with_several_cuts_loses_only_the_picked_stretch() {
        let circle = Circle::new(p(0.0, 0.0), 10.0);
        let cuts = [p(10.0, 0.0), p(0.0, 10.0), p(-10.0, 0.0), p(0.0, -10.0)];
        // Pick in the first quadrant: three quarters stay, from 90° to 360°.
        let kept = trim_circle(&circle, &cuts, p(7.0, 7.0), TOLERANCE).unwrap();
        assert!((kept.start - FRAC_PI_2).abs() < 1e-12);
        assert!((kept.sweep - 3.0 * FRAC_PI_2).abs() < 1e-12);
        // Pick in the fourth quadrant, past the last cut: wraps around.
        let kept = trim_circle(&circle, &cuts, p(7.0, -7.0), TOLERANCE).unwrap();
        assert!(kept.start.abs() < 1e-12);
        assert!((kept.sweep - 3.0 * FRAC_PI_2).abs() < 1e-12);
    }

    /// Tangent: a cutting edge that touches a circle once cannot open it.
    #[test]
    fn a_circle_needs_two_distinct_cuts() {
        let circle = Circle::new(p(0.0, 0.0), 10.0);
        assert_eq!(trim_circle(&circle, &[], p(0.0, 10.0), TOLERANCE), None);
        assert_eq!(
            trim_circle(&circle, &[p(10.0, 0.0)], p(0.0, 10.0), TOLERANCE),
            None
        );
        let twice = [p(10.0, 0.0), p(10.0, 0.0)];
        assert_eq!(trim_circle(&circle, &twice, p(0.0, 10.0), TOLERANCE), None);
    }
}

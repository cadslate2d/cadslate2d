//! Collinear runs (#234): segments that lie on one line and overlap or
//! touch, which `OVERKILL` merges into one.
//!
//! Segments are first sorted by the direction of their line and its
//! distance from the origin, so only neighbours are compared; a candidate
//! joins a line only when both its ends lie on it within the linear
//! tolerance at their magnitude. Along the line, the segments' spans are
//! merged where they overlap or meet.

use std::f64::consts::PI;

use cad_core::Point2;
use cad_core::tolerance::{AngularTolerance, LinearTolerance};

use crate::{Segment, normalized, perpendicular};

/// Segments of one line whose spans overlap or meet: their indices, the
/// first being the lowest, and the segment covering them all — from one
/// member's end to another's, the coordinates untouched, running the way
/// the first member runs.
#[derive(Clone, Debug, PartialEq)]
pub struct CollinearRun {
    pub members: Vec<usize>,
    pub segment: Segment,
}

/// Every run of two or more of `segments` that lie on one line and
/// overlap or touch. A segment of no length is in none.
pub fn collinear_runs(segments: &[Segment]) -> Vec<CollinearRun> {
    // Each segment's line: its direction as an angle in [0, π), and its
    // distance from the origin along that direction's normal.
    let mut lines: Vec<(f64, usize)> = segments
        .iter()
        .enumerate()
        .filter_map(|(index, segment)| {
            let tolerance = LinearTolerance::DEFAULT.around(&[segment.start, segment.end]);
            if segment.is_degenerate(tolerance) {
                return None;
            }
            let direction = normalized(segment.end - segment.start)?;
            Some((direction.y.atan2(direction.x).rem_euclid(PI), index))
        })
        .collect();
    lines.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
    // Directions within the angular tolerance, the last wrapping round to
    // the first at π.
    let angular = AngularTolerance::DEFAULT;
    let mut directions: Vec<Vec<usize>> = Vec::new();
    let mut last = f64::NEG_INFINITY;
    for (angle, index) in &lines {
        match directions.last_mut() {
            Some(group) if angular.eq(*angle, last) => group.push(*index),
            _ => directions.push(vec![*index]),
        }
        last = *angle;
    }
    if directions.len() > 1
        && let (Some(first), Some(end)) = (lines.first(), lines.last())
        && angular.eq(first.0 + PI, end.0)
    {
        let wrapped = directions.pop().expect("more than one");
        directions[0].extend(wrapped);
    }

    let mut runs = Vec::new();
    for group in directions {
        if group.len() < 2 {
            continue;
        }
        runs.extend(runs_in_direction(segments, group));
    }
    runs.sort_by_key(|run| run.members[0]);
    runs
}

/// The runs among segments whose lines share a direction.
fn runs_in_direction(segments: &[Segment], mut group: Vec<usize>) -> Vec<CollinearRun> {
    group.sort_unstable();
    let reference = segments[group[0]];
    let along = normalized(reference.end - reference.start).expect("not degenerate");
    let normal = perpendicular(along);
    // By distance from the origin, then each line's members together.
    let offset = |index: usize| normal.dot(segments[index].start - Point2::ORIGIN);
    group.sort_by(|a, b| offset(*a).total_cmp(&offset(*b)).then(a.cmp(b)));
    let mut runs = Vec::new();
    let mut at = 0;
    while at < group.len() {
        let first = segments[group[at]];
        let mut line = vec![group[at]];
        let mut next = at + 1;
        while next < group.len() {
            let candidate = segments[group[next]];
            let tolerance = LinearTolerance::DEFAULT.around(&[
                first.start,
                first.end,
                candidate.start,
                candidate.end,
            ]);
            if !tolerance.eq(offset(group[next]), offset(group[at])) {
                break;
            }
            next += 1;
            // Both ends on the line, not just the start.
            let on = |point: Point2| tolerance.is_zero(normal.dot(point - first.start));
            if on(candidate.start) && on(candidate.end) {
                line.push(group[next - 1]);
            }
        }
        at = next;
        if line.len() > 1 {
            runs.extend(spans(segments, line, along));
        }
    }
    runs
}

/// The runs of segments on one line whose spans along `along` overlap or
/// meet.
fn spans(segments: &[Segment], line: Vec<usize>, along: cad_core::Vec2) -> Vec<CollinearRun> {
    let origin = segments[line[0]].start;
    let t = |point: Point2| along.dot(point - origin);
    // Each segment's span, its ends ordered along the line.
    let mut spans: Vec<(f64, f64, Point2, Point2, usize)> = line
        .iter()
        .map(|&index| {
            let segment = segments[index];
            let (low, high) = if t(segment.start) <= t(segment.end) {
                (segment.start, segment.end)
            } else {
                (segment.end, segment.start)
            };
            (t(low), t(high), low, high, index)
        })
        .collect();
    spans.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.4.cmp(&b.4)));
    let mut runs: Vec<CollinearRun> = Vec::new();
    let mut current: Option<(f64, Point2, Point2, Vec<usize>)> = None;
    for (low_t, high_t, low, high, index) in spans {
        let tolerance = LinearTolerance::DEFAULT.around(&[low, high]);
        match current.as_mut() {
            Some((end_t, _, end, members)) if low_t <= *end_t + tolerance.value() => {
                members.push(index);
                if high_t > *end_t {
                    *end_t = high_t;
                    *end = high;
                }
            }
            _ => {
                if let Some(run) = current.take() {
                    runs.extend(finish(segments, run));
                }
                current = Some((high_t, low, high, vec![index]));
            }
        }
    }
    if let Some(run) = current {
        runs.extend(finish(segments, run));
    }
    runs
}

/// A run of two or more members, running the way its first member does.
fn finish(
    segments: &[Segment],
    (_, low, high, mut members): (f64, Point2, Point2, Vec<usize>),
) -> Option<CollinearRun> {
    if members.len() < 2 {
        return None;
    }
    members.sort_unstable();
    let first = segments[members[0]];
    let forwards = (first.end - first.start).dot(high - low) >= 0.0;
    let segment = if forwards {
        Segment::new(low, high)
    } else {
        Segment::new(high, low)
    };
    Some(CollinearRun { members, segment })
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    fn s(x0: f64, y0: f64, x1: f64, y1: f64) -> Segment {
        Segment::new(Point2::new(x0, y0), Point2::new(x1, y1))
    }

    #[test]
    fn overlapping_touching_and_contained_segments_make_one_run() {
        let segments = [
            s(0.0, 0.0, 10.0, 0.0),
            s(5.0, 0.0, 15.0, 0.0),
            s(25.0, 0.0, 15.0, 0.0),
            s(3.0, 0.0, 4.0, 0.0),
            s(30.0, 0.0, 40.0, 0.0),
            s(0.0, 1.0, 10.0, 1.0),
        ];
        assert_eq!(
            collinear_runs(&segments),
            vec![CollinearRun {
                members: vec![0, 1, 2, 3],
                segment: s(0.0, 0.0, 25.0, 0.0),
            }],
            "the gap to the fifth and the parallel sixth keep them out"
        );
    }

    #[test]
    fn a_run_runs_the_way_its_first_member_does_and_lines_near_horizontal_wrap() {
        let segments = [s(10.0, 0.0, 0.0, 0.0), s(5.0, 0.0, 20.0, 0.0)];
        assert_eq!(collinear_runs(&segments)[0].segment, s(20.0, 0.0, 0.0, 0.0));
        // Exactly equal, given as each other's reverse, and a vertical pair.
        let segments = [
            s(0.0, 0.0, 0.0, 10.0),
            s(0.0, 10.0, 0.0, 0.0),
            s(1.0, 1.0, 2.0, 2.0),
        ];
        let runs = collinear_runs(&segments);
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].members, vec![0, 1]);
    }

    #[test]
    fn segments_of_no_length_and_slightly_turned_ones_stay_out() {
        let segments = [
            s(0.0, 0.0, 10.0, 0.0),
            s(5.0, 0.0, 5.0, 0.0),
            s(5.0, 0.0, 15.0, 0.001),
        ];
        assert!(collinear_runs(&segments).is_empty());
    }

    proptest! {
        /// Pieces cut from one segment, shuffled and some reversed, merge
        /// back into it when they cover it without gaps.
        #[test]
        fn pieces_of_a_segment_merge_back_into_it(
            x in -1e4f64..1e4, y in -1e4f64..1e4, angle in 0.0f64..std::f64::consts::TAU,
            length in 1.0f64..1e3, cuts in prop::collection::vec(0.01f64..0.99, 1..6),
            reversed in prop::collection::vec(any::<bool>(), 7),
        ) {
            let start = Point2::new(x, y);
            let direction = cad_core::Vec2::from_polar(1.0, angle);
            let mut ts: Vec<f64> = cuts.clone();
            ts.push(0.0);
            ts.push(1.0);
            ts.sort_by(f64::total_cmp);
            ts.dedup();
            let at = |t: f64| if t == 0.0 { start } else { start + direction * (length * t) };
            let pieces: Vec<Segment> = ts
                .windows(2)
                .enumerate()
                .map(|(n, pair)| {
                    let (a, b) = (at(pair[0]), at(pair[1]));
                    if reversed[n % reversed.len()] { Segment::new(b, a) } else { Segment::new(a, b) }
                })
                .collect();
            prop_assume!(pieces.len() > 1);
            let runs = collinear_runs(&pieces);
            prop_assert_eq!(runs.len(), 1, "{:?}", pieces);
            let run = &runs[0];
            prop_assert_eq!(run.members.len(), pieces.len());
            let ends = [run.segment.start, run.segment.end];
            let tolerance = 1e-9 * (1.0 + x.abs() + y.abs() + length);
            let near = |p: Point2, q: Point2| p.distance_to(q) < tolerance;
            prop_assert!(ends.iter().any(|end| near(*end, start)));
            prop_assert!(ends.iter().any(|end| near(*end, start + direction * length)));
        }
    }
}

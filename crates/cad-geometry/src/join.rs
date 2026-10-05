//! Joining: several pieces that are really one (DESIGN §22, §31).
//!
//! Three cases, each unambiguous:
//!
//! - segments on one line become the segment that spans them — a gap
//!   between them is bridged, since there is only one way to do it;
//! - arcs on one circle that touch or overlap become one arc, or the whole
//!   circle. A gap between arcs is *not* bridged: there are two ways round;
//! - open paths that meet end to end become one path.
//!
//! Ends are matched with the linear tolerance handed in, never a literal.

use std::f64::consts::TAU;

use cad_core::Point2;
use cad_core::tolerance::LinearTolerance;

use crate::offset::distance_to_line;
use crate::{Arc, Circle, Path, Segment, normalize_angle};

/// The segment spanning `segments`, if they all lie on one line; it runs the
/// way the first of them does. `None` for fewer than two, or when they are
/// not collinear.
pub fn join_segments(segments: &[Segment], tolerance: LinearTolerance) -> Option<Segment> {
    let first = segments.first().filter(|_| segments.len() > 1)?;
    // The longest segment gives the line: the best-conditioned direction.
    let reference = segments
        .iter()
        .max_by(|a, b| a.length().total_cmp(&b.length()))?;
    let ends = || {
        segments
            .iter()
            .flat_map(|segment| [segment.start, segment.end])
    };
    for end in ends() {
        if !tolerance.is_zero(distance_to_line(reference, end)?) {
            return None;
        }
    }
    let direction = first.end - first.start;
    let along = |point: Point2| (point - first.start).dot(direction);
    let start = ends().min_by(|a, b| along(*a).total_cmp(&along(*b)))?;
    let end = ends().max_by(|a, b| along(*a).total_cmp(&along(*b)))?;
    Some(Segment::new(start, end))
}

/// What arcs of one circle add up to.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum JoinedArcs {
    Arc(Arc),
    /// Together they go all the way round.
    Circle(Circle),
}

/// Why arcs could not be joined.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArcJoinError {
    /// Fewer than two arcs, or arcs of different circles.
    NotOneCircle,
    /// On one circle, but with a gap between some of them.
    Apart,
}

/// Whether every arc is on the circle of the first.
pub fn cocircular(arcs: &[Arc], tolerance: LinearTolerance) -> bool {
    arcs.split_first().is_some_and(|(first, rest)| {
        rest.iter().all(|arc| {
            tolerance.coincident(arc.center, first.center) && tolerance.eq(arc.radius, first.radius)
        })
    })
}

/// The arc that `arcs` — all on one circle, each touching or overlapping
/// another — add up to, or the circle when they close it.
pub fn join_arcs(arcs: &[Arc], tolerance: LinearTolerance) -> Result<JoinedArcs, ArcJoinError> {
    let (first, rest) = arcs.split_first().ok_or(ArcJoinError::NotOneCircle)?;
    if rest.is_empty() || !cocircular(arcs, tolerance) {
        return Err(ArcJoinError::NotOneCircle);
    }
    // An end is matched within the linear tolerance, as an angle on this circle.
    let slack = tolerance.value() / first.radius;
    let (mut start, mut sweep) = (first.start, first.sweep);
    let mut waiting: Vec<&Arc> = rest.iter().collect();
    while !waiting.is_empty() {
        let before = waiting.len();
        waiting.retain(|arc| {
            let ahead = normalize_angle(arc.start - start);
            let behind = normalize_angle(start - arc.start);
            if ahead <= sweep + slack {
                // It starts on what there is so far.
                sweep = sweep.max(ahead + arc.sweep);
                false
            } else if behind <= arc.sweep + slack {
                // What there is so far starts on it.
                start = arc.start;
                sweep = arc.sweep.max(behind + sweep);
                false
            } else {
                true
            }
        });
        if sweep >= TAU - slack {
            return Ok(JoinedArcs::Circle(first.circle()));
        }
        if waiting.len() == before {
            return Err(ArcJoinError::Apart);
        }
    }
    Ok(JoinedArcs::Arc(Arc::new(
        first.center,
        first.radius,
        start,
        sweep,
    )))
}

/// A path made of others joined end to end.
#[derive(Clone, Debug, PartialEq)]
pub struct Chain {
    /// Which of the paths handed in it is made of, in ascending order.
    pub members: Vec<usize>,
    pub path: Path,
}

impl Path {
    /// The same path travelled the other way.
    pub fn reversed(&self) -> Path {
        let count = self.vertices.len();
        let bulge = |vertex: usize| self.bulges.get(vertex).copied().unwrap_or(0.0);
        let bulges = (0..count)
            .map(|index| {
                // Segment `index` of the result is this segment of the
                // original, backwards. An open path has no last segment.
                if self.closed {
                    -bulge((2 * count - 2 - index) % count)
                } else if index + 1 < count {
                    -bulge(count - 2 - index)
                } else {
                    0.0
                }
            })
            .map(|bulge| bulge + 0.0) // no negative zeros
            .collect();
        Path {
            vertices: self.vertices.iter().rev().copied().collect(),
            bulges,
            closed: self.closed,
        }
    }

    fn ends(&self) -> Option<(Point2, Point2)> {
        Some((*self.vertices.first()?, *self.vertices.last()?))
    }

    /// This open path carried on by `next`, which starts where it ends.
    fn followed_by(&self, next: &Path) -> Path {
        let own = self.vertices.len().saturating_sub(1);
        let bulge_of = |path: &Path, vertex: usize| path.bulges.get(vertex).copied().unwrap_or(0.0);
        Path {
            vertices: self.vertices[..own]
                .iter()
                .chain(&next.vertices)
                .copied()
                .collect(),
            bulges: (0..own)
                .map(|vertex| bulge_of(self, vertex))
                .chain((0..next.vertices.len()).map(|vertex| bulge_of(next, vertex)))
                .collect(),
            closed: false,
        }
    }
}

/// Joins the open paths among `paths` that meet end to end, turning them
/// round as needed. Each chain runs the way its first member does, and one
/// that comes back to its own start is closed. Paths that meet nothing are
/// in no chain; closed paths are never members.
///
/// Where more than two ends meet at a point, the path that comes first in
/// `paths` is taken.
pub fn chain_paths(paths: &[Path], tolerance: LinearTolerance) -> Vec<Chain> {
    let mut free: Vec<usize> = (0..paths.len())
        .filter(|index| !paths[*index].closed && paths[*index].vertices.len() > 1)
        .collect();
    let mut chains = Vec::new();
    while !free.is_empty() {
        let first = free.remove(0);
        let mut path = paths[first].clone();
        let mut members = vec![first];
        while let Some((start, end)) = path.ends() {
            if tolerance.coincident(start, end) {
                break;
            }
            // Which free path has an end at `point`, and whether it is its start.
            let meeting = |point: Point2| {
                free.iter().enumerate().find_map(|(position, index)| {
                    let (its_start, its_end) = paths[*index].ends()?;
                    if tolerance.coincident(its_start, point) {
                        Some((position, true))
                    } else if tolerance.coincident(its_end, point) {
                        Some((position, false))
                    } else {
                        None
                    }
                })
            };
            if let Some((position, at_its_start)) = meeting(end) {
                let index = free.remove(position);
                members.push(index);
                path = if at_its_start {
                    path.followed_by(&paths[index])
                } else {
                    path.followed_by(&paths[index].reversed())
                };
            } else if let Some((position, at_its_start)) = meeting(start) {
                let index = free.remove(position);
                members.push(index);
                path = if at_its_start {
                    paths[index].reversed().followed_by(&path)
                } else {
                    paths[index].followed_by(&path)
                };
            } else {
                break;
            }
        }
        if members.len() > 1 {
            if let Some((start, end)) = path.ends()
                && tolerance.coincident(start, end)
            {
                path.vertices.pop();
                path.bulges.truncate(path.vertices.len());
                path.closed = true;
            }
            members.sort_unstable();
            chains.push(Chain { members, path });
        }
    }
    chains
}

#[cfg(test)]
mod tests {
    use std::f64::consts::{FRAC_PI_2, PI};

    use proptest::prelude::*;

    use super::*;
    use crate::Primitive;

    const TOLERANCE: LinearTolerance = LinearTolerance::DEFAULT;

    fn p(x: f64, y: f64) -> Point2 {
        Point2::new(x, y)
    }

    fn seg(x0: f64, y0: f64, x1: f64, y1: f64) -> Segment {
        Segment::new(p(x0, y0), p(x1, y1))
    }

    fn open(points: &[(f64, f64)], bulges: &[f64]) -> Path {
        let mut all = bulges.to_vec();
        all.resize(points.len(), 0.0);
        Path {
            vertices: points.iter().map(|(x, y)| p(*x, *y)).collect(),
            bulges: all,
            closed: false,
        }
    }

    // ── segments ──

    #[test]
    fn collinear_segments_become_the_segment_that_spans_them() {
        // Touching, overlapping, apart, and backwards: all on y = x.
        let joined = join_segments(
            &[
                seg(10.0, 10.0, 20.0, 20.0),
                seg(20.0, 20.0, 30.0, 30.0),
                seg(25.0, 25.0, 45.0, 45.0),
                seg(80.0, 80.0, 60.0, 60.0),
            ],
            TOLERANCE,
        );
        assert_eq!(joined, Some(seg(10.0, 10.0, 80.0, 80.0)));
    }

    #[test]
    fn the_joined_segment_runs_the_way_the_first_one_does() {
        let joined = join_segments(
            &[seg(50.0, 0.0, 40.0, 0.0), seg(0.0, 0.0, 10.0, 0.0)],
            TOLERANCE,
        );
        assert_eq!(joined, Some(seg(50.0, 0.0, 0.0, 0.0)));
    }

    #[test]
    fn segments_off_the_line_or_alone_are_not_joined() {
        let line = seg(0.0, 0.0, 100.0, 0.0);
        assert_eq!(join_segments(&[line], TOLERANCE), None);
        assert_eq!(join_segments(&[], TOLERANCE), None);
        // Parallel, a hair off: not the same line.
        assert_eq!(
            join_segments(&[line, seg(120.0, 1e-6, 150.0, 1e-6)], TOLERANCE),
            None
        );
        // Meeting at an angle.
        assert_eq!(
            join_segments(&[line, seg(100.0, 0.0, 150.0, 1.0)], TOLERANCE),
            None
        );
        // Within the tolerance it is the same line.
        assert!(join_segments(&[line, seg(120.0, 1e-10, 150.0, 0.0)], TOLERANCE).is_some());
    }

    // ── arcs ──

    fn arc(start_degrees: f64, sweep_degrees: f64) -> Arc {
        Arc::new(
            p(10.0, 20.0),
            50.0,
            start_degrees.to_radians(),
            sweep_degrees.to_radians(),
        )
    }

    fn joined_arc(arcs: &[Arc]) -> Arc {
        match join_arcs(arcs, TOLERANCE) {
            Ok(JoinedArcs::Arc(arc)) => arc,
            other => panic!("expected an arc, got {other:?}"),
        }
    }

    #[test]
    fn arcs_that_touch_or_overlap_become_one_arc_in_any_order() {
        for arcs in [
            [arc(0.0, 90.0), arc(90.0, 60.0)],
            [arc(90.0, 60.0), arc(0.0, 90.0)],
            [arc(0.0, 120.0), arc(90.0, 60.0)],
            [arc(0.0, 150.0), arc(30.0, 60.0)],
        ] {
            let joined = joined_arc(&arcs);
            assert!(normalize_angle(joined.start).abs() < 1e-12, "{joined:?}");
            assert!(
                (joined.sweep - 150f64.to_radians()).abs() < 1e-12,
                "{joined:?}"
            );
            assert_eq!((joined.center, joined.radius), (p(10.0, 20.0), 50.0));
        }
    }

    #[test]
    fn arcs_join_across_the_zero_angle_and_through_a_third() {
        let joined = joined_arc(&[arc(300.0, 60.0), arc(0.0, 45.0)]);
        assert!((joined.start - 300f64.to_radians()).abs() < 1e-12);
        assert!((joined.sweep - 105f64.to_radians()).abs() < 1e-12);

        // The first and the last only meet through the middle one, given last.
        let joined = joined_arc(&[arc(0.0, 30.0), arc(80.0, 40.0), arc(30.0, 50.0)]);
        assert!((joined.sweep - 120f64.to_radians()).abs() < 1e-12);
    }

    #[test]
    fn arcs_that_go_all_the_way_round_are_the_circle() {
        let circle = Circle::new(p(10.0, 20.0), 50.0);
        assert_eq!(
            join_arcs(&[arc(0.0, 180.0), arc(180.0, 180.0)], TOLERANCE),
            Ok(JoinedArcs::Circle(circle))
        );
        assert_eq!(
            join_arcs(
                &[arc(10.0, 200.0), arc(200.0, 100.0), arc(290.0, 90.0)],
                TOLERANCE
            ),
            Ok(JoinedArcs::Circle(circle))
        );
    }

    #[test]
    fn arcs_with_a_gap_or_of_different_circles_are_refused() {
        assert_eq!(
            join_arcs(&[arc(0.0, 90.0), arc(100.0, 60.0)], TOLERANCE),
            Err(ArcJoinError::Apart)
        );
        let elsewhere = Arc::new(p(10.0, 21.0), 50.0, 0.0, 1.0);
        let larger = Arc::new(p(10.0, 20.0), 51.0, 0.0, 1.0);
        for other in [elsewhere, larger] {
            assert_eq!(
                join_arcs(&[arc(0.0, 90.0), other], TOLERANCE),
                Err(ArcJoinError::NotOneCircle)
            );
        }
        assert_eq!(
            join_arcs(&[arc(0.0, 90.0)], TOLERANCE),
            Err(ArcJoinError::NotOneCircle)
        );
    }

    /// The acceptance criterion: ends are matched with the tolerance class.
    #[test]
    fn ends_are_matched_within_the_linear_tolerance_and_no_further() {
        // An end 5e-10 short of the next start, measured along the arc.
        let nearly = Arc::new(p(10.0, 20.0), 50.0, FRAC_PI_2 + 5e-10 / 50.0, 1.0);
        assert!(join_arcs(&[arc(0.0, 90.0), nearly], TOLERANCE).is_ok());
        let apart = Arc::new(p(10.0, 20.0), 50.0, FRAC_PI_2 + 5e-8 / 50.0, 1.0);
        assert_eq!(
            join_arcs(&[arc(0.0, 90.0), apart], TOLERANCE),
            Err(ArcJoinError::Apart)
        );

        let a = open(&[(0.0, 0.0), (100.0, 0.0)], &[]);
        let near = open(&[(100.0, 5e-10), (100.0, 50.0)], &[]);
        let far = open(&[(100.0, 5e-8), (100.0, 50.0)], &[]);
        assert_eq!(chain_paths(&[a.clone(), near], TOLERANCE).len(), 1);
        assert!(chain_paths(&[a.clone(), far.clone()], TOLERANCE).is_empty());
        // A wider tolerance, handed in, joins them.
        assert_eq!(chain_paths(&[a, far], LinearTolerance::new(1e-6)).len(), 1);
    }

    // ── paths ──

    #[test]
    fn reversing_a_path_keeps_its_shape_and_turns_its_arcs_over() {
        let path = open(&[(0.0, 0.0), (100.0, 0.0), (100.0, 100.0)], &[0.5, -1.0]);
        let reversed = path.reversed();
        assert_eq!(
            reversed,
            open(&[(100.0, 100.0), (100.0, 0.0), (0.0, 0.0)], &[1.0, -0.5])
        );
        assert_eq!(reversed.reversed(), path);

        let closed = Path {
            vertices: vec![p(0.0, 0.0), p(100.0, 0.0), p(100.0, 100.0)],
            bulges: vec![0.1, 0.2, 0.3],
            closed: true,
        };
        let reversed = closed.reversed();
        assert_eq!(
            reversed.vertices,
            vec![p(100.0, 100.0), p(100.0, 0.0), p(0.0, 0.0)]
        );
        assert_eq!(reversed.bulges, vec![-0.2, -0.1, -0.3]);
        assert_eq!(reversed.reversed(), closed);
    }

    #[test]
    fn paths_meeting_end_to_end_become_one_whichever_way_they_run() {
        let first = open(&[(0.0, 0.0), (100.0, 0.0)], &[]);
        let arc = open(&[(100.0, 0.0), (100.0, 100.0)], &[1.0]);
        let last = open(&[(100.0, 100.0), (0.0, 100.0)], &[]);
        let expected = open(
            &[(0.0, 0.0), (100.0, 0.0), (100.0, 100.0), (0.0, 100.0)],
            &[0.0, 1.0],
        );
        for (arc, last) in [
            (arc.clone(), last.clone()),
            (arc.reversed(), last.clone()),
            (arc.clone(), last.reversed()),
            (arc.reversed(), last.reversed()),
        ] {
            let chains = chain_paths(&[first.clone(), arc, last], TOLERANCE);
            assert_eq!(chains.len(), 1);
            assert_eq!(chains[0].members, vec![0, 1, 2]);
            assert_eq!(chains[0].path, expected);
        }
    }

    #[test]
    fn a_chain_grows_at_its_start_too_and_keeps_the_first_members_direction() {
        let middle = open(&[(100.0, 0.0), (200.0, 0.0)], &[]);
        let before = open(&[(100.0, 0.0), (0.0, 0.0)], &[0.5]);
        let after = open(&[(200.0, 0.0), (300.0, 50.0)], &[]);
        let chains = chain_paths(&[middle, before, after], TOLERANCE);
        assert_eq!(chains.len(), 1);
        assert_eq!(
            chains[0].path,
            open(
                &[(0.0, 0.0), (100.0, 0.0), (200.0, 0.0), (300.0, 50.0)],
                &[-0.5]
            )
        );
    }

    #[test]
    fn a_chain_that_returns_to_its_start_is_closed() {
        let sides = [
            open(&[(0.0, 0.0), (100.0, 0.0)], &[]),
            open(&[(100.0, 100.0), (100.0, 0.0)], &[]),
            open(&[(100.0, 100.0), (0.0, 100.0)], &[0.3]),
            open(&[(0.0, 100.0), (0.0, 0.0)], &[]),
        ];
        let chains = chain_paths(&sides, TOLERANCE);
        assert_eq!(chains.len(), 1);
        assert_eq!(
            chains[0].path,
            Path {
                vertices: vec![p(0.0, 0.0), p(100.0, 0.0), p(100.0, 100.0), p(0.0, 100.0)],
                bulges: vec![0.0, 0.0, 0.3, 0.0],
                closed: true,
            }
        );
    }

    #[test]
    fn separate_groups_make_separate_chains_and_loners_make_none() {
        let paths = [
            open(&[(0.0, 0.0), (10.0, 0.0)], &[]),
            open(&[(500.0, 0.0), (510.0, 0.0)], &[]),
            open(&[(10.0, 0.0), (10.0, 10.0)], &[]),
            open(&[(900.0, 900.0), (950.0, 900.0)], &[]), // meets nothing
            open(&[(510.0, 0.0), (510.0, 10.0)], &[]),
            // Closed: never a member, though a vertex is on an end.
            Path {
                vertices: vec![p(10.0, 10.0), p(20.0, 10.0), p(20.0, 20.0)],
                bulges: vec![0.0; 3],
                closed: true,
            },
        ];
        let chains = chain_paths(&paths, TOLERANCE);
        let members: Vec<&[usize]> = chains
            .iter()
            .map(|chain| chain.members.as_slice())
            .collect();
        assert_eq!(members, vec![&[0, 2][..], &[1, 4][..]]);
        assert!(chain_paths(&paths[3..4], TOLERANCE).is_empty());
    }

    #[test]
    fn where_three_ends_meet_the_earlier_path_is_taken() {
        let paths = [
            open(&[(0.0, 0.0), (10.0, 0.0)], &[]),
            open(&[(10.0, 0.0), (10.0, 10.0)], &[]),
            open(&[(10.0, 0.0), (10.0, -10.0)], &[]),
        ];
        let chains = chain_paths(&paths, TOLERANCE);
        // The third is left out: the point it shares is now inside the chain.
        assert_eq!(chains.len(), 1);
        assert_eq!(chains[0].members, vec![0, 1]);
    }

    fn length(path: &Path) -> f64 {
        path.primitives()
            .iter()
            .map(|primitive| match primitive {
                Primitive::Segment(segment) => segment.length(),
                Primitive::Arc(arc) => arc.radius * arc.sweep,
                Primitive::Circle(circle) => 2.0 * PI * circle.radius,
            })
            .sum()
    }

    proptest! {
        /// Cutting a path at its vertices, shuffling and turning the pieces
        /// round, then chaining them gives back one path of the same length
        /// through the same points.
        #[test]
        fn a_path_cut_into_its_segments_chains_back_into_one(
            points in proptest::collection::vec((-100i32..100, -100i32..100), 3..8),
            bulges in proptest::collection::vec(prop_oneof![Just(0.0), -1.2..1.2f64], 8),
            flips in proptest::collection::vec(any::<bool>(), 8),
            rotate in 0..8usize,
        ) {
            let mut vertices: Vec<Point2> = Vec::new();
            for (x, y) in points {
                let point = p(f64::from(x), f64::from(y));
                if !vertices.contains(&point) {
                    vertices.push(point);
                }
            }
            prop_assume!(vertices.len() >= 3);
            let whole = Path {
                bulges: (0..vertices.len())
                    .map(|index| if index + 1 < vertices.len() { bulges[index] } else { 0.0 })
                    .collect(),
                vertices,
                closed: false,
            };
            let mut pieces: Vec<Path> = (0..whole.vertices.len() - 1)
                .map(|index| {
                    let piece = Path {
                        vertices: whole.vertices[index..=index + 1].to_vec(),
                        bulges: vec![whole.bulges[index], 0.0],
                        closed: false,
                    };
                    if flips[index] { piece.reversed() } else { piece }
                })
                .collect();
            let turn = rotate % pieces.len();
            pieces.rotate_left(turn);

            let chains = chain_paths(&pieces, TOLERANCE);
            prop_assert_eq!(chains.len(), 1);
            let joined = &chains[0].path;
            prop_assert_eq!(chains[0].members.len(), pieces.len());
            prop_assert!((length(joined) - length(&whole)).abs() < 1e-9 * length(&whole).max(1.0));
            prop_assert!(!joined.closed);
            let same = joined.vertices == whole.vertices;
            let mirrored = joined.reversed().vertices == whole.vertices;
            prop_assert!(same || mirrored, "{:?}", joined);
        }
    }
}

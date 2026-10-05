//! Rounding and bevelling the corners of a polyline (DESIGN §22): the
//! fillet arc or chamfer line becomes part of the path.
//!
//! A corner is a vertex where two straight segments meet at an angle. The
//! work is done by [`Corner`], exactly as for two separate lines; here the
//! result is spliced back into the path, which gains one vertex.

use cad_core::Point2;
use cad_core::tolerance::LinearTolerance;

use crate::path_offset::{PathSpan, path_spans};
use crate::polyline::{bulge_from_arc, is_straight};
use crate::{Corner, CornerError, Path, Segment};

/// What to put at a corner.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CornerJoin {
    /// An arc of this radius, tangent to both segments.
    Fillet(f64),
    /// A bevel cutting the segment that arrives at the corner back by the
    /// first distance and the one that leaves it by the second.
    Chamfer([f64; 2]),
}

/// Why a corner of a path could not be rounded or bevelled.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PathCornerError {
    /// Not a corner: an end of an open path, or no such vertex.
    NotACorner,
    /// One of the two segments is an arc.
    Curved,
    /// The path goes straight on there.
    Straight,
    /// A segment is too short for the radius or distance.
    TooLarge,
}

impl Path {
    /// The spans arriving at and leaving `vertex`, if it joins two.
    fn spans_at(&self, vertex: usize) -> Option<(PathSpan, PathSpan)> {
        let spans = path_spans(self);
        let count = self.vertices.len();
        if vertex >= count || spans.is_empty() {
            return None;
        }
        let closes = spans.len() == count;
        // Span `i` runs from vertex `i` to the next one.
        let arriving = match vertex {
            0 if closes => spans.len() - 1,
            0 => return None,
            _ => vertex - 1,
        };
        let leaving = (vertex < spans.len()).then_some(vertex)?;
        Some((spans[arriving], spans[leaving]))
    }

    /// The path with the corner at `vertex` replaced by a fillet arc or a
    /// chamfer line: one vertex becomes two.
    pub fn with_corner(
        &self,
        vertex: usize,
        join: CornerJoin,
        tolerance: LinearTolerance,
    ) -> Result<Path, PathCornerError> {
        let (arriving, leaving) = self.spans_at(vertex).ok_or(PathCornerError::NotACorner)?;
        if !is_straight(arriving.bulge) || !is_straight(leaving.bulge) {
            return Err(PathCornerError::Curved);
        }
        // Each segment is kept on the side away from the corner.
        let corner = Corner::new(
            &Segment::new(arriving.start, arriving.end),
            arriving.start,
            &Segment::new(leaving.start, leaving.end),
            leaving.end,
            tolerance,
        )
        .map_err(|_| PathCornerError::Straight)?;
        let too_large = |error: CornerError| match error {
            CornerError::Parallel => PathCornerError::Straight,
            CornerError::TooLarge | CornerError::NoSolution => PathCornerError::TooLarge,
        };
        // Where each segment now ends, and the bulge of what joins them.
        let (from, to, bulge): (Point2, Point2, f64) = match join {
            CornerJoin::Fillet(radius) => {
                let fillet = corner.fillet(radius, tolerance).map_err(too_large)?;
                let (from, to) = (fillet.lines[0].end, fillet.lines[1].start);
                let bulge = fillet.arc.map_or(0.0, |arc| {
                    let forwards =
                        arc.start_point().distance_to(from) <= arc.end_point().distance_to(from);
                    bulge_from_arc(&arc, forwards)
                });
                (from, to, bulge)
            }
            CornerJoin::Chamfer(distances) => {
                let chamfer = corner.chamfer(distances, tolerance).map_err(too_large)?;
                (chamfer.lines[0].end, chamfer.lines[1].start, 0.0)
            }
        };
        if tolerance.coincident(from, to) {
            // Radius or distances of zero: the corner is already sharp.
            return Ok(self.clone());
        }
        let mut path = self.clone();
        path.bulges.resize(path.vertices.len(), 0.0);
        path.vertices[vertex] = from;
        path.bulges[vertex] = bulge;
        path.vertices.insert(vertex + 1, to);
        path.bulges.insert(vertex + 1, 0.0);
        Ok(path)
    }

    /// The corner between the segments nearest to two picks: its vertex,
    /// and whether the first pick is on the segment that arrives there.
    /// `None` unless the picks are on two segments that share a vertex.
    pub fn corner_between(
        &self,
        first: Point2,
        second: Point2,
        tolerance: LinearTolerance,
    ) -> Option<(usize, bool)> {
        let (a, b) = (
            self.locate(first, tolerance)?,
            self.locate(second, tolerance)?,
        );
        // A pick exactly on a vertex is located at the start of the next
        // segment; nearest-segment order is what matters here.
        let count = path_spans(self).len();
        let follows = |earlier: usize, later: usize| {
            later == earlier + 1 || (self.closed && earlier + 1 == count && later == 0)
        };
        if follows(a.segment, b.segment) {
            Some(((a.segment + 1) % self.vertices.len(), true))
        } else if follows(b.segment, a.segment) {
            Some(((b.segment + 1) % self.vertices.len(), false))
        } else {
            None
        }
    }

    /// The path with every corner rounded or bevelled that can be, and how
    /// many were done and how many skipped — because a segment was too
    /// short, or an arc. Vertices where the path goes straight on are not
    /// corners and are not counted.
    pub fn with_every_corner(
        &self,
        join: CornerJoin,
        tolerance: LinearTolerance,
    ) -> (Path, usize, usize) {
        let mut path = self.clone();
        let (mut done, mut skipped) = (0, 0);
        let mut vertex = 0;
        for _ in 0..self.vertices.len() {
            match path.with_corner(vertex, join, tolerance) {
                Ok(next) if next.vertices.len() > path.vertices.len() => {
                    path = next;
                    done += 1;
                    vertex += 2; // past the two vertices the corner became
                }
                Ok(_) | Err(PathCornerError::NotACorner | PathCornerError::Straight) => vertex += 1,
                Err(PathCornerError::Curved | PathCornerError::TooLarge) => {
                    skipped += 1;
                    vertex += 1;
                }
            }
        }
        (path, done, skipped)
    }
}

#[cfg(test)]
mod tests {
    use std::f64::consts::SQRT_2;

    use super::*;
    use crate::Primitive;

    const TOLERANCE: LinearTolerance = LinearTolerance::DEFAULT;
    const QUARTER: f64 = SQRT_2 - 1.0;

    fn p(x: f64, y: f64) -> Point2 {
        Point2::new(x, y)
    }

    fn straight(points: &[(f64, f64)], closed: bool) -> Path {
        Path {
            vertices: points.iter().map(|(x, y)| p(*x, *y)).collect(),
            bulges: vec![0.0; points.len()],
            closed,
        }
    }

    /// Right 100, up 100: one corner, a left turn at (100, 0).
    fn corner() -> Path {
        straight(&[(0.0, 0.0), (100.0, 0.0), (100.0, 100.0)], false)
    }

    fn rectangle() -> Path {
        straight(
            &[(0.0, 0.0), (100.0, 0.0), (100.0, 60.0), (0.0, 60.0)],
            true,
        )
    }

    #[test]
    fn a_fillet_replaces_the_corner_with_a_tangent_arc_in_the_path() {
        let rounded = corner()
            .with_corner(1, CornerJoin::Fillet(20.0), TOLERANCE)
            .unwrap();
        assert_eq!(
            rounded.vertices,
            vec![p(0.0, 0.0), p(80.0, 0.0), p(100.0, 20.0), p(100.0, 100.0)]
        );
        // A left turn: a counter-clockwise quarter arc.
        assert!(
            (rounded.bulges[1] - QUARTER).abs() < 1e-12,
            "{:?}",
            rounded.bulges
        );
        assert_eq!((rounded.bulges[0], rounded.bulges[2]), (0.0, 0.0));
        let Primitive::Arc(arc) = rounded.primitives()[1] else {
            panic!("the corner is now an arc");
        };
        assert!(arc.center.distance_to(p(80.0, 20.0)) < 1e-9 && (arc.radius - 20.0).abs() < 1e-9);
    }

    #[test]
    fn a_right_turn_gets_a_clockwise_arc() {
        let down = straight(&[(0.0, 0.0), (100.0, 0.0), (100.0, -100.0)], false);
        let rounded = down
            .with_corner(1, CornerJoin::Fillet(20.0), TOLERANCE)
            .unwrap();
        assert!((rounded.bulges[1] + QUARTER).abs() < 1e-12);
        assert_eq!(rounded.vertices[2], p(100.0, -20.0));
    }

    #[test]
    fn a_chamfer_replaces_the_corner_with_a_bevel() {
        let bevelled = corner()
            .with_corner(1, CornerJoin::Chamfer([30.0, 10.0]), TOLERANCE)
            .unwrap();
        // 30 back along the segment that arrives, 10 along the one that leaves.
        assert_eq!(
            bevelled,
            straight(
                &[(0.0, 0.0), (70.0, 0.0), (100.0, 10.0), (100.0, 100.0)],
                false
            )
        );
    }

    #[test]
    fn the_first_vertex_of_a_closed_path_is_a_corner_too() {
        let rounded = rectangle()
            .with_corner(0, CornerJoin::Fillet(10.0), TOLERANCE)
            .unwrap();
        assert!(rounded.closed);
        assert_eq!(rounded.vertices.len(), 5);
        // The closing segment now ends at (0, 10); the arc runs on to (10, 0).
        assert_eq!(
            (rounded.vertices[0], rounded.vertices[1]),
            (p(0.0, 10.0), p(10.0, 0.0))
        );
        assert!((rounded.bulges[0] - QUARTER).abs() < 1e-12);
    }

    #[test]
    fn what_is_not_a_corner_is_refused_with_the_reason() {
        let join = CornerJoin::Fillet(10.0);
        assert_eq!(
            corner().with_corner(0, join, TOLERANCE),
            Err(PathCornerError::NotACorner)
        );
        assert_eq!(
            corner().with_corner(2, join, TOLERANCE),
            Err(PathCornerError::NotACorner)
        );
        assert_eq!(
            corner().with_corner(9, join, TOLERANCE),
            Err(PathCornerError::NotACorner)
        );
        let through = straight(&[(0.0, 0.0), (50.0, 0.0), (100.0, 0.0)], false);
        assert_eq!(
            through.with_corner(1, join, TOLERANCE),
            Err(PathCornerError::Straight)
        );
        assert_eq!(
            corner().with_corner(1, CornerJoin::Fillet(100.0), TOLERANCE),
            Err(PathCornerError::TooLarge)
        );
        let mut curved = corner();
        curved.bulges[1] = 0.5;
        assert_eq!(
            curved.with_corner(1, join, TOLERANCE),
            Err(PathCornerError::Curved)
        );
        // Nothing to do is not an error.
        assert_eq!(
            corner().with_corner(1, CornerJoin::Fillet(0.0), TOLERANCE),
            Ok(corner())
        );
    }

    #[test]
    fn two_picks_on_neighbouring_segments_name_the_corner_between_them() {
        let path = rectangle();
        // Bottom then right: the corner at vertex 1, first pick on the arriving segment.
        assert_eq!(
            path.corner_between(p(50.0, 0.0), p(100.0, 30.0), TOLERANCE),
            Some((1, true))
        );
        assert_eq!(
            path.corner_between(p(100.0, 30.0), p(50.0, 0.0), TOLERANCE),
            Some((1, false))
        );
        // Left (the closing segment) then bottom: the corner at vertex 0.
        assert_eq!(
            path.corner_between(p(0.0, 30.0), p(50.0, 0.0), TOLERANCE),
            Some((0, true))
        );
        // The same segment twice, or opposite sides: no corner between them.
        assert_eq!(
            path.corner_between(p(20.0, 0.0), p(80.0, 0.0), TOLERANCE),
            None
        );
        assert_eq!(
            path.corner_between(p(50.0, 0.0), p(50.0, 60.0), TOLERANCE),
            None
        );
        // An open path does not wrap.
        let open = straight(
            &[(0.0, 0.0), (100.0, 0.0), (100.0, 60.0), (0.0, 60.0)],
            false,
        );
        assert_eq!(
            open.corner_between(p(50.0, 60.0), p(50.0, 0.0), TOLERANCE),
            None
        );
    }

    #[test]
    fn every_corner_of_a_closed_path_is_rounded() {
        let (rounded, done, skipped) =
            rectangle().with_every_corner(CornerJoin::Fillet(10.0), TOLERANCE);
        assert_eq!((done, skipped), (4, 0));
        assert_eq!(rounded.vertices.len(), 8);
        assert!(rounded.closed);
        let arcs = rounded
            .primitives()
            .iter()
            .filter(|primitive| matches!(primitive, Primitive::Arc(_)))
            .count();
        assert_eq!(arcs, 4);
        // The perimeter loses eight setbacks of 10 and gains one full circle of radius 10.
        let length: f64 = rounded
            .primitives()
            .iter()
            .map(|primitive| match primitive {
                Primitive::Segment(segment) => segment.length(),
                Primitive::Arc(arc) => arc.radius * arc.sweep,
                Primitive::Circle(_) => 0.0,
            })
            .sum();
        assert!((length - (320.0 - 80.0 + std::f64::consts::TAU * 10.0)).abs() < 1e-9);
    }

    #[test]
    fn corners_that_do_not_fit_are_skipped_and_counted() {
        // A long run with a short jog of 5: the two corners of the jog
        // cannot take a radius of 10; the corner after it can.
        let jogged = straight(
            &[
                (0.0, 0.0),
                (100.0, 0.0),
                (100.0, 5.0),
                (200.0, 5.0),
                (200.0, 100.0),
            ],
            false,
        );
        let (rounded, done, skipped) =
            jogged.with_every_corner(CornerJoin::Fillet(10.0), TOLERANCE);
        assert_eq!((done, skipped), (1, 2));
        assert_eq!(rounded.vertices.len(), 6);
        assert_eq!(
            &rounded.vertices[..3],
            &jogged.vertices[..3],
            "the jog is untouched"
        );

        // Every corner chamfered; the open ends are not corners.
        let (bevelled, done, skipped) =
            corner().with_every_corner(CornerJoin::Chamfer([10.0, 10.0]), TOLERANCE);
        assert_eq!((done, skipped), (1, 0));
        assert_eq!(bevelled.vertices.len(), 4);
    }

    #[test]
    fn neighbouring_fillets_share_a_segment_without_overlapping() {
        // A 30-long segment between two corners: radius 10 fits twice (20 ≤ 30),
        // radius 20 fits once and then no longer.
        let zigzag = straight(
            &[(0.0, 0.0), (100.0, 0.0), (100.0, 30.0), (200.0, 30.0)],
            false,
        );
        let (_, done, skipped) = zigzag.with_every_corner(CornerJoin::Fillet(10.0), TOLERANCE);
        assert_eq!((done, skipped), (2, 0));
        let (rounded, done, skipped) =
            zigzag.with_every_corner(CornerJoin::Fillet(20.0), TOLERANCE);
        assert_eq!((done, skipped), (1, 1));
        // What was returned is still a well-formed path along the original.
        assert!(
            rounded
                .vertices
                .windows(2)
                .all(|pair| pair[0].distance_to(pair[1]) > 1e-9)
        );
    }
}

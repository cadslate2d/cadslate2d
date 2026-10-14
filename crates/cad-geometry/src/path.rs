//! A polyline as a path that can be measured along and cut (DESIGN §22):
//! the geometry under TRIM and EXTEND for polylines.
//!
//! A position on a path is a *location*: which segment, and how far along
//! it from 0 to 1 — by length on a straight segment, by angle on an arc.
//! Locations order the points of a path from its start to its end, which is
//! all trimming needs.

use cad_core::Point2;
use cad_core::tolerance::{LinearTolerance, ParametricTolerance};

use crate::polyline::{arc_from_bulge, bulge_primitive, is_straight, polyline_spans};
use crate::trim::{OpenCurve, Trimmed};
use crate::{Arc, Primitive, Segment, extend_arc, extend_segment};

/// The bulge of a `share` of a segment with this bulge: the same turn rate
/// over that share of the included angle. A whole segment keeps its bulge
/// to the last bit.
pub(crate) fn partial_bulge(bulge: f64, share: f64) -> f64 {
    if is_straight(bulge) {
        0.0
    } else if ParametricTolerance::DEFAULT.eq(share, 1.0) {
        bulge
    } else {
        (bulge.atan() * share).tan()
    }
}

/// The vertices of a polyline, the bulge of the segment starting at each,
/// and whether it closes back to its first vertex.
#[derive(Clone, Debug, PartialEq)]
pub struct Path {
    pub vertices: Vec<Point2>,
    /// One per vertex. See `crate::polyline` for the convention.
    pub bulges: Vec<f64>,
    pub closed: bool,
}

/// A point on a path.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Location {
    /// Index of the segment, in the order of [`Path::spans`].
    pub segment: usize,
    /// How far along that segment, from 0 at its start to 1 at its end.
    pub t: f64,
    /// The point itself, kept exactly as it was given.
    pub point: Point2,
}

impl Location {
    /// A single number that orders locations along the path.
    fn along(&self) -> f64 {
        self.segment as f64 + self.t
    }
}

/// Why a path could not be trimmed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PathTrimError {
    /// No cut divides the path.
    NoCut,
    /// A closed path has no ends: it takes two distinct cuts to remove anything.
    NeedsTwoCuts,
}

impl Path {
    fn spans(&self) -> Vec<(usize, usize)> {
        polyline_spans(self.vertices.len(), self.closed, |index| self.bulge(index)).collect()
    }

    fn bulge(&self, vertex: usize) -> f64 {
        self.bulges.get(vertex).copied().unwrap_or(0.0)
    }

    /// The signed area the path encloses when closed back to its first
    /// vertex — positive counter-clockwise — exactly, the arc segments'
    /// circular segments included (#197): the polygon of the vertices,
    /// plus, for each bulged segment, the area between its chord and its
    /// arc on the side the arc bulges to.
    pub fn area(&self) -> f64 {
        let polygon = crate::signed_area(&self.vertices);
        let segments: f64 = self
            .spans()
            .into_iter()
            .map(|(from, to)| {
                let bulge = self.bulge(from);
                if is_straight(bulge) {
                    return 0.0;
                }
                match arc_from_bulge(self.vertices[from], self.vertices[to], bulge) {
                    // A circular segment: r²(θ − sin θ)/2, bowing to the
                    // right of the chord for a positive (counter-clockwise)
                    // bulge, to the left for a negative.
                    Some(arc) => {
                        let segment = arc.radius * arc.radius * (arc.sweep - arc.sweep.sin()) / 2.0;
                        segment.copysign(bulge)
                    }
                    None => 0.0,
                }
            })
            .sum();
        polygon + segments
    }

    /// The point `distance` along the path from its first vertex, through
    /// its arcs by arc length (#198); `None` past the end, or before the
    /// start.
    pub fn point_along(&self, distance: f64) -> Option<Point2> {
        if distance.is_nan() || distance < 0.0 {
            return None;
        }
        let mut left = distance;
        let spans = self.spans();
        for (index, (from, to)) in spans.iter().enumerate() {
            let primitive =
                bulge_primitive(self.vertices[*from], self.vertices[*to], self.bulge(*from));
            let length = primitive.length();
            // The end of the last span counts as on the path.
            let last = index + 1 == spans.len();
            if left <= length
                || (last
                    && LinearTolerance::DEFAULT
                        .around(&[self.vertices[*from], self.vertices[*to]])
                        .is_zero(left - length))
            {
                let along = left.min(length);
                return Some(match primitive {
                    Primitive::Segment(segment) => {
                        if length > 0.0 {
                            segment.start + (segment.end - segment.start) * (along / length)
                        } else {
                            segment.start
                        }
                    }
                    // A positive bulge runs the arc counter-clockwise from
                    // its start; a negative one runs it back from its end.
                    Primitive::Arc(arc) => {
                        let turned = along / arc.radius;
                        if self.bulge(*from) > 0.0 {
                            arc.point_at(arc.start + turned)
                        } else {
                            arc.point_at(arc.start + arc.sweep - turned)
                        }
                    }
                    Primitive::Circle(circle) => circle.point_at(along / circle.radius),
                });
            }
            left -= length;
        }
        None
    }

    /// The length along the path from its first vertex to `location`.
    pub fn length_to(&self, location: Location) -> f64 {
        let spans = self.spans();
        let before: f64 = spans
            .iter()
            .take(location.segment)
            .map(|(from, to)| {
                bulge_primitive(self.vertices[*from], self.vertices[*to], self.bulge(*from))
                    .length()
            })
            .sum();
        let own = spans.get(location.segment).map_or(0.0, |(from, to)| {
            bulge_primitive(self.vertices[*from], self.vertices[*to], self.bulge(*from)).length()
        });
        before + own * location.t
    }

    /// The length along the path: its segments and arcs end to end.
    pub fn length(&self) -> f64 {
        self.primitives().iter().map(Primitive::length).sum()
    }

    /// The path's geometry, one primitive per segment.
    pub fn primitives(&self) -> Vec<Primitive> {
        self.spans()
            .into_iter()
            .map(|(from, to)| {
                crate::bulge_primitive(self.vertices[from], self.vertices[to], self.bulge(from))
            })
            .collect()
    }

    /// The location of the point of the path nearest to `point`. `None` for
    /// a path without segments.
    pub fn locate(&self, point: Point2, tolerance: LinearTolerance) -> Option<Location> {
        let spans = self.spans();
        let (segment, _) = self
            .primitives()
            .iter()
            .map(|primitive| primitive.distance_to_point(point))
            .enumerate()
            .min_by(|a, b| a.1.total_cmp(&b.1))?;
        let (from, to) = spans[segment];
        let (start, end) = (self.vertices[from], self.vertices[to]);
        let bulge = self.bulge(from);
        let t = match arc_from_bulge(start, end, bulge) {
            None => Segment::new(start, end).parameter_of(point),
            // The stored arc is counter-clockwise; a clockwise segment runs it backwards.
            Some(arc) if bulge > 0.0 => arc.parameter_of(point),
            Some(arc) => 1.0 - arc.parameter_of(point),
        };
        // A point on a shared vertex belongs to the start of the next segment,
        // so every vertex has one location.
        let at_segment_end = tolerance.coincident(point, end);
        let has_next = segment + 1 < spans.len() || self.closed;
        Some(if at_segment_end && has_next {
            Location {
                segment: (segment + 1) % spans.len(),
                t: 0.0,
                point,
            }
        } else if at_segment_end {
            Location {
                segment,
                t: 1.0,
                point,
            }
        } else if tolerance.coincident(point, start) {
            Location {
                segment,
                t: 0.0,
                point,
            }
        } else {
            Location { segment, t, point }
        })
    }

    fn start(&self) -> Option<Location> {
        let first = *self.vertices.first()?;
        (self.vertices.len() > 1).then_some(Location {
            segment: 0,
            t: 0.0,
            point: first,
        })
    }

    fn end(&self) -> Option<Location> {
        let spans = self.spans();
        let (_, last) = *spans.last()?;
        Some(Location {
            segment: spans.len() - 1,
            t: 1.0,
            point: self.vertices[last],
        })
    }

    /// The open path from `from` forwards to `to`. On a closed path, a `to`
    /// that lies before `from` is reached by going on round through the
    /// first vertex.
    pub fn between(&self, from: Location, to: Location, tolerance: LinearTolerance) -> Path {
        let spans = self.spans();
        let count = spans.len();
        let wraps = self.closed && to.along() < from.along();

        // (start point, bulge of the piece that starts there), then the last point.
        let mut pieces: Vec<(Point2, f64)> = Vec::new();
        let mut push = |start: Point2, bulge: f64, share: f64| {
            pieces.push((start, partial_bulge(bulge, share)));
        };

        let mut segment = from.segment;
        let mut t = from.t;
        let mut start = from.point;
        let mut first = true;
        loop {
            let (a, b) = spans[segment];
            let last = segment == to.segment && !(first && wraps);
            let until = if last { to.t } else { 1.0 };
            push(start, self.bulge(a), until - t);
            if last {
                break;
            }
            segment = (segment + 1) % count;
            start = self.vertices[b];
            t = 0.0;
            first = false;
        }

        // Drop pieces of no length — a cut exactly on a vertex makes one.
        let mut vertices: Vec<Point2> = Vec::with_capacity(pieces.len() + 1);
        let mut bulges: Vec<f64> = Vec::with_capacity(pieces.len() + 1);
        let ends = pieces
            .iter()
            .skip(1)
            .map(|(point, _)| *point)
            .chain([to.point]);
        for ((point, bulge), end) in pieces.iter().zip(ends) {
            if !tolerance.coincident(*point, end) {
                vertices.push(*point);
                bulges.push(*bulge);
            }
        }
        let closes_on_itself = vertices
            .last()
            .is_some_and(|last| tolerance.coincident(*last, to.point));
        if !closes_on_itself {
            vertices.push(to.point);
            bulges.push(0.0);
        }
        Path {
            vertices,
            bulges,
            closed: false,
        }
    }

    /// The path with the stretch around `pick` removed, between the nearest
    /// `cuts` on either side. An open path leaves one or two open paths; a
    /// closed one, which needs two distinct cuts, leaves one open path.
    pub fn trimmed(
        &self,
        cuts: &[Point2],
        pick: Point2,
        tolerance: LinearTolerance,
    ) -> Result<Trimmed<Path>, PathTrimError> {
        let (Some(start), Some(end), Some(at)) =
            (self.start(), self.end(), self.locate(pick, tolerance))
        else {
            return Err(PathTrimError::NoCut);
        };
        // Distinct cuts, located and ordered along the path.
        let mut located: Vec<Location> = Vec::new();
        for cut in cuts {
            let seen = located
                .iter()
                .any(|known| tolerance.coincident(known.point, *cut));
            if let Some(location) = self.locate(*cut, tolerance).filter(|_| !seen) {
                located.push(location);
            }
        }
        located.sort_by(|a, b| a.along().total_cmp(&b.along()));

        if self.closed {
            if located.len() < 2 {
                return Err(if located.is_empty() {
                    PathTrimError::NoCut
                } else {
                    PathTrimError::NeedsTwoCuts
                });
            }
            // The gap runs from the last cut at or before the pick to the next.
            let next = located
                .iter()
                .position(|cut| cut.along() > at.along())
                .unwrap_or(0);
            let previous = (next + located.len() - 1) % located.len();
            return Ok(Trimmed {
                kept: vec![self.between(located[next], located[previous], tolerance)],
                removed: self.between(located[previous], located[next], tolerance),
            });
        }

        // An open path: its own ends divide nothing.
        located.retain(|cut| {
            !tolerance.coincident(cut.point, start.point)
                && !tolerance.coincident(cut.point, end.point)
        });
        let before = located.iter().rev().find(|cut| cut.along() < at.along());
        let after = located.iter().find(|cut| cut.along() >= at.along());
        if before.is_none() && after.is_none() {
            return Err(PathTrimError::NoCut);
        }
        let from = before.copied().unwrap_or(start);
        let to = after.copied().unwrap_or(end);
        let head = before.map(|cut| self.between(start, *cut, tolerance));
        let tail = after.map(|cut| self.between(*cut, end, tolerance));
        Ok(Trimmed {
            kept: head.into_iter().chain(tail).collect(),
            removed: self.between(from, to, tolerance),
        })
    }

    /// What is left of the path broken between two points on it.
    ///
    /// An open path loses what lies between them and leaves up to two paths;
    /// with the two points the same it is split there. A closed path loses
    /// the part from the first point onwards, in its own direction, to the
    /// second, and what stays is one open path.
    ///
    /// `None` when nothing would change or nothing would be left: a split at
    /// an end, a break from end to end, one point on a closed path.
    pub fn broken(
        &self,
        first: Point2,
        second: Point2,
        tolerance: LinearTolerance,
    ) -> Option<Vec<Path>> {
        let (start, end) = (self.start()?, self.end()?);
        let (a, b) = (
            self.locate(first, tolerance)?,
            self.locate(second, tolerance)?,
        );
        // The points themselves may be off the path: use where they land on it.
        let on_path = |location: Location| Location {
            point: self.point_of(location),
            ..location
        };
        let (a, b) = (on_path(a), on_path(b));
        let same = tolerance.coincident(a.point, b.point);
        if self.closed {
            return (!same).then(|| vec![self.between(b, a, tolerance)]);
        }
        let (from, to) = if a.along() <= b.along() {
            (a, b)
        } else {
            (b, a)
        };
        let head = (!tolerance.coincident(from.point, start.point))
            .then(|| self.between(start, from, tolerance));
        let tail =
            (!tolerance.coincident(to.point, end.point)).then(|| self.between(to, end, tolerance));
        match (head, tail) {
            (None, None) => None,
            (None, Some(_)) | (Some(_), None) if same => None,
            (head, tail) => Some(head.into_iter().chain(tail).collect()),
        }
    }

    /// The point of the path at a location: the fraction along its segment.
    fn point_of(&self, location: Location) -> Point2 {
        let spans = self.spans();
        let (from, to) = spans[location.segment];
        crate::PathSpan {
            start: self.vertices[from],
            end: self.vertices[to],
            bulge: self.bulge(from),
        }
        .point_at(location.t)
    }

    /// The open path with its last segment (or, with `from_end` false, its
    /// first) lengthened until it meets `boundary`: a straight segment
    /// straight on, an arc along its circle. `None` for a closed path, which
    /// has no ends, or when the boundary is never met.
    /// The path with its last segment (or its first) made `delta` longer
    /// — shorter for a negative delta — along its own line or circle
    /// (#201). `None` for a closed path, or when the segment would vanish
    /// or an arc would reach its full circle.
    pub fn lengthened(&self, from_end: bool, delta: f64) -> Option<Path> {
        let spans = self.spans();
        if self.closed || spans.is_empty() {
            return None;
        }
        let (from, to) = if from_end { *spans.last()? } else { spans[0] };
        let (start, end) = (self.vertices[from], self.vertices[to]);
        let bulge = self.bulge(from);
        let mut longer = self.clone();
        match arc_from_bulge(start, end, bulge) {
            None => {
                let segment = crate::lengthen_segment(&Segment::new(start, end), from_end, delta)?;
                longer.vertices[from] = segment.start;
                longer.vertices[to] = segment.end;
            }
            Some(arc) => {
                let forwards = bulge > 0.0;
                let grown = crate::lengthen_arc(&arc, from_end == forwards, delta)?;
                let (arc_start, arc_end) = (grown.start_point(), grown.end_point());
                let (new_start, new_end) = if forwards {
                    (arc_start, arc_end)
                } else {
                    (arc_end, arc_start)
                };
                if from_end {
                    longer.vertices[to] = new_end;
                } else {
                    longer.vertices[from] = new_start;
                }
                if longer.bulges.len() <= from {
                    longer.bulges.resize(from + 1, 0.0);
                }
                longer.bulges[from] = crate::bulge_from_arc(&grown, forwards);
            }
        }
        Some(longer)
    }

    pub fn extended(
        &self,
        from_end: bool,
        boundary: &[Primitive],
        tolerance: LinearTolerance,
    ) -> Option<Path> {
        let spans = self.spans();
        if self.closed || spans.is_empty() {
            return None;
        }
        let (from, to) = if from_end { *spans.last()? } else { spans[0] };
        let (start, end) = (self.vertices[from], self.vertices[to]);
        let bulge = self.bulge(from);
        let mut longer = self.clone();
        match arc_from_bulge(start, end, bulge) {
            None => {
                let segment =
                    extend_segment(&Segment::new(start, end), from_end, boundary, tolerance)?;
                longer.vertices[from] = segment.start;
                longer.vertices[to] = segment.end;
            }
            Some(arc) => {
                // The path's end is the stored arc's end only when the
                // segment runs counter-clockwise.
                let forwards = bulge > 0.0;
                let grown: Arc = extend_arc(&arc, from_end == forwards, boundary, tolerance)?;
                let (arc_start, arc_end) = (grown.start_point(), grown.end_point());
                let (new_start, new_end) = if forwards {
                    (arc_start, arc_end)
                } else {
                    (arc_end, arc_start)
                };
                // Only the extended end moves; the other keeps its exact vertex.
                if from_end {
                    longer.vertices[to] = new_end;
                } else {
                    longer.vertices[from] = new_start;
                }
                if longer.bulges.len() <= from {
                    longer.bulges.resize(from + 1, 0.0);
                }
                longer.bulges[from] = crate::bulge_from_arc(&grown, forwards);
            }
        }
        Some(longer)
    }
}

#[cfg(test)]
mod tests {
    use std::f64::consts::{PI, SQRT_2};

    use super::*;

    /// #197: the area of a closed path is exact through its arcs — a square
    /// is its side squared, a circle of two half-turn bulges is πr², a
    /// square with one side bowed out gains the half-disc, bowed in loses
    /// it — and the sign follows the direction.
    #[test]
    fn the_area_of_a_path_counts_its_arcs_exactly_and_signs_by_direction() {
        let square = Path {
            vertices: vec![p(0.0, 0.0), p(10.0, 0.0), p(10.0, 10.0), p(0.0, 10.0)],
            bulges: vec![],
            closed: true,
        };
        assert!((square.area() - 100.0).abs() < 1e-9);
        assert!((square.length() - 40.0).abs() < 1e-9);
        let clockwise = Path {
            vertices: square.vertices.iter().rev().copied().collect(),
            bulges: vec![],
            closed: true,
        };
        assert!((clockwise.area() + 100.0).abs() < 1e-9);
        let circle = Path {
            vertices: vec![p(-5.0, 0.0), p(5.0, 0.0)],
            bulges: vec![1.0, 1.0],
            closed: true,
        };
        assert!(
            (circle.area() - PI * 25.0).abs() < 1e-9,
            "{}",
            circle.area()
        );
        assert!((circle.length() - 2.0 * PI * 5.0).abs() < 1e-9);
        // A positive bulge is a counter-clockwise arc, which bows away
        // from its centre: to the right of the chord, outside this square.
        let bowed_out = Path {
            vertices: square.vertices.clone(),
            bulges: vec![1.0, 0.0, 0.0, 0.0],
            closed: true,
        };
        assert!((bowed_out.area() - (100.0 + PI * 25.0 / 2.0)).abs() < 1e-9);
        let bowed_in = Path {
            vertices: square.vertices.clone(),
            bulges: vec![-1.0, 0.0, 0.0, 0.0],
            closed: true,
        };
        assert!((bowed_in.area() - (100.0 - PI * 25.0 / 2.0)).abs() < 1e-9);
        // An open path is measured as if closed back to its start.
        let open = Path {
            closed: false,
            ..square.clone()
        };
        assert!((open.area() - 100.0).abs() < 1e-9);
        assert!((open.length() - 30.0).abs() < 1e-9);
    }

    /// #198: a distance along the path lands on it, through its arcs by
    /// arc length and the way they run; the length back to a location
    /// agrees; reversed, the path runs the other way round the same curve.
    #[test]
    fn a_distance_along_a_path_lands_on_it_through_its_arcs_either_way_round() {
        // A straight run, then a half-turn bulging to the right (down),
        // then straight back: a slot lying on its side.
        let slot = Path {
            vertices: vec![p(0.0, 0.0), p(10.0, 0.0), p(10.0, -10.0), p(0.0, -10.0)],
            bulges: vec![0.0, 1.0, 0.0, -1.0],
            closed: true,
        };
        let tolerance = LinearTolerance::DEFAULT;
        assert_eq!(slot.point_along(0.0), Some(p(0.0, 0.0)));
        assert_eq!(slot.point_along(5.0), Some(p(5.0, 0.0)));
        // Halfway round the half-turn: the arc bows to the right of its
        // downward chord, inside the square.
        let on_arc = slot.point_along(10.0 + PI * 5.0 / 2.0).unwrap();
        assert!(tolerance.coincident(on_arc, p(5.0, -5.0)), "{on_arc:?}");
        // Past the arc, on the way back.
        let back = slot.point_along(10.0 + PI * 5.0 + 4.0).unwrap();
        assert!(tolerance.coincident(back, p(6.0, -10.0)), "{back:?}");
        // The left-hand arc, run from the bottom up: clockwise.
        let left = slot.point_along(20.0 + PI * 5.0 + PI * 5.0 / 2.0).unwrap();
        assert!(tolerance.coincident(left, p(-5.0, -5.0)), "{left:?}");
        let total = 20.0 + PI * 10.0;
        assert!(tolerance.coincident(slot.point_along(total).unwrap(), p(0.0, 0.0)));
        assert_eq!(slot.point_along(total + 1.0), None);
        assert_eq!(slot.point_along(-1.0), None);
        for distance in [0.0, 3.0, 12.0, 25.0, 40.0] {
            let at = slot.point_along(distance).unwrap();
            let location = slot.locate(at, tolerance).unwrap();
            assert!(tolerance.coincident(location.point, at));
            assert!(
                (slot.length_to(location) - distance).abs() < 1e-9,
                "{distance}: {}",
                slot.length_to(location)
            );
        }
        let reversed = slot.reversed();
        assert!((reversed.length() - slot.length()).abs() < 1e-9);
        assert!(
            (reversed.area() + slot.area()).abs() < 1e-9,
            "the other way round"
        );
        // An open path reversed runs the same curve from its other end.
        let open = Path {
            vertices: vec![p(0.0, 0.0), p(10.0, 0.0), p(10.0, 10.0)],
            bulges: vec![0.0, 1.0, 0.0],
            closed: false,
        };
        let reversed = open.reversed();
        assert_eq!(
            reversed.vertices,
            vec![p(10.0, 10.0), p(10.0, 0.0), p(0.0, 0.0)]
        );
        assert_eq!(reversed.bulges, vec![-1.0, 0.0, 0.0]);
        let length = open.length();
        for distance in [0.0, 3.0, 12.0, 20.0, length] {
            let forwards = open.point_along(distance).unwrap();
            let backwards = reversed.point_along(length - distance).unwrap();
            assert!(
                tolerance.coincident(forwards, backwards),
                "{distance}: {forwards:?} {backwards:?}"
            );
        }
    }

    const TOLERANCE: LinearTolerance = LinearTolerance::DEFAULT;
    const QUARTER: f64 = SQRT_2 - 1.0;

    fn p(x: f64, y: f64) -> Point2 {
        Point2::new(x, y)
    }

    fn close(a: Point2, b: Point2) -> bool {
        a.distance_to(b) < 1e-9
    }

    fn straight(vertices: Vec<Point2>, closed: bool) -> Path {
        let bulges = vec![0.0; vertices.len()];
        Path {
            vertices,
            bulges,
            closed,
        }
    }

    /// An open staircase: right 100, up 100, right 100.
    fn steps() -> Path {
        straight(
            vec![p(0.0, 0.0), p(100.0, 0.0), p(100.0, 100.0), p(200.0, 100.0)],
            false,
        )
    }

    fn square() -> Path {
        straight(
            vec![p(0.0, 0.0), p(100.0, 0.0), p(100.0, 100.0), p(0.0, 100.0)],
            true,
        )
    }

    /// Right 100, a quarter arc turning up to (150, 50), then up 100.
    fn rounded() -> Path {
        Path {
            vertices: vec![p(0.0, 0.0), p(100.0, 0.0), p(150.0, 50.0), p(150.0, 150.0)],
            bulges: vec![0.0, QUARTER, 0.0, 0.0],
            closed: false,
        }
    }

    /// Every point of `part` lies on `whole`.
    fn lies_on(part: &Path, whole: &Path) -> bool {
        let distance = |point: Point2| {
            whole
                .primitives()
                .iter()
                .map(|primitive| primitive.distance_to_point(point))
                .fold(f64::INFINITY, f64::min)
        };
        part.primitives().iter().all(|primitive| match primitive {
            Primitive::Segment(segment) => {
                distance(segment.start) < 1e-9
                    && distance(segment.end) < 1e-9
                    && distance(segment.start + (segment.end - segment.start) * 0.5) < 1e-9
            }
            Primitive::Arc(arc) => {
                distance(arc.start_point()) < 1e-9
                    && distance(arc.mid_point()) < 1e-9
                    && distance(arc.end_point()) < 1e-9
            }
            Primitive::Circle(_) => false,
        })
    }

    #[test]
    fn locations_order_points_along_the_path() {
        let path = steps();
        let on_first = path.locate(p(30.0, 1.0), TOLERANCE).unwrap();
        assert_eq!((on_first.segment, on_first.t), (0, 0.3));
        let on_second = path.locate(p(101.0, 75.0), TOLERANCE).unwrap();
        assert_eq!((on_second.segment, on_second.t), (1, 0.75));
        assert!(on_first.along() < on_second.along());
        // A shared vertex belongs to the segment that starts there.
        let corner = path.locate(p(100.0, 0.0), TOLERANCE).unwrap();
        assert_eq!((corner.segment, corner.t), (1, 0.0));
        // The two ends.
        assert_eq!(path.locate(p(0.0, 0.0), TOLERANCE).unwrap().along(), 0.0);
        assert_eq!(
            path.locate(p(200.0, 100.0), TOLERANCE).unwrap().along(),
            3.0
        );
    }

    #[test]
    fn one_cut_removes_the_side_of_an_open_path_that_was_picked() {
        let cuts = [p(100.0, 40.0)]; // on the riser
        let parts = steps().trimmed(&cuts, p(150.0, 100.0), TOLERANCE).unwrap();
        assert_eq!(
            parts.kept,
            vec![straight(
                vec![p(0.0, 0.0), p(100.0, 0.0), p(100.0, 40.0)],
                false
            )]
        );
        assert_eq!(
            parts.removed,
            straight(
                vec![p(100.0, 40.0), p(100.0, 100.0), p(200.0, 100.0)],
                false
            )
        );

        let parts = steps().trimmed(&cuts, p(20.0, 0.0), TOLERANCE).unwrap();
        assert_eq!(
            parts.kept,
            vec![straight(
                vec![p(100.0, 40.0), p(100.0, 100.0), p(200.0, 100.0)],
                false
            )]
        );
    }

    #[test]
    fn a_pick_between_two_cuts_leaves_two_paths() {
        let cuts = [p(150.0, 100.0), p(50.0, 0.0)];
        let parts = steps().trimmed(&cuts, p(100.0, 50.0), TOLERANCE).unwrap();
        assert_eq!(
            parts.kept,
            vec![
                straight(vec![p(0.0, 0.0), p(50.0, 0.0)], false),
                straight(vec![p(150.0, 100.0), p(200.0, 100.0)], false)
            ]
        );
        assert_eq!(
            parts.removed,
            straight(
                vec![
                    p(50.0, 0.0),
                    p(100.0, 0.0),
                    p(100.0, 100.0),
                    p(150.0, 100.0)
                ],
                false
            )
        );
    }

    /// Regression: a cut exactly at a vertex.
    #[test]
    fn a_cut_at_a_vertex_leaves_no_segment_of_zero_length() {
        let cuts = [p(100.0, 0.0)]; // the first corner
        let parts = steps().trimmed(&cuts, p(150.0, 100.0), TOLERANCE).unwrap();
        assert_eq!(
            parts.kept,
            vec![straight(vec![p(0.0, 0.0), p(100.0, 0.0)], false)]
        );
        assert_eq!(
            parts.removed,
            straight(vec![p(100.0, 0.0), p(100.0, 100.0), p(200.0, 100.0)], false)
        );
        // Picked on the other side of the same vertex.
        let parts = steps().trimmed(&cuts, p(50.0, 0.0), TOLERANCE).unwrap();
        assert_eq!(
            parts.kept,
            vec![straight(
                vec![p(100.0, 0.0), p(100.0, 100.0), p(200.0, 100.0)],
                false
            )]
        );
        // Two cuts at two vertices, picked between.
        let parts = steps()
            .trimmed(&[p(100.0, 0.0), p(100.0, 100.0)], p(100.0, 50.0), TOLERANCE)
            .unwrap();
        assert_eq!(
            parts.kept,
            vec![
                straight(vec![p(0.0, 0.0), p(100.0, 0.0)], false),
                straight(vec![p(100.0, 100.0), p(200.0, 100.0)], false)
            ]
        );
    }

    #[test]
    fn cuts_at_the_ends_of_an_open_path_divide_nothing() {
        let ends = [p(0.0, 0.0), p(200.0, 100.0)];
        assert_eq!(
            steps().trimmed(&ends, p(100.0, 50.0), TOLERANCE),
            Err(PathTrimError::NoCut)
        );
        assert_eq!(
            steps().trimmed(&[], p(100.0, 50.0), TOLERANCE),
            Err(PathTrimError::NoCut)
        );
    }

    #[test]
    fn a_closed_path_opens_between_the_two_cuts_around_the_pick() {
        // Cuts on the bottom and the top edge; pick the right side.
        let cuts = [p(50.0, 0.0), p(50.0, 100.0)];
        let parts = square().trimmed(&cuts, p(100.0, 50.0), TOLERANCE).unwrap();
        // What stays runs from the top cut, round through the first vertex, to the bottom cut.
        assert_eq!(
            parts.kept,
            vec![straight(
                vec![p(50.0, 100.0), p(0.0, 100.0), p(0.0, 0.0), p(50.0, 0.0)],
                false
            )]
        );
        assert_eq!(
            parts.removed,
            straight(
                vec![p(50.0, 0.0), p(100.0, 0.0), p(100.0, 100.0), p(50.0, 100.0)],
                false
            )
        );

        // Pick the left side: the right half stays, without wrapping.
        let parts = square().trimmed(&cuts, p(0.0, 50.0), TOLERANCE).unwrap();
        assert_eq!(
            parts.kept,
            vec![straight(
                vec![p(50.0, 0.0), p(100.0, 0.0), p(100.0, 100.0), p(50.0, 100.0)],
                false
            )]
        );
    }

    #[test]
    fn a_closed_path_needs_two_distinct_cuts() {
        let one = [p(50.0, 0.0), p(50.0, 0.0)];
        assert_eq!(
            square().trimmed(&one, p(100.0, 50.0), TOLERANCE),
            Err(PathTrimError::NeedsTwoCuts)
        );
        assert_eq!(
            square().trimmed(&[], p(100.0, 50.0), TOLERANCE),
            Err(PathTrimError::NoCut)
        );
        // Cuts at two vertices of a closed path are real cuts.
        let corners = [p(0.0, 0.0), p(100.0, 100.0)];
        let parts = square()
            .trimmed(&corners, p(100.0, 20.0), TOLERANCE)
            .unwrap();
        assert_eq!(
            parts.kept,
            vec![straight(
                vec![p(100.0, 100.0), p(0.0, 100.0), p(0.0, 0.0)],
                false
            )]
        );
    }

    #[test]
    fn a_cut_on_an_arc_segment_keeps_both_parts_on_the_arc() {
        let path = rounded();
        // The middle of the quarter arc about (100, 50).
        let cut = p(100.0 + 50.0 / SQRT_2, 50.0 - 50.0 / SQRT_2);
        let parts = path.trimmed(&[cut], p(150.0, 120.0), TOLERANCE).unwrap();
        assert_eq!(parts.kept.len(), 1);
        let kept = &parts.kept[0];
        assert_eq!(kept.vertices, vec![p(0.0, 0.0), p(100.0, 0.0), cut]);
        // Half of a quarter turn: tan(45° / 4).
        assert!((kept.bulges[1] - (std::f64::consts::PI / 16.0).tan()).abs() < 1e-12);
        assert!(lies_on(kept, &path) && lies_on(&parts.removed, &path));
        assert!(close(parts.removed.vertices[0], cut));
        assert!((parts.removed.bulges[0] - (std::f64::consts::PI / 16.0).tan()).abs() < 1e-12);
    }

    #[test]
    fn a_clockwise_arc_segment_is_cut_the_right_way_round() {
        // Left to right over a hump: a clockwise semicircle from (0,0) to (100,0).
        let hump = Path {
            vertices: vec![p(0.0, 0.0), p(100.0, 0.0), p(200.0, 0.0)],
            bulges: vec![-1.0, 0.0, 0.0],
            closed: false,
        };
        let top = p(50.0, 50.0);
        let at = hump.locate(top, TOLERANCE).unwrap();
        assert!((at.t - 0.5).abs() < 1e-12 && at.segment == 0);
        // A quarter of the way along the hump from its start.
        let quarter = p(50.0 - 50.0 / SQRT_2, 50.0 / SQRT_2);
        assert!((hump.locate(quarter, TOLERANCE).unwrap().t - 0.25).abs() < 1e-12);

        let parts = hump.trimmed(&[top], p(150.0, 0.0), TOLERANCE).unwrap();
        let kept = &parts.kept[0];
        assert_eq!(kept.vertices, vec![p(0.0, 0.0), top]);
        assert!(kept.bulges[0] < 0.0, "still clockwise");
        assert!(lies_on(kept, &hump) && lies_on(&parts.removed, &hump));
    }

    // ── BREAK (#126) ──

    #[test]
    fn an_open_path_broken_between_two_points_leaves_two_paths() {
        let parts = steps()
            .broken(p(50.0, 3.0), p(150.0, 97.0), TOLERANCE)
            .unwrap();
        assert_eq!(
            parts,
            vec![
                straight(vec![p(0.0, 0.0), p(50.0, 0.0)], false),
                straight(vec![p(150.0, 100.0), p(200.0, 100.0)], false)
            ]
        );
        // The same point twice splits it; at a vertex, cleanly.
        let split = steps()
            .broken(p(100.0, 0.0), p(100.0, 0.0), TOLERANCE)
            .unwrap();
        assert_eq!(
            split,
            vec![
                straight(vec![p(0.0, 0.0), p(100.0, 0.0)], false),
                straight(vec![p(100.0, 0.0), p(100.0, 100.0), p(200.0, 100.0)], false)
            ]
        );
    }

    #[test]
    fn an_open_path_is_not_broken_at_its_ends_or_away_entirely() {
        assert_eq!(steps().broken(p(0.0, 0.0), p(0.0, 0.0), TOLERANCE), None);
        assert_eq!(
            steps().broken(p(200.0, 100.0), p(200.0, 100.0), TOLERANCE),
            None
        );
        assert_eq!(
            steps().broken(p(0.0, 0.0), p(200.0, 100.0), TOLERANCE),
            None
        );
        // From a point to the end is fine: the tail goes.
        assert_eq!(
            steps().broken(p(100.0, 50.0), p(200.0, 100.0), TOLERANCE),
            Some(vec![straight(
                vec![p(0.0, 0.0), p(100.0, 0.0), p(100.0, 50.0)],
                false
            )])
        );
    }

    #[test]
    fn a_closed_path_opens_from_the_first_point_onwards_to_the_second() {
        // The square runs counter-clockwise. From the bottom to the right
        // side: the bottom-right corner goes.
        let parts = square()
            .broken(p(50.0, 0.0), p(100.0, 50.0), TOLERANCE)
            .unwrap();
        assert_eq!(
            parts,
            vec![straight(
                vec![
                    p(100.0, 50.0),
                    p(100.0, 100.0),
                    p(0.0, 100.0),
                    p(0.0, 0.0),
                    p(50.0, 0.0)
                ],
                false
            )]
        );
        // The other way round, only that corner stays.
        let parts = square()
            .broken(p(100.0, 50.0), p(50.0, 0.0), TOLERANCE)
            .unwrap();
        assert_eq!(
            parts,
            vec![straight(
                vec![p(50.0, 0.0), p(100.0, 0.0), p(100.0, 50.0)],
                false
            )]
        );
        assert_eq!(
            square().broken(p(50.0, 0.0), p(50.0, 0.0), TOLERANCE),
            None,
            "one point"
        );
    }

    fn wall(x: f64) -> Primitive {
        Primitive::Segment(Segment::new(p(x, -500.0), p(x, 500.0)))
    }

    #[test]
    fn an_open_path_is_extended_from_either_end() {
        let longer = steps().extended(true, &[wall(260.0)], TOLERANCE).unwrap();
        assert_eq!(longer.vertices[3], p(260.0, 100.0));
        assert_eq!(&longer.vertices[..3], &steps().vertices[..3]);

        let longer = steps().extended(false, &[wall(-40.0)], TOLERANCE).unwrap();
        assert_eq!(longer.vertices[0], p(-40.0, 0.0));
        assert_eq!(&longer.vertices[1..], &steps().vertices[1..]);

        assert_eq!(
            steps().extended(true, &[wall(-40.0)], TOLERANCE),
            None,
            "behind that end"
        );
        assert_eq!(
            square().extended(true, &[wall(260.0)], TOLERANCE),
            None,
            "closed"
        );
    }

    #[test]
    fn an_arc_at_the_end_of_a_path_grows_along_its_circle() {
        // Right 100, then a counter-clockwise quarter arc up to (150, 50).
        let hook = Path {
            vertices: vec![p(0.0, 0.0), p(100.0, 0.0), p(150.0, 50.0)],
            bulges: vec![0.0, QUARTER, 0.0],
            closed: false,
        };
        // The circle about (100, 50) meets the line y = 100 at its top, (100, 100).
        let ceiling = Primitive::Segment(Segment::new(p(-500.0, 100.0), p(500.0, 100.0)));
        let longer = hook.extended(true, &[ceiling], TOLERANCE).unwrap();
        assert!(close(longer.vertices[2], p(100.0, 100.0)));
        assert!((longer.bulges[1] - 1.0).abs() < 1e-9, "now a semicircle");
        assert_eq!(&longer.vertices[..2], &hook.vertices[..2]);
    }

    #[test]
    fn a_clockwise_arc_at_the_start_of_a_path_grows_backwards() {
        // A clockwise quarter arc from (0, 50) over to (50, 100), then right.
        let hook = Path {
            vertices: vec![p(0.0, 50.0), p(50.0, 100.0), p(200.0, 100.0)],
            bulges: vec![-QUARTER, 0.0, 0.0],
            closed: false,
        };
        // Its circle, about (50, 50), meets the floor y = 0 at (50, 0).
        let floor = Primitive::Segment(Segment::new(p(-500.0, 0.0), p(500.0, 0.0)));
        let longer = hook.extended(false, &[floor], TOLERANCE).unwrap();
        assert!(
            close(longer.vertices[0], p(50.0, 0.0)),
            "{:?}",
            longer.vertices[0]
        );
        assert!(
            (longer.bulges[0] + 1.0).abs() < 1e-9,
            "a clockwise semicircle"
        );
        assert_eq!(&longer.vertices[1..], &hook.vertices[1..]);
    }
}

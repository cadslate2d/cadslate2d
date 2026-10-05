//! Offsetting a polyline (DESIGN §22, ADR-0011): the curve a fixed distance
//! to one side of a path.
//!
//! The method is the one the experiment in ADR-0011 chose:
//!
//! 1. **Raw offset.** Every segment is offset on its own — a line to a
//!    parallel line, an arc to a concentric arc — and at every vertex the
//!    two neighbours are joined by an arc about that vertex. Each of these
//!    pieces is, by construction, at the offset distance from *its own*
//!    part of the source.
//! 2. **Slice.** A point of a raw piece is wrong exactly where some *other*
//!    part of the source is nearer than the offset distance. That can only
//!    change where the piece crosses the boundary of the region within that
//!    distance of a source segment: its two parallels and the circles about
//!    its ends. Every raw piece is cut at every such crossing.
//! 3. **Filter.** Between cuts a piece is right throughout or wrong
//!    throughout, so one sample decides: keep it if its middle is at the
//!    offset distance from the source as a whole.
//! 4. **Stitch.** What is left is joined end to end into paths.
//!
//! The result is therefore correct by a property that can be tested: every
//! point of it is at the offset distance from the source.

use cad_core::tolerance::{AngularTolerance, LinearTolerance};
use cad_core::{Aabb, Point2, Vec2};

use crate::path::partial_bulge;
use crate::polyline::{arc_from_bulge, bulge_end_tangent, bulge_primitive, polyline_spans};
use crate::trim::OpenCurve;
use crate::{Arc, Circle, Orientation, Path, Primitive, Segment, orientation};

const LINEAR: LinearTolerance = LinearTolerance::DEFAULT;
/// How close to the offset distance a piece's middle must be to count as
/// being *at* it: a hundred default tolerances, the slack that offsetting,
/// intersecting, and measuring add up to.
const AT_DISTANCE: LinearTolerance = LinearTolerance::new(1e-7);
/// Probes per stretch between cuts, to catch a cut that was missed.
const PROBES: u32 = 7;
/// Halvings to locate where a stretch stops being valid: to one part in 10¹².
const BISECTIONS: u32 = 40;
/// How close two piece ends must be to be the same point when stitching.
/// Both come from intersections of the same curves, computed separately.
const SAME_POINT: LinearTolerance = LinearTolerance::new(1e-7);

/// One directed segment of a path: straight, or an arc (see
/// `crate::polyline` for the bulge).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PathSpan {
    pub start: Point2,
    pub end: Point2,
    pub bulge: f64,
}

impl PathSpan {
    pub fn primitive(&self) -> Primitive {
        bulge_primitive(self.start, self.end, self.bulge)
    }

    fn arc(&self) -> Option<Arc> {
        arc_from_bulge(self.start, self.end, self.bulge)
    }

    /// How far along the span the closest point to `point` is, from 0 to 1:
    /// by length on a straight span, by angle on an arc.
    pub fn parameter_of(&self, point: Point2) -> f64 {
        match self.arc() {
            None => Segment::new(self.start, self.end).parameter_of(point),
            // The stored arc is counter-clockwise; a clockwise span runs it backwards.
            Some(arc) if self.bulge > 0.0 => arc.parameter_of(point),
            Some(arc) => 1.0 - arc.parameter_of(point),
        }
    }

    /// The point a fraction `t` of the way along.
    pub fn point_at(&self, t: f64) -> Point2 {
        match self.arc() {
            None => self.start + (self.end - self.start) * t,
            Some(arc) if self.bulge > 0.0 => arc.point_at(arc.start + arc.sweep * t),
            Some(arc) => arc.point_at(arc.end() - arc.sweep * t),
        }
    }

    /// The direction of travel on leaving the start.
    fn start_tangent(&self) -> Vec2 {
        // The mirror image of the arrival: the chord turned back by half the angle.
        let leaving = bulge_end_tangent(self.end, self.start, self.bulge);
        Vec2::new(-leaving.x, -leaving.y)
    }

    /// The direction of travel on arriving at the end.
    fn end_tangent(&self) -> Vec2 {
        bulge_end_tangent(self.start, self.end, self.bulge)
    }

    /// The part between two points on the span, at fractions `from` and `to`.
    fn part(&self, from: (f64, Point2), to: (f64, Point2)) -> Self {
        Self {
            start: from.1,
            end: to.1,
            bulge: partial_bulge(self.bulge, to.0 - from.0),
        }
    }

    /// The span moved `signed` to the left of its direction of travel
    /// (negative: to the right). `None` when an arc would shrink to nothing
    /// or through its own centre.
    pub fn offset(&self, signed: f64) -> Option<Self> {
        match self.arc() {
            None => {
                let along = self.end - self.start;
                let length = along.length();
                if LINEAR.is_zero(length) {
                    return None;
                }
                let shift = Vec2::new(-along.y, along.x) * (signed / length);
                Some(Self {
                    start: self.start + shift,
                    end: self.end + shift,
                    bulge: 0.0,
                })
            }
            Some(arc) => {
                // Left of a counter-clockwise arc is towards its centre.
                let radius = if self.bulge > 0.0 {
                    arc.radius - signed
                } else {
                    arc.radius + signed
                };
                if radius <= 0.0 || LINEAR.is_zero(radius) {
                    return None;
                }
                let scale = radius / arc.radius;
                Some(Self {
                    start: arc.center + (self.start - arc.center) * scale,
                    end: arc.center + (self.end - arc.center) * scale,
                    bulge: self.bulge,
                })
            }
        }
    }
}

/// The spans of a path, in order.
pub fn path_spans(path: &Path) -> Vec<PathSpan> {
    let bulge_at = |index: usize| path.bulges.get(index).copied().unwrap_or(0.0);
    polyline_spans(path.vertices.len(), path.closed, bulge_at)
        .map(|(from, to)| PathSpan {
            start: path.vertices[from],
            end: path.vertices[to],
            bulge: path.bulges.get(from).copied().unwrap_or(0.0),
        })
        .collect()
}

fn left_normal(tangent: Vec2) -> Option<Vec2> {
    let length = tangent.length();
    (length > 0.0).then(|| Vec2::new(-tangent.y, tangent.x) * (1.0 / length))
}

/// The arc about the vertex between `before` and `after` that joins their
/// offsets: from the end of one to the start of the other, turning the way
/// the path turns there. `None` where the path goes straight on.
fn join(before: &PathSpan, after: &PathSpan, signed: f64) -> Option<PathSpan> {
    let (arriving, leaving) = (before.end_tangent(), after.start_tangent());
    let turn = (arriving.x * leaving.y - arriving.y * leaving.x).atan2(arriving.dot(leaving));
    if AngularTolerance::DEFAULT.is_zero(turn) {
        return None;
    }
    let vertex = before.end;
    let start = vertex + left_normal(arriving)? * signed;
    let end = vertex + left_normal(leaving)? * signed;
    (!LINEAR.coincident(start, end)).then(|| PathSpan {
        start,
        end,
        bulge: (turn / 4.0).tan(),
    })
}

/// Step 1: every span offset on its own, with an arc about each vertex
/// joining neighbours. Self-crossing and partly wrong; see the module text.
pub fn raw_offset(path: &Path, signed: f64) -> Vec<PathSpan> {
    let spans = path_spans(path);
    let mut raw = Vec::with_capacity(2 * spans.len());
    for (index, span) in spans.iter().enumerate() {
        raw.extend(span.offset(signed));
        let next = match index + 1 {
            next if next < spans.len() => Some(next),
            _ if path.closed && spans.len() > 1 => Some(0),
            _ => None,
        };
        if let Some(next) = next {
            raw.extend(join(span, &spans[next], signed));
        }
    }
    raw
}

/// The boundary of the region within `distance` of a span: its offsets to
/// both sides and the circles about its two ends.
fn reach_boundary(span: &PathSpan, distance: f64) -> Vec<Primitive> {
    let sides = [span.offset(distance), span.offset(-distance)];
    sides
        .into_iter()
        .flatten()
        .map(|side| side.primitive())
        .chain([
            Primitive::Circle(Circle::new(span.start, distance)),
            Primitive::Circle(Circle::new(span.end, distance)),
        ])
        .collect()
}

/// What the source looks like to the slicing and filtering steps: for each
/// of its segments, the segment, the boundary of its reach, and a box around
/// that reach.
struct Source {
    distance: f64,
    segments: Vec<SourceSegment>,
}

struct SourceSegment {
    shape: Primitive,
    boundary: Vec<Primitive>,
    /// Everything within the offset distance of the segment is inside this.
    reach: Aabb,
}

impl Source {
    fn new(spans: &[PathSpan], distance: f64) -> Self {
        let segments = spans
            .iter()
            .map(|span| {
                let shape = span.primitive();
                SourceSegment {
                    boundary: reach_boundary(span, distance),
                    reach: shape
                        .bounds()
                        .expanded(distance + 2.0 * AT_DISTANCE.value()),
                    shape,
                }
            })
            .collect();
        Self { distance, segments }
    }

    /// The segments near enough to `piece` to matter to it. A segment whose
    /// reach the piece never enters can neither cut it nor be the reason a
    /// point of it is too close — so leaving the others out changes nothing
    /// but the time taken.
    fn near(&self, piece: &PathSpan) -> Vec<&SourceSegment> {
        let bounds = piece.primitive().bounds();
        self.segments
            .iter()
            .filter(|segment| segment.reach.intersects(&bounds))
            .collect()
    }
}

/// Steps 2 and 3 for one raw piece: cut it wherever it crosses a boundary,
/// and return the stretches that are at the offset distance from the source.
fn valid_parts(piece: &PathSpan, source: &Source) -> Vec<PathSpan> {
    let near = source.near(piece);
    let distance = source.distance;
    let shape = piece.primitive();
    let mut cuts: Vec<(f64, Point2)> = near
        .iter()
        .flat_map(|segment| &segment.boundary)
        .flat_map(|boundary| shape.intersect(boundary).as_slice().to_vec())
        .filter(|cut| !LINEAR.coincident(*cut, piece.start) && !LINEAR.coincident(*cut, piece.end))
        .map(|cut| (piece.parameter_of(cut), cut))
        .collect();
    cuts.sort_by(|a, b| a.0.total_cmp(&b.0));
    cuts.dedup_by(|later, earlier| LINEAR.coincident(later.1, earlier.1));

    let stops: Vec<(f64, Point2)> = std::iter::once((0.0, piece.start))
        .chain(cuts)
        .chain([(1.0, piece.end)])
        .collect();
    let at_distance = |t: f64| {
        let point = piece.point_at(t);
        let nearest = near
            .iter()
            .map(|segment| segment.shape.distance_to_point(point))
            .fold(f64::INFINITY, f64::min);
        AT_DISTANCE.eq(nearest, distance)
    };

    // In exact arithmetic every stretch between two cuts is right throughout
    // or wrong throughout. Intersections of very flat arcs can be missed,
    // though, so each stretch is probed, and where the answer changes
    // between two probes the change is located and becomes a cut too.
    let mut refined: Vec<(f64, Point2)> = vec![stops[0]];
    for pair in stops.windows(2) {
        let (from, to) = (pair[0].0, pair[1].0);
        let probes: Vec<(f64, bool)> = (1..=PROBES)
            .map(|step| from + (to - from) * f64::from(step) / f64::from(PROBES + 1))
            .map(|t| (t, at_distance(t)))
            .collect();
        for neighbours in probes.windows(2) {
            let ((mut low, valid_low), (mut high, valid_high)) = (neighbours[0], neighbours[1]);
            if valid_low != valid_high {
                for _ in 0..BISECTIONS {
                    let middle = low + (high - low) / 2.0;
                    if at_distance(middle) == valid_low {
                        low = middle;
                    } else {
                        high = middle;
                    }
                }
                // Stop on the valid side, so the part kept is valid to its end.
                let stop = if valid_low { low } else { high };
                refined.push((stop, piece.point_at(stop)));
            }
        }
        refined.push(pair[1]);
    }

    // Runs of valid stretches are put back together into one part each.
    let mut parts: Vec<PathSpan> = Vec::new();
    let mut run_start: Option<(f64, Point2)> = None;
    for pair in refined.windows(2) {
        let valid = at_distance(pair[0].0 + (pair[1].0 - pair[0].0) / 2.0)
            && !LINEAR.coincident(pair[0].1, pair[1].1);
        match (valid, run_start) {
            (true, None) => run_start = Some(pair[0]),
            (false, Some(start)) => {
                parts.push(piece.part(start, pair[0]));
                run_start = None;
            }
            _ => {}
        }
    }
    if let (Some(start), Some(last)) = (run_start, refined.last()) {
        parts.push(piece.part(start, *last));
    }
    parts
}

/// Step 4: joins spans end to start into paths. A chain that returns to its
/// own beginning is closed.
pub fn stitch_spans(mut pieces: Vec<PathSpan>) -> Vec<Path> {
    let mut paths = Vec::new();
    while !pieces.is_empty() {
        // Begin where nothing leads in, if there is such a piece: the start
        // of an open chain. Otherwise anywhere on a loop.
        let first = (0..pieces.len())
            .find(|candidate| {
                let start = pieces[*candidate].start;
                !pieces
                    .iter()
                    .any(|other| SAME_POINT.coincident(other.end, start))
            })
            .unwrap_or(0);
        let mut chain = vec![pieces.swap_remove(first)];
        loop {
            let end = chain[chain.len() - 1].end;
            if chain.len() > 1 && SAME_POINT.coincident(end, chain[0].start) {
                break;
            }
            let Some(next) = pieces
                .iter()
                .position(|piece| SAME_POINT.coincident(piece.start, end))
            else {
                break;
            };
            chain.push(pieces.swap_remove(next));
        }
        let closed =
            SAME_POINT.coincident(chain[chain.len() - 1].end, chain[0].start) && chain.len() > 1;
        let mut vertices: Vec<Point2> = chain.iter().map(|span| span.start).collect();
        let mut bulges: Vec<f64> = chain.iter().map(|span| span.bulge).collect();
        if !closed {
            vertices.push(chain[chain.len() - 1].end);
            bulges.push(0.0);
        }
        paths.push(Path {
            vertices,
            bulges,
            closed,
        });
    }
    paths
}

/// The offset of `path`, `signed` to the left of its direction of travel
/// (negative: to the right). Several paths when the offset falls apart;
/// none when nothing is left of it.
pub fn offset_path(path: &Path, signed: f64) -> Vec<Path> {
    let distance = signed.abs();
    let spans = path_spans(path);
    if spans.is_empty() || !signed.is_finite() || LINEAR.is_zero(distance) {
        return Vec::new();
    }
    let source = Source::new(&spans, distance);
    let pieces: Vec<PathSpan> = raw_offset(path, signed)
        .iter()
        .flat_map(|piece| valid_parts(piece, &source))
        .collect();
    stitch_spans(pieces)
}

/// Why a path could not be offset.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PathOffsetError {
    /// The side point is on the path, so it chooses no side.
    NoSide,
    /// Nothing is left: the path is too small to be offset that far inwards.
    Vanishes,
}

impl Path {
    /// Which side of the path `point` is on: `1.0` for the left of its
    /// direction of travel, `-1.0` for the right, `None` on the path itself.
    fn side_of(&self, point: Point2) -> Option<f64> {
        let spans = path_spans(self);
        let nearest = spans
            .iter()
            .map(|span| (span, span.primitive().distance_to_point(point)))
            .min_by(|a, b| a.1.total_cmp(&b.1))?;
        if LINEAR.is_zero(nearest.1) {
            return None;
        }
        let span = nearest.0;
        match span.arc() {
            // Exact, like every which-side question about a line.
            None => match orientation(span.start, span.end, point) {
                Orientation::Left => Some(1.0),
                Orientation::Right => Some(-1.0),
                Orientation::Collinear => None,
            },
            // Left of a counter-clockwise arc is the inside of its circle.
            Some(arc) => {
                let inside = arc.center.distance_to(point) < arc.radius;
                Some(if inside == (span.bulge > 0.0) {
                    1.0
                } else {
                    -1.0
                })
            }
        }
    }

    /// The paths `distance` from this one, on the side `side` is on.
    pub fn offset(&self, distance: f64, side: Point2) -> Result<Vec<Path>, PathOffsetError> {
        let sign = self.side_of(side).ok_or(PathOffsetError::NoSide)?;
        let offsets = offset_path(self, sign * distance);
        if offsets.is_empty() {
            Err(PathOffsetError::Vanishes)
        } else {
            Ok(offsets)
        }
    }
}

#[cfg(test)]
mod tests {
    use std::f64::consts::{PI, SQRT_2};

    use proptest::prelude::*;

    use super::*;

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

    fn length(path: &Path) -> f64 {
        path_spans(path)
            .iter()
            .map(|span| match span.primitive() {
                Primitive::Segment(segment) => segment.length(),
                Primitive::Arc(arc) => arc.radius * arc.sweep,
                Primitive::Circle(circle) => 2.0 * PI * circle.radius,
            })
            .sum()
    }

    /// The property the method stands on: every point of the result is at
    /// the offset distance from the source. Returns the worst deviation.
    fn worst_deviation(source: &Path, offsets: &[Path], distance: f64) -> f64 {
        let shape: Vec<Primitive> = path_spans(source).iter().map(PathSpan::primitive).collect();
        offsets
            .iter()
            .flat_map(path_spans)
            .flat_map(|span| (0..=10).map(move |step| span.point_at(f64::from(step) / 10.0)))
            .map(|point| {
                let nearest = shape
                    .iter()
                    .map(|primitive| primitive.distance_to_point(point))
                    .fold(f64::INFINITY, f64::min);
                (nearest - distance).abs()
            })
            .fold(0.0, f64::max)
    }

    /// A 100 × 60 rectangle, counter-clockwise: its left is its inside.
    fn rectangle() -> Path {
        straight(
            &[(0.0, 0.0), (100.0, 0.0), (100.0, 60.0), (0.0, 60.0)],
            true,
        )
    }

    #[test]
    fn a_rectangle_offset_inwards_is_a_smaller_rectangle_with_sharp_corners() {
        let offsets = offset_path(&rectangle(), 10.0);
        assert_eq!(offsets.len(), 1);
        let inner = &offsets[0];
        assert!(inner.closed);
        assert_eq!(inner.vertices.len(), 4, "{inner:?}");
        assert!(inner.bulges.iter().all(|bulge| *bulge == 0.0));
        assert!((length(inner) - 2.0 * (80.0 + 40.0)).abs() < 1e-9);
        assert!(worst_deviation(&rectangle(), &offsets, 10.0) < 1e-9);
    }

    #[test]
    fn a_rectangle_offset_outwards_gains_round_corners() {
        let offsets = offset_path(&rectangle(), -10.0);
        assert_eq!(offsets.len(), 1);
        let outer = &offsets[0];
        assert!(outer.closed);
        assert_eq!(outer.vertices.len(), 8, "four sides and four corner arcs");
        // The perimeter grows by one full turn of radius 10.
        assert!((length(outer) - (320.0 + 2.0 * PI * 10.0)).abs() < 1e-9);
        assert!(worst_deviation(&rectangle(), &offsets, 10.0) < 1e-9);
    }

    #[test]
    fn an_offset_too_far_inwards_leaves_nothing() {
        assert!(offset_path(&rectangle(), 31.0).is_empty());
        assert!(offset_path(&rectangle(), 500.0).is_empty());
        assert!(
            offset_path(&rectangle(), 0.0).is_empty(),
            "no distance, no offset"
        );
        assert!(offset_path(&rectangle(), f64::NAN).is_empty());
    }

    #[test]
    fn an_open_path_is_offset_to_one_side_with_flat_ends() {
        // Right 100, then up 100: a left turn.
        let corner = straight(&[(0.0, 0.0), (100.0, 0.0), (100.0, 100.0)], false);

        // To the left — the inside of the turn — the corner stays sharp.
        let inside = offset_path(&corner, 20.0);
        assert_eq!(inside.len(), 1);
        assert_eq!(
            inside[0],
            straight(&[(0.0, 20.0), (80.0, 20.0), (80.0, 100.0)], false)
        );
        // To the right — the outside — it is rounded.
        let outside = offset_path(&corner, -20.0);
        assert_eq!(outside.len(), 1);
        assert_eq!(outside[0].vertices.len(), 4);
        assert!((length(&outside[0]) - (200.0 + PI * 10.0)).abs() < 1e-9);
        assert!(!outside[0].closed);
        assert!(worst_deviation(&corner, &outside, 20.0) < 1e-9);
    }

    #[test]
    fn a_segment_too_short_for_the_offset_disappears() {
        // A notch 10 wide in a long run; offset 20 over it. The notch's
        // walls and floor are too far down to matter: what is left of them
        // is its two top corners, each 20 away from a short arc. The offset
        // therefore dips, by a hair, over the mouth of the notch.
        let notched = straight(
            &[
                (0.0, 0.0),
                (100.0, 0.0),
                (100.0, -50.0),
                (110.0, -50.0),
                (110.0, 0.0),
                (210.0, 0.0),
            ],
            false,
        );
        let offsets = offset_path(&notched, 20.0);
        assert_eq!(offsets.len(), 1, "{offsets:?}");
        let over = &offsets[0];
        assert_eq!(over.vertices.len(), 5, "{over:?}");
        assert_eq!(
            (over.vertices[0], over.vertices[4]),
            (p(0.0, 20.0), p(210.0, 20.0))
        );
        // The lowest point is midway, 20 from both corners: y = √(400 − 25).
        assert!(over.vertices[2].distance_to(p(105.0, 375f64.sqrt())) < 1e-9);
        let arc = 20.0 * (5.0f64 / 20.0).asin();
        assert!((length(over) - (200.0 + 2.0 * arc)).abs() < 1e-9);
        assert!(worst_deviation(&notched, &offsets, 20.0) < 1e-9);

        // To the other side the notch survives, with its corners rounded.
        let below = offset_path(&notched, -20.0);
        assert_eq!(below.len(), 1);
        assert!(worst_deviation(&notched, &below, 20.0) < 1e-9);
    }

    #[test]
    fn a_shape_with_a_narrow_waist_falls_apart_into_two() {
        // Two 60 × 60 rooms joined by a corridor 10 wide, offset 10 inwards:
        // the corridor is too narrow to survive, and each room is left with
        // a 40 × 40 square — dimpled where the corridor's mouth was, because
        // there the nearest walls are the mouth's two corners.
        let dumbbell = straight(
            &[
                (0.0, 0.0),
                (60.0, 0.0),
                (60.0, 25.0),
                (120.0, 25.0),
                (120.0, 0.0),
                (180.0, 0.0),
                (180.0, 60.0),
                (120.0, 60.0),
                (120.0, 35.0),
                (60.0, 35.0),
                (60.0, 60.0),
                (0.0, 60.0),
            ],
            true,
        );
        let offsets = offset_path(&dumbbell, 10.0);
        assert_eq!(offsets.len(), 2, "{offsets:?}");
        // The mouth, 10 wide, is replaced by two arcs of radius 10 over 30° each.
        let dimpled = 4.0 * 40.0 - 10.0 + 2.0 * 10.0 * (PI / 6.0);
        for room in &offsets {
            assert!(room.closed);
            assert!((length(room) - dimpled).abs() < 1e-9, "{}", length(room));
        }
        assert!(worst_deviation(&dumbbell, &offsets, 10.0) < 1e-9);
    }

    /// A slot 100 long and 40 high with semicircular ends, counter-clockwise.
    fn slot() -> Path {
        Path {
            vertices: vec![p(0.0, 0.0), p(100.0, 0.0), p(100.0, 40.0), p(0.0, 40.0)],
            bulges: vec![0.0, 1.0, 0.0, 1.0],
            closed: true,
        }
    }

    #[test]
    fn arcs_offset_to_concentric_arcs() {
        let inner = offset_path(&slot(), 5.0);
        assert_eq!(inner.len(), 1);
        assert_eq!(
            inner[0].vertices,
            vec![p(0.0, 5.0), p(100.0, 5.0), p(100.0, 35.0), p(0.0, 35.0)]
        );
        assert_eq!(inner[0].bulges, vec![0.0, 1.0, 0.0, 1.0]);
        assert!((length(&inner[0]) - (200.0 + 2.0 * PI * 15.0)).abs() < 1e-9);

        let outer = offset_path(&slot(), -5.0);
        assert_eq!(outer.len(), 1);
        assert!((length(&outer[0]) - (200.0 + 2.0 * PI * 25.0)).abs() < 1e-9);
        assert!(worst_deviation(&slot(), &outer, 5.0) < 1e-9);
    }

    #[test]
    fn an_arc_smaller_than_the_offset_collapses_to_a_sharp_corner() {
        // A square with one corner filleted at radius 5, offset 10 inwards:
        // the fillet is swallowed and the corner comes back sharp.
        let quarter = SQRT_2 - 1.0;
        let filleted = Path {
            vertices: vec![
                p(0.0, 0.0),
                p(95.0, 0.0),
                p(100.0, 5.0),
                p(100.0, 100.0),
                p(0.0, 100.0),
            ],
            bulges: vec![0.0, quarter, 0.0, 0.0, 0.0],
            closed: true,
        };
        let offsets = offset_path(&filleted, 10.0);
        assert_eq!(offsets.len(), 1);
        assert_eq!(offsets[0].vertices.len(), 4, "{:?}", offsets[0]);
        assert!((length(&offsets[0]) - 4.0 * 80.0).abs() < 1e-9);
        assert!(worst_deviation(&filleted, &offsets, 10.0) < 1e-9);
    }

    #[test]
    fn the_side_point_chooses_the_side() {
        let inner = rectangle().offset(10.0, p(50.0, 30.0)).unwrap();
        assert!((length(&inner[0]) - 240.0).abs() < 1e-9);
        let outer = rectangle().offset(10.0, p(50.0, -30.0)).unwrap();
        assert!(length(&outer[0]) > 320.0);
        assert_eq!(
            rectangle().offset(10.0, p(50.0, 0.0)),
            Err(PathOffsetError::NoSide)
        );
        assert_eq!(
            rectangle().offset(40.0, p(50.0, 30.0)),
            Err(PathOffsetError::Vanishes)
        );
        // By an arc segment, inside its circle is the left of a counter-clockwise arc.
        let inner = slot().offset(5.0, p(110.0, 20.0)).unwrap();
        assert!(length(&inner[0]) < length(&slot()));
    }

    /// Regression (found by the property test below): an arc so flat that
    /// its radius is 100,000 — a crossing of its offset was not found by
    /// intersection, and a stretch that ran 22 units off was kept. Probing
    /// each stretch now catches what intersection misses.
    #[test]
    fn a_missed_crossing_on_a_very_flat_arc_is_caught_by_probing() {
        let path = Path {
            vertices: vec![
                p(0.0, -40.79313164211775),
                p(-192.84856245719732, -94.41844070332529),
                p(0.0, -186.9870865413599),
                p(0.0, 133.65375689380312),
                p(121.49725133306622, 0.0),
                p(-174.9811812741329, 0.0),
            ],
            bulges: vec![
                0.0,
                0.0,
                -0.000746102365195887,
                0.0,
                0.9265266681888825,
                0.0,
            ],
            closed: false,
        };
        let distance = 26.061059034600945;
        let offsets = offset_path(&path, distance);
        assert!(!offsets.is_empty());
        let worst = worst_deviation(&path, &offsets, distance);
        assert!(worst < 1e-6, "off by {worst}");
    }

    fn coordinate() -> impl Strategy<Value = f64> {
        -200.0..200f64
    }

    /// A path of three to seven well-separated vertices, straight and arc
    /// segments mixed, open or closed. It may cross itself.
    fn any_path() -> impl Strategy<Value = Path> {
        (3..8usize, any::<bool>()).prop_flat_map(|(count, closed)| {
            (
                proptest::collection::vec((coordinate(), coordinate()), count),
                proptest::collection::vec(prop_oneof![3 => Just(0.0), 1 => -1.5..1.5f64], count),
            )
                .prop_filter("vertices apart", |(vertices, _)| {
                    vertices.iter().enumerate().all(|(index, a)| {
                        vertices[index + 1..]
                            .iter()
                            .all(|b| (a.0 - b.0).hypot(a.1 - b.1) > 5.0)
                    })
                })
                .prop_map(move |(vertices, bulges)| Path {
                    vertices: vertices.into_iter().map(|(x, y)| p(x, y)).collect(),
                    bulges,
                    closed,
                })
        })
    }

    proptest! {
        /// #122: every point of the result is the offset distance from the
        /// source, within tolerance — for any path, to either side.
        #[test]
        fn every_point_of_an_offset_is_at_the_offset_distance(
            path in any_path(), distance in 1.0..60f64, left in any::<bool>(),
        ) {
            let signed = if left { distance } else { -distance };
            let offsets = offset_path(&path, signed);
            let worst = worst_deviation(&path, &offsets, distance);
            prop_assert!(worst < 1e-5, "off by {} in {:?}", worst, offsets);
            for offset in &offsets {
                prop_assert!(offset.vertices.len() >= 2);
                prop_assert!(offset.vertices.iter().all(|vertex| vertex.is_finite()));
                prop_assert!(offset.bulges.iter().all(|bulge| bulge.is_finite()));
            }
        }

        /// A convex polygon offset outwards is one closed path, longer by a
        /// full turn of the offset radius: nothing valid is thrown away.
        #[test]
        fn a_convex_polygon_offset_outwards_grows_by_one_full_turn(
            sides in 3..9usize, radius in 20.0..200f64, distance in 1.0..80f64, turn in 0.0..360f64,
        ) {
            let vertices: Vec<(f64, f64)> = (0..sides)
                .map(|index| {
                    let angle = (turn + 360.0 * index as f64 / sides as f64).to_radians();
                    (radius * angle.cos(), radius * angle.sin())
                })
                .collect();
            let polygon = straight(&vertices, true);
            let offsets = offset_path(&polygon, -distance);
            prop_assert_eq!(offsets.len(), 1);
            prop_assert!(offsets[0].closed);
            let expected = length(&polygon) + 2.0 * PI * distance;
            prop_assert!((length(&offsets[0]) - expected).abs() < 1e-6 * expected);
        }
    }
}

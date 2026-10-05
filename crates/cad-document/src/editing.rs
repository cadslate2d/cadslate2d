//! What the modify commands can do to each kind of entity (DESIGN §22).
//!
//! M2 supports deliberately narrow cases; each operation here is a pure
//! function that either produces the result or says, as a value, why not.
//! The supported combinations are the published matrices in
//! `docs/modify-matrix.md`; a combination not listed there is an
//! `Unsupported` error, never a guess.

use cad_core::tolerance::LinearTolerance;
use cad_core::{Aabb, Point2, Vec2};
use cad_geometry::{
    Corner, CornerJoin, Path, PathCornerError, PathOffsetError, PathTrimError, Primitive,
    break_arc, break_circle, break_segment, distance_to_line, extend_arc, extend_segment,
    fillet_curves, offset_arc, offset_circle, offset_segment, trim_arc_parts, trim_circle_parts,
    trim_segment_parts,
};

use crate::{ArcEntity, CircleEntity, Entity, LineEntity, PolylineEntity};

const TOLERANCE: LinearTolerance = LinearTolerance::DEFAULT;

/// Whether `pick` is nearer the last vertex of an open polyline than the first.
fn polyline_end_is_nearer(polyline: &PolylineEntity, pick: Point2) -> bool {
    match (polyline.vertices.first(), polyline.vertices.last()) {
        (Some(first), Some(last)) => pick.distance_to(*last) <= pick.distance_to(*first),
        _ => true,
    }
}

/// Why an entity could not be offset.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OffsetError {
    /// The side point is on the entity, so it chooses no side.
    NoSide,
    /// Offsetting inwards by this much would leave nothing.
    Collapses,
}

/// Why two entities could not be joined at a corner.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CornerError {
    /// Only lines can be joined so far; carries the name of what was picked.
    Unsupported(&'static str),
    /// The lines are parallel.
    Parallel,
    /// A line is too short for the radius or distances asked for.
    TooLarge,
    /// No arc of that radius is tangent to both objects — or, with a radius
    /// of zero, they never meet.
    NoSolution,
    /// The two picks on a polyline are not on segments that meet at a corner.
    NotAdjacent,
}

impl From<cad_geometry::CornerError> for CornerError {
    fn from(error: cad_geometry::CornerError) -> Self {
        match error {
            cad_geometry::CornerError::Parallel => Self::Parallel,
            cad_geometry::CornerError::TooLarge => Self::TooLarge,
            cad_geometry::CornerError::NoSolution => Self::NoSolution,
        }
    }
}

/// Two entities joined at a corner: what each becomes, and the piece that
/// joins them — a fillet arc or a chamfer line — if there is one.
#[derive(Clone, Debug, PartialEq)]
pub struct Cornered {
    pub first: Entity,
    pub second: Entity,
    pub joint: Option<Entity>,
}

/// Why an entity could not be trimmed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrimError {
    /// This kind of entity cannot be trimmed yet; carries its name.
    Unsupported(&'static str),
    /// The cutting edge does not divide the entity: it misses it, or only
    /// touches one of its ends.
    NoIntersection,
    /// A closed shape has no ends, so it must be crossed at two distinct
    /// points to lose anything; carries what it is: `circle`, `closed polyline`.
    NeedsTwoCrossings(&'static str),
}

/// Why an entity could not be broken.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BreakError {
    /// A single break point at an end of the entity splits nothing off.
    AtEndpoint,
    /// The two points span the whole entity: nothing would be left.
    NothingLeft,
    /// A closed shape has no ends, so one point cannot break it; carries
    /// what it is: `circle`, `closed polyline`.
    NeedsTwoPoints(&'static str),
}

/// How much of an entity a stretch window takes hold of: how many of the
/// points that define it are inside.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StretchHold {
    /// None of them: the entity stays as it is.
    Nothing,
    /// Some: those move and the rest stay, so the entity changes shape.
    Part,
    /// All: the entity moves as it is.
    Whole,
}

/// An entity after trimming: what replaces it, and the part that was cut away.
#[derive(Clone, Debug, PartialEq)]
pub struct TrimParts {
    pub kept: Vec<Entity>,
    pub removed: Entity,
}

/// Why an entity could not be extended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExtendError {
    /// This kind of entity cannot be extended yet; carries its name.
    Unsupported(&'static str),
    /// Carried on from the picked end, the entity never meets the boundary.
    NoMeeting,
}

impl Entity {
    /// How much of this entity a stretch window takes hold of. The points
    /// that count are a line's two ends, an arc's two ends, a polyline's
    /// vertices, and a circle's centre — so a circle moves whole or not at
    /// all.
    pub fn stretch_hold(&self, window: &Aabb) -> StretchHold {
        let held = |points: &[Point2]| {
            let inside = points
                .iter()
                .filter(|point| window.contains_point(**point))
                .count();
            match inside {
                0 => StretchHold::Nothing,
                all if all == points.len() => StretchHold::Whole,
                _ => StretchHold::Part,
            }
        };
        match self {
            Entity::Line(line) => held(&[line.start, line.end]),
            Entity::Circle(circle) => held(&[circle.center]),
            Entity::Arc(entity) => {
                let arc = entity.arc();
                held(&[arc.start_point(), arc.end_point()])
            }
            Entity::Polyline(polyline) => held(&polyline.vertices),
        }
    }

    /// This entity with the points `window` takes hold of moved by `offset`
    /// and the rest left where they are (see [`Entity::stretch_hold`]).
    ///
    /// An arc held by one end follows the grip rule (ADR-0010): it is
    /// re-fitted through the moved end, its old midpoint, and its other end.
    /// A polyline's arc segments keep their bulge between their new ends.
    ///
    /// `None` when the result would not be a valid entity: a line of no
    /// length, an arc through three points in a line, a polyline vertex on
    /// top of its neighbour.
    pub fn stretched(&self, window: &Aabb, offset: Vec2) -> Option<Entity> {
        match self.stretch_hold(window) {
            StretchHold::Nothing => return Some(self.clone()),
            StretchHold::Whole => return Some(self.translated(offset)),
            StretchHold::Part => {}
        }
        let moved = |point: Point2| {
            if window.contains_point(point) {
                point + offset
            } else {
                point
            }
        };
        match self {
            Entity::Line(line) => {
                let (start, end) = (moved(line.start), moved(line.end));
                (!TOLERANCE.coincident(start, end)).then(|| LineEntity::new(start, end).into())
            }
            // A circle is held whole or not at all.
            Entity::Circle(_) => Some(self.clone()),
            Entity::Arc(entity) => {
                let arc = entity.arc();
                let (start, end) = (arc.start_point(), arc.end_point());
                // The grips of an arc: 0 is its start, 2 its end.
                if window.contains_point(start) {
                    self.grip_moved(0, start + offset)
                } else {
                    self.grip_moved(2, end + offset)
                }
            }
            Entity::Polyline(polyline) => {
                let vertices: Vec<Point2> = polyline.vertices.iter().copied().map(moved).collect();
                let count = vertices.len();
                let collapsed = (0..count)
                    .filter(|index| polyline.closed || index + 1 < count)
                    .any(|index| {
                        TOLERANCE.coincident(vertices[index], vertices[(index + 1) % count])
                    });
                (!collapsed).then(|| {
                    PolylineEntity::with_bulges(
                        vertices,
                        polyline.bulges().collect(),
                        polyline.closed,
                    )
                    .into()
                })
            }
        }
    }

    /// What is left of this entity broken between two points: the part
    /// between them goes, or — with the two points the same — the entity is
    /// split there. The points need not be on the entity; where each lands
    /// on it is what counts.
    ///
    /// A circle, and a closed polyline, lose the part from the first point
    /// onwards to the second — counter-clockwise for a circle, in its own
    /// direction for a polyline — and so need two different points.
    pub fn broken(&self, first: Point2, second: Point2) -> Result<Vec<Entity>, BreakError> {
        // Tell the two ways an open entity can fail apart.
        let open_failure = |start: Point2, end: Point2, a: Point2, b: Point2| {
            let at_end = |point: Point2| {
                TOLERANCE.coincident(point, start) || TOLERANCE.coincident(point, end)
            };
            if TOLERANCE.coincident(a, b) && at_end(a) {
                BreakError::AtEndpoint
            } else {
                BreakError::NothingLeft
            }
        };
        match self {
            Entity::Line(line) => {
                let segment = line.segment();
                break_segment(&segment, first, second, TOLERANCE)
                    .map(|pieces| {
                        pieces
                            .into_iter()
                            .map(|piece| LineEntity::new(piece.start, piece.end).into())
                            .collect()
                    })
                    .ok_or_else(|| {
                        let (a, b) = (segment.closest_point(first), segment.closest_point(second));
                        open_failure(segment.start, segment.end, a, b)
                    })
            }
            Entity::Arc(entity) => {
                let arc = entity.arc();
                break_arc(&arc, first, second, TOLERANCE)
                    .map(|pieces| {
                        pieces
                            .into_iter()
                            .map(|piece| ArcEntity::from(piece).into())
                            .collect()
                    })
                    .ok_or_else(|| {
                        let (a, b) = (arc.closest_point(first), arc.closest_point(second));
                        open_failure(arc.start_point(), arc.end_point(), a, b)
                    })
            }
            Entity::Circle(circle) => break_circle(&circle.circle(), first, second, TOLERANCE)
                .map(|arc| vec![ArcEntity::from(arc).into()])
                .ok_or(BreakError::NeedsTwoPoints("circle")),
            Entity::Polyline(polyline) => {
                let path = polyline.path();
                path.broken(first, second, TOLERANCE)
                    .map(|pieces| {
                        pieces
                            .into_iter()
                            .map(|piece| PolylineEntity::from(piece).into())
                            .collect()
                    })
                    .ok_or_else(|| {
                        if polyline.closed {
                            return BreakError::NeedsTwoPoints("closed polyline");
                        }
                        let nearest = |point: Point2| {
                            self.primitives()
                                .iter()
                                .map(|primitive| primitive.closest_point(point))
                                .min_by(|x, y| {
                                    x.distance_to(point).total_cmp(&y.distance_to(point))
                                })
                                .unwrap_or(point)
                        };
                        let ends = (polyline.vertices.first(), polyline.vertices.last());
                        match ends {
                            (Some(start), Some(end)) => {
                                open_failure(*start, *end, nearest(first), nearest(second))
                            }
                            _ => BreakError::NothingLeft,
                        }
                    })
            }
        }
    }

    /// What is left of this entity when the part at `pick` is cut away at
    /// `cutting`: one piece, or two when the pick was between two crossings.
    /// A trimmed circle becomes an arc.
    pub fn trimmed(&self, cutting: &Entity, pick: Point2) -> Result<Vec<Entity>, TrimError> {
        self.trim_parts(cutting, pick).map(|parts| parts.kept)
    }

    /// [`Entity::trimmed`], together with the part that is cut away — which
    /// is what a preview of the trim shows.
    pub fn trim_parts(&self, cutting: &Entity, pick: Point2) -> Result<TrimParts, TrimError> {
        self.trim_parts_at(&self.intersections(cutting), pick)
    }

    /// [`Entity::trim_parts`] for cuts found beforehand — the crossings of
    /// any number of cutting edges. The part at `pick` between the nearest
    /// cut on either side goes.
    pub fn trim_parts_at(&self, cuts: &[Point2], pick: Point2) -> Result<TrimParts, TrimError> {
        let line =
            |piece: cad_geometry::Segment| Entity::from(LineEntity::new(piece.start, piece.end));
        let arc = |piece: cad_geometry::Arc| Entity::from(ArcEntity::from(piece));
        match self {
            Entity::Line(entity) => trim_segment_parts(&entity.segment(), cuts, pick, TOLERANCE)
                .map(|parts| TrimParts {
                    kept: parts.kept.into_iter().map(line).collect(),
                    removed: line(parts.removed),
                })
                .ok_or(TrimError::NoIntersection),
            Entity::Arc(entity) => trim_arc_parts(&entity.arc(), cuts, pick, TOLERANCE)
                .map(|parts| TrimParts {
                    kept: parts.kept.into_iter().map(arc).collect(),
                    removed: arc(parts.removed),
                })
                .ok_or(TrimError::NoIntersection),
            Entity::Circle(entity) => {
                match trim_circle_parts(&entity.circle(), cuts, pick, TOLERANCE) {
                    Some(parts) => Ok(TrimParts {
                        kept: parts.kept.into_iter().map(arc).collect(),
                        removed: arc(parts.removed),
                    }),
                    None if cuts.is_empty() => Err(TrimError::NoIntersection),
                    None => Err(TrimError::NeedsTwoCrossings("circle")),
                }
            }
            Entity::Polyline(entity) => {
                let polyline = |path: Path| Entity::from(PolylineEntity::from(path));
                match entity.path().trimmed(cuts, pick, TOLERANCE) {
                    Ok(parts) => Ok(TrimParts {
                        kept: parts.kept.into_iter().map(polyline).collect(),
                        removed: polyline(parts.removed),
                    }),
                    Err(PathTrimError::NoCut) => Err(TrimError::NoIntersection),
                    Err(PathTrimError::NeedsTwoCuts) => {
                        Err(TrimError::NeedsTwoCrossings("closed polyline"))
                    }
                }
            }
        }
    }

    /// This entity lengthened from the end nearer to `pick` until it meets
    /// `boundary`: a line straight on, an arc along its circle.
    pub fn extended(&self, boundary: &Entity, pick: Point2) -> Result<Entity, ExtendError> {
        self.extended_to(&boundary.primitives(), pick)
    }

    /// [`Entity::extended`] against the geometry of any number of boundary
    /// edges: the first of them met is where the extension stops.
    pub fn extended_to(&self, boundary: &[Primitive], pick: Point2) -> Result<Entity, ExtendError> {
        match self {
            Entity::Line(line) => {
                let from_end = pick.distance_to(line.end) <= pick.distance_to(line.start);
                extend_segment(&line.segment(), from_end, boundary, TOLERANCE)
                    .map(|longer| LineEntity::new(longer.start, longer.end).into())
                    .ok_or(ExtendError::NoMeeting)
            }
            Entity::Arc(arc) => {
                let arc = arc.arc();
                let from_end =
                    pick.distance_to(arc.end_point()) <= pick.distance_to(arc.start_point());
                extend_arc(&arc, from_end, boundary, TOLERANCE)
                    .map(|longer| ArcEntity::from(longer).into())
                    .ok_or(ExtendError::NoMeeting)
            }
            // A closed polyline has no ends, like a circle.
            Entity::Polyline(polyline) if polyline.closed => {
                Err(ExtendError::Unsupported("closed polyline"))
            }
            Entity::Polyline(polyline) => {
                let from_end = polyline_end_is_nearer(polyline, pick);
                polyline
                    .path()
                    .extended(from_end, boundary, TOLERANCE)
                    .map(|longer| PolylineEntity::from(longer).into())
                    .ok_or(ExtendError::NoMeeting)
            }
            Entity::Circle(_) => Err(ExtendError::Unsupported(self.kind_name())),
        }
    }

    /// A box that anything this entity could be extended to, from the end
    /// nearer to `pick`, must touch — so boundary candidates can come from
    /// the spatial index instead of the whole drawing. `within` bounds how
    /// far to look: the extents of the drawing. `None` for entities that
    /// cannot be extended.
    pub fn extension_reach(&self, pick: Point2, within: &Aabb) -> Option<Aabb> {
        match self {
            Entity::Line(line) => {
                let from_end = pick.distance_to(line.end) <= pick.distance_to(line.start);
                let (tip, tail) = if from_end {
                    (line.end, line.start)
                } else {
                    (line.start, line.end)
                };
                let onwards = tip - tail;
                let length = onwards.length();
                if TOLERANCE.is_zero(length) {
                    return None;
                }
                // Far enough to leave `within` from wherever the tip is.
                let reach = within
                    .corners()
                    .iter()
                    .map(|corner| tip.distance_to(*corner))
                    .fold(0.0, f64::max);
                Some(Aabb::from_points(tip, tip + onwards * (reach / length)))
            }
            Entity::Arc(arc) => Some(arc.arc().circle().bounds()),
            // An open polyline reaches as its first or last segment would alone.
            Entity::Polyline(polyline) if !polyline.closed => {
                let segments: Vec<Primitive> = polyline.primitives().collect();
                let end_segment = if polyline_end_is_nearer(polyline, pick) {
                    segments.last()
                } else {
                    segments.first()
                };
                let alone: Entity = match end_segment? {
                    Primitive::Segment(segment) => {
                        LineEntity::new(segment.start, segment.end).into()
                    }
                    Primitive::Arc(arc) => ArcEntity::from(*arc).into(),
                    Primitive::Circle(_) => return None,
                };
                alone.extension_reach(pick, within)
            }
            Entity::Circle(_) | Entity::Polyline(_) => None,
        }
    }

    /// The corner this line makes with `other`, each picked on the side to
    /// keep. Only two lines have one so far.
    fn corner_with(
        &self,
        pick: Point2,
        other: &Entity,
        other_pick: Point2,
    ) -> Result<Corner, CornerError> {
        match (self, other) {
            (Entity::Line(first), Entity::Line(second)) => Ok(Corner::new(
                &first.segment(),
                pick,
                &second.segment(),
                other_pick,
                TOLERANCE,
            )?),
            (Entity::Line(_), not_a_line) | (not_a_line, _) => {
                Err(CornerError::Unsupported(not_a_line.kind_name()))
            }
        }
    }

    /// This entity and `other` joined by an arc of `radius` tangent to both,
    /// each cut back or carried on to meet it, keeping the side it was
    /// picked on. A zero radius brings them to a sharp corner with no arc.
    ///
    /// Lines, arcs, and circles can be joined in any combination; a circle
    /// is left whole. Polylines cannot yet.
    pub fn filleted(
        &self,
        pick: Point2,
        other: &Entity,
        other_pick: Point2,
        radius: f64,
    ) -> Result<Cornered, CornerError> {
        // Two lines: the corner construction, which also knows when a line
        // is too short for the radius.
        if let (Entity::Line(_), Entity::Line(_)) = (self, other) {
            let fillet = self
                .corner_with(pick, other, other_pick)?
                .fillet(radius, TOLERANCE)?;
            let [first, second] = fillet.lines;
            return Ok(Cornered {
                first: LineEntity::new(first.start, first.end).into(),
                second: LineEntity::new(second.start, second.end).into(),
                joint: fillet.arc.map(|arc| ArcEntity::from(arc).into()),
            });
        }
        let curve = |entity: &Entity| match entity {
            Entity::Line(line) => Ok(Primitive::Segment(line.segment())),
            Entity::Circle(circle) => Ok(Primitive::Circle(circle.circle())),
            Entity::Arc(arc) => Ok(Primitive::Arc(arc.arc())),
            Entity::Polyline(_) => Err(CornerError::Unsupported(entity.kind_name())),
        };
        let rebuilt = |original: &Entity, primitive: Primitive| -> Entity {
            match primitive {
                Primitive::Segment(segment) => LineEntity::new(segment.start, segment.end).into(),
                Primitive::Arc(arc) => ArcEntity::from(arc).into(),
                // A circle is left exactly as it was, not rebuilt from its parts.
                Primitive::Circle(_) => original.clone(),
            }
        };
        let fillet = fillet_curves(
            &curve(self)?,
            pick,
            &curve(other)?,
            other_pick,
            radius,
            TOLERANCE,
        )?;
        Ok(Cornered {
            first: rebuilt(self, fillet.first),
            second: rebuilt(other, fillet.second),
            joint: fillet.arc.map(|arc| ArcEntity::from(arc).into()),
        })
    }

    /// This polyline with the corner between the two picked segments rounded
    /// or bevelled: the arc or bevel becomes part of the polyline. For a
    /// chamfer, the first distance belongs to the segment picked first.
    pub fn with_corner_between(
        &self,
        pick: Point2,
        other_pick: Point2,
        join: CornerJoin,
    ) -> Result<Entity, CornerError> {
        let Entity::Polyline(polyline) = self else {
            return Err(CornerError::Unsupported(self.kind_name()));
        };
        let path = polyline.path();
        let (vertex, first_arrives) = path
            .corner_between(pick, other_pick, TOLERANCE)
            .ok_or(CornerError::NotAdjacent)?;
        // The path's order of the two segments may be the reverse of the picks'.
        let join = match join {
            CornerJoin::Chamfer([first, second]) if !first_arrives => {
                CornerJoin::Chamfer([second, first])
            }
            other => other,
        };
        path.with_corner(vertex, join, TOLERANCE)
            .map(|path| PolylineEntity::from(path).into())
            .map_err(|error| match error {
                PathCornerError::NotACorner => CornerError::NotAdjacent,
                PathCornerError::Curved => CornerError::Unsupported("arc segment"),
                PathCornerError::Straight => CornerError::Parallel,
                PathCornerError::TooLarge => CornerError::TooLarge,
            })
    }

    /// This polyline with every corner rounded or bevelled that can be, and
    /// how many were done and how many skipped.
    pub fn with_every_corner(
        &self,
        join: CornerJoin,
    ) -> Result<(Entity, usize, usize), CornerError> {
        let Entity::Polyline(polyline) = self else {
            return Err(CornerError::Unsupported(self.kind_name()));
        };
        let (path, done, skipped) = polyline.path().with_every_corner(join, TOLERANCE);
        Ok((PolylineEntity::from(path).into(), done, skipped))
    }

    /// This line and `other` cut back by `distances` from where they meet —
    /// the first distance along this line — and joined by a straight line.
    pub fn chamfered(
        &self,
        pick: Point2,
        other: &Entity,
        other_pick: Point2,
        distances: [f64; 2],
    ) -> Result<Cornered, CornerError> {
        let chamfer = self
            .corner_with(pick, other, other_pick)?
            .chamfer(distances, TOLERANCE)?;
        let [first, second] = chamfer.lines;
        Ok(Cornered {
            first: LineEntity::new(first.start, first.end).into(),
            second: LineEntity::new(second.start, second.end).into(),
            joint: chamfer
                .bevel
                .map(|bevel| LineEntity::new(bevel.start, bevel.end).into()),
        })
    }

    /// The kind of entity, as a lower-case noun for messages.
    pub fn kind_name(&self) -> &'static str {
        match self {
            Entity::Line(_) => "line",
            Entity::Circle(_) => "circle",
            Entity::Arc(_) => "arc",
            Entity::Polyline(_) => "polyline",
        }
    }

    /// What lies `distance` from the entity on the side `side` is on: a
    /// parallel line, a concentric circle or arc — or, for a polyline, its
    /// offset, which may be several polylines where the shape falls apart
    /// (ADR-0011).
    pub fn offset(&self, distance: f64, side: Point2) -> Result<Vec<Entity>, OffsetError> {
        // On a circle or an arc, a side point on the curve's circle chooses no side.
        let on_circle =
            |center: Point2, radius: f64| TOLERANCE.eq(center.distance_to(side), radius);
        let one = |entity: Option<Entity>, otherwise: OffsetError| {
            entity.map(|entity| vec![entity]).ok_or(otherwise)
        };
        match self {
            Entity::Line(line) => one(
                offset_segment(&line.segment(), distance, side)
                    .map(|parallel| LineEntity::new(parallel.start, parallel.end).into()),
                OffsetError::NoSide,
            ),
            Entity::Circle(circle) if on_circle(circle.center, circle.radius) => {
                Err(OffsetError::NoSide)
            }
            Entity::Circle(circle) => one(
                offset_circle(&circle.circle(), distance, side, TOLERANCE)
                    .map(|offset| CircleEntity::new(offset.center, offset.radius).into()),
                OffsetError::Collapses,
            ),
            Entity::Arc(arc) if on_circle(arc.center, arc.radius) => Err(OffsetError::NoSide),
            Entity::Arc(arc) => one(
                offset_arc(&arc.arc(), distance, side, TOLERANCE)
                    .map(|offset| ArcEntity::from(offset).into()),
                OffsetError::Collapses,
            ),
            Entity::Polyline(polyline) => polyline
                .path()
                .offset(distance, side)
                .map(|paths| {
                    paths
                        .into_iter()
                        .map(|path| PolylineEntity::from(path).into())
                        .collect()
                })
                .map_err(|error| match error {
                    PathOffsetError::NoSide => OffsetError::NoSide,
                    PathOffsetError::Vanishes => OffsetError::Collapses,
                }),
        }
    }

    /// The offset of the entity that passes through `point`: the distance is
    /// however far the point is from the entity — for a line, from the line
    /// carried on past its ends.
    pub fn offset_through(&self, point: Point2) -> Result<Vec<Entity>, OffsetError> {
        let distance = match self {
            Entity::Line(line) => {
                distance_to_line(&line.segment(), point).ok_or(OffsetError::NoSide)?
            }
            Entity::Circle(circle) => (circle.center.distance_to(point) - circle.radius).abs(),
            Entity::Arc(arc) => (arc.center.distance_to(point) - arc.radius).abs(),
            Entity::Polyline(_) => self.distance_to(point),
        };
        self.offset(distance, point)
    }
}

#[cfg(test)]
mod tests {
    use crate::{ArcEntity, PolylineEntity};

    use super::*;

    fn p(x: f64, y: f64) -> Point2 {
        Point2::new(x, y)
    }

    #[test]
    fn a_line_offsets_to_either_side() {
        let line: Entity = LineEntity::new(p(0.0, 0.0), p(10.0, 0.0)).into();
        assert_eq!(
            line.offset(2.0, p(3.0, 9.0)),
            Ok(vec![LineEntity::new(p(0.0, 2.0), p(10.0, 2.0)).into()])
        );
        assert_eq!(
            line.offset(2.0, p(3.0, -9.0)),
            Ok(vec![LineEntity::new(p(0.0, -2.0), p(10.0, -2.0)).into()])
        );
        assert_eq!(line.offset(2.0, p(3.0, 0.0)), Err(OffsetError::NoSide));
    }

    #[test]
    fn a_circle_offsets_in_either_direction_but_not_to_nothing() {
        let circle: Entity = CircleEntity::new(p(0.0, 0.0), 10.0).into();
        assert_eq!(
            circle.offset(4.0, p(50.0, 0.0)),
            Ok(vec![CircleEntity::new(p(0.0, 0.0), 14.0).into()])
        );
        assert_eq!(
            circle.offset(4.0, p(1.0, 0.0)),
            Ok(vec![CircleEntity::new(p(0.0, 0.0), 6.0).into()])
        );
        assert_eq!(
            circle.offset(10.0, p(1.0, 0.0)),
            Err(OffsetError::Collapses)
        );
        assert_eq!(
            circle.offset(25.0, p(1.0, 0.0)),
            Err(OffsetError::Collapses)
        );
        assert_eq!(circle.offset(4.0, p(10.0, 0.0)), Err(OffsetError::NoSide));
    }

    // ── TRIM (#97): every supported combination of cutting edge and target ──

    use std::f64::consts::{FRAC_PI_2, PI};

    /// Cutting edges of every kind, all of which include the vertical
    /// segment x = 40 from y = -60 to y = 60 or cross where it would.
    fn cutting_edges() -> Vec<(&'static str, Entity)> {
        vec![
            (
                "line",
                LineEntity::new(p(40.0, -60.0), p(40.0, 60.0)).into(),
            ),
            // A circle about (90, 0) of radius 50 crosses y = 0 at x = 40 and x = 140.
            ("circle", CircleEntity::new(p(90.0, 0.0), 50.0).into()),
            // The left half of that circle.
            (
                "arc",
                ArcEntity::new(p(90.0, 0.0), 50.0, FRAC_PI_2, 3.0 * FRAC_PI_2).into(),
            ),
            (
                "polyline",
                PolylineEntity::new(vec![p(40.0, -60.0), p(40.0, 60.0), p(200.0, 60.0)], false)
                    .into(),
            ),
        ]
    }

    fn close(a: Point2, b: Point2) -> bool {
        a.distance_to(b) < 1e-9
    }

    #[test]
    fn a_line_is_trimmed_by_every_kind_of_cutting_edge() {
        let target: Entity = LineEntity::new(p(0.0, 0.0), p(100.0, 0.0)).into();
        for (name, edge) in cutting_edges() {
            // Pick left of x = 40: the part from 40 on stays.
            let pieces = target.trimmed(&edge, p(10.0, 0.0)).unwrap_or_else(|error| {
                panic!("line trimmed by {name}: {error:?}");
            });
            let Entity::Line(kept) = &pieces[0] else {
                panic!("a trimmed line is a line");
            };
            assert_eq!(pieces.len(), 1, "cut by {name}");
            assert!(
                close(kept.start, p(40.0, 0.0)) && close(kept.end, p(100.0, 0.0)),
                "cut by {name}: {kept:?}"
            );

            // Pick right of it: the part up to 40 stays.
            let pieces = target.trimmed(&edge, p(70.0, 0.0)).unwrap();
            let Entity::Line(kept) = &pieces[0] else {
                panic!("a trimmed line is a line");
            };
            assert!(
                close(kept.start, p(0.0, 0.0)) && close(kept.end, p(40.0, 0.0)),
                "cut by {name}: {kept:?}"
            );
        }
    }

    #[test]
    fn an_arc_is_trimmed_by_every_kind_of_cutting_edge() {
        // The upper half circle of radius 50 about (40, 0): it crosses x = 40
        // at its top, (40, 50) — and the circle and arc edges somewhere on its
        // right half.
        let target: Entity = ArcEntity::new(p(40.0, 0.0), 50.0, 0.0, PI).into();
        for (name, edge) in cutting_edges() {
            let pieces = target
                .trimmed(&edge, p(-9.0, 10.0))
                .unwrap_or_else(|error| {
                    panic!("arc trimmed by {name}: {error:?}");
                });
            assert_eq!(pieces.len(), 1, "cut by {name}");
            let Entity::Arc(kept) = &pieces[0] else {
                panic!("a trimmed arc is an arc");
            };
            let kept = kept.arc();
            // The start, at (90, 0), is kept; the picked left end is gone.
            assert!(close(kept.start_point(), p(90.0, 0.0)), "cut by {name}");
            assert!(kept.sweep < PI - 1e-6, "cut by {name}: nothing was removed");
            assert!(
                edge.distance_to(kept.end_point()) < 1e-9,
                "cut by {name}: ends on the edge"
            );
        }
    }

    #[test]
    fn a_circle_is_trimmed_by_every_kind_of_cutting_edge_that_crosses_it_twice() {
        // A circle about (40, 0), radius 30: x = 40 crosses it at top and bottom.
        let target: Entity = CircleEntity::new(p(40.0, 0.0), 30.0).into();
        for (name, edge) in cutting_edges() {
            let pieces = target.trimmed(&edge, p(10.0, 0.0)).unwrap_or_else(|error| {
                panic!("circle trimmed by {name}: {error:?}");
            });
            assert_eq!(pieces.len(), 1, "cut by {name}");
            let Entity::Arc(kept) = &pieces[0] else {
                panic!("a trimmed circle becomes an arc");
            };
            let kept = kept.arc();
            assert_eq!((kept.center, kept.radius), (p(40.0, 0.0), 30.0));
            // The picked leftmost point is gone; the rightmost is still there.
            assert!(kept.distance_to_point(p(10.0, 0.0)) > 1.0, "cut by {name}");
            assert!(kept.distance_to_point(p(70.0, 0.0)) < 1e-9, "cut by {name}");
            assert!(
                edge.distance_to(kept.start_point()) < 1e-9
                    && edge.distance_to(kept.end_point()) < 1e-9
            );
        }
    }

    /// #118: the removed part is what the preview shows; with what is kept
    /// it makes up the original.
    #[test]
    fn trim_parts_name_what_goes_as_well_as_what_stays() {
        let edge: Entity = LineEntity::new(p(40.0, -60.0), p(40.0, 60.0)).into();
        let target: Entity = LineEntity::new(p(0.0, 0.0), p(100.0, 0.0)).into();
        let parts = target.trim_parts(&edge, p(80.0, 0.0)).unwrap();
        assert_eq!(
            parts.kept,
            vec![LineEntity::new(p(0.0, 0.0), p(40.0, 0.0)).into()]
        );
        assert_eq!(
            parts.removed,
            LineEntity::new(p(40.0, 0.0), p(100.0, 0.0)).into()
        );
        assert_eq!(target.trimmed(&edge, p(80.0, 0.0)).unwrap(), parts.kept);

        let circle: Entity = CircleEntity::new(p(40.0, 0.0), 30.0).into();
        let parts = circle.trim_parts(&edge, p(10.0, 0.0)).unwrap();
        let Entity::Arc(removed) = &parts.removed else {
            panic!("the removed part of a circle is an arc");
        };
        assert!(
            removed.arc().distance_to_point(p(10.0, 0.0)) < 1e-9,
            "the picked side goes"
        );
        assert!((removed.arc().sweep - PI).abs() < 1e-9);
    }

    // ── polylines as targets (#120) ──

    fn as_polyline(entity: &Entity) -> &PolylineEntity {
        match entity {
            Entity::Polyline(polyline) => polyline,
            other => panic!("expected a polyline, got {other:?}"),
        }
    }

    #[test]
    fn an_open_polyline_is_trimmed_by_every_kind_of_cutting_edge() {
        // Along the X axis, then up: every edge crosses the first segment at x = 40.
        let target: Entity =
            PolylineEntity::new(vec![p(0.0, 0.0), p(200.0, 0.0), p(200.0, 100.0)], false).into();
        for (name, edge) in cutting_edges() {
            let pieces = target.trimmed(&edge, p(10.0, 0.0)).unwrap_or_else(|error| {
                panic!("polyline trimmed by {name}: {error:?}");
            });
            let kept = as_polyline(pieces.last().unwrap());
            assert!(!kept.closed);
            assert!(
                close(*kept.vertices.last().unwrap(), p(200.0, 100.0)),
                "cut by {name}"
            );
            assert!(
                edge.distance_to(kept.vertices[0]) < 1e-9,
                "cut by {name}: starts on the edge"
            );
            assert!(
                kept.vertices[0].x > 30.0,
                "cut by {name}: the picked start is gone"
            );
        }
    }

    #[test]
    fn a_pick_in_the_middle_of_a_polyline_leaves_two_polylines() {
        let target: Entity = PolylineEntity::new(
            vec![p(0.0, 0.0), p(100.0, 0.0), p(100.0, 100.0), p(200.0, 100.0)],
            false,
        )
        .into();
        let first: Entity = LineEntity::new(p(50.0, -10.0), p(50.0, 10.0)).into();
        let second: Entity = LineEntity::new(p(150.0, 90.0), p(150.0, 110.0)).into();
        let cuts: Vec<Point2> = [first, second]
            .iter()
            .flat_map(|edge| target.intersections(edge))
            .collect();
        let parts = target.trim_parts_at(&cuts, p(100.0, 50.0)).unwrap();
        assert_eq!(
            parts.kept,
            vec![
                PolylineEntity::new(vec![p(0.0, 0.0), p(50.0, 0.0)], false).into(),
                PolylineEntity::new(vec![p(150.0, 100.0), p(200.0, 100.0)], false).into()
            ]
        );
        assert_eq!(
            parts.removed,
            PolylineEntity::new(
                vec![
                    p(50.0, 0.0),
                    p(100.0, 0.0),
                    p(100.0, 100.0),
                    p(150.0, 100.0)
                ],
                false
            )
            .into()
        );
    }

    #[test]
    fn a_closed_polyline_opens_and_must_be_crossed_twice() {
        let square: Entity = PolylineEntity::new(
            vec![p(0.0, 0.0), p(100.0, 0.0), p(100.0, 100.0), p(0.0, 100.0)],
            true,
        )
        .into();
        // A vertical line through the middle crosses the bottom and the top.
        let through: Entity = LineEntity::new(p(50.0, -20.0), p(50.0, 120.0)).into();
        let pieces = square.trimmed(&through, p(100.0, 50.0)).unwrap();
        assert_eq!(
            pieces,
            vec![
                PolylineEntity::new(
                    vec![p(50.0, 100.0), p(0.0, 100.0), p(0.0, 0.0), p(50.0, 0.0)],
                    false
                )
                .into()
            ]
        );
        // A line that only crosses the bottom edge.
        let stub: Entity = LineEntity::new(p(50.0, -20.0), p(50.0, 20.0)).into();
        assert_eq!(
            square.trimmed(&stub, p(100.0, 50.0)),
            Err(TrimError::NeedsTwoCrossings("closed polyline"))
        );
        let apart: Entity = LineEntity::new(p(500.0, -20.0), p(500.0, 20.0)).into();
        assert_eq!(
            square.trimmed(&apart, p(100.0, 50.0)),
            Err(TrimError::NoIntersection)
        );
    }

    #[test]
    fn trimming_a_bulged_polyline_keeps_its_arcs() {
        // A slot from (0,0) to (100,40) with semicircular ends of radius 20.
        let slot: Entity = PolylineEntity::with_bulges(
            vec![p(0.0, 0.0), p(100.0, 0.0), p(100.0, 40.0), p(0.0, 40.0)],
            vec![0.0, 1.0, 0.0, 1.0],
            true,
        )
        .into();
        // A vertical line through the middle; pick the right end: the left half stays.
        let through: Entity = LineEntity::new(p(50.0, -20.0), p(50.0, 60.0)).into();
        let pieces = slot.trimmed(&through, p(120.0, 20.0)).unwrap();
        let kept = as_polyline(&pieces[0]);
        assert_eq!(
            kept.vertices,
            vec![p(50.0, 40.0), p(0.0, 40.0), p(0.0, 0.0), p(50.0, 0.0)]
        );
        assert_eq!(kept.bulges().collect::<Vec<_>>(), vec![0.0, 1.0, 0.0, 0.0]);
        assert!(
            pieces[0].distance_to(p(-20.0, 20.0)) < 1e-9,
            "the left end is still round"
        );

        // A horizontal line through the middle cuts both round ends in half.
        let across: Entity = LineEntity::new(p(-50.0, 20.0), p(150.0, 20.0)).into();
        let pieces = slot.trimmed(&across, p(50.0, 40.0)).unwrap();
        let kept = as_polyline(&pieces[0]);
        assert!(close(kept.vertices[0], p(-20.0, 20.0)));
        assert!(close(*kept.vertices.last().unwrap(), p(120.0, 20.0)));
        // Two quarter arcs remain, each half of a semicircle.
        let quarter = (PI / 8.0).tan();
        let bulges: Vec<f64> = kept.bulges().collect();
        assert!(
            (bulges[0] - quarter).abs() < 1e-9 && (bulges[2] - quarter).abs() < 1e-9,
            "{bulges:?}"
        );
        assert!(
            pieces[0].distance_to(p(50.0, 0.0)) < 1e-9,
            "the bottom side stays"
        );
        assert!(
            pieces[0].distance_to(p(50.0, 40.0)) > 1.0,
            "the picked top side is gone"
        );
    }

    #[test]
    fn an_open_polyline_is_extended_to_every_kind_of_boundary() {
        // Up, then right along the X axis towards the boundaries at x = 100.
        let target: Entity =
            PolylineEntity::new(vec![p(0.0, -50.0), p(0.0, 0.0), p(60.0, 0.0)], false).into();
        for (name, boundary) in boundaries() {
            let longer = target
                .extended(&boundary, p(55.0, 0.0))
                .unwrap_or_else(|error| {
                    panic!("polyline extended to {name}: {error:?}");
                });
            let longer = as_polyline(&longer);
            assert_eq!(
                &longer.vertices[..2],
                &[p(0.0, -50.0), p(0.0, 0.0)],
                "to {name}"
            );
            assert!(
                close(longer.vertices[2], p(100.0, 0.0)),
                "to {name}: {:?}",
                longer.vertices[2]
            );
        }
    }

    #[test]
    fn a_polyline_is_extended_from_the_end_nearer_the_pick_arc_or_straight() {
        // A straight run ending in a counter-clockwise quarter arc up to (150, 50).
        let hook: Entity = PolylineEntity::with_bulges(
            vec![p(0.0, 0.0), p(100.0, 0.0), p(150.0, 50.0)],
            vec![0.0, std::f64::consts::SQRT_2 - 1.0],
            false,
        )
        .into();
        let ceiling: Entity = LineEntity::new(p(-500.0, 100.0), p(500.0, 100.0)).into();
        let longer = hook.extended(&ceiling, p(149.0, 48.0)).unwrap();
        let longer = as_polyline(&longer);
        assert!(
            close(longer.vertices[2], p(100.0, 100.0)),
            "round to the top of its circle"
        );
        assert!((longer.bulge(1) - 1.0).abs() < 1e-9);

        // The other end is straight and runs back to a wall on the left.
        let wall: Entity = LineEntity::new(p(-80.0, -50.0), p(-80.0, 50.0)).into();
        let longer = hook.extended(&wall, p(5.0, 0.0)).unwrap();
        assert_eq!(as_polyline(&longer).vertices[0], p(-80.0, 0.0));

        // The reach of each end is that of its segment alone.
        let within = Aabb::from_points(p(-500.0, -500.0), p(500.0, 500.0));
        let around_the_arc = hook.extension_reach(p(149.0, 48.0), &within).unwrap();
        assert!(
            close(around_the_arc.min(), p(50.0, 0.0))
                && close(around_the_arc.max(), p(150.0, 100.0))
        );
        assert!(hook.extension_reach(p(5.0, 0.0), &within).unwrap().min().x <= -500.0);
    }

    #[test]
    fn a_pick_between_two_crossings_splits_the_target_in_two() {
        let target: Entity = LineEntity::new(p(0.0, 0.0), p(200.0, 0.0)).into();
        let circle: Entity = CircleEntity::new(p(90.0, 0.0), 50.0).into();
        let pieces = target.trimmed(&circle, p(90.0, 0.0)).unwrap();
        assert_eq!(
            pieces,
            vec![
                LineEntity::new(p(0.0, 0.0), p(40.0, 0.0)).into(),
                LineEntity::new(p(140.0, 0.0), p(200.0, 0.0)).into()
            ]
        );
    }

    #[test]
    fn trimming_needs_a_crossing_and_says_what_is_missing() {
        let edge: Entity = LineEntity::new(p(40.0, -60.0), p(40.0, 60.0)).into();
        let apart: Entity = LineEntity::new(p(50.0, 0.0), p(100.0, 0.0)).into();
        assert_eq!(
            apart.trimmed(&edge, p(70.0, 0.0)),
            Err(TrimError::NoIntersection)
        );

        let circle_apart: Entity = CircleEntity::new(p(200.0, 0.0), 10.0).into();
        assert_eq!(
            circle_apart.trimmed(&edge, p(200.0, 10.0)),
            Err(TrimError::NoIntersection)
        );
    }

    /// Regression: tangent contact.
    #[test]
    fn a_tangent_cutting_edge_divides_a_line_but_cannot_open_a_circle() {
        // y = 50 touches the circle about (0,0) of radius 50 at (0, 50).
        let tangent: Entity = LineEntity::new(p(-100.0, 50.0), p(100.0, 50.0)).into();
        let circle: Entity = CircleEntity::new(p(0.0, 0.0), 50.0).into();

        // The line, cut by the circle at the single point of contact.
        let pieces = tangent.trimmed(&circle, p(60.0, 50.0)).unwrap();
        assert_eq!(pieces.len(), 1);
        let Entity::Line(kept) = &pieces[0] else {
            panic!("a trimmed line is a line");
        };
        assert!(
            close(kept.start, p(-100.0, 50.0)) && close(kept.end, p(0.0, 50.0)),
            "{kept:?}"
        );

        // The circle, touched once by the line: one crossing is not enough.
        assert_eq!(
            circle.trimmed(&tangent, p(0.0, -50.0)),
            Err(TrimError::NeedsTwoCrossings("circle"))
        );
    }

    /// Regression: a cutting edge that only touches the target's end.
    #[test]
    fn a_cutting_edge_at_the_targets_endpoint_trims_nothing() {
        let edge: Entity = LineEntity::new(p(100.0, -60.0), p(100.0, 60.0)).into();
        let target: Entity = LineEntity::new(p(0.0, 0.0), p(100.0, 0.0)).into();
        assert_eq!(
            target.trimmed(&edge, p(50.0, 0.0)),
            Err(TrimError::NoIntersection)
        );
        assert_eq!(
            target.trimmed(&edge, p(99.0, 0.0)),
            Err(TrimError::NoIntersection)
        );

        let arc: Entity = ArcEntity::new(p(50.0, 0.0), 50.0, 0.0, PI).into(); // ends at (100,0) and (0,0)
        assert_eq!(
            arc.trimmed(&edge, p(50.0, 50.0)),
            Err(TrimError::NoIntersection)
        );
    }

    // ── EXTEND (#98): every supported combination of boundary and target ──

    /// Boundaries of every kind, each met at (100, 0) by something coming
    /// along the X axis from the left.
    fn boundaries() -> Vec<(&'static str, Entity)> {
        vec![
            (
                "line",
                LineEntity::new(p(100.0, -80.0), p(100.0, 80.0)).into(),
            ),
            ("circle", CircleEntity::new(p(150.0, 0.0), 50.0).into()),
            (
                "arc",
                ArcEntity::new(p(150.0, 0.0), 50.0, FRAC_PI_2, 3.0 * FRAC_PI_2).into(),
            ),
            (
                "polyline",
                PolylineEntity::new(vec![p(300.0, 80.0), p(100.0, 80.0), p(100.0, -80.0)], false)
                    .into(),
            ),
        ]
    }

    #[test]
    fn a_line_is_extended_to_every_kind_of_boundary() {
        let target: Entity = LineEntity::new(p(0.0, 0.0), p(60.0, 0.0)).into();
        for (name, boundary) in boundaries() {
            let longer = target
                .extended(&boundary, p(55.0, 0.0))
                .unwrap_or_else(|error| {
                    panic!("line extended to {name}: {error:?}");
                });
            let Entity::Line(longer) = longer else {
                panic!("an extended line is a line");
            };
            assert!(
                close(longer.start, p(0.0, 0.0)) && close(longer.end, p(100.0, 0.0)),
                "to {name}: {longer:?}"
            );
        }
    }

    #[test]
    fn a_line_is_extended_from_the_end_nearer_the_pick() {
        let boundary: Entity = LineEntity::new(p(-50.0, -80.0), p(-50.0, 80.0)).into();
        let target: Entity = LineEntity::new(p(0.0, 0.0), p(60.0, 0.0)).into();
        assert_eq!(
            target.extended(&boundary, p(5.0, 0.0)),
            Ok(LineEntity::new(p(-50.0, 0.0), p(60.0, 0.0)).into())
        );
        // From the other end, the same boundary is behind it.
        assert_eq!(
            target.extended(&boundary, p(55.0, 0.0)),
            Err(ExtendError::NoMeeting)
        );
    }

    #[test]
    fn an_arc_is_extended_to_every_kind_of_boundary() {
        // A quarter arc about (50, 0), radius 50, from (50, 50) at 90° round
        // to (0, 0) at 180°. Grown clockwise from its start it reaches (100, 0).
        let target: Entity = ArcEntity::new(p(50.0, 0.0), 50.0, FRAC_PI_2, PI).into();
        for (name, boundary) in boundaries() {
            let longer = target
                .extended(&boundary, p(55.0, 50.0))
                .unwrap_or_else(|error| {
                    panic!("arc extended to {name}: {error:?}");
                });
            let Entity::Arc(longer) = longer else {
                panic!("an extended arc is an arc");
            };
            let longer = longer.arc();
            assert!(
                close(longer.end_point(), p(0.0, 0.0)),
                "to {name}: the other end stays"
            );
            assert!(
                close(longer.start_point(), p(100.0, 0.0)),
                "to {name}: {:?}",
                longer.start_point()
            );
        }
    }

    // ── BREAK (#126): every kind of entity ──

    #[test]
    fn a_line_and_an_arc_are_broken_between_two_points_or_split_at_one() {
        let line: Entity = LineEntity::new(p(0.0, 0.0), p(100.0, 0.0)).into();
        assert_eq!(
            line.broken(p(30.0, 2.0), p(70.0, -1.0)),
            Ok(vec![
                LineEntity::new(p(0.0, 0.0), p(30.0, 0.0)).into(),
                LineEntity::new(p(70.0, 0.0), p(100.0, 0.0)).into()
            ])
        );
        assert_eq!(
            line.broken(p(40.0, 0.0), p(40.0, 0.0)),
            Ok(vec![
                LineEntity::new(p(0.0, 0.0), p(40.0, 0.0)).into(),
                LineEntity::new(p(40.0, 0.0), p(100.0, 0.0)).into()
            ])
        );

        let arc: Entity = ArcEntity::new(p(0.0, 0.0), 50.0, 0.0, PI).into();
        let pieces = arc.broken(p(0.0, 50.0), p(0.0, 50.0)).unwrap();
        assert_eq!(pieces.len(), 2);
        let Entity::Arc(second) = &pieces[1] else {
            panic!("an arc breaks into arcs");
        };
        assert!(close(second.arc().start_point(), p(0.0, 50.0)));
        assert!(close(second.arc().end_point(), p(-50.0, 0.0)));
    }

    /// Breaking at an endpoint is refused.
    #[test]
    fn breaking_at_an_endpoint_or_away_the_whole_object_is_refused() {
        let line: Entity = LineEntity::new(p(0.0, 0.0), p(100.0, 0.0)).into();
        assert_eq!(
            line.broken(p(0.0, 0.0), p(0.0, 0.0)),
            Err(BreakError::AtEndpoint)
        );
        assert_eq!(
            line.broken(p(100.0, 0.0), p(100.0, 0.0)),
            Err(BreakError::AtEndpoint)
        );
        assert_eq!(
            line.broken(p(-5.0, 0.0), p(-9.0, 3.0)),
            Err(BreakError::AtEndpoint),
            "both land on the start"
        );
        assert_eq!(
            line.broken(p(0.0, 0.0), p(100.0, 0.0)),
            Err(BreakError::NothingLeft)
        );

        let arc: Entity = ArcEntity::new(p(0.0, 0.0), 50.0, 0.0, PI).into();
        assert_eq!(
            arc.broken(p(50.0, 0.0), p(50.0, 0.0)),
            Err(BreakError::AtEndpoint)
        );
        let open: Entity =
            PolylineEntity::new(vec![p(0.0, 0.0), p(50.0, 0.0), p(50.0, 50.0)], false).into();
        assert_eq!(
            open.broken(p(50.0, 50.0), p(50.0, 50.0)),
            Err(BreakError::AtEndpoint)
        );
        assert_eq!(
            open.broken(p(0.0, 0.0), p(50.0, 50.0)),
            Err(BreakError::NothingLeft)
        );
    }

    #[test]
    fn a_circle_needs_two_points_and_becomes_an_arc() {
        let circle: Entity = CircleEntity::new(p(0.0, 0.0), 50.0).into();
        // East to north: the first quadrant goes.
        let pieces = circle.broken(p(50.0, 0.0), p(0.0, 50.0)).unwrap();
        let Entity::Arc(kept) = &pieces[0] else {
            panic!("a broken circle is an arc");
        };
        assert!((kept.arc().sweep - 3.0 * FRAC_PI_2).abs() < 1e-12);
        assert!(close(kept.arc().start_point(), p(0.0, 50.0)));
        assert_eq!(
            circle.broken(p(50.0, 0.0), p(80.0, 0.0)),
            Err(BreakError::NeedsTwoPoints("circle"))
        );
    }

    #[test]
    fn polylines_are_broken_open_or_closed() {
        let open: Entity =
            PolylineEntity::new(vec![p(0.0, 0.0), p(100.0, 0.0), p(100.0, 100.0)], false).into();
        assert_eq!(
            open.broken(p(50.0, 0.0), p(100.0, 50.0)),
            Ok(vec![
                PolylineEntity::new(vec![p(0.0, 0.0), p(50.0, 0.0)], false).into(),
                PolylineEntity::new(vec![p(100.0, 50.0), p(100.0, 100.0)], false).into()
            ])
        );
        let square: Entity = PolylineEntity::new(
            vec![p(0.0, 0.0), p(100.0, 0.0), p(100.0, 100.0), p(0.0, 100.0)],
            true,
        )
        .into();
        assert_eq!(
            square.broken(p(50.0, 0.0), p(100.0, 50.0)),
            Ok(vec![
                PolylineEntity::new(
                    vec![
                        p(100.0, 50.0),
                        p(100.0, 100.0),
                        p(0.0, 100.0),
                        p(0.0, 0.0),
                        p(50.0, 0.0)
                    ],
                    false
                )
                .into()
            ])
        );
        assert_eq!(
            square.broken(p(50.0, 0.0), p(50.0, 0.0)),
            Err(BreakError::NeedsTwoPoints("closed polyline"))
        );
    }

    // ── STRETCH (#128): every kind of entity ──

    fn window(x0: f64, y0: f64, x1: f64, y1: f64) -> Aabb {
        Aabb::from_points(p(x0, y0), p(x1, y1))
    }

    #[test]
    fn a_line_stretches_by_the_end_inside_the_window_and_moves_when_both_are() {
        let line: Entity = LineEntity::new(p(0.0, 0.0), p(100.0, 0.0)).into();
        let offset = Vec2::new(20.0, 10.0);

        let right = window(80.0, -10.0, 120.0, 10.0);
        assert_eq!(line.stretch_hold(&right), StretchHold::Part);
        assert_eq!(
            line.stretched(&right, offset),
            Some(LineEntity::new(p(0.0, 0.0), p(120.0, 10.0)).into())
        );

        let all = window(-10.0, -10.0, 120.0, 10.0);
        assert_eq!(line.stretch_hold(&all), StretchHold::Whole);
        assert_eq!(
            line.stretched(&all, offset),
            Some(LineEntity::new(p(20.0, 10.0), p(120.0, 10.0)).into())
        );

        // The window crosses the line but holds neither end.
        let middle = window(40.0, -10.0, 60.0, 10.0);
        assert_eq!(line.stretch_hold(&middle), StretchHold::Nothing);
        assert_eq!(line.stretched(&middle, offset), Some(line.clone()));

        // Stretched onto its own other end: not a line any more.
        assert_eq!(line.stretched(&right, Vec2::new(-100.0, 0.0)), None);
    }

    /// Circles move only if their centre is inside.
    #[test]
    fn a_circle_moves_only_if_its_centre_is_inside_the_window() {
        let circle: Entity = CircleEntity::new(p(0.0, 0.0), 50.0).into();
        let offset = Vec2::new(30.0, 0.0);

        let around_centre = window(-10.0, -10.0, 60.0, 10.0);
        assert_eq!(circle.stretch_hold(&around_centre), StretchHold::Whole);
        assert_eq!(
            circle.stretched(&around_centre, offset),
            Some(CircleEntity::new(p(30.0, 0.0), 50.0).into())
        );

        // Most of the circle is inside, but not its centre.
        let most = window(1.0, -60.0, 60.0, 60.0);
        assert_eq!(circle.stretch_hold(&most), StretchHold::Nothing);
        assert_eq!(circle.stretched(&most, offset), Some(circle.clone()));
    }

    /// An arc held by one end follows the grip rule: through the moved end,
    /// the old midpoint, and the other end.
    #[test]
    fn an_arc_held_by_one_end_is_refitted_like_a_grip_edit() {
        let entity: Entity = ArcEntity::new(p(0.0, 0.0), 50.0, 0.0, PI).into();
        let offset = Vec2::new(20.0, -10.0);

        let at_start = window(40.0, -10.0, 60.0, 10.0);
        assert_eq!(entity.stretch_hold(&at_start), StretchHold::Part);
        let stretched = entity.stretched(&at_start, offset).unwrap();
        assert_eq!(stretched, entity.grip_moved(0, p(70.0, -10.0)).unwrap());
        let Entity::Arc(arc) = &stretched else {
            panic!("still an arc");
        };
        assert!(close(arc.arc().start_point(), p(70.0, -10.0)));
        assert!(close(arc.arc().end_point(), p(-50.0, 0.0)));
        assert!(
            arc.arc().distance_to_point(p(0.0, 50.0)) < 1e-9,
            "it still passes through the old midpoint"
        );

        let at_end = window(-60.0, -10.0, -40.0, 10.0);
        assert_eq!(
            entity.stretched(&at_end, offset),
            entity.grip_moved(2, p(-30.0, -10.0))
        );

        // Both ends inside: the arc moves as it is, though its top is outside.
        let both = window(-60.0, -10.0, 60.0, 10.0);
        assert_eq!(entity.stretch_hold(&both), StretchHold::Whole);
        assert_eq!(
            entity.stretched(&both, offset),
            Some(entity.translated(offset))
        );

        // Only its top inside: no end is held.
        assert_eq!(
            entity.stretch_hold(&window(-10.0, 40.0, 10.0, 60.0)),
            StretchHold::Nothing
        );

        // The moved end in line with the midpoint and the other end: no arc.
        let quarter: Entity = ArcEntity::new(p(0.0, 0.0), 50.0, 0.0, FRAC_PI_2).into();
        let Entity::Arc(q) = &quarter else {
            unreachable!()
        };
        let (middle, end) = (q.arc().mid_point(), q.arc().end_point());
        let onto_the_chord = middle + (middle - end) - p(50.0, 0.0);
        assert_eq!(
            quarter.stretched(&window(40.0, -10.0, 60.0, 10.0), onto_the_chord),
            None
        );
    }

    #[test]
    fn a_polyline_moves_the_vertices_inside_and_keeps_its_bulges() {
        // A 100 × 60 rectangle whose right side is a half circle.
        let slot: Entity = PolylineEntity::with_bulges(
            vec![p(0.0, 0.0), p(100.0, 0.0), p(100.0, 60.0), p(0.0, 60.0)],
            vec![0.0, 1.0, 0.0, 0.0],
            true,
        )
        .into();
        let offset = Vec2::new(40.0, 0.0);
        let right = window(90.0, -10.0, 140.0, 70.0);
        assert_eq!(slot.stretch_hold(&right), StretchHold::Part);
        assert_eq!(
            slot.stretched(&right, offset),
            Some(
                PolylineEntity::with_bulges(
                    vec![p(0.0, 0.0), p(140.0, 0.0), p(140.0, 60.0), p(0.0, 60.0)],
                    vec![0.0, 1.0, 0.0, 0.0],
                    true,
                )
                .into()
            )
        );

        // One vertex only: the corner is pulled out, the arc re-fitted to it.
        let corner = window(90.0, 50.0, 110.0, 70.0);
        let Some(Entity::Polyline(pulled)) = slot.stretched(&corner, Vec2::new(0.0, 20.0)) else {
            panic!("a polyline");
        };
        assert_eq!(pulled.vertices[2], p(100.0, 80.0));
        assert_eq!(pulled.bulge(1), 1.0, "still a half circle, now 80 across");

        assert_eq!(
            slot.stretch_hold(&window(-10.0, -10.0, 150.0, 70.0)),
            StretchHold::Whole
        );
        assert_eq!(
            slot.stretch_hold(&window(40.0, -10.0, 60.0, 70.0)),
            StretchHold::Nothing
        );
        // A vertex moved onto its neighbour: refused.
        assert_eq!(slot.stretched(&right, Vec2::new(-100.0, 0.0)), None);
    }

    // ── several edges at once (#119) ──

    #[test]
    fn cuts_from_several_edges_bound_the_part_that_goes() {
        let target: Entity = LineEntity::new(p(0.0, 0.0), p(100.0, 0.0)).into();
        let edges: [Entity; 3] = [
            LineEntity::new(p(20.0, -10.0), p(20.0, 10.0)).into(),
            LineEntity::new(p(50.0, -10.0), p(50.0, 10.0)).into(),
            LineEntity::new(p(80.0, -10.0), p(80.0, 10.0)).into(),
        ];
        let cuts: Vec<Point2> = edges
            .iter()
            .flat_map(|edge| target.intersections(edge))
            .collect();

        // Between the second and third edge: only that stretch goes.
        let parts = target.trim_parts_at(&cuts, p(65.0, 0.0)).unwrap();
        assert_eq!(
            parts.removed,
            LineEntity::new(p(50.0, 0.0), p(80.0, 0.0)).into()
        );
        assert_eq!(
            parts.kept,
            vec![
                LineEntity::new(p(0.0, 0.0), p(50.0, 0.0)).into(),
                LineEntity::new(p(80.0, 0.0), p(100.0, 0.0)).into()
            ]
        );
        // Before the first edge: from the start to it.
        let parts = target.trim_parts_at(&cuts, p(5.0, 0.0)).unwrap();
        assert_eq!(
            parts.removed,
            LineEntity::new(p(0.0, 0.0), p(20.0, 0.0)).into()
        );
        // With no cuts at all there is nothing to trim.
        assert_eq!(
            target.trim_parts_at(&[], p(5.0, 0.0)),
            Err(TrimError::NoIntersection)
        );
    }

    #[test]
    fn an_extension_stops_at_the_first_of_several_boundaries() {
        let target: Entity = LineEntity::new(p(0.0, 0.0), p(10.0, 0.0)).into();
        let far: Entity = LineEntity::new(p(80.0, -10.0), p(80.0, 10.0)).into();
        let near: Entity = CircleEntity::new(p(50.0, 0.0), 10.0).into();
        let boundary: Vec<Primitive> = [far, near].iter().flat_map(Entity::primitives).collect();
        assert_eq!(
            target.extended_to(&boundary, p(9.0, 0.0)),
            Ok(LineEntity::new(p(0.0, 0.0), p(40.0, 0.0)).into())
        );
        assert_eq!(
            target.extended_to(&[], p(9.0, 0.0)),
            Err(ExtendError::NoMeeting)
        );
    }

    #[test]
    fn the_reach_of_an_extension_covers_what_it_could_meet() {
        let within = Aabb::from_points(p(-100.0, -100.0), p(500.0, 300.0));
        let line: Entity = LineEntity::new(p(0.0, 0.0), p(10.0, 0.0)).into();
        // From the end: a box running east from (10, 0) past the far side.
        let east = line.extension_reach(p(9.0, 0.0), &within).unwrap();
        assert_eq!(east.min(), p(10.0, 0.0));
        assert!(east.max().x >= 500.0 && east.max().y == 0.0);
        // From the start: west.
        let west = line.extension_reach(p(1.0, 0.0), &within).unwrap();
        assert!(west.min().x <= -100.0 && west.max() == p(0.0, 0.0));

        let arc: Entity = ArcEntity::new(p(0.0, 0.0), 50.0, 0.0, 1.0).into();
        assert_eq!(
            arc.extension_reach(p(50.0, 0.0), &within),
            Some(Aabb::from_points(p(-50.0, -50.0), p(50.0, 50.0)))
        );
        let circle: Entity = CircleEntity::new(p(0.0, 0.0), 10.0).into();
        assert_eq!(circle.extension_reach(p(10.0, 0.0), &within), None);
    }

    /// When the extension would not meet the boundary, nothing changes.
    #[test]
    fn extending_needs_a_boundary_ahead_and_says_what_is_wrong() {
        let boundary: Entity = LineEntity::new(p(100.0, 20.0), p(100.0, 80.0)).into();
        let below: Entity = LineEntity::new(p(0.0, 0.0), p(60.0, 0.0)).into();
        assert_eq!(
            below.extended(&boundary, p(55.0, 0.0)),
            Err(ExtendError::NoMeeting)
        );

        let small_arc: Entity = ArcEntity::new(p(0.0, 0.0), 5.0, 0.0, 1.0).into();
        assert_eq!(
            small_arc.extended(&boundary, p(5.0, 0.0)),
            Err(ExtendError::NoMeeting)
        );

        let circle: Entity = CircleEntity::new(p(0.0, 0.0), 10.0).into();
        assert_eq!(
            circle.extended(&boundary, p(10.0, 0.0)),
            Err(ExtendError::Unsupported("circle"))
        );
        // A closed polyline has no end to extend.
        let closed: Entity =
            PolylineEntity::new(vec![p(0.0, 0.0), p(60.0, 0.0), p(60.0, 10.0)], true).into();
        assert_eq!(
            closed.extended(&boundary, p(55.0, 0.0)),
            Err(ExtendError::Unsupported("closed polyline"))
        );
        // An open one that would pass below the boundary never meets it.
        let open: Entity = PolylineEntity::new(vec![p(0.0, 0.0), p(60.0, 0.0)], false).into();
        assert_eq!(
            open.extended(&boundary, p(55.0, 0.0)),
            Err(ExtendError::NoMeeting)
        );
    }

    // ── FILLET (#99) and CHAMFER (#100) ──

    /// Two lines that would meet at the origin at a right angle.
    fn l_lines() -> (Entity, Entity) {
        (
            LineEntity::new(p(10.0, 0.0), p(100.0, 0.0)).into(),
            LineEntity::new(p(0.0, 10.0), p(0.0, 80.0)).into(),
        )
    }

    #[test]
    fn two_lines_are_filleted_with_a_tangent_arc() {
        let (first, second) = l_lines();
        let joined = first
            .filleted(p(50.0, 0.0), &second, p(0.0, 50.0), 20.0)
            .unwrap();
        assert_eq!(
            joined.first,
            LineEntity::new(p(20.0, 0.0), p(100.0, 0.0)).into()
        );
        assert_eq!(
            joined.second,
            LineEntity::new(p(0.0, 20.0), p(0.0, 80.0)).into()
        );
        let Some(Entity::Arc(arc)) = &joined.joint else {
            panic!("a fillet with a radius has an arc");
        };
        let arc = arc.arc();
        assert!(close(arc.center, p(20.0, 20.0)) && (arc.radius - 20.0).abs() < 1e-12);
        assert!((arc.sweep - FRAC_PI_2).abs() < 1e-12);
    }

    #[test]
    fn a_zero_radius_fillet_is_a_sharp_corner_without_an_arc() {
        let (first, second) = l_lines();
        let joined = first
            .filleted(p(50.0, 0.0), &second, p(0.0, 50.0), 0.0)
            .unwrap();
        assert_eq!(
            joined.first,
            LineEntity::new(p(0.0, 0.0), p(100.0, 0.0)).into()
        );
        assert_eq!(
            joined.second,
            LineEntity::new(p(0.0, 0.0), p(0.0, 80.0)).into()
        );
        assert_eq!(joined.joint, None);
    }

    // ── fillets with arcs and circles (#124) ──

    #[test]
    fn a_line_is_filleted_to_a_circle_which_stays_whole() {
        let floor: Entity = LineEntity::new(p(-200.0, 0.0), p(200.0, 0.0)).into();
        let circle: Entity = CircleEntity::new(p(0.0, 40.0), 30.0).into();
        let joined = floor
            .filleted(p(60.0, 0.0), &circle, p(25.0, 25.0), 10.0)
            .unwrap();
        assert_eq!(joined.second, circle, "untouched, to the last bit");
        let Entity::Line(line) = &joined.first else {
            panic!("a line stays a line");
        };
        assert!(close(line.start, p(700f64.sqrt(), 0.0)) && close(line.end, p(200.0, 0.0)));
        let Some(Entity::Arc(arc)) = &joined.joint else {
            panic!("a fillet arc");
        };
        assert!(close(arc.center, p(700f64.sqrt(), 10.0)) && (arc.radius - 10.0).abs() < 1e-12);
    }

    #[test]
    fn a_line_and_an_arc_are_filleted_in_either_order() {
        let floor: Entity = LineEntity::new(p(-100.0, 0.0), p(80.0, 0.0)).into();
        let arc: Entity = ArcEntity::new(p(0.0, 60.0), 50.0, -FRAC_PI_2, 0.0).into(); // (0,10) → (50,60)
        let one = floor
            .filleted(p(-50.0, 0.0), &arc, p(40.0, 30.0), 8.0)
            .unwrap();
        let other = arc
            .filleted(p(40.0, 30.0), &floor, p(-50.0, 0.0), 8.0)
            .unwrap();
        assert_eq!(one.first, other.second);
        assert_eq!(one.second, other.first);
        let (Some(Entity::Arc(a)), Some(Entity::Arc(b))) = (&one.joint, &other.joint) else {
            panic!("fillet arcs");
        };
        assert!(close(a.center, b.center) && (a.radius - 8.0).abs() < 1e-12);
        // The arc kept its far end and now starts where the fillet touches it.
        let Entity::Arc(kept) = &one.second else {
            panic!("an arc stays an arc");
        };
        assert!(close(kept.arc().end_point(), p(50.0, 60.0)));
        assert!((kept.arc().start_point().distance_to(a.center) - 8.0).abs() < 1e-9);
    }

    #[test]
    fn two_arcs_are_filleted_and_a_radius_that_does_not_fit_is_refused() {
        let left: Entity = ArcEntity::new(p(-50.0, 0.0), 30.0, 0.0, PI).into();
        let right: Entity = ArcEntity::new(p(50.0, 0.0), 30.0, 0.0, PI).into();
        let joined = left
            .filleted(p(-30.0, 22.0), &right, p(30.0, 22.0), 40.0)
            .unwrap();
        let Some(Entity::Arc(arc)) = &joined.joint else {
            panic!("a fillet arc");
        };
        assert!(close(arc.center, p(0.0, 2400f64.sqrt())));
        // Too small to reach across the gap of 40 between them.
        assert_eq!(
            left.filleted(p(-30.0, 22.0), &right, p(30.0, 22.0), 15.0),
            Err(CornerError::NoSolution)
        );
    }

    #[test]
    fn a_zero_radius_brings_a_line_and_an_arc_to_a_sharp_corner() {
        let level: Entity = LineEntity::new(p(-100.0, 40.0), p(10.0, 40.0)).into();
        let arc: Entity = ArcEntity::new(p(0.0, 0.0), 50.0, 0.0, 0.5).into();
        let joined = level
            .filleted(p(0.0, 40.0), &arc, p(48.0, 14.0), 0.0)
            .unwrap();
        assert_eq!(joined.joint, None);
        assert_eq!(
            joined.first,
            LineEntity::new(p(-100.0, 40.0), p(30.0, 40.0)).into()
        );
        let Entity::Arc(grown) = &joined.second else {
            panic!("an arc stays an arc");
        };
        assert!(close(grown.arc().end_point(), p(30.0, 40.0)));
    }

    // ── corners within a polyline (#125) ──

    fn l_polyline(closed: bool) -> Entity {
        PolylineEntity::new(
            vec![p(0.0, 0.0), p(100.0, 0.0), p(100.0, 60.0), p(0.0, 60.0)],
            closed,
        )
        .into()
    }

    #[test]
    fn a_corner_of_a_polyline_is_filleted_between_two_picked_segments() {
        let rounded = l_polyline(false)
            .with_corner_between(p(50.0, 0.0), p(100.0, 30.0), CornerJoin::Fillet(10.0))
            .unwrap();
        let Entity::Polyline(rounded) = rounded else {
            panic!("a polyline stays a polyline");
        };
        assert_eq!(
            rounded.vertices,
            vec![
                p(0.0, 0.0),
                p(90.0, 0.0),
                p(100.0, 10.0),
                p(100.0, 60.0),
                p(0.0, 60.0)
            ]
        );
        assert!((rounded.bulge(1) - (2f64.sqrt() - 1.0)).abs() < 1e-12);
        assert!(!rounded.closed);
    }

    #[test]
    fn a_chamfer_within_a_polyline_gives_the_first_distance_to_the_first_pick() {
        let join = CornerJoin::Chamfer([30.0, 10.0]);
        let bottom_first = l_polyline(false)
            .with_corner_between(p(50.0, 0.0), p(100.0, 30.0), join)
            .unwrap();
        assert_eq!(
            bottom_first,
            PolylineEntity::new(
                vec![
                    p(0.0, 0.0),
                    p(70.0, 0.0),
                    p(100.0, 10.0),
                    p(100.0, 60.0),
                    p(0.0, 60.0)
                ],
                false
            )
            .into()
        );
        let right_first = l_polyline(false)
            .with_corner_between(p(100.0, 30.0), p(50.0, 0.0), join)
            .unwrap();
        assert_eq!(
            right_first,
            PolylineEntity::new(
                vec![
                    p(0.0, 0.0),
                    p(90.0, 0.0),
                    p(100.0, 30.0),
                    p(100.0, 60.0),
                    p(0.0, 60.0)
                ],
                false
            )
            .into()
        );
    }

    #[test]
    fn polyline_corners_explain_what_cannot_be_done() {
        let join = CornerJoin::Fillet(10.0);
        let open = l_polyline(false);
        // The same segment twice; opposite sides; the open ends.
        assert_eq!(
            open.with_corner_between(p(20.0, 0.0), p(80.0, 0.0), join),
            Err(CornerError::NotAdjacent)
        );
        assert_eq!(
            open.with_corner_between(p(50.0, 0.0), p(50.0, 60.0), join),
            Err(CornerError::NotAdjacent)
        );
        assert_eq!(
            open.with_corner_between(p(50.0, 0.0), p(100.0, 30.0), CornerJoin::Fillet(70.0)),
            Err(CornerError::TooLarge)
        );
        // Closed, the last segment and the first do meet.
        assert!(
            l_polyline(true)
                .with_corner_between(p(0.0, 30.0), p(50.0, 0.0), join)
                .is_ok()
        );
        // An arc segment at the corner.
        let curved: Entity = PolylineEntity::with_bulges(
            vec![p(0.0, 0.0), p(100.0, 0.0), p(100.0, 60.0)],
            vec![0.0, 0.5],
            false,
        )
        .into();
        assert_eq!(
            curved.with_corner_between(p(50.0, 0.0), p(105.0, 30.0), join),
            Err(CornerError::Unsupported("arc segment"))
        );
        let line: Entity = LineEntity::new(p(0.0, 0.0), p(10.0, 0.0)).into();
        assert_eq!(
            line.with_every_corner(join),
            Err(CornerError::Unsupported("line"))
        );
    }

    #[test]
    fn every_corner_of_a_polyline_is_done_and_counted() {
        let (rounded, done, skipped) = l_polyline(true)
            .with_every_corner(CornerJoin::Fillet(10.0))
            .unwrap();
        assert_eq!((done, skipped), (4, 0));
        let Entity::Polyline(rounded) = rounded else {
            panic!("a polyline stays a polyline");
        };
        assert!(rounded.closed && rounded.vertices.len() == 8);
        // Open: the two ends are not corners.
        let (_, done, skipped) = l_polyline(false)
            .with_every_corner(CornerJoin::Chamfer([5.0, 5.0]))
            .unwrap();
        assert_eq!((done, skipped), (2, 0));
    }

    #[test]
    fn two_lines_are_chamfered_with_equal_or_unequal_distances() {
        let (first, second) = l_lines();
        let equal = first
            .chamfered(p(50.0, 0.0), &second, p(0.0, 50.0), [15.0, 15.0])
            .unwrap();
        assert_eq!(
            equal.first,
            LineEntity::new(p(15.0, 0.0), p(100.0, 0.0)).into()
        );
        assert_eq!(
            equal.second,
            LineEntity::new(p(0.0, 15.0), p(0.0, 80.0)).into()
        );
        assert_eq!(
            equal.joint,
            Some(LineEntity::new(p(15.0, 0.0), p(0.0, 15.0)).into())
        );

        let unequal = first
            .chamfered(p(50.0, 0.0), &second, p(0.0, 50.0), [30.0, 5.0])
            .unwrap();
        assert_eq!(
            unequal.joint,
            Some(LineEntity::new(p(30.0, 0.0), p(0.0, 5.0)).into())
        );
    }

    #[test]
    fn corners_refuse_parallel_lines_oversized_values_and_anything_but_lines() {
        let (first, second) = l_lines();
        let parallel: Entity = LineEntity::new(p(0.0, 30.0), p(100.0, 30.0)).into();
        assert_eq!(
            first.filleted(p(50.0, 0.0), &parallel, p(50.0, 30.0), 5.0),
            Err(CornerError::Parallel)
        );
        assert_eq!(
            first.chamfered(p(50.0, 0.0), &parallel, p(50.0, 30.0), [5.0, 5.0]),
            Err(CornerError::Parallel)
        );
        assert_eq!(
            first.filleted(p(50.0, 0.0), &second, p(0.0, 50.0), 90.0),
            Err(CornerError::TooLarge)
        );
        assert_eq!(
            first.chamfered(p(50.0, 0.0), &second, p(0.0, 50.0), [5.0, 80.0]),
            Err(CornerError::TooLarge)
        );

        // CHAMFER joins straight lines only; FILLET takes curves but not polylines yet.
        let arc: Entity = ArcEntity::new(p(0.0, 0.0), 10.0, 0.0, 1.0).into();
        assert_eq!(
            arc.chamfered(p(10.0, 0.0), &first, p(50.0, 0.0), [5.0, 5.0]),
            Err(CornerError::Unsupported("arc"))
        );
        let polyline: Entity = PolylineEntity::new(vec![p(0.0, 0.0), p(5.0, 5.0)], false).into();
        assert_eq!(
            first.filleted(p(50.0, 0.0), &polyline, p(2.0, 2.0), 5.0),
            Err(CornerError::Unsupported("polyline"))
        );
    }

    /// #121: both directions, and the collapse case.
    #[test]
    fn an_arc_offsets_to_a_concentric_arc_in_either_direction() {
        let arc: Entity = ArcEntity::new(p(0.0, 0.0), 10.0, 0.0, FRAC_PI_2).into();
        assert_eq!(
            arc.offset(4.0, p(50.0, 50.0)),
            Ok(vec![
                ArcEntity::new(p(0.0, 0.0), 14.0, 0.0, FRAC_PI_2).into()
            ])
        );
        assert_eq!(
            arc.offset(4.0, p(1.0, 1.0)),
            Ok(vec![
                ArcEntity::new(p(0.0, 0.0), 6.0, 0.0, FRAC_PI_2).into()
            ])
        );
        assert_eq!(arc.offset(10.0, p(1.0, 1.0)), Err(OffsetError::Collapses));
        assert_eq!(arc.offset(25.0, p(1.0, 1.0)), Err(OffsetError::Collapses));
        assert_eq!(arc.offset(4.0, p(0.0, 10.0)), Err(OffsetError::NoSide));
    }

    /// #123: polylines, through the method of ADR-0011.
    #[test]
    fn a_polyline_offsets_to_one_or_more_polylines() {
        let rectangle: Entity = PolylineEntity::new(
            vec![p(0.0, 0.0), p(100.0, 0.0), p(100.0, 60.0), p(0.0, 60.0)],
            true,
        )
        .into();
        // Inwards: a smaller rectangle with sharp corners.
        assert_eq!(
            rectangle.offset(10.0, p(50.0, 30.0)),
            Ok(vec![
                PolylineEntity::new(
                    vec![p(10.0, 10.0), p(90.0, 10.0), p(90.0, 50.0), p(10.0, 50.0)],
                    true
                )
                .into()
            ])
        );
        // Outwards: the corners are rounded, so the result has arcs.
        let outer = rectangle.offset(10.0, p(50.0, -30.0)).unwrap();
        assert_eq!(outer.len(), 1);
        let Entity::Polyline(outer) = &outer[0] else {
            panic!("a polyline offsets to polylines");
        };
        assert!(outer.closed && outer.has_arcs() && outer.vertices.len() == 8);
        // Too far inwards leaves nothing; a point on it chooses no side.
        assert_eq!(
            rectangle.offset(40.0, p(50.0, 30.0)),
            Err(OffsetError::Collapses)
        );
        assert_eq!(
            rectangle.offset(10.0, p(50.0, 0.0)),
            Err(OffsetError::NoSide)
        );

        // A shape with a waist falls apart into two.
        let dumbbell: Entity = PolylineEntity::new(
            vec![
                p(0.0, 0.0),
                p(60.0, 0.0),
                p(60.0, 25.0),
                p(120.0, 25.0),
                p(120.0, 0.0),
                p(180.0, 0.0),
                p(180.0, 60.0),
                p(120.0, 60.0),
                p(120.0, 35.0),
                p(60.0, 35.0),
                p(60.0, 60.0),
                p(0.0, 60.0),
            ],
            true,
        )
        .into();
        assert_eq!(dumbbell.offset(10.0, p(30.0, 30.0)).unwrap().len(), 2);
    }

    #[test]
    fn a_polyline_offset_through_a_point_passes_through_it() {
        let corner: Entity =
            PolylineEntity::new(vec![p(0.0, 0.0), p(100.0, 0.0), p(100.0, 100.0)], false).into();
        let through = p(40.0, 25.0);
        let offsets = corner.offset_through(through).unwrap();
        assert_eq!(
            offsets,
            vec![
                PolylineEntity::new(vec![p(0.0, 25.0), p(75.0, 25.0), p(75.0, 100.0)], false)
                    .into()
            ]
        );
        assert!(offsets[0].distance_to(through) < 1e-9);
        assert_eq!(
            corner.offset_through(p(40.0, 0.0)),
            Err(OffsetError::NoSide)
        );
    }

    /// #121: the Through option — the offset passes through the point.
    #[test]
    fn an_offset_through_a_point_passes_through_it() {
        let line: Entity = LineEntity::new(p(0.0, 0.0), p(10.0, 0.0)).into();
        assert_eq!(
            line.offset_through(p(300.0, 7.0)),
            Ok(vec![LineEntity::new(p(0.0, 7.0), p(10.0, 7.0)).into()]),
            "the distance is to the line carried on, not to the segment"
        );
        assert_eq!(line.offset_through(p(300.0, 0.0)), Err(OffsetError::NoSide));

        let circle: Entity = CircleEntity::new(p(0.0, 0.0), 10.0).into();
        assert_eq!(
            circle.offset_through(p(3.0, 4.0)),
            Ok(vec![CircleEntity::new(p(0.0, 0.0), 5.0).into()])
        );
        assert_eq!(
            circle.offset_through(p(0.0, 26.0)),
            Ok(vec![CircleEntity::new(p(0.0, 0.0), 26.0).into()])
        );
        assert_eq!(
            circle.offset_through(p(0.0, 0.0)),
            Err(OffsetError::Collapses)
        );
        assert_eq!(
            circle.offset_through(p(10.0, 0.0)),
            Err(OffsetError::NoSide)
        );

        let arc: Entity = ArcEntity::new(p(0.0, 0.0), 10.0, 0.0, FRAC_PI_2).into();
        assert_eq!(
            arc.offset_through(p(-12.0, -16.0)),
            Ok(vec![
                ArcEntity::new(p(0.0, 0.0), 20.0, 0.0, FRAC_PI_2).into()
            ])
        );
    }

    // ── invariants, over any input (#129) ──

    mod properties {
        use proptest::prelude::*;

        use super::*;

        fn coordinate() -> impl Strategy<Value = f64> {
            (-200i32..200).prop_map(f64::from)
        }

        fn any_point() -> impl Strategy<Value = Point2> {
            (coordinate(), coordinate()).prop_map(|(x, y)| p(x, y))
        }

        fn any_window() -> impl Strategy<Value = Aabb> {
            (any_point(), any_point()).prop_map(|(a, b)| Aabb::from_points(a, b))
        }

        /// A polyline of distinct vertices, with arc segments here and there.
        fn any_polyline() -> impl Strategy<Value = PolylineEntity> {
            (
                proptest::collection::vec(any_point(), 2..7),
                proptest::collection::vec(prop_oneof![Just(0.0), -1.2..1.2f64], 7),
                any::<bool>(),
            )
                .prop_filter_map("distinct vertices", |(points, bulges, closed)| {
                    let mut vertices: Vec<Point2> = Vec::new();
                    for point in points {
                        if !vertices.contains(&point) {
                            vertices.push(point);
                        }
                    }
                    (vertices.len() >= 2).then(|| {
                        let bulges = bulges[..vertices.len()].to_vec();
                        PolylineEntity::with_bulges(vertices, bulges, closed)
                    })
                })
        }

        proptest! {
            /// STRETCH: a vertex moves by the offset if the window holds it
            /// and stays where it is otherwise; arc segments keep their
            /// bulge; and holding everything is a plain move.
            #[test]
            fn stretching_moves_exactly_the_vertices_the_window_holds(
                polyline in any_polyline(), window in any_window(),
                dx in -80.0..80f64, dy in -80.0..80f64,
            ) {
                let offset = Vec2::new(dx, dy);
                let entity: Entity = polyline.clone().into();
                let held = polyline.vertices.iter().filter(|v| window.contains_point(**v)).count();
                let hold = entity.stretch_hold(&window);
                prop_assert_eq!(hold == StretchHold::Nothing, held == 0);
                prop_assert_eq!(hold == StretchHold::Whole, held == polyline.vertices.len());

                match entity.stretched(&window, offset) {
                    Some(Entity::Polyline(stretched)) => {
                        prop_assert_eq!(stretched.vertices.len(), polyline.vertices.len());
                        for (before, after) in polyline.vertices.iter().zip(&stretched.vertices) {
                            let expected = if window.contains_point(*before) { *before + offset } else { *before };
                            prop_assert_eq!(*after, expected);
                        }
                        prop_assert_eq!(stretched.closed, polyline.closed);
                        prop_assert_eq!(stretched.bulges().collect::<Vec<_>>(), polyline.bulges().collect::<Vec<_>>());
                        if hold == StretchHold::Whole {
                            prop_assert_eq!(Entity::from(stretched), entity.translated(offset));
                        }
                    }
                    Some(other) => prop_assert!(false, "a polyline became {:?}", other),
                    // Refused only for a vertex landing on its neighbour.
                    None => prop_assert_eq!(hold, StretchHold::Part),
                }
            }

            /// STRETCH: lines, circles, and arcs. A circle moves whole or not
            /// at all; an arc held by one end keeps its other end and still
            /// passes through its old midpoint.
            #[test]
            fn stretching_a_line_circle_or_arc_moves_only_what_is_held(
                a in any_point(), b in any_point(), radius in 1.0..80f64,
                start in 0.0..6.2f64, sweep in 0.2..6.0f64,
                window in any_window(), dx in -80.0..80f64, dy in -80.0..80f64,
            ) {
                let offset = Vec2::new(dx, dy);
                let moved = |point: Point2| if window.contains_point(point) { point + offset } else { point };

                if a != b {
                    let line: Entity = LineEntity::new(a, b).into();
                    if let Some(Entity::Line(stretched)) = line.stretched(&window, offset) {
                        prop_assert_eq!((stretched.start, stretched.end), (moved(a), moved(b)));
                    }
                }

                let circle: Entity = CircleEntity::new(a, radius).into();
                let expected: Entity = CircleEntity::new(moved(a), radius).into();
                prop_assert_eq!(circle.stretched(&window, offset), Some(expected));

                let entity: Entity = ArcEntity::new(a, radius, start, start + sweep).into();
                let Entity::Arc(original) = &entity else { unreachable!() };
                let arc = original.arc();
                let (first, last) = (arc.start_point(), arc.end_point());
                match (entity.stretch_hold(&window), entity.stretched(&window, offset)) {
                    (StretchHold::Nothing, result) => prop_assert_eq!(result, Some(entity.clone())),
                    (StretchHold::Whole, result) => prop_assert_eq!(result, Some(entity.translated(offset))),
                    (StretchHold::Part, Some(Entity::Arc(stretched))) => {
                        let stretched = stretched.arc();
                        let reach = 1e-6 * (1.0 + stretched.radius);
                        // An arc is kept counter-clockwise, so dragging an
                        // end across the chord swaps which end is its start.
                        let ends = [stretched.start_point(), stretched.end_point()];
                        for expected in [moved(first), moved(last)] {
                            prop_assert!(
                                ends.iter().any(|end| end.distance_to(expected) < reach),
                                "no end at {:?}: {:?}", expected, ends
                            );
                        }
                        prop_assert!(
                            stretched.distance_to_point(arc.mid_point()) < reach,
                            "through the old midpoint"
                        );
                    }
                    // Three points in a line make no arc: refused.
                    (StretchHold::Part, None) => {}
                    (StretchHold::Part, other) => prop_assert!(false, "an arc became {:?}", other),
                }
            }

            /// BREAK then JOIN: an entity broken at one point and joined
            /// again covers exactly what it did.
            #[test]
            fn breaking_at_a_point_and_joining_gives_the_entity_back(
                polyline in any_polyline(), along in 0.05..0.95f64, which in 0..6usize,
            ) {
                prop_assume!(!polyline.closed);
                let entity: Entity = polyline.clone().into();
                let primitives: Vec<_> = polyline.primitives().collect();
                let on = primitives[which % primitives.len()];
                let at = match on {
                    Primitive::Segment(segment) => segment.start + (segment.end - segment.start) * along,
                    Primitive::Arc(arc) => arc.point_at(arc.start + arc.sweep * along),
                    Primitive::Circle(circle) => circle.center,
                };
                // A path that crosses itself may be nearest somewhere else:
                // only breaks that land where they were aimed are compared.
                if let Ok(pieces) = entity.broken(at, at) {
                    prop_assert_eq!(pieces.len(), 2);
                    let refs: Vec<&Entity> = pieces.iter().collect();
                    if let Ok(joined) = crate::join(&refs) {
                        prop_assert_eq!(joined.len(), 1);
                        let Entity::Polyline(whole) = &joined[0].entity else {
                            return Err(TestCaseError::fail("not a polyline"));
                        };
                        prop_assert_eq!(whole.vertices.first(), polyline.vertices.first());
                        prop_assert_eq!(whole.vertices.last(), polyline.vertices.last());
                        let length = |polyline: &PolylineEntity| -> f64 {
                            polyline
                                .primitives()
                                .map(|primitive| match primitive {
                                    Primitive::Segment(segment) => segment.length(),
                                    Primitive::Arc(arc) => arc.radius * arc.sweep,
                                    Primitive::Circle(_) => 0.0,
                                })
                                .sum()
                        };
                        let (before, after) = (length(&polyline), length(whole));
                        prop_assert!((before - after).abs() < 1e-6 * (1.0 + before), "{} became {}", before, after);
                    }
                }
            }
        }
    }
}

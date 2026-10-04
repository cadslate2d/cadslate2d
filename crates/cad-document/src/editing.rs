//! What the modify commands can do to each kind of entity (DESIGN §22).
//!
//! M2 supports deliberately narrow cases; each operation here is a pure
//! function that either produces the result or says, as a value, why not.
//! The supported combinations are the published matrices in
//! `docs/modify-matrix.md`; a combination not listed there is an
//! `Unsupported` error, never a guess.

use cad_core::tolerance::LinearTolerance;
use cad_core::{Aabb, Point2};
use cad_geometry::{
    Corner, Primitive, extend_arc, extend_segment, offset_circle, offset_segment, trim_arc_parts,
    trim_circle_parts, trim_segment_parts,
};

use crate::{ArcEntity, CircleEntity, Entity, LineEntity};

const TOLERANCE: LinearTolerance = LinearTolerance::DEFAULT;

/// Why an entity could not be offset.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OffsetError {
    /// This kind of entity cannot be offset yet; carries its name.
    Unsupported(&'static str),
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
}

impl From<cad_geometry::CornerError> for CornerError {
    fn from(error: cad_geometry::CornerError) -> Self {
        match error {
            cad_geometry::CornerError::Parallel => Self::Parallel,
            cad_geometry::CornerError::TooLarge => Self::TooLarge,
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
    /// A circle has no ends: the cutting edge must cross it twice.
    CircleNeedsTwo,
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
                    None => Err(TrimError::CircleNeedsTwo),
                }
            }
            Entity::Polyline(_) => Err(TrimError::Unsupported(self.kind_name())),
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
            Entity::Circle(_) | Entity::Polyline(_) => {
                Err(ExtendError::Unsupported(self.kind_name()))
            }
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

    /// This line and `other` joined by an arc of `radius` tangent to both,
    /// each cut back or carried on to meet it. A zero radius makes a sharp
    /// corner and no arc.
    pub fn filleted(
        &self,
        pick: Point2,
        other: &Entity,
        other_pick: Point2,
        radius: f64,
    ) -> Result<Cornered, CornerError> {
        let fillet = self
            .corner_with(pick, other, other_pick)?
            .fillet(radius, TOLERANCE)?;
        let [first, second] = fillet.lines;
        Ok(Cornered {
            first: LineEntity::new(first.start, first.end).into(),
            second: LineEntity::new(second.start, second.end).into(),
            joint: fillet.arc.map(|arc| ArcEntity::from(arc).into()),
        })
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

    /// A copy of the entity `distance` away, on the side `side` is on: a
    /// parallel line, or a concentric circle.
    pub fn offset(&self, distance: f64, side: Point2) -> Result<Entity, OffsetError> {
        match self {
            Entity::Line(line) => offset_segment(&line.segment(), distance, side)
                .map(|parallel| LineEntity::new(parallel.start, parallel.end).into())
                .ok_or(OffsetError::NoSide),
            Entity::Circle(circle) => {
                let from_center = circle.center.distance_to(side);
                if TOLERANCE.eq(from_center, circle.radius) {
                    return Err(OffsetError::NoSide);
                }
                offset_circle(&circle.circle(), distance, side, TOLERANCE)
                    .map(|offset| CircleEntity::new(offset.center, offset.radius).into())
                    .ok_or(OffsetError::Collapses)
            }
            Entity::Arc(_) | Entity::Polyline(_) => Err(OffsetError::Unsupported(self.kind_name())),
        }
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
            Ok(LineEntity::new(p(0.0, 2.0), p(10.0, 2.0)).into())
        );
        assert_eq!(
            line.offset(2.0, p(3.0, -9.0)),
            Ok(LineEntity::new(p(0.0, -2.0), p(10.0, -2.0)).into())
        );
        assert_eq!(line.offset(2.0, p(3.0, 0.0)), Err(OffsetError::NoSide));
    }

    #[test]
    fn a_circle_offsets_in_either_direction_but_not_to_nothing() {
        let circle: Entity = CircleEntity::new(p(0.0, 0.0), 10.0).into();
        assert_eq!(
            circle.offset(4.0, p(50.0, 0.0)),
            Ok(CircleEntity::new(p(0.0, 0.0), 14.0).into())
        );
        assert_eq!(
            circle.offset(4.0, p(1.0, 0.0)),
            Ok(CircleEntity::new(p(0.0, 0.0), 6.0).into())
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

        let polyline: Entity = PolylineEntity::new(vec![p(0.0, 0.0), p(100.0, 0.0)], false).into();
        assert_eq!(
            polyline.trimmed(&edge, p(10.0, 0.0)),
            Err(TrimError::Unsupported("polyline"))
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
            Err(TrimError::CircleNeedsTwo)
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
        let polyline: Entity = PolylineEntity::new(vec![p(0.0, 0.0), p(60.0, 0.0)], false).into();
        assert_eq!(
            polyline.extended(&boundary, p(55.0, 0.0)),
            Err(ExtendError::Unsupported("polyline"))
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

        let circle: Entity = CircleEntity::new(p(0.0, 0.0), 10.0).into();
        let arc: Entity = ArcEntity::new(p(0.0, 0.0), 10.0, 0.0, 1.0).into();
        assert_eq!(
            first.filleted(p(50.0, 0.0), &circle, p(10.0, 0.0), 5.0),
            Err(CornerError::Unsupported("circle"))
        );
        assert_eq!(
            arc.chamfered(p(10.0, 0.0), &first, p(50.0, 0.0), [5.0, 5.0]),
            Err(CornerError::Unsupported("arc"))
        );
    }

    #[test]
    fn arcs_and_polylines_are_not_supported_yet_and_say_so() {
        let arc: Entity = ArcEntity::new(p(0.0, 0.0), 5.0, 0.0, 1.0).into();
        let polyline: Entity = PolylineEntity::new(vec![p(0.0, 0.0), p(1.0, 0.0)], false).into();
        assert_eq!(
            arc.offset(1.0, p(9.0, 9.0)),
            Err(OffsetError::Unsupported("arc"))
        );
        assert_eq!(
            polyline.offset(1.0, p(9.0, 9.0)),
            Err(OffsetError::Unsupported("polyline"))
        );
    }
}

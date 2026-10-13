//! Grips: the handles a selected entity offers for direct editing
//! (DESIGN §16.2), and the pure function behind every grip edit:
//! `(entity, grip, new position) → edited entity`.
//!
//! | Entity | Grips, in index order | Dragging it… |
//! | --- | --- | --- |
//! | Line | Endpoint, Midpoint, Endpoint | an endpoint stretches the line; the midpoint moves it |
//! | Circle | Center, then Quadrants at 0°, 90°, 180°, 270° | the centre moves it; a quadrant sets the radius |
//! | Arc | Endpoint (start), Midpoint, Endpoint (end), Center | see below |
//! | Polyline | one Vertex per vertex | moves that vertex |
//! | Text | Insertion | moves the text |
//! | MText | Insertion | moves the paragraph |
//! | Block reference | Insertion | moves the reference |
//!
//! Arc grips follow the familiar *stretch* behaviour (ADR-0010): an arc is
//! always the arc through its start, its midpoint, and its end. Dragging
//! one of those three re-fits the arc through the dragged point and the
//! other two, which stay where they were. The centre grip moves the arc.

use std::f64::consts::{FRAC_PI_2, PI};

use cad_core::Point2;
use cad_core::tolerance::LinearTolerance;
use cad_geometry::Arc;

use crate::{
    ArcEntity, BlockReferenceEntity, CircleEntity, EllipseEntity, Entity, LineEntity, MTextEntity,
    PolylineEntity, TextEntity,
};

const TOLERANCE: LinearTolerance = LinearTolerance::DEFAULT;

/// What a grip is attached to. There is deliberately no `Radius` kind: a
/// circle's quadrant grips edit its radius (§16.2).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GripKind {
    Endpoint,
    Midpoint,
    Center,
    Vertex,
    Quadrant,
    /// The insertion point of a text.
    Insertion,
    /// A point a dimension measures.
    Definition,
    /// Where a dimension is placed: its line, arc, or text.
    Placement,
}

/// One grip of one entity. Its index in [`Entity::grips`] identifies it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EntityGrip {
    pub kind: GripKind,
    pub position: Point2,
}

fn grip(kind: GripKind, position: Point2) -> EntityGrip {
    EntityGrip { kind, position }
}

fn midpoint(a: Point2, b: Point2) -> Point2 {
    a + (b - a) * 0.5
}

impl Entity {
    /// The entity's grips, in a fixed order (see the module table).
    pub fn grips(&self) -> Vec<EntityGrip> {
        match self {
            Entity::Line(line) => vec![
                grip(GripKind::Endpoint, line.start),
                grip(GripKind::Midpoint, midpoint(line.start, line.end)),
                grip(GripKind::Endpoint, line.end),
            ],
            Entity::Circle(circle) => {
                let circle = circle.circle();
                let quadrants = (0..4).map(|quadrant| {
                    let at = circle.point_at(f64::from(quadrant) * FRAC_PI_2);
                    grip(GripKind::Quadrant, at)
                });
                std::iter::once(grip(GripKind::Center, circle.center))
                    .chain(quadrants)
                    .collect()
            }
            Entity::Arc(arc) => {
                let arc = arc.arc();
                vec![
                    grip(GripKind::Endpoint, arc.start_point()),
                    grip(GripKind::Midpoint, arc.mid_point()),
                    grip(GripKind::Endpoint, arc.end_point()),
                    grip(GripKind::Center, arc.center),
                ]
            }
            // An ellipse: its centre and the four axis ends; an arc of one,
            // its ends and its centre.
            Entity::Ellipse(entity) => {
                let ellipse = entity.ellipse();
                if ellipse.is_full() {
                    std::iter::once(grip(GripKind::Center, ellipse.center))
                        .chain(
                            [0.0, FRAC_PI_2, PI, 3.0 * FRAC_PI_2]
                                .into_iter()
                                .map(|t| grip(GripKind::Quadrant, ellipse.point_at(t))),
                        )
                        .collect()
                } else {
                    vec![
                        grip(GripKind::Endpoint, ellipse.start_point()),
                        grip(GripKind::Endpoint, ellipse.end_point()),
                        grip(GripKind::Center, ellipse.center),
                    ]
                }
            }
            Entity::Polyline(polyline) => polyline
                .vertices
                .iter()
                .map(|vertex| grip(GripKind::Vertex, *vertex))
                .collect(),
            // A spline's grips are its defining points: fit or control.
            Entity::Spline(spline) => spline
                .defining_points()
                .iter()
                .map(|point| grip(GripKind::Vertex, *point))
                .collect(),
            Entity::Text(text) => vec![grip(GripKind::Insertion, text.insertion)],
            Entity::MText(text) => vec![grip(GripKind::Insertion, text.insertion)],
            Entity::BlockRef(reference) => vec![grip(GripKind::Insertion, reference.insertion)],
            Entity::Hatch(hatch) => hatch
                .vertices()
                .into_iter()
                .map(|vertex| grip(GripKind::Vertex, vertex))
                .collect(),
            Entity::Dimension(dimension) => {
                let measured = dimension.measure.points().len();
                dimension
                    .points()
                    .into_iter()
                    .enumerate()
                    .map(|(index, point)| {
                        let kind = if index < measured {
                            GripKind::Definition
                        } else {
                            GripKind::Placement
                        };
                        grip(kind, point)
                    })
                    .collect()
            }
        }
    }

    /// The entity as it would be with grip `index` dragged to `to`.
    ///
    /// `None` when there is no such grip, or when the result would not be a
    /// valid entity: a line of no length, a circle of no radius, an arc
    /// through three points in a line, a polyline vertex on top of its
    /// neighbour.
    pub fn grip_moved(&self, index: usize, to: Point2) -> Option<Entity> {
        if !to.is_finite() {
            return None;
        }
        match self {
            Entity::Line(line) => {
                let moved = match index {
                    0 => LineEntity::new(to, line.end),
                    1 => {
                        let offset = to - midpoint(line.start, line.end);
                        LineEntity::new(line.start + offset, line.end + offset)
                    }
                    2 => LineEntity::new(line.start, to),
                    _ => return None,
                };
                (!TOLERANCE.coincident(moved.start, moved.end)).then(|| moved.into())
            }
            Entity::Circle(circle) => match index {
                0 => Some(CircleEntity::new(to, circle.radius).into()),
                1..=4 => {
                    let radius = circle.center.distance_to(to);
                    (!TOLERANCE.is_zero(radius))
                        .then(|| CircleEntity::new(circle.center, radius).into())
                }
                _ => None,
            },
            Entity::Arc(entity) => {
                let arc = entity.arc();
                let (start, middle, end) = (arc.start_point(), arc.mid_point(), arc.end_point());
                let refit = |start, middle, end| {
                    Arc::through_points(start, middle, end, TOLERANCE)
                        .map(|arc| Entity::from(ArcEntity::from(arc)))
                };
                match index {
                    0 => refit(to, middle, end),
                    1 => refit(start, to, end),
                    2 => refit(start, middle, to),
                    3 => Some(
                        ArcEntity {
                            center: to,
                            ..*entity
                        }
                        .into(),
                    ),
                    _ => None,
                }
            }
            // The centre moves the ellipse whole; a major-axis end sets the
            // long axis and keeps the short one's length; a minor-axis end
            // sets the short one; an arc's end sets its parameter.
            Entity::Ellipse(entity) => {
                let ellipse = entity.ellipse();
                let (a, b) = (
                    ellipse.major.length(),
                    ellipse.major.length() * ellipse.ratio,
                );
                let moved = if ellipse.is_full() {
                    match index {
                        0 => EllipseEntity {
                            center: to,
                            ..*entity
                        },
                        1 | 3 => {
                            let major = if index == 1 {
                                to - ellipse.center
                            } else {
                                ellipse.center - to
                            };
                            let length = major.length();
                            if TOLERANCE.is_zero(length) {
                                return None;
                            }
                            EllipseEntity {
                                major,
                                ratio: (b / length).min(1.0),
                                ..*entity
                            }
                        }
                        2 | 4 => {
                            let length = ellipse.center.distance_to(to);
                            if TOLERANCE.is_zero(length) {
                                return None;
                            }
                            EllipseEntity {
                                ratio: (length / a).min(1.0),
                                ..*entity
                            }
                        }
                        _ => return None,
                    }
                } else {
                    match index {
                        0 => EllipseEntity {
                            start_parameter: ellipse.parameter_of(to),
                            ..*entity
                        },
                        1 => EllipseEntity {
                            end_parameter: ellipse.parameter_of(to),
                            ..*entity
                        },
                        2 => EllipseEntity {
                            center: to,
                            ..*entity
                        },
                        _ => return None,
                    }
                };
                moved.ellipse().is_valid().then(|| moved.into())
            }
            Entity::Spline(spline) => {
                let mut points = spline.defining_points().to_vec();
                *points.get_mut(index)? = to;
                spline.redefined(points).map(Entity::from)
            }
            Entity::Polyline(polyline) => {
                let (before, after) = polyline_neighbours(polyline, index)?;
                let lands_on = |neighbour: Option<usize>| {
                    neighbour.is_some_and(|at| TOLERANCE.coincident(polyline.vertices[at], to))
                };
                if lands_on(before) || lands_on(after) {
                    return None;
                }
                // The bulges stay: each neighbouring arc is re-fitted between
                // its new ends with the same included angle.
                let mut vertices = polyline.vertices.clone();
                vertices[index] = to;
                Some(
                    PolylineEntity::with_bulges(
                        vertices,
                        polyline.bulges().collect(),
                        polyline.closed,
                    )
                    .into(),
                )
            }
            Entity::Text(text) => (index == 0).then(|| {
                TextEntity {
                    insertion: to,
                    ..text.clone()
                }
                .into()
            }),
            Entity::MText(text) => (index == 0).then(|| {
                MTextEntity {
                    insertion: to,
                    ..text.clone()
                }
                .into()
            }),
            Entity::BlockRef(reference) => (index == 0).then(|| {
                BlockReferenceEntity {
                    insertion: to,
                    ..reference.clone()
                }
                .into()
            }),
            // A measured point or the placement moves; the result must
            // still be a dimension that can be laid out.
            Entity::Dimension(dimension) => dimension
                .with_point(index, to)
                .filter(|moved| moved.check().is_ok())
                .map(Entity::from),
            // A loop vertex moves, as a polyline's does: not onto a
            // neighbour, and the loop must still bound a region.
            Entity::Hatch(hatch) => {
                let vertices = hatch.vertices();
                let onto_neighbour = |other: Option<&Point2>| {
                    other.is_some_and(|other| TOLERANCE.coincident(*other, to))
                };
                if onto_neighbour(vertices.get(index.wrapping_sub(1)))
                    || onto_neighbour(vertices.get(index + 1))
                {
                    return None;
                }
                hatch
                    .with_vertex(index, to)
                    .filter(|moved| moved.check().is_ok())
                    .map(Entity::from)
            }
        }
    }

    /// What to draw while grip `index` is being dragged to `to`: the part of
    /// the entity that changes, as it would look.
    ///
    /// For most entities that is the whole edited entity. For a polyline it
    /// is only the two segments that meet at the dragged vertex, so previewing
    /// a vertex of a very large polyline never copies the polyline.
    pub fn grip_preview(&self, index: usize, to: Point2) -> Option<Entity> {
        let Entity::Polyline(polyline) = self else {
            return self.grip_moved(index, to);
        };
        if !to.is_finite() {
            return None;
        }
        let (before, after) = polyline_neighbours(polyline, index)?;
        let lands_on = |neighbour: Option<usize>| {
            neighbour.is_some_and(|at| TOLERANCE.coincident(polyline.vertices[at], to))
        };
        if lands_on(before) || lands_on(after) {
            return None;
        }
        // A short open polyline: the neighbour before, the dragged vertex,
        // the neighbour after — with the bulges of the two segments between.
        let mut vertices = Vec::with_capacity(3);
        let mut bulges = Vec::with_capacity(2);
        if let Some(before) = before {
            vertices.push(polyline.vertices[before]);
            bulges.push(polyline.bulge(before));
        }
        vertices.push(to);
        if let Some(after) = after {
            vertices.push(polyline.vertices[after]);
            bulges.push(polyline.bulge(index));
        }
        (vertices.len() > 1).then(|| PolylineEntity::with_bulges(vertices, bulges, false).into())
    }
}

/// The vertices joined to vertex `index` by a segment: the one before and
/// the one after, wrapping around when the polyline is closed. `None` when
/// there is no such vertex.
fn polyline_neighbours(
    polyline: &PolylineEntity,
    index: usize,
) -> Option<(Option<usize>, Option<usize>)> {
    let count = polyline.vertices.len();
    if index >= count {
        return None;
    }
    // Two vertices close only through an arc; see `polyline_closes`.
    let wraps = cad_geometry::polyline_closes(count, polyline.closed, |at| polyline.bulge(at));
    let before = match index {
        0 if wraps => Some(count - 1),
        0 => None,
        _ => Some(index - 1),
    };
    let after = match index + 1 {
        next if next < count => Some(next),
        _ if wraps => Some(0),
        _ => None,
    };
    Some((before, after))
}

#[cfg(test)]
mod tests {
    use std::f64::consts::PI;

    use super::*;

    fn p(x: f64, y: f64) -> Point2 {
        Point2::new(x, y)
    }

    fn kinds(entity: &Entity) -> Vec<GripKind> {
        entity.grips().iter().map(|grip| grip.kind).collect()
    }

    fn positions(entity: &Entity) -> Vec<Point2> {
        entity.grips().iter().map(|grip| grip.position).collect()
    }

    fn close(a: Point2, b: Point2) -> bool {
        a.distance_to(b) < 1e-9
    }

    // ── line (#87) ──

    fn line() -> Entity {
        LineEntity::new(p(0.0, 0.0), p(10.0, 4.0)).into()
    }

    #[test]
    fn a_line_has_an_endpoint_a_midpoint_and_an_endpoint() {
        use GripKind::{Endpoint, Midpoint};
        assert_eq!(kinds(&line()), vec![Endpoint, Midpoint, Endpoint]);
        assert_eq!(
            positions(&line()),
            vec![p(0.0, 0.0), p(5.0, 2.0), p(10.0, 4.0)]
        );
    }

    #[test]
    fn a_line_endpoint_grip_stretches_and_leaves_the_other_end() {
        assert_eq!(
            line().grip_moved(0, p(-3.0, 7.0)),
            Some(LineEntity::new(p(-3.0, 7.0), p(10.0, 4.0)).into())
        );
        assert_eq!(
            line().grip_moved(2, p(20.0, 0.0)),
            Some(LineEntity::new(p(0.0, 0.0), p(20.0, 0.0)).into())
        );
    }

    #[test]
    fn a_line_midpoint_grip_moves_the_whole_line() {
        assert_eq!(
            line().grip_moved(1, p(105.0, 52.0)),
            Some(LineEntity::new(p(100.0, 50.0), p(110.0, 54.0)).into())
        );
    }

    #[test]
    fn a_line_cannot_be_stretched_to_no_length() {
        assert_eq!(line().grip_moved(0, p(10.0, 4.0)), None);
        assert_eq!(line().grip_moved(2, p(0.0, 0.0)), None);
        assert_eq!(line().grip_moved(3, p(1.0, 1.0)), None, "no such grip");
        assert_eq!(line().grip_moved(0, p(f64::NAN, 0.0)), None);
    }

    // ── circle (#88) ──

    fn circle() -> Entity {
        CircleEntity::new(p(10.0, 20.0), 5.0).into()
    }

    #[test]
    fn a_circle_has_a_centre_and_four_quadrant_grips() {
        use GripKind::{Center, Quadrant};
        assert_eq!(
            kinds(&circle()),
            vec![Center, Quadrant, Quadrant, Quadrant, Quadrant]
        );
        let at = positions(&circle());
        let expected = [
            p(10.0, 20.0),
            p(15.0, 20.0),
            p(10.0, 25.0),
            p(5.0, 20.0),
            p(10.0, 15.0),
        ];
        assert!(at.iter().zip(expected).all(|(a, b)| close(*a, b)), "{at:?}");
    }

    #[test]
    fn the_centre_grip_moves_the_circle() {
        assert_eq!(
            circle().grip_moved(0, p(-1.0, -2.0)),
            Some(CircleEntity::new(p(-1.0, -2.0), 5.0).into())
        );
    }

    #[test]
    fn every_quadrant_grip_sets_the_radius_to_its_distance_from_the_centre() {
        for index in 1..=4 {
            // Wherever the grip is dragged, not only along its own axis.
            assert_eq!(
                circle().grip_moved(index, p(13.0, 24.0)),
                Some(CircleEntity::new(p(10.0, 20.0), 5.0).into()),
                "grip {index}: a 3-4-5 triangle keeps the radius at 5"
            );
            assert_eq!(
                circle().grip_moved(index, p(10.0, 32.0)),
                Some(CircleEntity::new(p(10.0, 20.0), 12.0).into())
            );
        }
    }

    /// #88: dragging a quadrant grip to the centre is rejected.
    #[test]
    fn a_quadrant_grip_dragged_to_the_centre_is_rejected() {
        for index in 1..=4 {
            assert_eq!(circle().grip_moved(index, p(10.0, 20.0)), None);
        }
        assert_eq!(circle().grip_moved(5, p(0.0, 0.0)), None, "no such grip");
    }

    // ── arc (#89) ──

    /// The upper half of a circle of radius 10 about the origin:
    /// from (10, 0) over (0, 10) to (-10, 0).
    fn arc() -> Entity {
        ArcEntity::new(p(0.0, 0.0), 10.0, 0.0, PI).into()
    }

    fn as_arc(entity: &Entity) -> Arc {
        match entity {
            Entity::Arc(arc) => arc.arc(),
            other => panic!("expected an arc, got {other:?}"),
        }
    }

    #[test]
    fn an_arc_has_start_midpoint_end_and_centre_grips() {
        use GripKind::{Center, Endpoint, Midpoint};
        assert_eq!(kinds(&arc()), vec![Endpoint, Midpoint, Endpoint, Center]);
        let at = positions(&arc());
        let expected = [p(10.0, 0.0), p(0.0, 10.0), p(-10.0, 0.0), p(0.0, 0.0)];
        assert!(at.iter().zip(expected).all(|(a, b)| close(*a, b)), "{at:?}");
    }

    #[test]
    fn the_arc_centre_grip_moves_the_arc_unchanged() {
        assert_eq!(
            arc().grip_moved(3, p(7.0, -3.0)),
            Some(ArcEntity::new(p(7.0, -3.0), 10.0, 0.0, PI).into())
        );
    }

    #[test]
    fn the_arc_midpoint_grip_keeps_both_ends_and_refits_through_the_new_point() {
        let edited = as_arc(&arc().grip_moved(1, p(0.0, 4.0)).unwrap());
        // Still from (10,0) to (-10,0), now a shallow arc through (0,4).
        assert!(close(edited.start_point(), p(10.0, 0.0)));
        assert!(close(edited.end_point(), p(-10.0, 0.0)));
        assert!(edited.distance_to_point(p(0.0, 4.0)) < 1e-9);
        assert!(edited.radius > 10.0 && edited.sweep < PI);
        // Centre on the perpendicular bisector of the chord.
        assert!(edited.center.x.abs() < 1e-9 && edited.center.y < 0.0);
    }

    #[test]
    fn dragging_the_arc_midpoint_across_the_chord_flips_the_arc_to_the_other_side() {
        let edited = as_arc(&arc().grip_moved(1, p(0.0, -10.0)).unwrap());
        // The lower half: stored counter-clockwise, so it now starts at (-10,0).
        assert!(close(edited.start_point(), p(-10.0, 0.0)));
        assert!(close(edited.end_point(), p(10.0, 0.0)));
        assert!(close(edited.mid_point(), p(0.0, -10.0)));
    }

    #[test]
    fn an_arc_endpoint_grip_keeps_the_midpoint_and_the_other_end() {
        // Drag the start from (10,0) out to (10,10).
        let edited = as_arc(&arc().grip_moved(0, p(10.0, 10.0)).unwrap());
        for fixed in [p(0.0, 10.0), p(-10.0, 0.0), p(10.0, 10.0)] {
            assert!(
                edited.distance_to_point(fixed) < 1e-9,
                "must pass through {fixed:?}"
            );
        }
        assert!(
            close(edited.end_point(), p(-10.0, 0.0)),
            "the other end stays an end"
        );
        assert!(close(edited.start_point(), p(10.0, 10.0)));

        let edited = as_arc(&arc().grip_moved(2, p(-10.0, -5.0)).unwrap());
        assert!(close(edited.start_point(), p(10.0, 0.0)));
        assert!(close(edited.end_point(), p(-10.0, -5.0)));
        assert!(edited.distance_to_point(p(0.0, 10.0)) < 1e-9);
    }

    /// #89: degenerate (collinear) positions are rejected, not turned into garbage.
    #[test]
    fn an_arc_grip_dragged_into_line_with_the_other_two_points_is_rejected() {
        // The midpoint onto the chord.
        assert_eq!(arc().grip_moved(1, p(0.0, 0.0)), None);
        assert_eq!(arc().grip_moved(1, p(3.0, 0.0)), None);
        // An endpoint onto the line through the midpoint and the other end.
        assert_eq!(
            arc().grip_moved(0, p(10.0, 20.0)),
            None,
            "on the line through (0,10) and (-10,0)"
        );
        // An endpoint onto another defining point.
        assert_eq!(arc().grip_moved(0, p(-10.0, 0.0)), None);
        assert_eq!(arc().grip_moved(2, p(0.0, 10.0)), None);
        assert_eq!(arc().grip_moved(4, p(1.0, 1.0)), None, "no such grip");
    }

    #[test]
    fn an_edited_arc_offers_grips_at_its_new_defining_points() {
        let edited = arc().grip_moved(1, p(0.0, 4.0)).unwrap();
        let at = positions(&edited);
        assert!(
            close(at[0], p(10.0, 0.0)) && close(at[1], p(0.0, 4.0)) && close(at[2], p(-10.0, 0.0))
        );
    }

    // ── polyline (#90) ──

    fn polyline(closed: bool) -> Entity {
        PolylineEntity::new(
            vec![p(0.0, 0.0), p(10.0, 0.0), p(10.0, 10.0), p(0.0, 10.0)],
            closed,
        )
        .into()
    }

    #[test]
    fn a_polyline_has_one_vertex_grip_per_vertex() {
        for closed in [false, true] {
            assert_eq!(kinds(&polyline(closed)), vec![GripKind::Vertex; 4]);
            assert_eq!(
                positions(&polyline(closed)),
                vec![p(0.0, 0.0), p(10.0, 0.0), p(10.0, 10.0), p(0.0, 10.0)]
            );
        }
    }

    #[test]
    fn a_vertex_grip_moves_only_that_vertex() {
        for closed in [false, true] {
            assert_eq!(
                polyline(closed).grip_moved(1, p(15.0, -5.0)),
                Some(
                    PolylineEntity::new(
                        vec![p(0.0, 0.0), p(15.0, -5.0), p(10.0, 10.0), p(0.0, 10.0)],
                        closed
                    )
                    .into()
                )
            );
        }
    }

    #[test]
    fn a_vertex_cannot_be_dropped_on_a_neighbour() {
        let open = polyline(false);
        assert_eq!(open.grip_moved(1, p(0.0, 0.0)), None);
        assert_eq!(open.grip_moved(1, p(10.0, 10.0)), None);
        // The ends of an open polyline are not neighbours…
        assert!(open.grip_moved(0, p(0.0, 10.0)).is_some());
        // …but in a closed one they are.
        assert_eq!(polyline(true).grip_moved(0, p(0.0, 10.0)), None);
        assert_eq!(open.grip_moved(4, p(1.0, 1.0)), None, "no such grip");
    }

    #[test]
    fn the_preview_of_a_vertex_drag_is_only_the_segments_that_change() {
        let to = p(20.0, 5.0);
        let stub = |vertices: Vec<Point2>| Some(Entity::from(PolylineEntity::new(vertices, false)));

        let open = polyline(false);
        assert_eq!(
            open.grip_preview(1, to),
            stub(vec![p(0.0, 0.0), to, p(10.0, 10.0)])
        );
        assert_eq!(open.grip_preview(0, to), stub(vec![to, p(10.0, 0.0)]));
        assert_eq!(open.grip_preview(3, to), stub(vec![p(10.0, 10.0), to]));

        let closed = polyline(true);
        assert_eq!(
            closed.grip_preview(0, to),
            stub(vec![p(0.0, 10.0), to, p(10.0, 0.0)])
        );
        assert_eq!(
            closed.grip_preview(3, to),
            stub(vec![p(10.0, 10.0), to, p(0.0, 0.0)])
        );
        assert_eq!(
            closed.grip_preview(1, p(0.0, 0.0)),
            None,
            "same rule as the edit"
        );
    }

    /// A slot: two straight sides and two semicircular ends.
    fn slot() -> Entity {
        PolylineEntity::with_bulges(
            vec![p(0.0, 0.0), p(10.0, 0.0), p(10.0, 4.0), p(0.0, 4.0)],
            vec![0.0, 1.0, 0.0, 1.0],
            true,
        )
        .into()
    }

    /// #114: vertex grips keep the bulges of the neighbouring segments.
    #[test]
    fn moving_a_vertex_keeps_the_bulges_of_its_segments() {
        let Some(Entity::Polyline(moved)) = slot().grip_moved(2, p(10.0, 8.0)) else {
            panic!("a polyline stays a polyline");
        };
        assert_eq!(moved.bulges().collect::<Vec<_>>(), vec![0.0, 1.0, 0.0, 1.0]);
        assert_eq!(moved.vertices[2], p(10.0, 8.0));
        // The right end is still a semicircle, now over a chord of 8.
        let Some(cad_geometry::Primitive::Arc(end)) = moved.primitives().nth(1) else {
            panic!("the second segment is an arc");
        };
        assert!((end.radius - 4.0).abs() < 1e-12 && end.center.distance_to(p(10.0, 4.0)) < 1e-12);
    }

    #[test]
    fn the_preview_of_a_bulged_vertex_drag_carries_both_bulges() {
        // Vertex 1 sits between a straight segment and a semicircle.
        let to = p(12.0, -3.0);
        assert_eq!(
            slot().grip_preview(1, to),
            Some(
                PolylineEntity::with_bulges(
                    vec![p(0.0, 0.0), to, p(10.0, 4.0)],
                    vec![0.0, 1.0],
                    false
                )
                .into()
            )
        );
        // Vertex 0 closes the loop: the arc from vertex 3 arrives, a straight one leaves.
        assert_eq!(
            slot().grip_preview(0, to),
            Some(
                PolylineEntity::with_bulges(
                    vec![p(0.0, 4.0), to, p(10.0, 0.0)],
                    vec![1.0, 0.0],
                    false
                )
                .into()
            )
        );
    }

    #[test]
    fn previews_of_other_entities_are_the_edited_entity() {
        for (entity, index) in [(line(), 0), (circle(), 2), (arc(), 1)] {
            let to = p(3.0, 7.5);
            assert_eq!(entity.grip_preview(index, to), entity.grip_moved(index, to));
        }
    }

    #[test]
    fn a_two_vertex_closed_polyline_has_one_neighbour_not_the_same_one_twice() {
        let pair: Entity = PolylineEntity::new(vec![p(0.0, 0.0), p(10.0, 0.0)], true).into();
        assert_eq!(
            pair.grip_preview(0, p(5.0, 5.0)),
            Some(PolylineEntity::new(vec![p(5.0, 5.0), p(10.0, 0.0)], false).into())
        );
    }
}

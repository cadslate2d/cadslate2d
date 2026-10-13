//! JOIN: several entities that are really one (DESIGN §22, §31).
//!
//! What is done depends on the whole set handed in, in this order:
//!
//! 1. lines that all lie on one line become one line;
//! 2. arcs that all lie on one circle, each touching or overlapping another,
//!    become one arc — or the circle, when they close it;
//! 3. otherwise, lines, arcs, and open polylines that meet end to end become
//!    polylines, one per chain. Entities that meet nothing are left alone.
//!
//! Like every modify operation this is a pure function that produces the
//! result or says, as a value, why not (`docs/modify-matrix.md`).

use cad_core::tolerance::LinearTolerance;
use cad_geometry::{
    Arc, ArcJoinError, JoinedArcs, Path, Segment, bulge_from_arc, chain_paths, cocircular,
    join_arcs, join_segments,
};

use crate::{ArcEntity, CircleEntity, Entity, LineEntity, PolylineEntity};

/// Ends are the same point within the linear tolerance class (DESIGN §21) —
/// never a literal.
const LINEAR: LinearTolerance = LinearTolerance::DEFAULT;

/// The linear tolerance for joining these entities: the default near the
/// origin, wider where their coordinates are too large to be told apart
/// that finely (ADR-0012). Without it, the two halves of a line broken ten
/// million units out would not count as collinear.
fn tolerance_for(entities: &[&Entity]) -> LinearTolerance {
    let corners: Vec<_> = entities
        .iter()
        .flat_map(|entity| {
            let bounds = entity.own_bounds();
            [bounds.min(), bounds.max()]
        })
        .collect();
    LINEAR.around(&corners)
}

/// One entity made from several.
#[derive(Clone, Debug, PartialEq)]
pub struct Joined {
    /// Which of the entities handed in it replaces, in ascending order.
    pub members: Vec<usize>,
    pub entity: Entity,
}

/// Why nothing was joined.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JoinError {
    /// Joining takes at least two entities.
    TooFew,
    /// Nothing handed in meets anything else end to end.
    NothingMeets,
    /// Arcs of one circle, with a gap between some of them. A gap could be
    /// closed either way round, so it is not closed at all.
    ArcsApart,
    /// Closing into a circle takes arcs, all of one circle.
    NotArcsOfOneCircle,
}

fn all_segments(entities: &[&Entity]) -> Option<Vec<Segment>> {
    entities
        .iter()
        .map(|entity| match entity {
            Entity::Line(line) => Some(line.segment()),
            _ => None,
        })
        .collect()
}

fn all_arcs(entities: &[&Entity]) -> Option<Vec<Arc>> {
    entities
        .iter()
        .map(|entity| match entity {
            Entity::Arc(arc) => Some(arc.arc()),
            _ => None,
        })
        .collect()
}

/// The entity as an open path, if it has two ends to join at.
fn open_path(entity: &Entity) -> Option<Path> {
    match entity {
        Entity::Line(line) => Some(Path {
            vertices: vec![line.start, line.end],
            bulges: vec![0.0; 2],
            closed: false,
        }),
        Entity::Arc(entity) => {
            let arc = entity.arc();
            Some(Path {
                vertices: vec![arc.start_point(), arc.end_point()],
                bulges: vec![bulge_from_arc(&arc, true), 0.0],
                closed: false,
            })
        }
        Entity::Polyline(polyline) if !polyline.closed => Some(polyline.path()),
        Entity::Polyline(_)
        | Entity::Circle(_)
        | Entity::Ellipse(_)
        | Entity::Spline(_)
        | Entity::Text(_)
        | Entity::AttributeDefinition(_)
        | Entity::MText(_)
        | Entity::BlockRef(_)
        | Entity::Dimension(_)
        | Entity::Hatch(_)
        | Entity::Viewport(_) => None,
    }
}

/// Joins what can be joined among `entities`. Each result names the
/// entities it replaces; any that are named by none are left as they are.
pub fn join(entities: &[&Entity]) -> Result<Vec<Joined>, JoinError> {
    if entities.len() < 2 {
        return Err(JoinError::TooFew);
    }
    let everything = || (0..entities.len()).collect();
    let tolerance = tolerance_for(entities);

    if let Some(segments) = all_segments(entities)
        && let Some(joined) = join_segments(&segments, tolerance)
    {
        return Ok(vec![Joined {
            members: everything(),
            entity: LineEntity::new(joined.start, joined.end).into(),
        }]);
    }

    if let Some(arcs) = all_arcs(entities) {
        match join_arcs(&arcs, tolerance) {
            Ok(joined) => {
                let entity = match joined {
                    JoinedArcs::Arc(arc) => ArcEntity::from(arc).into(),
                    JoinedArcs::Circle(circle) => {
                        CircleEntity::new(circle.center, circle.radius).into()
                    }
                };
                return Ok(vec![Joined {
                    members: everything(),
                    entity,
                }]);
            }
            Err(ArcJoinError::Apart) => return Err(JoinError::ArcsApart),
            // Arcs of different circles may still meet end to end.
            Err(ArcJoinError::NotOneCircle) => {}
        }
    }

    // Only entities with two ends take part; `sources` maps back from them.
    let (sources, paths): (Vec<usize>, Vec<Path>) = entities
        .iter()
        .enumerate()
        .filter_map(|(index, entity)| Some((index, open_path(entity)?)))
        .unzip();
    let chains = chain_paths(&paths, tolerance);
    if chains.is_empty() {
        return Err(JoinError::NothingMeets);
    }
    Ok(chains
        .into_iter()
        .map(|chain| Joined {
            members: chain
                .members
                .iter()
                .map(|member| sources[*member])
                .collect(),
            entity: PolylineEntity::from(chain.path).into(),
        })
        .collect())
}

/// The circle that `entities` — one arc or more, all of one circle — are
/// part of. Unlike [`join`], gaps do not matter: the whole circle is asked for.
pub fn close_into_circle(entities: &[&Entity]) -> Result<Entity, JoinError> {
    let arcs = all_arcs(entities).ok_or(JoinError::NotArcsOfOneCircle)?;
    let first = arcs.first().ok_or(JoinError::NotArcsOfOneCircle)?;
    if !cocircular(&arcs, tolerance_for(entities)) {
        return Err(JoinError::NotArcsOfOneCircle);
    }
    Ok(CircleEntity::new(first.center, first.radius).into())
}

#[cfg(test)]
mod tests {
    use std::f64::consts::{FRAC_PI_2, PI};

    use cad_core::Point2;

    use super::*;

    fn p(x: f64, y: f64) -> Point2 {
        Point2::new(x, y)
    }

    fn line(x0: f64, y0: f64, x1: f64, y1: f64) -> Entity {
        LineEntity::new(p(x0, y0), p(x1, y1)).into()
    }

    fn arc(start: f64, sweep: f64) -> Entity {
        ArcEntity::new(p(0.0, 0.0), 50.0, start, start + sweep).into()
    }

    fn joined(entities: &[Entity]) -> Result<Vec<Joined>, JoinError> {
        join(&entities.iter().collect::<Vec<_>>())
    }

    /// The single result of joining everything handed in.
    fn one(entities: &[Entity]) -> Entity {
        let mut results = joined(entities).expect("they join");
        assert_eq!(results.len(), 1, "{results:?}");
        let result = results.remove(0);
        assert_eq!(result.members, (0..entities.len()).collect::<Vec<_>>());
        result.entity
    }

    #[test]
    fn collinear_lines_become_one_line_across_gaps_and_overlaps() {
        assert_eq!(
            one(&[line(0.0, 0.0, 30.0, 0.0), line(30.0, 0.0, 100.0, 0.0)]),
            line(0.0, 0.0, 100.0, 0.0)
        );
        assert_eq!(
            one(&[
                line(0.0, 0.0, 30.0, 0.0),
                line(70.0, 0.0, 100.0, 0.0),
                line(20.0, 0.0, 40.0, 0.0)
            ]),
            line(0.0, 0.0, 100.0, 0.0),
            "the gap from 40 to 70 is bridged"
        );
    }

    #[test]
    fn arcs_of_one_circle_become_one_arc_or_the_circle() {
        let Entity::Arc(joined_arc) = one(&[arc(0.0, FRAC_PI_2), arc(FRAC_PI_2, 1.0)]) else {
            panic!("an arc");
        };
        assert!((joined_arc.arc().sweep - (FRAC_PI_2 + 1.0)).abs() < 1e-12);
        assert_eq!(
            one(&[arc(0.0, PI), arc(PI, PI)]),
            CircleEntity::new(p(0.0, 0.0), 50.0).into()
        );
    }

    #[test]
    fn arcs_of_one_circle_with_a_gap_are_refused_but_can_be_closed() {
        let apart = [arc(0.0, 1.0), arc(2.0, 1.0)];
        assert_eq!(joined(&apart), Err(JoinError::ArcsApart));
        let refs: Vec<&Entity> = apart.iter().collect();
        assert_eq!(
            close_into_circle(&refs),
            Ok(CircleEntity::new(p(0.0, 0.0), 50.0).into())
        );
        assert_eq!(
            close_into_circle(&refs[..1]),
            Ok(CircleEntity::new(p(0.0, 0.0), 50.0).into()),
            "one arc is enough"
        );
    }

    #[test]
    fn closing_takes_arcs_of_one_circle_and_nothing_else() {
        let other: Entity = ArcEntity::new(p(5.0, 0.0), 50.0, 0.0, 1.0).into();
        let a_line = line(0.0, 0.0, 10.0, 0.0);
        let an_arc = arc(0.0, 1.0);
        for set in [vec![&an_arc, &other], vec![&an_arc, &a_line], vec![]] {
            assert_eq!(close_into_circle(&set), Err(JoinError::NotArcsOfOneCircle));
        }
    }

    #[test]
    fn lines_and_arcs_meeting_end_to_end_become_one_polyline() {
        // A line, a half circle going up and back over it, and a line home.
        let joined_entity = one(&[
            line(0.0, 0.0, 100.0, 0.0),
            ArcEntity::new(p(100.0, 50.0), 50.0, -FRAC_PI_2, FRAC_PI_2).into(),
            line(0.0, 100.0, 100.0, 100.0),
        ]);
        let Entity::Polyline(polyline) = joined_entity else {
            panic!("a polyline");
        };
        assert!(!polyline.closed);
        assert_eq!(polyline.vertices.len(), 4);
        assert_eq!(polyline.vertices[0], p(0.0, 0.0));
        assert!(polyline.vertices[2].distance_to(p(100.0, 100.0)) < 1e-12);
        assert_eq!(polyline.vertices[3], p(0.0, 100.0));
        assert!((polyline.bulge(1) - 1.0).abs() < 1e-12, "the half circle");
        assert_eq!((polyline.bulge(0), polyline.bulge(2)), (0.0, 0.0));
    }

    #[test]
    fn an_arc_travelled_backwards_keeps_its_shape() {
        // The chain runs left to right; the arc was drawn right to left.
        let Entity::Polyline(polyline) = one(&[
            line(-100.0, 0.0, -50.0, 0.0),
            arc(0.0, PI), // from (50, 0) over the top to (-50, 0)
        ]) else {
            panic!("a polyline");
        };
        assert_eq!(polyline.vertices[0], p(-100.0, 0.0));
        assert!(polyline.vertices[2].distance_to(p(50.0, 0.0)) < 1e-12);
        assert!((polyline.bulge(1) + 1.0).abs() < 1e-12, "clockwise now");
        let over_the_top = polyline.primitives().nth(1).expect("the arc segment");
        assert!(
            over_the_top
                .closest_point(p(0.0, 60.0))
                .distance_to(p(0.0, 50.0))
                < 1e-9
        );
    }

    #[test]
    fn polylines_join_too_and_a_loop_closes() {
        let open: Entity =
            PolylineEntity::new(vec![p(0.0, 0.0), p(100.0, 0.0), p(100.0, 100.0)], false).into();
        assert_eq!(
            one(&[
                open,
                line(100.0, 100.0, 0.0, 100.0),
                line(0.0, 0.0, 0.0, 100.0)
            ]),
            PolylineEntity::new(
                vec![p(0.0, 0.0), p(100.0, 0.0), p(100.0, 100.0), p(0.0, 100.0)],
                true
            )
            .into()
        );
    }

    #[test]
    fn lines_that_are_not_all_collinear_are_chained_instead() {
        assert_eq!(
            one(&[
                line(0.0, 0.0, 50.0, 0.0),
                line(50.0, 0.0, 100.0, 0.0),
                line(100.0, 0.0, 100.0, 50.0)
            ]),
            PolylineEntity::new(
                vec![p(0.0, 0.0), p(50.0, 0.0), p(100.0, 0.0), p(100.0, 50.0)],
                false
            )
            .into()
        );
    }

    #[test]
    fn what_meets_nothing_is_left_out_and_separate_chains_stay_separate() {
        let results = joined(&[
            line(0.0, 0.0, 10.0, 0.0),
            line(500.0, 500.0, 600.0, 500.0), // alone
            line(10.0, 0.0, 10.0, 10.0),
            CircleEntity::new(p(10.0, 10.0), 5.0).into(), // no ends
            arc(0.0, 1.0),
            line(50.0, 0.0, 80.0, -30.0),
        ])
        .unwrap();
        let members: Vec<&[usize]> = results.iter().map(|r| r.members.as_slice()).collect();
        assert_eq!(members, vec![&[0, 2][..], &[4, 5][..]]);
    }

    #[test]
    fn entities_that_cannot_be_joined_say_why() {
        assert_eq!(joined(&[line(0.0, 0.0, 10.0, 0.0)]), Err(JoinError::TooFew));
        assert_eq!(joined(&[]), Err(JoinError::TooFew));
        assert_eq!(
            joined(&[line(0.0, 0.0, 10.0, 0.0), line(0.0, 5.0, 10.0, 5.0)]),
            Err(JoinError::NothingMeets),
            "parallel, not collinear"
        );
        assert_eq!(
            joined(&[
                CircleEntity::new(p(0.0, 0.0), 5.0).into(),
                line(5.0, 0.0, 10.0, 0.0)
            ]),
            Err(JoinError::NothingMeets)
        );
        let closed: Entity =
            PolylineEntity::new(vec![p(0.0, 0.0), p(10.0, 0.0), p(10.0, 10.0)], true).into();
        assert_eq!(
            joined(&[closed, line(10.0, 0.0, 20.0, 0.0)]),
            Err(JoinError::NothingMeets)
        );
    }

    /// Regression (ADR-0012): found by the stress tests. Ten million units
    /// from the origin a point cannot be within 10⁻⁹ of a line it was
    /// computed on, so the halves of a broken line were "not collinear".
    #[test]
    fn a_line_broken_far_from_the_origin_joins_again() {
        let far: Entity = LineEntity::new(
            p(8_434_235.061_870_288, 0.0),
            p(8_434_195.097_190_29, 91.667_713_3),
        )
        .into();
        let Entity::Line(original) = &far else {
            unreachable!()
        };
        let at = p(8_434_218.9, 37.0);
        let pieces = far.broken(at, at).expect("a point in the middle");
        assert_eq!(pieces.len(), 2);
        assert_eq!(one(&pieces), far);
        // The same again further out still, with arcs.
        let arc: Entity = ArcEntity::new(p(3.0e9, -2.0e9), 50.0, 0.3, 2.1).into();
        let Entity::Arc(whole) = &arc else {
            unreachable!()
        };
        let middle = whole.arc().mid_point();
        let halves = arc.broken(middle, middle).expect("a point in the middle");
        let Entity::Arc(joined) = one(&halves) else {
            panic!("an arc");
        };
        assert!((joined.arc().sweep - whole.arc().sweep).abs() < 1e-9);
        assert_eq!(original.start, p(8_434_235.061_870_288, 0.0));
    }

    /// The acceptance criterion: ends match within the tolerance class.
    #[test]
    fn ends_are_matched_within_the_linear_tolerance() {
        let near = [line(0.0, 0.0, 100.0, 0.0), line(100.0, 5e-10, 100.0, 50.0)];
        assert!(joined(&near).is_ok());
        let far = [line(0.0, 0.0, 100.0, 0.0), line(100.0, 5e-8, 100.0, 50.0)];
        assert_eq!(joined(&far), Err(JoinError::NothingMeets));
    }
}

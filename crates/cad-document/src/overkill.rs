//! What `OVERKILL` removes and merges (#234): objects drawn on top of
//! each other.
//!
//! Among the objects given, those that compare equal — the same kind, the
//! same geometry within the linear tolerance, the same properties but for
//! those ignored — are duplicates, and the oldest stays. Lines are taken
//! further: collinear lines of the same properties that overlap or meet
//! are one line, the oldest made to cover them all
//! (`cad_geometry::collinear_runs`). Nothing is changed here: the plan is
//! applied in one transaction by whoever asked.

use std::collections::HashMap;

use cad_core::tolerance::{AngularTolerance, LinearTolerance};
use cad_core::{EntityId, Point2};
use cad_geometry::{Segment, collinear_runs};

use crate::{Color, Drawing, Entity, LayerId, LineEntity, Linetype, Lineweight, Properties};

/// Which properties two objects may differ in and still be one.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Ignore {
    pub layer: bool,
    pub color: bool,
    pub linetype: bool,
    pub lineweight: bool,
}

impl Ignore {
    /// The properties that are compared, the ignored ones made alike.
    fn key(self, properties: Properties) -> Properties {
        Properties {
            layer: if self.layer {
                LayerId::FIRST
            } else {
                properties.layer
            },
            color: if self.color {
                Color::ByLayer
            } else {
                properties.color
            },
            linetype: if self.linetype {
                Linetype::ByLayer
            } else {
                properties.linetype
            },
            lineweight: if self.lineweight {
                Lineweight::ByLayer
            } else {
                properties.lineweight
            },
        }
    }
}

/// What `OVERKILL` does to the objects it was given.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Plan {
    /// Duplicates, and lines merged into another.
    pub removed: Vec<EntityId>,
    /// Lines that now cover the ones merged into them.
    pub lengthened: Vec<(EntityId, LineEntity)>,
}

/// The plan for `ids`, objects of `drawing`.
pub fn overkill(drawing: &Drawing, ids: &[EntityId], ignore: Ignore) -> Plan {
    let mut ids = ids.to_vec();
    ids.sort_unstable();
    ids.dedup();
    let mut lines: HashMap<Properties, Vec<(EntityId, LineEntity)>> = HashMap::new();
    let mut others: HashMap<(Properties, &'static str), Vec<(EntityId, &Entity)>> = HashMap::new();
    for id in ids {
        let Some(record) = drawing.record(id) else {
            continue;
        };
        let key = ignore.key(record.properties);
        match &*record.entity {
            Entity::Line(line) => lines.entry(key).or_default().push((id, *line)),
            entity => others
                .entry((key, entity.kind_name()))
                .or_default()
                .push((id, entity)),
        }
    }
    let mut plan = Plan::default();
    for group in lines.into_values() {
        let segments: Vec<Segment> = group.iter().map(|(_, line)| line.segment()).collect();
        for run in collinear_runs(&segments) {
            let (kept, line) = group[run.members[0]];
            let covering = LineEntity::new(run.segment.start, run.segment.end);
            if covering != line {
                plan.lengthened.push((kept, covering));
            }
            plan.removed
                .extend(run.members[1..].iter().map(|member| group[*member].0));
        }
    }
    for group in others.into_values() {
        let mut gone = vec![false; group.len()];
        for i in 0..group.len() {
            if gone[i] {
                continue;
            }
            for j in i + 1..group.len() {
                if !gone[j] && same(group[i].1, group[j].1) {
                    gone[j] = true;
                    plan.removed.push(group[j].0);
                }
            }
        }
    }
    plan.removed.sort_unstable();
    plan.lengthened.sort_by_key(|(id, _)| *id);
    plan
}

/// Whether two objects of one kind are the same within the tolerances.
fn same(a: &Entity, b: &Entity) -> bool {
    let near = |p: Point2, q: Point2| LinearTolerance::DEFAULT.around(&[p, q]).coincident(p, q);
    let equal = |x: f64, y: f64| {
        LinearTolerance::DEFAULT
            .at_magnitude(x.abs().max(y.abs()))
            .eq(x, y)
    };
    let angle = |x: f64, y: f64| {
        let turn = (x - y).rem_euclid(std::f64::consts::TAU);
        AngularTolerance::DEFAULT.is_zero(turn.min(std::f64::consts::TAU - turn))
    };
    match (a, b) {
        (Entity::Circle(a), Entity::Circle(b)) => {
            near(a.center, b.center) && equal(a.radius, b.radius)
        }
        (Entity::Arc(a), Entity::Arc(b)) => {
            near(a.center, b.center)
                && equal(a.radius, b.radius)
                && angle(a.start_angle, b.start_angle)
                && angle(a.end_angle, b.end_angle)
        }
        (Entity::Point(a), Entity::Point(b)) => near(a.position, b.position),
        (Entity::Polyline(a), Entity::Polyline(b)) => {
            a.closed == b.closed
                && a.vertices.len() == b.vertices.len()
                && equal(a.width(), b.width())
                && a.vertices
                    .iter()
                    .zip(&b.vertices)
                    .all(|(p, q)| near(*p, *q))
                && a.bulges().zip(b.bulges()).all(|(x, y)| equal(x, y))
        }
        // Everything else — a text, a dimension, a hatch, a reference —
        // is one when it is the same exactly.
        (a, b) => a == b,
    }
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;
    use crate::CircleEntity;

    fn p(x: f64, y: f64) -> Point2 {
        Point2::new(x, y)
    }

    fn line(x0: f64, y0: f64, x1: f64, y1: f64) -> Entity {
        LineEntity::new(p(x0, y0), p(x1, y1)).into()
    }

    fn plan(entities: Vec<Entity>, ignore: Ignore) -> (Drawing, Plan) {
        let drawing = Drawing::from_entities(entities);
        let ids: Vec<EntityId> = drawing.entities().map(|(id, _)| id).collect();
        let plan = overkill(&drawing, &ids, ignore);
        (drawing, plan)
    }

    fn id(raw: u64) -> EntityId {
        EntityId::from_raw(raw)
    }

    #[test]
    fn duplicates_go_and_collinear_lines_become_one() {
        let (_, plan) = plan(
            vec![
                line(0.0, 0.0, 10.0, 0.0),
                line(5.0, 0.0, 20.0, 0.0),
                line(2.0, 0.0, 3.0, 0.0),
                CircleEntity::new(p(50.0, 50.0), 5.0).into(),
                CircleEntity::new(p(50.0, 50.0), 5.0).into(),
                CircleEntity::new(p(50.0, 50.0), 6.0).into(),
                line(0.0, 5.0, 10.0, 5.0),
            ],
            Ignore::default(),
        );
        assert_eq!(plan.removed, vec![id(2), id(3), id(5)]);
        assert_eq!(
            plan.lengthened,
            vec![(id(1), LineEntity::new(p(0.0, 0.0), p(20.0, 0.0)))]
        );
    }

    #[test]
    fn properties_keep_objects_apart_unless_ignored() {
        let mut drawing = Drawing::new();
        let mut tx = drawing.transaction();
        let red = Properties {
            color: Color::Index(1),
            ..Properties::default()
        };
        tx.insert_with(CircleEntity::new(p(0.0, 0.0), 5.0), Properties::default())
            .unwrap();
        tx.insert_with(CircleEntity::new(p(0.0, 0.0), 5.0), red)
            .unwrap();
        tx.commit();
        let ids: Vec<EntityId> = drawing.entities().map(|(id, _)| id).collect();
        assert!(
            overkill(&drawing, &ids, Ignore::default())
                .removed
                .is_empty()
        );
        let ignoring = Ignore {
            color: true,
            ..Ignore::default()
        };
        assert_eq!(overkill(&drawing, &ids, ignoring).removed, vec![ids[1]]);
    }

    proptest! {
        /// Run once, applied, run again: nothing more to do; and every
        /// point of every line before lies on a line after.
        #[test]
        fn overkill_is_done_in_one_pass_and_covers_what_was_there(
            spans in prop::collection::vec((0..6i32, 0..40i32, 1..20i32, any::<bool>()), 1..12),
        ) {
            let entities: Vec<Entity> = spans
                .iter()
                .map(|(row, start, length, reversed)| {
                    let (a, b) = (f64::from(*start), f64::from(start + length));
                    let y = f64::from(*row);
                    if *reversed { line(b, y, a, y) } else { line(a, y, b, y) }
                })
                .collect();
            let (mut drawing, first) = plan(entities.clone(), Ignore::default());
            let mut tx = drawing.transaction();
            for (id, line) in &first.lengthened {
                tx.replace(*id, *line).unwrap();
            }
            for id in &first.removed {
                tx.remove(*id).unwrap();
            }
            tx.commit();
            let ids: Vec<EntityId> = drawing.entities().map(|(id, _)| id).collect();
            prop_assert_eq!(overkill(&drawing, &ids, Ignore::default()), Plan::default());
            let after: Vec<Segment> = drawing
                .entities()
                .filter_map(|(_, entity)| entity.as_line().map(LineEntity::segment))
                .collect();
            for entity in &entities {
                let before = entity.as_line().unwrap().segment();
                for t in [0.0, 0.25, 0.5, 0.75, 1.0] {
                    let point = before.start + (before.end - before.start) * t;
                    prop_assert!(
                        after.iter().any(|segment| segment.distance_to_point(point) < 1e-9),
                        "{point:?} is no longer drawn"
                    );
                }
            }
        }
    }
}

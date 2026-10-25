//! Settling constraints inside a transaction (ADR-0038 §5).
//!
//! When a transaction commits, the constraints it reaches are solved over
//! the drawing as it will be: the entities it changed, the constraints it
//! added or gave new values, and everything joined to those by a
//! constraint. Unknowns the transaction set are held — the end a grip put
//! somewhere, the object a new constraint is drawn to — and the rest of
//! each constrained part follows, in the same transaction, so one edit is
//! one undo. An edit that cannot be settled is not applied, and says which
//! constraint it would break.
//!
//! The mapping between entities and the solver's unknowns is here too: a
//! line is its two ends, a circle its centre and radius, an arc its centre,
//! radius, and two ends kept on its circle, a polyline its vertices, a
//! point its position.

use std::collections::{BTreeMap, BTreeSet};

use cad_core::{EntityId, Point2};
use cad_geometry::constraint::{
    Assessment, CircleVar, Equation, LineVar, PointVar, System, Tangency, Tolerances,
};

use super::{Drawing, TransactionBuilder};
use crate::constraint::{Axis, Constraint, ConstraintId, Kind, Part, Touch};
use crate::transaction::{Change, EntityChange};
use crate::{ArcEntity, CircleEntity, Entity, LineEntity, PointEntity, PolylineEntity};

/// An entity's unknowns.
#[derive(Clone, Debug)]
enum Vars {
    Line {
        start: PointVar,
        end: PointVar,
    },
    Circle(CircleVar),
    Arc {
        circle: CircleVar,
        start: PointVar,
        end: PointVar,
    },
    Polyline(Vec<PointVar>),
    Point(PointVar),
}

/// The unknowns of `entity`, added to `system`; `None` for a kind that is
/// not constrained.
fn vars_of(system: &mut System, entity: &Entity) -> Option<Vars> {
    let point = |system: &mut System, p: Point2| system.point(p.x, p.y);
    Some(match entity {
        Entity::Line(line) => Vars::Line {
            start: point(system, line.start),
            end: point(system, line.end),
        },
        Entity::Circle(circle) => Vars::Circle(CircleVar {
            center: point(system, circle.center),
            radius: system.param(circle.radius),
        }),
        Entity::Arc(arc) => {
            let a = arc.arc();
            Vars::Arc {
                circle: CircleVar {
                    center: point(system, arc.center),
                    radius: system.param(arc.radius),
                },
                start: point(system, a.start_point()),
                end: point(system, a.end_point()),
            }
        }
        Entity::Polyline(polyline) => Vars::Polyline(
            polyline
                .vertices
                .iter()
                .map(|&v| point(system, v))
                .collect(),
        ),
        Entity::Point(p) => Vars::Point(point(system, p.position)),
        _ => return None,
    })
}

impl Vars {
    /// Every unknown, in a fixed order.
    fn params(&self) -> Vec<usize> {
        let p = |v: &PointVar| [v.x, v.y];
        match self {
            Self::Line { start, end } => [p(start), p(end)].concat(),
            Self::Circle(c) => vec![c.center.x, c.center.y, c.radius],
            Self::Arc { circle, start, end } => [
                &[circle.center.x, circle.center.y, circle.radius][..],
                &p(start),
                &p(end),
            ]
            .concat(),
            Self::Polyline(vertices) => vertices.iter().flat_map(p).collect(),
            Self::Point(v) => p(v).to_vec(),
        }
    }

    fn point(&self, part: Part) -> Option<PointVar> {
        match (self, part) {
            (Self::Line { start, .. } | Self::Arc { start, .. }, Part::Start) => Some(*start),
            (Self::Line { end, .. } | Self::Arc { end, .. }, Part::End) => Some(*end),
            (Self::Circle(c) | Self::Arc { circle: c, .. }, Part::Center) => Some(c.center),
            (Self::Polyline(vertices), Part::Vertex(n)) => vertices.get(n as usize).copied(),
            (Self::Point(p), Part::Point) => Some(*p),
            _ => None,
        }
    }

    fn line(&self, part: Part) -> Option<LineVar> {
        match (self, part) {
            (Self::Line { start, end }, Part::Edge) => Some(LineVar { a: *start, b: *end }),
            (Self::Polyline(vertices), Part::Segment(n)) => {
                let n = n as usize;
                let a = *vertices.get(n)?;
                let b = *vertices.get((n + 1) % vertices.len())?;
                Some(LineVar { a, b })
            }
            _ => None,
        }
    }

    fn circle(&self, part: Part) -> Option<CircleVar> {
        match (self, part) {
            (Self::Circle(c) | Self::Arc { circle: c, .. }, Part::Edge) => Some(*c),
            _ => None,
        }
    }

    /// Equations that keep the entity itself whole: an arc's ends on its
    /// circle.
    fn own_equations(&self) -> Vec<Equation> {
        match self {
            Self::Arc { circle, start, end } => vec![
                Equation::PointOnCircle {
                    point: *start,
                    circle: *circle,
                },
                Equation::PointOnCircle {
                    point: *end,
                    circle: *circle,
                },
            ],
            _ => Vec::new(),
        }
    }

    /// `entity` with its geometry read back from `values`.
    fn write_back(&self, entity: &Entity, values: &[f64]) -> Entity {
        let at = |p: &PointVar| Point2::new(values[p.x], values[p.y]);
        match (self, entity) {
            (Self::Line { start, end }, _) => Entity::Line(LineEntity::new(at(start), at(end))),
            (Self::Circle(c), _) => {
                Entity::Circle(CircleEntity::new(at(&c.center), values[c.radius]))
            }
            (Self::Arc { circle, start, end }, _) => {
                let center = at(&circle.center);
                let angle = |p: Point2| (p.y - center.y).atan2(p.x - center.x);
                Entity::Arc(ArcEntity::new(
                    center,
                    values[circle.radius],
                    angle(at(start)),
                    angle(at(end)),
                ))
            }
            (Self::Polyline(vertices), Entity::Polyline(polyline)) => {
                let moved = vertices.iter().map(at).collect();
                Entity::Polyline(
                    PolylineEntity::with_bulges(
                        moved,
                        polyline.bulges().collect(),
                        polyline.closed,
                    )
                    .with_width(polyline.width()),
                )
            }
            (Self::Point(p), Entity::Point(_)) => Entity::Point(PointEntity::new(at(p))),
            _ => entity.clone(),
        }
    }
}

/// The equations of `constraint`, over the unknowns of its entities.
fn equations_of(constraint: &Constraint, vars: &BTreeMap<EntityId, Vars>) -> Option<Vec<Equation>> {
    let of = |i: usize| -> Option<(&Vars, Part)> {
        let reference = constraint.references.get(i)?;
        Some((vars.get(&reference.entity)?, reference.part))
    };
    let point = |i| of(i).and_then(|(v, part)| v.point(part));
    let line = |i| of(i).and_then(|(v, part)| v.line(part));
    let circle = |i| of(i).and_then(|(v, part)| v.circle(part));
    let coincide = |p: PointVar, q: PointVar| {
        vec![
            Equation::Equal { a: p.x, b: q.x },
            Equation::Equal { a: p.y, b: q.y },
        ]
    };
    Some(match &constraint.kind {
        Kind::Coincident => match (point(0), point(1)) {
            (Some(p), Some(q)) => coincide(p, q),
            (Some(p), None) | (None, Some(p)) => {
                let other = usize::from(point(0).is_some());
                match (line(other), circle(other)) {
                    (Some(l), _) => vec![Equation::PointOnLine { point: p, line: l }],
                    (_, Some(c)) => vec![Equation::PointOnCircle {
                        point: p,
                        circle: c,
                    }],
                    _ => return None,
                }
            }
            _ => return None,
        },
        Kind::Horizontal | Kind::Vertical => {
            let (p, q) = match line(0) {
                Some(l) => (l.a, l.b),
                None => (point(0)?, point(1)?),
            };
            if matches!(constraint.kind, Kind::Horizontal) {
                vec![Equation::Equal { a: p.y, b: q.y }]
            } else {
                vec![Equation::Equal { a: p.x, b: q.x }]
            }
        }
        Kind::Parallel => vec![Equation::Parallel {
            a: line(0)?,
            b: line(1)?,
        }],
        Kind::Perpendicular => vec![Equation::Perpendicular {
            a: line(0)?,
            b: line(1)?,
        }],
        Kind::Collinear => {
            let (a, b) = (line(0)?, line(1)?);
            vec![
                Equation::Parallel { a, b },
                Equation::PointOnLine {
                    point: b.a,
                    line: a,
                },
            ]
        }
        Kind::Concentric => coincide(circle(0)?.center, circle(1)?.center),
        Kind::Equal => match (line(0), line(1)) {
            (Some(a), Some(b)) => vec![Equation::EqualLength { a, b }],
            _ => vec![Equation::Equal {
                a: circle(0)?.radius,
                b: circle(1)?.radius,
            }],
        },
        Kind::Tangent { touch } => match (line(0), line(1), circle(0), circle(1)) {
            (Some(l), _, _, Some(c)) | (_, Some(l), Some(c), _) => vec![Equation::LineTangent {
                line: l,
                circle: c,
                side: if matches!(touch, Touch::Right) {
                    -1.0
                } else {
                    1.0
                },
            }],
            (_, _, Some(a), Some(b)) => vec![Equation::CircleTangent {
                a,
                b,
                tangency: match touch {
                    Touch::FirstInside => Tangency::FirstInside,
                    Touch::SecondInside => Tangency::SecondInside,
                    _ => Tangency::Outside,
                },
            }],
            _ => return None,
        },
        Kind::Symmetric => {
            let (a, b, l) = (point(0)?, point(1)?, line(2)?);
            vec![
                Equation::MidpointOnLine { a, b, line: l },
                Equation::PerpendicularTo { a, b, line: l },
            ]
        }
        Kind::Fix { at } => {
            let p = point(0)?;
            vec![
                Equation::Fixed {
                    param: p.x,
                    value: at.x,
                },
                Equation::Fixed {
                    param: p.y,
                    value: at.y,
                },
            ]
        }
        Kind::Distance { axis, value } => {
            let (p, q) = (point(0)?, point(1)?);
            vec![match axis {
                Axis::Horizontal => Equation::Offset {
                    from: p.x,
                    to: q.x,
                    value: *value,
                },
                Axis::Vertical => Equation::Offset {
                    from: p.y,
                    to: q.y,
                    value: *value,
                },
                Axis::Aligned => Equation::Distance {
                    a: p,
                    b: q,
                    value: *value,
                },
            }]
        }
        Kind::Angle { value } => vec![Equation::Angle {
            a: line(0)?,
            b: line(1)?,
            value: *value,
        }],
        Kind::Radius { value } => vec![Equation::Fixed {
            param: circle(0)?.radius,
            value: *value,
        }],
        Kind::Diameter { value } => vec![Equation::Fixed {
            param: circle(0)?.radius,
            value: value / 2.0,
        }],
    })
}

/// How a constraint is named when an edit would break it.
pub(crate) fn describe(constraint: &Constraint) -> String {
    match &constraint.kind {
        Kind::Angle { value } => format!("{} = {:.4}°", constraint.name, value.to_degrees()),
        kind if kind.is_dimensional() => {
            format!(
                "{} = {:.4}",
                constraint.name,
                kind.value().unwrap_or_default()
            )
        }
        kind => {
            let name = format!("{kind:?}");
            let word = name.split([' ', '{']).next().unwrap_or("").to_lowercase();
            format!("the {word} constraint")
        }
    }
}

/// The constraints a set of entities reaches, and the entities those
/// reach, until nothing more is joined; in ID order.
type Reach = (BTreeSet<EntityId>, Vec<(ConstraintId, Constraint)>);

/// A constrained part as the solver sees it: the system, each entity's
/// unknowns, and which constraint each equation is of.
pub(crate) struct Constrained {
    pub system: System,
    vars: BTreeMap<EntityId, Vars>,
    owners: Vec<Option<ConstraintId>>,
    constraints: BTreeMap<ConstraintId, Constraint>,
}

/// The drawing as it will be: some entities replaced, made, or gone, and
/// the constraint rows as they will be. A transaction's, before it
/// commits; a grip's preview's, while it is dragged.
pub(crate) struct Prospect<'a> {
    drawing: &'a Drawing,
    /// Entities other than the drawing's: replaced or made (`Some`), or
    /// gone (`None`).
    entities: BTreeMap<EntityId, Option<Entity>>,
    constraints: Vec<(ConstraintId, Constraint)>,
}

impl<'a> Prospect<'a> {
    /// The drawing with `entities` in place of its own.
    pub(crate) fn of(drawing: &'a Drawing, entities: BTreeMap<EntityId, Option<Entity>>) -> Self {
        Self {
            drawing,
            entities,
            constraints: drawing
                .constraints
                .iter()
                .map(|(id, c)| (id, c.clone()))
                .collect(),
        }
    }

    fn entity(&self, id: EntityId) -> Option<&Entity> {
        match self.entities.get(&id) {
            Some(entity) => entity.as_ref(),
            None => self.drawing.entity(id),
        }
    }

    /// The constraints `entities` reach, and their entities.
    fn reach(&self, mut entities: BTreeSet<EntityId>) -> Reach {
        let mut taken: BTreeSet<ConstraintId> = BTreeSet::new();
        loop {
            let mut grew = false;
            for (id, constraint) in &self.constraints {
                if !taken.contains(id)
                    && constraint
                        .references
                        .iter()
                        .any(|r| entities.contains(&r.entity))
                {
                    taken.insert(*id);
                    for r in &constraint.references {
                        grew |= entities.insert(r.entity);
                    }
                }
            }
            if !grew {
                break;
            }
        }
        let constraints = self
            .constraints
            .iter()
            .filter(|(id, _)| taken.contains(id))
            .cloned()
            .collect();
        (entities, constraints)
    }

    /// The solver's view of `reach`, at the entities as they will be.
    fn part(&self, (entities, constraints): Reach) -> Constrained {
        let mut system = System::default();
        let mut vars = BTreeMap::new();
        for id in entities {
            if let Some(entity) = self.entity(id)
                && let Some(v) = vars_of(&mut system, entity)
            {
                vars.insert(id, v);
            }
        }
        let mut owners = Vec::new();
        for v in vars.values() {
            for equation in v.own_equations() {
                system.equations.push(equation);
                owners.push(None);
            }
        }
        for (id, constraint) in &constraints {
            for equation in equations_of(constraint, &vars).unwrap_or_default() {
                system.equations.push(equation);
                owners.push(Some(*id));
            }
        }
        Constrained {
            system,
            vars,
            owners,
            constraints: constraints.into_iter().collect(),
        }
    }

    /// Settles what `triggers` reach: the unknowns of `moved` entities
    /// that differ from the drawing's are held, and every unknown of
    /// `held` ones. The other entities that move, with their new geometry;
    /// or why it cannot be settled.
    fn settle(
        &self,
        triggers: BTreeSet<EntityId>,
        moved: &BTreeSet<EntityId>,
        held: &BTreeSet<EntityId>,
    ) -> Result<Vec<(EntityId, Entity)>, String> {
        let reach = self.reach(triggers);
        if reach.1.is_empty() {
            return Ok(Vec::new());
        }
        let mut part = self.part(reach);
        for (id, v) in &part.vars {
            let params = v.params();
            if held.contains(id) {
                for &p in &params {
                    part.system.hold(p);
                }
            } else if moved.contains(id) {
                // The entity as it was, read the same way: an unknown
                // whose value differs is one the edit set.
                let mut was = System::default();
                let before: Vec<f64> = self
                    .drawing
                    .entity(*id)
                    .and_then(|entity| vars_of(&mut was, entity))
                    .map(|b| b.params().iter().map(|&q| was.values[q]).collect())
                    .unwrap_or_default();
                for (i, &p) in params.iter().enumerate() {
                    // Exactly: the same bits are an unknown the edit left alone.
                    if before.len() != params.len()
                        || before[i].to_bits() != part.system.values[p].to_bits()
                    {
                        part.system.hold(p);
                    }
                }
            }
        }
        let solution = cad_geometry::constraint::solve(&part.system, Tolerances::default());
        if !solution.is_solved() {
            // The constraints most broken at the solver's best compromise,
            // up to three.
            let mut blamed: Vec<ConstraintId> = Vec::new();
            for id in solution.unsatisfied.iter().filter_map(|&i| part.owners[i]) {
                if !blamed.contains(&id) && blamed.len() < 3 {
                    blamed.push(id);
                }
            }
            let names: Vec<String> = blamed
                .iter()
                .filter_map(|id| part.constraints.get(id))
                .map(describe)
                .collect();
            return Err(match names.as_slice() {
                [] => "The edit cannot keep the constraints.".to_owned(),
                [one] => format!("The edit would break {one}."),
                [first @ .., last] => {
                    format!("The edit would break {} and {last}.", first.join(", "))
                }
            });
        }
        Ok(part
            .vars
            .iter()
            .filter(|(_, v)| {
                v.params()
                    .iter()
                    .any(|&p| solution.values[p].to_bits() != part.system.values[p].to_bits())
            })
            .filter_map(|(id, v)| {
                let entity = self.entity(*id)?;
                Some((*id, v.write_back(entity, &solution.values)))
            })
            .collect())
    }
}

impl Drawing {
    /// What the rest of the constrained parts would become were `replaced`
    /// put in place (ADR-0038 §5): the other entities that would move, with
    /// their geometry — what a grip's preview draws as it is dragged — or
    /// why the edit would be refused.
    pub fn settled_with(
        &self,
        replaced: &[(EntityId, Entity)],
    ) -> Result<Vec<(EntityId, Entity)>, String> {
        if self.constraints.is_empty() {
            return Ok(Vec::new());
        }
        let entities = replaced
            .iter()
            .map(|(id, entity)| (*id, Some(entity.clone())))
            .collect();
        let ids: BTreeSet<EntityId> = replaced.iter().map(|(id, _)| *id).collect();
        Prospect::of(self, entities).settle(ids.clone(), &ids, &BTreeSet::new())
    }

    /// The constraints that do not hold as the drawing is, in ID order:
    /// none, when every edit went through a transaction (ADR-0038). A
    /// drawing read from a file may hold some.
    pub fn unsatisfied_constraints(&self) -> Vec<ConstraintId> {
        let prospect = Prospect::of(self, BTreeMap::new());
        let mut unsatisfied = Vec::new();
        for (id, constraint) in &prospect.constraints {
            let entities = constraint.references.iter().map(|r| r.entity).collect();
            let mut part = prospect.part((entities, vec![(*id, constraint.clone())]));
            // Every unknown held: solving only checks.
            for p in 0..part.system.values.len() {
                part.system.hold(p);
            }
            let fits = equations_of(constraint, &part.vars).is_some();
            if !fits
                || !cad_geometry::constraint::solve(&part.system, Tolerances::default()).is_solved()
            {
                unsatisfied.push(*id);
            }
        }
        unsatisfied
    }
}

impl TransactionBuilder<'_> {
    /// The drawing as this transaction leaves it.
    fn prospect(&self) -> Prospect<'_> {
        let entities = self
            .touched
            .iter()
            .map(|&id| {
                (
                    id,
                    self.record_now(id).map(|record| (*record.entity).clone()),
                )
            })
            .collect();
        Prospect {
            drawing: self.drawing,
            entities,
            constraints: self.constraints_now(),
        }
    }

    /// What adding `constraint` would do (ADR-0038 §4): fix something the
    /// constraints it reaches leave free, add nothing (redundant), or
    /// contradict them — judged with the drawing as this transaction
    /// leaves it. `None` when its parts cannot be read.
    pub fn assess(&self, constraint: &Constraint) -> Option<Assessment> {
        let entities = constraint.references.iter().map(|r| r.entity).collect();
        let prospect = self.prospect();
        let part = prospect.part(prospect.reach(entities));
        let equations = equations_of(constraint, &part.vars)?;
        Some(cad_geometry::constraint::assess(
            &part.system,
            &equations,
            Tolerances::default(),
        ))
    }

    /// Holds every unknown of `id` when the transaction settles: a
    /// constraint drawn to it moves the other object, as AutoCAD's does.
    pub fn hold(&mut self, id: EntityId) {
        self.held.insert(id);
    }

    /// Settles what this transaction reaches; why not, when it cannot be.
    pub(super) fn settle(&mut self) -> Result<(), String> {
        let constraint_changes: Vec<&Change> = self
            .others()
            .filter(|change| matches!(change, Change::Constraint(_)))
            .collect();
        if self.drawing.constraints.is_empty() && constraint_changes.is_empty() {
            return Ok(());
        }
        // What triggers a solve: entities whose geometry this transaction
        // changed or made, and constraints it added or gave a new value.
        let mut triggers: BTreeSet<EntityId> = BTreeSet::new();
        let mut moved: BTreeSet<EntityId> = BTreeSet::new();
        for &id in &self.touched {
            let before = self.drawing.entities.record(id);
            let after = self.record_now(id);
            match (before, after) {
                (Some(before), Some(after)) if before.entity != after.entity => {
                    triggers.insert(id);
                    moved.insert(id);
                }
                (None, Some(_)) => {
                    triggers.insert(id);
                }
                _ => {}
            }
        }
        for change in constraint_changes {
            if let Change::Constraint(change) = change {
                use crate::table::TableChange;
                match change {
                    TableChange::Insert { row, .. } => {
                        triggers.extend(row.references.iter().map(|r| r.entity));
                    }
                    TableChange::Replace { before, after, .. } if before != after => {
                        triggers.extend(after.references.iter().map(|r| r.entity));
                    }
                    _ => {}
                }
            }
        }
        let settled = self.prospect().settle(triggers, &moved, &self.held)?;
        for (id, entity) in settled {
            self.settle_entity(id, entity);
        }
        Ok(())
    }

    /// Puts `entity` as `id`'s geometry: into this transaction's change of
    /// it, or as a new replacement.
    fn settle_entity(&mut self, id: EntityId, entity: Entity) {
        if self.touched.contains(&id) {
            for change in self.changes.iter_mut().rev() {
                match change {
                    Change::Entity(EntityChange::Insert { id: at, record })
                    | Change::Entity(EntityChange::Replace {
                        id: at,
                        after: record,
                        ..
                    }) if *at == id => {
                        *record = record.with_entity(entity);
                        return;
                    }
                    _ => {}
                }
            }
        } else if let Some(before) = self.drawing.entities.record(id).cloned() {
            let after = before.with_entity(entity);
            self.push(EntityChange::Replace { id, before, after });
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::float_cmp)]

    use cad_core::{EntityId, Point2};

    use crate::constraint::{Axis, Constraint, Kind, Part, Reference};
    use crate::{ArcEntity, Drawing, Entity, LineEntity, PolylineEntity};

    fn near(a: Point2, b: Point2) -> bool {
        a.distance_to(b) < 1e-7
    }

    fn line_of(drawing: &Drawing, id: EntityId) -> LineEntity {
        *drawing
            .entity(id)
            .and_then(Entity::as_line)
            .expect("a line")
    }

    fn at(id: EntityId, part: Part) -> Reference {
        Reference::new(id, part)
    }

    fn add(drawing: &mut Drawing, line: LineEntity) -> EntityId {
        let mut tx = drawing.transaction();
        let id = tx.insert(line).unwrap();
        tx.commit();
        id
    }

    /// Two lines meeting at a corner: their shared end coincident, the
    /// first fixed at its start, horizontal, and 100 long; the second
    /// vertical.
    fn corner() -> (Drawing, EntityId, EntityId) {
        let mut drawing = Drawing::new();
        let a = add(
            &mut drawing,
            LineEntity::new(Point2::ORIGIN, Point2::new(100.0, 0.0)),
        );
        let b = add(
            &mut drawing,
            LineEntity::new(Point2::new(100.0, 0.0), Point2::new(100.0, 50.0)),
        );
        let mut tx = drawing.transaction();
        for constraint in [
            Constraint::geometric(Kind::Coincident, vec![at(a, Part::End), at(b, Part::Start)]),
            Constraint::geometric(Kind::Fix { at: Point2::ORIGIN }, vec![at(a, Part::Start)]),
            Constraint::geometric(Kind::Horizontal, vec![at(a, Part::Edge)]),
            Constraint::geometric(Kind::Vertical, vec![at(b, Part::Edge)]),
            Constraint::dimensional(
                "d1",
                Kind::Distance {
                    axis: Axis::Horizontal,
                    value: 100.0,
                },
                vec![at(a, Part::Start), at(a, Part::End)],
            ),
        ] {
            tx.add_constraint(constraint).unwrap();
        }
        assert_eq!(tx.commit().refused, None);
        (drawing, a, b)
    }

    #[test]
    fn a_new_constraint_moves_the_geometry_in_the_same_undo() {
        let mut drawing = Drawing::new();
        let slanted = add(
            &mut drawing,
            LineEntity::new(Point2::ORIGIN, Point2::new(100.0, 10.0)),
        );
        let revision = drawing.revision();
        let mut tx = drawing.transaction();
        tx.add_constraint(Constraint::geometric(
            Kind::Horizontal,
            vec![at(slanted, Part::Edge)],
        ))
        .unwrap();
        tx.commit();
        let line = line_of(&drawing, slanted);
        assert!((line.start.y - line.end.y).abs() < 1e-9, "{line:?}");
        // The nearest level line: both ends moved half the rise.
        assert!((line.start.y - 5.0).abs() < 1e-7, "{line:?}");
        // One undo takes the constraint and the move back together.
        drawing.undo();
        assert_eq!(line_of(&drawing, slanted).end, Point2::new(100.0, 10.0));
        assert!(drawing.constraints().is_empty());
        assert!(drawing.revision() != revision);
    }

    #[test]
    fn a_held_object_stays_and_the_other_comes_to_it() {
        let mut drawing = Drawing::new();
        let a = add(
            &mut drawing,
            LineEntity::new(Point2::ORIGIN, Point2::new(100.0, 0.0)),
        );
        let b = add(
            &mut drawing,
            LineEntity::new(Point2::new(110.0, 5.0), Point2::new(110.0, 60.0)),
        );
        let mut tx = drawing.transaction();
        tx.add_constraint(Constraint::geometric(
            Kind::Coincident,
            vec![at(a, Part::End), at(b, Part::Start)],
        ))
        .unwrap();
        tx.hold(a);
        tx.commit();
        assert_eq!(line_of(&drawing, a).end, Point2::new(100.0, 0.0));
        assert!(near(line_of(&drawing, b).start, Point2::new(100.0, 0.0)));
        assert_eq!(
            line_of(&drawing, b).end,
            Point2::new(110.0, 60.0),
            "the other end is free"
        );
    }

    #[test]
    fn a_dimension_given_a_new_value_moves_what_it_measures() {
        let (mut drawing, a, b) = corner();
        let d1 = drawing.constraints().find("d1").unwrap();
        let mut wider = drawing.constraints().get(d1).unwrap().clone();
        wider.kind = wider.kind.with_value(140.0);
        let mut tx = drawing.transaction();
        tx.replace_constraint(d1, wider).unwrap();
        tx.commit();
        assert!(near(line_of(&drawing, a).end, Point2::new(140.0, 0.0)));
        // The vertical line came along, its corner and its plumb kept.
        let b_line = line_of(&drawing, b);
        assert!(near(b_line.start, Point2::new(140.0, 0.0)));
        assert!((b_line.end.x - 140.0).abs() < 1e-7);
    }

    #[test]
    fn an_edit_moves_the_rest_of_its_part_and_one_that_breaks_it_is_refused() {
        let (mut drawing, a, b) = corner();
        // The second line's top dragged higher: nothing holds its height,
        // and its foot is the first line's end, which d1 and the fix keep.
        let mut tx = drawing.transaction();
        tx.replace(
            b,
            LineEntity::new(Point2::new(100.0, 0.0), Point2::new(100.0, 80.0)),
        )
        .unwrap();
        assert_eq!(tx.commit().refused, None);
        assert_eq!(line_of(&drawing, b).end, Point2::new(100.0, 80.0));

        // The first line's end pulled to 120: d1 says 100, and the start
        // is fixed. Refused, and nothing changes.
        let revision = drawing.revision();
        let mut tx = drawing.transaction();
        tx.replace(a, LineEntity::new(Point2::ORIGIN, Point2::new(120.0, 0.0)))
            .unwrap();
        let report = tx.commit();
        assert_eq!(
            report.refused.as_deref(),
            Some("The edit would break d1 = 100.0000 and the fix constraint.")
        );
        assert_eq!(drawing.revision(), revision);
        assert_eq!(line_of(&drawing, a).end, Point2::new(100.0, 0.0));
        assert_eq!(
            drawing.take_refusals(),
            ["The edit would break d1 = 100.0000 and the fix constraint."]
        );
        assert!(drawing.take_refusals().is_empty());

        // Moving both lines together breaks nothing but the fix: refused.
        let up = cad_core::Vec2::new(0.0, 5.0);
        let lines = [line_of(&drawing, a), line_of(&drawing, b)];
        let mut tx = drawing.transaction();
        for (id, line) in [a, b].into_iter().zip(lines) {
            tx.replace(id, LineEntity::new(line.start + up, line.end + up))
                .unwrap();
        }
        assert!(tx.commit().refused.is_some_and(|why| why.contains("fix")));
    }

    #[test]
    fn arcs_keep_their_ends_on_their_circle_and_polylines_their_bulges() {
        let mut drawing = Drawing::new();
        let mut tx = drawing.transaction();
        let arc = tx
            .insert(ArcEntity::new(
                Point2::ORIGIN,
                10.0,
                0.0,
                std::f64::consts::FRAC_PI_2,
            ))
            .unwrap();
        let polyline = tx
            .insert(PolylineEntity::with_bulges(
                vec![
                    Point2::new(0.0, 50.0),
                    Point2::new(40.0, 53.0),
                    Point2::new(80.0, 53.0),
                ],
                vec![0.0, 0.5, 0.0],
                false,
            ))
            .unwrap();
        tx.commit();
        let mut tx = drawing.transaction();
        tx.add_constraint(Constraint::dimensional(
            "r",
            Kind::Radius { value: 15.0 },
            vec![at(arc, Part::Edge)],
        ))
        .unwrap();
        tx.add_constraint(Constraint::geometric(
            Kind::Horizontal,
            vec![at(polyline, Part::Segment(0))],
        ))
        .unwrap();
        assert_eq!(tx.commit().refused, None);
        let Some(Entity::Arc(arc)) = drawing.entity(arc) else {
            panic!()
        };
        assert!((arc.radius - 15.0).abs() < 1e-9);
        assert!(arc.center.distance_to(Point2::ORIGIN) < 10.0, "{arc:?}");
        let Some(Entity::Polyline(polyline)) = drawing.entity(polyline) else {
            panic!()
        };
        assert!((polyline.vertices[0].y - polyline.vertices[1].y).abs() < 1e-9);
        assert_eq!(polyline.bulges().collect::<Vec<_>>(), [0.0, 0.5, 0.0]);
    }

    #[test]
    fn an_unconstrained_drawing_is_not_touched() {
        let mut drawing = Drawing::new();
        let a = add(
            &mut drawing,
            LineEntity::new(Point2::ORIGIN, Point2::new(1.0, 1.0)),
        );
        let mut tx = drawing.transaction();
        tx.replace(a, LineEntity::new(Point2::ORIGIN, Point2::new(2.0, 3.0)))
            .unwrap();
        let report = tx.commit();
        assert_eq!(report.refused, None);
        assert_eq!(report.replaced, [a]);
        assert_eq!(line_of(&drawing, a).end, Point2::new(2.0, 3.0));
    }
}

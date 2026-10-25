//! Constraints (ADR-0038, #266): geometric and dimensional relations between
//! points and edges of entities, kept as rows of the drawing's constraint
//! table and changed in the same transactions and history as everything
//! else.
//!
//! A constraint refers to entities by ID and to a part of each — an end, a
//! centre, a vertex, an edge — and holds what it needs to be met: a
//! dimension's value, a fixed point's place, which side a tangent touches
//! on. It holds no geometry of its own. An entity that is removed takes its
//! constraints with it, in the same transaction; one replaced by geometry
//! that no longer has a part a constraint names loses that constraint.
//!
//! Like the group table, the constraint table has no permanent first row.

use cad_core::{EntityId, Point2};

use crate::entity::Entity;
use crate::names::Named;
use crate::table::{Table, TableChange, TableId};

/// A part of an entity a constraint refers to.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Part {
    /// A line's or an arc's first end.
    Start,
    /// A line's or an arc's last end.
    End,
    /// A circle's or an arc's centre.
    Center,
    /// A polyline's vertex, by index.
    Vertex(u32),
    /// A point entity's position.
    Point,
    /// The whole curve: a line, a circle, an arc.
    Edge,
    /// A straight segment of a polyline, by the index of its first vertex.
    Segment(u32),
}

/// What a part of an entity is, for a constraint.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shape {
    /// A point: an end, a centre, a vertex, a point.
    Point,
    /// A straight edge: a line, a polyline's straight segment.
    Straight,
    /// A circular edge: a circle, an arc.
    Circular,
}

impl Part {
    /// What this part of `entity` is, or `None` when the entity has no
    /// such part or is not of a kind that is constrained.
    pub fn shape_on(self, entity: &Entity) -> Option<Shape> {
        match (entity, self) {
            (Entity::Line(_), Part::Start | Part::End) => Some(Shape::Point),
            (Entity::Line(_), Part::Edge) => Some(Shape::Straight),
            (Entity::Circle(_), Part::Center) => Some(Shape::Point),
            (Entity::Circle(_), Part::Edge) => Some(Shape::Circular),
            (Entity::Arc(_), Part::Start | Part::End | Part::Center) => Some(Shape::Point),
            (Entity::Arc(_), Part::Edge) => Some(Shape::Circular),
            (Entity::Point(_), Part::Point) => Some(Shape::Point),
            (Entity::Polyline(polyline), Part::Vertex(n)) => {
                ((n as usize) < polyline.vertices.len()).then_some(Shape::Point)
            }
            (Entity::Polyline(polyline), Part::Segment(n)) => {
                let n = n as usize;
                let count = polyline.vertices.len();
                let segments = if polyline.closed {
                    count
                } else {
                    count.saturating_sub(1)
                };
                // An arc segment is constrained by its vertices only.
                (n < segments && polyline.bulge(n) == 0.0).then_some(Shape::Straight)
            }
            _ => None,
        }
    }
}

impl Part {
    /// Where this point part of `entity` is; `None` for an edge, or a part
    /// the entity does not have.
    pub fn position_on(self, entity: &Entity) -> Option<Point2> {
        match (entity, self) {
            (Entity::Line(line), Part::Start) => Some(line.start),
            (Entity::Line(line), Part::End) => Some(line.end),
            (Entity::Circle(circle), Part::Center) => Some(circle.center),
            (Entity::Arc(arc), Part::Center) => Some(arc.center),
            (Entity::Arc(arc), Part::Start) => Some(arc.arc().start_point()),
            (Entity::Arc(arc), Part::End) => Some(arc.arc().end_point()),
            (Entity::Polyline(polyline), Part::Vertex(n)) => {
                polyline.vertices.get(n as usize).copied()
            }
            (Entity::Point(point), Part::Point) => Some(point.position),
            _ => None,
        }
    }

    /// Every point part `entity` has, in a fixed order.
    pub fn points_of(entity: &Entity) -> Vec<Part> {
        match entity {
            Entity::Line(_) => vec![Part::Start, Part::End],
            Entity::Circle(_) => vec![Part::Center],
            Entity::Arc(_) => vec![Part::Start, Part::End, Part::Center],
            Entity::Polyline(polyline) => (0..polyline.vertices.len() as u32)
                .map(Part::Vertex)
                .collect(),
            Entity::Point(_) => vec![Part::Point],
            _ => Vec::new(),
        }
    }
}

/// A part of one entity.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Reference {
    pub entity: EntityId,
    pub part: Part,
}

impl Reference {
    pub const fn new(entity: EntityId, part: Part) -> Self {
        Self { entity, part }
    }
}

/// Which way a linear dimensional constraint measures.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Axis {
    /// Along x, from the first point to the second; signed.
    Horizontal,
    /// Along y, from the first point to the second; signed.
    Vertical,
    /// Point to point; positive.
    Aligned,
}

/// How a tangent touches, found when the constraint is made and kept.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Touch {
    /// A line and a circle, the circle's centre left of the line's
    /// direction.
    Left,
    /// The same, the centre on the right.
    Right,
    /// Two circles, each outside the other.
    Outside,
    /// Two circles, the first inside the second.
    FirstInside,
    /// Two circles, the second inside the first.
    SecondInside,
}

impl Touch {
    /// How two edges touch, as they lie: the side of a line a circle is
    /// on; two circles outside or one inside the other. `None` unless one
    /// is straight and the other circular, or both circular.
    pub fn between(a: (&Entity, Part), b: (&Entity, Part)) -> Option<Touch> {
        let circle = |(entity, _): (&Entity, Part)| -> Option<(Point2, f64)> {
            match entity {
                Entity::Circle(c) => Some((c.center, c.radius)),
                Entity::Arc(a) => Some((a.center, a.radius)),
                _ => None,
            }
        };
        let line = |(entity, part): (&Entity, Part)| -> Option<(Point2, Point2)> {
            match (entity, part) {
                (Entity::Line(l), _) => Some((l.start, l.end)),
                (Entity::Polyline(p), Part::Segment(n)) => {
                    let n = n as usize;
                    Some((p.vertices[n], p.vertices[(n + 1) % p.vertices.len()]))
                }
                _ => None,
            }
        };
        let shape = |(entity, part): (&Entity, Part)| part.shape_on(entity);
        match (shape(a)?, shape(b)?) {
            (Shape::Straight, Shape::Circular) | (Shape::Circular, Shape::Straight) => {
                let (l, c) = if shape(a)? == Shape::Straight {
                    (a, b)
                } else {
                    (b, a)
                };
                let (start, end) = line(l)?;
                let (center, _) = circle(c)?;
                let d = end - start;
                let side = d.x * (center.y - start.y) - d.y * (center.x - start.x);
                Some(if side >= 0.0 {
                    Touch::Left
                } else {
                    Touch::Right
                })
            }
            (Shape::Circular, Shape::Circular) => {
                let ((ca, ra), (cb, rb)) = (circle(a)?, circle(b)?);
                let apart = ca.distance_to(cb);
                Some(
                    if (apart - (ra + rb)).abs() <= (apart - (ra - rb).abs()).abs() {
                        Touch::Outside
                    } else if ra < rb {
                        Touch::FirstInside
                    } else {
                        Touch::SecondInside
                    },
                )
            }
            _ => None,
        }
    }
}

/// What a constraint holds, and what it needs to be met.
#[derive(Clone, Debug, PartialEq)]
pub enum Kind {
    /// Two points at one place, or a point on an edge.
    Coincident,
    /// Two straight edges on one line.
    Collinear,
    /// Two circular edges about one centre.
    Concentric,
    /// A point kept where it is.
    Fix {
        at: Point2,
    },
    Parallel,
    Perpendicular,
    /// A straight edge, or two points, level.
    Horizontal,
    /// A straight edge, or two points, plumb.
    Vertical,
    /// Two edges touching, at least one circular.
    Tangent {
        touch: Touch,
    },
    /// Two points mirrored in a straight edge.
    Symmetric,
    /// Two straight edges of one length, or two circular ones of one
    /// radius.
    Equal,
    /// The distance between two points, along an axis or point to point.
    Distance {
        axis: Axis,
        value: f64,
    },
    /// The angle from one straight edge to another, radians,
    /// counter-clockwise.
    Angle {
        value: f64,
    },
    Radius {
        value: f64,
    },
    Diameter {
        value: f64,
    },
}

impl Kind {
    /// Whether this is a dimensional constraint, which has a name and a
    /// value.
    pub fn is_dimensional(&self) -> bool {
        matches!(
            self,
            Self::Distance { .. }
                | Self::Angle { .. }
                | Self::Radius { .. }
                | Self::Diameter { .. }
        )
    }

    /// Its value, when it is dimensional.
    pub fn value(&self) -> Option<f64> {
        match *self {
            Self::Distance { value, .. }
            | Self::Angle { value }
            | Self::Radius { value }
            | Self::Diameter { value } => Some(value),
            _ => None,
        }
    }

    /// The same kind with another value; itself when it has none.
    #[must_use]
    pub fn with_value(&self, value: f64) -> Self {
        match self {
            Self::Distance { axis, .. } => Self::Distance { axis: *axis, value },
            Self::Angle { .. } => Self::Angle { value },
            Self::Radius { .. } => Self::Radius { value },
            Self::Diameter { .. } => Self::Diameter { value },
            other => other.clone(),
        }
    }
}

/// A constraint: its kind, the parts it relates, and — a dimensional one —
/// its name.
#[derive(Clone, Debug, PartialEq)]
pub struct Constraint {
    pub kind: Kind,
    pub references: Vec<Reference>,
    /// `d1`, `d2`, …: a dimensional constraint's name, unique in the
    /// drawing; empty for a geometric one.
    pub name: String,
    /// Where a dimensional constraint's dimension line is shown, when it
    /// has been placed (#268, #269).
    pub place: Option<Point2>,
}

/// Why a constraint cannot be a row of the drawing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConstraintError {
    /// An entity it refers to is not in the drawing.
    UnknownEntity(EntityId),
    /// The entity has no such part, or is not of a kind that is constrained.
    NoSuchPart(Reference),
    /// The parts are not what the kind relates: two points for a
    /// coincidence, two straight edges for a parallel, and so on.
    WrongParts,
    /// One part named twice.
    Repeated(Reference),
    /// The entities are in different spaces.
    SpacesDiffer,
    /// A value that is not a number, or not one the kind can have: a
    /// radius of zero, a negative point-to-point distance.
    BadValue,
    /// A dimensional constraint needs a name; a geometric one has none.
    BadName,
}

impl std::fmt::Display for ConstraintError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownEntity(id) => write!(f, "no object {id} in the drawing"),
            Self::NoSuchPart(reference) => write!(
                f,
                "object {} has no {:?} that can be constrained",
                reference.entity, reference.part
            ),
            Self::WrongParts => f.write_str("those are not the parts this constraint relates"),
            Self::Repeated(reference) => {
                write!(
                    f,
                    "{:?} of object {} is named twice",
                    reference.part, reference.entity
                )
            }
            Self::SpacesDiffer => f.write_str("the objects are in different spaces"),
            Self::BadValue => f.write_str("the value is not one this constraint can have"),
            Self::BadName => {
                f.write_str("a dimensional constraint has a name, and a geometric one has none")
            }
        }
    }
}

impl Constraint {
    /// A geometric constraint.
    pub fn geometric(kind: Kind, references: Vec<Reference>) -> Self {
        Self {
            kind,
            references,
            name: String::new(),
            place: None,
        }
    }

    /// A dimensional constraint called `name`.
    pub fn dimensional(name: impl Into<String>, kind: Kind, references: Vec<Reference>) -> Self {
        Self {
            kind,
            references,
            name: name.into(),
            place: None,
        }
    }

    /// The same constraint, its dimension shown through `at`.
    #[must_use]
    pub fn placed(mut self, at: Point2) -> Self {
        self.place = Some(at);
        self
    }

    /// Whether it refers to `id`.
    pub fn refers_to(&self, id: EntityId) -> bool {
        self.references.iter().any(|r| r.entity == id)
    }

    /// Checks the constraint against what its references are: `shape`
    /// says what each is, or `None` for a part its entity does not have.
    pub fn check(&self, shape: impl Fn(Reference) -> Option<Shape>) -> Result<(), ConstraintError> {
        if self.kind.is_dimensional() == self.name.trim().is_empty() {
            return Err(ConstraintError::BadName);
        }
        let mut seen = Vec::new();
        let mut shapes = Vec::new();
        for &reference in &self.references {
            if seen.contains(&reference) {
                return Err(ConstraintError::Repeated(reference));
            }
            seen.push(reference);
            shapes.push(shape(reference).ok_or(ConstraintError::NoSuchPart(reference))?);
        }
        use Shape::{Circular as C, Point as P, Straight as S};
        let fits = match (&self.kind, shapes.as_slice()) {
            (Kind::Coincident, [P, P] | [P, S | C] | [S | C, P]) => true,
            (Kind::Collinear | Kind::Parallel | Kind::Perpendicular, [S, S]) => true,
            (Kind::Concentric, [C, C]) => true,
            (Kind::Fix { .. }, [P]) => true,
            (Kind::Horizontal | Kind::Vertical, [S] | [P, P]) => true,
            (Kind::Tangent { touch }, [S, C] | [C, S]) => {
                matches!(touch, Touch::Left | Touch::Right)
            }
            (Kind::Tangent { touch }, [C, C]) => !matches!(touch, Touch::Left | Touch::Right),
            (Kind::Symmetric, [P, P, S]) => true,
            (Kind::Equal, [S, S] | [C, C]) => true,
            (Kind::Distance { .. }, [P, P]) => true,
            (Kind::Angle { .. }, [S, S]) => true,
            (Kind::Radius { .. } | Kind::Diameter { .. }, [C]) => true,
            _ => false,
        };
        if !fits {
            return Err(ConstraintError::WrongParts);
        }
        let good = match self.kind {
            Kind::Fix { at } => at.x.is_finite() && at.y.is_finite(),
            Kind::Distance {
                axis: Axis::Aligned,
                value,
            } => value.is_finite() && value >= 0.0,
            Kind::Distance { value, .. } | Kind::Angle { value } => value.is_finite(),
            Kind::Radius { value } | Kind::Diameter { value } => value.is_finite() && value > 0.0,
            _ => true,
        };
        let placed = self
            .place
            .is_none_or(|at| at.x.is_finite() && at.y.is_finite());
        if good && placed {
            Ok(())
        } else {
            Err(ConstraintError::BadValue)
        }
    }
}

impl Named for Constraint {
    fn name(&self) -> &str {
        &self.name
    }
}

pub type ConstraintId = TableId<Constraint>;
pub type ConstraintTable = Table<Constraint>;
/// A reversible change to one constraint: added, removed, or — a value
/// changed — replaced.
pub type ConstraintChange = TableChange<Constraint>;

/// A new drawing's constraints: none.
pub(crate) fn initial_constraints() -> ConstraintTable {
    Table::empty()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ArcEntity, CircleEntity, LineEntity, PolylineEntity};

    fn id(n: u64) -> EntityId {
        EntityId::from_raw(n)
    }

    #[test]
    fn each_kind_of_entity_has_the_parts_it_has() {
        let line = Entity::Line(LineEntity::new(Point2::ORIGIN, Point2::new(1.0, 0.0)));
        let circle = Entity::Circle(CircleEntity::new(Point2::ORIGIN, 1.0));
        let arc = Entity::Arc(ArcEntity::new(Point2::ORIGIN, 1.0, 0.0, 1.0));
        let polyline = PolylineEntity::new(
            vec![Point2::ORIGIN, Point2::new(1.0, 0.0), Point2::new(1.0, 1.0)],
            false,
        );
        assert_eq!(Part::Start.shape_on(&line), Some(Shape::Point));
        assert_eq!(Part::Edge.shape_on(&line), Some(Shape::Straight));
        assert_eq!(Part::Center.shape_on(&line), None);
        assert_eq!(Part::Edge.shape_on(&circle), Some(Shape::Circular));
        assert_eq!(Part::Start.shape_on(&circle), None);
        assert_eq!(Part::End.shape_on(&arc), Some(Shape::Point));
        let pl = Entity::Polyline(polyline.clone());
        assert_eq!(Part::Vertex(2).shape_on(&pl), Some(Shape::Point));
        assert_eq!(Part::Vertex(3).shape_on(&pl), None);
        assert_eq!(Part::Segment(1).shape_on(&pl), Some(Shape::Straight));
        assert_eq!(Part::Segment(2).shape_on(&pl), None, "open: two segments");
        let bulged = Entity::Polyline(PolylineEntity::with_bulges(
            polyline.vertices.clone(),
            vec![0.5, 0.0, 0.0],
            false,
        ));
        assert_eq!(Part::Segment(0).shape_on(&bulged), None, "an arc segment");
        assert_eq!(Part::Vertex(0).shape_on(&bulged), Some(Shape::Point));
    }

    #[test]
    fn a_constraint_relates_the_parts_its_kind_relates() {
        let shapes = |r: Reference| match r.part {
            Part::Start | Part::End | Part::Center => Some(Shape::Point),
            Part::Edge if r.entity == id(3) => Some(Shape::Circular),
            Part::Edge => Some(Shape::Straight),
            _ => None,
        };
        let start = |n| Reference::new(id(n), Part::Start);
        let edge = |n| Reference::new(id(n), Part::Edge);
        let ok = |c: Constraint| c.check(shapes);
        assert_eq!(
            ok(Constraint::geometric(
                Kind::Coincident,
                vec![start(1), start(2)]
            )),
            Ok(())
        );
        assert_eq!(
            ok(Constraint::geometric(
                Kind::Parallel,
                vec![edge(1), edge(2)]
            )),
            Ok(())
        );
        assert_eq!(
            ok(Constraint::geometric(
                Kind::Parallel,
                vec![edge(1), edge(3)]
            )),
            Err(ConstraintError::WrongParts)
        );
        assert_eq!(
            ok(Constraint::geometric(
                Kind::Tangent { touch: Touch::Left },
                vec![edge(1), edge(3)]
            )),
            Ok(())
        );
        assert_eq!(
            ok(Constraint::geometric(
                Kind::Tangent {
                    touch: Touch::Outside
                },
                vec![edge(1), edge(3)]
            )),
            Err(ConstraintError::WrongParts),
            "a line touches on a side, not outside"
        );
        assert_eq!(
            ok(Constraint::geometric(
                Kind::Coincident,
                vec![start(1), start(1)]
            )),
            Err(ConstraintError::Repeated(start(1)))
        );
        assert_eq!(
            ok(Constraint::geometric(
                Kind::Coincident,
                vec![start(1), Reference::new(id(2), Part::Vertex(0))]
            )),
            Err(ConstraintError::NoSuchPart(Reference::new(
                id(2),
                Part::Vertex(0)
            )))
        );
        let radius = |value| Constraint::dimensional("d1", Kind::Radius { value }, vec![edge(3)]);
        assert_eq!(ok(radius(5.0)), Ok(()));
        assert_eq!(ok(radius(0.0)), Err(ConstraintError::BadValue));
        assert_eq!(ok(radius(f64::NAN)), Err(ConstraintError::BadValue));
        assert_eq!(
            ok(Constraint::geometric(
                Kind::Radius { value: 5.0 },
                vec![edge(3)]
            )),
            Err(ConstraintError::BadName),
            "a dimension needs a name"
        );
        assert_eq!(
            ok(Constraint::dimensional(
                "d2",
                Kind::Parallel,
                vec![edge(1), edge(2)]
            )),
            Err(ConstraintError::BadName),
            "a geometric constraint has none"
        );
        // A horizontal distance is signed; a point-to-point one is not.
        let distance = |axis, value| {
            Constraint::dimensional(
                "d3",
                Kind::Distance { axis, value },
                vec![start(1), start(2)],
            )
        };
        assert_eq!(ok(distance(Axis::Horizontal, -20.0)), Ok(()));
        assert_eq!(
            ok(distance(Axis::Aligned, -20.0)),
            Err(ConstraintError::BadValue)
        );
        assert_eq!(
            Kind::Radius { value: 5.0 }.with_value(7.0),
            Kind::Radius { value: 7.0 }
        );
    }
}

//! GEOMCONSTRAINT and DELCONSTRAINT (#267, ADR-0038): geometric constraints
//! as AutoCAD 2010 and later offer them, and their removal.
//!
//! GEOMCONSTRAINT asks for a kind — or is started with one, as
//! `GCPARALLEL` is — then for the points or objects that kind relates. A
//! point is the end, centre, or vertex nearest where the pointer or the
//! typed coordinates put it; an object is the edge picked: a line, a
//! circle, an arc, the straight segment of a polyline nearest the pick.
//! The constraint is assessed first: one the others already imply, or one
//! that contradicts them, is refused and says so. Otherwise it is added in
//! one transaction, which the constraints settle (ADR-0038 §5): between
//! two objects the first stays and the second moves to it. An edit the
//! constraints cannot settle is refused, and the session says which.

use cad_core::{EntityId, PixelTolerance, Point2};
use cad_document::{Constraint, Entity, Kind, Part, Reference, Shape, Touch};
use cad_geometry::constraint::Verdict;
use cad_input::{ExpectedInput, InputKind, Keyword};

use super::modify::{NOTHING_SELECTED, SELECT_OBJECTS, selection_expected};
use crate::{Command, CommandContext, CommandInput, Prompt, Step};

const HORIZONTAL: Keyword = Keyword::new("Horizontal");
const VERTICAL: Keyword = Keyword::new("Vertical");
const PERPENDICULAR: Keyword = Keyword::new("Perpendicular");
const PARALLEL: Keyword = Keyword::new("PArallel");
const TANGENT: Keyword = Keyword::new("Tangent");
const SMOOTH: Keyword = Keyword::new("SMooth");
const COINCIDENT: Keyword = Keyword::new("Coincident");
const CONCENTRIC: Keyword = Keyword::new("CONcentric");
const COLLINEAR: Keyword = Keyword::new("COLlinear");
const SYMMETRIC: Keyword = Keyword::new("Symmetric");
const EQUAL: Keyword = Keyword::new("Equal");
const FIX: Keyword = Keyword::new("Fix");
const TWO_POINTS: Keyword = Keyword::new("2Points");
const OBJECT: Keyword = Keyword::new("Object");

const KINDS: [Keyword; 12] = [
    HORIZONTAL,
    VERTICAL,
    PERPENDICULAR,
    PARALLEL,
    TANGENT,
    SMOOTH,
    COINCIDENT,
    CONCENTRIC,
    COLLINEAR,
    SYMMETRIC,
    EQUAL,
    FIX,
];

/// How near a point must be to the pointer, or to typed coordinates, to be
/// the end, centre, or vertex meant: the pick box.
const POINT_APERTURE: PixelTolerance = PixelTolerance(5.0);

/// The kinds GEOMCONSTRAINT offers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Geom {
    Horizontal,
    Vertical,
    Perpendicular,
    Parallel,
    Tangent,
    Coincident,
    Concentric,
    Collinear,
    Symmetric,
    Equal,
    Fix,
}

impl Geom {
    fn from_keyword(keyword: Keyword) -> Option<Self> {
        Some(match keyword {
            k if k == HORIZONTAL => Self::Horizontal,
            k if k == VERTICAL => Self::Vertical,
            k if k == PERPENDICULAR => Self::Perpendicular,
            k if k == PARALLEL => Self::Parallel,
            k if k == TANGENT => Self::Tangent,
            k if k == COINCIDENT => Self::Coincident,
            k if k == CONCENTRIC => Self::Concentric,
            k if k == COLLINEAR => Self::Collinear,
            k if k == SYMMETRIC => Self::Symmetric,
            k if k == EQUAL => Self::Equal,
            k if k == FIX => Self::Fix,
            _ => return None,
        })
    }

    /// What it asks for, in order.
    fn wants(self) -> &'static [Want] {
        use Want::{Object, Point};
        match self {
            // An object, or two points with the 2Points option.
            Self::Horizontal | Self::Vertical => &[Object],
            Self::Coincident => &[Point, Point],
            Self::Fix => &[Point],
            Self::Symmetric => &[Point, Point, Object],
            _ => &[Object, Object],
        }
    }
}

/// What a step of the command takes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Want {
    Point,
    Object,
}

#[derive(Debug)]
enum State {
    Kind,
    Picking {
        geom: Geom,
        /// What each step takes, the 2Points option having turned an
        /// object into two points.
        wants: Vec<Want>,
        picked: Vec<Reference>,
    },
}

#[derive(Debug)]
pub(crate) struct GeomConstraintCommand {
    state: State,
}

impl GeomConstraintCommand {
    /// GEOMCONSTRAINT: asks for the kind.
    pub(crate) fn new() -> Self {
        Self { state: State::Kind }
    }

    /// `GCPARALLEL` and the like: the kind given.
    pub(crate) fn of(geom: Geom) -> Self {
        Self {
            state: State::Picking {
                geom,
                wants: geom.wants().to_vec(),
                picked: Vec::new(),
            },
        }
    }
}

/// The end, centre, or vertex nearest `at` within the pick box, of an
/// object shown in the current space, other than one already `picked`:
/// two ends that meet are told apart by picking the place twice. The
/// lowest ID among equals.
pub(super) fn point_near(
    ctx: &CommandContext<'_>,
    at: Point2,
    picked: &[Reference],
) -> Option<Reference> {
    let reach = ctx.view.pixels_to_world(POINT_APERTURE.0);
    let screen = ctx.view.world_to_screen(at);
    let mut best: Option<(f64, Reference)> = None;
    let mut ids = ctx
        .drawing
        .query_shown_in(ctx.space, &ctx.view.pick_bounds(screen, POINT_APERTURE));
    ids.sort_unstable();
    for id in ids {
        let Some(entity) = ctx.drawing.entity(id) else {
            continue;
        };
        for part in Part::points_of(entity) {
            let Some(position) = part.position_on(entity) else {
                continue;
            };
            let distance = position.distance_to(at);
            if picked.contains(&Reference::new(id, part)) {
                continue;
            }
            if distance <= reach && best.is_none_or(|(d, _)| distance < d) {
                best = Some((distance, Reference::new(id, part)));
            }
        }
    }
    best.map(|(_, reference)| reference)
}

/// The edge of `entity` a pick at `at` means: the whole of a line, circle,
/// or arc; the polyline segment nearest it.
pub(super) fn edge_at(entity: &Entity, id: EntityId, at: Point2) -> Option<Reference> {
    match entity {
        Entity::Line(_) | Entity::Circle(_) | Entity::Arc(_) => {
            Some(Reference::new(id, Part::Edge))
        }
        Entity::Polyline(polyline) => {
            let count = polyline.vertices.len();
            let segments = if polyline.closed {
                count
            } else {
                count.saturating_sub(1)
            };
            (0..segments)
                .map(|n| {
                    let a = polyline.vertices[n];
                    let b = polyline.vertices[(n + 1) % count];
                    let primitive = cad_geometry::bulge_primitive(a, b, polyline.bulge(n));
                    (primitive.closest_point(at).distance_to(at), n)
                })
                .min_by(|x, y| x.0.total_cmp(&y.0))
                .map(|(_, n)| Reference::new(id, Part::Segment(n as u32)))
        }
        _ => None,
    }
}

/// Where a point part is now, for a fix.
fn position(ctx: &CommandContext<'_>, reference: Reference) -> Option<Point2> {
    reference
        .part
        .position_on(ctx.drawing.entity(reference.entity)?)
}

/// How two edges touch, as they are: the side of a line a circle is on;
/// two circles outside or one inside the other.
fn touch(ctx: &CommandContext<'_>, a: Reference, b: Reference) -> Option<Touch> {
    let shape = |r: Reference| r.part.shape_on(ctx.drawing.entity(r.entity)?);
    let circle = |r: Reference| -> Option<(Point2, f64)> {
        match ctx.drawing.entity(r.entity)? {
            Entity::Circle(c) => Some((c.center, c.radius)),
            Entity::Arc(a) => Some((a.center, a.radius)),
            _ => None,
        }
    };
    let line = |r: Reference| -> Option<(Point2, Point2)> {
        match (ctx.drawing.entity(r.entity)?, r.part) {
            (Entity::Line(l), _) => Some((l.start, l.end)),
            (Entity::Polyline(p), Part::Segment(n)) => {
                let n = n as usize;
                Some((p.vertices[n], p.vertices[(n + 1) % p.vertices.len()]))
            }
            _ => None,
        }
    };
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

impl GeomConstraintCommand {
    /// The constraint the picks make.
    fn constraint(
        geom: Geom,
        picked: &[Reference],
        ctx: &CommandContext<'_>,
    ) -> Option<Constraint> {
        let kind = match geom {
            Geom::Horizontal => Kind::Horizontal,
            Geom::Vertical => Kind::Vertical,
            Geom::Perpendicular => Kind::Perpendicular,
            Geom::Parallel => Kind::Parallel,
            Geom::Tangent => Kind::Tangent {
                touch: touch(ctx, picked[0], picked[1])?,
            },
            Geom::Coincident => Kind::Coincident,
            Geom::Concentric => Kind::Concentric,
            Geom::Collinear => Kind::Collinear,
            Geom::Symmetric => Kind::Symmetric,
            Geom::Equal => Kind::Equal,
            Geom::Fix => Kind::Fix {
                at: position(ctx, picked[0])?,
            },
        };
        Some(Constraint::geometric(kind, picked.to_vec()))
    }

    /// Adds the constraint, held by its first object; what to say.
    fn apply(geom: Geom, picked: &[Reference], ctx: &mut CommandContext<'_>) -> Option<String> {
        let Some(constraint) = Self::constraint(geom, picked, ctx) else {
            return Some("Those objects cannot take that constraint.".to_owned());
        };
        let mut transaction = ctx.transaction();
        match transaction.assess(&constraint).map(|a| a.verdict) {
            Some(Verdict::Redundant) => {
                return Some(
                    "The other constraints already imply it: it would over-constrain the geometry."
                        .to_owned(),
                );
            }
            Some(Verdict::Conflicting) => {
                return Some("It contradicts the other constraints.".to_owned());
            }
            _ => {}
        }
        if let Err(error) = transaction.add_constraint(constraint) {
            return Some(match error {
                cad_document::TransactionError::BadConstraint(why) => {
                    let why = why.to_string();
                    let mut text = why[..1].to_uppercase();
                    text.push_str(&why[1..]);
                    text.push('.');
                    text
                }
                other => other.to_string(),
            });
        }
        // Between two objects the first stays and the second comes to it;
        // a symmetry line stays too. Two parts of one object hold nothing.
        let mover = picked.get(1).map(|r| r.entity);
        for reference in picked {
            if mover.is_some_and(|mover| mover != reference.entity) {
                transaction.hold(reference.entity);
            }
        }
        // A refusal is logged by the session.
        transaction.commit();
        None
    }
}

impl Command for GeomConstraintCommand {
    fn start(&mut self, _ctx: &mut CommandContext<'_>) -> Step {
        Step::Continue
    }

    fn prompt(&self) -> Prompt {
        match &self.state {
            State::Kind => Prompt::new("Enter constraint type").with_default("Coincident"),
            State::Picking {
                geom,
                wants,
                picked,
            } => {
                let step = picked.len();
                let text = match (geom, wants.get(step), step) {
                    (Geom::Horizontal | Geom::Vertical, Some(Want::Object), _) => {
                        return Prompt::new("Select an object").with_default("2Points");
                    }
                    (Geom::Coincident, Some(Want::Point), 0) => {
                        return Prompt::new("Select first point").with_default("Object");
                    }
                    (Geom::Symmetric, _, 2) => "Select symmetry line",
                    (Geom::Fix, _, _) => "Select point",
                    (Geom::Coincident, Some(Want::Point), _) => "Select point",
                    (_, Some(Want::Point), 0) => "Select first point",
                    (_, Some(Want::Point), _) => "Select second point",
                    (_, _, 0) => "Select first object",
                    _ => "Select second object",
                };
                Prompt::new(text)
            }
        }
    }

    fn expected(&self) -> ExpectedInput {
        match &self.state {
            State::Kind => ExpectedInput::new(InputKind::Nothing).with_keywords(&KINDS),
            State::Picking {
                geom,
                wants,
                picked,
            } => match wants.get(picked.len()) {
                Some(Want::Object) if matches!(geom, Geom::Horizontal | Geom::Vertical) => {
                    ExpectedInput::object().with_keywords(&[TWO_POINTS])
                }
                Some(Want::Point) if *geom == Geom::Coincident && picked.is_empty() => {
                    ExpectedInput::point().with_keywords(&[OBJECT])
                }
                Some(Want::Point) => ExpectedInput::point(),
                _ => ExpectedInput::object(),
            },
        }
    }

    fn step(&mut self, input: CommandInput, ctx: &mut CommandContext<'_>) -> Step {
        let state = std::mem::replace(&mut self.state, State::Kind);
        match (state, input) {
            (State::Kind, CommandInput::Keyword(keyword)) if keyword == SMOOTH => {
                ctx.message("Smooth joins splines, which are not constrained.");
                Step::Finished
            }
            (State::Kind, CommandInput::Keyword(keyword)) => {
                let geom = Geom::from_keyword(keyword).unwrap_or(Geom::Coincident);
                *self = Self::of(geom);
                Step::Continue
            }
            (State::Kind, CommandInput::Accept) => {
                *self = Self::of(Geom::Coincident);
                Step::Continue
            }
            (State::Kind, _) => Step::Continue,
            (
                State::Picking {
                    geom,
                    mut wants,
                    mut picked,
                },
                input,
            ) => {
                let want = wants.get(picked.len()).copied();
                let reference = match (want, input) {
                    // Horizontal and vertical: two points instead of an object.
                    (Some(Want::Object), CommandInput::Keyword(k)) if k == TWO_POINTS => {
                        wants = vec![Want::Point, Want::Point];
                        None
                    }
                    (Some(Want::Object), CommandInput::Accept)
                        if matches!(geom, Geom::Horizontal | Geom::Vertical) =>
                    {
                        wants = vec![Want::Point, Want::Point];
                        None
                    }
                    // Coincident: a point on an object instead of two points.
                    (Some(Want::Point), CommandInput::Keyword(k)) if k == OBJECT => {
                        wants = vec![Want::Object, Want::Point];
                        None
                    }
                    (Some(Want::Point), CommandInput::Accept)
                        if geom == Geom::Coincident && picked.is_empty() =>
                    {
                        wants = vec![Want::Object, Want::Point];
                        None
                    }
                    (Some(Want::Point), CommandInput::Point(at)) => {
                        match point_near(ctx, at, &picked) {
                            Some(reference) => Some(reference),
                            None => {
                                ctx.message("No end, centre, or vertex there.");
                                None
                            }
                        }
                    }
                    (Some(Want::Object), CommandInput::Pick { entity, at }) => {
                        match ctx
                            .drawing
                            .entity(entity)
                            .and_then(|e| edge_at(e, entity, at))
                        {
                            Some(reference) => Some(reference),
                            None => {
                                ctx.message("That object cannot be constrained.");
                                None
                            }
                        }
                    }
                    _ => None,
                };
                if let Some(reference) = reference {
                    if picked.contains(&reference) {
                        ctx.message("That is already picked.");
                    } else {
                        picked.push(reference);
                    }
                }
                if picked.len() < wants.len() {
                    self.state = State::Picking {
                        geom,
                        wants,
                        picked,
                    };
                    return Step::Continue;
                }
                if let Some(message) = Self::apply(geom, &picked, ctx) {
                    ctx.message(message);
                }
                Step::Finished
            }
        }
    }
}

// ── DELCONSTRAINT ──

/// DELCONSTRAINT: every constraint on the selected objects, removed. The
/// geometry stays where it is.
#[derive(Debug)]
pub(crate) struct DelConstraintCommand;

impl DelConstraintCommand {
    fn remove(ctx: &mut CommandContext<'_>) -> Step {
        let ids: Vec<EntityId> = ctx.selection.iter().collect();
        if ids.is_empty() {
            ctx.message(NOTHING_SELECTED);
            return Step::Finished;
        }
        let mut doomed: Vec<_> = ids
            .iter()
            .flat_map(|&id| ctx.drawing.constraints_of(id).map(|(c, _)| c))
            .collect();
        doomed.sort_unstable();
        doomed.dedup();
        if doomed.is_empty() {
            ctx.message("The selected objects have no constraints.");
            return Step::Finished;
        }
        let mut transaction = ctx.transaction();
        for id in &doomed {
            transaction
                .remove_constraint(*id)
                .expect("a constraint of the drawing, once");
        }
        transaction.commit();
        ctx.message(match doomed.len() {
            1 => "1 constraint removed.".to_owned(),
            n => format!("{n} constraints removed."),
        });
        Step::Finished
    }
}

impl Command for DelConstraintCommand {
    fn start(&mut self, ctx: &mut CommandContext<'_>) -> Step {
        if ctx.selection.is_empty() {
            Step::Continue
        } else {
            Self::remove(ctx)
        }
    }

    fn prompt(&self) -> Prompt {
        Prompt::new(SELECT_OBJECTS)
    }

    fn expected(&self) -> ExpectedInput {
        selection_expected()
    }

    fn step(&mut self, input: CommandInput, ctx: &mut CommandContext<'_>) -> Step {
        match input {
            CommandInput::Accept => Self::remove(ctx),
            _ => Step::Continue,
        }
    }
}

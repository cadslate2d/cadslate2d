//! XLINE and RAY (#206; ADR-0031): construction lines, endless both ways
//! or one way. XLINE through a point and each point after it, or
//! horizontal, vertical, at an angle through each point, bisecting an
//! angle, or parallel to a line at a distance or through a point; RAY
//! from a start through each point after it. Until Enter; one undo.

use cad_core::{EntityId, Point2, Vec2};
use cad_document::{Entity, RayEntity, XLineEntity};
use cad_input::{ExpectedInput, InputKind, Keyword};

use super::ltscale::display;
use crate::{Command, CommandContext, CommandInput, OverlayItem, Prompt, Step};

const HOR: Keyword = Keyword::new("Hor");
const VER: Keyword = Keyword::new("Ver");
const ANG: Keyword = Keyword::new("Ang");
const BISECT: Keyword = Keyword::new("Bisect");
const OFFSET: Keyword = Keyword::new("Offset");
const THROUGH: Keyword = Keyword::new("Through");

fn insert(ctx: &mut CommandContext<'_>, entity: Entity) {
    let mut tx = ctx.transaction();
    tx.insert(entity)
        .expect("the current layer and linetype exist");
    tx.commit();
}

/// The base point and unit direction of a line object an xline can be
/// drawn parallel to: a line, an xline, or a ray.
fn line_of(entity: &Entity) -> Option<(Point2, Vec2)> {
    match entity {
        Entity::Line(line) => {
            let along = line.end - line.start;
            let length = along.length();
            (length > 0.0).then(|| (line.start, along * (1.0 / length)))
        }
        Entity::XLine(line) => Some((line.base, line.direction)),
        Entity::Ray(ray) => Some((ray.start, ray.direction)),
        _ => None,
    }
}

#[derive(Debug)]
enum State {
    First,
    /// Each point makes an xline through `base` and it.
    Through {
        base: Point2,
    },
    /// Each point makes an xline along `direction` through it.
    Oriented {
        direction: Vec2,
    },
    Angle,
    BisectVertex,
    BisectStart {
        vertex: Point2,
    },
    BisectEnd {
        vertex: Point2,
        start: Point2,
    },
    OffsetDistance,
    OffsetPick {
        distance: Option<f64>,
    },
    OffsetSide {
        distance: Option<f64>,
        line: (Point2, Vec2),
    },
}

#[derive(Debug)]
pub(crate) struct XLineCommand {
    state: State,
    /// The offset distance last given: the next default.
    distance: Option<f64>,
}

impl XLineCommand {
    pub(crate) fn new() -> Self {
        Self {
            state: State::First,
            distance: None,
        }
    }

    /// The xline the state and `point` make, if any.
    fn made(&self, point: Point2) -> Option<XLineEntity> {
        match &self.state {
            State::Through { base } => XLineEntity::through(*base, point),
            State::Oriented { direction } => XLineEntity::new(point, *direction),
            State::BisectEnd { vertex, start } => {
                let a = cad_document::XLineEntity::through(*vertex, *start)?.direction;
                let b = cad_document::XLineEntity::through(*vertex, point)?.direction;
                let sum = a + b;
                // Opposite rays: the bisector is square to them.
                let direction = if sum.length() > 1e-12 {
                    sum
                } else {
                    Vec2::new(-a.y, a.x)
                };
                XLineEntity::new(*vertex, direction)
            }
            State::OffsetSide {
                distance,
                line: (base, direction),
            } => {
                let normal = Vec2::new(-direction.y, direction.x);
                let towards = (point - *base).dot(normal);
                let moved = match distance {
                    Some(distance) if towards != 0.0 => normal * (distance * towards.signum()),
                    Some(_) => return None,
                    // Through the point: as far as it is.
                    None => normal * towards,
                };
                XLineEntity::new(*base + moved, *direction)
            }
            _ => None,
        }
    }
}

impl Command for XLineCommand {
    fn prompt(&self) -> Prompt {
        match &self.state {
            State::First => Prompt::new("Specify a point"),
            State::Through { .. } | State::Oriented { .. } => Prompt::new("Specify through point"),
            State::Angle => Prompt::new("Enter angle of xline"),
            State::BisectVertex => Prompt::new("Specify angle vertex point"),
            State::BisectStart { .. } => Prompt::new("Specify angle start point"),
            State::BisectEnd { .. } => Prompt::new("Specify angle end point"),
            State::OffsetDistance => match self.distance {
                Some(distance) => {
                    Prompt::new("Specify offset distance").with_default(display(distance))
                }
                None => Prompt::new("Specify offset distance").with_default("Through"),
            },
            State::OffsetPick { .. } => Prompt::new("Select a line object"),
            State::OffsetSide {
                distance: Some(_), ..
            } => Prompt::new("Specify side to offset"),
            State::OffsetSide { distance: None, .. } => Prompt::new("Specify through point"),
        }
    }

    fn expected(&self) -> ExpectedInput {
        match &self.state {
            State::First => ExpectedInput::point().with_keywords(&[HOR, VER, ANG, BISECT, OFFSET]),
            State::Angle => ExpectedInput::angle(),
            State::OffsetDistance => ExpectedInput::distance().with_keywords(&[THROUGH]),
            State::OffsetPick { .. } => ExpectedInput::new(InputKind::Object),
            _ => ExpectedInput::point(),
        }
    }

    fn step(&mut self, input: CommandInput, ctx: &mut CommandContext<'_>) -> Step {
        match (&self.state, input) {
            (State::First, CommandInput::Point(base)) => self.state = State::Through { base },
            (State::First, CommandInput::Keyword(HOR)) => {
                self.state = State::Oriented {
                    direction: Vec2::new(1.0, 0.0),
                };
            }
            (State::First, CommandInput::Keyword(VER)) => {
                self.state = State::Oriented {
                    direction: Vec2::new(0.0, 1.0),
                };
            }
            (State::First, CommandInput::Keyword(ANG)) => self.state = State::Angle,
            (State::First, CommandInput::Keyword(BISECT)) => self.state = State::BisectVertex,
            (State::First, CommandInput::Keyword(_)) => self.state = State::OffsetDistance,
            (State::Angle, CommandInput::Angle(degrees)) => {
                self.state = State::Oriented {
                    direction: cad_geometry::unit_at_degrees(degrees),
                };
            }
            (State::BisectVertex, CommandInput::Point(vertex)) => {
                self.state = State::BisectStart { vertex };
            }
            (State::BisectStart { vertex }, CommandInput::Point(start)) => {
                if start == *vertex {
                    ctx.message("The start point must differ from the vertex.");
                } else {
                    self.state = State::BisectEnd {
                        vertex: *vertex,
                        start,
                    };
                }
            }
            (State::OffsetDistance, CommandInput::Distance(distance)) => {
                if distance.is_finite() && distance > 0.0 {
                    self.distance = Some(distance);
                    self.state = State::OffsetPick {
                        distance: Some(distance),
                    };
                } else {
                    ctx.message("The offset distance must be a positive number.");
                }
            }
            (State::OffsetDistance, CommandInput::Keyword(_)) => {
                self.state = State::OffsetPick { distance: None };
            }
            (State::OffsetDistance, CommandInput::Accept) => {
                self.state = State::OffsetPick {
                    distance: self.distance,
                };
            }
            (State::OffsetPick { distance }, CommandInput::Pick { entity, .. }) => {
                match ctx.drawing.entity(entity).and_then(line_of) {
                    Some(line) => {
                        self.state = State::OffsetSide {
                            distance: *distance,
                            line,
                        };
                    }
                    None => ctx.message("Pick a line, a construction line, or a ray."),
                }
            }
            (
                State::Through { .. }
                | State::Oriented { .. }
                | State::BisectEnd { .. }
                | State::OffsetSide { .. },
                CommandInput::Point(point),
            ) => {
                match self.made(point) {
                    Some(line) => insert(ctx, line.into()),
                    None => ctx.message("That point gives no direction."),
                }
                if let State::OffsetSide { distance, .. } = self.state {
                    self.state = State::OffsetPick { distance };
                }
            }
            (_, CommandInput::Accept) => return Step::Finished,
            _ => {}
        }
        Step::Continue
    }

    /// The xline a point would make.
    fn preview(&self, cursor: Point2, out: &mut Vec<OverlayItem>) {
        if let Some(line) = self.made(cursor) {
            out.push(OverlayItem::PreviewEntity(line.into()));
        }
    }

    fn base_point(&self) -> Option<Point2> {
        match &self.state {
            State::Through { base } => Some(*base),
            State::BisectStart { vertex } | State::BisectEnd { vertex, .. } => Some(*vertex),
            _ => None,
        }
    }

    fn transient_sources(&self) -> &[EntityId] {
        &[]
    }
}

/// RAY: from a start point through each point after it.
#[derive(Debug, Default)]
pub(crate) struct RayCommand {
    start: Option<Point2>,
}

impl RayCommand {
    pub(crate) fn new() -> Self {
        Self::default()
    }
}

impl Command for RayCommand {
    fn prompt(&self) -> Prompt {
        Prompt::new(if self.start.is_none() {
            "Specify start point"
        } else {
            "Specify through point"
        })
    }

    fn expected(&self) -> ExpectedInput {
        ExpectedInput::point()
    }

    fn step(&mut self, input: CommandInput, ctx: &mut CommandContext<'_>) -> Step {
        match (self.start, input) {
            (None, CommandInput::Point(start)) => self.start = Some(start),
            (Some(start), CommandInput::Point(through)) => match RayEntity::through(start, through)
            {
                Some(ray) => insert(ctx, ray.into()),
                None => ctx.message("The through point must differ from the start."),
            },
            (_, CommandInput::Accept) => return Step::Finished,
            _ => {}
        }
        Step::Continue
    }

    fn preview(&self, cursor: Point2, out: &mut Vec<OverlayItem>) {
        if let Some(ray) = self
            .start
            .and_then(|start| RayEntity::through(start, cursor))
        {
            out.push(OverlayItem::PreviewEntity(ray.into()));
        }
    }

    fn base_point(&self) -> Option<Point2> {
        self.start
    }
}

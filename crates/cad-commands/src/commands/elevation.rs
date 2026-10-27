//! ELEVATION (#337, ADR-0040): a façade of a floor plan drawn by
//! FLOORPLAN. Pick an object of the plan, or give its group's name; the
//! side it is seen from; the roof — the brief's, or another — and its
//! pitch; and where the façade's left end stands on the ground. Drawn as
//! ordinary geometry on the `ELEV-*` layers, one undo, in a group `ELEV-n`
//! holding what it was drawn from (`crate::elevation`).

use cad_core::Point2;
use cad_input::{ExpectedInput, InputKind, Keyword};
use cad_layout::{Roof, Side, elevation};

use crate::elevation::{GROUP, LAYERS, description, entities};
use crate::floorplan::{DrawnPlan, PLAN_GROUP, drawn_plan, plan_holding};
use crate::views::draw_group;
use crate::{Command, CommandContext, CommandInput, OverlayItem, Prompt, Step};

const NAME: Keyword = Keyword::new("Name");
const FRONT: Keyword = Keyword::new("Front");
const BACK: Keyword = Keyword::new("Back");
const LEFT: Keyword = Keyword::new("Left");
const RIGHT: Keyword = Keyword::new("Right");
const FLAT: Keyword = Keyword::new("Flat");
const GABLE: Keyword = Keyword::new("Gable");
const HIP: Keyword = Keyword::new("Hip");

#[derive(Clone, Copy, Debug)]
enum State {
    Plan,
    Name,
    Side,
    Roof,
    Pitch,
    Place,
}

#[derive(Debug)]
pub(crate) struct ElevationCommand {
    state: State,
    plan: Option<Box<DrawnPlan>>,
    side: Side,
}

impl ElevationCommand {
    pub(crate) fn new() -> Self {
        Self {
            state: State::Plan,
            plan: None,
            side: Side::Front,
        }
    }

    fn take(&mut self, name: &str, ctx: &mut CommandContext<'_>) -> State {
        match drawn_plan(ctx.drawing, name) {
            Ok(plan) => {
                self.plan = Some(Box::new(plan));
                State::Side
            }
            Err(why) => {
                ctx.message(why);
                State::Plan
            }
        }
    }

    /// Where the façade goes by default: under the plan's terrain.
    fn default_place(&self) -> Point2 {
        self.plan.as_ref().map_or(Point2::ORIGIN, |plan| {
            Point2::new(plan.origin.x, plan.origin.y - 12.0 * plan.scale)
        })
    }

    fn finish(&self, at: Point2, ctx: &mut CommandContext<'_>) -> Step {
        let Some(plan) = &self.plan else {
            return Step::Finished;
        };
        let facade = elevation(&plan.spec, &plan.resolved, self.side);
        let made = entities(&facade, self.side, at, plan.scale);
        let space = ctx.space;
        match draw_group(
            ctx.drawing,
            space,
            &LAYERS,
            made,
            GROUP,
            description(&plan.name, self.side, at, plan.scale, &plan.spec.building),
        ) {
            Ok((_, name)) => ctx.message(format!(
                "{} elevation of {} drawn as {name}: {:.2} m long, {:.2} m high.",
                self.side.name(),
                plan.name,
                facade.width,
                facade.height
            )),
            Err(error) => ctx.message(error.to_string()),
        }
        Step::Finished
    }
}

impl Command for ElevationCommand {
    fn prompt(&self) -> Prompt {
        let roof = self
            .plan
            .as_ref()
            .map_or(Roof::Flat, |p| p.spec.building.roof);
        match self.state {
            State::Plan => Prompt::new("Select an object of a floor plan"),
            State::Name => Prompt::new("Enter the floor plan's group name"),
            State::Side => Prompt::new("Which side?").with_default(self.side.name()),
            State::Roof => Prompt::new("Which roof?").with_default(match roof {
                Roof::Flat => "Flat",
                Roof::Gable => "Gable",
                Roof::Hip => "Hip",
            }),
            State::Pitch => {
                Prompt::new("Specify roof pitch").with_default(super::ltscale::display(
                    self.plan
                        .as_ref()
                        .map_or(30.0, |p| p.spec.building.roof_pitch),
                ))
            }
            State::Place => Prompt::new("Specify the left end of the façade on the ground")
                .with_default({
                    let p = self.default_place();
                    format!(
                        "{},{}",
                        super::ltscale::display(p.x),
                        super::ltscale::display(p.y)
                    )
                }),
        }
    }

    fn expected(&self) -> ExpectedInput {
        match self.state {
            State::Plan => ExpectedInput::object().with_keywords(&[NAME]),
            State::Name => ExpectedInput::text(),
            State::Side => {
                ExpectedInput::new(InputKind::Nothing).with_keywords(&[FRONT, BACK, LEFT, RIGHT])
            }
            State::Roof => {
                ExpectedInput::new(InputKind::Nothing).with_keywords(&[FLAT, GABLE, HIP])
            }
            State::Pitch => ExpectedInput::angle(),
            State::Place => ExpectedInput::point(),
        }
    }

    fn preview(&self, cursor: Point2, out: &mut Vec<OverlayItem>) {
        if let (State::Place, Some(plan)) = (&self.state, &self.plan) {
            let facade = elevation(&plan.spec, &plan.resolved, self.side);
            for (_, entity) in entities(&facade, self.side, cursor, plan.scale) {
                out.push(OverlayItem::PreviewEntity(entity));
            }
        }
    }

    fn step(&mut self, input: CommandInput, ctx: &mut CommandContext<'_>) -> Step {
        self.state = match (self.state, input) {
            (State::Plan, CommandInput::Pick { entity, .. }) => {
                match plan_holding(ctx.drawing, entity) {
                    Some(name) => self.take(&name, ctx),
                    None => {
                        ctx.message("That object is not part of a floor plan drawn by FLOORPLAN.");
                        State::Plan
                    }
                }
            }
            (State::Plan, CommandInput::Keyword(_)) => State::Name,
            (State::Name, CommandInput::Text(name)) => self.take(&name, ctx),
            (State::Name, CommandInput::Accept) => {
                // The one plan there is, when there is one.
                let plans: Vec<String> = ctx
                    .drawing
                    .groups()
                    .iter()
                    .map(|(_, g)| g.name.clone())
                    .filter(|n| n.starts_with(PLAN_GROUP))
                    .collect();
                match plans.as_slice() {
                    [only] => {
                        let only = only.clone();
                        self.take(&only, ctx)
                    }
                    _ => State::Name,
                }
            }
            (State::Side, CommandInput::Keyword(k)) => {
                self.side = match k {
                    k if k == BACK => Side::Back,
                    k if k == LEFT => Side::Left,
                    k if k == RIGHT => Side::Right,
                    _ => Side::Front,
                };
                State::Roof
            }
            (State::Side, CommandInput::Accept) => State::Roof,
            (State::Roof, CommandInput::Keyword(k)) => {
                let roof = match k {
                    k if k == GABLE => Roof::Gable,
                    k if k == HIP => Roof::Hip,
                    _ => Roof::Flat,
                };
                if let Some(plan) = &mut self.plan {
                    plan.spec.building.roof = roof;
                }
                if roof == Roof::Flat {
                    State::Place
                } else {
                    State::Pitch
                }
            }
            (State::Roof, CommandInput::Accept) => {
                let flat = self
                    .plan
                    .as_ref()
                    .is_none_or(|p| p.spec.building.roof == Roof::Flat);
                if flat { State::Place } else { State::Pitch }
            }
            (State::Pitch, CommandInput::Angle(degrees)) => {
                if degrees > 0.0 && degrees < 75.0 {
                    if let Some(plan) = &mut self.plan {
                        plan.spec.building.roof_pitch = degrees;
                    }
                    State::Place
                } else {
                    ctx.message("A roof's pitch must be between 0 and 75 degrees.");
                    State::Pitch
                }
            }
            (State::Pitch, CommandInput::Accept) => State::Place,
            (State::Place, CommandInput::Point(at)) => return self.finish(at, ctx),
            (State::Place, CommandInput::Accept) => {
                let at = self.default_place();
                return self.finish(at, ctx);
            }
            (_, _) => return Step::Continue,
        };
        Step::Continue
    }
}

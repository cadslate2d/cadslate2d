//! BSECTION (#338, ADR-0040): a building section through a floor plan
//! drawn by FLOORPLAN. Pick an object of the plan or give its group's
//! name; two points of the cut line — across the plan or up it, square to
//! its walls, the section looking to the left of the way from the first
//! to the second — and where the floor's left end goes. Drawn on the
//! `SECT-*` layers, the cut marked on the plan with its letter, one undo,
//! in a group `SECT-n` holding what it was drawn from (`crate::bsection`).

use cad_core::Point2;
use cad_input::{ExpectedInput, Keyword};
use cad_layout::{Cut, section};

use crate::bsection::{GROUP, LAYERS, description, entities, mark};
use crate::floorplan::{DrawnPlan, PLAN_GROUP, drawn_plan, plan_holding};
use crate::views::draw_group;
use crate::{Command, CommandContext, CommandInput, OverlayItem, Prompt, Step};

const NAME: Keyword = Keyword::new("Name");

#[derive(Clone, Copy, Debug)]
enum State {
    Plan,
    Name,
    First,
    Second(Point2),
    Place(Cut),
}

#[derive(Debug)]
pub(crate) struct BuildingSectionCommand {
    state: State,
    plan: Option<Box<DrawnPlan>>,
}

impl BuildingSectionCommand {
    pub(crate) fn new() -> Self {
        Self {
            state: State::Plan,
            plan: None,
        }
    }

    fn take(&mut self, name: &str, ctx: &mut CommandContext<'_>) -> State {
        match drawn_plan(ctx.drawing, name) {
            Ok(plan) => {
                self.plan = Some(Box::new(plan));
                State::First
            }
            Err(why) => {
                ctx.message(why);
                State::Plan
            }
        }
    }

    /// The cut two points of the drawing mean: square to the plan's axis
    /// they run nearer, through their middle, looking to their left.
    fn cut(&self, a: Point2, b: Point2) -> Option<Cut> {
        let plan = self.plan.as_ref()?;
        let (dx, dy) = (b.x - a.x, b.y - a.y);
        let middle = Point2::new((a.x + b.x) / 2.0, (a.y + b.y) / 2.0);
        let metres = |v: f64, o: f64| (v - o) / plan.scale;
        Some(if dx.abs() >= dy.abs() {
            Cut {
                along_x: true,
                at: metres(middle.y, plan.origin.y),
                looking_up: dx > 0.0,
            }
        } else {
            Cut {
                along_x: false,
                at: metres(middle.x, plan.origin.x),
                looking_up: dy < 0.0,
            }
        })
    }

    /// The next letter free: A for the first section, B for the second.
    fn letter(ctx: &CommandContext<'_>) -> char {
        let taken = ctx
            .drawing
            .groups()
            .iter()
            .filter(|(_, g)| g.name.starts_with(GROUP))
            .count();
        char::from(b'A' + (taken % 26) as u8)
    }

    fn default_place(&self) -> Point2 {
        self.plan.as_ref().map_or(Point2::ORIGIN, |plan| {
            Point2::new(plan.origin.x, plan.origin.y - 12.0 * plan.scale)
        })
    }

    fn finish(&self, cut: Cut, at: Point2, ctx: &mut CommandContext<'_>) -> Step {
        let Some(plan) = &self.plan else {
            return Step::Finished;
        };
        let Some(cutting) = section(&plan.spec, &plan.layout, cut) else {
            ctx.message("The cut misses the building.");
            return Step::Finished;
        };
        let letter = Self::letter(ctx);
        let mut made = entities(
            &plan.spec,
            &plan.layout,
            cut,
            &cutting,
            letter,
            at,
            plan.scale,
        );
        made.extend(mark(&plan.layout, cut, letter, plan.origin, plan.scale));
        let space = ctx.space;
        match draw_group(
            ctx.drawing,
            space,
            &LAYERS,
            made,
            GROUP,
            description(&plan.name, cut, letter, at, plan.scale),
        ) {
            Ok((_, name)) => ctx.message(format!(
                "Section {letter}-{letter} of {} drawn as {name}: {:.2} m long, {:.2} m high.",
                plan.name, cutting.width, cutting.height
            )),
            Err(error) => ctx.message(error.to_string()),
        }
        Step::Finished
    }
}

impl Command for BuildingSectionCommand {
    fn prompt(&self) -> Prompt {
        match self.state {
            State::Plan => Prompt::new("Select an object of a floor plan"),
            State::Name => Prompt::new("Enter the floor plan's group name"),
            State::First => Prompt::new("Specify the cut's first point"),
            State::Second(_) => Prompt::new("Specify the cut's second point"),
            State::Place(_) => Prompt::new("Specify the left end of the floor").with_default({
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
            State::First | State::Second(_) | State::Place(_) => ExpectedInput::point(),
        }
    }

    fn base_point(&self) -> Option<Point2> {
        match self.state {
            State::Second(a) => Some(a),
            _ => None,
        }
    }

    fn preview(&self, cursor: Point2, out: &mut Vec<OverlayItem>) {
        match (self.state, &self.plan) {
            (State::Second(a), _) => out.push(OverlayItem::PreviewSegment {
                start: a,
                end: cursor,
            }),
            (State::Place(cut), Some(plan)) => {
                if let Some(cutting) = section(&plan.spec, &plan.layout, cut) {
                    for (_, entity) in entities(
                        &plan.spec,
                        &plan.layout,
                        cut,
                        &cutting,
                        '?',
                        cursor,
                        plan.scale,
                    ) {
                        out.push(OverlayItem::PreviewEntity(entity));
                    }
                }
            }
            _ => {}
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
            (State::First, CommandInput::Point(a)) => State::Second(a),
            (State::Second(a), CommandInput::Point(b)) => {
                if a.distance_to(b) <= 0.0 {
                    ctx.message("The cut's two points must differ.");
                    State::Second(a)
                } else {
                    match self.cut(a, b) {
                        Some(cut) => {
                            let plan = self.plan.as_ref().expect("taken");
                            if section(&plan.spec, &plan.layout, cut).is_none() {
                                ctx.message("The cut misses the building.");
                                State::First
                            } else {
                                State::Place(cut)
                            }
                        }
                        None => State::First,
                    }
                }
            }
            (State::Place(cut), CommandInput::Point(at)) => return self.finish(cut, at, ctx),
            (State::Place(cut), CommandInput::Accept) => {
                let at = self.default_place();
                return self.finish(cut, at, ctx);
            }
            _ => return Step::Continue,
        };
        Step::Continue
    }
}

//! DONUT (#204): a filled ring — a closed polyline of two half-turn
//! bulges on the circle midway between the two diameters, as wide as the
//! gap between them — at each centre given, until Enter. An inside
//! diameter of zero makes a filled disc.

use cad_core::{Point2, Vec2};
use cad_document::PolylineEntity;
use cad_input::ExpectedInput;

use super::ltscale::display;
use crate::{Command, CommandContext, CommandInput, OverlayItem, Prompt, Step};

#[derive(Debug)]
enum State {
    Inside,
    Outside,
    Center,
}

#[derive(Debug)]
pub(crate) struct DonutCommand {
    state: State,
    inside: f64,
    outside: f64,
}

/// The donut as a polyline: a ring of two half turns, as wide as the gap
/// between the diameters. `None` unless the outside is the larger.
pub fn donut(center: Point2, inside: f64, outside: f64) -> Option<PolylineEntity> {
    if !(inside.is_finite() && outside.is_finite() && inside >= 0.0 && outside > inside) {
        return None;
    }
    let radius = (inside + outside) / 4.0;
    let width = (outside - inside) / 2.0;
    Some(
        PolylineEntity::with_bulges(
            vec![
                center - Vec2::new(radius, 0.0),
                center + Vec2::new(radius, 0.0),
            ],
            vec![1.0, 1.0],
            true,
        )
        .with_width(width),
    )
}

impl DonutCommand {
    pub(crate) fn new() -> Self {
        Self {
            state: State::Inside,
            inside: 0.5,
            outside: 1.0,
        }
    }
}

impl Command for DonutCommand {
    fn prompt(&self) -> Prompt {
        match self.state {
            State::Inside => {
                Prompt::new("Specify inside diameter of donut").with_default(display(self.inside))
            }
            State::Outside => {
                Prompt::new("Specify outside diameter of donut").with_default(display(self.outside))
            }
            State::Center => Prompt::new("Specify center of donut").with_default("exit"),
        }
    }

    fn expected(&self) -> ExpectedInput {
        match self.state {
            State::Inside | State::Outside => ExpectedInput::distance(),
            State::Center => ExpectedInput::point(),
        }
    }

    fn step(&mut self, input: CommandInput, ctx: &mut CommandContext<'_>) -> Step {
        match (&self.state, input) {
            (State::Inside, CommandInput::Distance(inside)) => {
                if !(inside.is_finite() && inside >= 0.0) {
                    ctx.message("The inside diameter must be zero or a positive number.");
                } else {
                    self.inside = inside;
                    self.state = State::Outside;
                }
            }
            (State::Inside, CommandInput::Accept) => self.state = State::Outside,
            (State::Outside, CommandInput::Distance(outside)) => {
                if !(outside.is_finite() && outside > self.inside) {
                    ctx.message("The outside diameter must be larger than the inside.");
                } else {
                    self.outside = outside;
                    self.state = State::Center;
                }
            }
            (State::Outside, CommandInput::Accept) => {
                if self.outside > self.inside {
                    self.state = State::Center;
                } else {
                    ctx.message("The outside diameter must be larger than the inside.");
                }
            }
            (State::Center, CommandInput::Point(center)) => {
                if let Some(ring) = donut(center, self.inside, self.outside) {
                    let mut tx = ctx.transaction();
                    tx.insert(ring)
                        .expect("the current layer and linetype exist");
                    tx.commit();
                }
            }
            (State::Center, CommandInput::Accept) => return Step::Finished,
            _ => {}
        }
        Step::Continue
    }

    /// The donut under the pointer.
    fn preview(&self, cursor: Point2, out: &mut Vec<OverlayItem>) {
        if let State::Center = self.state
            && let Some(ring) = donut(cursor, self.inside, self.outside)
        {
            out.push(OverlayItem::PreviewEntity(ring.into()));
        }
    }
}

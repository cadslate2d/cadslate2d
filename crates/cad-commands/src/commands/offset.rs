//! OFFSET: a parallel copy of a line, or a concentric copy of a circle, at a
//! set distance on a chosen side (DESIGN §22). Repeats until Enter.

use cad_core::{EntityId, Point2};
use cad_document::{Entity, OffsetError};
use cad_input::ExpectedInput;

use crate::{Command, CommandContext, CommandInput, OverlayItem, Prompt, Step};

#[derive(Debug)]
enum State {
    Distance,
    Object,
    /// An object is chosen; waiting for the side to offset to.
    Side {
        id: EntityId,
        source: Entity,
    },
}

#[derive(Debug)]
pub(crate) struct OffsetCommand {
    state: State,
    /// The distance in use: the session's last one until another is typed.
    distance: Option<f64>,
}

impl OffsetCommand {
    pub(crate) fn new() -> Self {
        Self {
            state: State::Distance,
            distance: None,
        }
    }
}

/// `5`, `2.5`: as short as the value allows.
fn display(value: f64) -> String {
    let text = format!("{value:.4}");
    text.trim_end_matches('0').trim_end_matches('.').to_owned()
}

fn explain(error: OffsetError) -> String {
    match error {
        OffsetError::Unsupported(kind) => format!("OFFSET does not support {kind}s yet."),
        OffsetError::NoSide => "That point is on the object: pick a point to one side.".to_owned(),
        OffsetError::Collapses => "The offset would leave no circle.".to_owned(),
    }
}

impl Command for OffsetCommand {
    fn start(&mut self, ctx: &mut CommandContext<'_>) -> Step {
        self.distance = ctx.settings.offset_distance;
        Step::Continue
    }

    fn prompt(&self) -> Prompt {
        match (&self.state, self.distance) {
            (State::Distance, Some(distance)) => {
                Prompt::new("Specify offset distance").with_default(display(distance))
            }
            (State::Distance, None) => Prompt::new("Specify offset distance"),
            (State::Object, _) => Prompt::new("Select object to offset"),
            (State::Side { .. }, _) => Prompt::new("Specify point on side to offset"),
        }
    }

    fn expected(&self) -> ExpectedInput {
        match self.state {
            State::Distance => ExpectedInput::distance(),
            State::Object => ExpectedInput::object(),
            State::Side { .. } => ExpectedInput::point(),
        }
    }

    fn step(&mut self, input: CommandInput, ctx: &mut CommandContext<'_>) -> Step {
        match (&self.state, input) {
            (State::Distance, CommandInput::Distance(distance)) => {
                if distance.is_finite() && distance > 0.0 {
                    self.distance = Some(distance);
                    ctx.settings.offset_distance = Some(distance);
                    self.state = State::Object;
                } else {
                    ctx.message("The offset distance must be greater than zero.");
                }
            }
            // Enter takes the remembered distance, if there is one.
            (State::Distance, CommandInput::Accept) => match self.distance {
                Some(_) => self.state = State::Object,
                None => return Step::Finished,
            },
            (State::Object, CommandInput::Pick { entity: id, at }) => {
                let Some(source) = ctx.drawing.entity(id).cloned() else {
                    return Step::Continue;
                };
                // Only "unsupported" can be told before a side is chosen.
                match source.offset(self.distance.unwrap_or_default(), at) {
                    Err(error @ OffsetError::Unsupported(_)) => ctx.message(explain(error)),
                    _ => self.state = State::Side { id, source },
                }
            }
            (State::Side { source, .. }, CommandInput::Point(side)) => {
                match source.offset(self.distance.unwrap_or_default(), side) {
                    Ok(offset) => {
                        let mut transaction = ctx.drawing.transaction();
                        transaction.insert(offset);
                        transaction.commit();
                        self.state = State::Object;
                    }
                    Err(OffsetError::NoSide) => ctx.message(explain(OffsetError::NoSide)),
                    Err(error) => {
                        ctx.message(explain(error));
                        self.state = State::Object;
                    }
                }
            }
            (State::Object | State::Side { .. }, CommandInput::Accept) => return Step::Finished,
            _ => {}
        }
        Step::Continue
    }

    fn preview(&self, cursor: Point2, out: &mut Vec<OverlayItem>) {
        if let State::Side { id, source } = &self.state {
            out.push(OverlayItem::Highlight(*id));
            if let Ok(offset) = source.offset(self.distance.unwrap_or_default(), cursor) {
                out.push(OverlayItem::PreviewEntity(offset));
            }
        }
    }
}

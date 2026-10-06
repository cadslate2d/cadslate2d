//! OFFSET: a parallel copy of a line, a concentric copy of a circle or an
//! arc, or the offset of a polyline, on a chosen side (DESIGN §22, ADR-0011) — at a set distance, or, with the
//! `Through` option, through a picked point. Repeats until Enter.

use cad_core::{EntityId, Point2};
use cad_document::{Entity, OffsetError};
use cad_input::{ExpectedInput, Keyword};

use super::ltscale::display;
use crate::{Command, CommandContext, CommandInput, OverlayItem, Prompt, Step};

const THROUGH: Keyword = Keyword::new("Through");

/// How far the copy is from the original.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Amount {
    /// A fixed distance; the next point only chooses the side.
    Distance(f64),
    /// However far the next point is: the copy passes through it.
    Through,
}

impl Amount {
    /// The offset of `source` that the point `at` asks for: one object, or
    /// several where a polyline's offset falls apart.
    fn apply(self, source: &Entity, at: Point2) -> Result<Vec<Entity>, OffsetError> {
        match self {
            Self::Distance(distance) => source.offset(distance, at),
            Self::Through => source.offset_through(at),
        }
    }
}

#[derive(Debug)]
enum State {
    Distance,
    Object,
    /// An object is chosen; waiting for the point that places the copy.
    Place {
        id: EntityId,
        source: Entity,
    },
}

#[derive(Debug)]
pub(crate) struct OffsetCommand {
    state: State,
    /// What is in use: the session's last distance until something else is given.
    amount: Option<Amount>,
}

impl OffsetCommand {
    pub(crate) fn new() -> Self {
        Self {
            state: State::Distance,
            amount: None,
        }
    }
}

fn explain(error: OffsetError) -> String {
    match error {
        OffsetError::NoSide => "That point is on the object: pick a point to one side.".to_owned(),
        OffsetError::Collapses => "The offset would leave nothing.".to_owned(),
        OffsetError::Unsupported(kind) => format!("A {kind} cannot be offset."),
    }
}

impl Command for OffsetCommand {
    fn start(&mut self, ctx: &mut CommandContext<'_>) -> Step {
        self.amount = ctx.settings.offset_distance.map(Amount::Distance);
        Step::Continue
    }

    fn prompt(&self) -> Prompt {
        match (&self.state, self.amount) {
            (State::Distance, Some(Amount::Distance(distance))) => {
                Prompt::new("Specify offset distance").with_default(display(distance))
            }
            (State::Distance, _) => Prompt::new("Specify offset distance"),
            (State::Object, _) => Prompt::new("Select object to offset"),
            (State::Place { .. }, Some(Amount::Through)) => Prompt::new("Specify through point"),
            (State::Place { .. }, _) => Prompt::new("Specify point on side to offset"),
        }
    }

    fn expected(&self) -> ExpectedInput {
        match self.state {
            State::Distance => ExpectedInput::distance().with_keywords(&[THROUGH]),
            State::Object => ExpectedInput::object(),
            State::Place { .. } => ExpectedInput::point(),
        }
    }

    fn step(&mut self, input: CommandInput, ctx: &mut CommandContext<'_>) -> Step {
        match (&self.state, input) {
            (State::Distance, CommandInput::Distance(distance)) => {
                if distance.is_finite() && distance > 0.0 {
                    self.amount = Some(Amount::Distance(distance));
                    ctx.settings.offset_distance = Some(distance);
                    self.state = State::Object;
                } else {
                    ctx.message("The offset distance must be greater than zero.");
                }
            }
            (State::Distance, CommandInput::Keyword(_)) => {
                self.amount = Some(Amount::Through);
                self.state = State::Object;
            }
            // Enter takes the remembered distance, if there is one.
            (State::Distance, CommandInput::Accept) => match self.amount {
                Some(_) => self.state = State::Object,
                None => return Step::Finished,
            },
            (State::Object, CommandInput::Pick { entity: id, .. }) => {
                if let Some(source) = ctx.drawing.entity(id).cloned() {
                    self.state = State::Place { id, source };
                }
            }
            (State::Place { id, source }, CommandInput::Point(at)) => {
                let Some(amount) = self.amount else {
                    return Step::Continue;
                };
                match amount.apply(source, at) {
                    Ok(offsets) => {
                        let mut transaction = ctx.drawing.transaction();
                        for offset in offsets {
                            // An offset is a copy: it has its source's properties.
                            transaction
                                .insert_like(*id, offset)
                                .expect("the source was picked from the drawing");
                        }
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
            (State::Object | State::Place { .. }, CommandInput::Accept) => {
                return Step::Finished;
            }
            _ => {}
        }
        Step::Continue
    }

    fn preview(&self, cursor: Point2, out: &mut Vec<OverlayItem>) {
        if let (State::Place { id, source }, Some(amount)) = (&self.state, self.amount) {
            out.push(OverlayItem::Highlight(*id));
            if let Ok(offsets) = amount.apply(source, cursor) {
                out.extend(offsets.into_iter().map(OverlayItem::PreviewEntity));
            }
        }
    }
}

//! BREAK: removes the part of an object between two points, or splits it
//! at one (DESIGN §22, §31).
//!
//! The pick that selects the object is also the first break point, unless
//! the `First` option names another. A second point the same as the first —
//! typed as `@` — splits the object there and removes nothing.

use cad_core::{EntityId, Point2};
use cad_document::BreakError;
use cad_input::{ExpectedInput, Keyword};

use crate::{Command, CommandContext, CommandInput, OverlayItem, Prompt, Step};

const FIRST: Keyword = Keyword::new("First");

#[derive(Debug)]
enum State {
    Object,
    /// The `First` option: the pick only chose the object.
    FirstPoint {
        id: EntityId,
    },
    SecondPoint {
        id: EntityId,
        first: Point2,
    },
}

#[derive(Debug)]
pub(crate) struct BreakCommand {
    state: State,
}

impl BreakCommand {
    pub(crate) fn new() -> Self {
        Self {
            state: State::Object,
        }
    }
}

fn explain(error: BreakError) -> String {
    match error {
        BreakError::AtEndpoint => {
            "That is an end of the object: there is nothing to break off.".to_owned()
        }
        BreakError::NothingLeft => {
            "That would remove the whole object; use ERASE for that.".to_owned()
        }
        BreakError::NeedsTwoPoints(kind) => {
            format!("A {kind} has no ends: it needs two different break points.")
        }
    }
}

impl Command for BreakCommand {
    fn prompt(&self) -> Prompt {
        Prompt::new(match self.state {
            State::Object => "Select object",
            State::FirstPoint { .. } => "Specify first break point",
            State::SecondPoint { .. } => "Specify second break point",
        })
    }

    fn expected(&self) -> ExpectedInput {
        match self.state {
            State::Object => ExpectedInput::object(),
            State::FirstPoint { .. } => ExpectedInput::point(),
            State::SecondPoint { .. } => ExpectedInput::point().with_keywords(&[FIRST]),
        }
    }

    fn step(&mut self, input: CommandInput, ctx: &mut CommandContext<'_>) -> Step {
        match (&self.state, input) {
            (State::Object, CommandInput::Pick { entity, at }) => {
                // The first break point is where the object was picked: the
                // nearest point of it, since a pick is rarely exactly on it.
                self.state = State::SecondPoint {
                    id: entity,
                    first: at,
                };
            }
            (State::SecondPoint { id, .. }, CommandInput::Keyword(_)) => {
                self.state = State::FirstPoint { id: *id };
            }
            (State::FirstPoint { id }, CommandInput::Point(first)) => {
                self.state = State::SecondPoint { id: *id, first };
            }
            (State::SecondPoint { id, first }, CommandInput::Point(second)) => {
                let Some(entity) = ctx.drawing.entity(*id) else {
                    return Step::Finished;
                };
                match entity.broken(*first, second) {
                    Ok(pieces) => {
                        let mut pieces = pieces.into_iter();
                        let mut transaction = ctx.drawing.transaction();
                        if let Some(first) = pieces.next() {
                            transaction
                                .replace(*id, first)
                                .expect("the object was just read");
                        }
                        for piece in pieces {
                            transaction.insert(piece);
                        }
                        transaction.commit();
                        return Step::Finished;
                    }
                    Err(error) => ctx.message(explain(error)),
                }
            }
            (_, CommandInput::Accept) => return Step::Finished,
            _ => {}
        }
        Step::Continue
    }

    fn preview(&self, _cursor: Point2, out: &mut Vec<OverlayItem>) {
        match self.state {
            State::FirstPoint { id } | State::SecondPoint { id, .. } => {
                out.push(OverlayItem::Highlight(id));
            }
            State::Object => {}
        }
    }

    /// `@` at the second prompt is the first break point again.
    fn base_point(&self) -> Option<Point2> {
        match self.state {
            State::SecondPoint { first, .. } => Some(first),
            _ => None,
        }
    }

    /// The break points are positions along the object, not ends of a
    /// rubber-band line: ORTHO and POLAR have no say.
    fn tracking_base(&self) -> Option<Point2> {
        None
    }
}

//! MIRROR: reflects a selection set across a line through two points, and
//! asks whether to keep the originals (DESIGN §22).

use cad_core::Point2;
use cad_geometry::Transform;
use cad_input::{ExpectedInput, InputKind, Keyword};

use super::modify::{SELECT_OBJECTS, adopt_selection, selection_expected};
use crate::{Command, CommandContext, CommandInput, OverlayItem, Prompt, Step, TransientEdit};

const YES: Keyword = Keyword::new("Yes");
const NO: Keyword = Keyword::new("No");

#[derive(Debug)]
enum State {
    Selecting,
    FirstPoint,
    SecondPoint {
        first: Point2,
    },
    /// The mirror line is fixed; only the fate of the originals is open.
    EraseSource {
        mirror: Transform,
    },
}

#[derive(Debug)]
pub(crate) struct MirrorCommand {
    state: State,
    edit: TransientEdit,
}

impl MirrorCommand {
    pub(crate) fn new() -> Self {
        Self {
            state: State::Selecting,
            edit: TransientEdit::default(),
        }
    }

    fn adopt_selection(&mut self, ctx: &mut CommandContext<'_>) -> Step {
        match adopt_selection(ctx) {
            Some(edit) => {
                self.edit = edit;
                self.state = State::FirstPoint;
                Step::Continue
            }
            None => Step::Finished,
        }
    }

    fn finish(ctx: &mut CommandContext<'_>) -> Step {
        ctx.selection.clear();
        Step::Finished
    }
}

impl Command for MirrorCommand {
    fn start(&mut self, ctx: &mut CommandContext<'_>) -> Step {
        if ctx.selection.is_empty() {
            Step::Continue
        } else {
            self.adopt_selection(ctx)
        }
    }

    fn prompt(&self) -> Prompt {
        match self.state {
            State::Selecting => Prompt::new(SELECT_OBJECTS),
            State::FirstPoint => Prompt::new("Specify first point of mirror line"),
            State::SecondPoint { .. } => Prompt::new("Specify second point of mirror line"),
            State::EraseSource { .. } => Prompt::new("Erase source objects?").with_default("No"),
        }
    }

    fn expected(&self) -> ExpectedInput {
        match self.state {
            State::Selecting => selection_expected(),
            State::FirstPoint => ExpectedInput::point(),
            State::SecondPoint { .. } => ExpectedInput::point().with_direct_distance(),
            State::EraseSource { .. } => {
                ExpectedInput::new(InputKind::Nothing).with_keywords(&[YES, NO])
            }
        }
    }

    fn step(&mut self, input: CommandInput, ctx: &mut CommandContext<'_>) -> Step {
        match (&self.state, input) {
            (State::Selecting, CommandInput::Accept) => return self.adopt_selection(ctx),
            (State::FirstPoint, CommandInput::Point(first)) => {
                self.state = State::SecondPoint { first };
            }
            (State::SecondPoint { first }, CommandInput::Point(second)) => {
                match Transform::mirror(*first, second) {
                    Some(mirror) => self.state = State::EraseSource { mirror },
                    None => ctx.message("The mirror line needs two different points."),
                }
            }
            (State::EraseSource { mirror }, CommandInput::Keyword(keyword)) if keyword == YES => {
                self.edit.commit(ctx.drawing, *mirror);
                return Self::finish(ctx);
            }
            // No, or Enter for the default: keep the originals.
            (State::EraseSource { mirror }, CommandInput::Keyword(_) | CommandInput::Accept) => {
                self.edit.commit_copies(ctx.drawing, *mirror);
                return Self::finish(ctx);
            }
            (State::FirstPoint | State::SecondPoint { .. }, CommandInput::Accept) => {
                return Self::finish(ctx);
            }
            _ => {}
        }
        Step::Continue
    }

    fn preview(&self, cursor: Point2, out: &mut Vec<OverlayItem>) {
        match self.state {
            State::SecondPoint { first } => {
                out.push(OverlayItem::PreviewSegment {
                    start: first,
                    end: cursor,
                });
                if let Some(mirror) = Transform::mirror(first, cursor) {
                    self.edit.preview(mirror, out);
                }
            }
            State::EraseSource { mirror } => self.edit.preview(mirror, out),
            State::Selecting | State::FirstPoint => {}
        }
    }

    fn base_point(&self) -> Option<Point2> {
        match self.state {
            State::SecondPoint { first } => Some(first),
            _ => None,
        }
    }
}

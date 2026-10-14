//! ALIGN (#201): a selection moved by one pair of points, or moved and
//! turned by two — scaled too, if asked, so the second pair meets — as
//! one transform (`Transform::aligning`), previewed and committed once.

use cad_core::{EntityId, Point2};
use cad_geometry::Transform;
use cad_input::{ExpectedInput, InputKind, Keyword};

use super::modify::{SELECT_OBJECTS, adopt_selection, selection_expected};
use crate::{Command, CommandContext, CommandInput, OverlayItem, Prompt, Step, TransientEdit};

const YES: Keyword = Keyword::new("Yes");
const NO: Keyword = Keyword::new("No");

#[derive(Debug)]
enum State {
    Selecting,
    FirstSource,
    FirstDestination {
        a1: Point2,
    },
    SecondSource {
        a1: Point2,
        b1: Point2,
    },
    SecondDestination {
        a1: Point2,
        b1: Point2,
        a2: Point2,
    },
    Scale {
        a1: Point2,
        b1: Point2,
        a2: Point2,
        b2: Point2,
    },
}

#[derive(Debug)]
pub(crate) struct AlignCommand {
    state: State,
    edit: TransientEdit,
}

impl AlignCommand {
    pub(crate) fn new() -> Self {
        Self {
            state: State::Selecting,
            edit: TransientEdit::default(),
        }
    }

    fn adopt(&mut self, ctx: &mut CommandContext<'_>) -> Step {
        match adopt_selection(ctx) {
            Some(edit) => {
                self.edit = edit;
                self.state = State::FirstSource;
                Step::Continue
            }
            None => Step::Finished,
        }
    }

    fn commit(&self, ctx: &mut CommandContext<'_>, transform: Transform) -> Step {
        self.edit.commit(ctx.drawing, transform);
        ctx.selection.clear();
        ctx.message(format!(
            "{} aligned.",
            match self.edit.sources().len() {
                1 => "1 object".to_owned(),
                n => format!("{n} objects"),
            }
        ));
        Step::Finished
    }
}

impl Command for AlignCommand {
    fn start(&mut self, ctx: &mut CommandContext<'_>) -> Step {
        if ctx.selection.is_empty() {
            Step::Continue
        } else {
            self.adopt(ctx)
        }
    }

    fn prompt(&self) -> Prompt {
        match &self.state {
            State::Selecting => Prompt::new(SELECT_OBJECTS),
            State::FirstSource => Prompt::new("Specify first source point"),
            State::FirstDestination { .. } => Prompt::new("Specify first destination point"),
            State::SecondSource { .. } => {
                Prompt::new("Specify second source point").with_default("continue")
            }
            State::SecondDestination { .. } => Prompt::new("Specify second destination point"),
            State::Scale { .. } => {
                Prompt::new("Scale objects based on alignment points?").with_default("No")
            }
        }
    }

    fn expected(&self) -> ExpectedInput {
        match &self.state {
            State::Selecting => selection_expected(),
            State::Scale { .. } => ExpectedInput::new(InputKind::Nothing).with_keywords(&[YES, NO]),
            _ => ExpectedInput::point(),
        }
    }

    fn step(&mut self, input: CommandInput, ctx: &mut CommandContext<'_>) -> Step {
        match (&self.state, input) {
            (State::Selecting, CommandInput::Accept) => return self.adopt(ctx),
            (State::Selecting, _) => {}
            (State::FirstSource, CommandInput::Point(a1)) => {
                self.state = State::FirstDestination { a1 };
            }
            (State::FirstDestination { a1 }, CommandInput::Point(b1)) => {
                self.state = State::SecondSource { a1: *a1, b1 };
            }
            // One pair: a move.
            (State::SecondSource { a1, b1 }, CommandInput::Accept) => {
                let transform = Transform::translation(*b1 - *a1);
                return self.commit(ctx, transform);
            }
            (State::SecondSource { a1, b1 }, CommandInput::Point(a2)) => {
                if a2.distance_to(*a1) == 0.0 {
                    ctx.message("The second source point must differ from the first.");
                } else {
                    self.state = State::SecondDestination {
                        a1: *a1,
                        b1: *b1,
                        a2,
                    };
                }
            }
            (State::SecondDestination { a1, b1, a2 }, CommandInput::Point(b2)) => {
                if b2.distance_to(*b1) == 0.0 {
                    ctx.message("The second destination point must differ from the first.");
                } else {
                    self.state = State::Scale {
                        a1: *a1,
                        b1: *b1,
                        a2: *a2,
                        b2,
                    };
                }
            }
            (State::Scale { a1, b1, a2, b2 }, CommandInput::Keyword(keyword)) => {
                let Some(transform) = Transform::aligning(*a1, *a2, *b1, *b2, keyword == YES)
                else {
                    ctx.message("Those points give no alignment.");
                    return Step::Finished;
                };
                return self.commit(ctx, transform);
            }
            (State::Scale { a1, b1, a2, b2 }, CommandInput::Accept) => {
                let Some(transform) = Transform::aligning(*a1, *a2, *b1, *b2, false) else {
                    ctx.message("Those points give no alignment.");
                    return Step::Finished;
                };
                return self.commit(ctx, transform);
            }
            (_, CommandInput::Accept) => {
                ctx.selection.clear();
                return Step::Finished;
            }
            _ => {}
        }
        Step::Continue
    }

    /// The selection where the points given so far — and the pointer, for
    /// the one being asked — would put it.
    fn preview(&self, cursor: Point2, out: &mut Vec<OverlayItem>) {
        let transform = match &self.state {
            State::FirstDestination { a1 } => {
                out.push(OverlayItem::PreviewSegment {
                    start: *a1,
                    end: cursor,
                });
                Some(Transform::translation(cursor - *a1))
            }
            State::SecondSource { a1, b1 } => Some(Transform::translation(*b1 - *a1)),
            State::SecondDestination { a1, b1, a2 } => {
                out.push(OverlayItem::PreviewSegment {
                    start: *b1,
                    end: cursor,
                });
                Transform::aligning(*a1, *a2, *b1, cursor, false)
            }
            State::Scale { a1, b1, a2, b2 } => Transform::aligning(*a1, *a2, *b1, *b2, false),
            _ => None,
        };
        if let Some(transform) = transform {
            self.edit.preview(transform, out);
        }
    }

    fn base_point(&self) -> Option<Point2> {
        match &self.state {
            State::FirstDestination { a1 } => Some(*a1),
            State::SecondDestination { b1, .. } => Some(*b1),
            _ => None,
        }
    }

    fn transient_sources(&self) -> &[EntityId] {
        match &self.state {
            State::Selecting | State::FirstSource => &[],
            _ => self.edit.sources(),
        }
    }
}

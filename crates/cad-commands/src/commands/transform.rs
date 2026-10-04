//! MOVE, COPY, ROTATE, SCALE: commands that transform a selection set from a
//! base point (DESIGN §22).
//!
//! All four run the same steps — select, base point, one more input — and
//! differ only in what that last input is and what it does to the selection.
//! While the last input is being given, the result is previewed in the
//! overlay; the document changes once, in one transaction, on commit (§17).

use cad_core::{EntityId, Point2};
use cad_geometry::Transform;
use cad_input::ExpectedInput;

use super::modify::{SELECT_OBJECTS, adopt_selection, selection_expected};
use crate::command::angle_degrees;
use crate::{Command, CommandContext, CommandInput, OverlayItem, Prompt, Step, TransientEdit};

const BAD_SCALE_FACTOR: &str = "The scale factor must be greater than zero.";

/// What the command does with the selection once it has a base point.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Operation {
    /// Moves the originals by the displacement to a second point.
    Move,
    /// Leaves the originals and inserts copies; repeats until Enter.
    Copy,
    /// Rotates the originals about the base point.
    Rotate,
    /// Scales the originals uniformly about the base point.
    Scale,
}

impl Operation {
    /// The request for the input after the base point.
    fn prompt(self) -> &'static str {
        match self {
            Self::Move | Self::Copy => "Specify second point",
            Self::Rotate => "Specify rotation angle",
            Self::Scale => "Specify scale factor",
        }
    }

    fn expected(self) -> ExpectedInput {
        match self {
            Self::Move | Self::Copy => ExpectedInput::point().with_direct_distance(),
            Self::Rotate => ExpectedInput::angle(),
            Self::Scale => ExpectedInput::distance(),
        }
    }

    /// The transform an input asks for. `Err(Some(_))` explains an input of
    /// the right kind that cannot be used; `Err(None)` is an input this step
    /// does not take.
    fn transform(
        self,
        base: Point2,
        input: CommandInput,
    ) -> Result<Transform, Option<&'static str>> {
        match (self, input) {
            (Self::Move | Self::Copy, CommandInput::Point(second)) => {
                Ok(Transform::translation(second - base))
            }
            (Self::Rotate, CommandInput::Angle(degrees)) => Ok(Transform::rotation(base, degrees)),
            (Self::Scale, CommandInput::Distance(factor)) => {
                Transform::scaling(base, factor).ok_or(Some(BAD_SCALE_FACTOR))
            }
            _ => Err(None),
        }
    }

    /// The input a click at `cursor` would be: the session measures a point
    /// against the base point the same way where an angle or a distance is
    /// expected.
    fn input_at(self, base: Point2, cursor: Point2) -> CommandInput {
        let offset = cursor - base;
        match self {
            Self::Move | Self::Copy => CommandInput::Point(cursor),
            Self::Rotate => CommandInput::Angle(angle_degrees(offset)),
            Self::Scale => CommandInput::Distance(offset.length()),
        }
    }

    fn keeps_sources(self) -> bool {
        self == Self::Copy
    }
}

#[derive(Debug)]
enum State {
    Selecting,
    BasePoint,
    /// Waiting for the input that completes the transform.
    Parameter {
        base: Point2,
    },
}

#[derive(Debug)]
pub(crate) struct TransformCommand {
    operation: Operation,
    state: State,
    edit: TransientEdit,
}

impl TransformCommand {
    pub(crate) fn moving() -> Self {
        Self::new(Operation::Move)
    }

    pub(crate) fn copying() -> Self {
        Self::new(Operation::Copy)
    }

    pub(crate) fn rotating() -> Self {
        Self::new(Operation::Rotate)
    }

    pub(crate) fn scaling() -> Self {
        Self::new(Operation::Scale)
    }

    fn new(operation: Operation) -> Self {
        Self {
            operation,
            state: State::Selecting,
            edit: TransientEdit::default(),
        }
    }

    /// Fixes the selection as the edit's sources and asks for the base point.
    fn adopt_selection(&mut self, ctx: &mut CommandContext<'_>) -> Step {
        match adopt_selection(ctx) {
            Some(edit) => {
                self.edit = edit;
                self.state = State::BasePoint;
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

impl Command for TransformCommand {
    fn start(&mut self, ctx: &mut CommandContext<'_>) -> Step {
        if ctx.selection.is_empty() {
            Step::Continue
        } else {
            self.adopt_selection(ctx)
        }
    }

    fn prompt(&self) -> Prompt {
        Prompt::new(match self.state {
            State::Selecting => SELECT_OBJECTS,
            State::BasePoint => "Specify base point",
            State::Parameter { .. } => self.operation.prompt(),
        })
    }

    fn expected(&self) -> ExpectedInput {
        match self.state {
            State::Selecting => selection_expected(),
            State::BasePoint => ExpectedInput::point(),
            State::Parameter { .. } => self.operation.expected(),
        }
    }

    fn step(&mut self, input: CommandInput, ctx: &mut CommandContext<'_>) -> Step {
        match (&self.state, input) {
            (State::Selecting, CommandInput::Accept) => return self.adopt_selection(ctx),
            (State::BasePoint, CommandInput::Point(base)) => {
                self.state = State::Parameter { base };
            }
            (State::BasePoint | State::Parameter { .. }, CommandInput::Accept) => {
                return Self::finish(ctx);
            }
            (State::Parameter { base }, input) => match self.operation.transform(*base, input) {
                Ok(transform) if self.operation.keeps_sources() => {
                    self.edit.commit_copies(ctx.drawing, transform);
                }
                Ok(transform) => {
                    self.edit.commit(ctx.drawing, transform);
                    return Self::finish(ctx);
                }
                Err(Some(message)) => ctx.message(message),
                Err(None) => {}
            },
            _ => {}
        }
        Step::Continue
    }

    fn preview(&self, cursor: Point2, out: &mut Vec<OverlayItem>) {
        if let State::Parameter { base } = self.state {
            out.push(OverlayItem::PreviewSegment {
                start: base,
                end: cursor,
            });
            let at_cursor = self.operation.input_at(base, cursor);
            if let Ok(transform) = self.operation.transform(base, at_cursor) {
                self.edit.preview(transform, out);
            }
        }
    }

    fn base_point(&self) -> Option<Point2> {
        match self.state {
            State::Parameter { base } => Some(base),
            State::Selecting | State::BasePoint => None,
        }
    }

    fn transient_sources(&self) -> &[EntityId] {
        match &self.state {
            // Copies keep their sources in place; everything else leaves
            // stale geometry behind while it is previewed elsewhere.
            State::Parameter { .. } if !self.operation.keeps_sources() => self.edit.sources(),
            _ => &[],
        }
    }
}

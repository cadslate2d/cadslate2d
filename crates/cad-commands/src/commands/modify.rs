//! ERASE, MOVE, COPY: commands that act on a selection set.
//!
//! Each accepts a selection made beforehand (noun–verb) or asks for one
//! (verb–noun). Building the set is the session's job — clicks, windows, and
//! crossing boxes work the same at the idle prompt and at `Select objects:` —
//! so these state machines only say when they want one.

use cad_core::{EntityId, Point2};
use cad_input::{ExpectedInput, InputKind};

use crate::{Command, CommandContext, CommandInput, OverlayItem, Prompt, Step, TransientEdit};

const SELECT_OBJECTS: &str = "Select objects";
const NOTHING_SELECTED: &str = "No objects selected.";

fn selection_expected() -> ExpectedInput {
    ExpectedInput::new(InputKind::Selection)
}

// ── ERASE ──

#[derive(Debug)]
pub(crate) struct EraseCommand;

impl EraseCommand {
    fn erase(ctx: &mut CommandContext<'_>) -> Step {
        let ids: Vec<EntityId> = ctx.selection.iter().collect();
        if ids.is_empty() {
            ctx.message(NOTHING_SELECTED);
            return Step::Finished;
        }
        let mut transaction = ctx.drawing.transaction();
        for id in &ids {
            transaction
                .remove(*id)
                .expect("selected entities are live and appear once");
        }
        transaction.commit();
        ctx.message(format!("{} erased.", ids.len()));
        Step::Finished
    }
}

impl Command for EraseCommand {
    fn start(&mut self, ctx: &mut CommandContext<'_>) -> Step {
        if ctx.selection.is_empty() {
            Step::Continue
        } else {
            Self::erase(ctx)
        }
    }

    fn prompt(&self) -> Prompt {
        Prompt::new(SELECT_OBJECTS)
    }

    fn expected(&self) -> ExpectedInput {
        selection_expected()
    }

    fn step(&mut self, input: CommandInput, ctx: &mut CommandContext<'_>) -> Step {
        match input {
            CommandInput::Accept => Self::erase(ctx),
            _ => Step::Continue,
        }
    }
}

// ── MOVE and COPY ──

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Placement {
    /// Move the originals; one placement ends the command.
    Move,
    /// Leave the originals and insert copies; repeats until Enter.
    Copy,
}

#[derive(Debug)]
enum State {
    Selecting,
    BasePoint,
    SecondPoint { base: Point2 },
}

/// MOVE and COPY share every step except what a placement does.
#[derive(Debug)]
pub(crate) struct PlaceCommand {
    placement: Placement,
    state: State,
    edit: TransientEdit,
}

impl PlaceCommand {
    pub(crate) fn moving() -> Self {
        Self::new(Placement::Move)
    }

    pub(crate) fn copying() -> Self {
        Self::new(Placement::Copy)
    }

    fn new(placement: Placement) -> Self {
        Self {
            placement,
            state: State::Selecting,
            edit: TransientEdit::default(),
        }
    }

    /// Fixes the selection as the edit's sources and asks for the base point.
    fn adopt_selection(&mut self, ctx: &mut CommandContext<'_>) -> Step {
        self.edit = TransientEdit::of(ctx.selection);
        if self.edit.is_empty() {
            ctx.message(NOTHING_SELECTED);
            return Step::Finished;
        }
        self.state = State::BasePoint;
        Step::Continue
    }

    fn finish(ctx: &mut CommandContext<'_>) -> Step {
        ctx.selection.clear();
        Step::Finished
    }
}

impl Command for PlaceCommand {
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
            State::SecondPoint { .. } => "Specify second point",
        })
    }

    fn expected(&self) -> ExpectedInput {
        match self.state {
            State::Selecting => selection_expected(),
            State::BasePoint => ExpectedInput::point(),
            State::SecondPoint { .. } => ExpectedInput::point().with_direct_distance(),
        }
    }

    fn step(&mut self, input: CommandInput, ctx: &mut CommandContext<'_>) -> Step {
        match (&self.state, input) {
            (State::Selecting, CommandInput::Accept) => return self.adopt_selection(ctx),
            (State::BasePoint, CommandInput::Point(base)) => {
                self.state = State::SecondPoint { base }
            }
            (State::SecondPoint { base }, CommandInput::Point(second)) => {
                let offset = second - *base;
                match self.placement {
                    Placement::Move => {
                        self.edit.commit_translated(ctx.drawing, offset);
                        return Self::finish(ctx);
                    }
                    Placement::Copy => {
                        self.edit.commit_copies(ctx.drawing, offset);
                    }
                }
            }
            (State::BasePoint | State::SecondPoint { .. }, CommandInput::Accept) => {
                return Self::finish(ctx);
            }
            _ => {}
        }
        Step::Continue
    }

    fn preview(&self, cursor: Point2, out: &mut Vec<OverlayItem>) {
        if let State::SecondPoint { base } = self.state {
            out.push(OverlayItem::PreviewSegment {
                start: base,
                end: cursor,
            });
            self.edit.preview_translated(cursor - base, out);
        }
    }

    fn base_point(&self) -> Option<Point2> {
        match self.state {
            State::SecondPoint { base } => Some(base),
            State::Selecting | State::BasePoint => None,
        }
    }

    fn transient_sources(&self) -> &[EntityId] {
        match (self.placement, &self.state) {
            // Only a move leaves stale geometry behind; copies keep their sources in place.
            (Placement::Move, State::SecondPoint { .. }) => self.edit.sources(),
            _ => &[],
        }
    }
}

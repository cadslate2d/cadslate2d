//! ERASE, and what every command that acts on a selection set shares.
//!
//! Such a command accepts a selection made beforehand (noun–verb) or asks for
//! one (verb–noun). Building the set is the session's job — clicks, windows, and
//! crossing boxes work the same at the idle prompt and at `Select objects:` —
//! so these state machines only say when they want one.

use cad_core::EntityId;
use cad_input::{ExpectedInput, InputKind};

use crate::{Command, CommandContext, CommandInput, Prompt, Step, TransientEdit};

pub(super) const SELECT_OBJECTS: &str = "Select objects";
const NOTHING_SELECTED: &str = "No objects selected.";

pub(super) fn selection_expected() -> ExpectedInput {
    ExpectedInput::new(InputKind::Selection)
}

/// The current selection as the sources of an edit. With nothing selected
/// there is nothing to edit: says so and returns `None`.
pub(super) fn adopt_selection(ctx: &mut CommandContext<'_>) -> Option<TransientEdit> {
    let edit = TransientEdit::of(ctx.selection);
    if edit.is_empty() {
        ctx.message(NOTHING_SELECTED);
        return None;
    }
    Some(edit)
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

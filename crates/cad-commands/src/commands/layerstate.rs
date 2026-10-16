//! LAYERSTATE (#228): the layers saved by name, and switched back in one
//! step.
//!
//! `LAYERSTATE` (`LAS`) offers `Save` — the layers as they are, under a
//! name, over a state of that name if there is one — `Restore`, which sets
//! every layer the state names and leaves the rest, `Delete`, `REName`,
//! and `?` to list. What a state keeps and how it is restored are
//! `cad-document`'s; this is the conversation. Each is one undo.

use cad_document::{LayerStateId, TransactionError};
use cad_input::{ExpectedInput, InputKind, Keyword};

use crate::{Command, CommandContext, CommandInput, Prompt, Step};

const LIST: Keyword = Keyword::new("?");
const SAVE: Keyword = Keyword::new("Save");
const RESTORE: Keyword = Keyword::new("Restore");
const DELETE: Keyword = Keyword::new("Delete");
const RENAME: Keyword = Keyword::new("REName");
const OPTIONS: [Keyword; 5] = [LIST, SAVE, RESTORE, DELETE, RENAME];

#[derive(Debug)]
enum State {
    Option,
    /// The name to save under, restore, delete, or rename.
    Name(Keyword),
    NewName(LayerStateId),
}

#[derive(Debug)]
pub(crate) struct LayerStateCommand {
    state: State,
}

impl LayerStateCommand {
    pub(crate) fn new() -> Self {
        Self {
            state: State::Option,
        }
    }
}

/// The state a typed name names, or the message that there is none.
fn find(ctx: &CommandContext<'_>, name: &str) -> Result<LayerStateId, String> {
    ctx.drawing
        .layer_states()
        .find(name)
        .ok_or_else(|| format!("There is no layer state \"{name}\"."))
}

/// Why the table refused a name, in the user's terms.
fn explain(error: TransactionError) -> String {
    match error {
        TransactionError::BadLayerStateName(error) => error.explain("layer state"),
        TransactionError::DuplicateLayerStateName(_) => {
            "There is already a layer state of that name.".to_owned()
        }
        other => other.to_string(),
    }
}

fn layers(count: usize) -> String {
    if count == 1 {
        "1 layer".to_owned()
    } else {
        format!("{count} layers")
    }
}

impl Command for LayerStateCommand {
    fn prompt(&self) -> Prompt {
        match self.state {
            State::Option => Prompt::new("Enter an option").with_default("Restore"),
            State::Name(SAVE) => Prompt::new("Enter new layer state name"),
            State::Name(_) => Prompt::new("Enter name of layer state"),
            State::NewName(_) => Prompt::new("Enter its new name"),
        }
    }

    fn expected(&self) -> ExpectedInput {
        match self.state {
            State::Option => ExpectedInput::new(InputKind::Nothing).with_keywords(&OPTIONS),
            State::Name(_) | State::NewName(_) => ExpectedInput::text(),
        }
    }

    fn step(&mut self, input: CommandInput, ctx: &mut CommandContext<'_>) -> Step {
        let state = std::mem::replace(&mut self.state, State::Option);
        self.state = match (state, input) {
            (State::Option, CommandInput::Keyword(LIST)) => {
                let lines: Vec<String> = ctx
                    .drawing
                    .layer_states()
                    .iter()
                    .map(|(_, state)| {
                        format!("  \"{}\": {}", state.name, layers(state.layers.len()))
                    })
                    .collect();
                if lines.is_empty() {
                    ctx.message("The drawing has no layer states.");
                }
                for line in lines {
                    ctx.message(line);
                }
                return Step::Finished;
            }
            (State::Option, CommandInput::Keyword(option)) => State::Name(option),
            (State::Option, CommandInput::Accept) => State::Name(RESTORE),
            (State::Name(_), CommandInput::Accept) => return Step::Finished,
            (State::Name(SAVE), CommandInput::Text(name)) => {
                let name = name.trim().to_owned();
                let saved = ctx.drawing.layer_state_now(name.clone());
                let count = saved.layers.len();
                let existing = ctx.drawing.layer_states().find(&name);
                let mut transaction = ctx.transaction();
                let done = match existing {
                    Some(id) => transaction.replace_layer_state(id, saved),
                    None => transaction.add_layer_state(saved).map(|_| ()),
                };
                match done {
                    Ok(()) => {
                        transaction.commit();
                        ctx.message(format!("Layer state \"{name}\" saved: {}.", layers(count)));
                        return Step::Finished;
                    }
                    Err(error) => {
                        drop(transaction);
                        ctx.message(explain(error));
                        State::Name(SAVE)
                    }
                }
            }
            (State::Name(option), CommandInput::Text(name)) => match find(ctx, name.trim()) {
                Err(message) => {
                    ctx.message(message);
                    State::Name(option)
                }
                Ok(id) if option == RENAME => State::NewName(id),
                Ok(id) => {
                    let name = name.trim().to_owned();
                    let mut transaction = ctx.transaction();
                    let done = if option == DELETE {
                        transaction
                            .remove_layer_state(id)
                            .map(|()| format!("Layer state \"{name}\" deleted."))
                    } else {
                        transaction.restore_layer_state(id).map(|count| {
                            format!(
                                "Layer state \"{name}\" restored: {} changed.",
                                layers(count)
                            )
                        })
                    };
                    match done {
                        Ok(message) => {
                            transaction.commit();
                            ctx.message(message);
                        }
                        Err(error) => {
                            drop(transaction);
                            ctx.message(explain(error));
                        }
                    }
                    return Step::Finished;
                }
            },
            (State::NewName(id), CommandInput::Text(name)) => {
                let Some(before) = ctx.drawing.layer_states().get(id).cloned() else {
                    return Step::Finished;
                };
                let name = name.trim().to_owned();
                let mut transaction = ctx.transaction();
                let renamed = cad_document::LayerState {
                    name: name.clone(),
                    ..before.clone()
                };
                match transaction.replace_layer_state(id, renamed) {
                    Ok(()) => {
                        transaction.commit();
                        ctx.message(format!(
                            "Layer state \"{}\" renamed \"{name}\".",
                            before.name
                        ));
                        return Step::Finished;
                    }
                    Err(error) => {
                        drop(transaction);
                        ctx.message(explain(error));
                        State::NewName(id)
                    }
                }
            }
            (State::NewName(_), CommandInput::Accept) => return Step::Finished,
            (state, _) => state,
        };
        Step::Continue
    }
}

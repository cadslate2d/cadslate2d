//! VIEW (#235): places in the drawing saved by name and gone back to.
//!
//! `VIEW` (`V`) offers `Save` — what the screen shows now, of the current
//! space, under a name, over a view of that name if there is one —
//! `Restore`, which goes to the view's space and shows it, `Delete`, and
//! `?` to list. Saving and deleting are drawing changes, each one undo;
//! restoring moves the view, which is the session's: `ZOOM Previous`
//! comes back from it, and `U` does not.

use cad_document::{NamedView, TransactionError};
use cad_input::{ExpectedInput, InputKind, Keyword};

use crate::{Command, CommandContext, CommandInput, Prompt, SpaceRequest, Step};

const LIST: Keyword = Keyword::new("?");
const SAVE: Keyword = Keyword::new("Save");
const RESTORE: Keyword = Keyword::new("Restore");
const DELETE: Keyword = Keyword::new("Delete");
const OPTIONS: [Keyword; 4] = [LIST, SAVE, RESTORE, DELETE];

#[derive(Debug)]
enum State {
    Option,
    Name(Keyword),
}

#[derive(Debug)]
pub(crate) struct ViewCommand {
    state: State,
}

impl ViewCommand {
    pub(crate) fn new() -> Self {
        Self {
            state: State::Option,
        }
    }
}

fn explain(error: TransactionError) -> String {
    match error {
        TransactionError::BadViewName(error) => error.explain("view"),
        other => other.to_string(),
    }
}

impl Command for ViewCommand {
    fn prompt(&self) -> Prompt {
        match self.state {
            State::Option => Prompt::new("Enter an option").with_default("Restore"),
            State::Name(SAVE) => Prompt::new("Enter view name to save"),
            State::Name(_) => Prompt::new("Enter view name"),
        }
    }

    fn expected(&self) -> ExpectedInput {
        match self.state {
            State::Option => ExpectedInput::new(InputKind::Nothing).with_keywords(&OPTIONS),
            State::Name(_) => ExpectedInput::text(),
        }
    }

    fn step(&mut self, input: CommandInput, ctx: &mut CommandContext<'_>) -> Step {
        let state = std::mem::replace(&mut self.state, State::Option);
        self.state = match (state, input) {
            (State::Option, CommandInput::Keyword(LIST)) => {
                let lines: Vec<String> = ctx
                    .drawing
                    .views()
                    .iter()
                    .map(|(_, view)| {
                        let space = ctx
                            .drawing
                            .layouts()
                            .get(view.space)
                            .map_or("?", |layout| layout.name.as_str());
                        format!("  \"{}\" on {space}", view.name)
                    })
                    .collect();
                if lines.is_empty() {
                    ctx.message("The drawing has no named views.");
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
                let view = NamedView::of(name.clone(), ctx.space, &ctx.view.visible_bounds());
                let existing = ctx.drawing.views().find(&name);
                let mut transaction = ctx.transaction();
                let saved = match existing {
                    Some(id) => transaction.replace_view(id, view),
                    None => transaction.add_view(view).map(|_| ()),
                };
                match saved {
                    Ok(()) => {
                        transaction.commit();
                        ctx.message(format!("View \"{name}\" saved."));
                        return Step::Finished;
                    }
                    Err(error) => {
                        drop(transaction);
                        ctx.message(explain(error));
                        State::Name(SAVE)
                    }
                }
            }
            (State::Name(option), CommandInput::Text(name)) => {
                let name = name.trim().to_owned();
                let Some(id) = ctx.drawing.views().find(&name) else {
                    ctx.message(format!("There is no view \"{name}\"."));
                    self.state = State::Name(option);
                    return Step::Continue;
                };
                if option == DELETE {
                    let mut transaction = ctx.transaction();
                    transaction
                        .remove_view(id)
                        .expect("a view of the drawing, removed once");
                    transaction.commit();
                    ctx.message(format!("View \"{name}\" deleted."));
                } else if let Some(view) = ctx.drawing.views().get(id) {
                    ctx.request_space(SpaceRequest::Show {
                        space: view.space,
                        frame: view.frame(),
                    });
                    ctx.message(format!("View \"{name}\" restored."));
                }
                return Step::Finished;
            }
            (state, _) => state,
        };
        Step::Continue
    }
}

//! LINETYPE: loads linetypes and sets the one new objects are given
//! (DESIGN §23.2, §32; ADR-0015).
//!
//! Like LAYER, the command stays at its option prompt until Enter. The
//! linetypes that can be loaded are the built-in patterns; what the table
//! allows is decided by `cad-document`, and this is the conversation.

use cad_document::{
    Linetype, LinetypeDef, LinetypeId, TransactionBuilder, TransactionError, same_name,
};
use cad_input::{ExpectedInput, InputKind, Keyword};

use crate::{Command, CommandContext, CommandInput, Prompt, Step};

const LIST: Keyword = Keyword::new("?");
const LOAD: Keyword = Keyword::new("Load");
const SET: Keyword = Keyword::new("Set");
const DELETE: Keyword = Keyword::new("Delete");
const OPTIONS: [Keyword; 4] = [LIST, LOAD, SET, DELETE];

#[derive(Debug)]
enum State {
    Option,
    Load,
    Set,
    Delete,
}

#[derive(Debug)]
pub(crate) struct LinetypeCommand {
    state: State,
}

/// "1 linetype", "2 linetypes".
fn linetypes(count: usize) -> String {
    if count == 1 {
        "1 linetype".to_owned()
    } else {
        format!("{count} linetypes")
    }
}

/// The names in a comma-separated list, trimmed, without repeats.
fn names(text: &str) -> Vec<&str> {
    let mut names: Vec<&str> = Vec::new();
    for name in text.split(',').map(str::trim) {
        if !names.iter().any(|seen| same_name(seen, name)) {
            names.push(name);
        }
    }
    names
}

/// The names of the built-in linetypes, for the prompt.
pub(super) fn built_in_names() -> String {
    LinetypeDef::built_in()
        .iter()
        .map(|def| def.name.as_str())
        .collect::<Vec<_>>()
        .join(", ")
}

/// Why a linetype typed by name is not one that can be used: it is a
/// built-in that has not been loaded, or nothing at all.
pub(super) fn unknown_linetype(name: &str) -> String {
    if LinetypeDef::built_in_named(name).is_some() {
        format!("Linetype \"{name}\" is not loaded: load it with LINETYPE first.")
    } else {
        format!("There is no linetype \"{name}\".")
    }
}

/// Why the linetype table refused a change, in the user's terms.
fn explain(ctx: &CommandContext<'_>, error: TransactionError) -> String {
    let name = |id: LinetypeId| {
        ctx.drawing
            .linetypes()
            .get(id)
            .map_or_else(|| id.to_string(), |def| def.name.clone())
    };
    match error {
        TransactionError::BadLinetype(error) => error.to_string(),
        TransactionError::DuplicateLinetypeName(id) => {
            format!("Linetype \"{}\" is already loaded.", name(id))
        }
        TransactionError::PermanentLinetype => {
            "Linetype \"Continuous\" cannot be deleted.".to_owned()
        }
        TransactionError::LinetypeInUse(id) => format!(
            "Linetype \"{}\" is in use: by an object, a layer, or as the linetype for new objects.",
            name(id)
        ),
        other => other.to_string(),
    }
}

impl LinetypeCommand {
    pub(crate) fn new() -> Self {
        Self {
            state: State::Option,
        }
    }

    fn report_current(ctx: &mut CommandContext<'_>) {
        let current = ctx
            .drawing
            .current_linetype()
            .describe(ctx.drawing.linetypes());
        ctx.message(format!("Current linetype: \"{current}\""));
    }

    /// The `?` option: one line per loaded linetype, in the order loaded.
    fn list(ctx: &mut CommandContext<'_>) {
        let current = ctx.drawing.current_linetype();
        let lines: Vec<String> = ctx
            .drawing
            .linetypes()
            .iter()
            .map(|(id, def)| {
                let mut line = format!("  \"{}\": {}", def.name, def.description);
                if current == Linetype::Named(id) {
                    line.push_str(", current");
                }
                line
            })
            .collect();
        for line in lines {
            ctx.message(line);
        }
    }

    /// Runs one option's changes as one transaction. A refusal abandons all
    /// of them and is explained; nothing is half done.
    fn transact(
        ctx: &mut CommandContext<'_>,
        changes: impl FnOnce(&mut TransactionBuilder<'_>) -> Result<String, TransactionError>,
    ) {
        let mut transaction = ctx.drawing.transaction();
        match changes(&mut transaction) {
            Ok(done) => {
                transaction.commit();
                ctx.message(done);
            }
            Err(error) => {
                drop(transaction);
                let message = explain(ctx, error);
                ctx.message(message);
            }
        }
    }

    fn load(ctx: &mut CommandContext<'_>, text: &str) {
        let found: Result<Vec<LinetypeDef>, String> = names(text)
            .into_iter()
            .map(|name| {
                LinetypeDef::built_in_named(name)
                    .ok_or_else(|| format!("There is no built-in linetype \"{name}\"."))
            })
            .collect();
        match found {
            Ok(defs) => Self::transact(ctx, |transaction| {
                let count = defs.len();
                for def in defs {
                    transaction.add_linetype(def)?;
                }
                Ok(format!("{} loaded.", linetypes(count)))
            }),
            Err(message) => ctx.message(message),
        }
    }

    fn set(ctx: &mut CommandContext<'_>, text: &str) {
        let Some(linetype) = Linetype::parse(text, ctx.drawing.linetypes()) else {
            return ctx.message(unknown_linetype(text.trim()));
        };
        let shown = linetype.describe(ctx.drawing.linetypes());
        Self::transact(ctx, |transaction| {
            transaction.set_current_linetype(linetype)?;
            Ok(format!("Current linetype: \"{shown}\""))
        });
    }

    fn delete(ctx: &mut CommandContext<'_>, text: &str) {
        let found: Result<Vec<LinetypeId>, String> = names(text)
            .into_iter()
            .map(|name| {
                ctx.drawing
                    .linetypes()
                    .find(name)
                    .ok_or_else(|| format!("There is no linetype \"{name}\"."))
            })
            .collect();
        match found {
            Ok(ids) => Self::transact(ctx, |transaction| {
                for id in &ids {
                    transaction.remove_linetype(*id)?;
                }
                Ok(format!("{} deleted.", linetypes(ids.len())))
            }),
            Err(message) => ctx.message(message),
        }
    }
}

impl Command for LinetypeCommand {
    fn start(&mut self, ctx: &mut CommandContext<'_>) -> Step {
        Self::report_current(ctx);
        Step::Continue
    }

    fn prompt(&self) -> Prompt {
        match self.state {
            State::Option => Prompt::new("Press Enter to finish"),
            State::Load => Prompt::new(format!(
                "Enter name(s) of linetype(s) to load ({})",
                built_in_names()
            )),
            State::Set => {
                Prompt::new("Enter linetype for new objects (a loaded name, ByLayer, or ByBlock)")
            }
            State::Delete => Prompt::new("Enter name(s) of linetype(s) to delete"),
        }
    }

    fn expected(&self) -> ExpectedInput {
        match self.state {
            State::Option => ExpectedInput::new(InputKind::Nothing).with_keywords(&OPTIONS),
            _ => ExpectedInput::text(),
        }
    }

    fn step(&mut self, input: CommandInput, ctx: &mut CommandContext<'_>) -> Step {
        match (&self.state, input) {
            (State::Option, CommandInput::Accept) => return Step::Finished,
            (State::Option, CommandInput::Keyword(keyword)) => {
                self.state = match keyword {
                    LOAD => State::Load,
                    SET => State::Set,
                    DELETE => State::Delete,
                    _ => {
                        Self::list(ctx);
                        State::Option
                    }
                };
            }
            (State::Load, CommandInput::Text(text)) => {
                Self::load(ctx, &text);
                self.state = State::Option;
            }
            (State::Set, CommandInput::Text(text)) => {
                Self::set(ctx, &text);
                self.state = State::Option;
            }
            (State::Delete, CommandInput::Text(text)) => {
                Self::delete(ctx, &text);
                self.state = State::Option;
            }
            // Enter at any other prompt goes back to the options.
            (_, CommandInput::Accept) => self.state = State::Option,
            _ => {}
        }
        Step::Continue
    }
}

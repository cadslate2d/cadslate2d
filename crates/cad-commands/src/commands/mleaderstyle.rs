//! MLEADERSTYLE (#241, ADR-0033): the multileader styles, from the command
//! line, as DIMSTYLE is for dimensions.
//!
//! The command stays at its option prompt until Enter. `New` copies the
//! current style under a new name and makes it current; `Set` chooses the
//! current one; `Rename` and `Delete` as for every table. The values —
//! arrow size, landing gap, dogleg length, text height, the block a new
//! note is and its scale — each ask with the current style's value as the
//! default and change the current style: every multileader drawn by it
//! follows, one row changing (ADR-0020).

use cad_document::{MLeaderStyle, MLeaderStyleId, TransactionBuilder, TransactionError};
use cad_input::{ExpectedInput, InputKind, Keyword};

use super::layer::names;
use super::ltscale::display;
use crate::{Command, CommandContext, CommandInput, Prompt, Step};

const LIST: Keyword = Keyword::new("?");
const NEW: Keyword = Keyword::new("New");
const SET: Keyword = Keyword::new("Set");
const RENAME: Keyword = Keyword::new("Rename");
const DELETE: Keyword = Keyword::new("Delete");
const ARROW: Keyword = Keyword::new("Arrow");
const GAP: Keyword = Keyword::new("Gap");
/// Two letters: `D` is `Delete`.
const DOGLEG: Keyword = Keyword::new("DOgleg");
const HEIGHT: Keyword = Keyword::new("Height");
const BLOCK: Keyword = Keyword::new("Block");
const SCALE: Keyword = Keyword::new("SCale");
const OPTIONS: [Keyword; 11] = [
    LIST, NEW, SET, RENAME, DELETE, ARROW, GAP, DOGLEG, HEIGHT, BLOCK, SCALE,
];
/// Typed at the block prompt: new notes are words again.
const NO_BLOCK: &str = ".";

/// A value of the current style, asked for with its value as the default.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Value {
    Arrow,
    Gap,
    Dogleg,
    Height,
    Scale,
}

impl Value {
    fn prompt(self) -> &'static str {
        match self {
            Self::Arrow => "Enter arrow size",
            Self::Gap => "Enter landing gap",
            Self::Dogleg => "Enter dogleg length (0 for none)",
            Self::Height => "Enter text height",
            Self::Scale => "Enter block scale",
        }
    }

    /// As in `"Standard": arrow size 3.`.
    fn noun(self) -> &'static str {
        match self {
            Self::Arrow => "arrow size",
            Self::Gap => "landing gap",
            Self::Dogleg => "dogleg length",
            Self::Height => "text height",
            Self::Scale => "block scale",
        }
    }

    fn of(self, style: &MLeaderStyle) -> f64 {
        match self {
            Self::Arrow => style.arrow_size,
            Self::Gap => style.landing_gap,
            Self::Dogleg => style.dogleg_length,
            Self::Height => style.text_height,
            Self::Scale => style.block_scale,
        }
    }

    fn set(self, style: &mut MLeaderStyle, value: f64) {
        match self {
            Self::Arrow => style.arrow_size = value,
            Self::Gap => style.landing_gap = value,
            Self::Dogleg => style.dogleg_length = value,
            Self::Height => style.text_height = value,
            Self::Scale => style.block_scale = value,
        }
    }

    /// A length takes a unit; a scale is a plain number.
    fn expected(self) -> ExpectedInput {
        match self {
            Self::Scale => ExpectedInput::factor(),
            _ => ExpectedInput::distance(),
        }
    }
}

#[derive(Debug)]
enum State {
    Option,
    New,
    Set,
    RenameFrom,
    RenameTo(MLeaderStyleId),
    Delete,
    Value(Value),
    Block,
}

#[derive(Debug)]
pub(crate) struct MLeaderStyleCommand {
    state: State,
    /// The current style as the last option found it: what the prompts
    /// show as their defaults.
    current: MLeaderStyle,
}

/// The current style, by value.
fn current(ctx: &CommandContext<'_>) -> MLeaderStyle {
    let id = ctx.drawing.settings().current_mleader_style;
    ctx.drawing.tables().mleader_style(id).clone()
}

/// The style called `name`, or the message that there is none.
fn find(ctx: &CommandContext<'_>, name: &str) -> Result<MLeaderStyleId, String> {
    ctx.drawing
        .mleader_styles()
        .find(name)
        .ok_or_else(|| format!("There is no multileader style \"{name}\"."))
}

/// `"Standard": arrows 2.5, gap 1, dogleg 5, text 2.5, block "TAG" at 1`.
fn describe(style: &MLeaderStyle) -> String {
    let mut line = format!(
        "\"{}\": arrows {}, gap {}, dogleg {}, text {}",
        style.name,
        display(style.arrow_size),
        display(style.landing_gap),
        display(style.dogleg_length),
        display(style.text_height),
    );
    if let Some(block) = &style.block {
        line.push_str(&format!(
            ", block \"{block}\" at {}",
            display(style.block_scale)
        ));
    }
    line
}

/// Why the table refused a change, in the user's terms.
fn explain(ctx: &CommandContext<'_>, error: TransactionError) -> String {
    let name = |id: MLeaderStyleId| {
        ctx.drawing
            .mleader_styles()
            .get(id)
            .map_or_else(|| id.to_string(), |style| style.name.clone())
    };
    match error {
        TransactionError::BadMLeaderStyleName(error) => error.explain("multileader style"),
        TransactionError::BadMLeaderStyle(error) => error.explain(),
        TransactionError::DuplicateMLeaderStyleName(id) => {
            format!("There is already a multileader style \"{}\".", name(id))
        }
        TransactionError::PermanentMLeaderStyle => format!(
            "Multileader style \"{}\" cannot be deleted or renamed.",
            MLeaderStyle::STANDARD
        ),
        TransactionError::MLeaderStyleInUse(id)
            if ctx.drawing.settings().current_mleader_style == id =>
        {
            format!(
                "Multileader style \"{}\" is current: set another first.",
                name(id)
            )
        }
        TransactionError::MLeaderStyleInUse(id) => {
            format!(
                "Multileader style \"{}\" is used by a multileader.",
                name(id)
            )
        }
        other => other.to_string(),
    }
}

impl MLeaderStyleCommand {
    pub(crate) fn new() -> Self {
        Self {
            state: State::Option,
            current: MLeaderStyle::named(MLeaderStyle::STANDARD),
        }
    }

    fn list(ctx: &mut CommandContext<'_>) {
        let current = ctx.drawing.settings().current_mleader_style;
        let lines: Vec<String> = ctx
            .drawing
            .mleader_styles()
            .iter()
            .map(|(id, style)| {
                let mut line = format!("  {}", describe(style));
                if id == current {
                    line.push_str(", current");
                }
                line
            })
            .collect();
        for line in lines {
            ctx.message(line);
        }
    }

    /// Runs one option's changes as one transaction. A refusal abandons
    /// all of them and is explained.
    fn transact(
        ctx: &mut CommandContext<'_>,
        changes: impl FnOnce(&mut TransactionBuilder<'_>) -> Result<String, TransactionError>,
    ) {
        let mut transaction = ctx.transaction();
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

    fn new_style(ctx: &mut CommandContext<'_>, name: &str) {
        let from = current(ctx);
        let style = MLeaderStyle {
            name: name.trim().to_owned(),
            ..from.clone()
        };
        Self::transact(ctx, |transaction| {
            let id = transaction.add_mleader_style(style)?;
            transaction.set_current_mleader_style(id)?;
            Ok(format!(
                "Multileader style \"{}\" made from \"{}\" and set current.",
                name.trim(),
                from.name
            ))
        });
    }

    fn set(ctx: &mut CommandContext<'_>, name: &str) {
        match find(ctx, name.trim()) {
            Ok(id) => Self::transact(ctx, |transaction| {
                transaction.set_current_mleader_style(id)?;
                Ok(format!("Current multileader style: \"{}\".", name.trim()))
            }),
            Err(message) => ctx.message(message),
        }
    }

    fn rename(ctx: &mut CommandContext<'_>, id: MLeaderStyleId, name: &str) {
        let Some(style) = ctx.drawing.mleader_styles().get(id).cloned() else {
            return;
        };
        let renamed = MLeaderStyle {
            name: name.trim().to_owned(),
            ..style.clone()
        };
        Self::transact(ctx, |transaction| {
            transaction.replace_mleader_style(id, renamed)?;
            Ok(format!(
                "Multileader style \"{}\" renamed \"{}\".",
                style.name,
                name.trim()
            ))
        });
    }

    fn delete(ctx: &mut CommandContext<'_>, text: &str) {
        let found: Result<Vec<MLeaderStyleId>, String> = names(text)
            .into_iter()
            .map(|name| find(ctx, name))
            .collect();
        match found {
            Ok(ids) => Self::transact(ctx, |transaction| {
                for id in &ids {
                    transaction.remove_mleader_style(*id)?;
                }
                let plural = if ids.len() == 1 { "" } else { "s" };
                Ok(format!("{} multileader style{plural} deleted.", ids.len()))
            }),
            Err(message) => ctx.message(message),
        }
    }

    /// Changes the current style; every multileader it draws follows.
    fn restyle(ctx: &mut CommandContext<'_>, change: impl FnOnce(&mut MLeaderStyle) -> String) {
        let id = ctx.drawing.settings().current_mleader_style;
        let mut style = current(ctx);
        let message = change(&mut style);
        Self::transact(ctx, |transaction| {
            transaction.replace_mleader_style(id, style)?;
            Ok(message)
        });
    }
}

impl Command for MLeaderStyleCommand {
    fn start(&mut self, ctx: &mut CommandContext<'_>) -> Step {
        self.current = current(ctx);
        let message = format!("Current multileader style {}", describe(&self.current));
        ctx.message(message);
        Step::Continue
    }

    fn prompt(&self) -> Prompt {
        match &self.state {
            State::Option => Prompt::new("Press Enter to finish"),
            State::New => Prompt::new("Enter name for new multileader style"),
            State::Set => Prompt::new("Enter name of multileader style to set current"),
            State::RenameFrom => Prompt::new("Enter name of multileader style to rename"),
            State::RenameTo(_) => Prompt::new("Enter new name"),
            State::Delete => Prompt::new("Enter name(s) of multileader style(s) to delete"),
            State::Value(value) => {
                Prompt::new(value.prompt()).with_default(display(value.of(&self.current)))
            }
            State::Block => {
                let prompt = Prompt::new(format!(
                    "Enter the block a new note is (\"{NO_BLOCK}\" for words)"
                ));
                match &self.current.block {
                    Some(block) => prompt.with_default(block.clone()),
                    None => prompt,
                }
            }
        }
    }

    fn expected(&self) -> ExpectedInput {
        match &self.state {
            State::Option => ExpectedInput::new(InputKind::Nothing).with_keywords(&OPTIONS),
            State::Value(value) => value.expected(),
            _ => ExpectedInput::text(),
        }
    }

    fn step(&mut self, input: CommandInput, ctx: &mut CommandContext<'_>) -> Step {
        match (&self.state, input) {
            (State::Option, CommandInput::Accept) => return Step::Finished,
            (State::Option, CommandInput::Keyword(keyword)) => {
                self.current = current(ctx);
                self.state = match keyword {
                    NEW => State::New,
                    SET => State::Set,
                    RENAME => State::RenameFrom,
                    DELETE => State::Delete,
                    ARROW => State::Value(Value::Arrow),
                    GAP => State::Value(Value::Gap),
                    DOGLEG => State::Value(Value::Dogleg),
                    HEIGHT => State::Value(Value::Height),
                    SCALE => State::Value(Value::Scale),
                    BLOCK => State::Block,
                    _ => {
                        Self::list(ctx);
                        State::Option
                    }
                };
            }
            (State::New, CommandInput::Text(name)) => {
                Self::new_style(ctx, &name);
                self.state = State::Option;
            }
            (State::Set, CommandInput::Text(name)) => {
                Self::set(ctx, &name);
                self.state = State::Option;
            }
            (State::RenameFrom, CommandInput::Text(name)) => {
                self.state = match find(ctx, name.trim()) {
                    Ok(id) => State::RenameTo(id),
                    Err(message) => {
                        ctx.message(message);
                        State::Option
                    }
                };
            }
            (State::RenameTo(id), CommandInput::Text(name)) => {
                Self::rename(ctx, *id, &name);
                self.state = State::Option;
            }
            (State::Delete, CommandInput::Text(text)) => {
                Self::delete(ctx, &text);
                self.state = State::Option;
            }
            (State::Value(value), CommandInput::Distance(number)) => {
                let value = *value;
                Self::restyle(ctx, |style| {
                    value.set(style, number);
                    format!("\"{}\": {} {}.", style.name, value.noun(), display(number))
                });
                self.state = State::Option;
            }
            (State::Block, CommandInput::Text(text)) => {
                let name = text.trim();
                if name == NO_BLOCK {
                    Self::restyle(ctx, |style| {
                        style.block = None;
                        format!("\"{}\": new notes are words.", style.name)
                    });
                } else if let Some(id) = ctx.drawing.blocks().find(name) {
                    // The block's own name, as the table spells it.
                    let name = ctx
                        .drawing
                        .blocks()
                        .get(id)
                        .map_or_else(|| name.to_owned(), |block| block.name.clone());
                    Self::restyle(ctx, |style| {
                        style.block = Some(name.clone());
                        format!("\"{}\": new notes are block \"{name}\".", style.name)
                    });
                } else {
                    ctx.message(format!("There is no block \"{name}\"."));
                }
                self.state = State::Option;
            }
            // Enter at any other prompt keeps what there is and goes back.
            (_, CommandInput::Accept) => self.state = State::Option,
            _ => {}
        }
        Step::Continue
    }
}

//! DIMSTYLE: the dimension styles, from the command line (DESIGN §19.1,
//! §34).
//!
//! The command stays at its option prompt until Enter, like LAYER and
//! LINETYPE. `New` copies the current style under a new name and makes it
//! current; `Set` chooses the current one; the variables — text height,
//! arrow size, text gap, extension offset and overshoot, decimals, suffix
//! — each ask with the current style's value as the default and change the
//! current style, which redraws every dimension drawn by it (§19.6). What
//! the table allows is decided by `cad-document`; this is the conversation.

use cad_document::{DimStyle, DimStyleId, TransactionBuilder, TransactionError};
use cad_input::{ExpectedInput, InputKind, Keyword};

use super::layer::names;
use super::ltscale::display;
use crate::{Command, CommandContext, CommandInput, Prompt, Step};

const LIST: Keyword = Keyword::new("?");
const NEW: Keyword = Keyword::new("New");
const SET: Keyword = Keyword::new("Set");
const RENAME: Keyword = Keyword::new("Rename");
const DELETE: Keyword = Keyword::new("Delete");
const HEIGHT: Keyword = Keyword::new("Height");
const ARROW: Keyword = Keyword::new("Arrow");
const GAP: Keyword = Keyword::new("Gap");
const OFFSET: Keyword = Keyword::new("Offset");
const EXTENSION: Keyword = Keyword::new("Extension");
const PRECISION: Keyword = Keyword::new("Precision");
const SUFFIX: Keyword = Keyword::new("SUffix");
const OPTIONS: [Keyword; 12] = [
    LIST, NEW, SET, RENAME, DELETE, HEIGHT, ARROW, GAP, OFFSET, EXTENSION, PRECISION, SUFFIX,
];
/// Typed at the suffix prompt, clears the suffix.
const NO_SUFFIX: &str = ".";

/// A length of the current style, asked for with the current value as the
/// default.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Length {
    Height,
    Arrow,
    Gap,
    Offset,
    Extension,
}

impl Length {
    fn prompt(self) -> &'static str {
        match self {
            Self::Height => "Enter text height",
            Self::Arrow => "Enter arrow size",
            Self::Gap => "Enter gap between dimension line and text",
            Self::Offset => "Enter extension line offset",
            Self::Extension => "Enter extension line overshoot",
        }
    }

    /// As in `"Standard": text height 3.`.
    fn noun(self) -> &'static str {
        match self {
            Self::Height => "text height",
            Self::Arrow => "arrow size",
            Self::Gap => "text gap",
            Self::Offset => "extension line offset",
            Self::Extension => "extension line overshoot",
        }
    }

    fn of(self, style: &DimStyle) -> f64 {
        match self {
            Self::Height => style.text_height,
            Self::Arrow => style.arrow_size,
            Self::Gap => style.text_gap,
            Self::Offset => style.extension_offset,
            Self::Extension => style.extension_overshoot,
        }
    }

    fn set(self, style: &mut DimStyle, value: f64) {
        match self {
            Self::Height => style.text_height = value,
            Self::Arrow => style.arrow_size = value,
            Self::Gap => style.text_gap = value,
            Self::Offset => style.extension_offset = value,
            Self::Extension => style.extension_overshoot = value,
        }
    }
}

#[derive(Debug)]
enum State {
    Option,
    New,
    Set,
    RenameFrom,
    RenameTo(DimStyleId),
    Delete,
    Length(Length),
    Precision,
    Suffix,
}

#[derive(Debug)]
pub(crate) struct DimStyleCommand {
    state: State,
    /// The current style as the last option found it: what the prompts
    /// show as their defaults.
    current: DimStyle,
}

/// The current style, by value: what the prompts show and the variables change.
fn current(ctx: &CommandContext<'_>) -> DimStyle {
    let id = ctx.drawing.current_dim_style();
    ctx.drawing
        .dim_styles()
        .get(id)
        .cloned()
        .unwrap_or_else(|| DimStyle::named(DimStyle::STANDARD))
}

/// The style called `name`, or the message that there is none.
fn find(ctx: &CommandContext<'_>, name: &str) -> Result<DimStyleId, String> {
    ctx.drawing
        .dim_styles()
        .find(name)
        .ok_or_else(|| format!("There is no dimension style \"{name}\"."))
}

/// `"Standard": text 2.5, arrows 2.5, gap 0.625, offset 0.625, overshoot 1.25, 2 decimals`.
fn describe(style: &DimStyle) -> String {
    let mut line = format!(
        "\"{}\": text {}, arrows {}, gap {}, offset {}, overshoot {}, {} decimal{}",
        style.name,
        display(style.text_height),
        display(style.arrow_size),
        display(style.text_gap),
        display(style.extension_offset),
        display(style.extension_overshoot),
        style.decimals,
        if style.decimals == 1 { "" } else { "s" },
    );
    if !style.suffix.is_empty() {
        line.push_str(&format!(", suffix \"{}\"", style.suffix));
    }
    line
}

/// Why the table refused a change, in the user's terms.
fn explain(ctx: &CommandContext<'_>, error: TransactionError) -> String {
    let name = |id: DimStyleId| {
        ctx.drawing
            .dim_styles()
            .get(id)
            .map_or_else(|| id.to_string(), |style| style.name.clone())
    };
    match error {
        TransactionError::BadDimStyleName(error) => error.explain("dimension style"),
        TransactionError::BadDimStyle(error) => error.explain(),
        TransactionError::DuplicateDimStyleName(id) => {
            format!("There is already a dimension style \"{}\".", name(id))
        }
        TransactionError::PermanentDimStyle => format!(
            "Dimension style \"{}\" cannot be deleted or renamed.",
            DimStyle::STANDARD
        ),
        TransactionError::DimStyleInUse(id) => format!(
            "Dimension style \"{}\" is current: set another first.",
            name(id)
        ),
        other => other.to_string(),
    }
}

impl DimStyleCommand {
    pub(crate) fn new() -> Self {
        Self {
            state: State::Option,
            current: DimStyle::named(DimStyle::STANDARD),
        }
    }

    fn report_current(ctx: &mut CommandContext<'_>) {
        let message = format!("Current dimension style {}", describe(&current(ctx)));
        ctx.message(message);
    }

    /// The `?` option: one line per style, in table order.
    fn list(ctx: &mut CommandContext<'_>) {
        let current = ctx.drawing.current_dim_style();
        let lines: Vec<String> = ctx
            .drawing
            .dim_styles()
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
    /// all of them and is explained; nothing is half done.
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

    /// A new style with the current one's values, made current.
    fn new_style(ctx: &mut CommandContext<'_>, name: &str) {
        let from = current(ctx);
        let style = from.renamed(name.trim());
        Self::transact(ctx, |transaction| {
            let id = transaction.add_dim_style(style)?;
            transaction.set_current_dim_style(id)?;
            Ok(format!(
                "Dimension style \"{}\" made from \"{}\" and set current.",
                name.trim(),
                from.name
            ))
        });
    }

    fn set(ctx: &mut CommandContext<'_>, name: &str) {
        match find(ctx, name.trim()) {
            Ok(id) => Self::transact(ctx, |transaction| {
                transaction.set_current_dim_style(id)?;
                Ok(format!("Current dimension style: \"{}\".", name.trim()))
            }),
            Err(message) => ctx.message(message),
        }
    }

    fn rename(ctx: &mut CommandContext<'_>, id: DimStyleId, name: &str) {
        let Some(style) = ctx.drawing.dim_styles().get(id).cloned() else {
            return;
        };
        let renamed = style.renamed(name.trim());
        Self::transact(ctx, |transaction| {
            transaction.replace_dim_style(id, renamed)?;
            Ok(format!(
                "Dimension style \"{}\" renamed \"{}\".",
                style.name,
                name.trim()
            ))
        });
    }

    fn delete(ctx: &mut CommandContext<'_>, text: &str) {
        let found: Result<Vec<DimStyleId>, String> = names(text)
            .into_iter()
            .map(|name| find(ctx, name))
            .collect();
        match found {
            Ok(ids) => Self::transact(ctx, |transaction| {
                for id in &ids {
                    transaction.remove_dim_style(*id)?;
                }
                let plural = if ids.len() == 1 { "" } else { "s" };
                Ok(format!("{} dimension style{plural} deleted.", ids.len()))
            }),
            Err(message) => ctx.message(message),
        }
    }

    /// Changes one value of the current style; every dimension it draws follows.
    fn restyle(
        ctx: &mut CommandContext<'_>,
        change: impl FnOnce(&mut DimStyle),
        done: impl FnOnce(&DimStyle) -> String,
    ) {
        let id = ctx.drawing.current_dim_style();
        let mut style = current(ctx);
        change(&mut style);
        let message = done(&style);
        Self::transact(ctx, |transaction| {
            transaction.replace_dim_style(id, style)?;
            Ok(message)
        });
    }
}

impl Command for DimStyleCommand {
    fn start(&mut self, ctx: &mut CommandContext<'_>) -> Step {
        self.current = current(ctx);
        Self::report_current(ctx);
        Step::Continue
    }

    fn prompt(&self) -> Prompt {
        match &self.state {
            State::Option => Prompt::new("Press Enter to finish"),
            State::New => Prompt::new("Enter name for new dimension style"),
            State::Set => Prompt::new("Enter name of dimension style to set current"),
            State::RenameFrom => Prompt::new("Enter name of dimension style to rename"),
            State::RenameTo(_) => Prompt::new("Enter new name"),
            State::Delete => Prompt::new("Enter name(s) of dimension style(s) to delete"),
            State::Length(length) => {
                Prompt::new(length.prompt()).with_default(display(length.of(&self.current)))
            }
            State::Precision => Prompt::new("Enter number of decimals (0 to 8)")
                .with_default(self.current.decimals.to_string()),
            State::Suffix => {
                let prompt = Prompt::new(format!(
                    "Enter text after the value (\"{NO_SUFFIX}\" for none)"
                ));
                if self.current.suffix.is_empty() {
                    prompt
                } else {
                    prompt.with_default(self.current.suffix.clone())
                }
            }
        }
    }

    fn expected(&self) -> ExpectedInput {
        match &self.state {
            State::Option => ExpectedInput::new(InputKind::Nothing).with_keywords(&OPTIONS),
            State::Length(_) => ExpectedInput::distance(),
            State::Precision => ExpectedInput::factor(),
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
                    HEIGHT => State::Length(Length::Height),
                    ARROW => State::Length(Length::Arrow),
                    GAP => State::Length(Length::Gap),
                    OFFSET => State::Length(Length::Offset),
                    EXTENSION => State::Length(Length::Extension),
                    PRECISION => State::Precision,
                    SUFFIX => State::Suffix,
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
            (State::Length(length), CommandInput::Distance(value)) => {
                let length = *length;
                Self::restyle(
                    ctx,
                    |style| length.set(style, value),
                    |style| format!("\"{}\": {} {}.", style.name, length.noun(), display(value)),
                );
                self.state = State::Option;
            }
            (State::Precision, CommandInput::Distance(count)) => {
                // A count: whole, and no more than a value can show.
                let decimals = if count.is_finite() && count >= 0.0 && count.fract() == 0.0 {
                    u8::try_from(count as u32).unwrap_or(u8::MAX)
                } else {
                    u8::MAX
                };
                Self::restyle(
                    ctx,
                    |style| style.decimals = decimals,
                    |style| format!("\"{}\": {} decimals.", style.name, decimals),
                );
                self.state = State::Option;
            }
            (State::Suffix, CommandInput::Text(text)) => {
                let suffix = if text.trim() == NO_SUFFIX {
                    String::new()
                } else {
                    text
                };
                Self::restyle(
                    ctx,
                    |style| style.suffix = suffix.clone(),
                    |style| format!("\"{}\": suffix \"{}\".", style.name, style.suffix),
                );
                self.state = State::Option;
            }
            // Enter at any other prompt keeps what there is and goes back.
            (_, CommandInput::Accept) => self.state = State::Option,
            _ => {}
        }
        Step::Continue
    }
}

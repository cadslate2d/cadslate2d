//! STYLE (#221): text styles from the command line.
//!
//! Like DIMSTYLE, the command stays at its option prompt until Enter.
//! `New` copies the current style under a new name and makes it current;
//! `Set` chooses the current one; `Rename`, `Delete`, and `?` as for every
//! table. `Height`, `Width`, and `Oblique` change what the current style
//! gives a new text: a fixed height (zero to ask each time), a width
//! factor, a slant. A text already made keeps its own (DXF's TEXT does),
//! so no text changes. What the table allows is decided by `cad-document`.

use cad_document::{TextStyle, TextStyleId, TransactionBuilder, TransactionError};
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
const WIDTH: Keyword = Keyword::new("Width");
const OBLIQUE: Keyword = Keyword::new("Oblique");
const OPTIONS: [Keyword; 8] = [LIST, NEW, SET, RENAME, DELETE, HEIGHT, WIDTH, OBLIQUE];

#[derive(Debug)]
enum State {
    Option,
    New,
    Set,
    RenameFrom,
    RenameTo(TextStyleId),
    Delete,
    Height,
    Width,
    Oblique,
}

#[derive(Debug)]
pub(crate) struct StyleCommand {
    state: State,
    /// The current style as the last option found it: the prompts' defaults.
    current: TextStyle,
}

/// The current style, by value.
fn current(ctx: &CommandContext<'_>) -> TextStyle {
    ctx.drawing
        .text_styles()
        .get(ctx.drawing.current_text_style())
        .cloned()
        .unwrap_or_else(|| TextStyle::named(TextStyle::STANDARD))
}

fn find(ctx: &CommandContext<'_>, name: &str) -> Result<TextStyleId, String> {
    ctx.drawing
        .text_styles()
        .find(name)
        .ok_or_else(|| format!("There is no text style \"{name}\"."))
}

/// `"Notes": height 3.5, width 0.7, oblique 15°`, or `height asked`.
fn describe(style: &TextStyle) -> String {
    let height = if style.height > 0.0 {
        format!("height {}", display(style.height))
    } else {
        "height asked".to_owned()
    };
    format!(
        "\"{}\": {height}, width {}, oblique {}°",
        style.name,
        display(style.width_factor),
        display(style.oblique.to_degrees())
    )
}

/// Why the table refused a change, in the user's terms.
fn explain(ctx: &CommandContext<'_>, error: TransactionError) -> String {
    let name = |id: TextStyleId| {
        ctx.drawing
            .text_styles()
            .get(id)
            .map_or_else(|| id.to_string(), |style| style.name.clone())
    };
    match error {
        TransactionError::BadTextStyleName(error) => error.explain("text style"),
        TransactionError::BadTextStyle(error) => error.explain().to_owned(),
        TransactionError::DuplicateTextStyleName(id) => {
            format!("There is already a text style \"{}\".", name(id))
        }
        TransactionError::PermanentTextStyle => format!(
            "Text style \"{}\" cannot be deleted or renamed.",
            TextStyle::STANDARD
        ),
        TransactionError::TextStyleInUse(id) if ctx.drawing.current_text_style() == id => {
            format!("Text style \"{}\" is current: set another first.", name(id))
        }
        TransactionError::TextStyleInUse(id) => {
            format!("Text style \"{}\" is used by a text.", name(id))
        }
        other => other.to_string(),
    }
}

impl StyleCommand {
    pub(crate) fn new() -> Self {
        Self {
            state: State::Option,
            current: TextStyle::named(TextStyle::STANDARD),
        }
    }

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

    fn list(ctx: &mut CommandContext<'_>) {
        let current = ctx.drawing.current_text_style();
        let lines: Vec<String> = ctx
            .drawing
            .text_styles()
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

    /// A new style with the current one's values, made current.
    fn new_style(ctx: &mut CommandContext<'_>, name: &str) {
        let from = current(ctx);
        let style = TextStyle {
            name: name.trim().to_owned(),
            ..from.clone()
        };
        Self::transact(ctx, |transaction| {
            let id = transaction.add_text_style(style)?;
            transaction.set_current_text_style(id)?;
            Ok(format!(
                "Text style \"{}\" made from \"{}\" and set current.",
                name.trim(),
                from.name
            ))
        });
    }

    /// Changes one value of the current style.
    fn restyle(ctx: &mut CommandContext<'_>, change: impl FnOnce(&mut TextStyle)) {
        let id = ctx.drawing.current_text_style();
        let mut style = current(ctx);
        change(&mut style);
        let message = describe(&style);
        Self::transact(ctx, |transaction| {
            transaction.set_text_style_values(id, style)?;
            Ok(format!("{message}."))
        });
    }
}

impl Command for StyleCommand {
    fn start(&mut self, ctx: &mut CommandContext<'_>) -> Step {
        self.current = current(ctx);
        let message = format!("Current text style {}", describe(&self.current));
        ctx.message(message);
        Step::Continue
    }

    fn prompt(&self) -> Prompt {
        match &self.state {
            State::Option => Prompt::new("Press Enter to finish"),
            State::New => Prompt::new("Enter name for new text style"),
            State::Set => Prompt::new("Enter name of text style to set current"),
            State::RenameFrom => Prompt::new("Enter name of text style to rename"),
            State::RenameTo(_) => Prompt::new("Enter new name"),
            State::Delete => Prompt::new("Enter name(s) of text style(s) to delete"),
            State::Height => Prompt::new("Enter height (0 to ask each time)")
                .with_default(display(self.current.height)),
            State::Width => {
                Prompt::new("Enter width factor").with_default(display(self.current.width_factor))
            }
            State::Oblique => Prompt::new("Enter obliquing angle")
                .with_default(display(self.current.oblique.to_degrees())),
        }
    }

    fn expected(&self) -> ExpectedInput {
        match &self.state {
            State::Option => ExpectedInput::new(InputKind::Nothing).with_keywords(&OPTIONS),
            State::Height => ExpectedInput::distance(),
            State::Width => ExpectedInput::factor(),
            State::Oblique => ExpectedInput::angle(),
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
                    HEIGHT => State::Height,
                    WIDTH => State::Width,
                    OBLIQUE => State::Oblique,
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
                match find(ctx, name.trim()) {
                    Ok(id) => Self::transact(ctx, |transaction| {
                        transaction.set_current_text_style(id)?;
                        Ok(format!("Current text style: \"{}\".", name.trim()))
                    }),
                    Err(message) => ctx.message(message),
                }
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
                let id = *id;
                let old = ctx
                    .drawing
                    .text_styles()
                    .get(id)
                    .map(|style| style.name.clone())
                    .unwrap_or_default();
                Self::transact(ctx, |transaction| {
                    transaction.rename_text_style(id, name.trim())?;
                    Ok(format!("Text style \"{old}\" renamed \"{}\".", name.trim()))
                });
                self.state = State::Option;
            }
            (State::Delete, CommandInput::Text(text)) => {
                let found: Result<Vec<TextStyleId>, String> = names(&text)
                    .into_iter()
                    .map(|name| find(ctx, name))
                    .collect();
                match found {
                    Ok(ids) => Self::transact(ctx, |transaction| {
                        for id in &ids {
                            transaction.remove_text_style(*id)?;
                        }
                        let plural = if ids.len() == 1 { "" } else { "s" };
                        Ok(format!("{} text style{plural} deleted.", ids.len()))
                    }),
                    Err(message) => ctx.message(message),
                }
                self.state = State::Option;
            }
            (State::Height, CommandInput::Distance(height)) => {
                Self::restyle(ctx, |style| style.height = height);
                self.state = State::Option;
            }
            (State::Width, CommandInput::Distance(width)) => {
                Self::restyle(ctx, |style| style.width_factor = width);
                self.state = State::Option;
            }
            (State::Oblique, CommandInput::Angle(degrees)) => {
                // An angle typed as 345 is a lean of −15°.
                let signed = if degrees > 180.0 {
                    degrees - 360.0
                } else {
                    degrees
                };
                Self::restyle(ctx, |style| style.oblique = signed.to_radians());
                self.state = State::Option;
            }
            (_, CommandInput::Accept) => self.state = State::Option,
            _ => {}
        }
        Step::Continue
    }
}

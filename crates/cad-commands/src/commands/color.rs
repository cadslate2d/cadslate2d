//! COLOR: the colour new objects are given (DESIGN §32).
//!
//! By default that is `ByLayer`: an object is drawn in its layer's colour
//! and follows it when the layer is recoloured. A colour of its own stays
//! with the object whatever its layer does. The current colour is a drawing
//! setting, changed in a transaction and undone like anything else.

use cad_document::Color;
use cad_input::ExpectedInput;

use crate::{Command, CommandContext, CommandInput, Prompt, Step};

/// What is said when the text typed is not a colour.
pub(super) const NOT_A_COLOR: &str =
    "A colour is a number from 1 to 255, or red, yellow, green, cyan, blue, magenta, or white.";

#[derive(Debug)]
pub(crate) struct ColorCommand {
    /// The colour when the command started, shown as the default.
    current: Color,
}

impl ColorCommand {
    pub(crate) fn new() -> Self {
        Self {
            current: Color::ByLayer,
        }
    }
}

/// "New objects are red.", for every kind of colour.
fn describe(color: Color) -> String {
    match color {
        Color::ByLayer => "New objects take their layer's colour.".to_owned(),
        Color::ByBlock => "New objects take the colour of the block they are in.".to_owned(),
        Color::Index(index) => {
            let name = color.describe();
            if name == index.to_string() {
                format!("New objects are colour {index}.")
            } else {
                format!("New objects are {name} (colour {index}).")
            }
        }
    }
}

impl Command for ColorCommand {
    fn start(&mut self, ctx: &mut CommandContext<'_>) -> Step {
        self.current = ctx.drawing.current_color();
        Step::Continue
    }

    fn prompt(&self) -> Prompt {
        Prompt::new("Enter colour for new objects (1-255, a name, ByLayer, or ByBlock)")
            .with_default(self.current.describe())
    }

    fn expected(&self) -> ExpectedInput {
        ExpectedInput::text()
    }

    fn step(&mut self, input: CommandInput, ctx: &mut CommandContext<'_>) -> Step {
        let CommandInput::Text(text) = input else {
            // Enter keeps the colour there is.
            return Step::Finished;
        };
        let Some(color) = Color::parse(&text) else {
            ctx.message(format!("{NOT_A_COLOR} Or ByLayer, or ByBlock."));
            return Step::Continue;
        };
        if color != self.current {
            let mut transaction = ctx.transaction();
            transaction.set_current_color(color);
            transaction.commit();
        }
        ctx.message(describe(color));
        Step::Finished
    }
}

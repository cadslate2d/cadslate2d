//! LWEIGHT: the lineweight new objects are given (DESIGN §23.1, §32).
//!
//! By default that is `ByLayer`. Whether lineweights are *shown* is another
//! matter: the LWT toggle on the status bar, which belongs to the session
//! and not to the drawing.

use cad_document::Lineweight;
use cad_input::ExpectedInput;

use crate::{Command, CommandContext, CommandInput, Prompt, Step, Toggle};

/// What is said when the text typed is not a lineweight.
pub(super) const NOT_A_LINEWEIGHT: &str = "A lineweight is one of the standard widths in millimetres: 0, 0.05, 0.09, 0.13, 0.15, 0.18, 0.2, 0.25, 0.3, 0.35, 0.4, 0.5, 0.53, 0.6, 0.7, 0.8, 0.9, 1, 1.06, 1.2, 1.4, 1.58, 2, or 2.11.";

#[derive(Debug)]
pub(crate) struct LweightCommand {
    /// The lineweight when the command started, shown as the default.
    current: Lineweight,
}

impl LweightCommand {
    pub(crate) fn new() -> Self {
        Self {
            current: Lineweight::ByLayer,
        }
    }
}

fn describe(lineweight: Lineweight) -> String {
    match lineweight {
        Lineweight::ByLayer => "New objects take their layer's lineweight.".to_owned(),
        Lineweight::ByBlock => {
            "New objects take the lineweight of the block they are in.".to_owned()
        }
        Lineweight::Default => "New objects have the default lineweight.".to_owned(),
        Lineweight::Hundredths(_) => {
            format!(
                "New objects have a lineweight of {}.",
                lineweight.describe()
            )
        }
    }
}

impl Command for LweightCommand {
    fn start(&mut self, ctx: &mut CommandContext<'_>) -> Step {
        self.current = ctx.drawing.current_lineweight();
        Step::Continue
    }

    fn prompt(&self) -> Prompt {
        Prompt::new("Enter lineweight for new objects (mm, Default, ByLayer, or ByBlock)")
            .with_default(self.current.describe())
    }

    fn expected(&self) -> ExpectedInput {
        ExpectedInput::text()
    }

    fn step(&mut self, input: CommandInput, ctx: &mut CommandContext<'_>) -> Step {
        let CommandInput::Text(text) = input else {
            // Enter keeps the lineweight there is.
            return Step::Finished;
        };
        let Some(lineweight) = Lineweight::parse(&text) else {
            ctx.message(format!(
                "{NOT_A_LINEWEIGHT} Or Default, ByLayer, or ByBlock."
            ));
            return Step::Continue;
        };
        if lineweight != self.current {
            let mut transaction = ctx.drawing.transaction();
            transaction.set_current_lineweight(lineweight);
            transaction.commit();
        }
        ctx.message(describe(lineweight));
        if lineweight != Lineweight::ByLayer && !ctx.toggles.is_on(Toggle::Lineweight) {
            ctx.message("Lineweights are not being shown: switch LWT on to see them.");
        }
        Step::Finished
    }
}

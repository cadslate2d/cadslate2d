//! LTSCALE: what every linetype pattern is multiplied by (DESIGN §23.2;
//! ADR-0015).
//!
//! A drawing setting, so it is undone like one and travels with the file.
//! Changing it changes the pattern table and no geometry.

use cad_input::ExpectedInput;

use crate::{Command, CommandContext, CommandInput, Prompt, Step};

#[derive(Debug)]
pub(crate) struct LtscaleCommand {
    /// The scale when the command started, shown as the default.
    current: f64,
}

impl LtscaleCommand {
    pub(crate) fn new() -> Self {
        Self { current: 1.0 }
    }
}

/// `5`, `2.5`: as short as the value allows.
pub(super) fn display(value: f64) -> String {
    let text = format!("{value:.4}");
    text.trim_end_matches('0').trim_end_matches('.').to_owned()
}

impl Command for LtscaleCommand {
    fn start(&mut self, ctx: &mut CommandContext<'_>) -> Step {
        self.current = ctx.drawing.ltscale();
        Step::Continue
    }

    fn prompt(&self) -> Prompt {
        Prompt::new("Enter new linetype scale factor").with_default(display(self.current))
    }

    fn expected(&self) -> ExpectedInput {
        ExpectedInput::factor()
    }

    fn step(&mut self, input: CommandInput, ctx: &mut CommandContext<'_>) -> Step {
        let CommandInput::Distance(scale) = input else {
            // Enter keeps the scale there is.
            return Step::Finished;
        };
        let mut transaction = ctx.transaction();
        if transaction.set_ltscale(scale).is_err() {
            drop(transaction);
            ctx.message("The linetype scale must be a positive number.");
            return Step::Continue;
        }
        transaction.commit();
        ctx.message(format!("Linetype scale: {}", display(scale)));
        Step::Finished
    }
}

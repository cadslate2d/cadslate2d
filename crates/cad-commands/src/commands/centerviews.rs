//! CENTERVIEWS (#358): moves what is composed on the sheet — its views,
//! sections and their marks, tables, notes, and balloons — as one toward
//! the middle of its drawing area, clear of the title block, each view's
//! record with it. Views are placed from the top left so that what comes
//! after still finds room; this puts the finished sheet in the middle. A
//! document that composes a sheet ends with it. One undo.

use crate::compose::centre;
use crate::{Command, CommandContext, CommandInput, Prompt, Step};
use cad_input::{ExpectedInput, InputKind};

#[derive(Debug)]
pub(crate) struct CenterViewsCommand;

impl CenterViewsCommand {
    pub(crate) fn new() -> Self {
        Self
    }
}

impl Command for CenterViewsCommand {
    fn start(&mut self, ctx: &mut CommandContext<'_>) -> Step {
        let space = ctx.space;
        match centre(ctx.drawing, space) {
            Ok(Some(views)) => ctx.message(format!(
                "{} centred on the sheet.",
                if views == 1 {
                    "1 view".to_owned()
                } else {
                    format!("{views} views")
                }
            )),
            // Where they should be already: nothing to do, and nothing to
            // say — a sentence with no change would read as a refusal.
            Ok(None) => {}
            Err(reason) => ctx.message(reason),
        }
        Step::Finished
    }

    fn prompt(&self) -> Prompt {
        Prompt::new("Centering the sheet's views")
    }

    fn expected(&self) -> ExpectedInput {
        ExpectedInput::new(InputKind::Nothing)
    }

    fn step(&mut self, _input: CommandInput, _ctx: &mut CommandContext<'_>) -> Step {
        Step::Finished
    }
}

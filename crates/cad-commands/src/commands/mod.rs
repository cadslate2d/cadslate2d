//! The built-in commands. Each is an explicit state machine (DESIGN §15).

/// The rest of the `Command` trait for commands that do all their work in
/// `start` and are therefore never prompted or stepped.
macro_rules! immediate_command {
    () => {
        fn prompt(&self) -> $crate::Prompt {
            $crate::Prompt::new("")
        }

        fn expected(&self) -> cad_input::ExpectedInput {
            cad_input::ExpectedInput::command()
        }

        fn step(
            &mut self,
            _input: $crate::CommandInput,
            _ctx: &mut $crate::CommandContext<'_>,
        ) -> $crate::Step {
            $crate::Step::Finished
        }
    };
}

mod files;
mod history;
mod line;

pub(crate) use files::{ExportCommand, OpenCommand};
pub(crate) use history::{RedoCommand, UndoCommand};
pub(crate) use line::LineCommand;

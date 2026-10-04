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

mod arc;
mod circle;
mod files;
mod grid;
mod history;
mod line;
mod mirror;
mod modify;
mod osnap;
mod pline;
mod polarang;
mod rectang;
mod transform;

/// Option keywords shared by more than one command.
mod keywords {
    use cad_input::Keyword;

    pub(crate) const CLOSE: Keyword = Keyword::new("Close");
    pub(crate) const UNDO: Keyword = Keyword::new("Undo");
}

pub(crate) use arc::ArcCommand;
pub(crate) use circle::CircleCommand;
pub(crate) use files::{ExportCommand, OpenCommand};
pub(crate) use grid::GridCommand;
pub(crate) use history::{RedoCommand, UndoCommand};
pub(crate) use line::LineCommand;
pub(crate) use mirror::MirrorCommand;
pub(crate) use modify::EraseCommand;
pub(crate) use osnap::OsnapCommand;
pub(crate) use pline::PlineCommand;
pub(crate) use polarang::PolarAngleCommand;
pub(crate) use rectang::RectangCommand;
pub(crate) use transform::TransformCommand;

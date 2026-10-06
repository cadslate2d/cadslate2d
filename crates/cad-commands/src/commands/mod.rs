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
mod block;
#[path = "break_.rs"]
mod break_command;
mod chprop;
mod circle;
mod color;
mod corner;
mod dimcurve;
mod dimlinear;
mod dimstyle;
mod dimtext;
mod edge;
mod files;
mod grid;
pub(crate) mod grip;
pub(crate) mod hatch;
mod history;
mod join;
mod layer;
mod line;
mod linetype;
mod ltscale;
mod lweight;
mod mirror;
mod modify;
mod mtext;
mod offset;
mod osnap;
mod pline;
mod polarang;
mod rectang;
mod stretch;
mod text;
mod transform;
mod units;

/// Option keywords shared by more than one command.
mod keywords {
    use cad_input::Keyword;

    pub(crate) const CLOSE: Keyword = Keyword::new("Close");
    pub(crate) const UNDO: Keyword = Keyword::new("Undo");
}

pub(crate) use arc::ArcCommand;
pub(crate) use block::{BlockCommand, ExplodeCommand, InsertCommand};
pub(crate) use break_command::BreakCommand;
pub(crate) use chprop::ChpropCommand;
pub(crate) use circle::CircleCommand;
pub(crate) use color::ColorCommand;
pub(crate) use corner::CornerCommand;
pub(crate) use dimcurve::{DimAngularCommand, DimRadialCommand};
pub(crate) use dimlinear::DimLinearCommand;
pub(crate) use dimstyle::DimStyleCommand;
pub(crate) use edge::EdgeCommand;
pub(crate) use files::{ExportCommand, OpenCommand, SaveCommand};
pub(crate) use grid::GridCommand;
pub(crate) use grip::GripEditCommand;
pub(crate) use hatch::HatchCommand;
pub(crate) use history::{RedoCommand, UndoCommand};
pub(crate) use join::JoinCommand;
pub(crate) use layer::LayerCommand;
pub(crate) use line::LineCommand;
pub(crate) use linetype::LinetypeCommand;
pub(crate) use ltscale::LtscaleCommand;
pub(crate) use lweight::LweightCommand;
pub(crate) use mirror::MirrorCommand;
pub(crate) use modify::EraseCommand;
pub(crate) use mtext::MTextCommand;
pub(crate) use offset::OffsetCommand;
pub(crate) use osnap::OsnapCommand;
pub(crate) use pline::PlineCommand;
pub(crate) use polarang::PolarAngleCommand;
pub(crate) use rectang::RectangCommand;
pub(crate) use stretch::StretchCommand;
pub(crate) use text::{TextCommand, TextEditCommand};
pub(crate) use transform::TransformCommand;
pub(crate) use units::UnitsCommand;

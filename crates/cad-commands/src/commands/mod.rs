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

mod aliases;
mod align;
mod arc;
mod array;
mod attributes;
pub(crate) mod block;
#[path = "break_.rs"]
mod break_command;
mod chprop;
mod circle;
mod clipboard;
mod color;
mod construction;
mod corner;
mod dimchain;
mod dimcurve;
mod dimlinear;
mod dimstyle;
mod dimtext;
mod donut;
mod edge;
mod ellipse;
mod files;
mod find;
mod grid;
pub(crate) mod grip;
mod group;
pub(crate) mod hatch;
mod hatch_look;
mod history;
mod housekeeping;
mod inquiry;
mod join;
mod layer;
mod layout;
mod laytools;
mod leader;
mod lengthen;
mod line;
mod linetype;
mod ltscale;
mod lweight;
mod mirror;
mod modify;
mod mtext;
mod mview;
mod offset;
mod osnap;
mod pedit;
mod pline;
mod plot;
mod points;
mod polarang;
mod polygon;
mod rectang;
mod revcloud;
mod select;
mod snap;
mod spline;
mod stretch;
mod style;
mod text;
mod transform;
mod units;
mod wblock;
mod zoom;

/// Option keywords shared by more than one command.
mod keywords {
    use cad_input::Keyword;

    pub(crate) const CLOSE: Keyword = Keyword::new("Close");
    pub(crate) const UNDO: Keyword = Keyword::new("Undo");
}

pub(crate) use aliases::{AliasEditCommand, SettingsCommand};
pub(crate) use align::AlignCommand;
pub(crate) use arc::ArcCommand;
pub(crate) use array::ArrayCommand;
#[cfg(test)]
pub(crate) use array::{Polar, Rectangular};
pub(crate) use attributes::{AttdefCommand, AtteditCommand};
pub(crate) use block::{BlockCommand, ExplodeCommand, InsertCommand};
pub(crate) use break_command::BreakCommand;
pub(crate) use chprop::ChpropCommand;
pub(crate) use circle::CircleCommand;
pub(crate) use clipboard::{Clipboard, CopyClipCommand, PasteClipCommand};
pub(crate) use color::ColorCommand;
pub(crate) use construction::{RayCommand, XLineCommand};
pub(crate) use corner::CornerCommand;
pub(crate) use dimchain::DimChainCommand;
pub(crate) use dimcurve::{DimAngularCommand, DimRadialCommand};
pub(crate) use dimlinear::DimLinearCommand;
pub(crate) use dimstyle::DimStyleCommand;
pub(crate) use donut::DonutCommand;
pub(crate) use edge::EdgeCommand;
pub(crate) use ellipse::EllipseCommand;
pub(crate) use files::{ExportCommand, OpenCommand, SaveCommand};
pub(crate) use find::FindCommand;
pub(crate) use grid::GridCommand;
pub(crate) use grip::GripEditCommand;
pub(crate) use group::{GroupCommand, PickStyleCommand, UngroupCommand};
pub(crate) use hatch::{HatchCommand, HatchEditCommand};
pub(crate) use history::{RedoCommand, UndoCommand};
pub(crate) use housekeeping::{MatchpropCommand, PurgeCommand, RenameCommand};
pub(crate) use inquiry::{AreaCommand, DistCommand, IdCommand, ListCommand};
pub(crate) use join::JoinCommand;
pub(crate) use layer::LayerCommand;
pub(crate) use layout::{LayoutCommand, ModelCommand, MspaceCommand, PspaceCommand};
pub(crate) use laytools::{
    Isolation, LayIsoCommand, LayUnisoCommand, LayerAllCommand, LayerPickCommand,
};
pub(crate) use leader::LeaderCommand;
pub(crate) use lengthen::LengthenCommand;
pub(crate) use line::LineCommand;
pub(crate) use linetype::LinetypeCommand;
pub(crate) use ltscale::LtscaleCommand;
pub(crate) use lweight::LweightCommand;
pub(crate) use mirror::MirrorCommand;
pub(crate) use modify::EraseCommand;
pub(crate) use mtext::MTextCommand;
pub(crate) use mview::MviewCommand;
pub(crate) use offset::OffsetCommand;
pub(crate) use osnap::OsnapCommand;
pub(crate) use pedit::PeditCommand;
pub(crate) use pline::PlineCommand;
pub(crate) use plot::PlotCommand;
pub(crate) use points::{DivideCommand, MeasureCommand, PointCommand, PtypeCommand};
pub(crate) use polarang::PolarAngleCommand;
pub(crate) use polygon::PolygonCommand;
pub(crate) use rectang::RectangCommand;
pub(crate) use revcloud::RevCloudCommand;
pub(crate) use select::{QselectCommand, SelectCommand};
pub(crate) use snap::SnapCommand;
pub(crate) use spline::SplineCommand;
pub(crate) use stretch::StretchCommand;
pub(crate) use style::StyleCommand;
pub(crate) use text::{TextCommand, TextEditCommand};
pub(crate) use transform::TransformCommand;
pub(crate) use units::UnitsCommand;
pub(crate) use wblock::WBlockCommand;
pub(crate) use zoom::{PanCommand, ZoomCommand};

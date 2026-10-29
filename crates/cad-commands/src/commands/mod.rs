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
mod bomtable;
mod boundary;
#[path = "break_.rs"]
mod break_command;
mod bsection;
mod centerviews;
mod chprop;
mod circle;
mod clipboard;
mod color;
mod constrain;
mod construction;
mod corner;
mod dimchain;
mod dimconstrain;
mod dimcurve;
mod dimlinear;
mod dimordinate;
mod dimstyle;
mod dimtext;
mod donut;
mod draworder;
mod edge;
mod elevation;
mod ellipse;
mod files;
mod find;
mod floorplan;
mod generators;
mod grid;
pub(crate) mod grip;
mod group;
pub(crate) mod hatch;
mod hatch_look;
mod history;
mod housekeeping;
mod inquiry;
mod isoplane;
mod join;
mod layer;
mod layerstate;
mod laymerge;
mod layout;
mod laytools;
mod leader;
mod lengthen;
mod line;
mod linetype;
mod ltscale;
mod lweight;
mod members;
mod mirror;
mod mleader;
mod mleaderedit;
mod mleaderstyle;
mod modify;
mod mtext;
mod mview;
mod notes;
mod offset;
mod opening;
mod osnap;
mod overkill;
mod partviews;
mod pedit;
mod placement;
mod pline;
mod plot;
mod points;
mod polarang;
mod polygon;
mod rectang;
mod revcloud;
mod select;
mod sketch;
mod snap;
mod spline;
mod splinedit;
mod stair;
mod stretch;
mod style;
mod text;
mod titleblock;
mod transform;
mod units;
mod view;
mod viewframe;
mod walls;
mod wblock;
mod wipeout;
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
pub(crate) use bomtable::{BalloonCommand, BomTableCommand};
pub(crate) use boundary::BoundaryCommand;
pub(crate) use break_command::BreakCommand;
pub(crate) use bsection::BuildingSectionCommand;
pub(crate) use centerviews::CenterViewsCommand;
pub(crate) use chprop::ChpropCommand;
pub(crate) use circle::CircleCommand;
pub(crate) use clipboard::{Clipboard, CopyClipCommand, PasteClipCommand};
pub(crate) use color::ColorCommand;
pub(crate) use constrain::{
    ConstraintDisplayCommand, DelConstraintCommand, Geom, GeomConstraintCommand,
};
pub(crate) use construction::{RayCommand, XLineCommand};
pub(crate) use corner::CornerCommand;
pub(crate) use dimchain::DimChainCommand;
pub(crate) use dimconstrain::{DimConstraintCommand, Form, ParametersCommand};
pub(crate) use dimcurve::{DimAngularCommand, DimRadialCommand};
pub(crate) use dimlinear::DimLinearCommand;
pub(crate) use dimordinate::{DimOrdinateCommand, QdimCommand};
pub(crate) use dimstyle::DimStyleCommand;
pub(crate) use donut::DonutCommand;
pub(crate) use draworder::{DrawOrderCommand, TextToFrontCommand};
pub(crate) use edge::EdgeCommand;
pub(crate) use elevation::ElevationCommand;
pub(crate) use ellipse::EllipseCommand;
pub(crate) use files::{DxfOutCommand, ExportCommand, NewCommand, OpenCommand, SaveCommand};
pub(crate) use find::FindCommand;
pub(crate) use floorplan::FloorPlanCommand;
pub(crate) use generators::{BOLT_CIRCLE, FLANGE, GeneratorCommand, HOLE_PATTERN, PLATE, SLOT};
pub(crate) use grid::GridCommand;
pub(crate) use grip::GripEditCommand;
pub(crate) use group::{GroupCommand, PickStyleCommand, UngroupCommand};
pub(crate) use hatch::{HatchCommand, HatchEditCommand};
pub(crate) use history::{RedoCommand, UndoCommand};
pub(crate) use housekeeping::{MatchpropCommand, PurgeCommand, RenameCommand};
pub(crate) use inquiry::{AreaCommand, DistCommand, IdCommand, ListCommand};
pub(crate) use isoplane::IsoplaneCommand;
pub(crate) use join::JoinCommand;
pub(crate) use layer::LayerCommand;
pub(crate) use layerstate::LayerStateCommand;
pub(crate) use laymerge::LayerMergeCommand;
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
pub(crate) use members::MembersCommand;
pub(crate) use mirror::MirrorCommand;
pub(crate) use mleader::MLeaderCommand;
pub(crate) use mleaderedit::MLeaderEditCommand;
pub(crate) use mleaderstyle::MLeaderStyleCommand;
pub(crate) use modify::EraseCommand;
pub(crate) use mtext::MTextCommand;
pub(crate) use mview::MviewCommand;
pub(crate) use notes::{NOTES_LAYER, NotesCommand};
pub(crate) use offset::OffsetCommand;
pub(crate) use opening::{OpeningCommand, OpeningKind};
pub(crate) use osnap::OsnapCommand;
pub(crate) use overkill::OverkillCommand;
pub(crate) use partviews::PartViewsCommand;
pub(crate) use pedit::PeditCommand;
pub(crate) use placement::{ArrayGridCommand, ArrayPathCommand, ConcentricCommand, TanArcCommand};
pub(crate) use pline::PlineCommand;
pub(crate) use plot::PlotCommand;
pub(crate) use points::{DivideCommand, MeasureCommand, PointCommand, PtypeCommand};
pub(crate) use polarang::PolarAngleCommand;
pub(crate) use polygon::PolygonCommand;
pub(crate) use rectang::RectangCommand;
pub(crate) use revcloud::RevCloudCommand;
pub(crate) use select::{QselectCommand, SelectCommand};
pub(crate) use sketch::SketchCommand;
pub(crate) use snap::SnapCommand;
pub(crate) use spline::SplineCommand;
pub(crate) use splinedit::SplineEditCommand;
pub(crate) use stair::StairCommand;
pub(crate) use stretch::StretchCommand;
pub(crate) use style::StyleCommand;
pub(crate) use text::{TextCommand, TextEditCommand};
pub(crate) use titleblock::TitleBlockCommand;
pub(crate) use transform::TransformCommand;
pub(crate) use units::UnitsCommand;
pub(crate) use view::ViewCommand;
pub(crate) use viewframe::ViewFrameCommand;
pub(crate) use walls::WallsCommand;
pub(crate) use wblock::WBlockCommand;
pub(crate) use wipeout::WipeoutCommand;
pub(crate) use zoom::{PanCommand, ZoomCommand};

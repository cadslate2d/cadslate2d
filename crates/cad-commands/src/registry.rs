//! The command registry.
//!
//! Commands are registered, not hard-coded: adding one touches this table and
//! nothing in the resolver or the session (open/closed principle). Typed
//! input, shortcuts, and UI actions all reach commands through it.

use std::collections::HashMap;

use cad_input::{CommandAliases, DuplicateCommandName};

use crate::Command;
use crate::commands::{
    AliasEditCommand, AlignCommand, ArcCommand, AreaCommand, ArrayCommand, ArrayGridCommand,
    ArrayPathCommand, AttdefCommand, AtteditCommand, BOLT_CIRCLE, BalloonCommand, BlockCommand,
    BomTableCommand, BoundaryCommand, BreakCommand, BuildingSectionCommand, CenterViewsCommand,
    ChpropCommand, CircleCommand, ColorCommand, ConcentricCommand, ConstraintDisplayCommand,
    CopyClipCommand, CornerCommand, DelConstraintCommand, DimAngularCommand, DimChainCommand,
    DimConstraintCommand, DimLinearCommand, DimOrdinateCommand, DimRadialCommand, DimStyleCommand,
    DistCommand, DivideCommand, DonutCommand, DrawOrderCommand, DxfOutCommand, EdgeCommand,
    ElevationCommand, EllipseCommand, EraseCommand, ExplodeCommand, ExportCommand, FLANGE,
    FindCommand, FloorPlanCommand, Form, GeneratorCommand, Geom, GeomConstraintCommand,
    GridCommand, GroupCommand, HOLE_PATTERN, HatchCommand, HatchEditCommand, IdCommand,
    InsertCommand, IsoplaneCommand, JoinCommand, LayIsoCommand, LayUnisoCommand, LayerAllCommand,
    LayerCommand, LayerMergeCommand, LayerPickCommand, LayerStateCommand, LayoutCommand,
    LeaderCommand, LengthenCommand, LineCommand, LinetypeCommand, ListCommand, LtscaleCommand,
    LweightCommand, MLeaderCommand, MLeaderEditCommand, MLeaderStyleCommand, MTextCommand,
    MatchpropCommand, MeasureCommand, MembersCommand, MirrorCommand, ModelCommand, MspaceCommand,
    MviewCommand, NewCommand, NotesCommand, OffsetCommand, OpenCommand, OpeningCommand,
    OpeningKind, OsnapCommand, OverkillCommand, PLATE, PanCommand, ParametersCommand,
    PartViewsCommand, PasteClipCommand, PeditCommand, PickStyleCommand, PlineCommand, PlotCommand,
    PointCommand, PolarAngleCommand, PolygonCommand, PspaceCommand, PtypeCommand, PurgeCommand,
    QdimCommand, QselectCommand, RayCommand, RectangCommand, RedoCommand, RenameCommand,
    RevCloudCommand, SLOT, SaveCommand, SelectCommand, SettingsCommand, SketchCommand, SnapCommand,
    SplineCommand, SplineEditCommand, StairCommand, StretchCommand, StyleCommand, TanArcCommand,
    TextCommand, TextEditCommand, TextToFrontCommand, TitleBlockCommand, TransformCommand,
    UndoCommand, UngroupCommand, UnitsCommand, ViewCommand, ViewFrameCommand, WBlockCommand,
    WipeoutCommand, XLineCommand, ZoomCommand,
};
use crate::session::select_keywords;

type CommandFactory = fn() -> Box<dyn Command>;

struct Registration {
    name: &'static str,
    factory: CommandFactory,
}

pub struct CommandRegistry {
    aliases: CommandAliases,
    registrations: HashMap<String, Registration>,
}

impl std::fmt::Debug for CommandRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut names: Vec<&str> = self.registrations.values().map(|r| r.name).collect();
        names.sort_unstable();
        f.debug_struct("CommandRegistry")
            .field("commands", &names)
            .finish()
    }
}

impl CommandRegistry {
    pub fn empty() -> Self {
        Self {
            aliases: CommandAliases::new(),
            registrations: HashMap::new(),
        }
    }

    /// The commands that ship with CadSlate2D.
    pub fn standard() -> Self {
        let mut registry = Self::empty();
        let standard: [(&'static str, &[&str], CommandFactory); 180] = [
            ("XLINE", &["XL"], || Box::new(XLineCommand::new())),
            ("RAY", &[], || Box::new(RayCommand::new())),
            ("LEADER", &["LE", "LEAD"], || Box::new(LeaderCommand::new())),
            ("PEDIT", &["PE"], || Box::new(PeditCommand::new())),
            ("DONUT", &["DO", "DOUGHNUT"], || {
                Box::new(DonutCommand::new())
            }),
            ("LENGTHEN", &["LEN"], || Box::new(LengthenCommand::new())),
            ("ALIGN", &["AL"], || Box::new(AlignCommand::new())),
            ("POLYGON", &["POL"], || Box::new(PolygonCommand::new())),
            ("MATCHPROP", &["MA"], || Box::new(MatchpropCommand::new())),
            ("PURGE", &["PU"], || Box::new(PurgeCommand)),
            ("RENAME", &["REN"], || Box::new(RenameCommand::new())),
            ("ARRAY", &["AR"], || Box::new(ArrayCommand::new())),
            ("ARRAYPATH", &[], || Box::new(ArrayPathCommand::new())),
            ("ARRAYGRID", &[], || Box::new(ArrayGridCommand::new())),
            ("POINT", &["PO"], || Box::new(PointCommand)),
            ("DIVIDE", &["DIV"], || Box::new(DivideCommand::new())),
            ("MEASURE", &["ME"], || Box::new(MeasureCommand::new())),
            ("PTYPE", &["DDPTYPE"], || Box::new(PtypeCommand::new())),
            ("DIST", &["DI"], || Box::new(DistCommand::new())),
            ("ID", &[], || Box::new(IdCommand)),
            ("AREA", &["AA"], || Box::new(AreaCommand::new())),
            ("LIST", &["LI", "LS"], || Box::new(ListCommand)),
            ("PLOT", &["PDFOUT"], || Box::new(PlotCommand::new())),
            ("MVIEW", &["MV"], || Box::new(MviewCommand::new())),
            ("LAYOUT", &["LO"], || Box::new(LayoutCommand::new())),
            ("MODEL", &[], || Box::new(ModelCommand)),
            ("PSPACE", &["PS"], || Box::new(PspaceCommand)),
            ("MSPACE", &["MS"], || Box::new(MspaceCommand)),
            ("ALIASEDIT", &[], || Box::new(AliasEditCommand::new())),
            ("SETTINGS", &[], || Box::new(SettingsCommand)),
            ("SELECT", &[], || Box::new(SelectCommand::new())),
            // The selection methods by their full names at the idle prompt
            // (#207): their short forms there are other commands' aliases.
            ("FENCE", &[], || {
                Box::new(SelectCommand::opening_with(select_keywords::FENCE))
            }),
            ("WPOLYGON", &["WP"], || {
                Box::new(SelectCommand::opening_with(select_keywords::WPOLYGON))
            }),
            ("CPOLYGON", &["CPOLY"], || {
                Box::new(SelectCommand::opening_with(select_keywords::CPOLYGON))
            }),
            ("PREVIOUS", &[], || {
                Box::new(SelectCommand::opening_with(select_keywords::PREVIOUS))
            }),
            ("LAST", &[], || {
                Box::new(SelectCommand::opening_with(select_keywords::LAST))
            }),
            ("ALL", &[], || {
                Box::new(SelectCommand::opening_with(select_keywords::ALL))
            }),
            ("REMOVE", &[], || {
                Box::new(SelectCommand::opening_with(select_keywords::REMOVE))
            }),
            ("QSELECT", &["QS"], || Box::new(QselectCommand::new())),
            ("ERASE", &["E"], || Box::new(EraseCommand)),
            ("GEOMCONSTRAINT", &["GCON"], || {
                Box::new(GeomConstraintCommand::new())
            }),
            ("GCHORIZONTAL", &[], || {
                Box::new(GeomConstraintCommand::of(Geom::Horizontal))
            }),
            ("GCVERTICAL", &[], || {
                Box::new(GeomConstraintCommand::of(Geom::Vertical))
            }),
            ("GCPERPENDICULAR", &[], || {
                Box::new(GeomConstraintCommand::of(Geom::Perpendicular))
            }),
            ("GCPARALLEL", &[], || {
                Box::new(GeomConstraintCommand::of(Geom::Parallel))
            }),
            ("GCTANGENT", &[], || {
                Box::new(GeomConstraintCommand::of(Geom::Tangent))
            }),
            ("GCCOINCIDENT", &[], || {
                Box::new(GeomConstraintCommand::of(Geom::Coincident))
            }),
            ("GCCONCENTRIC", &[], || {
                Box::new(GeomConstraintCommand::of(Geom::Concentric))
            }),
            ("GCCOLLINEAR", &[], || {
                Box::new(GeomConstraintCommand::of(Geom::Collinear))
            }),
            ("GCSYMMETRIC", &[], || {
                Box::new(GeomConstraintCommand::of(Geom::Symmetric))
            }),
            ("GCEQUAL", &[], || {
                Box::new(GeomConstraintCommand::of(Geom::Equal))
            }),
            ("GCFIX", &[], || {
                Box::new(GeomConstraintCommand::of(Geom::Fix))
            }),
            ("DELCONSTRAINT", &["DELCON"], || {
                Box::new(DelConstraintCommand)
            }),
            ("DIMCONSTRAINT", &["DCON"], || {
                Box::new(DimConstraintCommand::new())
            }),
            ("CONSTRAINTBAR", &["CBAR"], || {
                Box::new(ConstraintDisplayCommand::bars())
            }),
            ("DCDISPLAY", &[], || {
                Box::new(ConstraintDisplayCommand::dimensions())
            }),
            ("PLATE", &[], || Box::new(GeneratorCommand::new(&PLATE))),
            ("SLOT", &[], || Box::new(GeneratorCommand::new(&SLOT))),
            ("HOLEPATTERN", &[], || {
                Box::new(GeneratorCommand::new(&HOLE_PATTERN))
            }),
            ("BOLTCIRCLE", &[], || {
                Box::new(GeneratorCommand::new(&BOLT_CIRCLE))
            }),
            ("FLANGE", &[], || Box::new(GeneratorCommand::new(&FLANGE))),
            (
                "FLOORPLAN",
                &["FPLAN"],
                || Box::new(FloorPlanCommand::new()),
            ),
            ("WINDOW", &[], || {
                Box::new(OpeningCommand::new(OpeningKind::Window))
            }),
            ("DOOR", &[], || {
                Box::new(OpeningCommand::new(OpeningKind::Door))
            }),
            ("MEMBERS", &[], || Box::new(MembersCommand::new())),
            ("SKETCH", &[], || Box::new(SketchCommand::new())),
            ("STAIR", &[], || Box::new(StairCommand::new())),
            ("ELEVATION", &[], || Box::new(ElevationCommand::new())),
            ("BSECTION", &[], || Box::new(BuildingSectionCommand::new())),
            ("PARTVIEWS", &[], || Box::new(PartViewsCommand::new())),
            ("TITLEBLOCK", &[], || Box::new(TitleBlockCommand::new())),
            ("VIEWFRAME", &["VF"], || Box::new(ViewFrameCommand::new())),
            ("BOMTABLE", &["BOM"], || Box::new(BomTableCommand::new())),
            ("BALLOON", &[], || Box::new(BalloonCommand::new())),
            ("NOTES", &[], || Box::new(NotesCommand::new())),
            ("CENTERVIEWS", &[], || Box::new(CenterViewsCommand::new())),
            ("DCLINEAR", &[], || {
                Box::new(DimConstraintCommand::of(Form::Linear))
            }),
            ("DCHORIZONTAL", &[], || {
                Box::new(DimConstraintCommand::of(Form::Horizontal))
            }),
            ("DCVERTICAL", &[], || {
                Box::new(DimConstraintCommand::of(Form::Vertical))
            }),
            ("DCALIGNED", &[], || {
                Box::new(DimConstraintCommand::of(Form::Aligned))
            }),
            ("DCANGULAR", &[], || {
                Box::new(DimConstraintCommand::of(Form::Angular))
            }),
            ("DCRADIUS", &[], || {
                Box::new(DimConstraintCommand::of(Form::Radial))
            }),
            ("DCDIAMETER", &[], || {
                Box::new(DimConstraintCommand::of(Form::Diameter))
            }),
            ("-PARAMETERS", &["PARAMETERS", "PAR", "-PAR"], || {
                Box::new(ParametersCommand::new())
            }),
            ("MOVE", &["M"], || Box::new(TransformCommand::moving())),
            ("COPY", &["CO", "CP"], || {
                Box::new(TransformCommand::copying())
            }),
            ("ROTATE", &["RO"], || Box::new(TransformCommand::rotating())),
            ("SCALE", &["SC"], || Box::new(TransformCommand::scaling())),
            ("MIRROR", &["MI"], || Box::new(MirrorCommand::new())),
            ("OFFSET", &["O"], || Box::new(OffsetCommand::new())),
            ("TRIM", &["TR"], || Box::new(EdgeCommand::trimming())),
            ("EXTEND", &["EX"], || Box::new(EdgeCommand::extending())),
            ("BREAK", &["BR"], || Box::new(BreakCommand::new())),
            ("JOIN", &["J"], || Box::new(JoinCommand)),
            ("BLOCK", &["B"], || Box::new(BlockCommand::new())),
            ("MLEADER", &["MLD"], || Box::new(MLeaderCommand::new())),
            ("MLEADEREDIT", &["MLE"], || {
                Box::new(MLeaderEditCommand::new())
            }),
            ("MLEADERSTYLE", &["MLS"], || {
                Box::new(MLeaderStyleCommand::new())
            }),
            ("SPLINEDIT", &["SPE"], || Box::new(SplineEditCommand::new())),
            ("LAYMRG", &[], || Box::new(LayerMergeCommand::merge())),
            ("LAYDEL", &[], || Box::new(LayerMergeCommand::delete())),
            ("VIEW", &["V"], || Box::new(ViewCommand::new())),
            ("OVERKILL", &["OV"], || Box::new(OverkillCommand::new())),
            ("BOUNDARY", &["BO", "BPOLY"], || {
                Box::new(BoundaryCommand::new())
            }),
            ("INSERT", &["I"], || Box::new(InsertCommand::new())),
            ("ATTDEF", &["ATT"], || Box::new(AttdefCommand::new())),
            ("ATTEDIT", &["ATE"], || Box::new(AtteditCommand::new())),
            ("EXPLODE", &["X"], || Box::new(ExplodeCommand)),
            ("STRETCH", &["S"], || Box::new(StretchCommand::new())),
            ("LAYER", &["LA"], || Box::new(LayerCommand::new())),
            ("UNITS", &["UN"], || Box::new(UnitsCommand::new())),
            ("LWEIGHT", &["LW", "LINEWEIGHT"], || {
                Box::new(LweightCommand::new())
            }),
            ("LINETYPE", &["LT", "LTYPE"], || {
                Box::new(LinetypeCommand::new())
            }),
            ("LTSCALE", &["LTS"], || Box::new(LtscaleCommand::new())),
            ("CHPROP", &["CH", "PROPERTIES"], || {
                Box::new(ChpropCommand::new())
            }),
            (
                "COLOR",
                &["COL", "COLOUR"],
                || Box::new(ColorCommand::new()),
            ),
            ("FILLET", &["F"], || Box::new(CornerCommand::fillet())),
            ("CHAMFER", &["CHA"], || Box::new(CornerCommand::chamfer())),
            ("PLINE", &["PL"], || Box::new(PlineCommand::new())),
            ("RECTANG", &["REC", "RECTANGLE"], || {
                Box::new(RectangCommand::new())
            }),
            ("CIRCLE", &["C"], || Box::new(CircleCommand::new())),
            ("TANARC", &[], || Box::new(TanArcCommand::new())),
            ("CONCENTRIC", &[], || Box::new(ConcentricCommand::new())),
            ("ARC", &["A"], || Box::new(ArcCommand::new())),
            ("ELLIPSE", &["EL"], || Box::new(EllipseCommand::new())),
            ("SPLINE", &["SPL"], || Box::new(SplineCommand::new())),
            ("LINE", &["L"], || Box::new(LineCommand::new())),
            ("TEXT", &["DT", "DTEXT"], || Box::new(TextCommand::new())),
            ("MTEXT", &["MT", "T"], || Box::new(MTextCommand::new())),
            ("TEXTEDIT", &["ED", "DDEDIT"], || {
                Box::new(TextEditCommand::new())
            }),
            ("UNDO", &["U"], || Box::new(UndoCommand)),
            ("REDO", &[], || Box::new(RedoCommand)),
            ("OSNAP", &["OS"], || Box::new(OsnapCommand)),
            ("GRID", &[], || Box::new(GridCommand::default())),
            ("POLARANG", &[], || Box::new(PolarAngleCommand::default())),
            ("NEW", &["QNEW"], || Box::new(NewCommand)),
            ("OPEN", &["DXFIN"], || Box::new(OpenCommand)),
            ("SAVEAS", &["EXPORT"], || Box::new(ExportCommand::default())),
            ("DXFOUT", &[], || Box::new(DxfOutCommand)),
            ("SAVE", &["QSAVE"], || Box::new(SaveCommand)),
            ("DIMSTYLE", &["D", "DST"], || {
                Box::new(DimStyleCommand::new())
            }),
            ("DIMLINEAR", &["DLI", "DIMLIN"], || {
                Box::new(DimLinearCommand::linear())
            }),
            ("DIMALIGNED", &["DAL", "DIMALI"], || {
                Box::new(DimLinearCommand::aligned())
            }),
            ("DIMANGULAR", &["DAN", "DIMANG"], || {
                Box::new(DimAngularCommand::new())
            }),
            ("DIMRADIUS", &["DRA", "DIMRAD"], || {
                Box::new(DimRadialCommand::radius())
            }),
            ("DIMDIAMETER", &["DDI", "DIMDIA"], || {
                Box::new(DimRadialCommand::diameter())
            }),
            ("FIND", &[], || Box::new(FindCommand::new())),
            ("SNAP", &["SN"], || Box::new(SnapCommand::new())),
            ("ISOPLANE", &[], || Box::new(IsoplaneCommand::default())),
            ("STYLE", &["ST"], || Box::new(StyleCommand::new())),
            ("WBLOCK", &["W"], || Box::new(WBlockCommand::new())),
            ("COPYCLIP", &[], || Box::new(CopyClipCommand::copy())),
            ("CUTCLIP", &[], || Box::new(CopyClipCommand::cut())),
            ("COPYBASE", &[], || Box::new(CopyClipCommand::with_base())),
            ("PASTECLIP", &[], || Box::new(PasteClipCommand::new())),
            ("REVCLOUD", &[], || Box::new(RevCloudCommand::new())),
            ("DRAWORDER", &["DR"], || Box::new(DrawOrderCommand::new())),
            ("TEXTTOFRONT", &[], || Box::new(TextToFrontCommand)),
            ("WIPEOUT", &[], || Box::new(WipeoutCommand::new())),
            ("GROUP", &["G"], || Box::new(GroupCommand::new())),
            ("UNGROUP", &[], || Box::new(UngroupCommand)),
            ("PICKSTYLE", &[], || Box::new(PickStyleCommand::new())),
            ("LAYISO", &[], || Box::new(LayIsoCommand::new())),
            ("LAYUNISO", &[], || Box::new(LayUnisoCommand)),
            ("LAYOFF", &[], || Box::new(LayerPickCommand::off())),
            ("LAYON", &[], || Box::new(LayerAllCommand::on())),
            ("LAYFRZ", &[], || Box::new(LayerPickCommand::freeze())),
            ("LAYTHW", &[], || Box::new(LayerAllCommand::thaw())),
            ("LAYLCK", &[], || Box::new(LayerPickCommand::lock())),
            ("LAYULK", &[], || Box::new(LayerPickCommand::unlock())),
            (
                "LAYMCUR",
                &[],
                || Box::new(LayerPickCommand::make_current()),
            ),
            ("DIMORDINATE", &["DOR", "DIMORD"], || {
                Box::new(DimOrdinateCommand::new())
            }),
            ("QDIM", &[], || Box::new(QdimCommand::new())),
            (
                "LAYERSTATE",
                &["LAS"],
                || Box::new(LayerStateCommand::new()),
            ),
            ("DIMBASELINE", &["DBA", "DIMBASE"], || {
                Box::new(DimChainCommand::baseline())
            }),
            ("DIMCONTINUE", &["DCO", "DIMCONT"], || {
                Box::new(DimChainCommand::continued())
            }),
            ("HATCH", &["H", "BHATCH"], || Box::new(HatchCommand::new())),
            ("HATCHEDIT", &["HE"], || Box::new(HatchEditCommand::new())),
            ("ZOOM", &["Z"], || Box::new(ZoomCommand::new())),
            ("PAN", &["P"], || Box::new(PanCommand::new())),
        ];
        for (name, aliases, factory) in standard {
            registry
                .register(name, aliases, factory)
                .expect("standard command names are unique");
        }
        registry
    }

    /// Registers a command under a canonical name and aliases.
    pub fn register(
        &mut self,
        name: &'static str,
        aliases: &[&str],
        factory: CommandFactory,
    ) -> Result<(), DuplicateCommandName> {
        self.aliases.register(name, aliases)?;
        self.registrations
            .insert(name.to_owned(), Registration { name, factory });
        Ok(())
    }

    /// The table that maps typed text to canonical names.
    pub fn aliases(&self) -> &CommandAliases {
        &self.aliases
    }

    /// The same table, for the user's aliases to be set (#186).
    pub fn aliases_mut(&mut self) -> &mut CommandAliases {
        &mut self.aliases
    }

    /// A fresh instance of the command with this canonical name.
    pub(crate) fn create(&self, canonical: &str) -> Option<(&'static str, Box<dyn Command>)> {
        self.registrations
            .get(canonical)
            .map(|registration| (registration.name, (registration.factory)()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn standard_commands_are_reachable_by_name_and_alias() {
        let registry = CommandRegistry::standard();
        assert_eq!(registry.aliases().resolve("l"), Some("LINE"));
        let (name, _) = registry.create("LINE").unwrap();
        assert_eq!(name, "LINE");
    }

    #[test]
    fn unknown_commands_cannot_be_created() {
        assert!(CommandRegistry::standard().create("NOPE").is_none());
    }

    #[test]
    fn duplicate_registration_is_rejected() {
        let mut registry = CommandRegistry::standard();
        let result = registry.register("NOTYET", &["L"], || Box::new(LineCommand::new()));
        assert_eq!(result, Err(DuplicateCommandName("L".to_owned())));
        assert!(registry.create("NOTYET").is_none());
    }
}

// The menus of AutoCAD 2007/2008's classic workspace (#247), in its order,
// with what this program has of them. Each item runs a command as typed
// (ADR-0034); an item AutoCAD has and this program does not is left out
// rather than shown dead — the M18 review lists them.
import { PAUSE, SELECTION } from "../app/macro";
import {
  SEPARATOR,
  cascade,
  command,
  type Menu,
  type MenuContext,
  type MenuItem,
  type Panel,
} from "./menu-model";

const canUndo = ({ ui }: MenuContext) => ui.canUndo;
const canRedo = ({ ui }: MenuContext) => ui.canRedo;
const canPaste = ({ ui }: MenuContext) => ui.canPaste;
const onPaper = ({ ui }: MenuContext) => ui.paper;
const isometric = ({ ui }: MenuContext) => ui.isoplane !== "";

/** An item that shows or hides a part of the shell, checked while it shows. */
function panel(label: string, part: Panel): MenuItem {
  return {
    kind: "item",
    label,
    action: { panel: part },
    checked: ({ panels }) => panels[part],
  };
}

const FILE: Menu = {
  label: "&File",
  items: [
    command("&New...", "NEW"),
    command("&Open...", "OPEN", { shortcut: "Ctrl+O" }),
    SEPARATOR,
    command("&Save", "SAVE", { shortcut: "Ctrl+S" }),
    // The format is the user's answer (#319); the host asks where after.
    command("Save &As...", { command: "SAVEAS", steps: [PAUSE] }, { shortcut: "Ctrl+Shift+S" }),
    command("&Export...", { command: "EXPORT", steps: [PAUSE] }),
    SEPARATOR,
    command("&Plot...", "PLOT", { shortcut: "Ctrl+P" }),
    SEPARATOR,
    cascade("Drawing &Utilities", [command("&Purge...", "PURGE")]),
    command("&Write Block...", "WBLOCK"),
  ],
};

const EDIT: Menu = {
  label: "&Edit",
  items: [
    command("&Undo", "U", { shortcut: "Ctrl+Z", enabled: canUndo }),
    command("&Redo", "REDO", { shortcut: "Ctrl+Y", enabled: canRedo }),
    SEPARATOR,
    command("Cu&t", "CUTCLIP", { shortcut: "Ctrl+X" }),
    command("&Copy", "COPYCLIP", { shortcut: "Ctrl+C" }),
    command("Copy with &Base Point", "COPYBASE"),
    command("&Paste", "PASTECLIP", { shortcut: "Ctrl+V", enabled: canPaste }),
    SEPARATOR,
    command("Cle&ar", "ERASE", { shortcut: "Del" }),
    command("Select A&ll", "ALL", { shortcut: "Ctrl+A" }),
    SEPARATOR,
    command("&Find...", "FIND"),
  ],
};

const VIEW: Menu = {
  label: "&View",
  items: [
    cascade("&Zoom", [
      command("&Previous", { command: "ZOOM", steps: ["P"] }),
      command("&Window", { command: "ZOOM", steps: ["W"] }),
      command("&Center", { command: "ZOOM", steps: ["C"] }),
      command("&Scale", { command: "ZOOM", steps: ["S"] }),
      SEPARATOR,
      command("&All", { command: "ZOOM", steps: ["A"] }),
      command("&Extents", { command: "ZOOM", steps: ["E"] }),
    ]),
    cascade("Pa&n", [command("&Point", "PAN")]),
    SEPARATOR,
    command("Named V&iews...", "VIEW"),
    SEPARATOR,
    cascade("Vie&wports", [
      command("&New Viewport", "MVIEW", { enabled: onPaper }),
      command("&Fit Viewport", { command: "MVIEW", steps: ["F"] }, { enabled: onPaper }),
    ]),
    command("&Model Space", "MSPACE", { enabled: onPaper }),
    command("&Paper Space", "PSPACE", { enabled: onPaper }),
    SEPARATOR,
    // What shows the constraints (#269): the bars, and the dimensional
    // ones as dimensions. Checked while they are shown.
    cascade("Constraint&s", [
      command(
        "&Show Constraint Bars",
        { command: "CONSTRAINTBAR", steps: ["S"] },
        { checked: ({ ui }) => ui.constraintBars },
      ),
      command("&Hide Constraint Bars", { command: "CONSTRAINTBAR", steps: ["H"] }),
      SEPARATOR,
      command(
        "Show &Dimensional Constraints",
        { command: "DCDISPLAY", steps: ["S"] },
        { checked: ({ ui }) => ui.constraintDimensions },
      ),
      command("H&ide Dimensional Constraints", { command: "DCDISPLAY", steps: ["H"] }),
    ]),
    SEPARATOR,
    panel("&Toolbar", "toolbar"),
    panel("&Layers Panel", "layers"),
    panel("Pr&operties Panel", "properties"),
    panel("&AI Assistant", "ai"),
  ],
};

const INSERT: Menu = {
  label: "&Insert",
  items: [
    command("&Block...", "INSERT"),
    command("&File as Block...", { command: "INSERT", steps: ["~"] }),
    SEPARATOR,
    cascade("&Layout", [command("&New Layout", { command: "LAYOUT", steps: ["N"] })]),
    // A sheet's border and title block (#340).
    command("&Title Block...", "TITLEBLOCK"),
    command("&View Frame...", "VIEWFRAME"),
    command("Bill of &Materials...", "BOMTABLE"),
    command("Balloo&n", "BALLOON"),
    command("N&otes...", "NOTES"),
    command("Center View&s", "CENTERVIEWS"),
  ],
};

const LAYER_TOOLS = cascade("Layer T&ools", [
  command("Make Object's Layer &Current", "LAYMCUR"),
  SEPARATOR,
  command("Layer &Isolate", "LAYISO"),
  command("Layer &Unisolate", "LAYUNISO"),
  command("Layer &Off", "LAYOFF"),
  command("Turn All Layers O&n", "LAYON"),
  command("Layer &Freeze", "LAYFRZ"),
  command("&Thaw All Layers", "LAYTHW"),
  command("Layer &Lock", "LAYLCK"),
  command("Layer Unloc&k", "LAYULK"),
  SEPARATOR,
  command("Layer &Merge", "LAYMRG"),
  command("Layer &Delete", "LAYDEL"),
]);

const FORMAT: Menu = {
  label: "F&ormat",
  items: [
    command("&Layer...", "LAYER"),
    command("Layer St&ates Manager...", "LAYERSTATE"),
    LAYER_TOOLS,
    SEPARATOR,
    command("&Color...", "COLOR"),
    command("Li&netype...", "LINETYPE"),
    command("Line&weight...", "LWEIGHT"),
    command("Linetype &Scale...", "LTSCALE"),
    SEPARATOR,
    command("&Text Style...", "STYLE"),
    command("&Dimension Style...", "DIMSTYLE"),
    command("&Multileader Style...", "MLEADERSTYLE"),
    command("&Point Style...", "PTYPE"),
    SEPARATOR,
    command("&Units...", "UNITS"),
    command("&Rename...", "RENAME"),
  ],
};

const TOOLS: Menu = {
  label: "&Tools",
  items: [
    command("&Quick Select...", "QSELECT"),
    cascade("Draw &Order", [
      command("Bring to &Front", { command: "DRAWORDER", steps: [SELECTION, "F"] }),
      command("Send to &Back", { command: "DRAWORDER", steps: [SELECTION, "B"] }),
      command("Bring &Above Objects", { command: "DRAWORDER", steps: [SELECTION, "A"] }),
      command("Send &Under Objects", { command: "DRAWORDER", steps: [SELECTION, "U"] }),
      SEPARATOR,
      command("Bring &Text and Dimensions to Front", "TEXTTOFRONT"),
    ]),
    cascade("&Inquiry", [
      command("&Distance", "DIST"),
      command("&Area", "AREA"),
      command("&List", "LIST"),
      command("&ID Point", "ID"),
    ]),
    SEPARATOR,
    command("&Group", "GROUP"),
    command("&Ungroup", "UNGROUP"),
    SEPARATOR,
    cascade("Dra&fting Settings", [
      command("&Object Snap...", "OSNAP"),
      command("&Snap...", "SNAP"),
      // Isometric drafting (#336): the style, then the plane, F5.
      command(
        "&Isometric Snap",
        { command: "SNAP", steps: ["S", "I"] },
        { checked: ({ ui }) => ui.isoplane !== "" },
      ),
      command("Isometric Pla&ne", "ISOPLANE", { shortcut: "F5", enabled: isometric }),
      command("&Grid...", "GRID"),
      command("&Polar Angle...", "POLARANG"),
      command("Pic&k Style...", "PICKSTYLE"),
    ]),
    panel("AI Assis&tant", "ai"),
    SEPARATOR,
    command("Edit &Aliases...", "ALIASEDIT"),
    command("O&ptions...", "SETTINGS"),
  ],
};

const DRAW: Menu = {
  label: "&Draw",
  items: [
    command("&Line", "LINE"),
    command("&Ray", "RAY"),
    command("Constr&uction Line", "XLINE"),
    SEPARATOR,
    command("&Polyline", "PLINE"),
    command("Pol&ygon", "POLYGON"),
    command("Rectan&gle", "RECTANG"),
    SEPARATOR,
    cascade("&Arc", [
      command("&3 Points", "ARC"),
      SEPARATOR,
      command("&Start, Center, End", { command: "ARC", steps: [PAUSE, "C"] }),
      command("S&tart, Center, Angle", { command: "ARC", steps: [PAUSE, "C", PAUSE, "A"] }),
    ]),
    cascade("&Circle", [
      command("Center, &Radius", "CIRCLE"),
      command("Center, &Diameter", { command: "CIRCLE", steps: [PAUSE, "D"] }),
      SEPARATOR,
      command("&2 Points", { command: "CIRCLE", steps: ["2P"] }),
      command("&3 Points", { command: "CIRCLE", steps: ["3P"] }),
      SEPARATOR,
      // Kept tangent to what they touch (#273).
      command("&Tan, Tan, Radius", { command: "CIRCLE", steps: ["Ttr"] }),
      command("T&an, Tan, Tan", { command: "CIRCLE", steps: ["TTT"] }),
    ]),
    command("&Donut", "DONUT"),
    command("&Spline", "SPLINE"),
    cascade("&Ellipse", [
      command("&Center", { command: "ELLIPSE", steps: ["C"] }),
      command("&Axis, End", "ELLIPSE"),
      command("A&rc", { command: "ELLIPSE", steps: ["A"] }),
      command("&Isocircle", { command: "ELLIPSE", steps: ["I"] }, { enabled: isometric }),
    ]),
    cascade("&Block", [command("&Make...", "BLOCK"), command("&Define Attributes...", "ATTDEF")]),
    cascade("Poin&t", [
      command("&Multiple Point", "POINT"),
      command("&Divide", "DIVIDE"),
      command("M&easure", "MEASURE"),
    ]),
    SEPARATOR,
    { kind: "item", label: "&Hatch...", action: { dialog: "hatch" } },
    command("Bou&ndary...", "BOUNDARY"),
    command("&Wipeout", "WIPEOUT"),
    SEPARATOR,
    // Buildings: a floor plan from a brief (#281, ADR-0039), openings in
    // a drawn wall (#325), a stair by Blondel's rule (#331).
    cascade("Bu&ilding", [
      command("&Floor Plan...", "FLOORPLAN"),
      command("&Window", "WINDOW"),
      command("&Door", "DOOR"),
      command("&Stair...", "STAIR"),
      command("&Elevation...", "ELEVATION"),
      command("Se&ction...", "BSECTION"),
    ]),
    command("&Orthographic Views...", "PARTVIEWS"),
    command("Re&vision Cloud", "REVCLOUD"),
    // Freehand (#326).
    command("S&ketch", "SKETCH"),
    SEPARATOR,
    cascade("Te&xt", [
      command("&Multiline Text...", "MTEXT"),
      command("&Single Line Text", "TEXT"),
    ]),
  ],
};

const DIMENSION: Menu = {
  label: "Dime&nsion",
  items: [
    command("&Quick Dimension", "QDIM"),
    SEPARATOR,
    command("&Linear", "DIMLINEAR"),
    command("Ali&gned", "DIMALIGNED"),
    command("&Ordinate", "DIMORDINATE"),
    command("&Radius", "DIMRADIUS"),
    command("&Diameter", "DIMDIAMETER"),
    command("&Angular", "DIMANGULAR"),
    SEPARATOR,
    command("&Baseline", "DIMBASELINE"),
    command("&Continue", "DIMCONTINUE"),
    SEPARATOR,
    command("&Multileader", "MLEADER"),
    command("L&eader", "LEADER"),
    SEPARATOR,
    command("Dimension &Style...", "DIMSTYLE"),
  ],
};

const MODIFY: Menu = {
  label: "&Modify",
  items: [
    panel("&Properties", "properties"),
    command("&Match Properties", "MATCHPROP"),
    SEPARATOR,
    cascade("&Object", [
      command("&Hatch...", "HATCHEDIT"),
      command("&Polyline", "PEDIT"),
      command("&Spline", "SPLINEDIT"),
      command("&Multileader", "MLEADEREDIT"),
      command("&Attribute...", "ATTEDIT"),
      command("&Text...", "TEXTEDIT"),
    ]),
    SEPARATOR,
    command("&Erase", "ERASE"),
    command("Cop&y", "COPY"),
    command("M&irror", "MIRROR"),
    command("Off&set", "OFFSET"),
    command("&Array...", "ARRAY"),
    command("Mo&ve", "MOVE"),
    command("&Rotate", "ROTATE"),
    command("Sca&le", "SCALE"),
    command("Stretc&h", "STRETCH"),
    command("Len&gthen", "LENGTHEN"),
    command("&Trim", "TRIM"),
    command("Exten&d", "EXTEND"),
    command("Brea&k", "BREAK"),
    command("&Join", "JOIN"),
    command("&Chamfer", "CHAMFER"),
    command("&Fillet", "FILLET"),
    SEPARATOR,
    command("Alig&n", "ALIGN"),
    command("E&xplode", "EXPLODE"),
    // Centrelines made into members with a width (#329).
    command("Mem&bers", "MEMBERS"),
    command("Delete D&uplicate Objects", "OVERKILL"),
  ],
};

// The Parametric menu came with constraints in AutoCAD 2010 (ADR-0038, #267),
// between Modify and Window; it is placed there.
const PARAMETRIC: Menu = {
  label: "&Parametric",
  items: [
    cascade("&Geometric Constraints", [
      command("&Coincident", "GCCOINCIDENT"),
      command("Co&llinear", "GCCOLLINEAR"),
      command("Co&ncentric", "GCCONCENTRIC"),
      command("&Fix", "GCFIX"),
      command("&Parallel", "GCPARALLEL"),
      command("P&erpendicular", "GCPERPENDICULAR"),
      command("&Horizontal", "GCHORIZONTAL"),
      command("&Vertical", "GCVERTICAL"),
      command("&Tangent", "GCTANGENT"),
      command("&Symmetric", "GCSYMMETRIC"),
      command("E&qual", "GCEQUAL"),
    ]),
    cascade("D&imensional Constraints", [
      command("&Linear", "DCLINEAR"),
      command("&Horizontal", "DCHORIZONTAL"),
      command("&Vertical", "DCVERTICAL"),
      command("&Aligned", "DCALIGNED"),
      command("A&ngular", "DCANGULAR"),
      command("&Radius", "DCRADIUS"),
      command("&Diameter", "DCDIAMETER"),
    ]),
    // Constrained parts (#272), made as one.
    cascade("Pa&rts", [
      command("&Plate", "PLATE"),
      command("&Slot", "SLOT"),
      command("&Hole Pattern", "HOLEPATTERN"),
      command("&Bolt Circle", "BOLTCIRCLE"),
      command("&Flange", "FLANGE"),
    ]),
    // Placed against what is there, and kept so (#273).
    cascade("Pl&acement", [
      command("Tangent &Arc", "TANARC"),
      command("&Concentric Circle", "CONCENTRIC"),
      command("&Path Array", "ARRAYPATH"),
      command("&Grid Array", "ARRAYGRID"),
    ]),
    SEPARATOR,
    command("&Delete Constraints", "DELCONSTRAINT"),
    SEPARATOR,
    command("&Parameters", "-PARAMETERS"),
  ],
};

const HELP: Menu = {
  label: "&Help",
  items: [
    { kind: "item", label: "&Tutorial...", action: { dialog: "tutorial" } },
    SEPARATOR,
    { kind: "item", label: "&About CadSlate2D...", action: { dialog: "about" } },
  ],
};

/** The menu bar, left to right, as AutoCAD 2007/2008's classic workspace orders it. */
export const MENUS: readonly Menu[] = [
  FILE,
  EDIT,
  VIEW,
  INSERT,
  FORMAT,
  TOOLS,
  DRAW,
  DIMENSION,
  MODIFY,
  PARAMETRIC,
  HELP,
];

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
    command("Save &As...", "SAVEAS", { shortcut: "Ctrl+Shift+S" }),
    command("&Export...", "EXPORT"),
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
    panel("&Toolbar", "toolbar"),
    panel("&Layers Panel", "layers"),
    panel("Pr&operties Panel", "properties"),
  ],
};

const INSERT: Menu = {
  label: "&Insert",
  items: [
    command("&Block...", "INSERT"),
    command("&File as Block...", { command: "INSERT", steps: ["~"] }),
    SEPARATOR,
    cascade("&Layout", [command("&New Layout", { command: "LAYOUT", steps: ["N"] })]),
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
      command("&Grid...", "GRID"),
      command("&Polar Angle...", "POLARANG"),
      command("Pic&k Style...", "PICKSTYLE"),
    ]),
    command("Edit &Aliases...", "ALIASEDIT"),
    command("O&ptions...", "SETTINGS"),
  ],
};

const DRAW: Menu = {
  label: "&Draw",
  items: [
    command("&Line", "LINE"),
    command("&Ray", "RAY"),
    command("Cons&truction Line", "XLINE"),
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
    ]),
    command("&Donut", "DONUT"),
    command("&Spline", "SPLINE"),
    cascade("&Ellipse", [
      command("&Center", { command: "ELLIPSE", steps: ["C"] }),
      command("&Axis, End", "ELLIPSE"),
      command("A&rc", { command: "ELLIPSE", steps: ["A"] }),
    ]),
    cascade("&Block", [command("&Make...", "BLOCK"), command("&Define Attributes...", "ATTDEF")]),
    cascade("P&oint", [
      command("&Multiple Point", "POINT"),
      command("&Divide", "DIVIDE"),
      command("M&easure", "MEASURE"),
    ]),
    SEPARATOR,
    command("&Hatch...", "HATCH"),
    command("Bou&ndary...", "BOUNDARY"),
    command("&Wipeout", "WIPEOUT"),
    command("Re&vision Cloud", "REVCLOUD"),
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
    command("Delete D&uplicate Objects", "OVERKILL"),
  ],
};

const HELP: Menu = {
  label: "&Help",
  items: [{ kind: "item", label: "&About CadSlate2D...", action: { dialog: "about" } }],
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
  HELP,
];

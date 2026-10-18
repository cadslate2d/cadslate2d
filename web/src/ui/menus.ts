// The menu bar's menus, in AutoCAD 2007/2008's classic order (#246, #247).
// Each item runs a command as typed (ADR-0034).
import { SEPARATOR, cascade, command, type Menu, type MenuItem, type Panel } from "./menu-model";

/** A View item that shows or hides a part of the shell, checked while it shows. */
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
    command("Save &As...", "SAVEAS"),
    command("&Export...", "EXPORT"),
  ],
};

const VIEW: Menu = {
  label: "&View",
  items: [
    cascade("&Zoom", [
      command("&Previous", { command: "ZOOM", steps: ["P"] }),
      command("&Window", { command: "ZOOM", steps: ["W"] }),
      SEPARATOR,
      command("&All", { command: "ZOOM", steps: ["A"] }),
      command("&Extents", { command: "ZOOM", steps: ["E"] }),
    ]),
    command("&Pan", "PAN"),
    SEPARATOR,
    panel("&Toolbar", "toolbar"),
    panel("&Layers Panel", "layers"),
    panel("Pr&operties Panel", "properties"),
  ],
};

const TOOLS: Menu = {
  label: "&Tools",
  items: [command("O&ptions...", "SETTINGS")],
};

/** The menu bar, left to right. */
export const MENUS: readonly Menu[] = [FILE, VIEW, TOOLS];

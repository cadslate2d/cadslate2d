// The drawing area's shortcut menu, as AutoCAD's right click opens it: what
// it offers follows what the session is doing. During a command, Enter,
// Cancel, and the command's options; with objects selected, what can be done
// to them and their properties; otherwise the clipboard, undo, and the view.
// Like the menu bar, it is data, and every item is a macro over the command
// line, something typed into it, or a panel (ADR-0034).
import { SELECTION, type Macro } from "../app/macro";
import type { UiSnapshot } from "../app/ui-store";
import { cascade, command, SEPARATOR, type MenuEntry, type MenuItem } from "./menu-model";

/** The options a prompt offers, as it writes them: `[Close/Undo]`. */
export function promptOptions(prompt: string): readonly string[] {
  const words = /\[([^\]]+)\]/.exec(prompt)?.[1];
  if (words === undefined) return [];
  return words
    .split("/")
    .map((option) => option.trim())
    .filter((option) => /^[\p{L}\p{N}][\p{L}\p{N} -]*$/u.test(option));
}

/** An item that types `text` into the command line: `""` is Enter. */
function typing(label: string, text: string, shortcut?: string): MenuItem {
  return { kind: "item", label, action: { input: text }, ...(shortcut ? { shortcut } : {}) };
}

/** A command acting on the selection: it is the command's selection, asked for when there is none. */
function onSelection(label: string, name: string): MenuItem {
  const macro: Macro = { command: name, steps: [SELECTION] };
  return command(label, macro);
}

function repeat(ui: UiSnapshot): readonly MenuEntry[] {
  return ui.repeat ? [typing(`&Repeat ${ui.repeat}`, "", "Enter"), SEPARATOR] : [];
}

const clipboard = (ui: UiSnapshot, selected: boolean): readonly MenuEntry[] => [
  command("Cu&t", "CUTCLIP", { shortcut: "Ctrl+X", enabled: () => selected }),
  command("&Copy", "COPYCLIP", { shortcut: "Ctrl+C", enabled: () => selected }),
  command("Copy with &Base Point", "COPYBASE", { enabled: () => selected }),
  command("&Paste", "PASTECLIP", { shortcut: "Ctrl+V", enabled: () => ui.canPaste }),
];

export function shortcutMenu(ui: UiSnapshot): readonly MenuEntry[] {
  if (ui.command !== undefined) {
    const options = promptOptions(ui.prompt);
    return [
      typing("&Enter", "", "Enter"),
      { kind: "item", label: "&Cancel", shortcut: "Esc", action: { cancel: true } },
      ...(options.length > 0
        ? [SEPARATOR, ...options.map((option) => typing(option, option))]
        : []),
    ];
  }
  if (ui.selectionCount > 0) {
    return [
      ...repeat(ui),
      ...clipboard(ui, true),
      SEPARATOR,
      onSelection("&Erase", "ERASE"),
      onSelection("&Move", "MOVE"),
      onSelection("Cop&y Selection", "COPY"),
      onSelection("&Scale", "SCALE"),
      onSelection("R&otate", "ROTATE"),
      cascade("Draw &Order", [
        command("Bring to &Front", { command: "DRAWORDER", steps: [SELECTION, "F"] }),
        command("Send to &Back", { command: "DRAWORDER", steps: [SELECTION, "B"] }),
        command("Bring &Above Objects", { command: "DRAWORDER", steps: [SELECTION, "A"] }),
        command("Send &Under Objects", { command: "DRAWORDER", steps: [SELECTION, "U"] }),
      ]),
      SEPARATOR,
      { kind: "item", label: "&Deselect All", shortcut: "Esc", action: { cancel: true } },
      command("&Quick Select...", "QSELECT"),
      command("&Find...", "FIND"),
      SEPARATOR,
      { kind: "item", label: "Propert&ies", action: { show: "properties" } },
    ];
  }
  return [
    ...repeat(ui),
    ...clipboard(ui, false),
    SEPARATOR,
    command("&Undo", "U", { shortcut: "Ctrl+Z", enabled: () => ui.canUndo }),
    command("&Redo", "REDO", { shortcut: "Ctrl+Y", enabled: () => ui.canRedo }),
    SEPARATOR,
    command("&Pan", "PAN"),
    command("&Zoom Extents", { command: "ZOOM", steps: ["E"] }),
    SEPARATOR,
    command("&Quick Select...", "QSELECT"),
    command("&Find...", "FIND"),
  ];
}

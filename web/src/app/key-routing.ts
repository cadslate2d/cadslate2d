// Keyboard semantics from DESIGN §13.4, as a pure decision table.
// It decides which *kind* of key was pressed; it never interprets command text.

export type KeyIntent =
  /** Submit the command line (Enter, or Space when it accepts). */
  | { kind: "submit" }
  | { kind: "escape" }
  | { kind: "undo" }
  | { kind: "redo" }
  | { kind: "open" }
  | { kind: "save" }
  /** Ctrl+Shift+A: group selection on or off (#214). */
  | { kind: "group-selection" }
  /** F5, Ctrl+E: the next isometric plane (#336). */
  | { kind: "isoplane" }
  /** A key that starts a command, as its menu item does (#247): Ctrl+P, Ctrl+Shift+S. */
  | { kind: "command"; command: string }
  /** Ctrl+A with nothing typed: everything, at the idle prompt (#247). */
  | { kind: "select-all" }
  /** Ctrl+C, Ctrl+X, Ctrl+V with nothing typed: the drawing's clipboard (#223). */
  | { kind: "copy" }
  | { kind: "cut" }
  | { kind: "paste" }
  /** A drafting toggle's function key. */
  | { kind: "toggle"; toggle: "snap" | "osnap" | "otrack" | "grid" | "ortho" | "polar" }
  /** Delete with nothing typed: erase the selection. */
  | { kind: "delete" }
  | { kind: "history-previous" }
  | { kind: "history-next" }
  /** A key that edits the command line; make sure it has focus. */
  | { kind: "text" }
  /** Not ours: leave it to the browser. */
  | { kind: "ignore" };

export interface KeyLike {
  readonly key: string;
  readonly ctrlKey: boolean;
  readonly metaKey: boolean;
  readonly altKey: boolean;
  readonly shiftKey: boolean;
}

/** Function keys from DESIGN §13.4. Browsers bind several of these; the app takes them. */
const TOGGLE_KEYS: Readonly<
  Record<string, "snap" | "osnap" | "otrack" | "grid" | "ortho" | "polar">
> = {
  F3: "osnap",
  F7: "grid",
  F8: "ortho",
  F9: "snap",
  F11: "otrack",
  F10: "polar",
};

const EDITING_KEYS = new Set(["Backspace", "Delete", "ArrowLeft", "ArrowRight", "Home", "End"]);

/**
 * @param spaceAccepts whether Space submits like Enter at the current prompt
 * @param inputEmpty   whether the command line has no text; Delete then acts on the selection
 * @param textSelected whether words are selected on the page — a reply in the AI panel, a line
 *                     of the log: Ctrl+C and Ctrl+X then copy them, as on any page
 */
export function classifyKey(
  event: KeyLike,
  spaceAccepts: boolean,
  inputEmpty = false,
  textSelected = false,
): KeyIntent {
  const command = event.ctrlKey || event.metaKey;
  if (command) {
    const key = event.key.toLowerCase();
    if (key === "z") return { kind: event.shiftKey ? "redo" : "undo" };
    if (key === "y") return { kind: "redo" };
    if (key === "o") return { kind: "open" };
    if (key === "s" && !event.shiftKey) return { kind: "save" };
    if (key === "a" && event.shiftKey) return { kind: "group-selection" };
    if (key === "e" && !event.shiftKey) return { kind: "isoplane" };
    if (key === "p" && !event.shiftKey) return { kind: "command", command: "PLOT" };
    if (key === "s" && event.shiftKey) return { kind: "command", command: "SAVEAS" };
    // With text typed, Ctrl+A selects the text, as in any field.
    if (key === "a" && inputEmpty) return { kind: "select-all" };
    // With text typed, copying and pasting are the browser's, on the text;
    // with words selected on the page, copying them is.
    if (inputEmpty && !event.shiftKey) {
      if (key === "c") return textSelected ? { kind: "ignore" } : { kind: "copy" };
      if (key === "x") return textSelected ? { kind: "ignore" } : { kind: "cut" };
      if (key === "v") return { kind: "paste" };
    }
    // Reload, devtools, copy/paste, and every other browser shortcut stay with the browser.
    return { kind: "ignore" };
  }
  if (event.altKey) return { kind: "ignore" };

  const toggle = TOGGLE_KEYS[event.key];
  if (toggle) return { kind: "toggle", toggle };
  if (event.key === "F5") return { kind: "isoplane" };

  switch (event.key) {
    case "Enter":
      return { kind: "submit" };
    case " ":
      return spaceAccepts ? { kind: "submit" } : { kind: "text" };
    case "Escape":
      return { kind: "escape" };
    case "ArrowUp":
      return { kind: "history-previous" };
    case "ArrowDown":
      return { kind: "history-next" };
    case "Delete":
      return inputEmpty ? { kind: "delete" } : { kind: "text" };
    default:
      // Printable characters have a one-character `key`.
      return event.key.length === 1 || EDITING_KEYS.has(event.key)
        ? { kind: "text" }
        : { kind: "ignore" };
  }
}

/** What a key does while the menu bar has the keyboard (#248), as in a Windows menu. */
export type MenuKey =
  | { kind: "up" }
  | { kind: "down" }
  | { kind: "left" }
  | { kind: "right" }
  | { kind: "home" }
  | { kind: "end" }
  /** Enter or Space: run the highlighted item, or open its cascade. */
  | { kind: "choose" }
  /** Esc: close one level. */
  | { kind: "close" }
  /** Tab: leave the menus. */
  | { kind: "leave" }
  /** A letter or digit: the entry it underlines. */
  | { kind: "mnemonic"; key: string }
  /** A browser chord: not the menu's. */
  | { kind: "pass" }
  /** Anything else: swallowed, so it does not reach the command line. */
  | { kind: "none" };

const MENU_KEYS: Readonly<Record<string, MenuKey>> = {
  ArrowUp: { kind: "up" },
  ArrowDown: { kind: "down" },
  ArrowLeft: { kind: "left" },
  ArrowRight: { kind: "right" },
  Home: { kind: "home" },
  End: { kind: "end" },
  Enter: { kind: "choose" },
  " ": { kind: "choose" },
  Escape: { kind: "close" },
  Tab: { kind: "leave" },
};

export function classifyMenuKey(event: KeyLike): MenuKey {
  if (event.ctrlKey || event.metaKey) return { kind: "pass" };
  const known = MENU_KEYS[event.key];
  if (known) return known;
  if (/^[a-z0-9]$/i.test(event.key)) return { kind: "mnemonic", key: event.key.toLowerCase() };
  return { kind: "none" };
}

/** Alt and a letter, from anywhere: the menu that letter underlines (#248). */
export function menuShortcut(event: KeyLike): string | undefined {
  if (!event.altKey || event.ctrlKey || event.metaKey) return undefined;
  return /^[a-z]$/i.test(event.key) ? event.key.toLowerCase() : undefined;
}

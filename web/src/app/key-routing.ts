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
  /** A drafting toggle's function key. */
  | { kind: "toggle"; toggle: "osnap" | "grid" | "ortho" | "polar" }
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
const TOGGLE_KEYS: Readonly<Record<string, "osnap" | "grid" | "ortho" | "polar">> = {
  F3: "osnap",
  F7: "grid",
  F8: "ortho",
  F10: "polar",
};

const EDITING_KEYS = new Set(["Backspace", "Delete", "ArrowLeft", "ArrowRight", "Home", "End"]);

/**
 * @param spaceAccepts whether Space submits like Enter at the current prompt
 * @param inputEmpty   whether the command line has no text; Delete then acts on the selection
 */
export function classifyKey(event: KeyLike, spaceAccepts: boolean, inputEmpty = false): KeyIntent {
  const command = event.ctrlKey || event.metaKey;
  if (command) {
    const key = event.key.toLowerCase();
    if (key === "z") return { kind: event.shiftKey ? "redo" : "undo" };
    if (key === "y") return { kind: "redo" };
    if (key === "o") return { kind: "open" };
    if (key === "s") return { kind: "save" };
    if (key === "a" && event.shiftKey) return { kind: "group-selection" };
    // Reload, devtools, copy/paste, and every other browser shortcut stay with the browser.
    return { kind: "ignore" };
  }
  if (event.altKey) return { kind: "ignore" };

  const toggle = TOGGLE_KEYS[event.key];
  if (toggle) return { kind: "toggle", toggle };

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

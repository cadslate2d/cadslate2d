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

const EDITING_KEYS = new Set(["Backspace", "Delete", "ArrowLeft", "ArrowRight", "Home", "End"]);

export function classifyKey(event: KeyLike, spaceAccepts: boolean): KeyIntent {
  const command = event.ctrlKey || event.metaKey;
  if (command) {
    const key = event.key.toLowerCase();
    if (key === "z") return { kind: event.shiftKey ? "redo" : "undo" };
    if (key === "y") return { kind: "redo" };
    if (key === "o") return { kind: "open" };
    if (key === "s") return { kind: "save" };
    // Reload, devtools, copy/paste, and every other browser shortcut stay with the browser.
    return { kind: "ignore" };
  }
  if (event.altKey) return { kind: "ignore" };

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
    default:
      // Printable characters have a one-character `key`.
      return event.key.length === 1 || EDITING_KEYS.has(event.key)
        ? { kind: "text" }
        : { kind: "ignore" };
  }
}

import { describe, expect, test } from "vitest";
import { classifyKey, classifyMenuKey, menuShortcut, type KeyLike } from "./key-routing";

const key = (name: string, modifiers: Partial<Omit<KeyLike, "key">> = {}): KeyLike => ({
  key: name,
  ctrlKey: false,
  metaKey: false,
  altKey: false,
  shiftKey: false,
  ...modifiers,
});

const kind = (event: KeyLike, spaceAccepts = true) => classifyKey(event, spaceAccepts).kind;

describe("classifyKey", () => {
  test("Enter submits", () => {
    expect(kind(key("Enter"))).toBe("submit");
  });

  test("Space submits like Enter, unless the command wants literal spaces", () => {
    expect(kind(key(" "), true)).toBe("submit");
    expect(kind(key(" "), false)).toBe("text");
  });

  test("Escape, Up, and Down have their CAD meanings", () => {
    expect(kind(key("Escape"))).toBe("escape");
    expect(kind(key("ArrowUp"))).toBe("history-previous");
    expect(kind(key("ArrowDown"))).toBe("history-next");
  });

  test("F5 and Ctrl+E switch the isometric plane (#336)", () => {
    expect(kind(key("F5"))).toBe("isoplane");
    expect(kind(key("e", { ctrlKey: true }))).toBe("isoplane");
    expect(kind(key("E", { ctrlKey: true, shiftKey: true }))).toBe("ignore");
  });

  test("undo and redo chords, on any platform", () => {
    expect(kind(key("z", { ctrlKey: true }))).toBe("undo");
    expect(kind(key("z", { metaKey: true }))).toBe("undo");
    expect(kind(key("Z", { ctrlKey: true, shiftKey: true }))).toBe("redo");
    expect(kind(key("y", { ctrlKey: true }))).toBe("redo");
  });

  test("the menus' keys (#247): Ctrl+P plots, Ctrl+Shift+S saves as, Ctrl+A selects all with nothing typed", () => {
    expect(classifyKey(key("p", { ctrlKey: true }), true)).toEqual({
      kind: "command",
      command: "PLOT",
    });
    expect(classifyKey(key("S", { ctrlKey: true, shiftKey: true }), true)).toEqual({
      kind: "command",
      command: "SAVEAS",
    });
    expect(kind(key("s", { ctrlKey: true }))).toBe("save");
    expect(classifyKey(key("a", { ctrlKey: true }), true, true).kind).toBe("select-all");
    expect(classifyKey(key("a", { ctrlKey: true }), true, false).kind).toBe("ignore");
    expect(classifyKey(key("A", { ctrlKey: true, shiftKey: true }), true, true).kind).toBe(
      "group-selection",
    );
  });

  test("open and save chords replace the browser's own dialogs", () => {
    expect(kind(key("o", { ctrlKey: true }))).toBe("open");
    expect(kind(key("s", { metaKey: true }))).toBe("save");
  });

  test("Ctrl+C, Ctrl+X, Ctrl+V are the drawing's clipboard only with nothing typed", () => {
    const empty = (name: string) => classifyKey(key(name, { ctrlKey: true }), true, true).kind;
    expect([empty("c"), empty("x"), empty("v")]).toEqual(["copy", "cut", "paste"]);
    // Words selected on the page — a reply in the AI panel — are the
    // browser's to copy; pasting is still the drawing's.
    const selected = (name: string) =>
      classifyKey(key(name, { ctrlKey: true }), true, true, true).kind;
    expect([selected("c"), selected("x"), selected("v")]).toEqual(["ignore", "ignore", "paste"]);
    expect(classifyKey(key("v", { metaKey: true }), true, true).kind).toBe("paste");
    // With text typed, they are the browser's, on the text.
    expect(classifyKey(key("c", { ctrlKey: true }), true, false).kind).toBe("ignore");
  });

  test("Ctrl+Shift+A switches group selection; Ctrl+A stays the browser's", () => {
    expect(kind(key("A", { ctrlKey: true, shiftKey: true }))).toBe("group-selection");
    expect(kind(key("a", { ctrlKey: true }))).toBe("ignore");
  });

  test("Delete erases the selection only when nothing is being typed", () => {
    expect(classifyKey(key("Delete"), true, true).kind).toBe("delete");
    expect(classifyKey(key("Delete"), true, false).kind).toBe("text");
  });

  test("F3, F7, F8, F9, F10, and F11 are the drafting toggles", () => {
    const toggles = ["F3", "F7", "F8", "F9", "F10", "F11"].map((name) =>
      classifyKey(key(name), true),
    );
    expect(toggles).toEqual([
      { kind: "toggle", toggle: "osnap" },
      { kind: "toggle", toggle: "grid" },
      { kind: "toggle", toggle: "ortho" },
      { kind: "toggle", toggle: "snap" },
      { kind: "toggle", toggle: "polar" },
      { kind: "toggle", toggle: "otrack" },
    ]);
  });

  test("printable characters and editing keys go to the command line", () => {
    for (const name of [
      "L",
      "l",
      "0",
      ",",
      "@",
      "<",
      ".",
      "-",
      "Backspace",
      "ArrowLeft",
      "Delete",
    ]) {
      expect(kind(key(name)), name).toBe("text");
    }
    expect(kind(key("A", { shiftKey: true }))).toBe("text");
  });

  test("browser shortcuts and non-text keys are left alone", () => {
    expect(kind(key("r", { ctrlKey: true }))).toBe("ignore");
    expect(kind(key("c", { metaKey: true }))).toBe("ignore");
    expect(kind(key("I", { ctrlKey: true, shiftKey: true }))).toBe("ignore");
    expect(kind(key("d", { altKey: true }))).toBe("ignore");
    for (const name of ["F12", "F1", "Tab", "Shift", "Control", "PageDown"]) {
      expect(kind(key(name)), name).toBe("ignore");
    }
  });
});

describe("keys in the menus (#248)", () => {
  test("arrows, Home, End, Enter, Space, Esc, and Tab move, choose, close, and leave", () => {
    expect(classifyMenuKey(key("ArrowDown"))).toEqual({ kind: "down" });
    expect(classifyMenuKey(key("ArrowLeft"))).toEqual({ kind: "left" });
    expect(classifyMenuKey(key("Home"))).toEqual({ kind: "home" });
    expect(classifyMenuKey(key("Enter"))).toEqual({ kind: "choose" });
    expect(classifyMenuKey(key(" "))).toEqual({ kind: "choose" });
    expect(classifyMenuKey(key("Escape"))).toEqual({ kind: "close" });
    expect(classifyMenuKey(key("Tab"))).toEqual({ kind: "leave" });
  });

  test("a letter or digit is a mnemonic; browser chords pass; the rest is swallowed", () => {
    expect(classifyMenuKey(key("L"))).toEqual({ kind: "mnemonic", key: "l" });
    expect(classifyMenuKey(key("2"))).toEqual({ kind: "mnemonic", key: "2" });
    expect(classifyMenuKey(key("r", { ctrlKey: true }))).toEqual({ kind: "pass" });
    expect(classifyMenuKey(key("F5"))).toEqual({ kind: "none" });
  });

  test("Alt and a letter names a menu; Alt with Ctrl does not", () => {
    expect(menuShortcut(key("f", { altKey: true }))).toBe("f");
    expect(menuShortcut(key("O", { altKey: true }))).toBe("o");
    expect(menuShortcut(key("f"))).toBeUndefined();
    expect(menuShortcut(key("f", { altKey: true, ctrlKey: true }))).toBeUndefined();
  });
});

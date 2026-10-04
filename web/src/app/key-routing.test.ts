import { describe, expect, test } from "vitest";
import { classifyKey, type KeyLike } from "./key-routing";

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

  test("undo and redo chords, on any platform", () => {
    expect(kind(key("z", { ctrlKey: true }))).toBe("undo");
    expect(kind(key("z", { metaKey: true }))).toBe("undo");
    expect(kind(key("Z", { ctrlKey: true, shiftKey: true }))).toBe("redo");
    expect(kind(key("y", { ctrlKey: true }))).toBe("redo");
  });

  test("open and save chords replace the browser's own dialogs", () => {
    expect(kind(key("o", { ctrlKey: true }))).toBe("open");
    expect(kind(key("s", { metaKey: true }))).toBe("save");
  });

  test("Delete erases the selection only when nothing is being typed", () => {
    expect(classifyKey(key("Delete"), true, true).kind).toBe("delete");
    expect(classifyKey(key("Delete"), true, false).kind).toBe("text");
  });

  test("F3, F7, F8, and F10 are the drafting toggles", () => {
    const toggles = ["F3", "F7", "F8", "F10"].map((name) => classifyKey(key(name), true));
    expect(toggles).toEqual([
      { kind: "toggle", toggle: "osnap" },
      { kind: "toggle", toggle: "grid" },
      { kind: "toggle", toggle: "ortho" },
      { kind: "toggle", toggle: "polar" },
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
    for (const name of ["F5", "F12", "F1", "F9", "Tab", "Shift", "Control", "PageDown"]) {
      expect(kind(key(name)), name).toBe("ignore");
    }
  });
});

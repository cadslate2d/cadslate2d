import { describe, expect, test } from "vitest";
import { Modifier } from "../cad/engine";
import { isMiddleDoubleClick, modifierBits, wheelPixels } from "./canvas-controller";

const keys = (
  overrides: Partial<Record<"shiftKey" | "ctrlKey" | "metaKey" | "altKey", boolean>>,
) => ({
  shiftKey: false,
  ctrlKey: false,
  metaKey: false,
  altKey: false,
  ...overrides,
});

describe("modifierBits", () => {
  test("no modifiers is zero", () => {
    expect(modifierBits(keys({}))).toBe(0);
  });

  test("combines modifiers", () => {
    expect(modifierBits(keys({ shiftKey: true, altKey: true }))).toBe(
      Modifier.Shift | Modifier.Alt,
    );
  });

  test("treats the platform command key as Ctrl", () => {
    expect(modifierBits(keys({ metaKey: true }))).toBe(Modifier.Ctrl);
    expect(modifierBits(keys({ ctrlKey: true, metaKey: true }))).toBe(Modifier.Ctrl);
  });
});

describe("wheelPixels", () => {
  test("pixel deltas pass through", () => {
    expect(wheelPixels({ deltaY: -120, deltaMode: 0 }, 600)).toBe(-120);
  });

  test("line deltas are scaled to pixels", () => {
    expect(wheelPixels({ deltaY: 3, deltaMode: 1 }, 600)).toBe(48);
  });

  test("page deltas use the canvas height", () => {
    expect(wheelPixels({ deltaY: 1, deltaMode: 2 }, 600)).toBe(600);
  });
});

describe("isMiddleDoubleClick", () => {
  test("is the second click of the middle button, and nothing else", () => {
    expect(isMiddleDoubleClick({ button: 1, detail: 2 })).toBe(true);
    expect(isMiddleDoubleClick({ button: 1, detail: 1 })).toBe(false);
    expect(isMiddleDoubleClick({ button: 0, detail: 2 })).toBe(false);
    expect(isMiddleDoubleClick({ button: 1, detail: 3 })).toBe(false);
  });
});

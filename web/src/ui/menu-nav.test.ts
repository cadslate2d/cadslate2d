import { describe, expect, test } from "vitest";
import type { UiSnapshot } from "../app/ui-store";
import { SEPARATOR, cascade, command, type Menu, type MenuContext } from "./menu-model";
import { byMnemonic, nextSelectable, openedByKeyboard, stepMenu, type OpenState } from "./menu-nav";

const context: MenuContext = {
  ui: { canUndo: false } as UiSnapshot,
  panels: { toolbar: true, layers: true, properties: true, ai: false },
};

const undo = command("&Undo", "U", { enabled: ({ ui }) => ui.canUndo });
const MENUS: readonly Menu[] = [
  {
    label: "&Edit",
    items: [undo, command("&Redo", "REDO"), SEPARATOR, command("&Copy", "COPYCLIP")],
  },
  {
    label: "&Draw",
    items: [
      command("&Line", "LINE"),
      cascade("&Circle", [
        command("Center, &Radius", "CIRCLE"),
        command("&2 Points", { command: "CIRCLE", steps: ["2P"] }),
      ]),
    ],
  },
];
const edit = MENUS[0]?.items ?? [];

describe("moving through a menu from the keyboard (#248)", () => {
  test("passes over separators and greyed items, and wraps", () => {
    expect(nextSelectable(edit, undefined, 1, context)).toBe(1);
    expect(nextSelectable(edit, 1, 1, context)).toBe(3);
    expect(nextSelectable(edit, 3, 1, context)).toBe(1);
    expect(nextSelectable(edit, 1, -1, context)).toBe(3);
    expect(byMnemonic(edit, "c", context)).toBe(3);
    expect(byMnemonic(edit, "u", context), "greyed").toBeUndefined();
  });

  test("a title alone: Left and Right move along the bar, Down opens the menu", () => {
    const title: OpenState = { menu: 0, path: [], depth: 0, keyboard: true };
    expect(stepMenu(title, { kind: "left" }, MENUS, context).next?.menu).toBe(1);
    expect(stepMenu(title, { kind: "down" }, MENUS, context).next).toEqual({
      menu: 0,
      path: [1],
      depth: 1,
      keyboard: true,
    });
    expect(stepMenu(title, { kind: "mnemonic", key: "d" }, MENUS, context).next?.menu).toBe(1);
    expect(stepMenu(title, { kind: "close" }, MENUS, context).next).toBeUndefined();
  });

  test("Right opens a cascade or moves to the next menu; Left closes it or moves back", () => {
    const draw = openedByKeyboard(1, MENUS, context);
    const onCircle = stepMenu(draw, { kind: "down" }, MENUS, context).next as OpenState;
    expect(onCircle.path).toEqual([1]);
    const inside = stepMenu(onCircle, { kind: "right" }, MENUS, context).next as OpenState;
    expect(inside).toMatchObject({ path: [1, 0], depth: 2 });
    expect(stepMenu(inside, { kind: "left" }, MENUS, context).next).toMatchObject({
      path: [1],
      depth: 1,
    });
    expect(stepMenu(draw, { kind: "right" }, MENUS, context).next).toMatchObject({ menu: 0 });
    expect(stepMenu(draw, { kind: "left" }, MENUS, context).next).toMatchObject({ menu: 0 });
  });

  test("a letter or Enter runs an item, or opens a cascade; a greyed item does nothing", () => {
    const draw = openedByKeyboard(1, MENUS, context);
    const circle = stepMenu(draw, { kind: "mnemonic", key: "c" }, MENUS, context).next as OpenState;
    expect(circle.depth).toBe(2);
    const twoPoints = stepMenu(circle, { kind: "mnemonic", key: "2" }, MENUS, context);
    expect(twoPoints.next).toBeUndefined();
    expect(twoPoints.chosen?.label).toBe("&2 Points");
    expect(stepMenu(draw, { kind: "choose" }, MENUS, context).chosen?.label).toBe("&Line");
    const onUndo: OpenState = { menu: 0, path: [0], depth: 1, keyboard: false };
    expect(stepMenu(onUndo, { kind: "choose" }, MENUS, context)).toEqual({ next: onUndo });
  });

  test("Esc closes one level: to the title when the keyboard opened the menu, away when the pointer did", () => {
    const keyboard = openedByKeyboard(0, MENUS, context);
    expect(stepMenu(keyboard, { kind: "close" }, MENUS, context).next).toEqual({
      ...keyboard,
      depth: 0,
      path: [],
    });
    const pointer: OpenState = { ...keyboard, keyboard: false };
    expect(stepMenu(pointer, { kind: "close" }, MENUS, context).next).toBeUndefined();
    expect(stepMenu(keyboard, { kind: "leave" }, MENUS, context).next).toBeUndefined();
  });
});

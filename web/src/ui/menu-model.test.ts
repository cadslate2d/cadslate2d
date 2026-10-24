import { describe, expect, test } from "vitest";
import type { UiSnapshot } from "../app/ui-store";
import { entriesAt } from "./menu-nav";
import {
  SEPARATOR,
  allItems,
  cascade,
  command,
  mnemonic,
  selectable,
  type Menu,
  type MenuContext,
} from "./menu-model";
import { MENUS } from "./menus";

const context = (ui: Partial<UiSnapshot> = {}): MenuContext => ({
  ui: { canUndo: false, canRedo: false, selectionCount: 0, ...ui } as UiSnapshot,
  panels: { toolbar: true, layers: false, properties: true, ai: false },
});

describe("the menu model (ADR-0034)", () => {
  test("a label's mnemonic is the letter after its &", () => {
    expect(mnemonic("Save &As...")).toEqual({ text: "Save As...", index: 5, key: "a" });
    expect(mnemonic("&File")).toEqual({ text: "File", index: 0, key: "f" });
    expect(mnemonic("Plain")).toEqual({ text: "Plain", index: undefined, key: undefined });
  });

  test("an item is greyed out by its own rule; separators are never chosen", () => {
    const undo = command("&Undo", "U", { enabled: ({ ui }) => ui.canUndo });
    expect(selectable(undo, context())).toBe(false);
    expect(selectable(undo, context({ canUndo: true }))).toBe(true);
    expect(selectable(SEPARATOR, context())).toBe(false);
    expect(selectable(cascade("&Zoom", []), context())).toBe(true);
  });

  test("the open list at each level follows the path through cascades", () => {
    const zoom = cascade("&Zoom", [command("&Extents", { command: "ZOOM", steps: ["E"] })]);
    const items = [command("&Pan", "PAN"), SEPARATOR, zoom];
    expect(entriesAt(items, [], 0)).toBe(items);
    expect(entriesAt(items, [2], 1)).toBe(zoom.items);
    expect(entriesAt(items, [0], 1)).toEqual([]);
  });

  test("every item is reached, with the labels that lead to it", () => {
    const menu: Menu = { label: "&View", items: [cascade("&Zoom", [command("&All", "ZOOM")])] };
    expect([...allItems([menu])].map(({ path }) => path)).toEqual([["View", "Zoom", "All"]]);
  });

  test("the panels' items are checked while their panel shows", () => {
    const view = MENUS.find((menu) => mnemonic(menu.label).text === "View");
    const checks = [...allItems(view ? [view] : [])]
      .filter(({ item }) => item.checked)
      .map(({ path, item }) => [path.at(-1), item.checked?.(context())]);
    expect(checks).toEqual([
      ["Toolbar", true],
      ["Layers Panel", false],
      ["Properties Panel", true],
      ["AI Assistant", false],
    ]);
  });
});

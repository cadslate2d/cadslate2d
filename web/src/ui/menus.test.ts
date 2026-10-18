import { beforeAll, describe, expect, test } from "vitest";
import type { UiSnapshot } from "../app/ui-store";
import { CadEngine } from "../cad/engine";
import { loadEngineForTests } from "../cad/test-support";
import { allItems, mnemonic, type MenuContext, type MenuEntry } from "./menu-model";
import { MENUS } from "./menus";

beforeAll(loadEngineForTests);

/** What the command line says when a typed answer is refused, or a name is not a command. */
const REFUSAL = /is required\.$|^Unknown command/;

const context = (ui: Partial<UiSnapshot>): MenuContext => ({
  ui: { canUndo: false, canRedo: false, canPaste: false, paper: false, ...ui } as UiSnapshot,
  panels: { toolbar: true, layers: true, properties: true },
});

/** The item at `path`, by labels without their `&`. */
function item(...path: string[]) {
  const found = [...allItems(MENUS)].find((entry) => entry.path.join(" › ") === path.join(" › "));
  if (!found) throw new Error(`no menu item ${path.join(" › ")}`);
  return found.item;
}

describe("the menus of the classic workspace (#247)", () => {
  test("are AutoCAD's, in its order", () => {
    expect(MENUS.map((menu) => mnemonic(menu.label).text)).toEqual([
      "File",
      "Edit",
      "View",
      "Insert",
      "Format",
      "Tools",
      "Draw",
      "Dimension",
      "Modify",
      "Help",
    ]);
  });

  test("every item's command is one the core knows, and every macro without a pause runs without a refusal", () => {
    let macros = 0;
    for (const { path, item: entry } of allItems(MENUS)) {
      if (!("macro" in entry.action)) continue;
      macros += 1;
      const { command, steps = [] } = entry.action.macro;
      const engine = new CadEngine();
      engine.resize(800, 600);
      engine.invoke(command);
      for (const step of steps) {
        // A pause waits for the user: what follows is checked by hand.
        if (typeof step !== "string") break;
        engine.submitText(step);
      }
      const refused = engine.logSince(0).filter((line) => REFUSAL.test(line));
      expect(refused, path.join(" › ")).toEqual([]);
      engine.dispose();
    }
    expect(macros).toBeGreaterThan(100);
  });

  test("every list has a mnemonic per entry, none twice", () => {
    const check = (label: string, entries: readonly MenuEntry[]) => {
      const keys = entries.flatMap((entry) => {
        if (entry.kind === "separator") return [];
        const key = mnemonic(entry.label).key;
        expect(key, `${label}: ${entry.label}`).toBeDefined();
        return [key];
      });
      expect(new Set(keys).size, `${label}: ${keys.join("")}`).toBe(keys.length);
      for (const entry of entries) if (entry.kind === "cascade") check(entry.label, entry.items);
    };
    const titles = MENUS.map((menu) => mnemonic(menu.label).key);
    expect(new Set(titles).size).toBe(titles.length);
    for (const menu of MENUS) check(menu.label, menu.items);
  });

  test("greyed out when they cannot apply: Undo, Redo, Paste, and the viewports off paper", () => {
    const enabled = (path: string[], ui: Partial<UiSnapshot>) =>
      item(...path).enabled?.(context(ui)) ?? true;
    expect(enabled(["Edit", "Undo"], {})).toBe(false);
    expect(enabled(["Edit", "Undo"], { canUndo: true })).toBe(true);
    expect(enabled(["Edit", "Redo"], { canRedo: true })).toBe(true);
    expect(enabled(["Edit", "Paste"], {})).toBe(false);
    expect(enabled(["Edit", "Paste"], { canPaste: true })).toBe(true);
    expect(enabled(["View", "Viewports", "New Viewport"], {})).toBe(false);
    expect(enabled(["View", "Viewports", "New Viewport"], { paper: true })).toBe(true);
    expect(enabled(["Draw", "Line"], {})).toBe(true);
  });
});

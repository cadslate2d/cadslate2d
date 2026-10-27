import { beforeAll, describe, expect, test } from "vitest";
import type { UiSnapshot } from "../app/ui-store";
import { CadEngine } from "../cad/engine";
import { loadEngineForTests } from "../cad/test-support";
import { allItems, mnemonic, selectable, type MenuEntry } from "./menu-model";
import { promptOptions, shortcutMenu } from "./shortcut-menu";

beforeAll(loadEngineForTests);

const snapshot = (ui: Partial<UiSnapshot>): UiSnapshot =>
  ({
    prompt: "Command:",
    command: undefined,
    selectionCount: 0,
    repeat: undefined,
    canUndo: false,
    canRedo: false,
    canPaste: false,
    ...ui,
  }) as UiSnapshot;

const labels = (entries: readonly MenuEntry[]) =>
  entries.map((entry) => (entry.kind === "separator" ? "—" : mnemonic(entry.label).text));

const enabled = (entries: readonly MenuEntry[], label: string, ui: UiSnapshot) => {
  const entry = entries.find((e) => e.kind !== "separator" && mnemonic(e.label).text === label);
  if (!entry) throw new Error(`no ${label}`);
  return selectable(entry, {
    ui,
    panels: { toolbar: true, layers: true, properties: true, ai: true },
  });
};

describe("the drawing area's shortcut menu", () => {
  test("with nothing selected: repeat, the clipboard, undo, and the view", () => {
    const ui = snapshot({ repeat: "LINE", canUndo: true });
    const entries = shortcutMenu(ui);
    expect(labels(entries)).toEqual([
      "Repeat LINE",
      "—",
      "Cut",
      "Copy",
      "Copy with Base Point",
      "Paste",
      "—",
      "Undo",
      "Redo",
      "—",
      "Pan",
      "Zoom Extents",
      "—",
      "Quick Select...",
      "Find...",
    ]);
    expect(enabled(entries, "Cut", ui)).toBe(false);
    expect(enabled(entries, "Paste", ui)).toBe(false);
    expect(enabled(entries, "Undo", ui)).toBe(true);
    expect(enabled(entries, "Redo", ui)).toBe(false);
    // Repeat is Enter at the idle prompt; with no command yet there is none.
    expect(entries[0]?.kind === "item" && entries[0].action).toEqual({ input: "" });
    expect(labels(shortcutMenu(snapshot({})))[0]).toBe("Cut");
  });

  test("with objects selected: what can be done to them, and their properties", () => {
    const ui = snapshot({ selectionCount: 1, repeat: "MOVE" });
    const entries = shortcutMenu(ui);
    expect(labels(entries)).toEqual([
      "Repeat MOVE",
      "—",
      "Cut",
      "Copy",
      "Copy with Base Point",
      "Paste",
      "—",
      "Erase",
      "Move",
      "Copy Selection",
      "Scale",
      "Rotate",
      "Draw Order",
      "—",
      "Deselect All",
      "Quick Select...",
      "Find...",
      "—",
      "Properties",
    ]);
    expect(enabled(entries, "Cut", ui)).toBe(true);
    const action = (label: string) =>
      [...allItems([{ label: "", items: entries }])].find(({ path }) => path.at(-1) === label)?.item
        .action;
    expect(action("Properties")).toEqual({ show: "properties" });
    expect(action("Deselect All")).toEqual({ cancel: true });
    expect(action("Move")).toEqual({ macro: { command: "MOVE", steps: [{ pause: "selection" }] } });
  });

  test("during a command: Enter, Cancel, and the options its prompt offers", () => {
    const engine = new CadEngine();
    engine.resize(800, 600);
    for (const answer of ["0,0", "100,0", "100,100"]) {
      if (answer === "0,0") engine.invoke("LINE");
      engine.submitText(answer);
    }
    const state = engine.uiState();
    expect(state.prompt).toContain("[Close/Undo]");
    const entries = shortcutMenu(snapshot({ prompt: state.prompt, command: state.command }));
    expect(labels(entries)).toEqual(["Enter", "Cancel", "—", "Close", "Undo"]);
    const close = entries[3];
    expect(close?.kind === "item" && close.action).toEqual({ input: "Close" });
    // Typed, Close closes the line, as the menu would.
    engine.submitText("Close");
    expect(engine.uiState().command).toBeUndefined();
    expect(engine.uiState().repeat).toBe("LINE");
    engine.dispose();
  });

  test("a prompt's options are the words between its brackets", () => {
    expect(promptOptions("Save as or [DXF/DWG2000/DWG2004/DWG2007] <DXF>:")).toEqual([
      "DXF",
      "DWG2000",
      "DWG2004",
      "DWG2007",
    ]);
    expect(promptOptions("Specify first point:")).toEqual([]);
    expect(promptOptions("Enter an option [ON/OFF] <ON>:")).toEqual(["ON", "OFF"]);
  });

  test("every macro it offers is a command the core knows", () => {
    for (const ui of [snapshot({}), snapshot({ selectionCount: 1 })]) {
      for (const { path, item } of allItems([{ label: "", items: shortcutMenu(ui) }])) {
        if (!("macro" in item.action)) continue;
        const engine = new CadEngine();
        engine.resize(800, 600);
        engine.invoke(item.action.macro.command);
        const refused = engine.logSince(0).filter((line) => /^Unknown command/.test(line));
        expect(refused, path.join(" › ")).toEqual([]);
        engine.dispose();
      }
    }
  });
});

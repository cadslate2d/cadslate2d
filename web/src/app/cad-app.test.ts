import { beforeAll, describe, expect, test, vi } from "vitest";
import { CadEngine, Dirty, EngineFailure, KeyAction, ToggleBit } from "../cad/engine";
import { loadEngineForTests } from "../cad/test-support";
import { AUTOSAVE_DELAY_MS, CadApp, DISCARD_CHANGES_MESSAGE, SNAPSHOT_SLICE } from "./cad-app";
import type { AutosaveRecord, AutosaveStore, FileAccess, FileTarget } from "./host-services";
import { parseSettings } from "./host-services";
import type { HostServices, PickedFile, Settings, SettingsStore, Theme } from "./host-services";

beforeAll(loadEngineForTests);

const encode = (text: string) => new TextEncoder().encode(text);

/** A DXF file from `code value` pairs. */
const dxf = (...groups: [number, string][]) =>
  groups.map(([code, value]) => `${code}\n${value}\n`).join("");

const LINE: [number, string][] = [
  [0, "LINE"],
  [8, "0"],
  [10, "0.0"],
  [20, "0.0"],
  [11, "100.0"],
  [21, "50.0"],
];
const cleanFile = dxf([0, "SECTION"], [2, "ENTITIES"], ...LINE, [0, "ENDSEC"], [0, "EOF"]);
const mixedFile = dxf(
  [0, "SECTION"],
  [2, "ENTITIES"],
  ...LINE,
  [0, "TRACE"],
  [70, "8"],
  [0, "ENDSEC"],
  [0, "EOF"],
);

/** One slot in memory, counting what was written. */
class FakeAutosave implements AutosaveStore {
  record: AutosaveRecord | undefined;
  saves = 0;
  load(): Promise<AutosaveRecord | undefined> {
    return Promise.resolve(this.record);
  }
  save(record: AutosaveRecord): Promise<void> {
    this.saves += 1;
    this.record = record;
    return Promise.resolve();
  }
  clear(): Promise<void> {
    this.record = undefined;
    return Promise.resolve();
  }
}

/** Files in memory: a download path or a write-back path, as the test says. */
class FakeFiles implements FileAccess {
  writesBack = false;
  fileToPick: PickedFile | undefined;
  picks = 0;
  /** What `saveAs` was asked for: a download, or a picker. */
  saved: { name: string; text: string; mimeType: string }[] = [];
  /** What was written back to a target. */
  written: { name: string; text: string }[] = [];
  writeAnswer = true;

  open(): Promise<PickedFile | undefined> {
    this.picks += 1;
    return Promise.resolve(this.fileToPick);
  }

  saveAs(name: string, text: string, mimeType: string): Promise<FileTarget | undefined> {
    this.saved.push({ name, text, mimeType });
    return Promise.resolve({ name });
  }

  write(target: FileTarget, text: string): Promise<boolean> {
    if (!this.writeAnswer) return Promise.resolve(false);
    this.written.push({ name: target.name, text });
    return Promise.resolve(true);
  }
}

class FakeSettings implements SettingsStore {
  kept: Partial<Settings> = {};
  saves = 0;

  load(): Partial<Settings> {
    return this.kept;
  }

  save(settings: Settings): void {
    this.kept = settings;
    this.saves += 1;
  }
}

class FakeHost implements HostServices {
  readonly autosave = new FakeAutosave();
  readonly files = new FakeFiles();
  readonly settings = new FakeSettings();
  themes: Theme[] = [];
  confirmAnswer = true;
  confirmations: string[] = [];
  reloads = 0;

  applyTheme(theme: Theme): void {
    this.themes.push(theme);
  }

  reload(): void {
    this.reloads += 1;
  }

  confirm(message: string): boolean {
    this.confirmations.push(message);
    return this.confirmAnswer;
  }
}

function harness() {
  const scheduled: (() => void)[] = [];
  const host = new FakeHost();
  const engine = new CadEngine();
  engine.resize(800, 600);
  /** Pending timers: the test fires them instead of the clock. */
  const timers: { callback: () => void; ms: number }[] = [];
  const app = new CadApp(engine, {
    schedule: (callback) => scheduled.push(callback),
    host,
    timer: (callback, ms) => {
      const timer = { callback, ms };
      timers.push(timer);
      return () => timers.splice(timers.indexOf(timer), 1);
    },
  });
  /** Runs queued frames after letting pending promises settle. */
  const tick = async () => {
    await new Promise((resolve) => setTimeout(resolve, 0));
    scheduled.splice(0).forEach((callback) => callback());
  };
  /** Fires every pending timer, as if the time had passed. */
  /** Fires every pending timer, and what those schedule, as if the time had passed. */
  const elapse = async () => {
    for (let round = 0; round < 100 && timers.length > 0; round += 1) {
      timers.splice(0).forEach((timer) => timer.callback());
      await tick();
    }
    await tick();
  };
  const decode = (record: AutosaveRecord | undefined) =>
    record ? new TextDecoder().decode(record.bytes) : undefined;
  const enter = (...lines: string[]) => lines.forEach((line) => app.submitText(line));
  const segmentCount = () => engine.readScene((frame) => frame.segments.length / 4);
  return { app, engine, host, tick, elapse, timers, scheduled, enter, segmentCount, decode };
}

// DESIGN §25, ADR-0019: the drawing is kept in the browser and offered back.
describe("autosave and recovery", () => {
  test("a pause after a change keeps the drawing once, as unexported", async () => {
    const { app, host, tick, elapse, timers, enter, decode } = harness();
    await tick();
    expect(timers).toHaveLength(0);
    enter("L", "0,0", "100,50", "");
    await tick();
    expect(timers).toHaveLength(1);
    expect(timers[0]?.ms).toBe(AUTOSAVE_DELAY_MS);
    expect(host.autosave.saves).toBe(0);

    // Another change before the pause ends restarts it: still one timer.
    enter("C", "10,10", "5");
    await tick();
    expect(timers).toHaveLength(1);
    await elapse();
    expect(host.autosave.saves).toBe(1);
    const record = host.autosave.record;
    expect(record?.exported).toBe(false);
    expect(record?.name).toBeUndefined();
    expect(decode(record)).toContain("CIRCLE");
    expect(app.ui.getSnapshot().unsavedChanges).toBe(true);

    // Nothing changed since: a redraw schedules nothing more.
    app.loop.invalidate(1);
    await tick();
    expect(timers).toHaveLength(0);
  });

  test("exporting keeps the drawing as exported, so a reload has nothing to recover", async () => {
    const { host, tick, elapse, enter, decode } = harness();
    enter("L", "0,0", "100,50", "", "DXFOUT");
    await tick();
    expect(host.files.saved).toHaveLength(1);
    expect(host.autosave.record?.exported).toBe(true);
    expect(decode(host.autosave.record)).toBe(host.files.saved[0]?.text);
    // The pending autosave was cancelled by the export.
    await elapse();
    expect(host.autosave.saves).toBe(1);
  });

  test("a kept drawing is offered on start and recovered as a normal open", async () => {
    const { app, host, tick, enter, segmentCount } = harness();
    host.autosave.record = {
      name: "plan.dxf",
      bytes: encode(cleanFile),
      savedAt: 1_700_000_000_000,
      exported: false,
    };
    await app.start();
    expect(app.ui.getSnapshot().recovery).toEqual({
      name: "plan.dxf",
      savedAt: 1_700_000_000_000,
    });
    expect(segmentCount()).toBe(0);

    app.recover();
    await tick();
    expect(app.ui.getSnapshot().recovery).toBeUndefined();
    expect(segmentCount()).toBe(1);
    expect(app.ui.getSnapshot().documentName).toBe("plan.dxf");
    // Still unexported: a reload would offer it again.
    expect(host.autosave.record?.exported).toBe(false);

    enter("L", "0,0", "1,1", "");
    await tick();
    expect(app.ui.getSnapshot().unsavedChanges).toBe(true);
  });

  test("a kept drawing can be declined, which frees the slot", async () => {
    const { app, host } = harness();
    host.autosave.record = {
      name: undefined,
      bytes: encode(cleanFile),
      savedAt: 1,
      exported: false,
    };
    await app.start();
    expect(app.ui.getSnapshot().recovery).toBeDefined();
    app.discardRecovery();
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(app.ui.getSnapshot().recovery).toBeUndefined();
    expect(host.autosave.record).toBeUndefined();
  });

  test("an exported record is not offered, and opening a file replaces the kept drawing", async () => {
    const { app, host, tick, enter } = harness();
    host.autosave.record = {
      name: "old.dxf",
      bytes: encode(cleanFile),
      savedAt: 1,
      exported: true,
    };
    await app.start();
    expect(app.ui.getSnapshot().recovery).toBeUndefined();

    host.files.fileToPick = { name: "plan.dxf", bytes: encode(cleanFile) };
    enter("OPEN");
    await tick();
    await tick();
    expect(host.autosave.record).toMatchObject({ name: "plan.dxf", exported: true });
  });
});

describe("CadApp", () => {
  test("typed input reaches the engine and the UI store follows at once", async () => {
    const { app, tick } = harness();
    app.submitText("L");
    // Before any frame: the next keystroke must already know the prompt.
    expect(app.ui.getSnapshot()).toMatchObject({ prompt: "Specify first point:", command: "LINE" });
    expect(app.ui.getSnapshot().log).toEqual(["Command: L"]);
    await tick();
    expect(app.ui.getSnapshot().log).toEqual(["Command: L"]);
  });

  test("draws only while a renderer is attached, and only when something is dirty", async () => {
    const { app, tick, scheduled } = harness();
    const render = vi.fn();
    const detach = app.setRenderer(render);
    await tick();
    expect(render).toHaveBeenCalledTimes(1);

    await tick();
    expect(scheduled).toHaveLength(0);
    expect(render).toHaveBeenCalledTimes(1);

    app.submitText("L");
    await tick();
    expect(render).toHaveBeenCalledTimes(2);

    detach();
    app.submitText("0,0");
    await tick();
    expect(render).toHaveBeenCalledTimes(2);
  });

  test("keys and history go through the engine", async () => {
    const { app, tick } = harness();
    app.submitText("L");
    app.key(KeyAction.Escape);
    await tick();
    expect(app.ui.getSnapshot().prompt).toBe("Command:");
    expect(app.historyPrevious()).toBe("L");
    expect(app.historyNext()).toBe("");
  });

  test("a queued pointer move is delivered before typed input", async () => {
    const { app, tick } = harness();
    app.loop.queuePointerMove({ x: 410, y: 280, modifiers: 0 });
    app.submitText("L");
    await tick();
    expect(app.ui.getSnapshot().cursor).toEqual([10, 20]);
  });
});

describe("export", () => {
  test("DXFOUT offers the drawing as a download and clears the unsaved flag", async () => {
    const { app, host, tick, enter } = harness();
    enter("L", "0,0", "100,50", "");
    await tick();
    expect(app.ui.getSnapshot().unsavedChanges).toBe(true);

    enter("DXFOUT");
    await tick();

    expect(host.files.saved).toHaveLength(1);
    const [file] = host.files.saved;
    expect(file).toMatchObject({ name: "drawing.dxf", mimeType: "application/dxf" });
    expect(file?.text).toContain("$ACADVER\n  1\nAC1015\n");
    expect(file?.text).toContain("AcDbLine\n 10\n0.0\n 20\n0.0\n 30\n0.0\n 11\n100.0\n 21\n50.0\n");
    expect(file?.text.endsWith("  0\nEOF\n")).toBe(true);
    expect(app.ui.getSnapshot().unsavedChanges).toBe(false);
    expect(app.ui.getSnapshot().log.at(-1)).toBe("Drawing exported as DXF.");
    expect(host.confirmations).toEqual([]);
  });

  test("the save shortcut exports too", async () => {
    const { app, host, tick } = harness();
    app.key(KeyAction.Save);
    await tick();
    expect(host.files.saved).toHaveLength(1);
  });
});

// DESIGN §25.3: where the browser can write back to a file, SAVE does; elsewhere, a download.
describe("saving to a file", () => {
  test("with write-back: OPEN keeps the file, SAVE writes to it, SAVEAS asks and switches", async () => {
    const { app, host, tick, enter } = harness();
    host.files.writesBack = true;
    host.files.fileToPick = {
      name: "plan.dxf",
      bytes: encode(cleanFile),
      target: { name: "plan.dxf" },
    };
    enter("OPEN");
    await tick();
    enter("L", "0,0", "10,0", "");
    await tick();
    expect(app.ui.getSnapshot()).toMatchObject({ documentName: "plan.dxf", unsavedChanges: true });

    enter("SAVE");
    await tick();
    expect(host.files.written.map((write) => write.name)).toEqual(["plan.dxf"]);
    expect(host.files.saved).toHaveLength(0);
    expect(host.files.written[0]?.text).toContain("AcDbLine");
    expect(app.ui.getSnapshot().unsavedChanges).toBe(false);
    expect(host.autosave.record).toMatchObject({ name: "plan.dxf", exported: true });

    // SAVEAS asks where even with a file, and the new file is the one from then on.
    enter("C", "5,5", "1", "SAVEAS");
    await tick();
    expect(host.files.saved.map((save) => save.name)).toEqual(["plan.dxf"]);
    host.files.saved = [];
    enter("L", "0,1", "10,1", "", "SAVE");
    await tick();
    expect(host.files.written).toHaveLength(2);
    expect(host.files.saved).toHaveLength(0);
  });

  test("with write-back but no file yet, SAVE asks once and then writes back", async () => {
    const { app, host, tick, enter } = harness();
    host.files.writesBack = true;
    enter("L", "0,0", "10,0", "", "SAVE");
    await tick();
    expect(host.files.saved.map((save) => save.name)).toEqual(["drawing.dxf"]);
    expect(app.ui.getSnapshot().documentName).toBe("drawing.dxf");
    enter("L", "0,1", "10,1", "", "SAVE");
    await tick();
    expect(host.files.saved).toHaveLength(1);
    expect(host.files.written.map((write) => write.name)).toEqual(["drawing.dxf"]);
  });

  test("a write the browser refuses falls back to asking where", async () => {
    const { host, tick, enter } = harness();
    host.files.writesBack = true;
    enter("L", "0,0", "10,0", "", "SAVE");
    await tick();
    host.files.writeAnswer = false;
    enter("C", "5,5", "1", "SAVE");
    await tick();
    expect(host.files.written).toHaveLength(0);
    expect(host.files.saved).toHaveLength(2);
  });

  test("without write-back, SAVE and SAVEAS both download under the drawing's name", async () => {
    const { app, host, tick, enter } = harness();
    host.files.fileToPick = { name: "plan.dxf", bytes: encode(cleanFile) };
    enter("OPEN");
    await tick();
    enter("L", "0,0", "10,0", "", "SAVE");
    await tick();
    enter("SAVEAS");
    await tick();
    expect(host.files.saved.map((save) => save.name)).toEqual(["plan.dxf", "plan.dxf"]);
    expect(host.files.written).toHaveLength(0);
    expect(app.ui.getSnapshot().unsavedChanges).toBe(false);
  });
});

describe("open", () => {
  test("OPEN replaces the drawing with the picked file", async () => {
    const { app, host, tick, enter, segmentCount } = harness();
    host.files.fileToPick = { name: "plan.dxf", bytes: encode(cleanFile) };

    enter("OPEN");
    await tick();

    expect(segmentCount()).toBe(1);
    expect(app.ui.getSnapshot()).toMatchObject({
      documentName: "plan.dxf",
      unsavedChanges: false,
      exportLosses: [],
    });
    expect(app.ui.getSnapshot().log.at(-1)).toBe("Imported 1 line.");
  });

  test("an export after opening keeps the file's name", async () => {
    const { host, tick, enter } = harness();
    host.files.fileToPick = { name: "plan.dxf", bytes: encode(cleanFile) };
    enter("OPEN");
    await tick();
    enter("SAVE");
    await tick();
    expect(host.files.saved[0]?.name).toBe("plan.dxf");
  });

  test("cancelling the picker changes nothing", async () => {
    const { app, host, tick, enter, segmentCount } = harness();
    enter("L", "0,0", "10,0", "");
    host.files.fileToPick = undefined;
    enter("OPEN");
    await tick();
    expect(segmentCount()).toBe(1);
    expect(app.ui.getSnapshot().documentName).toBeUndefined();
  });

  test("asks before discarding unexported changes", async () => {
    const { host, tick, enter, segmentCount } = harness();
    enter("L", "0,0", "10,0", "", "L", "0,5", "10,5", "");
    host.files.fileToPick = { name: "plan.dxf", bytes: encode(cleanFile) };

    host.confirmAnswer = false;
    enter("OPEN");
    await tick();
    expect(host.confirmations).toEqual([DISCARD_CHANGES_MESSAGE]);
    expect(host.files.picks).toBe(0);
    expect(segmentCount()).toBe(2);

    host.confirmAnswer = true;
    enter("OPEN");
    await tick();
    expect(segmentCount()).toBe(1);
  });

  test("a file that cannot be read keeps the drawing and explains why", async () => {
    const { app, host, tick, enter, segmentCount } = harness();
    enter("L", "0,0", "10,0", "");
    enter("DXFOUT"); // so opening does not ask about unsaved changes
    host.files.fileToPick = { name: "broken.dxf", bytes: encode("not a dxf file\n") };

    enter("OPEN");
    await tick();

    expect(segmentCount()).toBe(1);
    expect(app.ui.getSnapshot().log.at(-1)).toBe(
      "Cannot open broken.dxf: line 1: expected a DXF group code.",
    );
    // The drawing is still the one saved as drawing.dxf, not the broken file.
    expect(app.ui.getSnapshot().documentName).toBe("drawing.dxf");
  });
});

// DESIGN §24.2: warn before a potentially lossy export.
describe("unsupported content", () => {
  test("is reported on import and confirmed before export", async () => {
    const { app, host, tick, enter } = harness();
    host.files.fileToPick = { name: "mixed.dxf", bytes: encode(mixedFile) };
    enter("OPEN");
    await tick();

    const losses = ["Not imported (unsupported): 1 TRACE."];
    expect(app.ui.getSnapshot().exportLosses).toEqual(losses);
    expect(app.ui.getSnapshot().importReport).toEqual({
      serial: 1,
      lines: ["Imported 1 line.", ...losses],
    });
    expect(app.ui.getSnapshot().log.slice(-2)).toEqual(["Imported 1 line.", ...losses]);

    // The warning names the losses; cancelling exports nothing.
    enter("DXFOUT");
    await tick();
    expect(app.ui.getSnapshot().exportWarning).toEqual(losses);
    expect(host.files.saved).toHaveLength(0);
    app.cancelExport();
    expect(app.ui.getSnapshot().exportWarning).toBeUndefined();
    expect(host.files.saved).toHaveLength(0);
    expect(host.confirmations).toEqual([]);

    // Asked again and told to go ahead: the file, without what was lost.
    enter("DXFOUT");
    await tick();
    app.exportAnyway();
    await tick();
    expect(app.ui.getSnapshot().exportWarning).toBeUndefined();
    expect(host.files.saved).toHaveLength(1);
    expect(host.files.saved[0]?.text).not.toContain("HATCH");
    // Exporting does not bring the lost content back.
    expect(app.ui.getSnapshot().exportLosses).toEqual(losses);

    // The report is dismissed from the screen; the log keeps its lines.
    app.dismissImportReport();
    await tick();
    expect(app.ui.getSnapshot().importReport).toBeUndefined();
    expect(app.ui.getSnapshot().log).toEqual(
      expect.arrayContaining(["Imported 1 line.", ...losses]),
    );
  });
});

// #140: the Layers panel acts through the command line and reads the UI state.
describe("layers for the panel", () => {
  test("the UI state lists the layers, and a panel action is LAYER with its answers", async () => {
    const { app, tick, enter } = harness();
    await tick();
    expect(app.ui.getSnapshot().layers).toEqual([
      {
        name: "0",
        color: 7,
        on: true,
        locked: false,
        current: true,
        linetype: "Continuous",
        lineweight: "Default",
      },
    ]);

    app.run("LAYER", "New", "walls", "");
    app.run("LAYER", "Set", "walls", "");
    app.run("LAYER", "Lock", "walls", "");
    await tick();
    const snapshot = app.ui.getSnapshot();
    expect(snapshot.layers.map((layer) => [layer.name, layer.current, layer.locked])).toEqual([
      ["0", false, false],
      ["walls", true, true],
    ]);
    // Every step is in the log, as if typed.
    expect(snapshot.log).toContain(
      "Press Enter to finish or [?/New/Set/Color/LWeight/LType/ON/OFF/Lock/Unlock/Rename/Delete]: New",
    );
    expect(snapshot.log).toContain("Enter name(s) for new layer(s): walls");
    expect(snapshot.log).toContain("1 layer created.");
    expect(snapshot.log).toContain('Current layer: "walls"');
    expect(snapshot.log).toContain("1 layer locked.");
    expect(snapshot.command).toBeUndefined();

    // A refusal is explained in the log; nothing else happens.
    app.run("LAYER", "Delete", "walls", "");
    await tick();
    expect(app.ui.getSnapshot().log.at(-2)).toBe(
      'Layer "walls" is the current layer: it cannot be switched off or deleted.',
    );
    expect(app.ui.getSnapshot().layers).toHaveLength(2);

    // Undo takes the whole panel action back at once.
    enter("U");
    await tick();
    expect(app.ui.getSnapshot().layers[1]?.locked).toBe(false);
  });
});

// #141: the Properties panel reads a bounded summary and changes through CHPROP.
describe("properties for the panel", () => {
  test("the UI state summarises the selection; a panel change is CHPROP with its answers", async () => {
    const { app, engine, tick, enter } = harness();
    enter("L", "0,0", "30,40", "", "LA", "N", "walls", "");
    await tick();
    expect(app.ui.getSnapshot().selection).toMatchObject({ count: 0, headline: "No selection" });
    expect(app.ui.getSnapshot().linetypes).toEqual(["Continuous"]);

    // A click on the line at the idle prompt selects it. The view is 800×600
    // with the origin at its centre and y up.
    app.loop.invalidate(engine.pointerDown(415, 280, 0, 0));
    app.loop.invalidate(engine.pointerUp(415, 280, 0, 0));
    await tick();
    const { selection } = app.ui.getSnapshot();
    expect(selection).toMatchObject({
      count: 1,
      headline: "1 line",
      kinds: [["line", 1]],
      layer: "0",
      color: "ByLayer",
      linetype: "ByLayer",
      lineweight: "ByLayer",
    });
    expect(selection.geometry).toEqual([
      ["Start", "0.0000, 0.0000"],
      ["End", "30.0000, 40.0000"],
      ["Length", "50.0000"],
      ["Angle", "53.1301°"],
    ]);

    app.run("CHPROP", "LAyer", "walls", "");
    app.run("CHPROP", "Color", "red", "");
    await tick();
    const changed = app.ui.getSnapshot();
    expect(changed.selection).toMatchObject({ layer: "walls", color: "red" });
    expect(changed.log).toContain('1 object changed to layer "walls".');
    expect(changed.log).toContain("1 object changed to red.");
    expect(changed.command).toBeUndefined();

    app.key(KeyAction.Escape);
    await tick();
    expect(app.ui.getSnapshot().selection.count).toBe(0);
  });
});

// #176: a panic in the engine leaves the drawing recoverable.
describe("a panic in the engine", () => {
  test("is told, keeps the autosave as it was, declines everything, and reloads on request", async () => {
    const { app, engine, host, tick, elapse, timers, scheduled, enter } = harness();
    await tick();
    enter("L", "0,0", "100,50", "");
    await tick();
    await elapse();
    expect(host.autosave.saves).toBe(1);
    const kept = host.autosave.record;
    // A change the pause has not kept yet, then the panic.
    enter("C", "10,10", "5");
    await tick();
    expect(timers).toHaveLength(1);
    expect(() => engine.panicForTest()).toThrow(EngineFailure);

    expect(app.failed).toBe(true);
    const failure = app.ui.getSnapshot().failure;
    expect(failure?.message).toContain("asked to fail");
    expect(failure?.keptAt).toBe(kept?.savedAt);
    expect(timers, "the pending autosave is cancelled").toHaveLength(0);

    // Nothing reaches the engine: no throw, no frame, no write.
    scheduled.length = 0;
    expect(() => enter("L", "0,0", "1,1", "")).not.toThrow();
    app.key(KeyAction.Escape);
    app.loop.invalidate(Dirty.Scene);
    app.loop.queuePointerMove({ x: 1, y: 1, modifiers: 0 });
    expect(app.historyPrevious()).toBeUndefined();
    expect(scheduled).toHaveLength(0);
    await elapse();
    expect(host.autosave.saves).toBe(1);
    expect(host.autosave.record).toBe(kept);

    app.reload();
    expect(host.reloads).toBe(1);
    expect(() => app.dispose()).not.toThrow();
  });
});

// #178, ADR-0024: the snapshot is written in slices, a turn of the event
// loop apart, and abandoned when the drawing changes under it.
describe("the autosave in slices", () => {
  test("a change while the snapshot is being written abandons it; the next pause keeps both", async () => {
    const { app, host, tick, timers, enter, decode } = harness();
    await tick();
    // Many entities: more than one slice.
    enter("L");
    for (let i = 0; i <= SNAPSHOT_SLICE + 1; i += 1) enter(`${i},0`);
    enter("");
    await tick();
    expect(timers).toHaveLength(1);
    // The pause ends: the first slice runs and schedules the next.
    timers.splice(0).forEach((timer) => timer.callback());
    expect(timers.map((timer) => timer.ms)).toEqual([0]);
    expect(host.autosave.saves).toBe(0);

    // A change now: the slice under way finds the drawing changed.
    enter("C", "10,10", "5");
    await tick();
    const slice = timers.findIndex((timer) => timer.ms === 0);
    timers.splice(slice, 1).forEach((timer) => timer.callback());
    await tick();
    expect(host.autosave.saves, "abandoned, not written").toBe(0);
    // The change scheduled its own pause; when it ends, the whole drawing is kept.
    expect(timers.map((timer) => timer.ms)).toEqual([AUTOSAVE_DELAY_MS]);
    for (let round = 0; round < 100 && timers.length > 0; round += 1) {
      timers.splice(0).forEach((timer) => timer.callback());
      await tick();
    }
    expect(host.autosave.saves).toBe(1);
    const kept = decode(host.autosave.record) ?? "";
    expect(kept).toContain("CIRCLE");
    expect(kept.split("\nAcDbLine\n").length - 1).toBe(SNAPSHOT_SLICE + 1);
    expect(app.ui.getSnapshot().unsavedChanges).toBe(true);
  });
});

// #186: what the browser kept is given back on start, and kept again when it changes.
describe("settings", () => {
  test("the toggles, the unit, and the aliases come back as last left, and the theme is applied", async () => {
    const { app, host, tick } = harness();
    host.settings.kept = {
      toggles: ToggleBit.Ortho | ToggleBit.Lineweight,
      unit: "in",
      aliases: "LL, LAYER\n",
      autosaveDelayMs: 2500,
      theme: "light",
    };
    await app.start();
    await tick();
    const ui = app.ui.getSnapshot();
    expect(ui.toggles).toBe(ToggleBit.Ortho | ToggleBit.Lineweight);
    expect(ui.unit).toBe("in");
    expect(ui.userAliases).toBe("LL, LAYER\n");
    expect(host.themes).toEqual(["light"]);
    expect(app.settings.getSnapshot()).toMatchObject({ theme: "light", autosaveDelayMs: 2500 });
    // Nothing changed: nothing was written back.
    expect(host.settings.saves).toBe(0);

    // The alias is a command; the delay is the kept one.
    app.submitText("LL");
    await tick();
    expect(app.ui.getSnapshot().command).toBe("LAYER");
    app.key(KeyAction.Escape);
    app.submitText("L");
    app.submitText("0,0");
    app.submitText("10,0");
    app.submitText("");
    await tick();
    expect(host.autosave.saves).toBe(0);
  });

  test("a toggle flipped, an alias defined, and a unit set are kept, once each", async () => {
    const { app, host, tick } = harness();
    await app.start();
    await tick();
    app.key(KeyAction.ToggleGrid);
    await tick();
    expect(host.settings.saves).toBe(1);
    expect(host.settings.kept.toggles).toBe(ToggleBit.Osnap);
    expect(host.settings.kept.theme).toBe("dark");

    app.submitText("ALIASEDIT");
    app.submitText("CC, CIRCLE");
    await tick();
    expect(host.settings.kept.aliases).toBe("CC, CIRCLE\n");
    app.submitText("UNITS");
    app.submitText("MM");
    await tick();
    expect(host.settings.kept.unit).toBe("mm");
    expect(host.settings.saves).toBe(3);

    // Pointer movement and drawing change no setting: nothing more is written.
    app.submitText("L");
    app.submitText("0,0");
    app.submitText("10,0");
    app.submitText("");
    await tick();
    expect(host.settings.saves).toBe(3);
  });

  test("SETTINGS opens the panel; the theme and the delay set there are applied and kept", async () => {
    const { app, host, tick, timers, enter } = harness();
    await app.start();
    expect(app.settings.getSnapshot().open).toBe(false);
    app.submitText("SETTINGS");
    await tick();
    expect(app.settings.getSnapshot().open).toBe(true);
    app.closeSettings();
    expect(app.settings.getSnapshot().open).toBe(false);

    app.setTheme("light");
    expect(host.themes).toEqual(["dark", "light"]);
    expect(host.settings.kept.theme).toBe("light");
    app.setAutosaveDelay(3000);
    expect(host.settings.kept.autosaveDelayMs).toBe(3000);
    app.setAutosaveDelay(5);
    expect(app.settings.getSnapshot().autosaveDelayMs).toBe(100);

    enter("L", "0,0", "100,50", "");
    await tick();
    expect(timers[0]?.ms).toBe(100);
  });

  test("settings a browser cannot read are nothing kept", () => {
    expect(parseSettings(null)).toEqual({});
    expect(parseSettings("not json")).toEqual({});
    expect(parseSettings("[1]")).toEqual({});
    expect(parseSettings('{"toggles":"x","unit":3,"theme":"blue","autosaveDelayMs":-1}')).toEqual(
      {},
    );
    expect(
      parseSettings(
        '{"toggles":5,"unit":"mm","aliases":"A, LINE","theme":"light","autosaveDelayMs":0}',
      ),
    ).toEqual({
      toggles: 5,
      unit: "mm",
      aliases: "A, LINE",
      theme: "light",
      autosaveDelayMs: 0,
    });
  });
});

// #187: PLOT makes a PDF of model space and offers it under the drawing's name.
describe("plot", () => {
  test("PLOT saves a PDF named after the drawing", async () => {
    const { app, host, tick, enter } = harness();
    enter("L", "0,0", "100,50", "", "PLOT", "A3", "E", "1:2", "Y", "M");
    await tick();
    expect(host.files.saved).toHaveLength(1);
    const [saved] = host.files.saved;
    expect(saved?.name).toBe("drawing.pdf");
    expect(saved?.mimeType).toBe("application/pdf");
    expect(saved?.text.startsWith("%PDF-1.4\n")).toBe(true);
    expect(saved?.text).toContain("/MediaBox [0 0 1190.551 841.89]");
    expect(saved?.text.endsWith("%%EOF\n")).toBe(true);
    expect(app.ui.getSnapshot().log.at(-1)).toBe(
      "Plotting the extents of model space in black on A3, at 1:2.",
    );
    // Plotting is not saving: the drawing is still unexported.
    expect(app.ui.getSnapshot().unsavedChanges).toBe(true);
  });

  // #194: a layout plots under the drawing's name and its own.
  test("PLOT on a layout saves the sheet under the layout's name", async () => {
    const { host, tick, enter } = harness();
    enter("LAYOUT", "N", "Sheet one", "L", "10,10", "200,10", "", "PLOT", "", "", "", "", "");
    await tick();
    expect(host.files.saved).toHaveLength(1);
    const [saved] = host.files.saved;
    expect(saved?.name).toBe("drawing-Sheet one.pdf");
    // An A3 landscape sheet, the title line at one to one.
    expect(saved?.text).toContain("/MediaBox [0 0 1190.551 841.89]");
    expect(saved?.text).toContain("\n28.346 28.346 m\n566.929 28.346 l\n");
  });
});

import { beforeAll, describe, expect, test, vi } from "vitest";
import { CadEngine, KeyAction } from "../cad/engine";
import { loadEngineForTests } from "../cad/test-support";
import { CadApp, DISCARD_CHANGES_MESSAGE, LOSSY_EXPORT_MESSAGE } from "./cad-app";
import type { HostServices, PickedFile } from "./host-services";

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
  [0, "TEXT"],
  [1, "hello"],
  [0, "ENDSEC"],
  [0, "EOF"],
);

class FakeHost implements HostServices {
  fileToPick: PickedFile | undefined;
  confirmAnswer = true;
  confirmations: string[] = [];
  picks = 0;
  saved: { name: string; text: string; mimeType: string }[] = [];

  pickFile(): Promise<PickedFile | undefined> {
    this.picks += 1;
    return Promise.resolve(this.fileToPick);
  }

  saveFile(name: string, text: string, mimeType: string): void {
    this.saved.push({ name, text, mimeType });
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
  const app = new CadApp(engine, { schedule: (callback) => scheduled.push(callback), host });
  /** Runs queued frames after letting pending promises settle. */
  const tick = async () => {
    await new Promise((resolve) => setTimeout(resolve, 0));
    scheduled.splice(0).forEach((callback) => callback());
  };
  const enter = (...lines: string[]) => lines.forEach((line) => app.submitText(line));
  const segmentCount = () => engine.readScene((frame) => frame.segments.length / 4);
  return { app, engine, host, tick, scheduled, enter, segmentCount };
}

describe("CadApp", () => {
  test("typed input reaches the engine and the UI store follows on the next frame", async () => {
    const { app, tick } = harness();
    app.submitText("L");
    expect(app.ui.getSnapshot().prompt).toBe("Command:");
    await tick();
    expect(app.ui.getSnapshot()).toMatchObject({ prompt: "Specify first point:", command: "LINE" });
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

    expect(host.saved).toHaveLength(1);
    const [file] = host.saved;
    expect(file).toMatchObject({ name: "drawing.dxf", mimeType: "application/dxf" });
    expect(file?.text).toContain("LINE\n  8\n0\n 10\n0.0\n 20\n0.0\n 11\n100.0\n 21\n50.0\n");
    expect(file?.text.endsWith("  0\nEOF\n")).toBe(true);
    expect(app.ui.getSnapshot().unsavedChanges).toBe(false);
    expect(app.ui.getSnapshot().log.at(-1)).toBe("Drawing exported as DXF.");
    expect(host.confirmations).toEqual([]);
  });

  test("the save shortcut exports too", async () => {
    const { app, host, tick } = harness();
    app.key(KeyAction.Save);
    await tick();
    expect(host.saved).toHaveLength(1);
  });
});

describe("open", () => {
  test("OPEN replaces the drawing with the picked file", async () => {
    const { app, host, tick, enter, segmentCount } = harness();
    host.fileToPick = { name: "plan.dxf", bytes: encode(cleanFile) };

    enter("OPEN");
    await tick();

    expect(segmentCount()).toBe(1);
    expect(app.ui.getSnapshot()).toMatchObject({
      documentName: "plan.dxf",
      unsavedChanges: false,
      exportIsLossy: false,
    });
    expect(app.ui.getSnapshot().log.at(-1)).toBe("Imported 1 line.");
  });

  test("an export after opening keeps the file's name", async () => {
    const { host, tick, enter } = harness();
    host.fileToPick = { name: "plan.dxf", bytes: encode(cleanFile) };
    enter("OPEN");
    await tick();
    enter("SAVE");
    await tick();
    expect(host.saved[0]?.name).toBe("plan.dxf");
  });

  test("cancelling the picker changes nothing", async () => {
    const { app, host, tick, enter, segmentCount } = harness();
    enter("L", "0,0", "10,0", "");
    host.fileToPick = undefined;
    enter("OPEN");
    await tick();
    expect(segmentCount()).toBe(1);
    expect(app.ui.getSnapshot().documentName).toBeUndefined();
  });

  test("asks before discarding unexported changes", async () => {
    const { host, tick, enter, segmentCount } = harness();
    enter("L", "0,0", "10,0", "", "L", "0,5", "10,5", "");
    host.fileToPick = { name: "plan.dxf", bytes: encode(cleanFile) };

    host.confirmAnswer = false;
    enter("OPEN");
    await tick();
    expect(host.confirmations).toEqual([DISCARD_CHANGES_MESSAGE]);
    expect(host.picks).toBe(0);
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
    host.fileToPick = { name: "broken.dxf", bytes: encode("not a dxf file\n") };

    enter("OPEN");
    await tick();

    expect(segmentCount()).toBe(1);
    expect(app.ui.getSnapshot().log.at(-1)).toBe(
      "Cannot open broken.dxf: line 1: expected a DXF group code.",
    );
    expect(app.ui.getSnapshot().documentName).toBeUndefined();
  });
});

// DESIGN §24.2: warn before a potentially lossy export.
describe("unsupported content", () => {
  test("is reported on import and confirmed before export", async () => {
    const { app, host, tick, enter } = harness();
    host.fileToPick = { name: "mixed.dxf", bytes: encode(mixedFile) };
    enter("OPEN");
    await tick();

    expect(app.ui.getSnapshot().exportIsLossy).toBe(true);
    expect(app.ui.getSnapshot().log.slice(-2)).toEqual([
      "Imported 1 line.",
      "Not imported (unsupported): 1 TEXT.",
    ]);

    host.confirmAnswer = false;
    enter("DXFOUT");
    await tick();
    expect(host.confirmations).toEqual([LOSSY_EXPORT_MESSAGE]);
    expect(host.saved).toHaveLength(0);

    host.confirmAnswer = true;
    enter("DXFOUT");
    await tick();
    expect(host.saved).toHaveLength(1);
    expect(host.saved[0]?.text).not.toContain("TEXT");
  });
});

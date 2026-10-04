import { beforeAll, describe, expect, test } from "vitest";
import {
  CadEngine,
  coreVersion,
  Dirty,
  KeyAction,
  StyleRole,
  wasmMemory,
  type FrameBuffers,
} from "./engine";
import { loadEngineForTests } from "./test-support";

beforeAll(loadEngineForTests);

function engine(): CadEngine {
  const engine = new CadEngine();
  engine.resize(800, 600);
  return engine;
}

function enter(cad: CadEngine, ...lines: string[]): number {
  return lines.reduce((dirty, line) => dirty | cad.submitText(line), 0);
}

const copy = (frame: FrameBuffers) => ({
  segments: Array.from(frame.segments),
  batches: Array.from(frame.batches),
  arcs: Array.from(frame.arcs),
  arcBatches: Array.from(frame.arcBatches),
});
const copyScene = (cad: CadEngine) => cad.readScene(copy);
const copyOverlay = (cad: CadEngine) => cad.readOverlay(copy);
const EMPTY = { segments: [], batches: [], arcs: [], arcBatches: [] };

describe("view", () => {
  test("reports the core version", () => {
    expect(coreVersion()).toMatch(/^\d+\.\d+\.\d+$/);
  });

  test("resize flags the view, not the scene", () => {
    const dirty = new CadEngine().resize(800, 600);
    expect(dirty & Dirty.View).toBeTruthy();
    expect(dirty & Dirty.Scene).toBe(0);
  });

  test("the clip transform maps the canvas to clip space", () => {
    expect(Array.from(engine().sceneTransform())).toEqual([
      Math.fround(2 / 800),
      Math.fround(2 / 600),
      0,
      0,
    ]);
  });

  test("zooming changes the transform but not the scene revision", () => {
    const cad = engine();
    const revision = cad.sceneRevision;
    const before = Array.from(cad.sceneTransform());

    const dirty = cad.wheel(400, 300, -100, 0);

    expect(dirty & Dirty.View).toBeTruthy();
    expect(dirty & Dirty.Scene).toBe(0);
    expect(cad.sceneRevision).toBe(revision);
    expect(cad.sceneTransform()[0]).toBeGreaterThan(before[0] ?? Infinity);
  });

  test("middle-button drag pans; an ordinary move does not", () => {
    const cad = engine();
    // An ordinary move only moves the crosshair and the coordinate readout.
    expect(cad.pointerMove(10, 10, 0)).toBe(Dirty.Overlay | Dirty.Ui);

    cad.pointerDown(100, 100, 1, 0);
    const dirty = cad.pointerMove(140, 100, 0);
    cad.pointerUp(140, 100, 1, 0);

    expect(dirty & Dirty.View).toBeTruthy();
    // 40px right on an 800px canvas is 0.1 in clip space.
    expect(cad.sceneTransform()[2]).toBeCloseTo(0.1, 6);
  });

  test("unknown pointer buttons and key codes are ignored", () => {
    const cad = engine();
    expect(cad.pointerDown(0, 0, 7, 0)).toBe(0);
    expect(cad.key(99 as KeyAction)).toBe(0);
  });
});

describe("commands", () => {
  test("a new drawing has an empty scene", () => {
    expect(copyScene(engine())).toEqual(EMPTY);
  });

  test("typed LINE input commits a segment and bumps the scene revision", () => {
    const cad = engine();
    const revision = cad.sceneRevision;

    expect(enter(cad, "L", "0,0") & Dirty.Scene).toBe(0);
    expect(cad.sceneRevision).toBe(revision);

    expect(enter(cad, "100,0", "") & Dirty.Scene).toBeTruthy();
    expect(cad.sceneRevision).not.toBe(revision);

    // One segment, stored relative to the scene origin (the middle of the line).
    expect(copyScene(cad)).toEqual({
      ...EMPTY,
      segments: [-50, 0, 50, 0],
      batches: [StyleRole.Entity, 0, 1],
    });
  });

  test("ui state follows the command", () => {
    const cad = engine();
    expect(cad.uiState()).toMatchObject({
      prompt: "Command:",
      selectionCount: 0,
      spaceAccepts: true,
    });
    expect(cad.uiState().command).toBeUndefined();
    expect(cad.uiState().cursor).toBeUndefined();

    enter(cad, "L");
    cad.pointerMove(410, 280, 0);
    expect(cad.uiState()).toMatchObject({
      prompt: "Specify first point:",
      command: "LINE",
      cursor: [10, 20],
    });
  });

  test("log lines are fetched incrementally", () => {
    const cad = engine();
    enter(cad, "L");
    const seen = cad.uiState().logTotal;
    enter(cad, "banana");
    expect(cad.logSince(seen)).toEqual(["Specify first point: banana", "A point is required."]);
    expect(cad.logSince(0)).toHaveLength(3);
  });

  test("escape cancels and the undo/redo keys walk the history", () => {
    const cad = engine();
    enter(cad, "L", "0,0", "10,0");
    expect(cad.key(KeyAction.Escape) & Dirty.Ui).toBeTruthy();
    expect(cad.uiState().prompt).toBe("Command:");

    expect(cad.key(KeyAction.Undo) & Dirty.Scene).toBeTruthy();
    expect(copyScene(cad).segments).toHaveLength(0);
    expect(cad.uiState()).toMatchObject({ canUndo: false, canRedo: true });

    expect(cad.key(KeyAction.Redo) & Dirty.Scene).toBeTruthy();
    expect(copyScene(cad).segments).toHaveLength(4);
  });

  test("history recall returns submitted lines", () => {
    const cad = engine();
    enter(cad, "L", "0,0");
    expect(cad.historyPrevious()).toBe("0,0");
    expect(cad.historyPrevious()).toBe("L");
    expect(cad.historyPrevious()).toBeUndefined();
    expect(cad.historyNext()).toBe("0,0");
    expect(cad.historyNext()).toBe("");
    expect(cad.historyNext()).toBeUndefined();
  });
});

describe("curves", () => {
  test("a circle crosses the boundary as one arc instance with a full sweep", () => {
    const cad = engine();
    enter(cad, "C", "100,50", "25");
    const scene = copyScene(cad);
    // Relative to the scene origin, which is the circle's centre here.
    expect(scene.arcs).toEqual([0, 0, 25, 0, Math.fround(2 * Math.PI)]);
    expect(scene.arcBatches).toEqual([StyleRole.Entity, 0, 1]);
    expect(scene.segments).toEqual([]);
  });

  test("a three-point arc is stored counter-clockwise", () => {
    const cad = engine();
    enter(cad, "A", "10,0", "0,10", "-10,0");
    const [, , radius, start, sweep] = copyScene(cad).arcs;
    expect(radius).toBeCloseTo(10, 5);
    expect(start).toBeCloseTo(0, 5);
    expect(sweep).toBeCloseTo(Math.PI, 5);
  });

  test("the circle preview is transient: overlay only, no scene change", () => {
    const cad = engine();
    enter(cad, "C", "0,0");
    const revision = cad.sceneRevision;
    cad.pointerMove(430, 260, 0); // world (30, 40): radius 50

    const overlay = copyOverlay(cad);
    expect(overlay.arcBatches).toEqual([StyleRole.Preview, 0, 1]);
    expect(overlay.arcs[2]).toBeCloseTo(50, 4);
    expect(cad.sceneRevision).toBe(revision);
    expect(copyScene(cad)).toEqual(EMPTY);
  });

  test("C is CIRCLE at the idle prompt and Close inside LINE", () => {
    const cad = engine();
    enter(cad, "L", "0,0", "10,0", "10,10", "C");
    expect(cad.uiState().prompt).toBe("Command:");
    expect(copyScene(cad).segments).toHaveLength(12);

    enter(cad, "C");
    expect(cad.uiState().command).toBe("CIRCLE");
  });
});

describe("overlay", () => {
  test("is empty until the pointer is over the canvas", () => {
    expect(copyOverlay(engine())).toEqual(EMPTY);
  });

  test("shows a crosshair, and a rubber band during LINE", () => {
    const cad = engine();
    cad.pointerMove(400, 300, 0);
    expect(copyOverlay(cad).batches).toEqual([StyleRole.Crosshair, 0, 6]);

    enter(cad, "L", "0,0");
    cad.pointerMove(450, 300, 0);
    const overlay = copyOverlay(cad);
    expect(overlay.batches).toEqual([StyleRole.Preview, 0, 1, StyleRole.Crosshair, 1, 6]);
    expect(overlay.segments.slice(0, 4)).toEqual([0, 0, 50, 0]);

    expect(cad.pointerLeave() & Dirty.Overlay).toBeTruthy();
    expect(copyOverlay(cad)).toEqual(EMPTY);
  });

  // DESIGN §26.3 criteria 5 and 6.
  test("pointer movement changes the overlay and never the scene revision", () => {
    const cad = engine();
    enter(cad, "L", "0,0", "100,0");
    const revision = cad.sceneRevision;
    for (let step = 0; step < 200; step += 1) {
      const dirty = cad.pointerMove(step, step, 0);
      expect(dirty & Dirty.Overlay).toBeTruthy();
      expect(dirty & Dirty.Scene).toBe(0);
    }
    expect(cad.sceneRevision).toBe(revision);
  });
});

describe("views into WASM memory", () => {
  // ADR-0002 §4: views die when memory grows, so the wrapper re-creates them on every pull.
  test("pulls stay correct after memory grows, while a retained view is detached", () => {
    const cad = engine();
    enter(cad, "L", "0,0", "100,0", "100,50", "");
    const before = copyScene(cad);
    const retained = cad.readScene((frame) => frame.segments);
    expect(retained.length).toBe(8);

    wasmMemory().grow(4);

    expect(retained.length).toBe(0); // the hazard the wrapper protects against
    expect(copyScene(cad)).toEqual(before);
  });

  test("both arrays of one pull are alive at the same time", () => {
    const cad = engine();
    enter(cad, "L", "0,0", "100,0", "");
    cad.pointerMove(10, 10, 0);
    const sizes = (frame: FrameBuffers) => [frame.segments.length, frame.batches.length];
    expect(cad.readScene(sizes)).toEqual([4, 3]);
    expect(cad.readOverlay(sizes)).toEqual([24, 3]); // the crosshair
  });
});

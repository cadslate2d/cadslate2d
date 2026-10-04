import { beforeAll, describe, expect, test } from "vitest";
import { CadEngine, coreVersion, Dirty } from "./engine";
import { loadEngineForTests } from "./test-support";

beforeAll(loadEngineForTests);

function engine(): CadEngine {
  const engine = new CadEngine();
  engine.resize(800, 600);
  return engine;
}

describe("CadEngine", () => {
  test("reports the core version", () => {
    expect(coreVersion()).toMatch(/^\d+\.\d+\.\d+$/);
  });

  test("resize flags the view, not the scene", () => {
    const dirty = new CadEngine().resize(800, 600);
    expect(dirty & Dirty.View).toBeTruthy();
    expect(dirty & Dirty.Scene).toBe(0);
  });

  test("a new drawing has an empty scene", () => {
    const { segments, batches } = engine().readScene((frame) => ({
      segments: frame.segments.length,
      batches: frame.batches.length,
    }));
    expect(segments).toBe(0);
    expect(batches).toBe(0);
  });

  test("the clip transform maps the canvas to clip space", () => {
    const transform = engine().sceneTransform();
    expect(Array.from(transform)).toEqual([Math.fround(2 / 800), Math.fround(2 / 600), 0, 0]);
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
    expect(cad.pointerMove(10, 10, 0)).toBe(0);

    cad.pointerDown(100, 100, 1, 0);
    const dirty = cad.pointerMove(140, 100, 0);
    cad.pointerUp(140, 100, 1, 0);

    expect(dirty & Dirty.View).toBeTruthy();
    // 40px right on an 800px canvas is 0.1 in clip space.
    expect(cad.sceneTransform()[2]).toBeCloseTo(0.1, 6);
  });

  test("unknown pointer buttons are ignored", () => {
    expect(engine().pointerDown(0, 0, 7, 0)).toBe(0);
  });
});

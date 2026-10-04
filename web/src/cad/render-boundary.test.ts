// The TypeScript half of the render-boundary suite (DESIGN §41). The Rust
// half is crates/cad-render/tests/render_boundary.rs.
import { readFile } from "node:fs/promises";
import { beforeAll, describe, expect, test } from "vitest";
import { renderFrame, type FrameSink } from "../renderer/render-frame";
import { CadEngine } from "./engine";
import { loadEngineForTests } from "./test-support";

beforeAll(loadEngineForTests);

/** Public members of the exported `CadSession` class, from the generated typings. */
async function exportedSessionApi(): Promise<string[]> {
  const typings = await readFile(new URL("./pkg/cad_wasm.d.ts", import.meta.url), "utf8");
  const body = /export class CadSession \{([\s\S]*?)\n\}/.exec(typings)?.[1];
  if (!body) throw new Error("CadSession is missing from the generated typings");
  return [...body.matchAll(/^ {4}(?:readonly )?(\w+)(?=[(:])/gm)]
    .map((match) => match[1] ?? "")
    .filter((name) => !["free", "constructor"].includes(name) && !name.startsWith("__"))
    .sort();
}

describe("WASM API surface", () => {
  // ADR-0002 §7 forbids per-entity and per-vertex accessors. Every export is
  // listed here on purpose: adding one fails this test, so the addition gets
  // reviewed for coarseness before it ships.
  test("is exactly the reviewed set of coarse calls", async () => {
    expect(await exportedSessionApi()).toEqual([
      "export_dxf",
      "history_next",
      "history_previous",
      "key",
      "load_dxf",
      "log_since",
      "overlay_arc_batches",
      "overlay_arcs",
      "overlay_batches",
      "overlay_segments",
      "overlay_transform",
      "pointer_down",
      "pointer_leave",
      "pointer_move",
      "pointer_up",
      "resize",
      "scene_arc_batches",
      "scene_arcs",
      "scene_batches",
      "scene_revision",
      "scene_segments",
      "scene_transform",
      "submit_text",
      "take_host_request",
      "ui_state",
      "wheel",
    ]);
  });

  test("nothing in it names an entity, a vertex, or an index", async () => {
    for (const name of await exportedSessionApi()) {
      expect(name).not.toMatch(/entity|entities|vertex|vertices|get_|_at$|_count$/);
    }
  });
});

describe("calls across the boundary per frame", () => {
  const sink = (): FrameSink & { sceneRevision: number | undefined } => ({
    sceneRevision: undefined,
    updateScene(revision) {
      this.sceneRevision = revision;
    },
    updateOverlay() {},
    draw() {},
  });

  /** Counts every engine method `renderFrame` calls. */
  function counting(engine: CadEngine): { source: CadEngine; calls: () => number } {
    let calls = 0;
    const source = new Proxy(engine, {
      get(target, property) {
        const value = Reflect.get(target, property, target) as unknown;
        if (typeof value !== "function") {
          calls += 1; // a getter such as sceneRevision
          return value;
        }
        return (...args: unknown[]) => {
          calls += 1;
          return (value as (...args: unknown[]) => unknown).apply(target, args);
        };
      },
    });
    return { source, calls: () => calls };
  }

  function engineWithLines(count: number): CadEngine {
    const engine = new CadEngine();
    engine.resize(800, 600);
    engine.submitText("L");
    for (let index = 0; index <= count; index += 1) engine.submitText(`${index * 2},${index % 9}`);
    engine.submitText("");
    return engine;
  }

  test("do not grow with the number of entities", () => {
    const perFrame = [1, 100, 2000].map((lines) => {
      const { source, calls } = counting(engineWithLines(lines));
      const target = sink();
      renderFrame(source, target); // first frame: includes the one-off scene upload
      const afterFirst = calls();
      source.pointerMove(100, 100, 0);
      renderFrame(source, target); // steady state
      return { first: afterFirst, steady: calls() - afterFirst - 1 };
    });

    expect(perFrame[1]).toEqual(perFrame[0]);
    expect(perFrame[2]).toEqual(perFrame[0]);
    // Revision check, overlay read, and two transforms.
    expect(perFrame[0]?.steady).toBe(4);
  });
});

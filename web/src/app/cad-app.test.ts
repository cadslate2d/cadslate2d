import { beforeAll, describe, expect, test, vi } from "vitest";
import { CadEngine, KeyAction } from "../cad/engine";
import { loadEngineForTests } from "../cad/test-support";
import { CadApp } from "./cad-app";

beforeAll(loadEngineForTests);

function harness() {
  const scheduled: (() => void)[] = [];
  const engine = new CadEngine();
  engine.resize(800, 600);
  const app = new CadApp(engine, (callback) => scheduled.push(callback));
  const tick = () => scheduled.splice(0).forEach((callback) => callback());
  return { app, tick, scheduled };
}

describe("CadApp", () => {
  test("typed input reaches the engine and the UI store follows on the next frame", () => {
    const { app, tick } = harness();
    app.submitText("L");
    expect(app.ui.getSnapshot().prompt).toBe("Command:");
    tick();
    expect(app.ui.getSnapshot()).toMatchObject({ prompt: "Specify first point:", command: "LINE" });
    expect(app.ui.getSnapshot().log).toEqual(["Command: L"]);
  });

  test("draws only while a renderer is attached, and only when something is dirty", () => {
    const { app, tick, scheduled } = harness();
    const render = vi.fn();
    const detach = app.setRenderer(render);
    tick();
    expect(render).toHaveBeenCalledTimes(1);

    tick();
    expect(scheduled).toHaveLength(0);
    expect(render).toHaveBeenCalledTimes(1);

    app.submitText("L");
    tick();
    expect(render).toHaveBeenCalledTimes(2);

    detach();
    app.submitText("0,0");
    tick();
    expect(render).toHaveBeenCalledTimes(2);
  });

  test("keys and history go through the engine", () => {
    const { app, tick } = harness();
    app.submitText("L");
    app.key(KeyAction.Escape);
    tick();
    expect(app.ui.getSnapshot().prompt).toBe("Command:");
    expect(app.historyPrevious()).toBe("L");
    expect(app.historyNext()).toBe("");
  });

  test("a queued pointer move is delivered before typed input", () => {
    const { app, tick } = harness();
    app.loop.queuePointerMove({ x: 410, y: 280, modifiers: 0 });
    app.submitText("L");
    tick();
    expect(app.ui.getSnapshot().cursor).toEqual([10, 20]);
  });
});

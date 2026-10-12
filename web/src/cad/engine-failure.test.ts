// A panic in the engine (#176): the call it happens in reports it, nothing
// reaches the session afterwards, and disposing is safe. Its own file: the
// module instance it traps is not shared with the other engine tests.
import { beforeAll, describe, expect, test } from "vitest";
import { CadEngine, Dirty, EngineFailure } from "./engine";
import { loadEngineForTests } from "./test-support";

beforeAll(loadEngineForTests);

describe("a panic", () => {
  test("stops the engine: reported once, then every call is declined before it is made", () => {
    const engine = new CadEngine();
    engine.resize(800, 600);
    expect(engine.submitText("L") & Dirty.Ui).toBeTruthy();
    const failures: string[] = [];
    engine.onFailure = (message) => failures.push(message);
    expect(engine.failure).toBeUndefined();

    expect(() => engine.panicForTest()).toThrow(EngineFailure);
    expect(engine.failure).toContain("asked to fail, for the test");
    expect(failures).toEqual([engine.failure]);

    // A declined call throws the same failure, from the guard: a trap would
    // be a RuntimeError, and a successful call would answer.
    expect(() => engine.uiState()).toThrow(EngineFailure);
    expect(() => engine.submitText("0,0")).toThrow(engine.failure ?? "");
    expect(() => engine.readScene((frame) => frame.segments.length)).toThrow(EngineFailure);
    expect(failures).toHaveLength(1);
    expect(() => engine.dispose()).not.toThrow();
  });
});

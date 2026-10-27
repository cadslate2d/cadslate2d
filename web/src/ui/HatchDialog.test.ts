import { beforeAll, describe, expect, test } from "vitest";
import { CadEngine, hatchSwatches } from "../cad/engine";
import { loadEngineForTests } from "../cad/test-support";
import { hatchMacro } from "./HatchDialog";

beforeAll(loadEngineForTests);

/** Runs a macro as the command line types it. */
function type(engine: CadEngine, macro: ReturnType<typeof hatchMacro>) {
  engine.invoke(macro.command);
  for (const step of macro.steps ?? []) {
    if (typeof step === "string") engine.submitText(step);
  }
}

describe("the hatch dialog (#346)", () => {
  test("offers every pattern of the table with its swatch, SOLID first", () => {
    const swatches = hatchSwatches();
    expect(swatches[0]).toMatchObject({ name: "SOLID", solid: true, lines: [] });
    const names = swatches.map((s) => s.name);
    for (const name of ["ANSI31", "ANSI37", "BRICK", "EARTH", "STEEL"])
      expect(names).toContain(name);
    for (const swatch of swatches.slice(1)) {
      expect(swatch.lines.length, swatch.name).toBeGreaterThan(0);
      expect(swatch.lines.length % 4).toBe(0);
      for (const v of swatch.lines) expect(v >= -1e-6 && v <= 1 + 1e-6, swatch.name).toBe(true);
    }
  });

  test("its macro hatches with the pattern, angle, and scale, which HATCH then remembers", () => {
    const engine = new CadEngine();
    engine.resize(800, 600);
    for (const line of ["RECTANG", "0,0", "100,50"]) engine.submitText(line);
    type(engine, hatchMacro({ pattern: "ANSI31", angle: 45, scale: 2 }, false));
    expect(engine.uiState().prompt).toMatch(/^Pick internal point/);
    engine.submitText("50,25");
    expect(engine.uiState().command).toBeUndefined();
    expect(engine.logSince(0).some((line) => /error|no hatch pattern/i.test(line))).toBe(false);
    const state = engine.uiState();
    expect([state.hatchPattern, state.hatchAngle, state.hatchScale]).toEqual(["ANSI31", 45, 2]);
    // Select Objects ends at HATCH's own question for the boundary.
    type(engine, hatchMacro({ pattern: "EARTH", angle: 0, scale: 1 }, true));
    expect(engine.uiState().prompt).toMatch(/^Select boundary objects/);
    engine.dispose();
  });
});

import { describe, expect, test } from "vitest";
import { PAUSE, SELECTION, macroText, pauseState, selecting, type PausedMacro } from "./macro";

const paused = (kind: PausedMacro["kind"], prompt: string): PausedMacro => ({
  command: "ARC",
  prompt,
  kind,
  rest: ["C"],
});

describe("a paused macro (ADR-0034)", () => {
  test("waits while the command asks the same question, and resumes once it asks another", () => {
    const at = paused("answer", "Specify start point of arc:");
    expect(pauseState(at, { command: "ARC", prompt: "Specify start point of arc:" })).toBe("wait");
    expect(
      pauseState(at, { command: "ARC", prompt: "Specify second point of arc or [Center]:" }),
    ).toBe("resume");
  });

  test("is dropped when its command ends or another starts", () => {
    const at = paused("answer", "Specify start point of arc:");
    expect(pauseState(at, { command: undefined, prompt: "Command:" })).toBe("drop");
    expect(pauseState(at, { command: "LINE", prompt: "Specify first point:" })).toBe("drop");
  });

  test("at a selection, waits while objects are asked for", () => {
    const at = { ...paused("selection", "Select objects:"), command: "DRAWORDER" };
    expect(pauseState(at, { command: "DRAWORDER", prompt: "Select objects:" })).toBe("wait");
    expect(pauseState(at, { command: "DRAWORDER", prompt: "Remove objects:" })).toBe("wait");
    expect(
      pauseState(at, {
        command: "DRAWORDER",
        prompt: "Enter object ordering option or [Above objects/Under objects/Front/Back] <Back>:",
      }),
    ).toBe("resume");
    expect(selecting("Select objects:")).toBe(true);
    expect(selecting("Specify first point:")).toBe(false);
  });

  test("reads as AutoCAD writes it", () => {
    expect(macroText({ command: "ARC", steps: [PAUSE, "C"] })).toBe("ARC \\ C");
    expect(macroText({ command: "DRAWORDER", steps: [SELECTION, "F"] })).toBe("DRAWORDER \\ F");
    expect(macroText({ command: "LINE" })).toBe("LINE");
  });
});

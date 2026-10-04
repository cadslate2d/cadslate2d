import { describe, expect, test } from "vitest";
import { StyleRole } from "../cad/engine";
import { parseHexColor, readPalette } from "./palette";

describe("parseHexColor", () => {
  test("parses six-digit colours", () => {
    expect(parseHexColor("#ff8000")).toEqual([1, 128 / 255, 0, 1]);
  });

  test("parses three-digit shorthand", () => {
    expect(parseHexColor("#f80")).toEqual([1, 136 / 255, 0, 1]);
  });

  test("parses an alpha channel", () => {
    expect(parseHexColor("#00000080")).toEqual([0, 0, 0, 128 / 255]);
  });

  test("tolerates the whitespace computed styles include", () => {
    expect(parseHexColor("  #FFFFFF ")).toEqual([1, 1, 1, 1]);
  });

  test("rejects anything that is not a hex colour", () => {
    expect(() => parseHexColor("rgb(0 0 0)")).toThrow(/expected a hex colour/);
    expect(() => parseHexColor("")).toThrow();
  });
});

describe("readPalette", () => {
  const tokens: Record<string, string> = {
    "--canvas-background": "#000000",
    "--canvas-entity": "#ffffff",
    "--canvas-selection": "#0000ff",
    "--canvas-preview": "#808080",
    "--canvas-crosshair": "#ff0000",
    "--canvas-lineweight-entity": "1.25",
    "--canvas-lineweight-selection": " 2.5",
    "--canvas-lineweight-preview": "1",
    "--canvas-lineweight-crosshair": "1",
    "--canvas-selection-window": "#0000ff",
    "--canvas-selection-crossing": "#00ff00",
    "--canvas-lineweight-selection-window": "1",
    "--canvas-lineweight-selection-crossing": "1",
    "--canvas-snap-marker": "#ffaa00",
    "--canvas-lineweight-snap-marker": "1.5",
    "--canvas-tracking": "#00aaaa",
    "--canvas-lineweight-tracking": "1",
    "--canvas-grid-minor": "#111111",
    "--canvas-grid-major": "#222222",
    "--canvas-lineweight-grid-minor": "1",
    "--canvas-lineweight-grid-major": "1",
    "--canvas-grip": "#0000ee",
    "--canvas-grip-hot": "#ee0000",
    "--canvas-lineweight-grip": "1.5",
    "--canvas-lineweight-grip-hot": "2",
    "--canvas-hover": "#aaccff",
    "--canvas-removal": "#ff5555",
    "--canvas-lineweight-hover": "2.25",
    "--canvas-lineweight-removal": "2.25",
  };
  const read = (property: string) => tokens[property] ?? "";

  test("maps every style role to its themed stroke", () => {
    const palette = readPalette(read);
    expect(palette.background).toEqual([0, 0, 0, 1]);
    expect(palette.strokes[StyleRole.Entity]).toEqual({ color: [1, 1, 1, 1], width: 1.25 });
    expect(palette.strokes[StyleRole.Selection]).toEqual({ color: [0, 0, 1, 1], width: 2.5 });
    expect(palette.strokes[StyleRole.Crosshair].color).toEqual([1, 0, 0, 1]);
    expect(Object.keys(palette.strokes)).toHaveLength(Object.keys(StyleRole).length);
  });

  test("fails loudly when a token is missing", () => {
    expect(() => readPalette(() => "")).toThrow();
    expect(() =>
      readPalette((property) => (property === "--canvas-lineweight-entity" ? "0" : read(property))),
    ).toThrow(/lineweight-entity/);
  });
});

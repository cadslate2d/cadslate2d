import { describe, expect, test } from "vitest";
import {
  BYTES_PER_STYLE,
  PATTERN_CONTINUOUS,
  STYLE_FOREGROUND,
  STYLE_RGB,
  STYLE_WEIGHT_DEFAULT,
} from "../../cad/engine";
import {
  lineweightWidth,
  STYLE_TABLE_WIDTH,
  STYLES_PER_ROW,
  styleTexels,
  type StyleLook,
} from "./style-table";

const LOOK: StyleLook = {
  foreground: [0.5, 0.25, 1, 1],
  defaultWidth: 1.25,
  lineweights: true,
  devicePixelRatio: 1,
};

/** A style row: a colour, or the foreground, a weight, and a pattern. */
function row(
  colour: [number, number, number] | "foreground",
  weight = STYLE_WEIGHT_DEFAULT,
  pattern = PATTERN_CONTINUOUS,
) {
  const [red, green, blue] = colour === "foreground" ? [9, 9, 9] : colour;
  const kind = colour === "foreground" ? STYLE_FOREGROUND : STYLE_RGB;
  return [red, green, blue, kind, weight, pattern, 0, 0];
}

/** The colour texel, the half width in device pixels, and the pattern of style `index`. */
function style(rows: number[][], index: number, look: StyleLook = LOOK) {
  const texels = styleTexels(new Uint8Array(rows.flat()), look);
  const at = index * 8;
  return {
    colour: [...texels.data.slice(at, at + 4)],
    halfWidth: (texels.data[at + 4] ?? 0) / 16,
    pattern: texels.data[at + 5],
    texels,
  };
}

describe("styleTexels: colour", () => {
  test("a colour row becomes an opaque texel of that colour", () => {
    const rows = [row([255, 0, 0]), row([10, 20, 30])];
    expect(style(rows, 0).colour).toEqual([255, 0, 0, 255]);
    expect(style(rows, 1).colour).toEqual([10, 20, 30, 255]);
    expect(style(rows, 0).texels.width).toBe(STYLE_TABLE_WIDTH);
    expect(style(rows, 0).texels.height).toBe(1);
  });

  test("a foreground row takes the theme's colour, whatever its bytes say", () => {
    expect(style([row("foreground")], 0).colour).toEqual([128, 64, 255, 255]);
    // The same drawing on a light theme: the same row, a dark texel.
    const light = { ...LOOK, foreground: [0.1, 0.1, 0.1, 1] as const };
    expect(style([row("foreground")], 0, light).colour).toEqual([26, 26, 26, 255]);
  });

  test("an empty table is still one row of texels, so the sampler is complete", () => {
    const texels = styleTexels(new Uint8Array(0), LOOK);
    expect(texels.height).toBe(1);
    expect(texels.data.length).toBe(STYLE_TABLE_WIDTH * 4);
  });

  test("more styles than a row holds wrap onto further rows", () => {
    const count = STYLES_PER_ROW + 3;
    const rows = Array.from({ length: count }, (_, index) => row([index % 256, 1, 2]));
    const texels = styleTexels(new Uint8Array(rows.flat()), LOOK);
    expect(rows.flat().length).toBe(count * BYTES_PER_STYLE);
    expect(texels.height).toBe(2);
    expect(texels.data.length).toBe(STYLE_TABLE_WIDTH * 2 * 4);
    // Style 513 is the second style of the second row, as the shader reads it.
    expect(style(rows, STYLES_PER_ROW + 1).colour).toEqual([(STYLES_PER_ROW + 1) % 256, 1, 2, 255]);
  });
});

describe("styleTexels: width", () => {
  test("the default weight is the theme's ordinary width", () => {
    expect(style([row([255, 0, 0])], 0).halfWidth).toBeCloseTo(1.25 / 2, 1);
  });

  test("a lineweight is drawn at its true size: 96 CSS pixels to the inch", () => {
    // 1.00 mm is 3.78 px; 2.11 mm is 7.97 px.
    expect(lineweightWidth(100, 1.25)).toBeCloseTo(3.7795, 3);
    expect(lineweightWidth(211, 1.25)).toBeCloseTo(7.9748, 3);
    expect(style([row([255, 0, 0], 100)], 0).halfWidth).toBeCloseTo(3.7795 / 2, 1);
    expect(style([row([255, 0, 0], 211)], 0).halfWidth).toBeCloseTo(7.9748 / 2, 1);
  });

  test("a lineweight thinner than the ordinary width is drawn at the ordinary width", () => {
    for (const hundredths of [0, 5, 13, 25]) {
      expect(lineweightWidth(hundredths, 1.25)).toBe(1.25);
    }
    expect(lineweightWidth(35, 1.25)).toBeGreaterThan(1.25);
  });

  test("with lineweights not shown, every line has the ordinary width", () => {
    const off = { ...LOOK, lineweights: false };
    const rows = [row([255, 0, 0], 211), row("foreground", 100), row([0, 0, 255])];
    const widths = [0, 1, 2].map((index) => style(rows, index, off).halfWidth);
    expect(new Set(widths).size).toBe(1);
    expect(widths[0]).toBeCloseTo(1.25 / 2, 1);
    // The colours are untouched.
    expect(style(rows, 0, off).colour).toEqual([255, 0, 0, 255]);
  });

  test("widths are in device pixels", () => {
    const dense = { ...LOOK, devicePixelRatio: 2 };
    expect(style([row([255, 0, 0], 100)], 0, dense).halfWidth).toBeCloseTo(3.7795, 1);
    expect(style([row([255, 0, 0])], 0, dense).halfWidth).toBeCloseTo(1.25, 1);
  });

  // ADR-0015: the pattern row travels beside the width, untouched by the
  // theme, the LWT toggle, or the device pixel ratio.
  test("a row's pattern is the green channel of its second texel", () => {
    const rows = [row([255, 0, 0]), row("foreground", 50, 3), row([0, 0, 255], 25, 255)];
    expect(style(rows, 0).pattern).toBe(PATTERN_CONTINUOUS);
    expect(style(rows, 1).pattern).toBe(3);
    expect(style(rows, 2).pattern).toBe(255);
    const off = { ...LOOK, lineweights: false, devicePixelRatio: 2 };
    expect(style(rows, 1, off).pattern).toBe(3);
  });
});

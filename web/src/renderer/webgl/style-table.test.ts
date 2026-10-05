import { describe, expect, test } from "vitest";
import { BYTES_PER_STYLE, STYLE_FOREGROUND, STYLE_RGB } from "../../cad/engine";
import { STYLE_TABLE_WIDTH, styleTexels } from "./style-table";

const FOREGROUND = [0.5, 0.25, 1, 1] as const;

describe("styleTexels", () => {
  test("a colour row becomes an opaque texel of that colour", () => {
    const rows = new Uint8Array([255, 0, 0, STYLE_RGB, 10, 20, 30, STYLE_RGB]);
    const texels = styleTexels(rows, FOREGROUND);
    expect([texels.width, texels.height]).toEqual([STYLE_TABLE_WIDTH, 1]);
    expect([...texels.data.slice(0, 8)]).toEqual([255, 0, 0, 255, 10, 20, 30, 255]);
  });

  test("a foreground row takes the theme's colour, whatever its bytes say", () => {
    const rows = new Uint8Array([9, 9, 9, STYLE_FOREGROUND]);
    const texels = styleTexels(rows, FOREGROUND);
    expect([...texels.data.slice(0, 4)]).toEqual([128, 64, 255, 255]);
    // The same drawing on a light theme: the same row, a dark texel.
    expect([...styleTexels(rows, [0.1, 0.1, 0.1, 1]).data.slice(0, 4)]).toEqual([26, 26, 26, 255]);
  });

  test("an empty table is still one row of texels, so the sampler is complete", () => {
    const texels = styleTexels(new Uint8Array(0), FOREGROUND);
    expect(texels.height).toBe(1);
    expect(texels.data.length).toBe(STYLE_TABLE_WIDTH * 4);
  });

  test("more styles than a row holds wrap onto further rows", () => {
    const count = STYLE_TABLE_WIDTH + 3;
    const rows = new Uint8Array(count * BYTES_PER_STYLE);
    for (let style = 0; style < count; style += 1) {
      rows.set([style % 256, 1, 2, STYLE_RGB], style * BYTES_PER_STYLE);
    }
    const texels = styleTexels(rows, FOREGROUND);
    expect(texels.height).toBe(2);
    expect(texels.data.length).toBe(STYLE_TABLE_WIDTH * 2 * 4);
    // Style 1025 is the second texel of the second row, as the shader reads it.
    const at = (STYLE_TABLE_WIDTH + 1) * 4;
    expect([...texels.data.slice(at, at + 4)]).toEqual([(STYLE_TABLE_WIDTH + 1) % 256, 1, 2, 255]);
  });
});

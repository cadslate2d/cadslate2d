import { describe, expect, test } from "vitest";
import { FLOATS_PER_PATTERN } from "../../cad/engine";
import { PATTERN_TABLE_WIDTH, PATTERNS_PER_ROW, patternTexels } from "./linetypes";

/** A pattern row as Rust lays it out: period, count, then the lengths padded to eight. */
function pattern(lengths: number[]): number[] {
  const period = lengths.reduce((sum, length) => sum + length, 0);
  return [period, lengths.length, ...lengths, ...new Array<number>(8 - lengths.length).fill(0)];
}

/** The twelve floats of pattern `index`: its three texels. */
function texelsOf(rows: number[][], index: number): number[] {
  const texels = patternTexels(new Float32Array(rows.flat()));
  return [...texels.data.slice(index * 12, index * 12 + 12)];
}

describe("the pattern table as texels (ADR-0015)", () => {
  test("a pattern is its period and count, then its lengths four to a texel", () => {
    const rows = [pattern([]), pattern([0.5, 0.25]), pattern([1.25, 0.25, 0.25, 0.25, 0.25, 0.25])];
    expect(rows[1]).toHaveLength(FLOATS_PER_PATTERN);
    expect(texelsOf(rows, 0)).toEqual([0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
    expect(texelsOf(rows, 1)).toEqual([0.75, 2, 0, 0, 0.5, 0.25, 0, 0, 0, 0, 0, 0]);
    expect(texelsOf(rows, 2)).toEqual([2.5, 6, 0, 0, 1.25, 0.25, 0.25, 0.25, 0.25, 0.25, 0, 0]);
  });

  test("an empty table is still one row of texels, so the sampler is complete", () => {
    const texels = patternTexels(new Float32Array(0));
    expect(texels.width).toBe(PATTERN_TABLE_WIDTH);
    expect(texels.height).toBe(1);
    expect(texels.data).toHaveLength(PATTERN_TABLE_WIDTH * 4);
    expect([...texels.data.slice(0, 4)]).toEqual([0, 0, 0, 0]);
  });

  test("more patterns than a row holds wrap onto further rows", () => {
    const rows = Array.from({ length: PATTERNS_PER_ROW + 1 }, (_, i) => pattern([i + 1, 1]));
    const texels = patternTexels(new Float32Array(rows.flat()));
    expect(texels.height).toBe(2);
    // The last pattern starts the second row.
    const start = PATTERN_TABLE_WIDTH * 4;
    expect([...texels.data.slice(start, start + 2)]).toEqual([PATTERNS_PER_ROW + 2, 2]);
  });
});

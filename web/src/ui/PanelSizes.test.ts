import { expect, test } from "vitest";
import { clampWidth, maxWidth, MIN_HEIGHT, MIN_WIDTH, moveDivider } from "./PanelSizes";

test("a divider gives one panel what it takes from the other", () => {
  expect(moveDivider(200, 300, 50)).toEqual([250, 250]);
  expect(moveDivider(200, 300, -50)).toEqual([150, 350]);
  const [a, b] = moveDivider(200, 300, 37.5);
  expect(a + b).toBe(500);
});

test("neither panel goes under its least height", () => {
  expect(moveDivider(200, 300, 10_000)).toEqual([500 - MIN_HEIGHT, MIN_HEIGHT]);
  expect(moveDivider(200, 300, -10_000)).toEqual([MIN_HEIGHT, 500 - MIN_HEIGHT]);
  expect(moveDivider(200, 300, Infinity)).toEqual([500 - MIN_HEIGHT, MIN_HEIGHT]);
  // Two panels already too short to keep it share what there is.
  expect(moveDivider(40, 40, 30)).toEqual([40, 40]);
});

test("the column stays between its narrowest and a part of the window", () => {
  expect(clampWidth(100, 1600)).toBe(MIN_WIDTH);
  expect(clampWidth(500, 1600)).toBe(500);
  expect(clampWidth(5000, 1600)).toBe(maxWidth(1600));
  expect(maxWidth(1600)).toBeLessThan(1600);
  // A window too narrow for both still lets the column be its narrowest.
  expect(clampWidth(500, 300)).toBe(MIN_WIDTH);
});

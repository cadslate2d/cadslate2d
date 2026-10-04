import { expect, test } from "vitest";
import { formatCoordinates } from "./StatusBar";

test("formats world coordinates to four decimals", () => {
  expect(formatCoordinates([234.5671, -123.45])).toBe("234.5671, -123.4500");
});

test("shows a dash when the pointer is off the canvas", () => {
  expect(formatCoordinates(undefined)).toBe("—");
});

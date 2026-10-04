import { expect, test } from "vitest";
import { nextCapacity } from "./buffer-capacity";

test("keeps the current capacity when the data fits", () => {
  expect(nextCapacity(8192, 100)).toBe(8192);
  expect(nextCapacity(8192, 8192)).toBe(8192);
});

test("starts at the minimum, even for empty data", () => {
  expect(nextCapacity(0, 0)).toBe(4096);
  expect(nextCapacity(0, 16)).toBe(4096);
});

test("grows geometrically", () => {
  expect(nextCapacity(4096, 4097)).toBe(8192);
});

test("jumps straight to very large requests", () => {
  expect(nextCapacity(4096, 100_000)).toBe(100_000);
});

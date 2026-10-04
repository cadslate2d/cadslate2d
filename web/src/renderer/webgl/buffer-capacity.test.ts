import { expect, test } from "vitest";
import { nextCapacity } from "./buffer-capacity";

test("keeps the current capacity when the data fits", () => {
  expect(nextCapacity(4096, 100)).toBe(4096);
  expect(nextCapacity(4096, 4096)).toBe(4096);
});

test("starts at the minimum", () => {
  expect(nextCapacity(0, 16)).toBe(1024);
});

test("grows geometrically", () => {
  expect(nextCapacity(4096, 4097)).toBe(8192);
});

test("jumps straight to very large requests", () => {
  expect(nextCapacity(4096, 100_000)).toBe(100_000);
});

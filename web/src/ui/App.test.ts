import { expect, test } from "vitest";
import { App } from "./App";

test("App is a component", () => {
  expect(typeof App).toBe("function");
});

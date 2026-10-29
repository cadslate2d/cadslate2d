import { expect, test } from "vitest";
import { elapsed } from "./AiPanel";

test("the time a request has been out reads in seconds, then minutes", () => {
  expect(elapsed(0)).toBe("0s");
  expect(elapsed(47.9)).toBe("47s");
  expect(elapsed(60)).toBe("1m 00s");
  expect(elapsed(65)).toBe("1m 05s");
  expect(elapsed(154.3)).toBe("2m 34s");
  expect(elapsed(-1)).toBe("0s");
});

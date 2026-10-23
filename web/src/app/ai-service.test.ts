import { describe, expect, test } from "vitest";
import { AI_SERVICE_URL, aiServiceUrl } from "./ai-service";

describe("the AI service's address (ADR-0037)", () => {
  test("comes from the build configuration, https or a local http", () => {
    expect(aiServiceUrl("https://ai.cadslate2d.example")?.href).toBe(
      "https://ai.cadslate2d.example/",
    );
    expect(aiServiceUrl("  http://localhost:8080 ")?.port).toBe("8080");
    expect(aiServiceUrl("http://127.0.0.1:8080")?.hostname).toBe("127.0.0.1");
  });

  test("is none without one, or with what is not one", () => {
    for (const value of [
      undefined,
      "",
      "  ",
      "not a url",
      "http://ai.example",
      "ftp://ai.example",
      "https://user:secret@ai.example",
      "https://ai.example/?key=1",
    ]) {
      expect(aiServiceUrl(value), String(value)).toBeUndefined();
    }
  });

  test("this build has none: the tests run without the service", () => {
    expect(AI_SERVICE_URL).toBeUndefined();
  });
});

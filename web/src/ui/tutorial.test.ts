import { readFile } from "node:fs/promises";
import { beforeAll, describe, expect, test } from "vitest";
import { CadEngine } from "../cad/engine";
import { loadEngineForTests } from "../cad/test-support";
import { AI, CHAPTERS, DRAWING } from "./tutorial";

beforeAll(loadEngineForTests);

/** Every request of the AI corpus, at every level (test-data/ai/README.md). */
async function corpus(): Promise<Set<string>> {
  const prompts = new Set<string>();
  for (const level of ["l1", "l2", "l3", "l4", "l5"]) {
    const text = await readFile(
      new URL(`../../../test-data/ai/${level}/cases.jsonl`, import.meta.url),
      "utf8",
    );
    for (const line of text.split("\n")) {
      if (line.trim()) prompts.add((JSON.parse(line) as { prompt: string }).prompt);
    }
  }
  return prompts;
}

describe("the tutorial (Help › Tutorial)", () => {
  test("has the drawing basics, then the AI", () => {
    expect(CHAPTERS.map((chapter) => chapter.id)).toEqual(["drawing", "ai"]);
    expect(DRAWING.steps.length).toBeGreaterThanOrEqual(5);
    expect(AI.steps.length).toBeGreaterThanOrEqual(5);
  });

  test("every step it tries runs a command the core knows, unrefused", () => {
    let tried = 0;
    for (const step of CHAPTERS.flatMap((chapter) => chapter.steps)) {
      if (!step.command) continue;
      tried += 1;
      const engine = new CadEngine();
      engine.resize(800, 600);
      engine.invoke(step.command.command);
      for (const answer of step.command.steps ?? []) {
        if (typeof answer === "string") engine.submitText(answer);
      }
      const refused = engine
        .logSince(0)
        .filter((line) => /is required\.$|^Unknown command/.test(line));
      expect(refused, step.title).toEqual([]);
      engine.dispose();
    }
    expect(tried).toBe(DRAWING.steps.length);
  });

  test("every AI example is a request of the corpus, which the model is checked against", async () => {
    const prompts = await corpus();
    const examples = AI.steps.flatMap((step) => step.examples ?? []);
    expect(examples.length).toBeGreaterThanOrEqual(10);
    for (const example of examples) expect(prompts, example).toContain(example);
  });
});

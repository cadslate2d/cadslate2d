// What a worker could take off the main thread (#178): the mixed drawings
// opened, exported, and autosaved in both browsers, each measured as the
// time the main thread is held. Needs the generated drawings (see
// benches/generated-drawings/README.md).
import { existsSync, readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { expect, test } from "@playwright/test";
import { openApp } from "../e2e/support";

const SIZES = [10_000, 50_000, 100_000];

const drawingPath = (size: number) =>
  fileURLToPath(
    new URL(`../../benches/generated-drawings/mixed-${size / 1000}k.dxf`, import.meta.url),
  );

for (const size of SIZES) {
  test(`main-thread tasks at ${size} mixed entities`, async ({ page, browserName }) => {
    const path = drawingPath(size);
    test.skip(
      !existsSync(path),
      `generate ${path} first (see benches/generated-drawings/README.md)`,
    );
    await openApp(page);
    const report = await page.evaluate(
      async (text) => {
        const handle = window.__cadslate;
        if (!handle) throw new Error("debug handle is not published");
        return handle.probeMainThread(text);
      },
      readFileSync(path, "utf8"),
    );
    console.log(`MAIN_THREAD ${JSON.stringify({ browser: browserName, size, ...report })}`);
    expect(report.imported).toMatch(/^Imported /);
  });
}

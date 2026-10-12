// The §42 targets measured (#171): the mixed drawings at 10k, 50k, and
// 100k entities opened in the application, in Chromium and Firefox, and
// their frames timed while panning, zooming, hovering, and editing.
// Needs the generated drawings (see benches/generated-drawings/README.md).
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
  test(`frames at ${size} mixed entities`, async ({ page, browserName }) => {
    const path = drawingPath(size);
    test.skip(
      !existsSync(path),
      `generate ${path} first (see benches/generated-drawings/README.md)`,
    );
    await openApp(page);
    // Chromium's headless rasteriser (SwiftShader) takes seconds a frame at
    // 10k and tens of seconds beyond: a few frames say enough there, and
    // the larger sizes are skipped unless asked for. MIXED_FRAMES sets the
    // frames per run.
    const software = browserName === "chromium";
    test.skip(
      software && size > 10_000 && !process.env.MIXED_ALL,
      "tens of seconds a frame on SwiftShader; MIXED_ALL=1 runs it",
    );
    const frames = Number(process.env.MIXED_FRAMES ?? (software ? 10 : 60));
    const report = await page.evaluate(
      async ([text, frames]) => {
        const handle = window.__cadslate;
        if (!handle) throw new Error("debug handle is not published");
        return handle.probeFrames(text, frames);
      },
      [readFileSync(path, "utf8"), frames] as const,
    );
    console.log(`MIXED ${JSON.stringify({ browser: browserName, size, ...report })}`);
    expect(report.imported).toMatch(/^Imported /);
  });
}

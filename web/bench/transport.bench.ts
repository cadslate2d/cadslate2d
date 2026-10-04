// Render-transport measurement (DESIGN §12.6). Opens generated drawings in the
// production bundle and runs the in-page probe on each.
import { existsSync, readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { expect, test } from "@playwright/test";
import { chooseFile, enter, logLines, openApp } from "../e2e/support";

const SIZES = [1_000, 10_000, 100_000];

const drawingPath = (size: number) =>
  fileURLToPath(
    new URL(`../../benches/generated-drawings/lines-${size / 1000}k.dxf`, import.meta.url),
  );

for (const size of SIZES) {
  test(`transport at ${size} lines`, async ({ page }) => {
    const path = drawingPath(size);
    test.skip(
      !existsSync(path),
      `generate ${path} first (see benches/generated-drawings/README.md)`,
    );

    await openApp(page);
    const loadStart = Date.now();
    await chooseFile(page, () => enter(page, "OPEN"), path);
    await expect(logLines(page).last()).toHaveText(`Imported ${size} lines.`, { timeout: 120_000 });
    const openMs = Date.now() - loadStart;

    const report = await page.evaluate(async () => {
      const handle = window.__cadslate;
      if (!handle) throw new Error("debug handle is not published");
      return handle.probeTransport();
    });
    // Import alone, without the file picker and the first render.
    const importMs = await page.evaluate(
      async (text) => {
        const handle = window.__cadslate;
        if (!handle) throw new Error("debug handle is not published");
        return handle.probeOpen(text);
      },
      readFileSync(path, "utf8"),
    );

    console.log(`TRANSPORT ${JSON.stringify({ size, openMs, importMs, ...report })}`);
    expect(report.segments).toBe(size);
  });
}

// The M7 exit scenario (DESIGN §35, §42; #179): the 100k mixed drawing
// opened, panned, zoomed into a detail, edited, and saved, within the
// targets as #171 measures them. Needs the generated drawing (see
// benches/generated-drawings/README.md); skipped where it is absent, as in
// CI, and in Chromium, whose software rasteriser takes seconds a frame.
// The bounds it holds are "usable" with room for a machine whose CPU the
// rest of the suite shares: frames under half a second and an open under
// two, where the measured figures (docs/performance.md, the benches) are
// a tenth of that. A regression to seconds a frame fails here; the exact
// figures are the benches' to record.
import { existsSync, readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { expect, test } from "@playwright/test";
import { downloadFrom, enter, openApp, prompt } from "./support";

const DRAWING = fileURLToPath(
  new URL("../../benches/generated-drawings/mixed-100k.dxf", import.meta.url),
);

test("the M7 scenario: a 100k drawing opened, moved about, edited, and saved", async ({
  page,
  browserName,
}) => {
  test.skip(!existsSync(DRAWING), "generate mixed-100k.dxf first");
  test.skip(browserName === "chromium", "SwiftShader takes seconds a frame at this size");
  test.slow();
  await openApp(page);
  const text = readFileSync(DRAWING, "utf8");

  // Opened, panned, zoomed, hovered, edited, and zoomed in on a detail —
  // each frame timed from the input to the GPU finishing (probeFrames).
  const report = await page.evaluate(async (dxf) => {
    const handle = window.__cadslate;
    if (!handle) throw new Error("debug handle is not published");
    return handle.probeFrames(dxf, 60);
  }, text);
  console.log(`M7 ${JSON.stringify({ ...report, imported: undefined })}`);
  expect(report.imported).toMatch(/^Imported 44000 lines/);
  expect(report.openMs, "open").toBeLessThan(2000);
  expect(report.firstFrameMs, "first frame").toBeLessThan(3000);
  expect(report.pan.mean, "pan").toBeLessThan(500);
  expect(report.zoom.mean, "zoom").toBeLessThan(400);
  expect(report.pointer.mean, "pointer").toBeLessThan(500);
  expect(report.edit.mean, "edit").toBeLessThan(600);
  expect(report.zoomedIn.pan.mean, "pan, zoomed in").toBeLessThan(400);

  // The drawing on screen is the probe's: an edit through the command line
  // reaches it, and a save writes every entity back.
  await enter(page, "L", "0,0", "10,10", "");
  await expect(prompt(page)).toHaveText("Command:");
  const file = await downloadFrom(page, () => enter(page, "DXFOUT"));
  expect(file.text.length).toBeGreaterThan(30_000_000);
  // The drawing's own, and those of its block definitions.
  expect(file.text.split("\nLWPOLYLINE\n").length - 1).toBeGreaterThanOrEqual(10_000);
  expect(file.text.split("\nHATCH\n").length - 1).toBeGreaterThanOrEqual(4_000);
  expect(file.text.split("\nDIMENSION\n").length - 1).toBeGreaterThanOrEqual(8_000);
});

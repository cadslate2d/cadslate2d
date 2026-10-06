// Linetypes on screen (ADR-0015, #138): each built-in pattern on a line and
// on an arc, a polyline whose pattern runs on through its vertices, and the
// rules that keep the committed geometry untouched.
import { expect, test, type Locator, type Page } from "@playwright/test";
import {
  centerOf,
  enter,
  litPixelsAround,
  logLines,
  openApp,
  prompt,
  rendererStats,
} from "./support";

const BUILT_IN = ["Dashed", "Hidden", "Center", "Phantom", "Dot", "Dashdot"] as const;

/**
 * Patterns are defined in drawing units, and the unzoomed view shows one
 * unit per CSS pixel: at their defined size every period is under four
 * pixels and drawn solid. Twenty times larger, a Dashed period is 15 px.
 */
const LTSCALE = "20";

/** Loads `name`, makes it current, and scales patterns up to be seen. */
async function useLinetype(page: Page, name: string): Promise<void> {
  await enter(page, "LINETYPE");
  await expect(prompt(page)).toHaveText("Press Enter to finish or [?/Load/Set/Delete]:");
  await enter(page, "L", name, "S", name);
  await expect(logLines(page).last()).toHaveText(`Current linetype: "${name}"`);
  await enter(page, "", "LTSCALE");
  await expect(prompt(page)).toHaveText("Enter new linetype scale factor <1>:");
  await enter(page, LTSCALE);
}

/** A line across the top and a half-circle arc of radius 50 below it, grid off. */
async function lineAndArc(page: Page): Promise<Locator> {
  const canvas = await openApp(page);
  await page.keyboard.press("F7");
  await enter(page, "L", "-80,50", "80,50", "");
  await enter(page, "A", "-50,-30", "0,20", "50,-30");
  await expect(prompt(page)).toHaveText("Command:");
  await page.mouse.move(5, 5);
  return canvas;
}

test.describe("linetypes", () => {
  for (const name of BUILT_IN) {
    test(`${name} on a line and on an arc`, async ({ page, browserName }) => {
      test.skip(browserName !== "chromium", "baselines are recorded in Chromium");
      await openApp(page);
      await page.keyboard.press("F7");
      await useLinetype(page, name);
      await enter(page, "L", "-80,50", "80,50", "");
      await enter(page, "A", "-50,-30", "0,20", "50,-30");
      await expect(prompt(page)).toHaveText("Command:");
      await page.mouse.move(5, 5);

      const canvas = page.getByRole("region", { name: "Model space" }).locator("canvas");
      const center = await centerOf(canvas);
      await expect(page).toHaveScreenshot(`linetype-${name.toLowerCase()}.png`, {
        clip: {
          x: Math.round(center.x) - 90,
          y: Math.round(center.y) - 60,
          width: 180,
          height: 100,
        },
        // A different pattern moves hundreds of pixels; driver antialiasing
        // noise a handful.
        maxDiffPixels: 60,
      });
    });
  }

  test("a polyline's pattern runs on through its vertices", async ({ page }) => {
    const canvas = await openApp(page);
    await page.keyboard.press("F7");
    await useLinetype(page, "Dashed");
    // Dashes of 10 and gaps of 5: along the first segment the dashes are
    // 0–10 and 15–20, the second one starting at 20 — five into a dash. So
    // up the second segment a dash ends 5 above the corner and a gap
    // follows to 10. Had the pattern started again at the corner, the dash
    // would reach 10.
    await enter(page, "PL", "0,0", "20,0", "20,40", "");
    await expect(prompt(page)).toHaveText("Command:");
    await page.mouse.move(5, 5);

    const lit = (x: number, y: number) =>
      litPixelsAround(page, canvas, { x, y }, { width: 1, height: 1 });
    expect(await lit(20, 2), "the dash continues round the corner").toBeGreaterThan(0);
    expect(await lit(20, 8), "and ends five above it").toBe(0);
    expect(await lit(20, 15), "the next dash").toBeGreaterThan(0);
    expect(await lit(5, 0), "the first dash").toBeGreaterThan(0);
    expect(await lit(12, 0), "the first gap").toBe(0);
  });

  test("a pattern too fine to see is drawn solid, and LTSCALE uploads no geometry", async ({
    page,
  }) => {
    const canvas = await lineAndArc(page);
    await page.mouse.move(5, 5);
    const solid = await litPixelsAround(page, canvas, { x: 0, y: 50 }, { width: 40, height: 3 });
    expect(solid).toBeGreaterThan(60);

    // Dashed at its defined size: a period of 0.75 px. Nothing to see, so
    // the line stays whole rather than shimmering.
    await enter(page, "LINETYPE", "L", "Dashed", "");
    await enter(page, "LAYER", "LT", "Dashed", "0", "");
    await page.mouse.move(6, 6);
    expect(await litPixelsAround(page, canvas, { x: 0, y: 50 }, { width: 40, height: 3 })).toBe(
      solid,
    );

    // Scaled up, the gaps appear — through the pattern table alone.
    const before = await rendererStats(page);
    await enter(page, "LTSCALE", LTSCALE);
    await page.mouse.move(7, 7);
    const dashed = await litPixelsAround(page, canvas, { x: 0, y: 50 }, { width: 40, height: 3 });
    expect(dashed).toBeLessThan(solid * 0.8);
    expect(dashed).toBeGreaterThan(solid * 0.5);
    const after = await rendererStats(page);
    expect(after.sceneUploads, "no committed geometry was uploaded").toBe(before.sceneUploads);
    expect(after.styleUploads).toBe(before.styleUploads + 1);

    // Undoing the scale is a document change like any other.
    await page.keyboard.press("Control+z");
    await page.mouse.move(8, 8);
    expect(await litPixelsAround(page, canvas, { x: 0, y: 50 }, { width: 40, height: 3 })).toBe(
      solid,
    );
  });
});

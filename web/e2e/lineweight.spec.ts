// Lineweights on screen (ADR-0014, #136): a heavy line is drawn heavy while
// the LWT toggle is on, measured here from the canvas itself.
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

const lwt = (page: Page) => page.getByRole("button", { name: "LWT", exact: true });

/** Lit pixels in a strip four pixels wide across the horizontal line through (50, y). */
const across = (page: Page, canvas: Locator, y: number) =>
  litPixelsAround(page, canvas, { x: 50, y }, { width: 2, height: 14 });

/** A thin line at y = 0 and a 2.11 mm one at y = 40, with the grid off. */
async function thinAndHeavy(page: Page): Promise<Locator> {
  const canvas = await openApp(page);
  await page.keyboard.press("F7");
  await enter(page, "L", "0,0", "100,0", "");
  await enter(page, "LWEIGHT");
  await expect(prompt(page)).toHaveText(
    "Enter lineweight for new objects (mm, Default, ByLayer, or ByBlock) <ByLayer>:",
  );
  await enter(page, "2.11");
  await expect(logLines(page).last()).toHaveText(
    "Lineweights are not being shown: switch LWT on to see them.",
  );
  await enter(page, "L", "0,40", "100,40", "");
  await page.mouse.move(5, 5);
  return canvas;
}

test.describe("lineweights", () => {
  test("a heavy line is drawn heavy with LWT on, and like any other with it off", async ({
    page,
  }) => {
    const canvas = await thinAndHeavy(page);
    await expect(lwt(page)).toHaveAttribute("aria-pressed", "false");
    const thin = await across(page, canvas, 0);
    expect(thin).toBeGreaterThan(3);
    expect(await across(page, canvas, 40), "LWT off: the same width").toBe(thin);
    const before = await rendererStats(page);

    await lwt(page).click();
    await expect(lwt(page)).toHaveAttribute("aria-pressed", "true");
    await page.mouse.move(6, 6);
    // 2.11 mm is eight pixels: several times the ordinary line.
    const heavy = await across(page, canvas, 40);
    expect(heavy).toBeGreaterThan(3 * thin);
    expect(await across(page, canvas, 0), "the thin line is as it was").toBe(thin);
    // Showing lineweights lays the style table out again; no geometry moves.
    const shown = await rendererStats(page);
    expect(shown.sceneUploads).toBe(before.sceneUploads);
    expect(shown.styleUploads).toBe(before.styleUploads + 1);

    await lwt(page).click();
    await page.mouse.move(7, 7);
    expect(await across(page, canvas, 40)).toBe(thin);
  });

  test("a layer's lineweight is what its ByLayer objects are drawn with", async ({ page }) => {
    const canvas = await openApp(page);
    await page.keyboard.press("F7");
    await enter(page, "LA", "N", "walls", "S", "walls", "", "L", "0,0", "100,0", "");
    await lwt(page).click();
    await page.mouse.move(5, 5);
    const thin = await across(page, canvas, 0);
    const before = await rendererStats(page);

    await enter(page, "LA", "LW", "2", "walls", "");
    await expect(
      logLines(page).filter({ hasText: "1 layer set to lineweight 2.00 mm." }),
    ).toHaveCount(1);
    await page.mouse.move(6, 6);
    // 2 mm is seven and a half pixels against the ordinary one and a quarter.
    expect(await across(page, canvas, 0)).toBeGreaterThan(1.5 * thin);
    expect((await rendererStats(page)).sceneUploads, "a table change only").toBe(
      before.sceneUploads,
    );
  });

  test("zooming does not change the width on screen and uploads nothing", async ({ page }) => {
    const canvas = await thinAndHeavy(page);
    await lwt(page).click();
    // Zoom about a point on the heavy line, so the line stays under the strip.
    const center = await centerOf(canvas);
    const on = { x: center.x + 50, y: center.y - 40 };
    await page.mouse.move(on.x, on.y);
    await page.keyboard.press("F3"); // no snap marker over the strip
    const before = await rendererStats(page);
    const width = await across(page, canvas, 40);
    expect(width).toBeGreaterThan(20);

    for (const delta of [-300, -300, 500, 200]) {
      await page.mouse.wheel(0, delta);
      await page.mouse.move(on.x + 1, on.y);
      await page.mouse.move(on.x, on.y);
      expect(await across(page, canvas, 40), `after a wheel of ${delta}`).toBe(width);
    }
    const after = await rendererStats(page);
    expect(after.sceneUploads).toBe(before.sceneUploads);
    expect(after.styleUploads).toBe(before.styleUploads);
  });
});

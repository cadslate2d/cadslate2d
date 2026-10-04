import { expect, test } from "@playwright/test";
import {
  centerOf,
  enter,
  gridPixels,
  litPixels,
  logLines,
  openApp,
  prompt,
  rendererStats,
  settle,
} from "./support";

test.describe("GRID", () => {
  test("is shown from the start, and F7 or the status bar hides and shows it", async ({ page }) => {
    const canvas = await openApp(page);
    const grid = page.getByRole("button", { name: "GRID" });
    await page.mouse.move(0, 0); // off the canvas: no crosshair in the picture
    await settle(page);
    await expect(grid).toHaveAttribute("aria-pressed", "true");
    expect(await gridPixels(canvas)).toBeGreaterThan(5000);

    await page.keyboard.press("F7");
    await expect(grid).toHaveAttribute("aria-pressed", "false");
    await expect(logLines(page).last()).toHaveText("<GRID off>");
    // Only the rounded corners of the drawing area are left in that range.
    expect(await gridPixels(canvas)).toBeLessThan(50);

    await grid.click();
    await expect(grid).toHaveAttribute("aria-pressed", "true");
    expect(await gridPixels(canvas)).toBeGreaterThan(5000);
  });

  test("is faint: it never counts as drawn geometry", async ({ page }) => {
    const canvas = await openApp(page);
    await page.mouse.move(0, 0);
    await settle(page);
    expect(await litPixels(canvas)).toBe(0);
  });

  test("follows pan and zoom without uploading committed geometry", async ({ page }) => {
    const canvas = await openApp(page);
    await enter(page, "L", "0,0", "100,0", "");
    const { x, y } = await centerOf(canvas);
    const before = await rendererStats(page);

    await page.mouse.move(x, y);
    for (let step = 0; step < 6; step += 1) {
      await page.mouse.wheel(0, step % 2 === 0 ? 300 : -120);
      await settle(page);
      expect(await gridPixels(canvas)).toBeGreaterThan(2000);
    }
    await page.mouse.down({ button: "middle" });
    await page.mouse.move(x + 137, y - 61, { steps: 4 });
    await page.mouse.up({ button: "middle" });
    await settle(page);

    expect(await gridPixels(canvas)).toBeGreaterThan(2000);
    const after = await rendererStats(page);
    expect(after.overlayUploads).toBeGreaterThan(before.overlayUploads);
    expect(after.sceneUploads).toBe(before.sceneUploads);
  });

  test("zooming far out shows fewer lines, never a solid fill", async ({ page }) => {
    const canvas = await openApp(page);
    const { x, y } = await centerOf(canvas);
    const box = await canvas.boundingBox();
    const area = (box?.width ?? 0) * (box?.height ?? 0);

    await page.mouse.move(x, y);
    for (let step = 0; step < 15; step += 1) {
      await page.mouse.wheel(0, 500);
      await settle(page);
      // Lines at least 12px apart cover well under a quarter of the canvas.
      expect(await gridPixels(canvas)).toBeLessThan(area / 4);
    }
    expect(await gridPixels(canvas)).toBeGreaterThan(2000);
  });

  test("the GRID command sets the spacing", async ({ page }) => {
    const canvas = await openApp(page);
    await page.mouse.move(0, 0);
    const dense = await gridPixels(canvas);

    await enter(page, "GRID");
    await expect(prompt(page)).toHaveText("Specify grid spacing <10>:");
    await enter(page, "100");
    await expect(logLines(page).last()).toHaveText("Grid spacing: 100");
    const sparse = await gridPixels(canvas);
    expect(sparse).toBeGreaterThan(1000);
    expect(sparse).toBeLessThan(dense / 2);
  });
});

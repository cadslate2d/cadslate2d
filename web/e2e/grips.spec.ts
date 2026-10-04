import { expect, test, type Locator, type Page } from "@playwright/test";
import {
  centerOf,
  clickWorld,
  downloadFrom,
  enter,
  hotGripPixels,
  hoverWorld,
  logLines,
  openApp,
  prompt,
  rendererStats,
  settle,
} from "./support";

const exported = async (page: Page) => (await downloadFrom(page, () => enter(page, "DXFOUT"))).text;
const snapLabel = (page: Page) => page.getByTestId("snap");

/** A line from the origin to (100, 0), selected by a click away from its grips. */
async function selectedLine(page: Page) {
  const canvas = await openApp(page);
  await enter(page, "L", "0,0", "100,0", "");
  await clickWorld(page, canvas, 30, 0);
  return canvas;
}

test.describe("grips", () => {
  test("hovering a grip makes it hot; selecting and hovering upload no committed geometry", async ({
    page,
  }) => {
    const canvas = await openApp(page);
    await enter(page, "L", "0,0", "100,0", "");
    const before = await rendererStats(page);
    await clickWorld(page, canvas, 30, 0);
    expect(await hotGripPixels(canvas)).toBe(0);

    await hoverWorld(page, canvas, 98, 2);
    expect(await hotGripPixels(canvas)).toBeGreaterThan(20);
    await hoverWorld(page, canvas, 80, 30);
    expect(await hotGripPixels(canvas)).toBe(0);

    await page.keyboard.press("Escape");
    await settle(page);
    expect((await rendererStats(page)).sceneUploads).toBe(before.sceneUploads);
  });

  test("click a grip, click again: the endpoint moves, in one undoable step", async ({ page }) => {
    const canvas = await selectedLine(page);
    await clickWorld(page, canvas, 100, 0);
    await expect(prompt(page)).toHaveText("Specify stretch point:");
    await expect(logLines(page).last()).toHaveText("** STRETCH **");
    const during = await rendererStats(page);

    await hoverWorld(page, canvas, 140, 60);
    expect((await rendererStats(page)).sceneUploads).toBe(during.sceneUploads);
    await page.keyboard.press("F3"); // free placement
    await clickWorld(page, canvas, 150, 50);
    await expect(prompt(page)).toHaveText("Command:");
    expect(await exported(page)).toContain(" 10\n0.0\n 20\n0.0\n 11\n150.0\n 21\n50.0\n");

    await page.keyboard.press("Control+z");
    expect(await exported(page)).toContain(" 11\n100.0\n 21\n0.0\n");
  });

  test("a grip can be dragged and dropped", async ({ page }) => {
    const canvas = await selectedLine(page);
    await page.keyboard.press("F3");
    const { x, y } = await centerOf(canvas);
    await page.mouse.move(x + 100, y);
    await page.mouse.down();
    await page.mouse.move(x + 120, y - 40, { steps: 5 });
    await page.mouse.up();
    await settle(page);
    await expect(prompt(page)).toHaveText("Command:");
    expect(await exported(page)).toContain(" 11\n120.0\n 21\n40.0\n");
  });

  test("a grip takes typed coordinates and Escape puts it back", async ({ page }) => {
    const canvas = await selectedLine(page);
    await clickWorld(page, canvas, 100, 0);
    await enter(page, "@0,25");
    expect(await exported(page)).toContain(" 11\n100.0\n 21\n25.0\n");

    await clickWorld(page, canvas, 0, 0);
    await hoverWorld(page, canvas, -60, -60);
    await page.keyboard.press("Escape");
    await expect(prompt(page)).toHaveText("Command:");
    expect(await exported(page)).toContain(" 10\n0.0\n 20\n0.0\n 11\n100.0\n 21\n25.0\n");
  });

  test("a grip snaps to other entities but not to its own", async ({ page }) => {
    const canvas = await openApp(page);
    await enter(page, "L", "0,0", "100,0", "", "L", "150,40", "150,140", "");
    await clickWorld(page, canvas, 30, 0);
    await clickWorld(page, canvas, 100, 0);

    await hoverWorld(page, canvas, 52, 2); // its own midpoint
    await expect(snapLabel(page)).toHaveCount(0);
    await hoverWorld(page, canvas, 148, 42); // the other line's endpoint
    await expect(snapLabel(page)).toHaveText("Endpoint");
    await clickWorld(page, canvas, 148, 42);
    expect(await exported(page)).toContain(" 10\n0.0\n 20\n0.0\n 11\n150.0\n 21\n40.0\n");
  });

  test("a circle's quadrant grip sets the radius", async ({ page }) => {
    const canvas = await openApp(page);
    await enter(page, "C", "0,0", "50");
    await clickWorld(page, canvas, 30, 40);
    await clickWorld(page, canvas, 50, 0);
    await enter(page, "80,0");
    expect(await exported(page)).toContain("CIRCLE\n  8\n0\n 10\n0.0\n 20\n0.0\n 40\n80.0\n");
  });
});

// Baselines: Chromium only, like the snap markers.
test.describe("grips (visual baseline)", () => {
  const HALF_CLIP = 14;
  const clipAt = async (canvas: Locator, x: number) => {
    const center = await centerOf(canvas);
    return {
      x: Math.round(center.x + x) - HALF_CLIP,
      y: Math.round(center.y) - HALF_CLIP,
      width: 2 * HALF_CLIP,
      height: 2 * HALF_CLIP,
    };
  };

  test("a grip, idle and hot", async ({ page, browserName }) => {
    test.skip(browserName !== "chromium", "baselines are recorded in Chromium");
    const canvas = await selectedLine(page);
    await hoverWorld(page, canvas, 30, 60);
    await expect(page).toHaveScreenshot("grip-idle.png", {
      clip: await clipAt(canvas, 100),
      maxDiffPixels: 40,
    });

    await hoverWorld(page, canvas, 98, 2);
    await expect(page).toHaveScreenshot("grip-hot.png", {
      clip: await clipAt(canvas, 100),
      maxDiffPixels: 40,
    });
  });
});

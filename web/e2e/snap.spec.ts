import { expect, test, type Page } from "@playwright/test";
import {
  centerOf,
  clickWorld,
  downloadFrom,
  enter,
  hoverWorld,
  markerPixels,
  openApp,
  rendererStats,
} from "./support";

const snapLabel = (page: Page) => page.getByTestId("snap");
const coordinates = (page: Page) => page.getByTestId("coordinates");
const ONE_LINE = ["L", "0,0", "100,0", ""];

test.describe("object snap", () => {
  test("hovering near an endpoint marks it, names it, and reports the exact point", async ({
    page,
  }) => {
    const canvas = await openApp(page);
    await enter(page, ...ONE_LINE, "L");

    await hoverWorld(page, canvas, 97, 3);
    await expect(snapLabel(page)).toHaveText("Endpoint");
    await expect(coordinates(page)).toHaveText("100.0000, 0.0000");
    expect(await markerPixels(canvas)).toBeGreaterThan(20);

    await hoverWorld(page, canvas, 60, 80);
    await expect(snapLabel(page)).toHaveCount(0);
    expect(await markerPixels(canvas)).toBe(0);
  });

  test("a click near a snap point commits the exact point", async ({ page }) => {
    const canvas = await openApp(page);
    await enter(page, ...ONE_LINE, "L");
    await clickWorld(page, canvas, 97, 3);
    await enter(page, "@0,50", "");

    const file = await downloadFrom(page, () => enter(page, "DXFOUT"));
    expect(file.text).toContain("LINE\n  8\n0\n 10\n100.0\n 20\n0.0\n 11\n100.0\n 21\n50.0\n");
  });

  test("there is no snapping at the idle prompt, where the pointer selects", async ({ page }) => {
    const canvas = await openApp(page);
    await enter(page, ...ONE_LINE);
    await hoverWorld(page, canvas, 99, 1);
    await expect(snapLabel(page)).toHaveCount(0);
    expect(await markerPixels(canvas)).toBe(0);
  });

  test("F3 and the status bar switch snapping off and on", async ({ page }) => {
    const canvas = await openApp(page);
    const osnap = page.getByRole("button", { name: "OSNAP" });
    await enter(page, ...ONE_LINE, "L");
    await hoverWorld(page, canvas, 97, 3);
    await expect(snapLabel(page)).toHaveText("Endpoint");

    await page.keyboard.press("F3");
    await expect(osnap).toHaveAttribute("aria-pressed", "false");
    await expect(snapLabel(page)).toHaveCount(0);
    expect(await markerPixels(canvas)).toBe(0);
    await expect(coordinates(page)).not.toHaveText("100.0000, 0.0000");

    await osnap.click();
    await expect(osnap).toHaveAttribute("aria-pressed", "true");
    await hoverWorld(page, canvas, 97, 3); // back from the status bar
    await expect(snapLabel(page)).toHaveText("Endpoint");
  });

  test("sweeping the pointer across snap points never uploads committed geometry", async ({
    page,
  }) => {
    const canvas = await openApp(page);
    await enter(page, ...ONE_LINE, "C", "50,0", "30", "L");
    const before = await rendererStats(page);

    let snapped = 0;
    for (let x = -10; x <= 110; x += 8) {
      await hoverWorld(page, canvas, x, 2);
      snapped += await snapLabel(page).count();
    }
    const after = await rendererStats(page);
    expect(snapped).toBeGreaterThan(3);
    expect(after.overlayUploads).toBeGreaterThan(before.overlayUploads);
    expect(after.sceneUploads).toBe(before.sceneUploads);
  });

  test("the intersection of two lines is a snap point", async ({ page }) => {
    const canvas = await openApp(page);
    await enter(page, "L", "0,0", "100,100", "", "L", "0,100", "100,0", "", "C");
    await hoverWorld(page, canvas, 53, 49);
    await expect(snapLabel(page)).toHaveText("Intersection");
    await expect(coordinates(page)).toHaveText("50.0000, 50.0000");
  });

  test("OSNAP chooses which snaps run", async ({ page }) => {
    const canvas = await openApp(page);
    await enter(page, ...ONE_LINE, "OSNAP", "E", "", "L");

    await hoverWorld(page, canvas, 98, 2);
    await expect(snapLabel(page)).toHaveCount(0);
    await hoverWorld(page, canvas, 48, 2);
    await expect(snapLabel(page)).toHaveText("Midpoint");
  });

  test("an entity being moved is not a snap target, but the others are", async ({ page }) => {
    const canvas = await openApp(page);
    await enter(page, ...ONE_LINE, "L", "0,60", "100,60", "");
    await clickWorld(page, canvas, 30, 0);
    await enter(page, "M", "0,0");

    await hoverWorld(page, canvas, 98, 2); // its own old endpoint
    await expect(snapLabel(page)).toHaveCount(0);
    await hoverWorld(page, canvas, 98, 58); // the other line's endpoint
    await expect(snapLabel(page)).toHaveText("Endpoint");
    await expect(coordinates(page)).toHaveText("100.0000, 60.0000");
  });
});

// One baseline image per marker. Rendering differs slightly between browser
// engines, so the comparison runs in Chromium, the browser CI uses.
test.describe("snap markers (visual baseline)", () => {
  const CIRCLE = ["C", "0,0", "60"];
  const CASES = [
    { mode: "Endpoint", setup: ONE_LINE, hover: [98, 2], at: [100, 0] },
    { mode: "Midpoint", setup: ONE_LINE, hover: [52, 2], at: [50, 0] },
    { mode: "Center", setup: CIRCLE, hover: [3, 2], at: [0, 0] },
    { mode: "Quadrant", setup: CIRCLE, hover: [58, 3], at: [60, 0] },
    {
      mode: "Intersection",
      setup: ["L", "0,0", "100,100", "", "L", "0,100", "100,0", ""],
      hover: [52, 48],
      at: [50, 50],
    },
    { mode: "Nearest", setup: [...ONE_LINE, "OSNAP", "N", ""], hover: [80, 3], at: [80, 0] },
  ] as const;
  /** Half the side of the square compared: the marker and a small margin. */
  const HALF_CLIP = 14;

  for (const { mode, setup, hover, at } of CASES) {
    test(`${mode} marker`, async ({ page, browserName }) => {
      test.skip(browserName !== "chromium", "baselines are recorded in Chromium");
      const canvas = await openApp(page);
      await enter(page, ...setup, "L");
      await hoverWorld(page, canvas, hover[0], hover[1]);
      await expect(snapLabel(page)).toHaveText(mode);

      const center = await centerOf(canvas);
      await expect(page).toHaveScreenshot(`snap-marker-${mode.toLowerCase()}.png`, {
        clip: {
          x: Math.round(center.x + at[0]) - HALF_CLIP,
          y: Math.round(center.y - at[1]) - HALF_CLIP,
          width: 2 * HALF_CLIP,
          height: 2 * HALF_CLIP,
        },
        // A different marker changes well over a hundred pixels; antialiasing
        // noise between GPU drivers changes a handful.
        maxDiffPixels: 40,
      });
    });
  }
});

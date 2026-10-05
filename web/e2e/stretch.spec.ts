import { expect, test, type Page } from "@playwright/test";
import {
  clickWorld,
  downloadFrom,
  enter,
  hoverWorld,
  litPixels,
  logLines,
  openApp,
  prompt,
  rendererStats,
} from "./support";

const exported = async (page: Page) => (await downloadFrom(page, () => enter(page, "DXFOUT"))).text;

/** Every LINE in a DXF export, as [x0, y0, x1, y1]. */
function lines(text: string): number[][] {
  const pattern = /LINE\n {2}8\n0\n 10\n(.+)\n 20\n(.+)\n 11\n(.+)\n 21\n(.+)\n/g;
  return [...text.matchAll(pattern)].map((match) => match.slice(1).map(Number));
}

/** A 100 × 60 box drawn as four separate lines. */
async function box(page: Page) {
  const canvas = await openApp(page);
  await enter(page, "L", "0,0", "100,0", "100,60", "0,60", "C");
  return canvas;
}

test.describe("STRETCH", () => {
  test("moves the ends inside the window and leaves the rest", async ({ page }) => {
    await box(page);
    await enter(page, "S");
    await expect(prompt(page)).toHaveText("Specify first corner of crossing window:");
    await enter(page, "80,-20");
    await expect(prompt(page)).toHaveText("Specify opposite corner:");
    await enter(page, "130,80");
    await expect(logLines(page).last()).toHaveText("3 found");
    await expect(prompt(page)).toHaveText("Specify base point:");
    await enter(page, "100,0");
    await expect(prompt(page)).toHaveText("Specify second point:");
    await enter(page, "@40,0");
    await expect(prompt(page)).toHaveText("Command:");

    expect(lines(await exported(page))).toEqual([
      [0, 0, 140, 0],
      [140, 0, 140, 60],
      [140, 60, 0, 60],
      [0, 60, 0, 0],
    ]);

    await page.keyboard.press("Control+z");
    expect(lines(await exported(page))[1]).toEqual([100, 0, 100, 60]);
  });

  test("is drawn with the pointer: window, preview, and commit", async ({ page }) => {
    const canvas = await box(page);
    await page.keyboard.press("F3"); // the points below come from the pointer alone
    await enter(page, "STRETCH");
    await clickWorld(page, canvas, 130, 80);
    await hoverWorld(page, canvas, 80, -20);
    await clickWorld(page, canvas, 80, -20);
    await expect(logLines(page).last()).toHaveText("3 found");

    await clickWorld(page, canvas, 100, 0);
    const before = await rendererStats(page);
    const idle = await litPixels(canvas);
    for (let step = 1; step <= 6; step += 1) {
      await hoverWorld(page, canvas, 100 + step * 10, step * 2);
    }
    const during = await rendererStats(page);
    expect(during.sceneUploads).toBe(before.sceneUploads);
    // The stretched lines are previewed on top of the originals.
    expect(await litPixels(canvas)).toBeGreaterThan(idle + 100);

    await clickWorld(page, canvas, 160, 12);
    expect((await rendererStats(page)).sceneUploads).toBe(before.sceneUploads + 1);
    const [bottom, right, top, left] = lines(await exported(page));
    expect(left).toEqual([0, 60, 0, 0]);
    expect(bottom?.slice(0, 2)).toEqual([0, 0]);
    expect(top?.slice(2)).toEqual([0, 60]);
    // The right-hand line moved whole, and the others still meet it.
    expect((right?.[3] ?? 0) - (right?.[1] ?? 0)).toBeCloseTo(60, 9);
    expect(right?.[0]).toBeCloseTo(160, 0);
    expect(bottom?.slice(2)).toEqual(right?.slice(0, 2));
    expect(top?.slice(0, 2)).toEqual(right?.slice(2));
  });

  test("moves a circle only if its centre is inside the window", async ({ page }) => {
    await openApp(page);
    await enter(page, "C", "100,30", "15", "C", "60,30", "30");
    await enter(page, "S", "80,-20", "130,80");
    await expect(logLines(page).last()).toHaveText("1 found");
    await enter(page, "0,0", "50,0");

    const text = await exported(page);
    expect(text).toContain("CIRCLE\n  8\n0\n 10\n150.0\n 20\n30.0\n 40\n15.0\n");
    expect(text).toContain("CIRCLE\n  8\n0\n 10\n60.0\n 20\n30.0\n 40\n30.0\n");
  });

  test("says so when the window holds nothing", async ({ page }) => {
    await openApp(page);
    await enter(page, "L", "0,10", "300,10", "");
    await enter(page, "S", "80,-20", "130,80");
    await expect(logLines(page).last()).toHaveText("Nothing in the window to stretch.");
    await expect(prompt(page)).toHaveText("Specify first corner of crossing window:");
  });
});

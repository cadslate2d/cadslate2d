import { expect, test, type Page } from "@playwright/test";
import {
  clickWorld,
  downloadFrom,
  enter,
  hoverWorld,
  litPixels,
  openApp,
  prompt,
  rendererStats,
} from "./support";

/** One horizontal line from the origin, selected, at the idle prompt. */
async function selectedLine(page: Page) {
  const canvas = await openApp(page);
  await enter(page, "L", "0,0", "100,0", "");
  await clickWorld(page, canvas, 50, 0);
  return canvas;
}

const exported = async (page: Page) => (await downloadFrom(page, () => enter(page, "DXFOUT"))).text;

test.describe("ROTATE", () => {
  test("rotates the selection by a typed angle", async ({ page }) => {
    await selectedLine(page);
    await enter(page, "RO");
    await expect(prompt(page)).toHaveText("Specify base point:");
    await enter(page, "0,0");
    await expect(prompt(page)).toHaveText("Specify rotation angle:");
    await enter(page, "90");
    await expect(prompt(page)).toHaveText("Command:");
    expect(await exported(page)).toContain(
      "LINE\n  8\n0\n 10\n0.0\n 20\n0.0\n 11\n0.0\n 21\n100.0\n",
    );
  });

  test("previews with the pointer without uploading committed geometry, then commits", async ({
    page,
  }) => {
    const canvas = await selectedLine(page);
    await enter(page, "RO", "0,0");
    const before = await rendererStats(page);
    const idle = await litPixels(canvas);

    for (let step = 1; step <= 8; step += 1) {
      await hoverWorld(page, canvas, 120 - step * 12, step * 14);
    }
    await page.keyboard.press("F3"); // no snapping: the angle comes from the pointer alone
    await hoverWorld(page, canvas, 0, 90);
    const during = await rendererStats(page);
    expect(during.sceneUploads).toBe(before.sceneUploads);
    // The original, its rotated preview, and the rubber band are all on screen.
    expect(await litPixels(canvas)).toBeGreaterThan(idle + 100);

    await clickWorld(page, canvas, 0, 90);
    expect((await rendererStats(page)).sceneUploads).toBe(before.sceneUploads + 1);
    expect(await exported(page)).toContain(" 11\n0.0\n 21\n100.0\n");
  });

  test("one undo restores the original", async ({ page }) => {
    await selectedLine(page);
    await enter(page, "RO", "0,0", "45");
    await page.keyboard.press("Control+z");
    expect(await exported(page)).toContain(
      "LINE\n  8\n0\n 10\n0.0\n 20\n0.0\n 11\n100.0\n 21\n0.0\n",
    );
  });
});

test.describe("SCALE", () => {
  test("scales the selection about the base point", async ({ page }) => {
    await selectedLine(page);
    await enter(page, "SC", "100,0");
    await expect(prompt(page)).toHaveText("Specify scale factor:");
    await enter(page, "0.5");
    expect(await exported(page)).toContain(
      "LINE\n  8\n0\n 10\n50.0\n 20\n0.0\n 11\n100.0\n 21\n0.0\n",
    );
  });

  test("refuses zero and negative factors and keeps asking", async ({ page }) => {
    await selectedLine(page);
    await enter(page, "SC", "0,0", "0");
    await expect(page.getByTestId("log-line").last()).toHaveText(
      "The scale factor must be greater than zero.",
    );
    await enter(page, "-3");
    await expect(prompt(page)).toHaveText("Specify scale factor:");
    await enter(page, "2");
    expect(await exported(page)).toContain(" 11\n200.0\n 21\n0.0\n");
  });
});

test.describe("MIRROR", () => {
  test("adds a reflection and keeps the original by default", async ({ page }) => {
    await selectedLine(page);
    await enter(page, "MI");
    await expect(prompt(page)).toHaveText("Specify first point of mirror line:");
    await enter(page, "0,0", "0,10");
    await expect(prompt(page)).toHaveText("Erase source objects? [Yes/No] <No>:");
    await enter(page, "");

    const text = await exported(page);
    expect(text.match(/^LINE$/gm)).toHaveLength(2);
    expect(text).toContain(" 11\n100.0\n 21\n0.0\n");
    expect(text).toContain(" 11\n-100.0\n 21\n0.0\n");
  });

  test("can erase the original", async ({ page }) => {
    await selectedLine(page);
    await enter(page, "MI", "0,0", "0,10", "Y");
    const text = await exported(page);
    expect(text.match(/^LINE$/gm)).toHaveLength(1);
    expect(text).toContain(" 11\n-100.0\n 21\n0.0\n");
  });
});

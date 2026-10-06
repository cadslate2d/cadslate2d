// Text on screen (ADR-0016, #143): a line of text drawn with the stroke
// font through the line instances, at two zooms and turned.
import { expect, test, type Page } from "@playwright/test";
import {
  centerOf,
  clickWorld,
  downloadFrom,
  enter,
  litPixelsAround,
  logLines,
  openApp,
  prompt,
} from "./support";

/** Compares a clip around the canvas centre with its baseline. */
async function matches(page: Page, name: string, clip: { width: number; height: number }) {
  const canvas = page.getByRole("region", { name: "Model space" }).locator("canvas");
  const center = await centerOf(canvas);
  await expect(page).toHaveScreenshot(name, {
    clip: {
      x: Math.round(center.x) - clip.width / 2,
      y: Math.round(center.y) - clip.height / 2,
      width: clip.width,
      height: clip.height,
    },
    // Different glyphs move hundreds of pixels; driver antialiasing a handful.
    maxDiffPixels: 60,
  });
}

test.describe("text", () => {
  test("a line of text, at two zooms and turned (visual baseline)", async ({
    page,
    browserName,
  }) => {
    test.skip(browserName !== "chromium", "baselines are recorded in Chromium");
    await openApp(page);
    await page.keyboard.press("F7");
    await enter(page, "TEXT", "-60,-8", "16", "0", "Slate 12.5");
    await expect(prompt(page)).toHaveText("Command:");
    await page.mouse.move(5, 5);
    await matches(page, "text-zoom-1.png", { width: 180, height: 60 });

    // Four times closer: the strokes stay one line wide.
    const canvas = page.getByRole("region", { name: "Model space" }).locator("canvas");
    const center = await centerOf(canvas);
    await page.mouse.move(center.x, center.y);
    await page.mouse.wheel(0, -1000);
    await page.mouse.wheel(0, -1000);
    await page.mouse.move(5, 5);
    await matches(page, "text-zoom-in.png", { width: 360, height: 120 });

    // Back out, and a second text turned 30 degrees.
    await page.mouse.move(center.x, center.y);
    await page.mouse.wheel(0, 1000);
    await page.mouse.wheel(0, 1000);
    await enter(page, "TEXT", "-70,-70", "16", "30", "Turned");
    await page.mouse.move(5, 5);
    await matches(page, "text-turned.png", { width: 200, height: 160 });
  });

  test("text is drawn, picked by its box, edited, and exported", async ({ page }) => {
    const canvas = await openApp(page);
    await page.keyboard.press("F7");
    const empty = await litPixelsAround(page, canvas, { x: 40, y: 8 }, { width: 60, height: 20 });
    await enter(page, "TEXT", "0,0", "16", "0", "Hello");
    await expect(prompt(page)).toHaveText("Command:");
    await page.mouse.move(5, 5);
    expect(
      await litPixelsAround(page, canvas, { x: 40, y: 8 }, { width: 60, height: 20 }),
    ).toBeGreaterThan(empty + 100);

    // A click inside the box selects it, and the panel says what it is.
    await clickWorld(page, canvas, 10, 8);
    await expect(page.getByTestId("selection-headline")).toHaveText("1 text");
    await page.keyboard.press("Escape");

    await enter(page, "TEXTEDIT");
    await clickWorld(page, canvas, 10, 8);
    await expect(prompt(page)).toHaveText("Enter new text <Hello>:");
    await enter(page, "Hello world");
    await expect(prompt(page)).toHaveText("Command:");

    const file = await downloadFrom(page, () => enter(page, "DXFOUT"));
    expect(file.text).toContain(
      "  0\nTEXT\n  8\n0\n 10\n0.0\n 20\n0.0\n 40\n16.0\n  1\nHello world\n",
    );
    await expect(logLines(page).last()).toHaveText("Drawing exported as DXF.");
  });
});

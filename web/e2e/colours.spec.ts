// Colours on screen (ADR-0014): committed geometry is drawn in the colour of
// its layer, read back here from the canvas itself.
import { expect, test, type Locator, type Page } from "@playwright/test";
import { enter, openApp, pixelsAround, rendererStats } from "./support";

type Pixel = [red: number, green: number, blue: number];
// A line is a pixel wide and antialiased, so a pixel of it is its colour at
// part strength over the dark canvas: one channel clearly lit, and well
// above the other two.
const dominant = (lit: number, others: number[]) =>
  lit > 90 && others.every((other) => lit > 2.5 * other);
const isRed = ([red, green, blue]: Pixel) => dominant(red, [green, blue]);
const isGreen = ([red, green, blue]: Pixel) => dominant(green, [red, blue]);
const isBlue = ([red, green, blue]: Pixel) => dominant(blue, [red, green]);
/** Lit and uncoloured: the theme's foreground on the dark canvas. */
const isForeground = ([red, green, blue]: Pixel) =>
  red + green + blue > 3 * 96 && Math.max(red, green, blue) - Math.min(red, green, blue) < 60;

/** How many pixels of each kind lie along the line through (50, y). */
async function along(page: Page, canvas: Locator, y: number) {
  const count = (matches: (pixel: Pixel) => boolean) =>
    pixelsAround(page, canvas, { x: 50, y }, { width: 40, height: 4 }, matches);
  return {
    red: await count(isRed),
    green: await count(isGreen),
    blue: await count(isBlue),
    foreground: await count(isForeground),
  };
}

test.describe("layer colours", () => {
  test("a line is drawn in the colour of its layer", async ({ page }) => {
    const canvas = await openApp(page);
    await page.keyboard.press("F7"); // no grid
    await enter(page, "LA", "N", "walls,doors", "C", "1", "walls", "C", "5", "doors", "");
    await enter(page, "LA", "S", "walls", "", "L", "0,0", "100,0", "");
    await enter(page, "LA", "S", "doors", "", "L", "0,40", "100,40", "");
    await enter(page, "LA", "S", "0", "", "L", "0,-40", "100,-40", "");
    await page.mouse.move(5, 5);

    const walls = await along(page, canvas, 0);
    expect(walls.red).toBeGreaterThan(60);
    expect(walls.green + walls.blue + walls.foreground).toBe(0);

    const doors = await along(page, canvas, 40);
    expect(doors.blue).toBeGreaterThan(60);
    expect(doors.red + doors.green + doors.foreground).toBe(0);

    // Layer 0 has colour 7: light on the dark canvas, not a colour.
    const zero = await along(page, canvas, -40);
    expect(zero.foreground).toBeGreaterThan(60);
    expect(zero.red + zero.green + zero.blue).toBe(0);
  });

  test("recolouring a layer changes what is drawn without uploading geometry", async ({ page }) => {
    const canvas = await openApp(page);
    await page.keyboard.press("F7");
    await enter(page, "LA", "N", "walls", "C", "1", "walls", "S", "walls", "");
    await enter(page, "L", "0,0", "100,0", "", "C", "50,60", "20");
    await page.mouse.move(5, 5);
    expect((await along(page, canvas, 0)).red).toBeGreaterThan(60);
    const before = await rendererStats(page);

    await enter(page, "LA", "C", "3", "walls", "");
    await page.mouse.move(6, 6);
    const line = await along(page, canvas, 0);
    expect(line.green).toBeGreaterThan(60);
    expect(line.red).toBe(0);
    // The circle, an arc instance, follows too: its bottom is at (50, 40).
    expect((await along(page, canvas, 40)).green).toBeGreaterThan(10);

    const after = await rendererStats(page);
    expect(after.sceneUploads, "no geometry was uploaded").toBe(before.sceneUploads);
    expect(after.styleUploads).toBe(before.styleUploads + 1);

    // Undo puts the colour back, again through the table alone.
    await page.keyboard.press("Control+z");
    await page.mouse.move(7, 7);
    expect((await along(page, canvas, 0)).red).toBeGreaterThan(60);
    expect((await rendererStats(page)).sceneUploads).toBe(before.sceneUploads);
  });

  test("an object's own colour wins over its layer's; ByLayer follows it", async ({ page }) => {
    const canvas = await openApp(page);
    await page.keyboard.press("F7");
    // One line coloured by its layer, one with a colour of its own.
    await enter(page, "L", "0,0", "100,0", "");
    await enter(page, "COLOR", "red", "L", "0,40", "100,40", "");
    await page.mouse.move(5, 5);
    expect((await along(page, canvas, 0)).foreground).toBeGreaterThan(60);
    expect((await along(page, canvas, 40)).red).toBeGreaterThan(60);

    await enter(page, "LA", "C", "blue", "0", "");
    await page.mouse.move(6, 6);
    expect((await along(page, canvas, 0)).blue).toBeGreaterThan(60);
    const own = await along(page, canvas, 40);
    expect(own.red).toBeGreaterThan(60);
    expect(own.blue).toBe(0);

    // Back to ByLayer: the next line takes the layer's blue.
    await enter(page, "COLOR", "ByLayer", "L", "0,-40", "100,-40", "");
    await page.mouse.move(7, 7);
    expect((await along(page, canvas, -40)).blue).toBeGreaterThan(60);
  });

  test("pointer movement, pan, and zoom upload neither geometry nor styles", async ({ page }) => {
    const canvas = await openApp(page);
    await enter(page, "LA", "N", "walls", "C", "1", "walls", "S", "walls", "");
    await enter(page, "L", "0,0", "100,0", "");
    await page.mouse.move(300, 300);
    const before = await rendererStats(page);

    const box = await canvas.boundingBox();
    if (!box) throw new Error("the canvas has no box");
    for (let step = 0; step < 10; step += 1) {
      await page.mouse.move(box.x + 100 + step * 20, box.y + 100 + step * 10);
    }
    await page.mouse.wheel(0, -200);
    await page.mouse.down({ button: "middle" });
    await page.mouse.move(box.x + 250, box.y + 220);
    await page.mouse.up({ button: "middle" });

    const after = await rendererStats(page);
    expect(after.sceneUploads).toBe(before.sceneUploads);
    expect(after.styleUploads).toBe(before.styleUploads);
    expect(after.frames).toBeGreaterThan(before.frames);
  });
});

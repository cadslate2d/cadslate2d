import { expect, type Locator, type Page } from "@playwright/test";
import { PNG } from "pngjs";

export interface RendererStats {
  frames: number;
  drawCalls: number;
  sceneUploads: number;
  overlayUploads: number;
  bufferReallocations: number;
}

/** Opens the app with debug counters enabled and waits for the first frame. */
export async function openApp(page: Page): Promise<Locator> {
  await page.goto("./?debug");
  const canvas = page.getByRole("region", { name: "Model space" }).locator("canvas");
  await expect(canvas).toBeVisible();
  await expect.poll(async () => (await rendererStats(page)).frames).toBeGreaterThan(0);
  return canvas;
}

export async function rendererStats(page: Page): Promise<RendererStats> {
  return page.evaluate(() => {
    const handle = window.__cadslate;
    if (!handle) throw new Error("debug handle is not published");
    return { ...handle.renderer };
  });
}

/** Lets queued animation frames run. */
export async function settle(page: Page): Promise<void> {
  await page.evaluate(
    () =>
      new Promise<void>((resolve) =>
        requestAnimationFrame(() => requestAnimationFrame(() => resolve())),
      ),
  );
}

/** Page coordinates of the centre of `canvas`. */
export async function centerOf(canvas: Locator): Promise<{ x: number; y: number }> {
  const box = await canvas.boundingBox();
  if (!box) throw new Error("canvas is not laid out");
  return { x: box.x + box.width / 2, y: box.y + box.height / 2 };
}

/** Types each line and presses Enter after it. */
export async function enter(page: Page, ...lines: string[]): Promise<void> {
  for (const line of lines) {
    if (line) await page.keyboard.type(line);
    await page.keyboard.press("Enter");
  }
  await settle(page);
}

export const prompt = (page: Page): Locator => page.getByTestId("prompt");
export const logLines = (page: Page): Locator => page.getByTestId("log-line");
export const commandInput = (page: Page): Locator => page.locator("#command-input");

/**
 * Counts canvas pixels that are clearly brighter than the background, i.e.
 * pixels covered by drawn geometry. Reads a real screenshot, so it sees
 * exactly what the user sees.
 */
export async function litPixels(canvas: Locator): Promise<number> {
  const image = PNG.sync.read(await canvas.screenshot());
  let lit = 0;
  for (let offset = 0; offset < image.data.length; offset += 4) {
    const [red, green, blue] = [
      image.data[offset] ?? 0,
      image.data[offset + 1] ?? 0,
      image.data[offset + 2] ?? 0,
    ];
    if (red + green + blue > 3 * 96) lit += 1;
  }
  return lit;
}

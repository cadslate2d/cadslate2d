import { readFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import { expect, type Locator, type Page } from "@playwright/test";
import { PNG } from "pngjs";

export interface RendererStats {
  frames: number;
  drawCalls: number;
  sceneUploads: number;
  styleUploads: number;
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

type Rgb = readonly [number, number, number];

/** Counts canvas pixels matching `predicate`, from a real screenshot — exactly what the user sees. */
async function countPixels(canvas: Locator, predicate: (rgb: Rgb) => boolean): Promise<number> {
  const image = PNG.sync.read(await canvas.screenshot());
  let count = 0;
  for (let offset = 0; offset < image.data.length; offset += 4) {
    const rgb = [
      image.data[offset] ?? 0,
      image.data[offset + 1] ?? 0,
      image.data[offset + 2] ?? 0,
    ] as const;
    if (predicate(rgb)) count += 1;
  }
  return count;
}

/** Pixels clearly brighter than the background, i.e. covered by drawn geometry. */
export function litPixels(canvas: Locator): Promise<number> {
  return countPixels(canvas, ([red, green, blue]) => red + green + blue > 3 * 96);
}

/**
 * Pixels that `matches`, in a small rectangle of world space, `half.width` ×
 * `half.height` around a world point, in a view that has not been panned or
 * zoomed.
 */
export async function pixelsAround(
  page: Page,
  canvas: Locator,
  at: { x: number; y: number },
  half: { width: number; height: number },
  matches: (pixel: [red: number, green: number, blue: number]) => boolean,
): Promise<number> {
  const center = await centerOf(canvas);
  const image = PNG.sync.read(
    await page.screenshot({
      clip: {
        x: Math.round(center.x + at.x) - half.width,
        y: Math.round(center.y - at.y) - half.height,
        width: 2 * half.width,
        height: 2 * half.height,
      },
    }),
  );
  let count = 0;
  for (let offset = 0; offset < image.data.length; offset += 4) {
    const pixel: [number, number, number] = [
      image.data[offset] ?? 0,
      image.data[offset + 1] ?? 0,
      image.data[offset + 2] ?? 0,
    ];
    if (matches(pixel)) count += 1;
  }
  return count;
}

/** Lit pixels — clearly brighter than the background — around a world point. */
export function litPixelsAround(
  page: Page,
  canvas: Locator,
  at: { x: number; y: number },
  half: { width: number; height: number },
): Promise<number> {
  return pixelsAround(page, canvas, at, half, ([red, green, blue]) => red + green + blue > 3 * 96);
}

/** Pixels in the crossing-selection colour (green), including antialiased ones. */
export function crossingPixels(canvas: Locator): Promise<number> {
  return countPixels(
    canvas,
    ([red, green, blue]) => green > 80 && green > red * 1.8 && green > blue * 1.5,
  );
}

/** Pixels in the selection accent colour (blue, not white). */
export function highlightPixels(canvas: Locator): Promise<number> {
  return countPixels(
    canvas,
    ([red, green, blue]) => blue > 180 && red < 140 && blue - red > 80 && green < 200,
  );
}

/** Pixels in the snap-marker colour (amber), including antialiased ones. */
export function markerPixels(canvas: Locator): Promise<number> {
  return countPixels(
    canvas,
    ([red, green, blue]) =>
      red > 150 && green > red * 0.5 && green < red * 0.85 && blue < red * 0.5,
  );
}

/**
 * Pixels in the warning red shared by the hot grip and by geometry an edit
 * would remove, including antialiased ones.
 */
export function hotGripPixels(canvas: Locator): Promise<number> {
  return countPixels(
    canvas,
    ([red, green, blue]) => red > 150 && green < red * 0.6 && blue < red * 0.6,
  );
}

/** Pixels in the faint colours of the grid: darker than any geometry, lighter than the background. */
export function gridPixels(canvas: Locator): Promise<number> {
  return countPixels(canvas, ([red, , blue]) => blue >= 28 && blue <= 70 && red <= 40);
}

/** Pixels in the polar-tracking colour (teal), including antialiased ones. */
export function trackingPixels(canvas: Locator): Promise<number> {
  return countPixels(
    canvas,
    ([red, green, blue]) => green > 60 && blue > 55 && red < green * 0.6 && blue > green * 0.7,
  );
}

/** Moves the pointer to a world position, in a view that has not been panned or zoomed. */
export async function hoverWorld(page: Page, canvas: Locator, x: number, y: number): Promise<void> {
  const center = await centerOf(canvas);
  await page.mouse.move(center.x + x, center.y - y);
  await settle(page);
}

/** Clicks a world position, in a view that has not been panned or zoomed. */
export async function clickWorld(page: Page, canvas: Locator, x: number, y: number): Promise<void> {
  const center = await centerOf(canvas);
  await page.mouse.click(center.x + x, center.y - y);
  await settle(page);
}

/** Absolute path of a DXF fixture under `test-data/dxf/`. */
export function fixturePath(relative: string): string {
  return fileURLToPath(new URL(`../../test-data/dxf/${relative}`, import.meta.url));
}

/** Runs `action`, which must trigger a download, and returns the downloaded text and file name. */
export async function downloadFrom(
  page: Page,
  action: () => Promise<void>,
): Promise<{ name: string; text: string }> {
  const pending = page.waitForEvent("download");
  await action();
  const download = await pending;
  return {
    name: download.suggestedFilename(),
    text: await readFile(await download.path(), "utf8"),
  };
}

/** Runs `action`, which must open a file picker, and chooses `file` in it. */
export async function chooseFile(
  page: Page,
  action: () => Promise<void>,
  file: string | { name: string; mimeType: string; buffer: Buffer },
): Promise<void> {
  const pending = page.waitForEvent("filechooser");
  await action();
  await (await pending).setFiles(file);
  await settle(page);
}

/** Records every request the page makes from now on. */
export function recordRequests(page: Page): { url: string; method: string }[] {
  const requests: { url: string; method: string }[] = [];
  page.on("request", (request) => requests.push({ url: request.url(), method: request.method() }));
  return requests;
}

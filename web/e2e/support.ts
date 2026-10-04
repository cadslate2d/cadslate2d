import { expect, type Locator, type Page } from "@playwright/test";

export interface RendererStats {
  frames: number;
  drawCalls: number;
  sceneUploads: number;
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

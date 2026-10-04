import { expect, test } from "@playwright/test";
import { centerOf, openApp, rendererStats, settle } from "./support";

test.describe("render pipeline", () => {
  test("uploads the committed scene once and draws", async ({ page }) => {
    await openApp(page);
    const stats = await rendererStats(page);
    expect(stats.sceneUploads).toBe(1);
    expect(stats.frames).toBeGreaterThan(0);
  });

  // DESIGN §26.3 criterion 5: pointer activity must not re-upload committed geometry.
  test("pan, zoom, and pointer movement never re-upload committed geometry", async ({ page }) => {
    const canvas = await openApp(page);
    const before = await rendererStats(page);
    const { x: cx, y: cy } = await centerOf(canvas);

    await page.mouse.move(cx, cy);
    for (let step = 0; step < 5; step += 1) {
      await page.mouse.wheel(0, -120);
      await settle(page);
    }
    await page.mouse.down({ button: "middle" });
    await page.mouse.move(cx + 120, cy + 60, { steps: 8 });
    await page.mouse.up({ button: "middle" });
    await page.mouse.move(cx - 80, cy - 40, { steps: 8 });
    await settle(page);

    const after = await rendererStats(page);
    expect(after.frames).toBeGreaterThan(before.frames);
    expect(after.sceneUploads).toBe(before.sceneUploads);
    expect(after.bufferReallocations).toBe(before.bufferReallocations);
  });

  test("an idle app draws no frames", async ({ page }) => {
    await openApp(page);
    await settle(page);
    const before = await rendererStats(page);
    await page.waitForTimeout(400);
    expect((await rendererStats(page)).frames).toBe(before.frames);
  });

  test("the canvas backing store follows its size and the device pixel ratio", async ({ page }) => {
    const canvas = await openApp(page);
    const measure = () =>
      canvas.evaluate((element: HTMLCanvasElement) => ({
        width: element.width,
        expected: Math.round(element.getBoundingClientRect().width * window.devicePixelRatio),
      }));

    const initial = await measure();
    expect(initial.width).toBe(initial.expected);

    await page.setViewportSize({ width: 900, height: 500 });
    await settle(page);
    const resized = await measure();
    expect(resized.width).toBe(resized.expected);
    expect(resized.width).not.toBe(initial.width);
  });

  test("recovers from a lost WebGL context by re-uploading the scene", async ({ page }) => {
    const canvas = await openApp(page);
    const before = await rendererStats(page);

    await canvas.evaluate(async (element: HTMLCanvasElement) => {
      const lose = element.getContext("webgl2")?.getExtension("WEBGL_lose_context");
      if (!lose) throw new Error("WEBGL_lose_context is unavailable");
      const restored = new Promise((resolve) =>
        element.addEventListener("webglcontextrestored", resolve, { once: true }),
      );
      lose.loseContext();
      await new Promise((resolve) => setTimeout(resolve, 50));
      lose.restoreContext();
      await restored;
    });
    await settle(page);

    const after = await rendererStats(page);
    expect(after.sceneUploads).toBe(before.sceneUploads + 1);
    expect(after.frames).toBeGreaterThan(before.frames);
  });
});

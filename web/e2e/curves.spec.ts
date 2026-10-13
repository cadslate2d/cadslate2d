import { expect, test } from "@playwright/test";
import {
  arcs,
  centerOf,
  chooseFile,
  clickWorld,
  downloadFrom,
  enter,
  entitiesSection,
  highlightPixels,
  litPixels,
  logLines,
  openApp,
  polylines,
  prompt,
  rendererStats,
  settle,
} from "./support";

test.describe("PLINE arc segments", () => {
  // The issue's criterion: the result exports and re-imports unchanged.
  test("Arc and Line draw a slot that exports with bulges and reopens identically", async ({
    page,
  }) => {
    const canvas = await openApp(page);
    await page.mouse.move(0, 0);
    await enter(page, "PL", "0,0", "100,0", "A");
    await expect(prompt(page)).toHaveText("Specify endpoint of arc or [Line/Undo]:");
    await enter(page, "100,40", "L");
    await expect(prompt(page)).toHaveText("Specify next point or [Arc/Close/Undo]:");
    await enter(page, "0,40", "A", "C");
    await expect(prompt(page)).toHaveText("Command:");

    // The right end bulges 20 beyond its vertices. A click out there, on
    // the arc, selects the polyline: the arc is drawn and can be picked.
    await clickWorld(page, canvas, 120, 20);
    expect(await highlightPixels(canvas)).toBeGreaterThan(300);
    await page.keyboard.press("Escape");

    const first = await downloadFrom(page, () => enter(page, "DXFOUT"));
    expect(first.text.match(/^LWPOLYLINE$/gm)).toHaveLength(1);
    expect(polylines(first.text)[0]?.vertices).toHaveLength(4);
    // Two semicircular ends: bulge 1 on the vertices that start them.
    const bulges = [...entitiesSection(first.text).matchAll(/^ 42\n(.+)$/gm)].map((match) =>
      Number(match[1]),
    );
    expect(bulges).toHaveLength(2);
    for (const bulge of bulges) expect(bulge).toBeCloseTo(1, 12);

    await chooseFile(page, () => enter(page, "OPEN"), {
      name: "slot.dxf",
      mimeType: "application/dxf",
      buffer: Buffer.from(first.text),
    });
    await expect(logLines(page).last()).toHaveText("Imported 1 polyline.");
    const second = await downloadFrom(page, () => enter(page, "DXFOUT"));
    expect(second.text).toBe(first.text);
  });
});

test.describe("CIRCLE", () => {
  test("draws a circle from centre and radius", async ({ page }) => {
    const canvas = await openApp(page);
    await page.mouse.move(0, 0);
    const empty = await litPixels(canvas);

    await enter(page, "C");
    await expect(prompt(page)).toHaveText("Specify center point for circle or [3P/2P]:");
    await enter(page, "0,0");
    await expect(prompt(page)).toHaveText("Specify radius of circle or [Diameter]:");
    await enter(page, "100");

    await expect(prompt(page)).toHaveText("Command:");
    // Circumference 2π·100 ≈ 628 px, a little over a pixel wide.
    const lit = (await litPixels(canvas)) - empty;
    expect(lit).toBeGreaterThan(500);
    expect(lit).toBeLessThan(2500);
  });

  test("is hollow: the stroke follows the curve, not the disc", async ({ page }) => {
    const canvas = await openApp(page);
    await enter(page, "C", "0,0", "100");
    await page.mouse.move(0, 0);
    await settle(page);
    // A filled disc of radius 100 would light about 31,000 pixels.
    expect(await litPixels(canvas)).toBeLessThan(3000);
  });

  test("the radius can be clicked, with a live preview that never touches committed buffers", async ({
    page,
  }) => {
    const canvas = await openApp(page);
    const { x, y } = await centerOf(canvas);
    await enter(page, "C");
    await page.mouse.click(x, y);
    const before = await rendererStats(page);

    for (let step = 1; step <= 10; step += 1) {
      await page.mouse.move(x + step * 12, y - step * 5);
      await settle(page);
    }
    const during = await rendererStats(page);
    expect(during.overlayUploads).toBeGreaterThan(before.overlayUploads);
    expect(during.sceneUploads).toBe(before.sceneUploads);

    await page.mouse.click(x + 120, y - 50);
    await expect(prompt(page)).toHaveText("Command:");
    expect((await rendererStats(page)).sceneUploads).toBe(before.sceneUploads + 1);
  });

  test("stays a crisp thin stroke when zoomed far in", async ({ page }) => {
    const canvas = await openApp(page);
    const { x, y } = await centerOf(canvas);
    await enter(page, "C", "0,0", "100");
    // Zoom in on the rightmost point of the circle until its radius is about 40,000 pixels.
    await page.mouse.move(x + 100, y);
    for (let step = 0; step < 6; step += 1) {
      await page.mouse.wheel(0, -500);
      await settle(page);
    }
    await page.mouse.move(0, 0);
    await settle(page);

    // A near-vertical line down the canvas: a little over a pixel wide.
    const box = await canvas.boundingBox();
    const lit = await litPixels(canvas);
    expect(lit).toBeGreaterThan((box?.height ?? 0) * 0.8);
    expect(lit).toBeLessThan((box?.height ?? 0) * 4);
  });

  // #112: the zoom sweep that measured ADR-0009's limit, as a test. Each
  // wheel step is e times closer; from a radius of 100 px, seventeen steps
  // pass 10⁹ px. The analytic shader stops at 5×10⁴ px and the engine draws
  // the visible span instead, in the overlay; nothing committed moves.
  test("keeps a crisp stroke up to an on-screen radius past 10⁹ px", async ({ page }) => {
    const canvas = await openApp(page);
    const { x, y } = await centerOf(canvas);
    await enter(page, "C", "0,0", "100");
    const box = await canvas.boundingBox();
    const height = box?.height ?? 0;
    const before = await rendererStats(page);
    await page.mouse.move(x + 100, y);
    for (let step = 1; step <= 17; step += 1) {
      await page.mouse.wheel(0, -500);
      await settle(page);
      if (step % 3 === 0 || step === 17) {
        await page.mouse.move(0, 0);
        await settle(page);
        const lit = await litPixels(canvas);
        const radius = 100 * Math.exp(step);
        expect(lit, `radius ${radius.toExponential(1)} px`).toBeGreaterThan(height * 0.8);
        expect(lit, `radius ${radius.toExponential(1)} px`).toBeLessThan(height * 4);
        await page.mouse.move(x + 100, y);
        await settle(page);
      }
    }
    expect((await rendererStats(page)).sceneUploads).toBe(before.sceneUploads);
  });

  test("is selected by its curve and highlighted", async ({ page }) => {
    const canvas = await openApp(page);
    const { x, y } = await centerOf(canvas);
    await enter(page, "C", "0,0", "100");

    await page.mouse.click(x, y); // the centre is not on the circle
    await settle(page);
    expect(await highlightPixels(canvas)).toBe(0);
    await page.keyboard.press("Escape"); // the click on empty space opened a selection box

    await page.mouse.click(x + 100, y + 2);
    await settle(page);
    expect(await highlightPixels(canvas)).toBeGreaterThan(500);
  });
});

test.describe("ARC", () => {
  test("draws an arc through three points", async ({ page }) => {
    const canvas = await openApp(page);
    await page.mouse.move(0, 0);
    const empty = await litPixels(canvas);

    await enter(page, "A", "100,0");
    await expect(prompt(page)).toHaveText("Specify second point of arc or [Center]:");
    await enter(page, "0,100", "-100,0");
    await expect(prompt(page)).toHaveText("Command:");

    // Half the circumference of the circle above.
    const lit = (await litPixels(canvas)) - empty;
    expect(lit).toBeGreaterThan(250);
    expect(lit).toBeLessThan(1300);
  });

  test("start, centre, included angle", async ({ page }) => {
    await openApp(page);
    await enter(page, "A", "100,0", "C", "0,0", "A");
    await expect(prompt(page)).toHaveText("Specify included angle:");
    await enter(page, "90");
    const file = await downloadFrom(page, () => enter(page, "DXFOUT"));
    expect(arcs(file.text)).toContainEqual([0, 0, 100, 0, 90]);
    expect(file.text).toContain("AcDbArc\n 50\n0.0\n 51\n90.0\n");
  });
});

test.describe("polylines", () => {
  test("RECTANG draws a closed rectangle as one entity", async ({ page }) => {
    const canvas = await openApp(page);
    const { x, y } = await centerOf(canvas);
    await page.mouse.move(0, 0);
    const empty = await litPixels(canvas);

    await enter(page, "REC", "-100,-50", "@200,100");
    await expect(prompt(page)).toHaveText("Command:");
    // Perimeter 600 px.
    expect((await litPixels(canvas)) - empty).toBeGreaterThan(500);

    // One click on any edge selects all four.
    await page.mouse.click(x - 100, y);
    await settle(page);
    expect(await highlightPixels(canvas)).toBeGreaterThan(1000);

    const file = await downloadFrom(page, () => enter(page, "DXFOUT"));
    expect(file.text.match(/^LWPOLYLINE$/gm)).toHaveLength(1);
    expect(polylines(file.text)[0]?.vertices).toHaveLength(4);
    expect(polylines(file.text)[0]?.closed).toBe(true);
  });

  test("PLINE commits one polyline when it ends, and nothing if cancelled", async ({ page }) => {
    const canvas = await openApp(page);
    await page.mouse.move(0, 0);
    const empty = await litPixels(canvas);

    await enter(page, "PL", "-100,0", "0,80", "100,0");
    await expect(prompt(page)).toHaveText("Specify next point or [Arc/Close/Undo]:");
    await page.keyboard.press("Escape");
    await settle(page);
    expect(await litPixels(canvas)).toBe(empty);

    await enter(page, "PL", "-100,0", "0,80", "100,0", "C");
    await expect(prompt(page)).toHaveText("Command:");
    expect(await litPixels(canvas)).toBeGreaterThan(empty + 400);

    await page.keyboard.press("Control+z");
    await settle(page);
    expect(await litPixels(canvas)).toBe(empty);
  });
});

test("circles and arcs survive export, refresh, and reopen", async ({ page }) => {
  let canvas = await openApp(page);
  await enter(page, "C", "0,0", "80", "A", "120,0", "0,120", "-120,0", "L", "-50,-50", "50,50", "");
  await page.mouse.move(0, 0);
  await settle(page);
  const drawn = await litPixels(canvas);
  const exported = await downloadFrom(page, () => enter(page, "DXFOUT"));
  expect(exported.text).toContain("CIRCLE");

  await page.reload();
  canvas = await openApp(page);
  await chooseFile(page, () => enter(page, "OPEN"), {
    name: exported.name,
    mimeType: "application/dxf",
    buffer: Buffer.from(exported.text),
  });
  await expect(logLines(page).last()).toHaveText("Imported 1 line, 1 circle, 1 arc.");
  await page.mouse.move(0, 0);
  await settle(page);
  // Opening frames the drawing, so the image is the same shapes at a different zoom.
  expect(await litPixels(canvas)).toBeGreaterThan(drawn * 0.5);
});

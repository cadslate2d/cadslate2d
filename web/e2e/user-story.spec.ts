// The M0 user story (DESIGN §26.1), end to end against the production bundle:
//
//   open → type L → 0,0 → @1000<45 → Enter → line appears → select it →
//   undo → redo → export DXF → refresh → reopen DXF → same line appears
import { expect, test } from "@playwright/test";
import {
  centerOf,
  chooseFile,
  downloadFrom,
  enter,
  highlightPixels,
  litPixels,
  logLines,
  openApp,
  prompt,
  recordRequests,
  rendererStats,
  settle,
} from "./support";

test("the M0 user story", async ({ page }) => {
  const requests = recordRequests(page);
  let canvas = await openApp(page);
  await page.mouse.move(0, 0);
  const empty = await litPixels(canvas);

  // type L → 0,0 → @1000<45 → Enter
  await enter(page, "L");
  await expect(prompt(page)).toHaveText("Specify first point:");
  await enter(page, "0,0", "@1000<45", "");
  await expect(prompt(page)).toHaveText("Command:");

  // line appears
  const drawn = await litPixels(canvas);
  expect(drawn).toBeGreaterThan(empty + 200);

  // select it: the line leaves the origin (canvas centre) at 45°
  const { x, y } = await centerOf(canvas);
  await page.mouse.click(x + 100, y - 100);
  await settle(page);
  expect(await highlightPixels(canvas)).toBeGreaterThan(200);
  await page.keyboard.press("Escape");
  await page.mouse.move(0, 0);
  await settle(page);

  // undo → redo
  await page.keyboard.press("Control+z");
  await settle(page);
  expect(await litPixels(canvas)).toBe(empty);
  await page.keyboard.press("Control+y");
  await settle(page);
  expect(await litPixels(canvas)).toBe(drawn);

  // export DXF
  const exported = await downloadFrom(page, () => enter(page, "DXFOUT"));
  expect(exported.text).toContain(" 11\n707.1067811865476\n 21\n707.106781186547");

  // refresh: nothing survives in the page, and nothing is fetched from a backend
  await page.reload();
  canvas = await openApp(page);
  await page.mouse.move(0, 0);
  expect(await litPixels(canvas)).toBe(empty);

  // reopen DXF → same line appears
  await chooseFile(page, () => enter(page, "OPEN"), {
    name: exported.name,
    mimeType: "application/dxf",
    buffer: Buffer.from(exported.text),
  });
  await expect(logLines(page).last()).toHaveText("Imported 1 line.");
  expect(await litPixels(canvas)).toBeGreaterThan(empty + 200);

  // "Same line" is checked exactly: exporting again yields the identical file.
  const again = await downloadFrom(page, () => enter(page, "DXFOUT"));
  expect(again.text).toBe(exported.text);

  // DESIGN §26.3 criterion 18: every request was a GET for a same-origin static file.
  const origin = new URL(page.url()).origin;
  for (const request of requests.filter((request) => !request.url.startsWith("blob:"))) {
    expect(request.method, request.url).toBe("GET");
    expect(new URL(request.url).origin, request.url).toBe(origin);
    expect(request.url, request.url).toMatch(/\/(\?debug)?$|\.(js|css|wasm|html)$/);
  }
});

test("the same story driven by the pointer", async ({ page }) => {
  const canvas = await openApp(page);
  const { x, y } = await centerOf(canvas);
  const before = await rendererStats(page);

  await enter(page, "L");
  await page.mouse.click(x - 150, y + 40);
  await page.mouse.move(x, y - 20, { steps: 5 });
  await page.mouse.click(x + 150, y - 60);
  await page.keyboard.press("Enter");
  await expect(prompt(page)).toHaveText("Command:");

  // One committed upload for the one new line; the rubber band never touched it.
  expect((await rendererStats(page)).sceneUploads).toBe(before.sceneUploads + 1);

  // Click the middle of the line to select it.
  await page.mouse.click(x, y - 10);
  await settle(page);
  expect(await highlightPixels(canvas)).toBeGreaterThan(200);

  const exported = await downloadFrom(page, () => enter(page, "DXFOUT"));
  expect(exported.text.match(/^LINE$/gm)).toHaveLength(1);
});

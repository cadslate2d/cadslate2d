// The M3 exit scenario (DESIGN §31): the modify commands on geometry that
// is not just lines.
//
// A slotted link is drafted: a slot drawn as one polyline with arc ends,
// OFFSET outwards into a second one, two construction lines TRIMmed against
// both at once into four webs, the outer slot opened between them — a
// polyline as the object being trimmed — and JOINed with two of the webs
// into one polyline. STRETCH then lengthens the whole link, arcs and all.
// Every step is checked through the exported file; then everything is
// undone, redone, exported, and reopened.
import { expect, test, type Page } from "@playwright/test";
import {
  chooseFile,
  clickWorld,
  downloadFrom,
  enter,
  hoverWorld,
  lines,
  logLines,
  openApp,
  polylines,
  prompt,
  rendererStats,
} from "./support";
import type { Polyline } from "./support";

const exported = async (page: Page) => (await downloadFrom(page, () => enter(page, "DXFOUT"))).text;
const count = (text: string, record: string) =>
  (text.match(new RegExp(`^${record}$`, "gm")) ?? []).length;

/** The polyline has exactly these vertices and bulges, in this order. */
function expectPolyline(
  actual: Polyline | undefined,
  closed: boolean,
  vertices: [number, number, number][],
): void {
  expect(actual?.closed, "closed").toBe(closed);
  expect(actual?.vertices.length, JSON.stringify(actual?.vertices)).toBe(vertices.length);
  vertices.forEach((want, index) => {
    const got = actual?.vertices[index] ?? [NaN, NaN, NaN];
    for (let part = 0; part < 3; part += 1) {
      expect(got[part], `vertex ${index}: ${JSON.stringify(got)}`).toBeCloseTo(
        want[part] ?? NaN,
        9,
      );
    }
  });
}

test("the M3 drafting scenario", async ({ page }) => {
  // A long script with many downloads: allow for a loaded machine.
  test.slow();
  const canvas = await openApp(page);

  // ── a slot as one polyline: two straight sides and two half-circle ends ──
  await enter(page, "PL", "0,0", "100,0", "A", "100,40", "L", "0,40", "A", "C");
  await expect(prompt(page)).toHaveText("Command:");
  const slot: [number, number, number][] = [
    [0, 0, 0],
    [100, 0, 1],
    [100, 40, 0],
    [0, 40, 1],
  ];
  let text = await exported(page);
  expect(count(text, "LWPOLYLINE")).toBe(1);
  expectPolyline(polylines(text)[0], true, slot);

  // ── OFFSET the polyline outwards: a larger slot, arcs offset as arcs ──
  await enter(page, "O", "10");
  await hoverWorld(page, canvas, 50, 0);
  await clickWorld(page, canvas, 50, 0);
  await expect(prompt(page)).toHaveText("Specify point on side to offset:");
  await clickWorld(page, canvas, 50, -30);
  await enter(page, "");
  text = await exported(page);
  expect(count(text, "LWPOLYLINE")).toBe(2);
  expectPolyline(polylines(text)[0], true, slot);
  expectPolyline(polylines(text)[1], true, [
    [0, -10, 0],
    [100, -10, 1],
    [100, 50, 0],
    [0, 50, 1],
  ]);

  // ── two construction lines across both slots ──
  await enter(page, "L", "30,-30", "30,70", "", "L", "70,-30", "70,70", "");

  // ── TRIM against every edge at once: each line is cut by both polylines ──
  await enter(page, "TR");
  await expect(prompt(page)).toHaveText("Select cutting edges <all>:");
  await enter(page, "");
  await expect(logLines(page).last()).toHaveText("Every object is an edge.");
  // Of each line: the end above the link, the end below, and the middle
  // inside the inner slot. What is left is a web between the two slots.
  for (const x of [30, 70]) {
    for (const y of [65, -25, 20]) await clickWorld(page, canvas, x, y);
  }
  // A polyline as the object trimmed: the top of the outer slot, between
  // the two webs that now end on it. The closed polyline opens.
  await hoverWorld(page, canvas, 50, 50);
  await clickWorld(page, canvas, 50, 50);
  await enter(page, "");
  await expect(prompt(page)).toHaveText("Command:");
  text = await exported(page);
  expect(lines(text)).toEqual([
    [30, -10, 30, 0],
    [70, -10, 70, 0],
    [30, 40, 30, 50],
    [70, 40, 70, 50],
  ]);
  expectPolyline(polylines(text)[0], true, slot);
  const opened: [number, number, number][] = [
    [30, 50, 0],
    [0, 50, 1],
    [0, -10, 0],
    [100, -10, 1],
    [100, 50, 0],
    [70, 50, 0],
  ];
  expectPolyline(polylines(text)[1], false, opened);

  // ── JOIN the opened slot and the two webs at its ends into one polyline ──
  await enter(page, "J");
  await clickWorld(page, canvas, 30, 45);
  await clickWorld(page, canvas, 70, 45);
  await clickWorld(page, canvas, 50, -10);
  await enter(page, "");
  await expect(logLines(page).last()).toHaveText("3 objects joined into 1 polyline.");
  text = await exported(page);
  expect(count(text, "LWPOLYLINE")).toBe(2);
  expect(lines(text)).toEqual([
    [30, -10, 30, 0],
    [70, -10, 70, 0],
  ]);
  expectPolyline(polylines(text)[1], false, [[30, 40, 0], ...opened, [70, 40, 0]]);

  // ── STRETCH the right-hand end 20 further out: arcs follow their ends ──
  await enter(page, "S", "85,-40", "150,80");
  await expect(logLines(page).last()).toHaveText("2 found");
  await enter(page, "0,0");
  const before = await rendererStats(page);
  for (let step = 1; step <= 5; step += 1) await hoverWorld(page, canvas, step * 4, 0);
  expect(
    (await rendererStats(page)).sceneUploads,
    "the preview uploads no committed geometry",
  ).toBe(before.sceneUploads);
  await enter(page, "20,0");
  await expect(prompt(page)).toHaveText("Command:");

  const finished = await downloadFrom(page, () => enter(page, "DXFOUT"));
  const [inner, outer] = polylines(finished.text);
  expectPolyline(inner, true, [
    [0, 0, 0],
    [120, 0, 1],
    [120, 40, 0],
    [0, 40, 1],
  ]);
  expectPolyline(outer, false, [
    [30, 40, 0],
    [30, 50, 0],
    [0, 50, 1],
    [0, -10, 0],
    [120, -10, 1],
    [120, 50, 0],
    [70, 50, 0],
    [70, 40, 0],
  ]);
  expect(count(finished.text, "LWPOLYLINE")).toBe(2);
  expect(count(finished.text, "LINE")).toBe(2);
  await expect(logLines(page).filter({ hasText: "Unknown command" })).toHaveCount(0);

  // ── seven commands, seven undo steps; redo brings it all back ──
  for (let step = 0; step < 7; step += 1) await page.keyboard.press("Control+z");
  text = await exported(page);
  expect(count(text, "LWPOLYLINE") + count(text, "LINE")).toBe(0);
  await page.keyboard.press("Control+z");
  await expect(logLines(page).last()).toHaveText("Nothing to undo.");

  for (let step = 0; step < 7; step += 1) await page.keyboard.press("Control+y");
  expect(await exported(page)).toBe(finished.text);

  // ── export, refresh, reopen: the same drawing, bulges and all ──
  await page.reload();
  await openApp(page);
  await chooseFile(page, () => enter(page, "OPEN"), {
    name: finished.name,
    mimeType: "application/dxf",
    buffer: Buffer.from(finished.text),
  });
  await expect(logLines(page).last()).toHaveText("Imported 2 lines, 2 polylines.");
  expect(await exported(page)).toBe(finished.text);
});

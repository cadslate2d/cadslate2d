// The M1 exit scenario (DESIGN §29): "A user can create and edit a basic
// precise drawing primarily through keyboard and pointer interaction."
//
// A small flange is drawn from the keyboard with every M1 draw command,
// edited with every M1 modify command, undone and redone, exported,
// and reopened. Geometry is checked exactly, through the exported file.
import { expect, test } from "@playwright/test";
import {
  centerOf,
  chooseFile,
  circles,
  downloadFrom,
  enter,
  highlightPixels,
  lines,
  litPixels,
  logLines,
  openApp,
  prompt,
  settle,
} from "./support";

const count = (text: string, record: string) =>
  (text.match(new RegExp(`^${record}$`, "gm")) ?? []).length;

test("the M1 drafting scenario", async ({ page }) => {
  // A long script with many downloads: allow for a loaded machine.
  test.slow();
  let canvas = await openApp(page);
  const { x, y } = await centerOf(canvas);

  // ── draw: RECTANG, CIRCLE, ARC, PLINE, LINE ──
  await enter(page, "REC", "-100,-60", "@200,120"); // the plate
  await enter(page, "C", "0,0", "25"); // the bore
  await enter(page, "C", "-70,30", "D", "16"); // a bolt hole, by diameter
  await enter(page, "A", "-40,-45", "0,-55", "40,-45"); // a slot edge through three points
  await enter(page, "PL", "-100,60", "-60,90", "60,90", "100,60", ""); // a lip, open polyline
  await enter(page, "L", "-25,0", "@50,0", ""); // a centre line by relative coordinates
  await expect(prompt(page)).toHaveText("Command:");

  let file = await downloadFrom(page, () => enter(page, "DXFOUT"));
  expect(count(file.text, "LWPOLYLINE")).toBe(2);
  expect(count(file.text, "CIRCLE")).toBe(2);
  expect(count(file.text, "ARC")).toBe(1);
  expect(count(file.text, "LINE")).toBe(1);
  expect(circles(file.text)).toContainEqual([-70, 30, 8]);

  // ── edit: COPY the bolt hole to the other three corners ──
  await page.mouse.click(x - 70 + 8, y - 30); // on the hole's rightmost point
  await settle(page);
  expect(await highlightPixels(canvas)).toBeGreaterThan(50);
  await enter(page, "CO", "-70,30", "70,30", "70,-30", "-70,-30", "");
  file = await downloadFrom(page, () => enter(page, "DXFOUT"));
  expect(count(file.text, "CIRCLE")).toBe(5);
  for (const [x, y] of [
    [70, 30],
    [70, -30],
    [-70, -30],
  ]) {
    expect(circles(file.text)).toContainEqual([x, y, 8]);
  }

  // ── edit: MOVE the centre line up by an exact distance ──
  await page.mouse.click(x + 10, y); // the centre line (the bore's curve is 25 away)
  await enter(page, "M", "0,0", "@0,10");
  file = await downloadFrom(page, () => enter(page, "DXFOUT"));
  expect(lines(file.text)).toContainEqual([-25, 10, 25, 10]);

  // ── edit: ERASE the arc with a crossing box, verb first ──
  await enter(page, "E");
  await expect(prompt(page)).toHaveText("Select objects:");
  // From empty space right of the arc, leftwards across it: a crossing box
  // that stays clear of the plate's bottom edge just below.
  await page.mouse.click(x + 35, y + 40);
  await page.mouse.click(x + 25, y + 53);
  await expect(logLines(page).last()).toHaveText("1 found, 1 total");
  await enter(page, "");
  file = await downloadFrom(page, () => enter(page, "DXFOUT"));
  expect(count(file.text, "ARC")).toBe(0);

  // ── undo and redo walk whole commands ──
  await page.keyboard.press("Control+z"); // the erase
  await page.keyboard.press("Control+z"); // the move
  await page.keyboard.press("Control+z"); // the copy: all three copies at once
  await page.keyboard.press("Escape");
  file = await downloadFrom(page, () => enter(page, "DXFOUT"));
  expect(count(file.text, "CIRCLE")).toBe(2);
  expect(count(file.text, "ARC")).toBe(1);
  expect(lines(file.text)).toContainEqual([-25, 0, 25, 0]);

  for (let step = 0; step < 3; step += 1) await page.keyboard.press("Control+y");
  const finished = await downloadFrom(page, () => enter(page, "DXFOUT"));
  expect(count(finished.text, "CIRCLE")).toBe(5);
  expect(count(finished.text, "ARC")).toBe(0);

  // ── export, refresh, reopen: the same drawing ──
  await page.mouse.move(0, 0);
  await settle(page);
  const drawn = await litPixels(canvas);
  await page.reload();
  canvas = await openApp(page);
  await chooseFile(page, () => enter(page, "OPEN"), {
    name: finished.name,
    mimeType: "application/dxf",
    buffer: Buffer.from(finished.text),
  });
  await expect(logLines(page).last()).toHaveText("Imported 1 line, 5 circles, 2 polylines.");
  await page.mouse.move(0, 0);
  await settle(page);
  expect(await litPixels(canvas)).toBeGreaterThan(drawn * 0.5);

  const reopened = await downloadFrom(page, () => enter(page, "DXFOUT"));
  expect(reopened.text).toBe(finished.text);
});

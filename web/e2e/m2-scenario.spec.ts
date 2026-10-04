// The M2 exit scenario (DESIGN §30): "Ordinary drafting begins to feel
// recognizably like classic CAD."
//
// A small plate is drafted the way a drafter would: four loose edges are
// joined with FILLET and CHAMFER, an edge is stretched by its grip and
// another extended to it, a hole is placed by object snap and cut with
// TRIM, then MIRROR, OFFSET, SCALE, ROTATE, and an ORTHO line. Every step
// is checked exactly, through the exported file; then everything is undone,
// redone, exported, and reopened.
import { expect, test, type Page } from "@playwright/test";
import {
  chooseFile,
  clickWorld,
  downloadFrom,
  enter,
  hoverWorld,
  logLines,
  openApp,
  prompt,
} from "./support";

const exported = async (page: Page) => (await downloadFrom(page, () => enter(page, "DXFOUT"))).text;
const count = (text: string, record: string) =>
  (text.match(new RegExp(`^${record}$`, "gm")) ?? []).length;

/** A number as the DXF writer prints it. */
const number = (value: number) => (Number.isInteger(value) ? value.toFixed(1) : String(value));
const line = (x0: number, y0: number, x1: number, y1: number) =>
  `LINE\n  8\n0\n 10\n${number(x0)}\n 20\n${number(y0)}\n 11\n${number(x1)}\n 21\n${number(y1)}\n`;

/** Every ARC in the file as [centre x, centre y, radius, start°, end°]. */
function arcs(text: string): number[][] {
  const pattern = /^ARC\n {2}8\n0\n 10\n(.+)\n 20\n(.+)\n 40\n(.+)\n 50\n(.+)\n 51\n(.+)\n/gm;
  return [...text.matchAll(pattern)].map((match) => match.slice(1).map(Number));
}

function expectArc(text: string, expected: number[]): void {
  const matches = (arc: number[]) =>
    arc.every((value, index) => {
      const want = expected[index] ?? NaN;
      // Angles are compared on the circle: 360° is 0°.
      const difference =
        index >= 3 ? Math.abs(((value - want + 540) % 360) - 180) : Math.abs(value - want);
      return difference < 1e-6;
    });
  expect(
    arcs(text).some(matches),
    `an arc ${JSON.stringify(expected)} in ${JSON.stringify(arcs(text))}`,
  ).toBe(true);
}

test("the M2 drafting scenario", async ({ page }) => {
  const canvas = await openApp(page);

  // ── four edges of a plate, deliberately not meeting ──
  await enter(page, "L", "20,0", "100,0", ""); // bottom
  await enter(page, "L", "120,20", "120,60", ""); // right
  await enter(page, "L", "100,80", "20,80", ""); // top
  await enter(page, "L", "0,60", "0,20", ""); // left

  // ── FILLET with a radius: bottom and right ──
  await enter(page, "F", "R", "10");
  await clickWorld(page, canvas, 60, 0);
  await clickWorld(page, canvas, 120, 40);
  await expect(prompt(page)).toHaveText("Command:");
  let text = await exported(page);
  expect(text).toContain(line(20, 0, 110, 0));
  expect(text).toContain(line(120, 10, 120, 60));
  expectArc(text, [110, 10, 10, 270, 0]);

  // ── CHAMFER: right and top ──
  await enter(page, "CHA", "D", "15", "");
  await clickWorld(page, canvas, 120, 40);
  await clickWorld(page, canvas, 60, 80);
  text = await exported(page);
  expect(text).toContain(line(120, 10, 120, 65));
  expect(text).toContain(line(105, 80, 20, 80));
  expect(text).toContain(line(120, 65, 105, 80));

  // ── FILLET with radius zero: a sharp corner between top and left ──
  await enter(page, "F", "R", "0");
  await clickWorld(page, canvas, 60, 80);
  await clickWorld(page, canvas, 0, 40);
  text = await exported(page);
  expect(text).toContain(line(105, 80, 0, 80));
  expect(text).toContain(line(0, 80, 0, 20));

  // ── a grip: stretch the left edge down to the corner, by typed coordinates ──
  await clickWorld(page, canvas, 0, 65); // select it, away from its grips
  await clickWorld(page, canvas, 0, 20); // take the lower endpoint grip
  await expect(prompt(page)).toHaveText("Specify stretch point:");
  await enter(page, "0,0");
  await page.keyboard.press("Escape"); // clear the selection
  text = await exported(page);
  expect(text).toContain(line(0, 80, 0, 0));

  // ── EXTEND the bottom edge to the left edge ──
  await enter(page, "EX");
  await clickWorld(page, canvas, 0, 60);
  await clickWorld(page, canvas, 25, 0);
  await enter(page, "");
  text = await exported(page);
  expect(text).toContain(line(0, 0, 110, 0));

  // ── object snap: a hole centred exactly on the midpoint of the left edge ──
  await enter(page, "C");
  await hoverWorld(page, canvas, 3, 42);
  await expect(page.getByTestId("snap")).toHaveText("Midpoint");
  await expect(page.getByTestId("coordinates")).toHaveText("0.0000, 40.0000");
  await clickWorld(page, canvas, 3, 42);
  await enter(page, "12");
  text = await exported(page);
  expect(text).toContain("CIRCLE\n  8\n0\n 10\n0.0\n 20\n40.0\n 40\n12.0\n");

  // ── TRIM: the half of the hole outside the plate ──
  await enter(page, "TR");
  await clickWorld(page, canvas, 0, 70); // the left edge is the cutting edge
  await clickWorld(page, canvas, -12, 40);
  await enter(page, "");
  text = await exported(page);
  expect(count(text, "CIRCLE")).toBe(0);
  expectArc(text, [0, 40, 12, 270, 90]);

  // ── MIRROR the notch to the other side, keeping the original ──
  await clickWorld(page, canvas, 12, 40);
  await enter(page, "MI", "60,0", "60,10", "");
  text = await exported(page);
  expectArc(text, [0, 40, 12, 270, 90]);
  expectArc(text, [120, 40, 12, 90, 270]);

  // ── OFFSET the bottom edge inwards ──
  await enter(page, "O", "25");
  await clickWorld(page, canvas, 60, 0);
  await clickWorld(page, canvas, 60, 45);
  await enter(page, "");
  text = await exported(page);
  expect(text).toContain(line(0, 25, 110, 25));

  // ── SCALE, then ROTATE, that line about its start ──
  await clickWorld(page, canvas, 70, 25);
  await enter(page, "SC", "0,25", "0.5");
  text = await exported(page);
  expect(text).toContain(line(0, 25, 55, 25));
  await clickWorld(page, canvas, 40, 25);
  await enter(page, "RO", "0,25", "180");
  text = await exported(page);
  expect(text).toContain(line(0, 25, -55, 25));

  // ── ORTHO: a line straight down from the end of the bottom edge, found by snap ──
  await page.keyboard.press("F8");
  await enter(page, "L");
  await hoverWorld(page, canvas, 108, 2);
  await expect(page.getByTestId("snap")).toHaveText("Endpoint");
  await clickWorld(page, canvas, 108, 2);
  await hoverWorld(page, canvas, 114, -40);
  await expect(page.getByTestId("tracking")).toHaveText("Ortho: 40.0000 < 270°");
  await clickWorld(page, canvas, 114, -40);
  await enter(page, "");
  await page.keyboard.press("F8");

  const finished = await downloadFrom(page, () => enter(page, "DXFOUT"));
  expect(finished.text).toContain(line(110, 0, 110, -40));
  expect(count(finished.text, "LINE")).toBe(7);
  expect(count(finished.text, "ARC")).toBe(3);
  expect(count(finished.text, "CIRCLE")).toBe(0);
  await expect(logLines(page).filter({ hasText: "Unknown command" })).toHaveCount(0);

  // ── sixteen commands, sixteen undo steps; redo brings it all back ──
  for (let step = 0; step < 16; step += 1) await page.keyboard.press("Control+z");
  text = await exported(page);
  expect(count(text, "LINE") + count(text, "ARC") + count(text, "CIRCLE")).toBe(0);
  await page.keyboard.press("Control+z");
  await expect(logLines(page).last()).toHaveText("Nothing to undo.");

  for (let step = 0; step < 16; step += 1) await page.keyboard.press("Control+y");
  expect(await exported(page)).toBe(finished.text);

  // ── export, refresh, reopen: the same drawing ──
  await page.reload();
  await openApp(page);
  await chooseFile(page, () => enter(page, "OPEN"), {
    name: finished.name,
    mimeType: "application/dxf",
    buffer: Buffer.from(finished.text),
  });
  await expect(logLines(page).last()).toHaveText("Imported 7 lines, 3 arcs.");
  expect(await exported(page)).toBe(finished.text);
});

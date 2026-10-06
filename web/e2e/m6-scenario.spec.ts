// The M6 exit scenario (DESIGN §34): a part drawn, dimensioned every way,
// hatched, saved, and reopened the same; a style change redraws every
// dimension in one step.
import { expect, test } from "@playwright/test";
import {
  chooseFile,
  clickWorld,
  downloadFrom,
  enter,
  entities,
  group,
  litPixelsAround,
  logLines,
  num,
  openApp,
  prompt,
  tableRow,
} from "./support";

test("the M6 annotation scenario", async ({ page }) => {
  test.slow();
  const canvas = await openApp(page);
  await page.keyboard.press("F7");

  // ── a plate with a rounded end and a hole ──
  await enter(page, "PL", "-120,-50", "60,-50", "A", "60,50", "L", "-120,50", "C");
  await enter(page, "C", "-60,0", "20");
  await expect(prompt(page)).toHaveText("Command:");

  // ── a style of its own, current ──
  await enter(page, "DIMSTYLE", "N", "Part", "H", "5", "P", "1", "");
  await expect(prompt(page)).toHaveText("Command:");

  // ── dimensioned every way ──
  await enter(page, "DIMLINEAR", "-120,-50", "60,-50", "0,-80");
  await enter(page, "DIMLINEAR", "-120,-50", "-120,50", "-150,0");
  await enter(page, "DIMALIGNED", "60,50", "110,0", "110,60");
  await enter(page, "DIMRADIUS");
  await clickWorld(page, canvas, -40, 0);
  await enter(page, "-50,10");
  await enter(page, "DIMDIAMETER");
  await clickWorld(page, canvas, -80, 0);
  await enter(page, "-90,40");
  await enter(page, "DIMANGULAR", "", "60,0", "60,50", "110,0", "100,30");
  await expect(prompt(page)).toHaveText("Command:");

  // ── hatched with ANSI31 between the outline and the hole ──
  await enter(page, "HATCH", "P", "ANSI31", "SC", "2");
  await clickWorld(page, canvas, 0, 0);
  await expect(prompt(page)).toHaveText("Command:");
  await page.mouse.move(5, 5);
  expect(
    await litPixelsAround(page, canvas, { x: 0, y: 0 }, { width: 20, height: 20 }),
  ).toBeGreaterThan(40);

  // ── a style change redraws every dimension, in one step ──
  const textOf = { x: -30, y: -72 };
  const half = { width: 40, height: 10 };
  const before = await litPixelsAround(page, canvas, textOf, half);
  await enter(page, "DIMSTYLE", "H", "9", "");
  await page.mouse.move(5, 5);
  const after = await litPixelsAround(page, canvas, textOf, half);
  expect(after).toBeGreaterThan(before);
  await enter(page, "U");
  await page.mouse.move(5, 5);
  expect(await litPixelsAround(page, canvas, textOf, half)).toBe(before);

  // ── saved whole ──
  const first = await downloadFrom(page, () => enter(page, "DXFOUT"));
  const dimensions = entities(first.text, "DIMENSION");
  expect(dimensions).toHaveLength(6);
  expect(new Set(dimensions.map((record) => group(record, 3)))).toEqual(new Set(["Part"]));
  const values = dimensions.map((record) => Math.round(num(record, 42) * 1000) / 1000);
  expect(values).toEqual([180, 100, 70.711, 20, 40, 90]);
  const part = tableRow(first.text, "DIMSTYLE", "Part");
  expect([group(part, 140), group(part, 271)]).toEqual(["5.0", "1"]);
  expect(first.text).toContain("  9\n$DIMSTYLE\n  2\nPart\n");
  const hatches = entities(first.text, "HATCH");
  expect(hatches).toHaveLength(1);
  expect([group(hatches[0], 2), group(hatches[0], 91), group(hatches[0], 41)]).toEqual([
    "ANSI31",
    "2",
    "2.0",
  ]);

  // ── reopened, and saved again: the same bytes ──
  await chooseFile(page, () => enter(page, "OPEN"), {
    name: first.name,
    mimeType: "application/dxf",
    buffer: Buffer.from(first.text),
  });
  await expect(
    logLines(page).filter({ hasText: "Imported 1 circle, 1 polyline, 6 dimensions, 1 hatch." }),
  ).toHaveCount(1);
  await expect(page.getByTestId("lossy")).toHaveCount(0);
  const again = await downloadFrom(page, () => enter(page, "DXFOUT"));
  expect(again.text).toBe(first.text);
  await expect(logLines(page).filter({ hasText: "Unknown command" })).toHaveCount(0);
});

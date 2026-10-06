// The M4 exit scenario (DESIGN §32): a drawing organised the way drafters
// organise one.
//
// A layer with a colour and a linetype is made current; a chair is drawn
// on it and made into a block, which is inserted twice more — once scaled;
// a label is written on layer 0 and recoloured with CHPROP; the file is
// exported and every table and entity is checked in it; the chair is
// redefined and every reference follows; the layer is switched off and on.
// Then everything is undone and redone byte for byte, and the file is
// reopened after a reload and exports the same bytes.
import { expect, test, type Page } from "@playwright/test";
import {
  blocks,
  chooseFile,
  clickWorld,
  downloadFrom,
  enter,
  entities,
  group,
  hoverWorld,
  lines,
  litPixelsAround,
  logLines,
  num,
  openApp,
  prompt,
  rendererStats,
  tableRow,
} from "./support";

const exported = async (page: Page) => (await downloadFrom(page, () => enter(page, "DXFOUT"))).text;
const count = (text: string, record: string) =>
  (text.match(new RegExp(`^${record}$`, "gm")) ?? []).length;

test("the M4 drawing-organisation scenario", async ({ page }) => {
  // A long script with many downloads: allow for a loaded machine.
  test.slow();
  const canvas = await openApp(page);
  await page.keyboard.press("F7");
  const lit = (x: number, y: number, half = { width: 6, height: 6 }) =>
    litPixelsAround(page, canvas, { x, y }, half);

  // ── a layer with a colour and a linetype, made current ──
  await enter(page, "LINETYPE", "L", "Dashed", "");
  await enter(
    page,
    "LAYER",
    "N",
    "walls",
    "C",
    "4",
    "walls",
    "LT",
    "Dashed",
    "walls",
    "S",
    "walls",
    "",
  );
  await expect(prompt(page)).toHaveText("Command:");

  // ── a chair on it, made into a block ──
  await enter(page, "L", "0,0", "40,0", "", "L", "0,0", "0,40", "");
  await enter(page, "B", "Chair", "0,0");
  await clickWorld(page, canvas, 20, 0);
  await clickWorld(page, canvas, 0, 20);
  await enter(page, "");
  await expect(logLines(page).last()).toHaveText('Block "Chair" defined from 2 objects.');

  // ── INSERT twice: the preview at the pointer uploads no committed geometry ──
  await enter(page, "I", "Chair");
  await expect(prompt(page)).toHaveText("Specify insertion point:");
  const before = await rendererStats(page);
  for (let step = 1; step <= 5; step += 1) await hoverWorld(page, canvas, 100 + step * 4, 0);
  expect(
    (await rendererStats(page)).sceneUploads,
    "the preview uploads no committed geometry",
  ).toBe(before.sceneUploads);
  await enter(page, "100,0", "", "");
  await enter(page, "I", "Chair", "100,100", "2", "0");
  await expect(prompt(page)).toHaveText("Command:");
  await page.mouse.move(5, 5);
  expect(await lit(120, 0)).toBeGreaterThan(5);
  expect(await lit(160, 100)).toBeGreaterThan(5);

  // ── a label on layer 0, recoloured through CHPROP ──
  await enter(page, "LAYER", "S", "0", "");
  await enter(page, "TEXT", "0,-30", "10", "0", "Chairs");
  await clickWorld(page, canvas, 20, -25);
  await expect(page.getByTestId("selection-headline")).toHaveText("1 text");
  await enter(page, "CHPROP", "C", "yellow", "");
  await expect(logLines(page).filter({ hasText: "changed to yellow." })).toHaveCount(1);
  await page.keyboard.press("Escape");

  // ── the file holds every table and entity ──
  let text = await exported(page);
  expect(tableRow(text, "LTYPE", "Dashed")).toBeDefined();
  const walls = tableRow(text, "LAYER", "walls");
  expect([group(walls, 70), group(walls, 62), group(walls, 6)]).toEqual(["0", "4", "Dashed"]);
  expect(blocks(text)).toEqual(["Chair"]);
  // The chair's two lines are in its definition, on walls; none in the model.
  expect(lines(text)).toEqual([]);
  expect(lines(text, "BLOCKS")).toEqual([
    [0, 0, 40, 0],
    [0, 0, 0, 40],
  ]);
  expect(group(entities(text, "LINE", "BLOCKS")[0], 8)).toBe("walls");
  const placed = entities(text, "INSERT");
  expect(
    placed.map((insert) => [
      group(insert, 8),
      group(insert, 2),
      num(insert, 10),
      num(insert, 20),
      group(insert, 41),
    ]),
  ).toEqual([
    ["walls", "Chair", 0, 0, undefined],
    ["walls", "Chair", 100, 0, undefined],
    ["walls", "Chair", 100, 100, "2.0"],
  ]);
  const label = entities(text, "TEXT")[0];
  expect([
    group(label, 8),
    group(label, 62),
    num(label, 10),
    num(label, 20),
    num(label, 40),
    group(label, 1),
  ]).toEqual(["0", "2", 0, -30, 10, "Chairs"]);

  // ── redefine the chair as a tall stool: every reference follows ──
  await enter(page, "L", "200,0", "200,60", "");
  await enter(page, "B", "Chair");
  await expect(prompt(page)).toHaveText(
    'Block "Chair" already exists. Redefine it? [Yes/No] <No>:',
  );
  await enter(page, "Yes", "200,0");
  await clickWorld(page, canvas, 200, 30);
  await enter(page, "");
  await expect(logLines(page).last()).toHaveText('Block "Chair" redefined from 1 object.');
  await page.mouse.move(5, 5);
  expect(await lit(120, 0)).toBe(0);
  expect(await lit(20, 0)).toBe(0);
  expect(await lit(0, 30)).toBeGreaterThan(5);
  expect(await lit(100, 30)).toBeGreaterThan(5);
  expect(await lit(100, 200)).toBeGreaterThan(5);
  // The stool line is in the definition now, and BLOCK placed a fourth
  // reference where it was.
  text = await exported(page);
  expect(count(text, "LINE")).toBe(1);
  expect(count(text, "INSERT")).toBe(4);
  expect(await lit(200, 30)).toBeGreaterThan(5);

  // ── the layer off: the chairs are gone, the label stays; and on again ──
  await enter(page, "LAYER", "OFF", "walls", "");
  await page.mouse.move(5, 5);
  expect(await lit(100, 30)).toBe(0);
  expect(await lit(25, -25, { width: 20, height: 8 })).toBeGreaterThan(5);
  await enter(page, "LAYER", "ON", "walls", "");
  await page.mouse.move(5, 5);
  expect(await lit(100, 30)).toBeGreaterThan(5);
  await expect(logLines(page).filter({ hasText: "Unknown command" })).toHaveCount(0);
  const finished = await downloadFrom(page, () => enter(page, "DXFOUT"));

  // ── every step undone, one per command; every step redone, byte for byte ──
  let steps = 0;
  for (; steps < 40; steps += 1) {
    await page.keyboard.press("Control+z");
    if ((await logLines(page).last().textContent()) === "Nothing to undo.") break;
  }
  expect(steps).toBeGreaterThan(10);
  text = await exported(page);
  expect(count(text, "LINE") + count(text, "INSERT") + count(text, "TEXT")).toBe(0);
  expect(tableRow(text, "LAYER", "walls")).toBeUndefined();
  expect(blocks(text)).toEqual([]);
  for (let step = 0; step < steps; step += 1) await page.keyboard.press("Control+y");
  expect(await exported(page)).toBe(finished.text);

  // ── export, refresh, reopen: the same drawing, tables and blocks and all ──
  await page.reload();
  await openApp(page);
  await chooseFile(page, () => enter(page, "OPEN"), {
    name: finished.name,
    mimeType: "application/dxf",
    buffer: Buffer.from(finished.text),
  });
  await expect(
    logLines(page).filter({ hasText: "Imported 1 text, 4 block references." }),
  ).toHaveCount(1);
  await expect(logLines(page).last()).toHaveText("With 1 layer, 1 linetype, and 1 block.");
  await enter(page, "LAYER", "?");
  await expect(logLines(page).last()).toHaveText('"walls": colour 4, on, Dashed');
  await enter(page, "");
  expect(await exported(page)).toBe(finished.text);
});

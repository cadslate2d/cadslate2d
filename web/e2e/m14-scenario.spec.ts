// The M14 exit scenario (#230): a plate dimensioned by ordinates with
// QDIM, a note over a hatch on a wipeout, its texts brought to the front,
// layer states for two sheets, a boundary offset; saved, reopened, and a
// state restored in the drawing read back.
import { expect, test, type Locator, type Page } from "@playwright/test";
import {
  chooseFile,
  clickWorld,
  downloadFrom,
  dxfSection,
  enter,
  entities,
  group,
  logLines,
  openApp,
  pixelsAround,
  polylines,
  prompt,
} from "./support";

type Pixel = [red: number, green: number, blue: number];
const light = ([r, g, b]: Pixel) => r > 140 && g > 140 && b > 140;

/** Light pixels — the foreground — in a small box about a world point. */
function lightAround(page: Page, canvas: Locator, x: number, y: number): Promise<number> {
  return pixelsAround(page, canvas, { x, y }, { width: 5, height: 5 }, light);
}

test("the M14 scenario: ordinates, a note on a wipeout over a hatch, layer states, a boundary", async ({
  page,
}) => {
  const canvas = await openApp(page);
  await page.keyboard.press("F7");
  await page.keyboard.press("F3");
  await enter(page, "LAYER", "N", "PLAN", "N", "NOTES", "S", "PLAN", "");

  // 1. A plate with two holes, dimensioned by ordinates from its corner.
  await enter(page, "PLINE", "0,0", "120,0", "120,70", "0,70", "C");
  await enter(page, "C", "30,35", "8", "C", "90,35", "8");
  await enter(page, "QDIM", "ALL", "", "O", "D", "0,0", "60,-25");
  await expect(logLines(page).last()).toHaveText("4 dimensions made.");
  await enter(page, "QDIM", "ALL", "", "O", "-25,35");
  await expect(logLines(page).last()).toHaveText("3 dimensions made.");

  // 2. A note on a wipeout over a hatch, beside the plate.
  await enter(page, "LAYER", "S", "NOTES", "");
  await enter(page, "PLINE", "150,0", "250,0", "250,70", "150,70", "C");
  await enter(page, "HATCH", "P", "SOLID", "200,10");
  await expect(prompt(page)).toHaveText("Command:");
  await enter(page, "LINE", "140,35", "260,35", "");
  await enter(page, "WIPEOUT", "170,25", "230,25", "230,45", "170,45", "");
  await enter(page, "TEXT", "175,31", "8", "0", "NOTE");
  await enter(page, "TEXTTOFRONT", "");
  await page.mouse.move(5, 5);
  expect(await lightAround(page, canvas, 222, 35)).toBe(0);
  expect(await lightAround(page, canvas, 160, 55)).toBeGreaterThan(50);

  // 3. A boundary round the plate's outline, offset outwards.
  await enter(page, "LAYER", "S", "PLAN", "");
  await enter(page, "BO", "I", "5,5", "");
  await expect(logLines(page).filter({ hasText: "BOUNDARY created 1 polyline." })).toHaveCount(1);
  await enter(page, "OFFSET", "5");
  await clickWorld(page, canvas, 60, 0);
  await clickWorld(page, canvas, 60, -40);
  await enter(page, "");

  // 4. Layer states for two sheets: everything, and the plate alone.
  await enter(page, "LAS", "S", "Sheet A");
  await enter(page, "LAYER", "OFF", "NOTES", "");
  await enter(page, "LAS", "S", "Plate only");
  await enter(page, "LAS", "R", "Sheet A");
  await expect(logLines(page).last()).toHaveText(
    'Layer state "Sheet A" restored: 1 layer changed.',
  );

  // 5. Saved: the ordinates, the wipeout, the states, the offset.
  const file = await downloadFrom(page, () => enter(page, "DXFOUT"));
  const dimensions = entities(file.text, "DIMENSION");
  expect(dimensions).toHaveLength(7);
  const kinds = dimensions.map((dimension) => Number(group(dimension, 70)) & 0x4f);
  expect(kinds.filter((kind) => kind === 0x46)).toHaveLength(4);
  expect(kinds.filter((kind) => kind === 0x06)).toHaveLength(3);
  expect(entities(file.text, "WIPEOUT")).toHaveLength(1);
  const objects = dxfSection(file.text, "OBJECTS");
  expect(objects.filter((record) => record.type === "XRECORD")).toHaveLength(2);
  // The plate's outline five out, its corners rounded.
  const offset = polylines(file.text).find((polyline) =>
    polyline.vertices.some(([x, y]) => x === 0 && y === -5),
  );
  expect(offset?.closed).toBe(true);
  expect(offset?.vertices.filter(([, , bulge]) => bulge !== 0)).toHaveLength(4);

  // 6. Reopened: the states come back, and one is restored.
  await chooseFile(page, () => enter(page, "OPEN"), {
    name: "plate.dxf",
    mimeType: "application/dxf",
    buffer: Buffer.from(file.text),
  });
  const imported = logLines(page).filter({ hasText: "With " }).last();
  await expect(imported).toContainText("2 layer states");
  await page.getByRole("button", { name: "Dismiss the import report" }).click();
  await enter(page, "LAS", "R", "Plate only");
  await expect(logLines(page).last()).toHaveText(
    'Layer state "Plate only" restored: 1 layer changed.',
  );
  await page.mouse.move(5, 5);
  expect(await lightAround(page, canvas, 160, 55)).toBe(0);
  await expect(logLines(page).filter({ hasText: "Unknown command" })).toHaveCount(0);
});

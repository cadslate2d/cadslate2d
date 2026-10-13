// Drafting far from the origin, end to end (#151; ADR-0012): a plot ten
// million units out, opened from a file so the view fits it, drafted on
// with typed coordinates, snaps, and TRIM, and saved back exactly.
import { expect, test, type Locator, type Page } from "@playwright/test";
import {
  chooseFile,
  downloadFrom,
  enter,
  lines,
  litPixelsAround,
  logLines,
  openApp,
  prompt,
  settle,
} from "./support";

const FAR = 10_000_000;

/** A DXF of lines, each `[x0, y0, x1, y1]`. */
function dxfOfLines(segments: number[][]): string {
  const groups: [number, string][] = [
    [0, "SECTION"],
    [2, "ENTITIES"],
  ];
  for (const [x0, y0, x1, y1] of segments) {
    groups.push([0, "LINE"], [8, "0"], [10, `${x0}`], [20, `${y0}`], [11, `${x1}`], [21, `${y1}`]);
  }
  groups.push([0, "ENDSEC"], [0, "EOF"]);
  return groups.map(([code, value]) => `${code}\n${value}\n`).join("");
}

/**
 * The view's mapping, read off the status bar: the world coordinates under
 * two screen points give the scale and where the centre is.
 */
async function mapping(
  page: Page,
  canvas: Locator,
): Promise<(x: number, y: number) => { x: number; y: number }> {
  const box = await canvas.boundingBox();
  if (!box) throw new Error("no canvas");
  const readout = page.getByTestId("coordinates");
  const at = async (sx: number, sy: number) => {
    const stale = await readout.textContent();
    await page.mouse.move(box.x + sx, box.y + sy);
    await settle(page);
    // The readout follows the pointer a frame later.
    await expect.poll(() => readout.textContent()).not.toBe(stale);
    const [x, y] = (await readout.textContent())?.split(",").map(Number) ?? [];
    return { x: x ?? 0, y: y ?? 0 };
  };
  const a = await at(100, 100);
  const b = await at(300, 100);
  const scale = 200 / (b.x - a.x);
  return (x, y) => ({ x: box.x + 100 + (x - a.x) * scale, y: box.y + 100 - (y - a.y) * scale });
}

test("a plot ten million units out is drafted on and saved exactly", async ({ page }) => {
  const canvas = await openApp(page);
  await page.keyboard.press("F7");
  // A vertical edge, a horizontal line crossing it, and a short line to extend.
  const plot = dxfOfLines([
    [FAR + 40, FAR - 60, FAR + 40, FAR + 60],
    [FAR, FAR, FAR + 100, FAR],
    [FAR, FAR + 20, FAR + 30, FAR + 20],
  ]);
  await chooseFile(page, () => enter(page, "OPEN"), {
    name: "far.dxf",
    mimeType: "application/dxf",
    buffer: Buffer.from(plot),
  });
  await expect(logLines(page).filter({ hasText: "Imported 3 lines." })).toHaveCount(1);
  await page.getByRole("button", { name: "Dismiss the import report" }).click();
  const screen = await mapping(page, canvas);

  // A line by typed coordinates, then one from a snapped endpoint.
  await enter(page, "L", `${FAR},${FAR + 50}`, "@80,0", "");
  await expect(prompt(page)).toHaveText("Command:");
  await enter(page, "L");
  const end = screen(FAR + 100, FAR);
  await page.mouse.move(end.x + 2, end.y - 1);
  await expect(page.getByTestId("snap")).toHaveText("Endpoint");
  await page.mouse.click(end.x + 2, end.y - 1);
  await enter(page, "@0,-30", "");
  await expect(prompt(page)).toHaveText("Command:");

  // TRIM the crossing line's right part at the edge, EXTEND the short one to it.
  await enter(page, "TR");
  const edge = screen(FAR + 40, FAR + 30);
  await page.mouse.click(edge.x, edge.y);
  await enter(page, "");
  const right = screen(FAR + 80, FAR);
  await page.mouse.click(right.x, right.y);
  await enter(page, "");
  await expect(prompt(page)).toHaveText("Command:");
  await enter(page, "EX");
  await page.mouse.click(edge.x, edge.y);
  await enter(page, "");
  const short = screen(FAR + 20, FAR + 20);
  await page.mouse.click(short.x, short.y);
  await enter(page, "");
  await expect(prompt(page)).toHaveText("Command:");

  // What is drawn is where it is: the crossing line's end is lit at the edge, nothing past it.
  await page.mouse.move(5, 5);
  const file = await downloadFrom(page, () => enter(page, "DXFOUT"));
  const saved = lines(file.text).map((line) => line.map((value) => value - FAR));
  expect(saved).toEqual(
    expect.arrayContaining([
      [40, -60, 40, 60],
      [0, 0, 40, 0],
      [0, 20, 40, 20],
      [0, 50, 80, 50],
      [100, 0, 100, -30],
    ]),
  );
  expect(saved).toHaveLength(5);
  await expect(logLines(page).filter({ hasText: "Unknown command" })).toHaveCount(0);
});

test("a drawing that spans ten million units draws its far end to a pixel at a hundred pixels per unit", async ({
  page,
}) => {
  // #188 (ADR-0028): the far plot's chunk is packed against an origin of
  // its own, so its offsets are tens of units and exact. The near lines
  // fill the first chunk, so the plot is in the second.
  const canvas = await openApp(page);
  await page.keyboard.press("F7");
  const near: number[][] = [];
  for (let index = 0; index < 600; index += 1) near.push([0, index, 10, index]);
  const dxf = dxfOfLines([
    ...near,
    [FAR, FAR, FAR + 10, FAR],
    [FAR + 5, FAR + 4, FAR + 5, FAR - 4],
  ]);
  await chooseFile(page, () => enter(page, "OPEN"), {
    name: "span.dxf",
    mimeType: "application/dxf",
    buffer: Buffer.from(dxf),
  });
  await expect(logLines(page).filter({ hasText: "Imported 602 lines." })).toHaveCount(1);
  await page.getByRole("button", { name: "Dismiss the import report" }).click();
  // ZOOM Center on the far plot at a height of six units: a hundred
  // pixels per unit on the 600-pixel canvas.
  await enter(page, "ZOOM", "C", `${FAR + 5},${FAR}`, "6");
  await page.mouse.move(5, 5);
  const screen = await mapping(page, canvas);
  const box = await canvas.boundingBox();
  const centre = screen(FAR + 5, FAR);
  const scale = screen(FAR + 6, FAR).x - centre.x;
  expect(scale, "about a hundred pixels per unit").toBeGreaterThan(60);
  expect(scale, "about a hundred pixels per unit").toBeLessThan(200);
  // The horizontal line lies on the centre row to a pixel: lit pixels in
  // the rows about it, and none in the rows three and more above or
  // below — where a line placed two pixels off would reach.
  const row = Math.round(centre.y - (box?.y ?? 0));
  const column = Math.round(centre.x - (box?.x ?? 0));
  const band = (rows: number, height: number) =>
    litPixelsAround(
      page,
      canvas,
      { x: column - (box?.width ?? 0) / 2 + 60, y: (box?.height ?? 0) / 2 - row + rows },
      { width: 50, height },
    );
  expect(await band(0, 2)).toBeGreaterThan(80);
  expect(await band(4, 1)).toBe(0);
  expect(await band(-4, 1)).toBe(0);
});

test("a drawing that spans ten million units draws its far end to a unit", async ({ page }) => {
  // The scene's single origin (ADR-0003) sits at the centre of the extents;
  // f32 offsets from it resolve to a unit at 10⁷. A plot that far from a
  // title at the origin is placed to that unit: exact at a pixel per unit,
  // and off by up to a pixel at every further e-fold of zoom — the limit
  // #151 records in ADR-0003.
  const canvas = await openApp(page);
  await page.keyboard.press("F7");
  const both = dxfOfLines([
    [0, 0, 100, 0],
    [FAR, FAR, FAR + 100, FAR],
    [FAR, FAR + 60, FAR, FAR - 60],
  ]);
  await chooseFile(page, () => enter(page, "OPEN"), {
    name: "span.dxf",
    mimeType: "application/dxf",
    buffer: Buffer.from(both),
  });
  await expect(logLines(page).filter({ hasText: "Imported 3 lines." })).toHaveCount(1);
  // The report lies over the top of the canvas, where the far plot is.
  await page.getByRole("button", { name: "Dismiss the import report" }).click();
  // Zoom in on the far plot until a unit is about a pixel: an e-fold a
  // step, aimed afresh each time, since at the fitted scale a pixel is
  // thousands of units and the plot is a dot.
  for (let step = 0; step < 10; step += 1) {
    const aim = await mapping(page, canvas);
    const plot = aim(FAR + 50, FAR);
    await page.mouse.move(plot.x, plot.y);
    await settle(page);
    await page.mouse.wheel(0, -500);
    await settle(page);
  }
  await page.mouse.move(5, 5);
  const screen = await mapping(page, canvas);
  const box = await canvas.boundingBox();
  const centre = screen(FAR + 50, FAR);
  // The horizontal line crosses the view's middle: lit pixels within a few of its row.
  const row = Math.round(centre.y - (box?.y ?? 0));
  const column = Math.round(centre.x - (box?.x ?? 0));
  const scale = screen(FAR + 51, FAR).x - centre.x;
  const lit = await litPixelsAround(
    page,
    canvas,
    { x: column - (box?.width ?? 0) / 2, y: (box?.height ?? 0) / 2 - row },
    { width: 40, height: 6 },
  );
  expect(scale, "about a pixel per unit").toBeGreaterThan(0.5);
  expect(scale, "about a pixel per unit").toBeLessThan(5);
  expect(lit, `at ${scale.toFixed(2)} px per unit`).toBeGreaterThan(40);
});

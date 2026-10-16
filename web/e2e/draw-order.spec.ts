// The drawing order and wipeouts (#226, ADR-0032): a hatch brought to the
// front hides the line through it, a text drawn after it lies over it, a
// wipeout masks what is beneath it and a note lies over the wipeout — on
// screen, and in the file.
import { expect, test, type Locator, type Page } from "@playwright/test";
import {
  centerOf,
  clickWorld,
  downloadFrom,
  dxfSection,
  enter,
  entities,
  group,
  logLines,
  openApp,
  pixelsAround,
  prompt,
} from "./support";

type Pixel = [red: number, green: number, blue: number];
const red = ([r, g, b]: Pixel) => r > 150 && g < 90 && b < 90;
const light = ([r, g, b]: Pixel) => r > 140 && g > 140 && b > 140;

/** Light pixels — the foreground — in a small box about a world point. */
function lightAround(page: Page, canvas: Locator, x: number, y: number): Promise<number> {
  return pixelsAround(page, canvas, { x, y }, { width: 6, height: 6 }, light);
}

/** Compares a clip around the canvas centre with its baseline, in Chromium. */
async function matches(page: Page, canvas: Locator, name: string) {
  const center = await centerOf(canvas);
  await expect(page).toHaveScreenshot(name, {
    clip: { x: Math.round(center.x) - 80, y: Math.round(center.y) - 45, width: 160, height: 90 },
    maxDiffPixels: 60,
  });
}

test("a hatch brought to the front hides the line through it, and a text lies over it", async ({
  page,
  browserName,
}) => {
  const canvas = await openApp(page);
  await page.keyboard.press("F7");
  await enter(page, "PLINE", "-60,-30", "60,-30", "60,30", "-60,30", "C");
  await enter(page, "COLOR", "red", "HATCH");
  await clickWorld(page, canvas, -40, 15);
  await expect(prompt(page)).toHaveText("Command:");
  await enter(page, "COLOR", "bylayer", "LINE", "-80,0", "80,0", "");
  await page.mouse.move(5, 5);
  // Drawn after the hatch, the line is over it.
  expect(await lightAround(page, canvas, -20, 0)).toBeGreaterThan(4);

  await clickWorld(page, canvas, -40, 15);
  await enter(page, "DR");
  await expect(prompt(page)).toHaveText(
    "Enter object ordering option or [Above objects/Under objects/Front/Back] <Back>:",
  );
  await enter(page, "F");
  await expect(logLines(page).last()).toHaveText("1 object brought to the front.");
  await page.mouse.move(5, 5);
  // In front, the hatch hides the line inside it, and only there.
  expect(await lightAround(page, canvas, -20, 0)).toBe(0);
  expect(
    await pixelsAround(page, canvas, { x: -20, y: 0 }, { width: 6, height: 6 }, red),
  ).toBeGreaterThan(100);
  expect(await lightAround(page, canvas, 70, 0)).toBeGreaterThan(4);

  // A text made now is in the hatch's tier, a stroke over its fill.
  await enter(page, "TEXT", "-45,8", "12", "0", "NOTE");
  await page.mouse.move(5, 5);
  if (browserName === "chromium") await matches(page, canvas, "text-over-hatch.png");

  // U puts the hatch back where it was.
  await enter(page, "U", "U");
  await page.mouse.move(5, 5);
  expect(await lightAround(page, canvas, -20, 0)).toBeGreaterThan(4);
  await enter(page, "REDO", "REDO");

  // The file keeps the order as the order of its records, which every
  // reader paints in: the hatch after the line.
  const file = await downloadFrom(page, () => enter(page, "DXFOUT"));
  expect(entities(file.text).map((record) => record.type)).toEqual([
    "LWPOLYLINE",
    "LINE",
    "HATCH",
    "TEXT",
  ]);
});

test("a wipeout masks the lines beneath it and a note lies over it", async ({
  page,
  browserName,
}) => {
  const canvas = await openApp(page);
  await page.keyboard.press("F7");
  for (const y of [-20, -10, 0, 10, 20]) {
    await enter(page, "LINE", `-80,${y}`, `80,${y}`, "");
  }
  await enter(page, "WIPEOUT", "-50,-25", "50,-25", "50,25", "-50,25", "");
  await expect(prompt(page)).toHaveText("Command:");
  await page.mouse.move(5, 5);
  expect(await lightAround(page, canvas, -30, 0)).toBe(0);
  expect(await lightAround(page, canvas, 70, 0)).toBeGreaterThan(4);
  // Its frame is shown.
  expect(await lightAround(page, canvas, 0, 25)).toBeGreaterThan(4);

  await enter(page, "TEXT", "-40,-5", "10", "0", "NOTE");
  await page.mouse.move(5, 5);
  if (browserName === "chromium") await matches(page, canvas, "wipeout-under-note.png");

  await enter(page, "WIPEOUT", "F", "OFF");
  await expect(logLines(page).last()).toHaveText("Wipeout frames are hidden.");
  await page.mouse.move(5, 5);
  expect(await lightAround(page, canvas, 0, 25)).toBe(0);

  const file = await downloadFrom(page, () => enter(page, "DXFOUT"));
  expect(entities(file.text, "WIPEOUT")).toHaveLength(1);
  const variables = dxfSection(file.text, "OBJECTS").find(
    (record) => record.type === "WIPEOUTVARIABLES",
  );
  expect(group(variables, 70)).toBe("0");
});

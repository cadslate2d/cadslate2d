// Images (ADR-0042, #524–#527): a DXF holding a picture — copied into the
// file by our writer — opens with the picture under the lines drawn over
// it, its quarters where the picture has them; faded, it is paler; the
// file is written back with the image and its copy; a plot embeds the
// picture, compressed as it is held; and IMAGEATTACH places a PNG the
// browser decodes and the host makes again, under a line drawn over it,
// each undone in turn.
import { readFileSync } from "node:fs";
import { expect, test, type Locator, type Page } from "@playwright/test";
import {
  centerOf,
  chooseFile,
  downloadFrom,
  enter,
  entities,
  fixturePath,
  logLines,
  openApp,
  pixelsAround,
  prompt,
} from "./support";

type Pixel = [red: number, green: number, blue: number];
const red = ([r, g, b]: Pixel) => r > 150 && g < 100 && b < 100;
const blue = ([r, g, b]: Pixel) => b > 150 && r < 110 && g < 140;
/** The red quarter faded halfway toward the dark canvas: a darker red. */
const fadedRed = ([r, g, b]: Pixel) => r > 60 && r < 150 && g < 60 && b < 60;

/**
 * Pixels that `matches` about a point `x` right of and `y` above the
 * canvas centre, in CSS pixels. The picture is fitted to the view, centred:
 * its quarters lie about 120 pixels either side and 90 above or below,
 * a dark diagonal from its lower-left corner across them.
 */
function around(page: Page, canvas: Locator, x: number, y: number, matches: (p: Pixel) => boolean) {
  return pixelsAround(page, canvas, { x, y }, { width: 8, height: 8 }, matches);
}

/** Compares a clip around the canvas centre with its baseline, in Chromium. */
async function matches(page: Page, canvas: Locator, name: string) {
  const center = await centerOf(canvas);
  await expect(page).toHaveScreenshot(name, {
    clip: { x: Math.round(center.x) - 100, y: Math.round(center.y) - 70, width: 200, height: 140 },
    maxDiffPixels: 120,
  });
}

/** Opens the picture fixture, `edit`ed, and fits it to the view. */
async function openPicture(page: Page, edit: (text: string) => string = (text) => text) {
  const canvas = await openApp(page);
  const text = readFileSync(fixturePath("valid/image-embedded.dxf"), "utf8");
  await chooseFile(page, () => enter(page, "OPEN"), {
    name: "pattern.dxf",
    mimeType: "application/dxf",
    buffer: Buffer.from(edit(text)),
  });
  await expect(logLines(page).filter({ hasText: "1 image" })).not.toHaveCount(0);
  await page.getByRole("button", { name: "Dismiss the import report" }).click();
  await enter(page, "ZOOM", "E");
  await expect(prompt(page)).toHaveText("Command:");
  await page.mouse.move(5, 5);
  return canvas;
}

test("a picture in a DXF is drawn under the lines over it, the right way up", async ({
  page,
  browserName,
}) => {
  const canvas = await openPicture(page);
  // The picture's upper-left quarter is red and its lower-left blue, once
  // the browser has decoded it.
  await expect.poll(() => around(page, canvas, -120, 90, red)).toBeGreaterThan(20);
  // Clear of the dark diagonal across the blue quarter.
  expect(await around(page, canvas, -180, -30, blue)).toBeGreaterThan(20);
  if (browserName === "chromium") await matches(page, canvas, "image-under-lines.png");

  // Written back with the image and its copy of the picture.
  const file = await downloadFrom(page, () => enter(page, "DXFOUT"));
  expect(entities(file.text, "IMAGE")).toHaveLength(1);
  expect(file.text).toContain("CADSLATE_IMAGES");

  // Plotted, the page draws the picture, embedded once: the PNG's
  // compressed samples, ASCII on the way, decoded by the reader.
  await enter(page, "PLOT");
  const pdf = await downloadFrom(page, () => enter(page, "A4", "E", "Fit", "Y", "C"));
  expect(pdf.text.startsWith("%PDF-1.4\n")).toBe(true);
  expect(pdf.text).toMatch(/\/XObject << \/Im\d+ 5 0 R >>/);
  expect(pdf.text).toContain(
    "/Subtype /Image /Width 64 /Height 48 /ColorSpace /DeviceRGB /BitsPerComponent 8 " +
      "/Filter [/ASCII85Decode /FlateDecode] " +
      "/DecodeParms [null << /Predictor 15 /Colors 3 /BitsPerComponent 8 /Columns 64 >>]",
  );
  expect(pdf.text).toMatch(/ cm\n\/Im\d+ Do\nQ\n/);
  expect(pdf.text.endsWith("%%EOF\n")).toBe(true);
});

test("a faded picture is drawn paler, toward the canvas", async ({ page, browserName }) => {
  // The same file, its image faded halfway (283).
  const canvas = await openPicture(page, (text) => text.replace("\n283\n0\n", "\n283\n50\n"));
  await expect.poll(() => around(page, canvas, -120, 90, fadedRed)).toBeGreaterThan(20);
  expect(await around(page, canvas, -120, 90, red)).toBe(0);
  if (browserName === "chromium") await matches(page, canvas, "image-faded.png");
});

test("IMAGEATTACH places a PNG under what is drawn over it, and undo takes each away", async ({
  page,
}) => {
  const canvas = await openApp(page);
  // The test picture: 16 × 12, red above blue on the left.
  await chooseFile(page, () => enter(page, "IAT"), {
    name: "quarters.png",
    mimeType: "image/png",
    buffer: readFileSync(new URL("../../test-data/images/quarters.png", import.meta.url)),
  });
  await expect(prompt(page)).toHaveText("Specify insertion point:");
  await expect(logLines(page).last()).toHaveText('"quarters": 16 × 12 pixels, PNG.');
  await enter(page, "0,0", "640", "0");
  await expect(logLines(page).last()).toHaveText(
    'Attached "quarters", 16 × 12 pixels, 640.0000 across.',
  );
  await enter(page, "ZOOM", "E");
  await page.mouse.move(5, 5);
  await expect.poll(() => around(page, canvas, -120, 90, red)).toBeGreaterThan(20);
  expect(await around(page, canvas, -120, -90, blue)).toBeGreaterThan(20);

  // A line over it, then each undone in turn: the line, then the picture.
  const drawn = async () => {
    const file = await downloadFrom(page, () => enter(page, "DXFOUT"));
    return [entities(file.text, "IMAGE").length, entities(file.text, "LINE").length];
  };
  await enter(page, "L", "0,0", "640,480", "");
  expect(await drawn()).toEqual([1, 1]);
  await enter(page, "U");
  expect(await drawn()).toEqual([1, 0]);
  expect(await around(page, canvas, -120, 90, red)).toBeGreaterThan(20);
  await enter(page, "U");
  expect(await drawn()).toEqual([0, 0]);
  await expect.poll(() => around(page, canvas, -120, 90, red)).toBe(0);
});

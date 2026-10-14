// Text styles (#221): a style made with STYLE — a fixed height, a width
// factor, a slant — taken by TEXT, drawn widened and leaning, and written
// with the text's own width and slant.
import { expect, test } from "@playwright/test";
import {
  downloadFrom,
  enter,
  entities,
  group,
  logLines,
  openApp,
  prompt,
  tableRow,
} from "./support";

test("a text in a widened, slanted style is drawn and saved that way", async ({
  page,
  browserName,
}) => {
  const canvas = await openApp(page);
  await page.keyboard.press("F7");
  await enter(page, "STYLE", "N", "Slanted", "H", "12", "W", "1.5", "O", "20", "");
  await expect(
    logLines(page).filter({ hasText: '"Slanted": height 12, width 1.5, oblique 20°.' }),
  ).toHaveCount(1);
  await enter(page, "TEXT", "-60,-6");
  await expect(prompt(page)).toHaveText("Specify rotation angle of text <0>:");
  await enter(page, "", "Slanted");
  await page.mouse.move(2, 2);

  if (browserName === "chromium") {
    const box = await canvas.boundingBox();
    if (!box) throw new Error("no canvas");
    await expect(page).toHaveScreenshot("text-style-slanted.png", {
      clip: {
        x: box.x + box.width / 2 - 75,
        y: box.y + box.height / 2 - 20,
        width: 150,
        height: 40,
      },
      maxDiffPixels: 80,
    });
  }

  const file = await downloadFrom(page, () => enter(page, "DXFOUT"));
  const text = entities(file.text, "TEXT")[0];
  expect([group(text, 40), group(text, 41), group(text, 7)]).toEqual(["12.0", "1.5", "Slanted"]);
  expect(Number(group(text, 51))).toBeCloseTo(20, 9);
  const style = tableRow(file.text, "STYLE", "Slanted");
  expect([group(style, 40), group(style, 41)]).toEqual(["12.0", "1.5"]);
  expect(file.text).toContain("\n  9\n$TEXTSTYLE\n  7\nSlanted\n");
});

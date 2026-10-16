// The M13 exit scenario (#224): a plan laid out with grid snap and object
// snap tracking, labelled in a text style of its own, a title block
// written as a file, and a second drawing that inserts the title block and
// takes a label pasted across; both saved, and the second reopened.
import { readFileSync } from "node:fs";
import { expect, test, type Locator, type Page } from "@playwright/test";
import {
  centerOf,
  chooseFile,
  downloadFrom,
  enter,
  entities,
  fixturePath,
  group,
  logLines,
  num,
  openApp,
  pointerRests,
  prompt,
  settle,
  tableRow,
} from "./support";

/**
 * Moves the pointer to `(x, y)` and waits until it has rested there, as
 * tracking needs. It waits for that rest to be told, rather than for a
 * time: under load a browser runs frames and timers late — Firefox a frame
 * a second — and a fixed wait ran out first. A rest still due from where
 * the pointer was before does not count: the rest must be at `(x, y)`.
 */
async function restAt(page: Page, canvas: Locator, x: number, y: number): Promise<void> {
  const center = await centerOf(canvas);
  const box = await canvas.boundingBox();
  if (!box) throw new Error("the canvas is not laid out");
  const target = { x: center.x + x, y: center.y - y };
  const before = (await pointerRests(page)).count;
  await page.mouse.move(target.x, target.y);
  await expect
    .poll(
      async () => {
        const rests = await pointerRests(page);
        const at = rests.at;
        return (
          rests.count > before &&
          at !== undefined &&
          Math.abs(at.x - (target.x - box.x)) < 1 &&
          Math.abs(at.y - (target.y - box.y)) < 1
        );
      },
      { timeout: 15_000 },
    )
    .toBe(true);
  await settle(page);
}

async function clickAt(page: Page, canvas: Locator, x: number, y: number): Promise<void> {
  const center = await centerOf(canvas);
  await page.mouse.click(center.x + x, center.y - y);
}

test("the M13 scenario: snap and tracking, a style, a title block as a file, the clipboard across drawings", async ({
  page,
}) => {
  const canvas = await openApp(page);
  await page.keyboard.press("F7");

  // 1. Grid snap: two walls drawn with the pointer, on the 10 grid.
  await page.keyboard.press("F3");
  await page.keyboard.press("F9");
  await enter(page, "SNAP", "10", "L");
  await clickAt(page, canvas, 1.2, -2.7);
  await clickAt(page, canvas, 98.6, 1.9);
  await clickAt(page, canvas, 101.4, 62.2);
  await enter(page, "");

  // 2. Tracking: the corner level with one wall's end and above the
  // other's, found by resting on both ends.
  await page.keyboard.press("F3");
  await page.keyboard.press("F11");
  await enter(page, "L");
  await restAt(page, canvas, 100, 60);
  await restAt(page, canvas, 0, 0);
  await restAt(page, canvas, 3, 57);
  await expect(page.getByTestId("tracking")).toHaveText("Tracking: intersection");
  await clickAt(page, canvas, 3, 57);
  await enter(page, "0,0", "");
  await page.keyboard.press("F11");

  // 3. A label in a style of its own: fixed height, narrow, slanted.
  await enter(page, "STYLE", "N", "Labels", "H", "5", "W", "0.8", "O", "10", "");
  await enter(page, "TEXT", "20,25", "", "Kitchen");

  // 4. A title block, written as a file about its corner; its text in the
  // current style, whose fixed height is not asked for.
  await page.keyboard.press("F9");
  await enter(page, "REC", "0,-40", "100,-15", "DT", "5,-30", "0", "Flat 3");
  await enter(page, "SELECT", "CPOLYGON", "-5,-45", "105,-45", "105,-10", "-5,-10", "", "");
  const title = await downloadFrom(page, () => enter(page, "WBLOCK", "0,-40", "title"));
  expect(title.name).toBe("title.dxf");

  // 5. The label copied, and the plan saved.
  await enter(page, "SELECT", "CPOLYGON", "15,20", "60,20", "60,35", "15,35", "", "");
  await page.keyboard.press("Control+c");
  await expect(logLines(page).last()).toHaveText("1 object copied to the clipboard.");
  const plan = await downloadFrom(page, () => enter(page, "DXFOUT"));
  const walls = entities(plan.text, "LINE").map((line) =>
    [10, 20, 11, 21].map((code) => num(line, code)),
  );
  expect(walls).toEqual([
    [0, 0, 100, 0],
    [100, 0, 100, 60],
    [0, 60, 0, 0],
  ]);

  // 6. A second drawing: the title block inserted, the label pasted.
  await chooseFile(page, () => enter(page, "OPEN"), {
    name: "sheet.dxf",
    mimeType: "application/dxf",
    buffer: readFileSync(fixturePath("valid/layouts.dxf")),
  });
  await page.getByRole("button", { name: "Dismiss the import report" }).click();
  await chooseFile(page, () => enter(page, "INSERT", "~"), {
    name: title.name,
    mimeType: "application/dxf",
    buffer: Buffer.from(title.text),
  });
  await enter(page, "300,0", "", "");
  await page.keyboard.press("Control+v");
  await expect(prompt(page)).toHaveText("Specify insertion point:");
  await enter(page, "310,40");
  await expect(logLines(page).last()).toHaveText("1 object pasted.");
  const sheet = await downloadFrom(page, () => enter(page, "DXFOUT"));
  expect(entities(sheet.text, "INSERT").map((insert) => group(insert, 2))).toEqual(["title"]);
  const label = entities(sheet.text, "TEXT").find((text) => group(text, 1) === "Kitchen");
  expect([group(label, 7), group(label, 41), group(label, 40)]).toEqual(["Labels", "0.8", "5.0"]);
  expect(tableRow(sheet.text, "STYLE", "Labels")).toBeDefined();

  // 7. Reopened: the block, the label, and its style as they were.
  await chooseFile(page, () => enter(page, "OPEN"), {
    name: "sheet.dxf",
    mimeType: "application/dxf",
    buffer: Buffer.from(sheet.text),
  });
  const imported = logLines(page).filter({ hasText: "Imported " }).last();
  await expect(imported).toContainText("1 block reference");
  await page.getByRole("button", { name: "Dismiss the import report" }).click();
  await enter(page, "STYLE", "S", "Labels", "");
  await expect(logLines(page).filter({ hasText: 'Current text style: "Labels".' })).toHaveCount(1);
  await expect(logLines(page).filter({ hasText: "Unknown command" })).toHaveCount(0);
});

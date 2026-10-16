// The M16 exit scenario (#238): a drawing as it might arrive, cleaned up —
// duplicates removed, layers merged and deleted — a spline edited, and a
// named view saved and gone back to; saved, reopened, and the view
// restored in the drawing read back.
import { expect, test } from "@playwright/test";
import {
  chooseFile,
  clickWorld,
  downloadFrom,
  entities,
  enter,
  group,
  logLines,
  openApp,
  tableRow,
} from "./support";

test("the M16 scenario: OVERKILL, LAYMRG and LAYDEL, SPLINEDIT, and named views", async ({
  page,
}) => {
  const canvas = await openApp(page);
  await page.keyboard.press("F7");
  await page.keyboard.press("F3");

  // 1. As it arrived: walls on two layers, drawn twice and in pieces;
  // notes on a third; a column twice.
  await enter(page, "LAYER", "N", "WALLS,WALL-OLD,NOTES,KEEP", "S", "WALLS", "");
  await enter(page, "L", "0,0", "200,0", "");
  await enter(page, "L", "0,0", "200,0", "");
  await enter(page, "L", "150,0", "300,0", "");
  await enter(page, "C", "100,50", "10", "C", "100,50", "10");
  await enter(page, "LAYER", "S", "WALL-OLD", "");
  await enter(page, "L", "0,100", "300,100", "");
  await enter(page, "LAYER", "S", "NOTES", "");
  await enter(page, "TEXT", "10,120", "8", "0", "Old note");
  await enter(page, "LAYER", "S", "KEEP", "");

  // 2. Duplicates removed: the walls one line, the column one circle.
  await enter(page, "OVERKILL", "ALL", "", "");
  await expect(logLines(page).last()).toHaveText(
    "3 objects removed; 1 line lengthened to cover them.",
  );

  // 3. The old walls folded into the walls, the notes deleted.
  await enter(page, "LAYMRG", "N", "WALL-OLD", "", "N", "WALLS");
  await expect(logLines(page).last()).toHaveText('1 layer merged into "WALLS": 1 object moved.');
  await enter(page, "LAYDEL", "N", "NOTES", "", "Y");
  await expect(logLines(page).last()).toHaveText("1 layer deleted, with 1 object on it.");

  // 4. A spline below the walls, closed.
  await enter(page, "SPLINE", "0,-100", "100,-60", "200,-100", "300,-60", "");
  await enter(page, "SPLINEDIT");
  await clickWorld(page, canvas, 0, -100);
  await enter(page, "C", "");

  // 5. A named view of the column, gone back to after zooming out.
  await enter(page, "Z", "W", "80,30", "120,70");
  await enter(page, "VIEW", "S", "Column");
  await enter(page, "Z", "E");
  await enter(page, "VIEW", "R", "Column");
  await expect(logLines(page).last()).toHaveText('View "Column" restored.');

  // 6. Saved: two lines, one circle, the spline closed, the layers gone,
  // the view kept.
  const file = await downloadFrom(page, () => enter(page, "DXFOUT"));
  expect(entities(file.text, "LINE")).toHaveLength(2);
  expect(entities(file.text, "CIRCLE")).toHaveLength(1);
  expect(entities(file.text, "TEXT")).toHaveLength(0);
  const splines = entities(file.text, "SPLINE");
  expect(Number(group(splines[0], 70)) & 1).toBe(1);
  expect(tableRow(file.text, "LAYER", "WALL-OLD")).toBeUndefined();
  expect(tableRow(file.text, "LAYER", "NOTES")).toBeUndefined();
  expect(tableRow(file.text, "VIEW", "Column")).toBeDefined();

  // 7. Reopened: the view comes back and is gone to.
  await chooseFile(page, () => enter(page, "OPEN"), {
    name: "cleaned.dxf",
    mimeType: "application/dxf",
    buffer: Buffer.from(file.text),
  });
  const imported = logLines(page).filter({ hasText: "With " }).last();
  await expect(imported).toContainText("1 named view");
  await page.getByRole("button", { name: "Dismiss the import report" }).click();
  await enter(page, "VIEW", "R", "Column");
  await expect(logLines(page).last()).toHaveText('View "Column" restored.');
  await expect(logLines(page).filter({ hasText: "Unknown command" })).toHaveCount(0);
});

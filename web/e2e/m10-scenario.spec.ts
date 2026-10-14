// The M10 exit scenario (#202): a day's drafting — a polygon, divided, a
// block arrayed round it, properties matched, a length asked and set, the
// file purged and saved, and reopened with it all.
import { expect, test } from "@playwright/test";
import {
  chooseFile,
  clickWorld,
  downloadFrom,
  enter,
  entities,
  group,
  logLines,
  openApp,
  prompt,
  tableRow,
} from "./support";

test("the M10 scenario: a day's drafting with the daily commands, purged, saved, and reopened", async ({
  page,
}) => {
  const canvas = await openApp(page);
  await page.keyboard.press("F7");

  // 1. A hexagon, with twelve points along it.
  await enter(page, "POLYGON", "6", "0,0", "", "50");
  await expect(prompt(page)).toHaveText("Command:");
  await enter(page, "PTYPE", "X", "", "4", "DIVIDE");
  await clickWorld(page, canvas, 50, 0);
  await enter(page, "12");
  await expect(logLines(page).last()).toHaveText("Divided into 12 segments: 12 points placed.");

  // 2. A chair block, arrayed round the hexagon: six of them.
  await enter(page, "L", "100,0", "110,0", "", "C", "105,5", "3");
  await clickWorld(page, canvas, 95, 12);
  await clickWorld(page, canvas, 115, -5);
  await enter(page, "BLOCK", "CHAIR", "105,0");
  await clickWorld(page, canvas, 105, 0);
  await expect(page.getByTestId("selection-headline")).toHaveText("1 block reference");
  await enter(page, "ARRAY", "P", "0,0", "6", "", "");
  await expect(logLines(page).last()).toHaveText("6 items round 0,0, over 360°: 5 copies made.");

  // 3. A styled line, its properties matched onto the hexagon.
  await enter(page, "LAYER", "N", "WALLS", "S", "WALLS", "", "COLOR", "red");
  await enter(page, "L", "-200,-100", "-100,-100", "");
  await enter(page, "LAYER", "S", "0", "", "COLOR", "bylayer");
  await enter(page, "MATCHPROP");
  await clickWorld(page, canvas, -150, -100);
  await clickWorld(page, canvas, 50, 0);
  await enter(page, "");
  await expect(logLines(page).last()).toContainText("1 object matched");

  // 4. A length asked, then set: the styled line to 150.
  await enter(page, "DIST", "-200,-100", "-100,-100");
  await expect(logLines(page).last()).toHaveText(
    "Distance = 100.0000, Delta X = 100.0000, Delta Y = 0.0000, Angle = 0.0000°",
  );
  await enter(page, "LENGTHEN", "T", "150");
  await clickWorld(page, canvas, -105, -100);
  await enter(page, "");
  await enter(page, "LIST");
  await clickWorld(page, canvas, -150, -100);
  await enter(page, "");
  await expect(logLines(page).last()).toHaveText("  Angle: 0.0000°");

  // 5. An unused layer and a renamed one, then the file purged and saved.
  await enter(page, "LAYER", "N", "SPARE", "", "RENAME", "LA", "WALLS", "Partitions");
  await enter(page, "PURGE", "");
  await expect(logLines(page).last()).toHaveText("Purged 1 layer (SPARE).");
  const file = await downloadFrom(page, () => enter(page, "DXFOUT"));
  expect(entities(file.text, "POINT")).toHaveLength(12);
  expect(entities(file.text, "INSERT")).toHaveLength(6);
  expect(entities(file.text, "LWPOLYLINE")).toHaveLength(1);
  expect(group(entities(file.text, "LWPOLYLINE")[0], 8)).toBe("Partitions");
  expect(tableRow(file.text, "LAYER", "SPARE")).toBeUndefined();
  expect(tableRow(file.text, "LAYER", "Partitions")).toBeDefined();
  const lines = entities(file.text, "LINE");
  const styled = lines.find((line) => group(line, 8) === "Partitions");
  expect(Number(group(styled, 11))).toBeCloseTo(-50, 9);
  expect(file.text).toContain("\n  9\n$PDMODE\n 70\n3\n");

  // 6. Reopened: everything there, nothing to purge.
  await chooseFile(page, () => enter(page, "OPEN"), {
    name: "day.dxf",
    mimeType: "application/dxf",
    buffer: Buffer.from(file.text),
  });
  const imported = logLines(page).filter({ hasText: "Imported " });
  await expect(imported).toHaveCount(1);
  for (const part of ["1 line,", "1 polyline", "6 block references", "12 points"]) {
    await expect(imported).toContainText(part);
  }
  await page.getByRole("button", { name: "Dismiss the import report" }).click();
  await enter(page, "PURGE", "?");
  await expect(logLines(page).last()).toHaveText("Nothing to purge.");
  await page.keyboard.press("Escape");
  await expect(logLines(page).filter({ hasText: "Unknown command" })).toHaveCount(0);
});

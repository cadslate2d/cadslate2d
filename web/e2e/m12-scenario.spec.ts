// The M12 exit scenario (#217): a plan dimensioned with a continued chain
// and a baseline chain, the walls isolated by picking and the rest
// brought back, a door grouped and moved as one, a revision cloud round
// the change, a word replaced in every note, and the file saved and
// reopened with it all.
import { expect, test } from "@playwright/test";
import {
  chooseFile,
  clickWorld,
  downloadFrom,
  enter,
  entities,
  group,
  logLines,
  mapping,
  num,
  openApp,
  prompt,
} from "./support";

test("the M12 scenario: chains, layers by picking, a group, a cloud, and a word replaced, saved and reopened", async ({
  page,
}) => {
  const canvas = await openApp(page);
  await page.keyboard.press("F7");
  await page.keyboard.press("F3");

  // 1. A plan: the walls, a door — a leaf and its swing — and two notes.
  await enter(page, "LA", "N", "WALLS,DOORS,DIMS,NOTES", "S", "WALLS", "");
  await enter(page, "REC", "0,0", "120,80");
  await enter(page, "LA", "S", "DOORS", "", "L", "20,0", "20,20", "", "C", "20,0", "20");
  await enter(page, "LA", "S", "NOTES", "", "DT", "10,100", "5", "0", "Front door");
  await enter(page, "LEADER", "60,40", "70,60", "", "door to be replaced");

  // 2. Dimensioned end to end along the bottom, and from one baseline up the side.
  await enter(page, "LA", "S", "DIMS", "", "DLI", "0,0", "40,0", "20,-15");
  await enter(page, "DCO", "80,0", "120,0", "", "");
  await enter(page, "DLI", "0,0", "0,40", "-15,20");
  await enter(page, "DBA", "0,80", "", "");
  await expect(prompt(page)).toHaveText("Command:");

  // 3. The walls isolated by picking one, then the rest brought back.
  await enter(page, "LAYISO");
  await clickWorld(page, canvas, 60, 80);
  await enter(page, "");
  await expect(logLines(page).last()).toHaveText('Layer "WALLS" is isolated.');
  await enter(page, "SELECT", "ALL", "");
  await expect(logLines(page).last()).toHaveText("1 selected.");
  await page.keyboard.press("Escape");
  await enter(page, "LAYUNISO");
  await expect(logLines(page).last()).toHaveText("4 layers restored.");

  // 4. The door grouped, and moved along the wall as one.
  await clickWorld(page, canvas, 20, 10);
  await clickWorld(page, canvas, 34.14, 14.14);
  await enter(page, "G");
  await expect(logLines(page).last()).toHaveText("Made the unnamed group *A1 of 2 objects.");
  await clickWorld(page, canvas, 20, 10);
  await expect(page.getByTestId("selection-groups")).toHaveText("in group *A1");
  await enter(page, "M", "0,0", "30,0");

  // 5. A revision cloud round the change.
  await enter(page, "REVCLOUD", "25,-10", "80,40");
  await expect(logLines(page).last()).toHaveText("Revision cloud made: 16 arcs.");

  // 6. A word replaced in every note.
  await enter(page, "FIND", "door", "gate");
  await expect(logLines(page).last()).toHaveText("Replaced 2 times in 2 objects.");
  await expect(logLines(page).filter({ hasText: "Unknown command" })).toHaveCount(0);

  // 7. Saved with it all.
  const file = await downloadFrom(page, () => enter(page, "DXFOUT"));
  const dimensions = entities(file.text, "DIMENSION");
  expect(dimensions.map((dimension) => num(dimension, 42))).toEqual([40, 40, 40, 40, 80]);
  expect(dimensions.map((dimension) => group(dimension, 8))).toEqual(Array(5).fill("DIMS"));
  const lines = entities(file.text, "LINE");
  expect(lines.map((line) => [group(line, 8), group(line, 10)])).toEqual([["DOORS", "50.0"]]);
  expect(group(entities(file.text, "CIRCLE")[0], 10)).toBe("50.0");
  const groups = entities(file.text, "GROUP", "OBJECTS");
  expect(groups).toHaveLength(1);
  expect(file.text).toContain("\n  3\n*A1\n350\n");
  const polylines = entities(file.text, "LWPOLYLINE");
  expect(polylines.map((polyline) => group(polyline, 90))).toEqual(["4", "16"]);
  expect(group(entities(file.text, "TEXT")[0], 1)).toBe("Front gate");
  expect(entities(file.text, "MTEXT").map((note) => group(note, 1))).toEqual([
    "gate to be replaced",
  ]);

  // 8. Reopened: the chains, the group, and the notes as they were.
  await chooseFile(page, () => enter(page, "OPEN"), {
    name: "plan.dxf",
    mimeType: "application/dxf",
    buffer: Buffer.from(file.text),
  });
  const imported = logLines(page).filter({ hasText: "Imported " });
  await expect(imported).toHaveCount(1);
  for (const part of ["5 dimensions", "1 leader", "2 polylines", "1 circle"]) {
    await expect(imported).toContainText(part);
  }
  await expect(logLines(page).filter({ hasText: "1 group." })).toHaveCount(1);
  await page.getByRole("button", { name: "Dismiss the import report" }).click();
  await enter(page, "SELECT", "ALL", "");
  await expect(logLines(page).last()).toHaveText("11 selected.");
  await page.keyboard.press("Escape");
  // The view now fits the drawing: pick through its mapping.
  const screen = await mapping(page, canvas);
  const leaf = screen(50, 10);
  await page.mouse.click(leaf.x, leaf.y);
  await expect(page.getByTestId("selection-headline")).toHaveText("2 objects: 1 line, 1 circle");
});

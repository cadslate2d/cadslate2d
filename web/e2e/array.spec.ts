// ARRAY (#199): a block reference arrayed in rows and columns, every copy
// a reference of its own, written as INSERT records.
import { expect, test } from "@playwright/test";
import {
  clickWorld,
  downloadFrom,
  enter,
  entities,
  group,
  logLines,
  openApp,
  prompt,
} from "./support";

test("ARRAY lays a block out in rows and columns", async ({ page }) => {
  const canvas = await openApp(page);
  await page.keyboard.press("F7");
  await enter(page, "L", "0,0", "20,0", "", "C", "10,10", "5");
  await clickWorld(page, canvas, -10, 20);
  await clickWorld(page, canvas, 30, -10);
  // BLOCK leaves one reference in the selection's place.
  await enter(page, "BLOCK", "CHAIR", "0,0");
  await expect(prompt(page)).toHaveText("Command:");
  await clickWorld(page, canvas, 10, 0);
  await expect(page.getByTestId("selection-headline")).toHaveText("1 block reference");
  await enter(page, "ARRAY");
  await expect(prompt(page)).toHaveText(
    "Enter the type of array or [Rectangular/Polar] <Rectangular>:",
  );
  await enter(page, "R", "2", "3", "50", "60");
  await expect(logLines(page).last()).toHaveText("2 rows by 3 columns: 5 copies made.");
  await expect(prompt(page)).toHaveText("Command:");

  const file = await downloadFrom(page, () => enter(page, "DXFOUT"));
  const inserts = entities(file.text, "INSERT");
  expect(inserts.map((insert) => [group(insert, 2), group(insert, 10), group(insert, 20)])).toEqual(
    [
      ["CHAIR", "0.0", "0.0"],
      ["CHAIR", "60.0", "0.0"],
      ["CHAIR", "120.0", "0.0"],
      ["CHAIR", "0.0", "50.0"],
      ["CHAIR", "60.0", "50.0"],
      ["CHAIR", "120.0", "50.0"],
    ],
  );
  // One undo takes the five copies.
  await enter(page, "U");
  await enter(page, "SELECT", "ALL", "");
  await expect(logLines(page).last()).toHaveText("1 selected.");
});

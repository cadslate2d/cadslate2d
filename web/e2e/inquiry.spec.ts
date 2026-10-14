// Inquiry (#197): DIST and AREA answer in the log and change nothing.
import { expect, test } from "@playwright/test";
import { clickWorld, enter, logLines, openApp, prompt } from "./support";

test("DIST and AREA answer in the log and leave the drawing as it was", async ({ page }) => {
  const canvas = await openApp(page);
  await enter(page, "REC", "0,0", "100,50");
  await enter(page, "DIST");
  await expect(prompt(page)).toHaveText("Specify first point:");
  await enter(page, "0,0", "100,50");
  await expect(logLines(page).last()).toHaveText(
    "Distance = 111.8034, Delta X = 100.0000, Delta Y = 50.0000, Angle = 26.5651°",
  );
  await enter(page, "AREA", "O");
  await expect(prompt(page)).toHaveText("Select object:");
  await clickWorld(page, canvas, 50, 0);
  await expect(logLines(page).last()).toHaveText("Area = 5000.0000, Perimeter = 300.0000");
  await enter(page, "AA", "0,0", "10,0", "10,10", "");
  await expect(logLines(page).last()).toHaveText("Area = 50.0000, Perimeter = 34.1421");
  await expect(prompt(page)).toHaveText("Command:");
  // Nothing to undo but the rectangle: U takes it, and the next U has nothing.
  await enter(page, "U");
  await enter(page, "SELECT", "ALL", "");
  await expect(logLines(page).last()).toHaveText("0 selected.");
});

// Dimension chains (#212): a stepped outline dimensioned end to end with
// DIMCONTINUE above it and from one baseline with DIMBASELINE below it,
// written as six dimensions on their lines.
import { expect, test } from "@playwright/test";
import { downloadFrom, enter, entities, logLines, num, openApp, prompt } from "./support";

test("DIMCONTINUE and DIMBASELINE dimension a stepped outline both ways", async ({ page }) => {
  await openApp(page);
  await page.keyboard.press("F7");
  await enter(page, "PL", "0,0", "30,0", "30,10", "60,10", "60,20", "90,20", "90,30", "");

  // End to end above: one dimension, then two continued along its line.
  await enter(page, "DLI", "0,0", "30,10", "15,50");
  await enter(page, "DCO");
  await expect(prompt(page)).toHaveText(
    "Specify second extension line origin or [Select] <select>:",
  );
  await enter(page, "60,20", "90,30", "");
  await expect(prompt(page)).toHaveText("Select continued dimension:");
  await enter(page, "");

  // From one baseline below: one dimension, then two stacked out from it.
  await enter(page, "DLI", "0,0", "30,10", "15,-15");
  await enter(page, "DBA", "60,20", "90,30", "", "");
  await expect(prompt(page)).toHaveText("Command:");
  await expect(logLines(page).filter({ hasText: "Unknown command" })).toHaveCount(0);

  const file = await downloadFrom(page, () => enter(page, "DXFOUT"));
  const dimensions = entities(file.text, "DIMENSION");
  expect(dimensions.map((dimension) => num(dimension, 42))).toEqual([30, 30, 30, 30, 60, 90]);
  // The continued ones share the line at 50; the baseline ones step out
  // from -15 by the style's 3.75.
  expect(dimensions.map((dimension) => num(dimension, 20))).toEqual([
    50, 50, 50, -15, -18.75, -22.5,
  ]);
});

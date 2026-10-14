// The clipboard (#223): objects copied with Ctrl+C, another drawing
// opened, and pasted into it with Ctrl+V, their layer coming along.
import { readFileSync } from "node:fs";
import { expect, test } from "@playwright/test";
import {
  chooseFile,
  downloadFrom,
  enter,
  entities,
  fixturePath,
  group,
  logLines,
  openApp,
  prompt,
  tableRow,
} from "./support";

test("Ctrl+C copies, and Ctrl+V pastes into the next drawing opened", async ({ page }) => {
  await openApp(page);
  await page.keyboard.press("F7");
  await enter(page, "LA", "N", "NOTES", "C", "2", "NOTES", "S", "NOTES", "");
  await enter(page, "L", "0,0", "40,0", "", "C", "20,10", "5");
  await enter(page, "SELECT", "ALL", "");
  await page.keyboard.press("Control+c");
  await expect(logLines(page).last()).toHaveText("2 objects copied to the clipboard.");

  page.once("dialog", (dialog) => void dialog.accept());
  await chooseFile(page, () => enter(page, "OPEN"), {
    name: "layouts.dxf",
    mimeType: "application/dxf",
    buffer: readFileSync(fixturePath("valid/layouts.dxf")),
  });
  await page.getByRole("button", { name: "Dismiss the import report" }).click();
  await page.keyboard.press("Control+v");
  await expect(prompt(page)).toHaveText("Specify insertion point:");
  await enter(page, "200,100");
  await expect(logLines(page).last()).toHaveText("2 objects pasted.");

  const file = await downloadFrom(page, () => enter(page, "DXFOUT"));
  const pasted = entities(file.text, "LINE").filter((line) => group(line, 8) === "NOTES");
  expect(pasted.map((line) => [group(line, 10), group(line, 20)])).toEqual([["200.0", "100.0"]]);
  expect(group(tableRow(file.text, "LAYER", "NOTES"), 62)).toBe("2");
});

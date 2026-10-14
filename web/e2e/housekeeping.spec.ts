// PURGE and RENAME (#200): a file from another writer purged of its unused
// rows and saved without them; a layer renamed, its objects following.
import { readFileSync } from "node:fs";
import { expect, test } from "@playwright/test";
import {
  chooseFile,
  downloadFrom,
  enter,
  fixturePath,
  group,
  logLines,
  openApp,
  prompt,
  tableRow,
} from "./support";

test("PURGE removes the unused rows of a file and RENAME renames a layer, both saved", async ({
  page,
}) => {
  await openApp(page);
  await chooseFile(page, () => enter(page, "OPEN"), {
    name: "layers.dxf",
    mimeType: "application/dxf",
    buffer: readFileSync(fixturePath("valid/ezdxf-r12-layers-linetypes.dxf")),
  });
  await expect(logLines(page).filter({ hasText: "With 5 layers and 3 linetypes." })).toHaveCount(1);
  await page.getByRole("button", { name: "Dismiss the import report" }).click();
  await enter(page, "PURGE");
  await expect(prompt(page)).toHaveText(
    "Enter type of unused objects to purge or [Blocks/Dimstyles/LAyers/LTypes/STyles/All/?] <All>:",
  );
  await enter(page, "?");
  await expect(logLines(page).last()).toHaveText("Unused: 1 layer (Defpoints).");
  await enter(page, "");
  await expect(logLines(page).last()).toHaveText("Purged 1 layer (Defpoints).");
  await expect(prompt(page)).toHaveText("Command:");
  await enter(page, "RENAME", "LA", "WALLS", "Partitions");
  await expect(logLines(page).last()).toHaveText('Layer "WALLS" renamed "Partitions".');

  const file = await downloadFrom(page, () => enter(page, "DXFOUT"));
  expect(tableRow(file.text, "LAYER", "Defpoints")).toBeUndefined();
  expect(tableRow(file.text, "LAYER", "WALLS")).toBeUndefined();
  expect(tableRow(file.text, "LAYER", "Partitions")).toBeDefined();
  expect(tableRow(file.text, "LAYER", "AXES")).toBeDefined();
  expect(tableRow(file.text, "LTYPE", "DOT")).toBeDefined();
  // The objects that were on WALLS are on Partitions: the first line.
  const first = file.text.indexOf("\nLINE\n");
  const record = file.text.slice(first, file.text.indexOf("\n  0\n", first + 1));
  expect(record).toContain("\n  8\nPartitions\n");
  // One undo each: the rename, then the purge.
  await enter(page, "U", "U");
  await enter(page, "PURGE", "?");
  await expect(logLines(page).last()).toHaveText("Unused: 1 layer (Defpoints).");
  await page.keyboard.press("Escape");
  void group;
});

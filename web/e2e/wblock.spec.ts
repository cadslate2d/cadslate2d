// A block as a file and a file as a block (#222): a title block drawn in
// one drawing, written with WBLOCK, and inserted with INSERT ~ into
// another, its layer coming with it.
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

test("WBLOCK writes a selection as a file and INSERT ~ brings it into another drawing", async ({
  page,
}) => {
  await openApp(page);
  await page.keyboard.press("F7");
  await enter(page, "LA", "N", "FRAME", "C", "3", "FRAME", "S", "FRAME", "");
  await enter(page, "REC", "0,0", "80,20", "DT", "5,8", "5", "0", "Site plan");

  // The selection, about its corner, written as a file.
  await enter(page, "SELECT", "ALL", "");
  const block = await downloadFrom(page, () => enter(page, "WBLOCK", "0,0", "title-block"));
  expect(block.name).toBe("title-block.dxf");
  await expect(
    logLines(page).filter({ hasText: "Writing title-block.dxf: 2 objects." }),
  ).toHaveCount(1);
  expect(entities(block.text, "LWPOLYLINE")).toHaveLength(1);
  expect(group(entities(block.text, "TEXT")[0], 1)).toBe("Site plan");
  expect(group(tableRow(block.text, "LAYER", "FRAME"), 62)).toBe("3");

  // Another drawing, this one's changes let go, and the file inserted
  // into it as a block.
  page.once("dialog", (dialog) => void dialog.accept());
  await chooseFile(page, () => enter(page, "OPEN"), {
    name: "layouts.dxf",
    mimeType: "application/dxf",
    buffer: readFileSync(fixturePath("valid/layouts.dxf")),
  });
  await page.getByRole("button", { name: "Dismiss the import report" }).click();
  await chooseFile(page, () => enter(page, "INSERT", "~"), {
    name: block.name,
    mimeType: "application/dxf",
    buffer: Buffer.from(block.text),
  });
  await expect(
    logLines(page).filter({ hasText: 'Block "title-block" made from title-block.dxf: 2 objects.' }),
  ).toHaveCount(1);
  await expect(prompt(page)).toHaveText("Specify insertion point:");
  await enter(page, "200,0", "", "");
  await expect(prompt(page)).toHaveText("Command:");

  const file = await downloadFrom(page, () => enter(page, "DXFOUT"));
  const inserts = entities(file.text, "INSERT");
  expect(inserts.map((insert) => [group(insert, 2), group(insert, 10)])).toEqual([
    ["title-block", "200.0"],
  ]);
  expect(group(tableRow(file.text, "LAYER", "FRAME"), 62)).toBe("3");
});

// Layouts kept through a round trip (#184): a file with two layouts and a
// viewport opens without a loss, says what it kept, and saves with them.
import { readFileSync } from "node:fs";
import { expect, test } from "@playwright/test";
import { chooseFile, downloadFrom, enter, fixturePath, logLines, openApp } from "./support";

test("a file's layouts are kept, named, and written back", async ({ page }) => {
  await openApp(page);
  const text = readFileSync(fixturePath("valid/layouts.dxf"), "utf8");
  await chooseFile(page, () => enter(page, "OPEN"), {
    name: "layouts.dxf",
    mimeType: "application/dxf",
    buffer: Buffer.from(text),
  });
  await expect(logLines(page).filter({ hasText: "Imported 1 line." })).toHaveCount(1);
  await expect(
    logLines(page).filter({ hasText: "Kept 2 layouts, not drawn: Layout1, Layout2." }),
  ).toHaveCount(1);
  await expect(page.getByTestId("layouts")).toHaveText("2 layouts kept");
  await expect(page.getByTestId("lossy")).toHaveCount(0);

  const file = await downloadFrom(page, () => enter(page, "DXFOUT"));
  expect(file.text).toContain("\n  1\nLayout2\n");
  expect(file.text).toContain("\nVIEWPORT\n");
  expect(file.text).toContain("\n  2\n*Paper_Space0\n");
});

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
  await expect(logLines(page).filter({ hasText: "Imported 3 lines, 1 viewport." })).toHaveCount(1);
  await expect(logLines(page).filter({ hasText: "2 layouts: Layout1, Layout2." })).toHaveCount(1);
  await expect(page.getByTestId("layout-tab")).toHaveCount(3);
  await expect(page.getByTestId("lossy")).toHaveCount(0);

  const file = await downloadFrom(page, () => enter(page, "DXFOUT"));
  expect(file.text).toContain("\n  1\nLayout2\n");
  expect(file.text).toContain("\nVIEWPORT\n");
  expect(file.text).toContain("\n  2\n*Paper_Space0\n");
});

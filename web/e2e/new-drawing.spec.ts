// File › New (#245): an empty, untitled drawing, after asking whether the
// unsaved one may go.
import { expect, test } from "@playwright/test";
import { downloadFrom, enter, entities, logLines, openApp } from "./support";

test("NEW asks before discarding, then starts an empty untitled drawing", async ({ page }) => {
  await openApp(page);
  await enter(page, "L", "0,0", "100,0", "");
  await expect(page).toHaveTitle("• CadSlate2D");

  const asked: string[] = [];
  page.once("dialog", (dialog) => {
    asked.push(dialog.message());
    void dialog.dismiss();
  });
  await enter(page, "NEW");
  await expect
    .poll(() => asked)
    .toEqual(["The current drawing has changes that have not been exported. Discard them?"]);
  await expect(page).toHaveTitle("• CadSlate2D");

  page.once("dialog", (dialog) => void dialog.accept());
  await enter(page, "QNEW");
  await expect(page).toHaveTitle("CadSlate2D");
  await expect(logLines(page).filter({ hasText: "Unknown command" })).toHaveCount(0);

  const file = await downloadFrom(page, () => enter(page, "DXFOUT"));
  expect(entities(file.text)).toHaveLength(0);
});

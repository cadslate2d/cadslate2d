// Grid snap (#219): F9 and SNAP put the pointer on the snap grid, so a
// line is drawn from grid point to grid point without typing.
import { expect, test } from "@playwright/test";
import { clickWorld, downloadFrom, enter, entities, group, logLines, openApp } from "./support";

test("with SNAP on, the pointer draws a line from grid point to grid point", async ({ page }) => {
  const canvas = await openApp(page);
  await page.keyboard.press("F3");
  const snap = page.getByRole("button", { name: "SNAP", exact: true });
  await expect(snap).toHaveAttribute("aria-pressed", "false");
  await page.keyboard.press("F9");
  await expect(snap).toHaveAttribute("aria-pressed", "true");
  await expect(logLines(page).last()).toHaveText("<SNAP on>");
  await enter(page, "SNAP", "5");
  await expect(logLines(page).last()).toHaveText("Snap spacing: 5");

  await enter(page, "L");
  await clickWorld(page, canvas, 21.3, 28.6);
  await clickWorld(page, canvas, 58.2, -11.9);
  await enter(page, "");
  const file = await downloadFrom(page, () => enter(page, "DXFOUT"));
  const line = entities(file.text, "LINE")[0];
  expect([10, 20, 11, 21].map((code) => group(line, code))).toEqual([
    "20.0",
    "30.0",
    "60.0",
    "-10.0",
  ]);
});

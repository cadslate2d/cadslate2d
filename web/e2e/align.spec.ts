// ALIGN (#201): a block reference aligned onto a line by two pairs of
// points — moved and turned — and written where it landed.
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

test("ALIGN moves and turns a block onto a line", async ({ page }) => {
  const canvas = await openApp(page);
  await page.keyboard.press("F7");
  // The block: a line along X with a circle above it, placed at the origin.
  await enter(page, "L", "0,0", "20,0", "", "C", "10,5", "3");
  await clickWorld(page, canvas, -10, 20);
  await clickWorld(page, canvas, 30, -10);
  await enter(page, "BLOCK", "BAR", "0,0");
  await expect(prompt(page)).toHaveText("Command:");
  // The line to align onto: a diagonal far away.
  await enter(page, "L", "100,50", "160,110", "");
  await clickWorld(page, canvas, 10, 0);
  await expect(page.getByTestId("selection-headline")).toHaveText("1 block reference");
  await enter(page, "ALIGN");
  await expect(prompt(page)).toHaveText("Specify first source point:");
  await enter(page, "0,0", "100,50", "20,0", "160,110");
  await expect(prompt(page)).toHaveText("Scale objects based on alignment points? [Yes/No] <No>:");
  await enter(page, "");
  await expect(logLines(page).last()).toHaveText("1 object aligned.");

  const file = await downloadFrom(page, () => enter(page, "DXFOUT"));
  const inserts = entities(file.text, "INSERT");
  expect(inserts).toHaveLength(1);
  expect(Number(group(inserts[0], 10))).toBeCloseTo(100, 9);
  expect(Number(group(inserts[0], 20))).toBeCloseTo(50, 9);
  expect(Number(group(inserts[0], 50))).toBeCloseTo(45, 6);
  expect(Number(group(inserts[0], 41) ?? "1")).toBeCloseTo(1, 9);
  await enter(page, "U");
  const back = await downloadFrom(page, () => enter(page, "DXFOUT"));
  expect(group(entities(back.text, "INSERT")[0], 10)).toBe("0.0");
});

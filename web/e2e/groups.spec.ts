// Groups (#214): a door — a leaf and its swing — grouped, picked whole,
// and moved as one; group selection switched with Ctrl+Shift+A; the group
// written as a GROUP.
import { expect, test } from "@playwright/test";
import { clickWorld, downloadFrom, enter, entities, group, logLines, openApp } from "./support";

test("a door grouped is picked whole, moved as one, and saved as a group", async ({ page }) => {
  const canvas = await openApp(page);
  await page.keyboard.press("F7");
  await page.keyboard.press("F3");
  await enter(page, "L", "0,0", "0,60", "");
  await enter(page, "C", "0,0", "60");
  await enter(page, "L", "100,0", "150,0", "");

  // Grouped from a selection, unnamed.
  await clickWorld(page, canvas, 0, 30);
  await clickWorld(page, canvas, 42.43, 42.43);
  await enter(page, "G");
  await expect(logLines(page).last()).toHaveText("Made the unnamed group *A1 of 2 objects.");

  // One member picked: the door.
  await clickWorld(page, canvas, 0, 30);
  await expect(page.getByTestId("selection-headline")).toHaveText("2 objects: 1 line, 1 circle");
  await expect(page.getByTestId("selection-groups")).toHaveText("in group *A1");
  await page.keyboard.press("Escape");

  // Group selection off: the one object; on again.
  await page.keyboard.press("Control+Shift+A");
  await expect(logLines(page).last()).toHaveText("<Group selection off>");
  await clickWorld(page, canvas, 0, 30);
  await expect(page.getByTestId("selection-headline")).toHaveText("1 line");
  await page.keyboard.press("Escape");
  await page.keyboard.press("Control+Shift+A");
  await expect(logLines(page).last()).toHaveText("<Group selection on>");

  // Moved as one.
  await clickWorld(page, canvas, 0, 30);
  await enter(page, "M", "0,0", "-100,0");
  const file = await downloadFrom(page, () => enter(page, "DXFOUT"));
  const lines = entities(file.text, "LINE");
  expect(lines.map((line) => group(line, 10))).toEqual(["-100.0", "100.0"]);
  expect(group(entities(file.text, "CIRCLE")[0], 10)).toBe("-100.0");
  const groups = entities(file.text, "GROUP", "OBJECTS");
  expect(groups).toHaveLength(1);
  expect(group(groups[0], 70)).toBe("1");
  expect(file.text).toContain("\n  3\n*A1\n350\n");
});

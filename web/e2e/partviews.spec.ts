// PARTVIEWS (#339): a part's views from its JSON, from Draw › Orthographic Views...,
// hidden lines in their linetype, a section hatched, one undo.
import { expect, test } from "@playwright/test";
import {
  chooseMenu,
  downloadFrom,
  enter,
  entities,
  group,
  logLines,
  openApp,
  prompt,
  tableRow,
} from "./support";

const BRACKET = {
  profile: [
    [0, 0],
    [100, 0],
    [100, 20],
    [30, 20],
    [30, 60],
    [0, 60],
  ],
  depth: 40,
  holes: [{ axis: "z", at: [70, 20], diameter: 10 }],
};

test("a bracket's views, sectioned, from the menu", async ({ page }) => {
  await openApp(page);
  await chooseMenu(page, "Draw", "Orthographic Views...");
  await expect(prompt(page)).toHaveText("Describe the part or [Json/Select] <Json>:");
  await enter(page, "", JSON.stringify(BRACKET), "First", "Yes", "0,0");
  await expect(logLines(page).last()).toContainText(
    "Part views drawn as PART-1: front, top, and right side, first-angle.",
  );
  const file = await downloadFrom(page, () => enter(page, "DXFOUT"));
  const on = (layer: string) => entities(file.text).filter((entity) => group(entity, 8) === layer);
  expect(on("PART-HATCH").filter((e) => e.type === "HATCH").length).toBeGreaterThanOrEqual(2);
  expect(on("PART-HIDDEN").length).toBeGreaterThan(0);
  expect(group(tableRow(file.text, "LAYER", "PART-HIDDEN"), 6)).toBe("Hidden");
  await enter(page, "U");
  const after = await downloadFrom(page, () => enter(page, "DXFOUT"));
  expect(entities(after.text).filter((e) => group(e, 8) === "PART-HATCH")).toHaveLength(0);
});

// #340: a title block on a layout, from Insert › Title Block....
test("a title block on a layout, from the menu", async ({ page }) => {
  await openApp(page);
  await enter(page, "LAYOUT", "N", "Sheet");
  await chooseMenu(page, "Insert", "Title Block...");
  await expect(prompt(page)).toHaveText("Paper size? [A4/A3/A2/A1/A0] <A3>:");
  await enter(page, "A3", "Landscape", "BRACKET", "M-7", "", "", "", "First", "");
  await expect(logLines(page).last()).toContainText("A3 landscape sheet drawn as TITLE-1");
  const file = await downloadFrom(page, () => enter(page, "DXFOUT"));
  const titles = entities(file.text, "TEXT", "BLOCKS").concat(entities(file.text, "TEXT"));
  expect(titles.map((e) => group(e, 1))).toContain("BRACKET");
});

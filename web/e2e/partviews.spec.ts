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

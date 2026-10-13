// Block attributes (#183): a definition made with ATTDEF, put in a block,
// asked for on INSERT, shown as its value, edited with ATTEDIT, and saved
// as an ATTRIB beside its INSERT.
import { expect, test } from "@playwright/test";
import {
  clickWorld,
  downloadFrom,
  enter,
  entities,
  group,
  litPixelsAround,
  openApp,
  prompt,
} from "./support";

test("an attribute is defined, asked for, shown, edited, and saved", async ({ page }) => {
  const canvas = await openApp(page);
  await page.keyboard.press("F7");
  await enter(page, "L", "0,0", "60,0", "");
  await enter(page, "ATTDEF", "ROOM", "Room number", "000", "5,5", "10", "");
  await expect(prompt(page)).toHaveText("Command:");
  await page.mouse.move(5, 5);
  // On its own, the definition shows its tag.
  expect(
    await litPixelsAround(page, canvas, { x: 20, y: 9 }, { width: 20, height: 7 }),
  ).toBeGreaterThan(20);

  // A window over the line and the definition, then BLOCK on the selection.
  await clickWorld(page, canvas, -10, 25);
  await clickWorld(page, canvas, 70, -10);
  await enter(page, "BLOCK", "TAG", "0,0");
  await expect(prompt(page)).toHaveText("Command:");
  await enter(page, "INSERT", "TAG", "0,-100", "", "");
  await expect(prompt(page)).toHaveText("Room number <000>:");
  await enter(page, "101");
  await expect(prompt(page)).toHaveText("Command:");
  await page.mouse.move(5, 5);
  expect(
    await litPixelsAround(page, canvas, { x: 20, y: -91 }, { width: 20, height: 7 }),
  ).toBeGreaterThan(20);

  await clickWorld(page, canvas, 30, -100);
  await expect(page.getByTestId("selection-headline")).toContainText("block reference");
  await expect(page.getByRole("complementary", { name: "Properties" })).toContainText("101");
  await page.keyboard.press("Escape");

  await enter(page, "ATTEDIT");
  await clickWorld(page, canvas, 30, -100);
  await expect(prompt(page)).toHaveText("Room number <101>:");
  await enter(page, "202");
  await expect(prompt(page)).toHaveText("Command:");

  const file = await downloadFrom(page, () => enter(page, "DXFOUT"));
  const attribs = entities(file.text, "ATTRIB");
  expect(attribs.length).toBeGreaterThanOrEqual(1);
  expect(attribs.map((record) => [group(record, 2), group(record, 1)])).toContainEqual([
    "ROOM",
    "202",
  ]);
  expect(entities(file.text, "ATTDEF", "BLOCKS")).toHaveLength(1);
});

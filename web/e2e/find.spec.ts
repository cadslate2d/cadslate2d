// FIND (#216): a word replaced in a text, a paragraph, and a leader's
// note at once; the paragraph's break written as its code, untouched.
import { expect, test, type Page } from "@playwright/test";
import { downloadFrom, enter, entities, group, logLines, openApp } from "./support";

const editor = (page: Page) => page.getByRole("dialog", { name: "Text editor" });
const field = (page: Page) => page.getByRole("textbox", { name: "Paragraph" });

test("FIND replaces a word in a text, a paragraph, and a leader note, as one undo", async ({
  page,
}) => {
  await openApp(page);
  await enter(page, "DT", "-100,60", "5", "0", "Door to keep");
  await enter(page, "MTEXT", "-100,40", "60,-40");
  await field(page).fill("New door\nthe door frame");
  await editor(page).getByRole("button", { name: "OK" }).click();
  await expect(editor(page)).toHaveCount(0);
  await enter(page, "LEADER", "80,0", "100,30", "", "Fire door");

  await enter(page, "FIND", "door", "gate");
  await expect(logLines(page).last()).toHaveText("Replaced 4 times in 3 objects.");
  let file = await downloadFrom(page, () => enter(page, "DXFOUT"));
  expect(group(entities(file.text, "TEXT")[0], 1)).toBe("gate to keep");
  const paragraphs = entities(file.text, "MTEXT");
  expect(paragraphs.map((paragraph) => group(paragraph, 1))).toEqual([
    "New gate\\Pthe gate frame",
    "Fire gate",
  ]);

  // One undo puts every word back.
  await enter(page, "U");
  file = await downloadFrom(page, () => enter(page, "DXFOUT"));
  expect(group(entities(file.text, "TEXT")[0], 1)).toBe("Door to keep");
  await expect(logLines(page).filter({ hasText: "Unknown command" })).toHaveCount(0);
});

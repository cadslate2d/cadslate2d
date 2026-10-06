// MTEXT and the paragraph editor (#144): two corners, then the words in a
// small editor whose result goes through the command line.
import { expect, test, type Page } from "@playwright/test";
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

const editor = (page: Page) => page.getByRole("dialog", { name: "Text editor" });
const field = (page: Page) => page.getByRole("textbox", { name: "Paragraph" });

test.describe("MTEXT", () => {
  test("two corners open the editor; Ctrl+Enter sets the paragraph; it exports as lines", async ({
    page,
  }) => {
    const canvas = await openApp(page);
    await expect(editor(page)).toHaveCount(0);
    await enter(page, "MTEXT", "H", "8", "-100,40", "60,-40");
    await expect(prompt(page)).toHaveText("Enter text in the editor:");
    await expect(editor(page)).toBeVisible();
    await expect(field(page)).toBeFocused();

    // Enter inside the editor breaks the line rather than submitting.
    await field(page).pressSequentially("Site plan");
    await page.keyboard.press("Enter");
    await field(page).pressSequentially("scale one to fifty");
    await expect(prompt(page)).toHaveText("Enter text in the editor:");
    await page.keyboard.press("Control+Enter");
    await expect(editor(page)).toHaveCount(0);
    await expect(prompt(page)).toHaveText("Command:");
    await expect(logLines(page).last()).toHaveText(
      "Enter text in the editor: Site plan ⏎ scale one to fifty",
    );

    // Picked by its box; the panel knows it; the export has one TEXT per line.
    await clickWorld(page, canvas, -90, 30);
    await expect(page.getByTestId("selection-headline")).toHaveText("1 multiline text");
    await page.keyboard.press("Escape");
    const file = await downloadFrom(page, () => enter(page, "DXFOUT"));
    // Written as one MTEXT, its break as a code (ADR-0018).
    const paragraphs = entities(file.text, "MTEXT");
    expect(paragraphs).toHaveLength(1);
    expect(group(paragraphs[0], 1)).toBe("Site plan\\Pscale one to fifty");
    expect(group(paragraphs[0], 40)).toBe("8.0");
  });

  test("TEXTEDIT reopens the editor with the words; Escape and Cancel keep them", async ({
    page,
  }) => {
    const canvas = await openApp(page);
    await enter(page, "MTEXT", "-100,40", "60,-40");
    await field(page).fill("one\ntwo");
    await editor(page).getByRole("button", { name: "OK" }).click();
    await expect(editor(page)).toHaveCount(0);

    await enter(page, "TEXTEDIT");
    await clickWorld(page, canvas, -90, 35);
    await expect(prompt(page)).toHaveText("Edit the text in the editor:");
    await expect(field(page)).toHaveValue("one\ntwo");
    await field(page).fill("one\ntwo\nthree");
    await editor(page).getByRole("button", { name: "Cancel" }).click();
    await expect(editor(page)).toHaveCount(0);
    await expect(prompt(page)).toHaveText("Command:");

    await enter(page, "TEXTEDIT");
    await clickWorld(page, canvas, -90, 35);
    await field(page).fill("changed");
    await page.keyboard.press("Escape");
    await expect(editor(page)).toHaveCount(0);

    const file = await downloadFrom(page, () => enter(page, "DXFOUT"));
    expect(group(entities(file.text, "MTEXT")[0], 1)).toBe("one\\Ptwo");
    expect(file.text).not.toContain("three");
    expect(file.text).not.toContain("changed");
  });
});

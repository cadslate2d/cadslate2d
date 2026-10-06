// Saving to a file where the browser allows it (DESIGN §25.3; #159). The
// File System Access pickers cannot be driven headless, so the harness fakes
// them in Chromium: files live in the page, and the test reads them back.
// Firefox has no such API; its saves are downloads (`files.spec.ts`).
import { expect, test } from "@playwright/test";
import { enter, fakeFiles, lines, logLines, openApp, prompt } from "./support";

test.describe("file handles", () => {
  test("SAVE asks once, then writes back to the same file; SAVEAS asks again", async ({
    page,
    browserName,
  }) => {
    test.skip(browserName !== "chromium", "the File System Access API is Chromium's");
    await openApp(page, { fileSystemAccess: "fake" });
    let downloads = 0;
    page.on("download", () => (downloads += 1));

    await enter(page, "L", "0,0", "100,50", "");
    await expect(page.getByTestId("document")).toHaveText("Untitled •");
    await expect(page).toHaveTitle("• CadSlate2D");

    // The first save has no file yet: the picker is shown once.
    await page.keyboard.press("Control+s");
    await expect(logLines(page).last()).toHaveText("Drawing exported as DXF.");
    let store = await fakeFiles(page);
    expect(store.pickers).toEqual({ save: 1, open: 0 });
    expect(lines(store.files["drawing.dxf"] ?? "")).toEqual([[0, 0, 100, 50]]);
    await expect(page.getByTestId("document")).toHaveText("drawing.dxf");
    await expect(page).toHaveTitle("drawing.dxf — CadSlate2D");

    // A change, and SAVE again: written back, no dialog.
    await enter(page, "C", "50,50", "10");
    await expect(page.getByTestId("document")).toHaveText("drawing.dxf •");
    await enter(page, "SAVE");
    await expect(prompt(page)).toHaveText("Command:");
    store = await fakeFiles(page);
    expect(store.pickers.save).toBe(1);
    expect(store.files["drawing.dxf"]).toContain("AcDbCircle");
    await expect(page.getByTestId("document")).toHaveText("drawing.dxf");

    // SAVEAS asks where even with a file.
    await enter(page, "SAVEAS");
    await expect(prompt(page)).toHaveText("Command:");
    store = await fakeFiles(page);
    expect(store.pickers.save).toBe(2);
    expect(downloads).toBe(0);
  });

  // A real handle rides in the autosave record (it survives the structured
  // clone); the faked ones here are plain objects and cannot, so this checks
  // the name, and that the drawing comes back under it.
  test("a recovered drawing remembers its file's name", async ({ page, browserName }) => {
    test.skip(browserName !== "chromium", "the File System Access API is Chromium's");
    await openApp(page, { fileSystemAccess: "fake" });
    await enter(page, "L", "0,0", "100,50", "");
    await page.keyboard.press("Control+s");
    await expect(page.getByTestId("document")).toHaveText("drawing.dxf");
    await enter(page, "C", "50,50", "10");
    await page.waitForTimeout(1500);
    page.once("dialog", (dialog) => void dialog.accept());
    await page.reload();
    await openApp(page, { fileSystemAccess: "fake" });
    const offer = page.getByTestId("recovery");
    await expect(offer).toContainText("drawing.dxf, kept at");
    await offer.getByRole("button", { name: "Recover" }).click();
    await expect(page.getByTestId("document")).toHaveText("drawing.dxf");
  });
});

// Autosave and recovery (DESIGN §25; ADR-0019; #158): the drawing is kept
// in the browser after a change and offered back after a reload; a drawing
// that was exported is not; and nothing leaves the browser.
import { expect, test } from "@playwright/test";
import { downloadFrom, enter, lines, openApp, prompt, recordRequests } from "./support";

const ONE_LINE = [[0, 0, 100, 50]];

test.describe("autosave", () => {
  test("a drawing drawn and not exported is offered back after a reload, whole", async ({
    page,
  }) => {
    await openApp(page);
    const requests = recordRequests(page);
    await enter(page, "L", "0,0", "100,50", "");
    await expect(prompt(page)).toHaveText("Command:");
    // The autosave follows the last change by a pause.
    await page.waitForTimeout(1500);

    // Leaving with unexported changes asks; the test says yes.
    page.once("dialog", (dialog) => void dialog.accept());
    await page.reload();
    await openApp(page);
    const offer = page.getByTestId("recovery");
    await expect(offer).toContainText("An unnamed drawing, kept at");
    await offer.getByRole("button", { name: "Recover" }).click();
    await expect(offer).toHaveCount(0);

    const file = await downloadFrom(page, () => enter(page, "DXFOUT"));
    expect(lines(file.text)).toEqual(ONE_LINE);
    expect(file.name).toBe("recovered.dxf");
    // Nothing left the browser: the reload fetched the app's own files from
    // where the page came from, and nothing else.
    const origin = new URL(page.url()).origin;
    const foreign = requests.filter(
      (request) => !request.url.startsWith("blob:") && new URL(request.url).origin !== origin,
    );
    expect(foreign).toEqual([]);
  });

  test("a drawing that was exported is not offered", async ({ page }) => {
    await openApp(page);
    await enter(page, "L", "0,0", "100,50", "");
    await downloadFrom(page, () => enter(page, "DXFOUT"));
    await page.waitForTimeout(1500);
    await page.reload();
    await openApp(page);
    await page.waitForTimeout(300);
    await expect(page.getByTestId("recovery")).toHaveCount(0);
  });

  test("a kept drawing can be discarded, after which nothing is offered", async ({ page }) => {
    await openApp(page);
    await enter(page, "C", "0,0", "10");
    await page.waitForTimeout(1500);
    page.once("dialog", (dialog) => void dialog.accept());
    await page.reload();
    await openApp(page);
    const offer = page.getByTestId("recovery");
    await offer.getByRole("button", { name: "Discard" }).click();
    await expect(offer).toHaveCount(0);
    await page.reload();
    await openApp(page);
    await page.waitForTimeout(300);
    await expect(page.getByTestId("recovery")).toHaveCount(0);
  });
});

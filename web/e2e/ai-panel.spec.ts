// The AI panel (#260), against the stand-in service of ai-fake.ts and real
// recorded model answers: signing in by an emailed link, a request and its
// preview, Accept as one undo, Reject, Revise, the consent for drawing
// context, the allowance, signing out and deleting the account — and
// everything else working signed out.
import { expect, test, type Page } from "@playwright/test";
import { fakeAiService, type FakeAiService } from "./ai-fake";
import { chooseMenu, enter, openApp, rendererStats } from "./support";

/** After a page load: the canvas, then its first frame. */
async function loaded(page: Page) {
  await expect(page.getByRole("region", { name: "Model space" }).locator("canvas")).toBeVisible();
  await expect.poll(async () => (await rendererStats(page)).frames).toBeGreaterThan(0);
}

const CIRCLE = "Draw a circle of radius 25 centred at 100,100";
const MOVE = "Move these 25 mm up";
const IMPOSSIBLE = "Extrude this into a 3D box and render it";

const panel = (page: Page) => page.getByTestId("ai-panel");

/** What ALL selects, as the Properties panel says it; then nothing is selected. */
async function contents(page: Page): Promise<string> {
  await enter(page, "ALL");
  const headline = (await page.getByTestId("selection-headline").textContent()) ?? "";
  // The first Escape ends ALL's selection prompt, the second clears the selection.
  await page.keyboard.press("Escape");
  await page.keyboard.press("Escape");
  return headline;
}

/** Asks for a link, then opens the app at it, as the email's link does. */
async function signIn(page: Page, service: FakeAiService, email = "drafter@example.com") {
  await chooseMenu(page, "View", "AI Assistant");
  await panel(page).getByLabel("Email").fill(email);
  await panel(page).getByRole("button", { name: "Email me a sign-in link" }).click();
  await expect(panel(page).getByTestId("ai-link-sent")).toContainText(email);
  // The email's link opens the app afresh, as a new page load.
  await page.goto("about:blank");
  await page.goto(`./?debug#ai-sign-in=${service.tokenFor(email)}`);
  await loaded(page);
  await expect(panel(page).getByTestId("ai-email")).toHaveText(email);
  // The token is spent and taken off the address bar.
  expect(page.url()).not.toContain("ai-sign-in");
}

async function ask(page: Page, prompt: string) {
  await panel(page)
    .getByLabel(/What should be drawn or changed\?|How should it change\?/)
    .fill(prompt);
  await panel(page).getByRole("button", { name: "Ask" }).click();
}

test("a request is previewed, accepted as one undo, rejected, and revised", async ({ page }) => {
  const service = await fakeAiService(page);
  await openApp(page);
  await signIn(page, service);
  await expect(panel(page).getByTestId("ai-allowance")).toContainText("100 of 100 left");

  // The proposal is on the canvas; the drawing is unchanged until Accept.
  await ask(page, CIRCLE);
  const proposal = panel(page).getByTestId("ai-proposal");
  await expect(proposal).toContainText("Adds 1 object");
  await expect(panel(page).getByTestId("ai-turns")).toContainText("circle of radius 25");
  await expect(panel(page).getByTestId("ai-allowance")).toContainText("99 of 100 left");
  expect(await contents(page)).toBe("No selection");
  await proposal.getByRole("button", { name: "Accept" }).click();
  await expect(proposal).toHaveCount(0);
  expect(await contents(page)).toBe("1 circle");
  // One U takes it back.
  await enter(page, "U");
  expect(await contents(page)).toBe("No selection");

  // Rejected: nothing changes.
  await ask(page, CIRCLE);
  await proposal.getByRole("button", { name: "Reject" }).click();
  await expect(proposal).toHaveCount(0);
  expect(await contents(page)).toBe("No selection");

  // Revised: the proposal stays while the next request refines it, which
  // is sent as a revision of the first.
  await ask(page, CIRCLE);
  await proposal.getByRole("button", { name: "Revise" }).click();
  await expect(panel(page).getByTestId("ai-revising")).toBeVisible();
  await expect(proposal).toContainText("Adds 1 object");
  await ask(page, CIRCLE);
  expect(service.interpreted.at(-1)?.prompt).toBe(
    `Revise the previous request "${CIRCLE}" as follows: ${CIRCLE}`,
  );
  await proposal.getByRole("button", { name: "Accept" }).click();
  expect(await contents(page)).toBe("1 circle");
  await expect
    .poll(() => service.outcomes.map(([, outcome]) => outcome))
    .toEqual(["accepted", "rejected", "rejected", "accepted"]);

  // A request the operations cannot do is answered, and proposes nothing.
  await ask(page, IMPOSSIBLE);
  await expect(panel(page).getByTestId("ai-turns")).toContainText("2D drawing operations only");
  await expect(proposal).toHaveCount(0);
});

test("drawing context is sent only when the user agrees, for that request", async ({ page }) => {
  const service = await fakeAiService(page);
  await openApp(page);
  await signIn(page, service);
  await enter(page, "C", "30,30", "8", "C", "70,30", "8", "ALL");

  // Asked with a selection: the user sees what would go, and nothing has.
  await ask(page, MOVE);
  const consent = panel(page).getByTestId("ai-consent");
  await expect(consent).toContainText("circle");
  await expect(consent).toContainText("radius 8.0000");
  expect(service.interpreted).toEqual([]);

  // Cancelled: nothing at all is sent.
  await consent.getByRole("button", { name: "Cancel" }).click();
  await expect(consent).toHaveCount(0);
  expect(service.interpreted).toEqual([]);

  // Without: the request alone.
  await ask(page, MOVE);
  await consent.getByRole("button", { name: "Send without" }).click();
  await expect.poll(() => service.interpreted.length).toBe(1);
  expect(service.interpreted[0]).toEqual({ prompt: MOVE });

  // With: the selection as the panel described it, and the layers.
  await panel(page).getByTestId("ai-proposal").getByRole("button", { name: "Reject" }).click();
  await ask(page, MOVE);
  await consent.getByRole("button", { name: "Send with these" }).click();
  await expect.poll(() => service.interpreted.length).toBe(2);
  const context = service.interpreted[1]?.context;
  expect(context?.layers).toEqual(["0"]);
  expect(context?.objects.map((object) => object.kind)).toEqual(["circle", "circle"]);
  expect(context?.objects[0]?.geometry).toContain("radius 8.0000");

  // No selection: no question, and no context.
  await panel(page).getByTestId("ai-proposal").getByRole("button", { name: "Reject" }).click();
  await page.keyboard.press("Escape");
  await page.keyboard.press("Escape");
  await ask(page, CIRCLE);
  await expect.poll(() => service.interpreted.length).toBe(3);
  expect(service.interpreted[2]?.context).toBeUndefined();
});

test("the allowance shows what remains, and at the limit what to do", async ({ page }) => {
  const service = await fakeAiService(page);
  service.allowance = 1;
  await openApp(page);
  await signIn(page, service);
  await expect(panel(page).getByTestId("ai-allowance")).toContainText("1 of 1 left");
  await ask(page, CIRCLE);
  await expect(panel(page).getByTestId("ai-allowance")).toContainText("0 of 1 left");
  // The prompt is closed at the limit; the panel says when it comes back.
  await expect(panel(page).getByRole("button", { name: "Ask" })).toBeDisabled();
  await expect(panel(page).getByTestId("ai-limit")).toContainText("used this month's 1 AI actions");
  await expect(panel(page).getByTestId("ai-limit")).toContainText(/1 November|November 1/);
  service.used = 0;
  service.allowance = 0;
  await page.reload();
  await loaded(page);
  await expect(panel(page).getByTestId("ai-allowance")).toContainText("0 of 0 left");
  await expect(panel(page).getByTestId("ai-allowance")).toContainText(
    /resets (1 November|November 1)/,
  );
});

test("signing out and deleting the account; everything else works signed out", async ({ page }) => {
  const service = await fakeAiService(page);
  await openApp(page);

  // Signed out, the panel offers sign-in and says what is sent and kept;
  // drafting is untouched.
  await chooseMenu(page, "View", "AI Assistant");
  await expect(panel(page).getByTestId("ai-sign-in")).toBeVisible();
  await expect(panel(page).getByTestId("ai-privacy")).toHaveAttribute(
    "href",
    "https://ai-service.e2e.invalid/privacy",
  );
  await enter(page, "L", "0,0", "100,0", "", "C", "50,50", "10");
  expect(await contents(page)).toBe("2 objects: 1 line, 1 circle");
  await enter(page, "U");
  expect(await contents(page)).toBe("1 line");
  await chooseMenu(page, "View", "AI Assistant");
  await expect(panel(page)).toHaveCount(0);
  expect(service.interpreted).toEqual([]);

  // Signed in, then out.
  await signIn(page, service);
  await expect(panel(page).getByTestId("ai-privacy")).toBeVisible();
  await panel(page).getByRole("button", { name: "Sign out" }).click();
  await expect(panel(page).getByTestId("ai-sign-in")).toBeVisible();
  expect(await page.evaluate(() => localStorage.getItem("cadslate2d.ai.session"))).toBeNull();

  // Signed in again, and the account deleted after a second click.
  await chooseMenu(page, "View", "AI Assistant");
  await signIn(page, service);
  expect(service.accounts.has("drafter@example.com")).toBe(true);
  await panel(page).getByRole("button", { name: "Delete account" }).click();
  await expect(panel(page).getByTestId("ai-delete-confirm")).toBeVisible();
  await panel(page).getByRole("button", { name: "Delete", exact: true }).click();
  await expect(panel(page).getByTestId("ai-sign-in")).toBeVisible();
  expect(service.accounts.has("drafter@example.com")).toBe(false);
  // The drawing never noticed.
  expect(await contents(page)).toBe("No selection");
});

test("the panel with a proposal", async ({ page, browserName }) => {
  test.skip(browserName !== "chromium", "visual baselines are compared in Chromium only");
  const service = await fakeAiService(page);
  await openApp(page);
  // The AI panel alone in the column, as a drafter talking to it would have it.
  await chooseMenu(page, "View", "Layers Panel");
  await chooseMenu(page, "View", "Properties Panel");
  await signIn(page, service);
  await ask(page, CIRCLE);
  await expect(panel(page).getByTestId("ai-proposal")).toBeVisible();
  await expect(panel(page)).toHaveScreenshot("ai-panel-proposal.png");
});

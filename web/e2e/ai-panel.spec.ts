// The AI panel (#260, #261, #274, #282), against the stand-in service of
// ai-fake.ts and the corpus's recorded model answers: signing in by an
// emailed link, a request and its preview, Accept as one undo, Reject,
// Revise, the consent for drawing context, a creation and a selection edit
// as they land in the drawing, a part made and then widened by its
// parameter, a refusal explained, the allowance, signing out and deleting
// the account — and everything else working signed out.
import { expect, test, type Page } from "@playwright/test";
import { fakeAiService, type FakeAiService } from "./ai-fake";
import {
  chooseMenu,
  downloadFrom,
  enter,
  entities,
  group,
  num,
  openApp,
  rendererStats,
} from "./support";

/** After a page load: the canvas, then its first frame. */
async function loaded(page: Page) {
  await expect(page.getByRole("region", { name: "Model space" }).locator("canvas")).toBeVisible();
  await expect.poll(async () => (await rendererStats(page)).frames).toBeGreaterThan(0);
}

const CIRCLE = "Draw a circle of radius 25 centred at 100,100";
const BIGGER = "Make it radius 40 instead";
const MOVE = "Move these 25 mm up";
const IMPOSSIBLE = "Extrude this into a 3D box and render it";
const PLATE =
  "A 120 by 80 rectangle with its lower left corner at the origin, and a hole of diameter 10 at each corner, 10 in from both edges";
const HOLES = "Make these holes diameter 6";
const TOO_BIG = "Fillet these lines with a radius of 50";
const HOUSE =
  "A two-bedroom house on a 12 × 20 m plot with a 5 m front setback: living room 25 m², kitchen 10 m², bedrooms of 12 and 10 m², bathroom 5 m²; the kitchen opens onto the living room";
const BIGGER_KITCHEN = "Make the kitchen 14 m²";
const PART =
  "Make a 120 × 80 mm plate with its lower left corner at the origin and four Ø8 holes 10 mm in from the corners";
const WIDER = "Make this plate 20 mm wider without moving the holes relative to the edges";

/** The lines of the drawing as DXFOUT writes them: the x of each end. */
async function lineXs(page: Page): Promise<number[]> {
  const file = await downloadFrom(page, () => enter(page, "DXFOUT"));
  return entities(file.text, "LINE").flatMap((line) => [num(line, 10), num(line, 11)]);
}

/** The circles of the drawing as DXFOUT writes them: centre and radius. */
async function circles(page: Page): Promise<[number, number, number][]> {
  const file = await downloadFrom(page, () => enter(page, "DXFOUT"));
  return entities(file.text, "CIRCLE")
    .map((circle): [number, number, number] => [num(circle, 10), num(circle, 20), num(circle, 40)])
    .sort((a, b) => a[0] - b[0] || a[1] - b[1]);
}

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
  // Open from the start in a build with the service.
  await expect(panel(page)).toBeVisible();
  await panel(page).getByLabel("Email").fill(email);
  await panel(page).getByRole("button", { name: "Email me a sign-in link" }).click();
  await expect(panel(page).getByTestId("ai-link-sent")).toContainText(email);
  // The email's link opens the app afresh, as a new page load.
  await page.goto("about:blank");
  await page.goto(`./?debug#ai-sign-in=${service.tokenFor(email)}`);
  await loaded(page);
  await expect(page.getByTestId("ai-email")).toHaveText(email);
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
  service.plan = "pro"; // revisions and floor plans through the AI are Pro (ADR-0002)
  await openApp(page);
  await signIn(page, service);
  await expect(panel(page).getByTestId("ai-allowance")).toContainText("Pro · 2000 of 2000 left");

  // The proposal is on the canvas; the drawing is unchanged until Accept.
  await ask(page, CIRCLE);
  const proposal = panel(page).getByTestId("ai-proposal");
  await expect(proposal).toContainText("Adds 1 object");
  await expect(panel(page).getByTestId("ai-turns")).toContainText("circle of radius 25");
  await expect(panel(page).getByTestId("ai-allowance")).toContainText("1999 of 2000 left");
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
  // goes with the proposal's request and document, and replaces it.
  await ask(page, CIRCLE);
  await proposal.getByRole("button", { name: "Revise" }).click();
  await expect(panel(page).getByTestId("ai-revising")).toBeVisible();
  await expect(proposal).toContainText("Adds 1 object");
  await ask(page, BIGGER);
  await expect(panel(page).getByTestId("ai-turns")).toContainText("40");
  expect(service.interpreted.at(-1)).toMatchObject({
    prompt: BIGGER,
    previous: {
      prompt: CIRCLE,
      intent: { operations: [{ op: "circle", center: [100, 100], radius: 25 }] },
    },
  });
  await proposal.getByRole("button", { name: "Accept" }).click();
  expect(await contents(page)).toBe("1 circle");
  expect(await circles(page)).toEqual([[100, 100, 40]]);
  await expect
    .poll(() => service.outcomes.map(([, outcome]) => outcome))
    .toEqual(["accepted", "rejected", "rejected", "accepted"]);

  // A request the operations cannot do is answered, and proposes nothing.
  await ask(page, IMPOSSIBLE);
  await expect(panel(page).getByTestId("ai-turns").locator("li").last()).toContainText("2D");
  await expect(proposal).toHaveCount(0);
});

test("drawing context is sent only when the user agrees, for that request", async ({ page }) => {
  const service = await fakeAiService(page);
  await openApp(page);
  // A long address beside Layers and Properties: the exchange keeps room
  // for the proposal's buttons (the M20 scenario found it shrunk to nothing).
  await signIn(page, service, "a-drafter-with-a-rather-long-address@example.com");
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

test("a creation lands in the drawing as the model proposed it", async ({ page }) => {
  const service = await fakeAiService(page);
  await openApp(page);
  await signIn(page, service);
  // The model answers with the plate part: four sides and four holes.
  await ask(page, PLATE);
  const proposal = panel(page).getByTestId("ai-proposal");
  await expect(proposal).toContainText("Adds 8 objects");
  await proposal.getByRole("button", { name: "Accept" }).click();
  expect(await contents(page)).toBe("8 objects: 4 lines, 4 circles");
  expect(await circles(page)).toEqual([
    [10, 10, 5],
    [10, 70, 5],
    [110, 10, 5],
    [110, 70, 5],
  ]);
});

// #274: a part by prompt, then widened by prompt — the model sets its width
// parameter from the context, and the holes keep their places from the edges.
test("a plate made by prompt, then widened by prompt with its holes following", async ({
  page,
}) => {
  const service = await fakeAiService(page);
  await openApp(page);
  await signIn(page, service);
  await ask(page, PART);
  const proposal = panel(page).getByTestId("ai-proposal");
  await expect(proposal).toContainText("Adds 8 objects");
  await proposal.getByRole("button", { name: "Accept" }).click();
  await expect(proposal).toHaveCount(0);

  await enter(page, "ALL");
  await page.keyboard.press("Escape");
  await ask(page, WIDER);
  const consent = panel(page).getByTestId("ai-consent");
  await expect(consent).toContainText("d1 = 120: horizontal distance");
  await consent.getByRole("button", { name: "Send with these" }).click();
  await expect.poll(() => service.interpreted.length).toBe(2);
  expect(service.interpreted[1]?.context?.parameters).toContain(
    "d1 = 120: horizontal distance, id 4 end to id 2 start",
  );
  await expect(proposal).toContainText("Changes");
  await proposal.getByRole("button", { name: "Accept" }).click();
  await expect(proposal).toHaveCount(0);

  const xs = await lineXs(page);
  const [left, right] = [Math.min(...xs), Math.max(...xs)];
  expect(right - left).toBeCloseTo(140, 6);
  const holes = await circles(page);
  expect(holes).toHaveLength(4);
  for (const [x, , r] of holes) {
    expect(r).toBeCloseTo(4, 6);
    expect(Math.min(x - left, right - x)).toBeCloseTo(10, 6);
  }
  // One U takes the widening back.
  await enter(page, "U");
  const back = await lineXs(page);
  expect(Math.max(...back) - Math.min(...back)).toBeCloseTo(120, 6);
});

test("an edit of the selection, by the IDs the context gave out", async ({ page }) => {
  const service = await fakeAiService(page);
  await openApp(page);
  await signIn(page, service);
  await enter(page, "C", "30,30", "8", "C", "70,30", "8", "ALL");
  await ask(page, HOLES);
  await panel(page)
    .getByTestId("ai-consent")
    .getByRole("button", { name: "Send with these" })
    .click();
  const proposal = panel(page).getByTestId("ai-proposal");
  await expect(proposal).toContainText("Changes 2 objects");
  expect(service.interpreted[0]?.context?.objects.map((object) => object.id)).toEqual([1, 2]);
  await proposal.getByRole("button", { name: "Accept" }).click();
  await expect(proposal).toHaveCount(0);
  expect(await circles(page)).toEqual([
    [30, 30, 3],
    [70, 30, 3],
  ]);
});

// #282: a floor plan by a brief, revised before it is accepted, then drawn.
test("a floor plan by a brief, a revision, and Accept", async ({ page }) => {
  const service = await fakeAiService(page);
  service.plan = "pro"; // revisions and floor plans through the AI are Pro (ADR-0002)
  await openApp(page);
  await signIn(page, service);
  await ask(page, HOUSE);
  const proposal = panel(page).getByTestId("ai-proposal");
  await expect(proposal).toContainText("Adds");
  expect(await contents(page)).toBe("No selection");

  await proposal.getByRole("button", { name: "Revise" }).click();
  await ask(page, BIGGER_KITCHEN);
  await expect.poll(() => service.interpreted.length).toBe(2);
  expect(service.interpreted[1]).toMatchObject({
    prompt: BIGGER_KITCHEN,
    previous: { prompt: HOUSE, intent: { operations: [{ op: "floor_plan" }] } },
  });
  await expect(panel(page).getByTestId("ai-turns").locator("li").last()).toContainText("14");
  await proposal.getByRole("button", { name: "Accept" }).click();
  await expect(proposal).toHaveCount(0);

  const file = await downloadFrom(page, () => enter(page, "DXFOUT"));
  const labels = entities(file.text, "TEXT").map((text) => group(text, 1) ?? "");
  for (const name of ["living room", "kitchen", "bathroom"]) expect(labels).toContain(name);
  // A label says the area the revision asked for the kitchen.
  const areas = labels.filter((label) => label.endsWith(" m²")).map((label) => parseFloat(label));
  expect(areas.some((area) => Math.abs(area - 14) <= 0.7)).toBe(true);
  expect(entities(file.text).filter((e) => group(e, 8) === "WALL-EXT").length).toBeGreaterThan(4);
  // One U takes the plan away.
  await enter(page, "U");
  expect(await contents(page)).toBe("No selection");
});

test("what the engine refuses is explained, and nothing is proposed", async ({ page }) => {
  const service = await fakeAiService(page);
  await openApp(page);
  await signIn(page, service);
  await enter(page, "L", "0,0", "10,0", "", "L", "0,0", "0,10", "", "ALL");
  await ask(page, TOO_BIG);
  await panel(page)
    .getByTestId("ai-consent")
    .getByRole("button", { name: "Send with these" })
    .click();
  await expect
    .poll(() => service.explained.map(([, reason]) => reason))
    .toEqual(["operation 0: FILLET refused: The radius is too large for these lines"]);
  await expect(panel(page).getByTestId("ai-turns").locator("li").last()).toContainText("smaller");
  await expect(panel(page).getByTestId("ai-proposal")).toHaveCount(0);
  expect(await contents(page)).toBe("2 lines");
  expect(service.outcomes).toEqual([]);
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
  // drafting is untouched. The panel is open from the start.
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
  await chooseMenu(page, "View", "AI Assistant");
  await signIn(page, service);
  await page.getByRole("button", { name: /^Account, / }).click();
  await expect(page.getByTestId("ai-account").getByTestId("ai-privacy")).toBeVisible();
  await page.getByTestId("ai-account").getByRole("button", { name: "Sign out" }).click();
  await expect(panel(page).getByTestId("ai-sign-in")).toBeVisible();
  expect(await page.evaluate(() => localStorage.getItem("cadslate2d.ai.session"))).toBeNull();

  // Signed in again, and the account deleted after a second click.
  await signIn(page, service);
  expect(service.accounts.has("drafter@example.com")).toBe(true);
  await page.getByRole("button", { name: /^Account, / }).click();
  await page.getByTestId("ai-account").getByRole("button", { name: "Delete account" }).click();
  await expect(page.getByTestId("ai-delete-confirm")).toBeVisible();
  await page.getByTestId("ai-account").getByRole("button", { name: "Delete", exact: true }).click();
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

test("the panel with the way to Pro", async ({ page, browserName }) => {
  test.skip(browserName !== "chromium", "visual baselines are compared in Chromium only");
  const service = await fakeAiService(page);
  await openApp(page);
  await chooseMenu(page, "View", "Layers Panel");
  await chooseMenu(page, "View", "Properties Panel");
  await signIn(page, service);
  await ask(page, HOUSE);
  await expect(panel(page).getByTestId("ai-upgrade")).toBeVisible();
  await expect(panel(page)).toHaveScreenshot("ai-panel-upgrade.png");
});

// #9 (ADR-0002): what Free does not hold is refused with the way to Pro;
// paid for in Checkout's tab, the same requests are Pro's.
test("Free sees the way to Pro; after paying, the floor plan and the revision come", async ({
  page,
}) => {
  const service = await fakeAiService(page);
  await openApp(page);
  await signIn(page, service);
  const allowance = panel(page).getByTestId("ai-allowance");
  await expect(allowance).toContainText("Free · 100 of 100 left");

  // A floor plan through the AI: refused, not counted, nothing proposed.
  await ask(page, HOUSE);
  const error = panel(page).getByTestId("ai-error");
  await expect(error).toHaveAttribute("data-kind", "plan");
  await expect(error).toContainText("Floor plans through the AI are part of Pro.");
  await expect(panel(page).getByTestId("ai-upgrade")).toBeVisible();
  await expect(panel(page).getByTestId("ai-proposal")).toHaveCount(0);
  await expect(allowance).toContainText("100 of 100 left");

  // A command is Free; revising it is not, and the model is not asked.
  await ask(page, CIRCLE);
  const proposal = panel(page).getByTestId("ai-proposal");
  await expect(proposal).toContainText("Adds 1 object");
  await proposal.getByRole("button", { name: "Revise" }).click();
  await ask(page, BIGGER);
  await expect(error).toContainText("Revising a proposal is part of Pro.");
  expect(service.interpreted.map((request) => request.prompt)).toEqual([HOUSE, CIRCLE]);

  // Upgrade: Checkout opens in a tab of its own, and the drawing stays here.
  const [checkout] = await Promise.all([
    page.waitForEvent("popup"),
    panel(page).getByRole("button", { name: "Upgrade to Pro, monthly" }).click(),
  ]);
  await expect(checkout.locator("h1")).toContainText("paid");
  expect(new URL(checkout.url()).searchParams.get("interval")).toBe("month");
  expect(service.stripePages).toEqual(["checkout month"]);
  await checkout.close();
  // Back in the app's tab, the account is read again: Pro, the refusal gone.
  await page.evaluate(() => window.dispatchEvent(new Event("focus")));
  await expect(allowance).toContainText("Pro · 1999 of 2000 left");
  await expect(error).toHaveCount(0);
  await expect(proposal).toContainText("Adds 1 object");

  // Pro: the revision and the floor plan.
  await ask(page, BIGGER);
  await expect.poll(() => service.interpreted.length).toBe(3);
  expect(service.interpreted[2]).toMatchObject({ prompt: BIGGER, previous: { prompt: CIRCLE } });
  await ask(page, HOUSE);
  await expect(proposal).toContainText("Adds");
  await expect(error).toHaveCount(0);
  await expect(allowance).toContainText("1997 of 2000 left");
});

test("the panel's account view, on Pro", async ({ page, browserName }) => {
  test.skip(browserName !== "chromium", "visual baselines are compared in Chromium only");
  const service = await fakeAiService(page);
  service.subscribe("drafter@example.com");
  await openApp(page);
  await chooseMenu(page, "View", "Layers Panel");
  await chooseMenu(page, "View", "Properties Panel");
  await signIn(page, service);
  await page.getByRole("button", { name: /^Account, / }).click();
  await expect(page.getByTestId("ai-plan")).toContainText("renews");
  await expect(page.getByTestId("ai-account")).toHaveScreenshot("ai-panel-account.png");
});

// #289: the account in the panel — the plan, the month's actions, the way
// to Pro, Stripe's billing portal, and deletion, which cancels Pro first.
test("the account: upgrade, manage, and delete", async ({ page }) => {
  const service = await fakeAiService(page);
  await openApp(page);
  await signIn(page, service);
  await ask(page, CIRCLE);
  await expect(panel(page).getByTestId("ai-proposal")).toBeVisible();
  await page.getByRole("button", { name: /^Account, / }).click();
  const account = page.getByTestId("ai-account");
  await expect(account.getByTestId("ai-plan")).toHaveText("Free");
  await expect(account.getByTestId("ai-usage")).toContainText("1 of 100 AI actions used");
  await expect(account.getByRole("button", { name: "Manage billing" })).toHaveCount(0);

  // Upgrade, in Checkout's tab; back here, Pro and when it renews.
  const [checkout] = await Promise.all([
    page.waitForEvent("popup"),
    account.getByRole("button", { name: "Yearly" }).click(),
  ]);
  expect(service.stripePages).toEqual(["checkout year"]);
  await checkout.close();
  await page.evaluate(() => window.dispatchEvent(new Event("focus")));
  await expect(account.getByTestId("ai-plan")).toHaveText(/^Pro · renews on November 26, 2026/);
  await expect(account.getByTestId("ai-usage")).toContainText("1 of 2000");

  // Manage: Stripe's portal, in a tab of its own. Cancelled there, Pro
  // lasts to the period's end; a failed payment is said too.
  const [portal] = await Promise.all([
    page.waitForEvent("popup"),
    account.getByRole("button", { name: "Manage billing" }).click(),
  ]);
  await expect(portal.locator("h1")).toContainText("Billing portal");
  await portal.close();
  const subscription = service.subscriptions.get("drafter@example.com");
  if (!subscription) throw new Error("no subscription");
  subscription.cancelAtPeriodEnd = true;
  subscription.paymentFailed = true;
  await page.evaluate(() => window.dispatchEvent(new Event("focus")));
  await expect(account.getByTestId("ai-plan")).toHaveText(/^Pro · ends on November 26, 2026/);
  await expect(account.getByTestId("ai-payment-failed")).toBeVisible();

  // Delete: the subscription is cancelled first, then the account goes.
  await account.getByRole("button", { name: "Delete account" }).click();
  const confirm = page.getByTestId("ai-delete-confirm");
  await expect(confirm).toContainText("Its subscription is cancelled first.");
  await confirm.getByRole("button", { name: "Delete" }).click();
  await expect(panel(page).getByTestId("ai-sign-in")).toBeVisible();
  expect(service.cancelled).toEqual(["drafter@example.com"]);
  expect(service.accounts.has("drafter@example.com")).toBe(false);
  // The proposal went with the account; the drawing never changed.
  expect(await contents(page)).toBe("No selection");
});

// AI service #10: a request that asks for options. Pro steps through
// them and accepts the one shown as one undo; Free gets one, and the note
// of how many more Pro shows.
const PLATE_OPTIONS =
  "Show me three options for a 120 × 80 mm plate with its lower left corner at the origin and four Ø8 holes 10 mm in from the corners: square corners, corners rounded to 10 mm, or 10 mm chamfers";

test("options: Pro steps through them and accepts one; Free gets one with the note", async ({
  page,
}) => {
  const service = await fakeAiService(page);
  service.plan = "pro";
  await openApp(page);
  await signIn(page, service);
  await ask(page, PLATE_OPTIONS);
  const proposal = panel(page).getByTestId("ai-proposal");
  const options = proposal.getByTestId("ai-options");
  await expect(options).toContainText("Option 1 of 3");
  await expect(proposal.getByTestId("ai-more")).toHaveCount(0);
  const first = (await proposal.locator(".ai__steps").textContent()) ?? "";
  await options.getByRole("button", { name: "Next option" }).click();
  await expect(options).toContainText("Option 2 of 3");
  await options.getByRole("button", { name: "Next option" }).click();
  await options.getByRole("button", { name: "Next option" }).click();
  await expect(options).toContainText("Option 1 of 3");
  await expect(proposal.locator(".ai__steps")).toHaveText(first);
  await options.getByRole("button", { name: "Previous option" }).click();
  await expect(options).toContainText("Option 3 of 3");
  // Nothing in the drawing until Accept; then the third, as one undo.
  expect(await contents(page)).toBe("No selection");
  await proposal.getByRole("button", { name: "Accept" }).click();
  await expect(proposal).toHaveCount(0);
  await expect.poll(() => service.options.at(-1)).toBe(2);
  expect(await contents(page)).not.toBe("No selection");
  // One action for the three.
  await expect(panel(page).getByTestId("ai-allowance")).toContainText("1999 of 2000 left");
  await enter(page, "U");
  expect(await contents(page)).toBe("No selection");

  // Free: one option, and the way to the others.
  service.plans.set("drafter@example.com", "free");
  await page.evaluate(() => window.dispatchEvent(new Event("focus")));
  await expect(panel(page).getByTestId("ai-allowance")).toContainText("Free");
  await ask(page, PLATE_OPTIONS);
  await expect(proposal).toBeVisible();
  await expect(proposal.getByTestId("ai-options")).toHaveCount(0);
  const more = proposal.getByTestId("ai-more");
  await expect(more).toContainText("Pro shows 2 more options for this request.");
  await expect(more.getByRole("button", { name: "Upgrade to Pro, monthly" })).toBeVisible();
});

// The AI Assistant is open from the start, and one button away when closed.
test("the AI Assistant button beside the quick actions shows and hides the panel", async ({
  page,
}) => {
  await fakeAiService(page);
  await openApp(page);
  await expect(panel(page)).toBeVisible();
  const button = page.getByRole("button", { name: "AI Assistant" });
  await expect(button).toHaveAttribute("aria-pressed", "true");
  await button.click();
  await expect(panel(page)).toHaveCount(0);
  await expect(button).toHaveAttribute("aria-pressed", "false");
  await button.click();
  await expect(panel(page)).toBeVisible();
  // Kept for the next visit.
  await page.reload();
  await expect(panel(page)).toBeVisible();
});

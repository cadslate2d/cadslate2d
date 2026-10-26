// The M20 exit scenario (#263): signing in to the AI by an emailed link,
// drawing by a request, editing the selection by a request with consent,
// the proposal previewed, accepted, and undone, the month's allowance
// reached, and signing out — and drafting going on throughout.
//
// It runs against the tests' stand-in service (ai-fake.ts), answering from
// the corpus's recordings of the real model. With AI_SERVICE_LIVE set to a
// running AI service — the staging build with its replay model, mail sink,
// and an allowance of 3 (cadslate2d-ai-service, scripts/app-scenario.sh) —
// and the bundle built with that address, it runs against that service
// instead, reading the sign-in link from the service's test mailbox.
import { expect, test, type Page } from "@playwright/test";
import { fakeAiService } from "./ai-fake";
import { downloadFrom, enter, entities, num, openApp, rendererStats } from "./support";

const LIVE = process.env["AI_SERVICE_LIVE"];
// A new account each run against a service that keeps its database.
const EMAIL = LIVE ? `scenario-${Date.now()}@example.com` : "scenario@example.com";
const CIRCLE = "Draw a circle of radius 25 centred at 100,100";
const MOVE = "Move these 25 mm up";
const IMPOSSIBLE = "Extrude this into a 3D box and render it";

const panel = (page: Page) => page.getByTestId("ai-panel");

/** The sign-in link's token, as the email carries it. */
async function emailedToken(page: Page, tokenFor: (email: string) => string): Promise<string> {
  if (!LIVE) return tokenFor(EMAIL);
  const response = await page.request.post(new URL("v1/test/mailbox", `${LIVE}/`).href, {
    data: { to: EMAIL },
  });
  const messages = (await response.json()) as { text: string }[];
  const token = /ai-sign-in=([A-Za-z0-9_-]+)/.exec(messages.at(-1)?.text ?? "")?.[1];
  if (!token) throw new Error(`no sign-in link for ${EMAIL} in the service's mailbox`);
  return token;
}

/** What ALL selects, as the Properties panel says it; then nothing is selected. */
async function contents(page: Page): Promise<string> {
  await enter(page, "ALL");
  const headline = (await page.getByTestId("selection-headline").textContent()) ?? "";
  await page.keyboard.press("Escape");
  await page.keyboard.press("Escape");
  return headline;
}

/** The drawing as DXFOUT writes it: its circles' centres and radii, its lines' ends. */
async function saved(page: Page): Promise<{ circles: number[][]; lines: number[][] }> {
  const file = await downloadFrom(page, () => enter(page, "DXFOUT"));
  return {
    circles: entities(file.text, "CIRCLE").map((c) => [num(c, 10), num(c, 20), num(c, 40)]),
    lines: entities(file.text, "LINE").map((l) => [num(l, 10), num(l, 20), num(l, 11), num(l, 21)]),
  };
}

async function ask(page: Page, request: string) {
  await panel(page).getByLabel("What should be drawn or changed?").fill(request);
  await panel(page).getByRole("button", { name: "Ask" }).click();
}

test("the M20 scenario: drawing and editing by request, accepted, undone, to the limit, and signed out", async ({
  page,
}) => {
  const service = LIVE ? undefined : await fakeAiService(page);
  if (service) service.allowance = 3;
  await openApp(page);

  // 1. Sign in: a link by email, opened in this browser.
  await expect(panel(page)).toBeVisible(); // open from the start
  await panel(page).getByLabel("Email").fill(EMAIL);
  await panel(page).getByRole("button", { name: "Email me a sign-in link" }).click();
  await expect(panel(page).getByTestId("ai-link-sent")).toContainText(EMAIL);
  const token = await emailedToken(page, (email) => service?.tokenFor(email) ?? "");
  await page.goto("about:blank");
  await page.goto(`./?debug#ai-sign-in=${token}`);
  await expect(page.getByRole("region", { name: "Model space" }).locator("canvas")).toBeVisible();
  await expect.poll(async () => (await rendererStats(page)).frames).toBeGreaterThan(0);
  await expect(page.getByTestId("ai-email")).toHaveText(EMAIL);
  await expect(panel(page).getByTestId("ai-allowance")).toContainText("3 of 3 left");

  // Drafting by hand, as ever.
  await enter(page, "L", "0,0", "200,0", "");
  expect(await contents(page)).toBe("1 line");

  // 2. Draw by a request: proposed, nothing drawn until Accept.
  await ask(page, CIRCLE);
  const proposal = panel(page).getByTestId("ai-proposal");
  await expect(proposal).toContainText("Adds 1 object");
  expect(await contents(page)).toBe("1 line");
  await proposal.getByRole("button", { name: "Accept" }).click();
  await expect(proposal).toHaveCount(0);
  expect(await contents(page)).toBe("2 objects: 1 line, 1 circle");
  await expect(panel(page).getByTestId("ai-allowance")).toContainText("2 of 3 left");

  // 3. Edit the selection by a request: the user sees what would be sent,
  // and sends it.
  await enter(page, "ALL");
  await ask(page, MOVE);
  const consent = panel(page).getByTestId("ai-consent");
  await expect(consent).toContainText("line");
  await expect(consent).toContainText("radius 25.0000");
  await consent.getByRole("button", { name: "Send with these" }).click();

  // 4. Previewed, accepted as one change, and undone with one U.
  await expect(proposal).toContainText("Changes 2 objects");
  await proposal.getByRole("button", { name: "Accept" }).click();
  await expect(proposal).toHaveCount(0);
  await page.keyboard.press("Escape");
  await page.keyboard.press("Escape");
  expect(await saved(page)).toEqual({ circles: [[100, 125, 25]], lines: [[0, 25, 200, 25]] });
  await enter(page, "U");
  expect(await saved(page)).toEqual({ circles: [[100, 100, 25]], lines: [[0, 0, 200, 0]] });
  await expect(panel(page).getByTestId("ai-allowance")).toContainText("1 of 3 left");

  // 5. The month's allowance reached: what cannot be done is answered and
  // counted, then the prompt closes and says when it comes back.
  await ask(page, IMPOSSIBLE);
  await expect(panel(page).getByTestId("ai-turns").locator("li").last()).toContainText("2D");
  await expect(panel(page).getByTestId("ai-allowance")).toContainText("0 of 3 left");
  await expect(panel(page).getByTestId("ai-limit")).toContainText("used this month's 3 AI actions");
  await expect(panel(page).getByRole("button", { name: "Ask" })).toBeDisabled();
  // Drafting is unaffected.
  await enter(page, "C", "300,0", "10");
  expect(await contents(page)).toBe("3 objects: 1 line, 2 circles");

  // 6. Signed out: the panel offers sign-in again, and drafting goes on.
  await page.getByRole("button", { name: /^Account, / }).click();
  await page.getByTestId("ai-account").getByRole("button", { name: "Sign out" }).click();
  await expect(panel(page).getByTestId("ai-sign-in")).toBeVisible();
  await enter(page, "L", "0,50", "200,50", "");
  expect(await contents(page)).toBe("4 objects: 2 lines, 2 circles");
  if (service) {
    expect(service.outcomes.map(([, outcome]) => outcome)).toEqual(["accepted", "accepted"]);
  }
});

// The M26 exit scenario: answers checked against their requests (#362–#365).
// A request whose sizes contradict each other is answered with what
// conflicts and nothing drawn; asked for a sheet, the answer comes with how
// it was checked — what was read from the request, each requirement met —
// and is accepted as one proposal and undone in one step.
//
// The requests are answered by the tests' stand-in service (ai-fake.ts)
// from the corpus's recordings of the real model: L6's contradiction and
// L5's bracket. How the bracket was checked is the service's, added here
// as the service sends it.
import { expect, test, type Page } from "@playwright/test";
import { fakeAiService, recordedAnswer } from "./ai-fake";
import { downloadFrom, enter, entities, logLines, openApp, prompt, rendererStats } from "./support";

const CONTRADICTION = "Draw a rectangle 100 wide and 60 high whose diagonal is 150.";
const SHEET =
  "Draw a drawing sheet for an L-bracket on an A4 sheet at 1:1 in third-angle projection, with front, left side and top views. The bracket is 80 mm wide, 60 mm high and 40 mm deep, of 5 mm plate. Dimension the width, height and depth. Add a bill of materials — part 1, Bracket, 1 off, steel S235 — and balloon it.";

const panel = (page: Page) => page.getByTestId("ai-panel");

async function drawn(page: Page): Promise<number> {
  const text = (await downloadFrom(page, () => enter(page, "DXFOUT"))).text;
  return entities(text).length;
}

async function ask(page: Page, request: string) {
  await panel(page).getByLabel("What should be drawn or changed?").fill(request);
  await panel(page).getByRole("button", { name: "Ask" }).click();
}

test("the M26 scenario: a contradiction said, a sheet checked and accepted", async ({ page }) => {
  const service = await fakeAiService(page);
  service.plan = "pro";
  const sheet = recordedAnswer(SHEET);
  if (!sheet?.intent)
    throw new Error("The L5 corpus has no recorded answer to the bracket's sheet.");
  service.answers.set(SHEET, {
    ...sheet,
    trail: {
      stated: ["A4", "1:1"],
      rounds: [],
      checked: [
        { says: "front, left side and top views", met: true },
        { says: "a bill of materials", met: true },
      ],
      departs: [],
    },
  });
  const canvas = await openApp(page);
  await panel(page).getByLabel("Email").fill("scenario@example.com");
  await panel(page).getByRole("button", { name: "Email me a sign-in link" }).click();
  await page.goto("about:blank");
  await page.goto(`./?debug#ai-sign-in=${service.tokenFor("scenario@example.com")}`);
  await expect(canvas).toBeVisible();
  await expect.poll(async () => (await rendererStats(page)).frames).toBeGreaterThan(0);
  await enter(page, "UNITS", "mm");
  const before = await drawn(page);

  // 1. Sizes that contradict each other: the reply says so, nothing drawn.
  await ask(page, CONTRADICTION);
  const reply = panel(page).getByTestId("ai-turns").locator(".ai__turn").last();
  await expect(reply).toContainText(/diagonal/i);
  await expect(reply).toContainText(
    /116|117|contradict|inconsistent|cannot|can't|conflict|impossible/i,
  );
  await expect(panel(page).getByTestId("ai-proposal")).toHaveCount(0);
  expect(await drawn(page)).toBe(before);

  // 2. The sheet, with how it was checked, accepted as one proposal.
  await ask(page, SHEET);
  const proposal = panel(page).getByTestId("ai-proposal");
  await expect(proposal).toContainText("Adds");
  const trail = panel(page).getByTestId("ai-trail");
  await expect(trail).toContainText("How this was checked — 2 of 2 requirements met · asked once");
  await trail.locator("summary").click();
  await expect(trail).toContainText("Read from your request: A4 · 1:1");
  await expect(panel(page).getByTestId("ai-checklist")).toContainText(
    "✓ front, left side and top views",
  );
  await proposal.getByRole("button", { name: "Accept" }).click();
  await expect(proposal).toHaveCount(0);
  await page.keyboard.press("Escape");
  const accepted = await drawn(page);
  expect(accepted).toBeGreaterThan(before + 10);

  // 3. One step undoes the whole sheet, and one redoes it.
  await enter(page, "U");
  expect(await drawn(page)).toBe(before);
  await enter(page, "REDO");
  expect(await drawn(page)).toBe(accepted);
  await expect(prompt(page)).toHaveText("Command:");
  await expect(logLines(page).filter({ hasText: "Unknown command" })).toHaveCount(0);
});

// The M22 exit scenario (#283): a floor plan from a brief by a request,
// revised, another of its plans chosen, accepted; its walls edited by hand
// with the ordinary tools; then dimensioned, saved, and plotted.
//
// It runs against the tests' stand-in service (ai-fake.ts), answering from
// the L3 corpus's recordings of the real model.
import { expect, test, type Page } from "@playwright/test";
import { fakeAiService } from "./ai-fake";
import {
  downloadFrom,
  enter,
  entities,
  group,
  logLines,
  num,
  openApp,
  prompt,
  rendererStats,
  type DxfRecord,
} from "./support";

const HOUSE =
  "A two-bedroom house on a 12 × 20 m plot with a 5 m front setback: living room 25 m², kitchen 10 m², bedrooms of 12 and 10 m², bathroom 5 m²; the kitchen opens onto the living room";
const KITCHEN = "Make the kitchen 14 m²";
const DIFFERENT = "Show me a different layout";

const panel = (page: Page) => page.getByTestId("ai-panel");

async function ask(page: Page, request: string) {
  await panel(page)
    .getByLabel(/What should be drawn or changed\?|How should it change\?/)
    .fill(request);
  await panel(page).getByRole("button", { name: "Ask" }).click();
}

async function saved(page: Page): Promise<string> {
  return (await downloadFrom(page, () => enter(page, "DXFOUT"))).text;
}

const on = (text: string, layer: string) =>
  entities(text).filter((entity) => group(entity, 8) === layer);
const ends = (line: DxfRecord) => [num(line, 10), num(line, 20), num(line, 11), num(line, 21)];

test("the M22 scenario: a plan by a brief, revised, another chosen, edited, dimensioned, saved, plotted", async ({
  page,
}) => {
  const service = await fakeAiService(page);
  service.plan = "pro"; // revisions and floor plans through the AI are Pro (ADR-0002)
  const canvas = await openApp(page);
  await expect(panel(page)).toBeVisible(); // open from the start
  await panel(page).getByLabel("Email").fill("scenario@example.com");
  await panel(page).getByRole("button", { name: "Email me a sign-in link" }).click();
  await page.goto("about:blank");
  await page.goto(`./?debug#ai-sign-in=${service.tokenFor("scenario@example.com")}`);
  await expect(canvas).toBeVisible();
  await expect.poll(async () => (await rendererStats(page)).frames).toBeGreaterThan(0);

  // 1. A brief by a request: previewed, the drawing untouched.
  await ask(page, HOUSE);
  const proposal = panel(page).getByTestId("ai-proposal");
  await expect(proposal).toContainText("Adds");
  expect(entities(await saved(page))).toHaveLength(0);

  // 2. Revised: the kitchen bigger, the brief it revises sent along.
  await proposal.getByRole("button", { name: "Revise" }).click();
  await ask(page, KITCHEN);
  await expect.poll(() => service.interpreted.length).toBe(2);
  expect(service.interpreted[1]?.previous?.prompt).toBe(HOUSE);

  // 3. Another of its plans, the bigger kitchen kept; accepted.
  await proposal.getByRole("button", { name: "Revise" }).click();
  await ask(page, DIFFERENT);
  await expect.poll(() => service.interpreted.length).toBe(3);
  expect(service.interpreted[2]?.previous?.prompt).toBe(KITCHEN);
  await proposal.getByRole("button", { name: "Accept" }).click();
  await expect(proposal).toHaveCount(0);
  let text = await saved(page);
  const labels = on(text, "ROOM-TEXT").map((label) => group(label, 1) ?? "");
  for (const name of ["living room", "kitchen", "bathroom"]) expect(labels).toContain(name);
  const areas = labels.filter((l) => l.endsWith(" m²")).map((l) => parseFloat(l));
  expect(areas.some((area) => Math.abs(area - 14) <= 0.7)).toBe(true);

  // 4. A wall edited by hand: one interior face moved 0.1 along itself,
  //    chosen by a crossing polygon round its middle.
  const face = on(text, "WALL-INT").find((e) => e.type === "LINE");
  expect(face).toBeDefined();
  const [x0, y0, x1, y1] = ends(face as DxfRecord);
  const [mx, my] = [((x0 ?? 0) + (x1 ?? 0)) / 2, ((y0 ?? 0) + (y1 ?? 0)) / 2];
  const r = 0.01;
  // What the plan made is the selection after Accept: cleared first.
  await page.keyboard.press("Escape");
  await page.keyboard.press("Escape");
  await enter(
    page,
    "MOVE",
    "CP",
    `${mx - r},${my - r}`,
    `${mx + r},${my - r}`,
    `${mx + r},${my + r}`,
    `${mx - r},${my + r}`,
    "",
    "",
    "0,0",
    "0.1,0.1",
  );
  text = await saved(page);
  const moved = on(text, "WALL-INT")
    .filter((e) => e.type === "LINE")
    .map(ends)
    .some(
      ([a, b, c, d]) =>
        Math.abs((a ?? 0) - (x0 ?? 0) - 0.1) < 1e-9 &&
        Math.abs((b ?? 0) - (y0 ?? 0) - 0.1) < 1e-9 &&
        Math.abs((c ?? 0) - (x1 ?? 0) - 0.1) < 1e-9 &&
        Math.abs((d ?? 0) - (y1 ?? 0) - 0.1) < 1e-9,
    );
  expect(moved).toBe(true);

  // 5. Dimensioned across the front, saved, and plotted.
  const outside = on(text, "WALL-EXT").find((e) => e.type === "LWPOLYLINE") as DxfRecord;
  const xs = outside.groups.filter(([code]) => code === 10).map(([, v]) => Number(v));
  const ys = outside.groups.filter(([code]) => code === 20).map(([, v]) => Number(v));
  const [left, right, front] = [Math.min(...xs), Math.max(...xs), Math.min(...ys)];
  await enter(page, "DIMLINEAR", `${left},${front}`, `${right},${front}`, `${left},${front - 1.5}`);
  const file = await downloadFrom(page, () => enter(page, "SAVEAS", ""));
  const dimension = entities(file.text, "DIMENSION")[0];
  expect(num(dimension, 42)).toBeCloseTo(right - left, 6);
  expect(file.text).toContain("WALL-EXT");

  await enter(page, "PLOT");
  const plot = await downloadFrom(page, () => enter(page, "A4", "E", "1:100", "Y", "C"));
  expect(plot.text.startsWith("%PDF-1.4\n")).toBe(true);
  await expect(prompt(page)).toHaveText("Command:");
  await expect(logLines(page).filter({ hasText: "Unknown command" })).toHaveCount(0);
});

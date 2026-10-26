// The M21 exit scenario (#275): a sketch constrained by hand, a plate made
// by a request, a flange made by command, both edited through their
// parameters — the plate by a request, the flange at the command line —
// then saved and reopened with every constraint held and still driving.
//
// The plate comes before the flange: a parameter's name is the drawing's,
// and the stand-in service (ai-fake.ts) answers "wider" with the recorded
// model's `d1`, which names the plate's width only when the plate is the
// first part. The live model reads the name from the context.
import { expect, test, type Page } from "@playwright/test";
import { fakeAiService } from "./ai-fake";
import {
  chooseFile,
  clickWorld,
  downloadFrom,
  enter,
  entities,
  logLines,
  num,
  openApp,
  rendererStats,
  type DxfRecord,
} from "./support";

const PART =
  "Make a 120 × 80 mm plate with its lower left corner at the origin and four Ø8 holes 10 mm in from the corners";
const WIDER = "Make this plate 20 mm wider without moving the holes relative to the edges";

const panel = (page: Page) => page.getByTestId("ai-panel");

async function ask(page: Page, request: string) {
  await panel(page).getByLabel("What should be drawn or changed?").fill(request);
  await panel(page).getByRole("button", { name: "Ask" }).click();
}

/** The drawing as DXFOUT writes it. */
async function saved(page: Page): Promise<string> {
  return (await downloadFrom(page, () => enter(page, "DXFOUT"))).text;
}

const ends = (line: DxfRecord) => [num(line, 10), num(line, 20), num(line, 11), num(line, 21)];

/** The plate's width, and how far each hole is from its nearest side: its lines are right of x = -50. */
function plate(text: string): { width: number; fromEdges: number[] } {
  const xs = entities(text, "LINE")
    .map(ends)
    .filter(([x]) => (x ?? 0) > -50)
    .flatMap(([x0, , x1]) => [x0 ?? 0, x1 ?? 0]);
  const [left, right] = [Math.min(...xs), Math.max(...xs)];
  const fromEdges = entities(text, "CIRCLE")
    .filter((circle) => num(circle, 10) < 200 && num(circle, 40) < 5)
    .map((circle) => Math.min(num(circle, 10) - left, right - num(circle, 10)));
  return { width: right - left, fromEdges };
}

/** Every parameter, as `-PARAMETERS ?` lists it: `name = value`. */
async function parameters(page: Page): Promise<string[]> {
  const before = await logLines(page).count();
  await enter(page, "-PARAMETERS", "?");
  await expect.poll(async () => logLines(page).count()).toBeGreaterThan(before + 1);
  const lines = await logLines(page).allTextContents();
  return lines.slice(before).filter((line) => / = /.test(line) && !line.includes(":"));
}

test("the M21 scenario: constrain, generate by command and by request, edit, save, reopen", async ({
  page,
}) => {
  const service = await fakeAiService(page);
  const canvas = await openApp(page);
  // Signed in first, by an emailed link: the link opens the app afresh.
  await expect(panel(page)).toBeVisible(); // open from the start
  await panel(page).getByLabel("Email").fill("scenario@example.com");
  await panel(page).getByRole("button", { name: "Email me a sign-in link" }).click();
  await page.goto("about:blank");
  await page.goto(`./?debug#ai-sign-in=${service.tokenFor("scenario@example.com")}`);
  await expect(canvas).toBeVisible();
  await expect.poll(async () => (await rendererStats(page)).frames).toBeGreaterThan(0);
  await expect(page.getByTestId("ai-email")).toHaveText("scenario@example.com");

  // 1. A sketch, constrained by hand: an L of two lines, their corner
  //    coincident, the first level, the second plumb, its start fixed.
  await enter(page, "L", "-150,0", "-90,0", "-90,40", "");
  await enter(page, "GCCOINCIDENT", "-90,0", "-90,0");
  await enter(page, "GCHORIZONTAL");
  await clickWorld(page, canvas, -120, 0);
  await enter(page, "GCVERTICAL");
  await clickWorld(page, canvas, -90, 20);
  await enter(page, "GCFIX", "-150,0");
  // The plumb line moved right: the level one follows, its start fixed.
  await enter(page, "MOVE");
  await clickWorld(page, canvas, -90, 20);
  await enter(page, "", "0,0", "10,0");
  let text = await saved(page);
  const [first, second] = entities(text, "LINE").map(ends);
  expect(first?.slice(0, 2)).toEqual([-150, 0]);
  expect(first?.[2]).toBeCloseTo(-80, 6);
  expect(first?.[3]).toBeCloseTo(0, 6);
  expect(second?.[0]).toBeCloseTo(-80, 6);
  expect(second?.[1]).toBeCloseTo(0, 6);

  // 2. A plate by a request.
  await ask(page, PART);
  const proposal = panel(page).getByTestId("ai-proposal");
  await expect(proposal).toContainText("Adds 8 objects");
  await proposal.getByRole("button", { name: "Accept" }).click();
  await expect(proposal).toHaveCount(0);

  // 3. A round flange by command.
  await page.keyboard.press("Escape");
  await enter(page, "FLANGE", "", "300,0", "150", "50", "110", "8", "12");
  await expect(logLines(page).last()).toContainText("constraints.");

  // 4. Both edited: the plate by a request, with consent for its context…
  await enter(page, "ALL");
  await page.keyboard.press("Escape");
  await ask(page, WIDER);
  const consent = panel(page).getByTestId("ai-consent");
  await expect(consent).toContainText("d1 = 120: horizontal distance");
  await consent.getByRole("button", { name: "Send with these" }).click();
  await expect(proposal).toContainText("Changes");
  await proposal.getByRole("button", { name: "Accept" }).click();
  await expect(proposal).toHaveCount(0);
  // …and the flange's outside diameter at the command line.
  const outside = (await parameters(page)).find((line) => line.endsWith(" = 150"));
  expect(outside).toBeDefined();
  const name = outside?.split(" = ")[0] ?? "";
  await enter(page, "-PARAMETERS", "Edit", name, "180");
  await expect(logLines(page).last()).toHaveText(`${name} = 180`);

  text = await saved(page);
  let measured = plate(text);
  expect(measured.width).toBeCloseTo(140, 6);
  expect(measured.fromEdges).toHaveLength(4);
  for (const distance of measured.fromEdges) expect(distance).toBeCloseTo(10, 6);
  const radii = entities(text, "CIRCLE")
    .filter((circle) => num(circle, 10) > 200)
    .map((circle) => num(circle, 40));
  expect(Math.max(...radii)).toBeCloseTo(90, 6);
  const listed = await parameters(page);

  // 5. Saved and reopened: the same parameters, and they still drive.
  await chooseFile(page, () => enter(page, "OPEN"), {
    name: "parts.dxf",
    mimeType: "application/dxf",
    buffer: Buffer.from(text),
  });
  await page.getByRole("button", { name: "Dismiss the import report" }).click();
  expect(await parameters(page)).toEqual(listed);
  await enter(page, "-PARAMETERS", "Edit", "d1", "160");
  measured = plate(await saved(page));
  expect(measured.width).toBeCloseTo(160, 6);
  for (const distance of measured.fromEdges) expect(distance).toBeCloseTo(10, 6);
  // The sketch's constraints came back too: its fixed start holds.
  // (The file opened zoomed to its extents: the level line is chosen by a
  // window polygon typed round it.)
  await enter(page, "MOVE", "WP", "-155,-5", "-75,-5", "-75,5", "-155,5", "", "", "0,0", "0,10");
  await expect(logLines(page).last()).toContainText("The edit would break");
  await expect(logLines(page).last()).toContainText("fix");
  await expect(logLines(page).filter({ hasText: "Unknown command" })).toHaveCount(0);
});

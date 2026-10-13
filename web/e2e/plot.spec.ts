// PLOT (#187): model space as a one-page PDF, downloaded under the
// drawing's name, with the page, the scale, and the lineweights asked for.
import { expect, test } from "@playwright/test";
import { downloadFrom, enter, logLines, openApp, prompt } from "./support";

test("PLOT downloads a PDF of the drawing at the scale asked for", async ({ page }) => {
  await openApp(page);
  await enter(page, "L", "0,0", "200,0", "", "C", "100,50", "30", "DT", "0,80", "10", "0", "Plot");
  await enter(page, "LWEIGHT", "0.5", "L", "0,-50", "200,-50", "");
  await enter(page, "PLOT");
  await expect(prompt(page)).toHaveText("Paper size or [A4/A3/A2/A1/A0/Letter/Tabloid] <A4>:");
  const file = await downloadFrom(page, () => enter(page, "A4", "E", "1:2", "Y", "C"));
  expect(file.name).toBe("drawing.pdf");
  expect(file.text.startsWith("%PDF-1.4\n")).toBe(true);
  // A wide drawing: A4 landscape, in points.
  expect(file.text).toContain("/MediaBox [0 0 841.89 595.276]");
  // The 0.50 mm line at its width, and the default for the rest.
  expect(file.text).toContain("\n1.417 w\n");
  expect(file.text).toContain("\n0.709 w\n");
  expect(file.text.endsWith("%%EOF\n")).toBe(true);
  await expect(logLines(page).last()).toHaveText(
    "Plotting the extents of model space on A4, at 1:2.",
  );
  await expect(prompt(page)).toHaveText("Command:");
});

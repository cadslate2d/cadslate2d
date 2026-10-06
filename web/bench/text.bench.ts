// The text rendering experiment (DESIGN §23.5, ADR-0016).
import { writeFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { expect, test } from "@playwright/test";
import { openApp } from "../e2e/support";

for (const texts of [10_000, 100_000]) {
  test(`text: ${texts} texts of eight characters`, async ({ page }) => {
    await openApp(page);
    const report = await page.evaluate(async (count) => {
      const handle = window.__cadslate;
      if (!handle) throw new Error("debug handle is not published");
      return handle.probeText(count);
    }, texts);
    console.log(`TEXT ${JSON.stringify(report)}`);
    expect(report.candidates).toHaveLength(3);
  });
}

// The pictures in the ADR. Re-run to regenerate them.
test("text: pictures", async ({ page }) => {
  await openApp(page);
  const pictures = await page.evaluate(async () => {
    const handle = window.__cadslate;
    if (!handle) throw new Error("debug handle is not published");
    return handle.textPictures();
  });
  expect(pictures).toHaveLength(3);
  for (const picture of pictures) {
    for (const candidate of ["stroke", "atlas", "fill"] as const) {
      const path = fileURLToPath(
        new URL(
          `../../docs/adr/images/0016-${candidate}-zoom-${picture.zoom}.png`,
          import.meta.url,
        ),
      );
      writeFileSync(path, Buffer.from(picture[candidate].split(",")[1] ?? "", "base64"));
    }
  }
});

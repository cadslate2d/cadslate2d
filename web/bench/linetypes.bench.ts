// The linetype experiment (DESIGN §23.2, ADR-0015).
import { writeFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { expect, test } from "@playwright/test";
import { openApp } from "../e2e/support";

for (const segments of [10_000, 100_000]) {
  test(`linetypes: ${segments} dashed segments`, async ({ page }) => {
    await openApp(page);
    const report = await page.evaluate(async (count) => {
      const handle = window.__cadslate;
      if (!handle) throw new Error("debug handle is not published");
      return handle.probeLinetypes(count);
    }, segments);
    console.log(`LINETYPES ${JSON.stringify(report)}`);
    expect(report.candidates).toHaveLength(2);
  });
}

// The pictures in the ADR. Re-run to regenerate them.
test("linetypes: pictures", async ({ page }) => {
  await openApp(page);
  const pictures = await page.evaluate(async () => {
    const handle = window.__cadslate;
    if (!handle) throw new Error("debug handle is not published");
    return handle.linetypePictures();
  });
  expect(pictures).toHaveLength(3);
  for (const picture of pictures) {
    for (const candidate of ["shader", "split"] as const) {
      const path = fileURLToPath(
        new URL(
          `../../docs/adr/images/0015-${candidate}-zoom-${picture.zoom}.png`,
          import.meta.url,
        ),
      );
      writeFileSync(path, Buffer.from(picture[candidate].split(",")[1] ?? "", "base64"));
    }
  }
});

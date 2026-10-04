// The curve-rendering experiment (DESIGN §23.3, ADR-0009).
import { expect, test } from "@playwright/test";
import { openApp } from "../e2e/support";

for (const arcs of [1_000, 10_000]) {
  test(`curves: ${arcs} arcs`, async ({ page }) => {
    await openApp(page);
    const report = await page.evaluate(async (count) => {
      const handle = window.__cadslate;
      if (!handle) throw new Error("debug handle is not published");
      return handle.probeCurves(count);
    }, arcs);
    console.log(`CURVES ${JSON.stringify(report)}`);
    expect(report.views).toHaveLength(3);
  });
}

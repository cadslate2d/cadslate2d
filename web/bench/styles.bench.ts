// The colour-transport experiment (DESIGN §12.4, ADR-0014).
import { expect, test } from "@playwright/test";
import { openApp } from "../e2e/support";

const CASES: [segments: number, styles: number, order: "grouped" | "interleaved"][] = [
  [100_000, 8, "grouped"],
  [100_000, 8, "interleaved"],
  [100_000, 256, "grouped"],
  [100_000, 256, "interleaved"],
  [10_000, 8, "interleaved"],
];

for (const [segments, styles, order] of CASES) {
  test(`styles: ${segments} segments, ${styles} styles, ${order}`, async ({ page }) => {
    await openApp(page);
    const report = await page.evaluate(
      async ([count, kinds, how]) => {
        const handle = window.__cadslate;
        if (!handle) throw new Error("debug handle is not published");
        return handle.probeStyles(count, kinds, how);
      },
      [segments, styles, order] as const,
    );
    console.log(`STYLES ${JSON.stringify(report)}`);
    expect(report.candidates).toHaveLength(3);
  });
}

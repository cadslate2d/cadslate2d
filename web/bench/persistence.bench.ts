// The local-persistence experiment (DESIGN §25.1, ADR-0019): the generated
// drawings, written to IndexedDB and to OPFS on the main thread.
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { expect, test } from "@playwright/test";
import { openApp } from "../e2e/support";

for (const size of ["10k", "50k", "100k"]) {
  test(`persistence: lines-${size}`, async ({ page, browserName }) => {
    const path = fileURLToPath(
      new URL(`../../benches/generated-drawings/lines-${size}.dxf`, import.meta.url),
    );
    const text = readFileSync(path, "utf8");
    await openApp(page);
    const report = await page.evaluate(async (dxf) => {
      const handle = window.__cadslate;
      if (!handle) throw new Error("debug handle is not published");
      return handle.probePersistence(dxf);
    }, text);
    console.log(`PERSISTENCE ${browserName} ${size} ${JSON.stringify(report)}`);
    expect(report.stores).toHaveLength(2);
  });
}

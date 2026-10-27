// The built page's content-security policy (SECURITY.md): present, and
// nothing the app does at work — WASM, the renderer, a drawing, a file
// saved, the AI panel — runs against it.
import { expect, test } from "@playwright/test";
import { downloadFrom, enter, openApp } from "./support";

test("the page runs under its content-security policy without a violation", async ({ page }) => {
  await page.addInitScript(() => {
    const seen: string[] = [];
    (window as unknown as { violations: string[] }).violations = seen;
    document.addEventListener("securitypolicyviolation", (event) =>
      seen.push(`${event.violatedDirective} ${event.blockedURI}`),
    );
  });
  await openApp(page);
  const policy = await page
    .locator('meta[http-equiv="Content-Security-Policy"]')
    .getAttribute("content");
  expect(policy).toContain("script-src 'self' 'wasm-unsafe-eval'");
  expect(policy).toContain("connect-src 'self' https://ai-service.e2e.invalid");
  await enter(page, "CIRCLE", "0,0", "50", "TEXT", "0,0", "5", "0", "Hello");
  await downloadFrom(page, () => enter(page, "DXFOUT"));
  expect(
    await page.evaluate(() => (window as unknown as { violations: string[] }).violations),
  ).toEqual([]);
});

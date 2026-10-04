import { expect, test } from "@playwright/test";

test("the static bundle loads the Rust core", async ({ page }) => {
  await page.goto("./");
  await expect(page.getByTestId("core-version")).toHaveText(/^core \d+\.\d+\.\d+$/);
});

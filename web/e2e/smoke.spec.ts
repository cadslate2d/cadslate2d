import { expect, test } from "@playwright/test";

test("the static bundle loads the Rust core and shows the shell", async ({ page }) => {
  const errors: string[] = [];
  page.on("pageerror", (error) => errors.push(error.message));

  await page.goto("./");

  await expect(page.locator("header")).toHaveAttribute("data-core-version", /^\d+\.\d+\.\d+$/);
  await expect(page.getByRole("region", { name: "Model space" }).locator("canvas")).toBeVisible();
  await expect(page.getByRole("region", { name: "Command line" })).toContainText("Command:");
  await expect(page.getByRole("contentinfo", { name: "Status bar" })).toContainText("ORTHO");
  await expect(page.getByRole("alert")).toHaveCount(0);
  expect(errors).toEqual([]);
});

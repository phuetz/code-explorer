import { expect, test } from "@playwright/test";

test.describe("NexusBrain smoke", () => {
  test("loads the landing screen", async ({ page }) => {
    await page.goto("/");

    await expect(page.getByRole("heading", { name: "NexusBrain" })).toBeVisible();
    await expect(page.getByRole("button", { name: /Select Vault Directory/i })).toBeVisible();
  });

  test("sidebar shows empty state when no vault", async ({ page }) => {
    await page.goto("/");
    await expect(page.getByText(/No vault opened/i)).toBeVisible();
  });
});

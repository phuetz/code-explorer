import { expect, test } from "@playwright/test";

test.describe("Code Explorer desktop UI smoke", () => {
  test("loads the welcome screen in browser mode", async ({ page }) => {
    await page.goto("/");

    await expect(page.getByRole("heading", { name: "Code Explorer", exact: true })).toBeVisible();
    await expect(
      page.getByRole("button", { name: /Analyze Project/i }),
    ).toBeVisible();
    await expect(
      page.getByRole("button", { name: /code-explorer/i }),
    ).toBeVisible();
  });

  test("can open a mocked repository and render the explorer", async ({ page }) => {
    await page.goto("/");

    await page.getByRole("button", { name: /code-explorer/i }).click();

    await expect(page.getByRole("tree", { name: /file explorer/i })).toBeVisible();
    await expect(
      page.getByRole("application", { name: /interactive code dependency graph/i }),
    ).toBeVisible();
  });

  test("opens the command palette from the mode bar", async ({ page }) => {
    await page.goto("/");

    await page.getByRole("button", { name: /command palette/i }).click();

    await expect(
      page.getByRole("dialog", { name: /command palette/i }),
    ).toBeVisible();
  });

  test("chat mode shows the assistant setup guard when no API key is configured", async ({
    page,
  }) => {
    await page.goto("/");
    await page.getByRole("button", { name: /code-explorer/i }).click();

    await page.keyboard.press("Control+3");

    await expect(
      page.getByRole("heading", { name: /Configure AI Assistant/i }),
    ).toBeVisible();
  });

  test("manage mode docs tab shows the generate-docs empty state", async ({ page }) => {
    await page.goto("/");

    await page.getByRole("button", { name: /manage/i }).click();
    await page.getByRole("tab", { name: /documentation/i }).click();

    await expect(
      page.getByRole("heading", { name: /Generate Documentation/i }),
    ).toBeVisible();
    await expect(
      page.getByRole("button", { name: /Generate Docs/i }),
    ).toBeVisible();
  });

  test("manage mode settings can switch the theme", async ({ page }) => {
    await page.goto("/");

    await page.getByRole("button", { name: /manage/i }).click();
    await page.getByRole("tab", { name: /settings/i }).click();
    await page.getByRole("button", { name: /light/i }).click();

    await expect(page.locator("html")).toHaveAttribute("data-theme", "light");
  });
});

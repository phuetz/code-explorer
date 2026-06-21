import { expect, test } from "@playwright/test";

/**
 * Smoke coverage for the panels shipped recently:
 *   - Notebooks (opened via Ctrl+Shift+N / command palette)
 *   - Dashboards (opened via Ctrl+Shift+B)
 *   - Workflows (opened via Ctrl+Shift+W)
 *   - User slash commands
 *   - Snapshots sub-view in Analyze mode
 *   - Rename refactor modal
 *   - Feature-Dev / Code-Review / Simplify mode buttons
 *
 * Tauri is not available in preview mode → Tauri invoke() calls silently no-op.
 * These tests therefore assert on UI shell (modals open, controls render,
 * empty states appear) rather than on backend results.
 */

test.describe("New panels — smoke", () => {
  test("opens the rename refactor modal via command palette", async ({ page }) => {
    await page.goto("/");
    await page.getByRole("button", { name: /command palette/i }).click();
    await page.getByRole("dialog", { name: /command palette/i }).waitFor();
    await page.keyboard.type("rename refactor");
    // Click the matching cmdk item (cmdk renders items with role="option").
    await page
      .getByRole("option", { name: /rename refactor/i })
      .first()
      .click();
    await expect(
      page.getByRole("button", { name: /close/i }),
    ).toBeVisible();
    // Form fields should be present.
    await expect(page.getByPlaceholder("e.g. UserService")).toBeVisible();
    await expect(page.getByPlaceholder("e.g. AccountService")).toBeVisible();
  });

  test("opens the notebooks panel via command palette", async ({ page }) => {
    await page.goto("/");
    await page.getByRole("button", { name: /command palette/i }).click();
    await page.keyboard.type("cypher notebooks");
    await page.getByRole("option", { name: /cypher notebooks/i }).first().click();
    // Sidebar header.
    await expect(page.getByText(/Notebooks$/).first()).toBeVisible();
    // Welcome/empty state text.
    await expect(
      page.getByText(/Open a notebook from the sidebar/i),
    ).toBeVisible();
  });

  test("opens the custom dashboards panel", async ({ page }) => {
    await page.goto("/");
    await page.getByRole("button", { name: /command palette/i }).click();
    await page.keyboard.type("custom dashboards");
    await page
      .getByRole("option", { name: /custom dashboards/i })
      .first()
      .click();
    await expect(page.getByText(/Dashboards$/).first()).toBeVisible();
    await expect(
      page.getByText(/Open a dashboard from the sidebar/i),
    ).toBeVisible();
  });

  test("opens the workflow editor", async ({ page }) => {
    await page.goto("/");
    await page.getByRole("button", { name: /command palette/i }).click();
    await page.keyboard.type("workflow editor");
    await page.getByRole("option", { name: /workflow editor/i }).first().click();
    await expect(page.getByText(/Workflows$/).first()).toBeVisible();
    await expect(
      page.getByText(/Open a workflow from the sidebar/i),
    ).toBeVisible();
  });

  test("opens the user slash commands panel", async ({ page }) => {
    await page.goto("/");
    await page.getByRole("button", { name: /command palette/i }).click();
    await page.keyboard.type("user slash");
    await page
      .getByRole("option", { name: /user slash commands/i })
      .first()
      .click();
    await expect(page.getByText(/User slash commands/i).first()).toBeVisible();
  });

  test("chat mode exposes the 5-way mode switcher", async ({ page }) => {
    await page.goto("/");
    await page.getByRole("button", { name: /code-explorer/i }).click();
    await page.keyboard.press("Control+3");
    // The ModeSwitcher renders each mode as a button with the mode label.
    await expect(page.getByRole("button", { name: /Q&A/i }).first()).toBeVisible();
    await expect(
      page.getByRole("button", { name: /Feature-Dev/i }).first(),
    ).toBeVisible();
    await expect(
      page.getByRole("button", { name: /Review/i }).first(),
    ).toBeVisible();
    await expect(
      page.getByRole("button", { name: /Simplify/i }).first(),
    ).toBeVisible();
  });

  test("analyze mode exposes the Snapshots sub-view", async ({ page }) => {
    await page.goto("/");
    await page.getByRole("button", { name: /code-explorer/i }).click();
    await page.getByRole("button", { name: /analyze/i }).click();
    // AnalyzeNav is a vertical list of buttons; Snapshots is a new entry.
    await expect(
      page.getByRole("button", { name: /snapshots/i }).first(),
    ).toBeVisible();
  });
});

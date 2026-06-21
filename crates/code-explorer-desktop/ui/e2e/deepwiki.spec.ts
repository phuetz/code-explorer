import { expect, test } from "@playwright/test";

test.describe("Code Explorer DeepWiki & Business Features", () => {
  test.beforeEach(async ({ page }) => {
    await page.goto("/");
    // Open the mock repo
    await page.getByRole("button", { name: /code-explorer/i }).click();
  });

  test("can navigate to Analyze > Process Flows", async ({ page }) => {
    // Switch to Analyze mode
    await page.getByRole("button", { name: "Analyze", exact: true }).click();
    
    // Check if Process Flows nav item is visible
    const flowsNav = page.getByRole("button", { name: /Process Flows/i });
    await expect(flowsNav).toBeVisible();
    
    // Click it
    await flowsNav.click();
    
    // Verify header
    await expect(page.getByRole("heading", { name: /Process Flows/i })).toBeVisible();
  });

  test("can interact with a business process flow", async ({ page }) => {
    await page.getByRole("button", { name: "Analyze", exact: true }).click();
    await page.getByRole("button", { name: /Process Flows/i }).click();

    // The mock data should provide some flows (e.g., "Système de Courriers")
    // Wait for the list to render
    const flowItem = page.getByText(/Système de Courriers/i);
    await expect(flowItem).toBeVisible();
    
    // Expand the flow
    await flowItem.click();
    
    // Check for Mermaid diagram container (rendered as SVG or div with mermaid class)
    // In our implementation, we use a MermaidDiagram component
    await expect(page.locator(".mermaid")).toBeVisible({ timeout: 5000 });
    
    // Check for flow steps
    await expect(page.getByText(/Step Sequence/i)).toBeVisible();
    await expect(page.getByText(/View Code/i).first()).toBeVisible();
  });

  test("can see the Obsidian Vault export option in Manage mode", async ({ page }) => {
    // Switch to Manage mode
    await page.getByRole("button", { name: "Manage", exact: true }).click();
    
    // The Export section is inside the Repositories tab
    await expect(page.getByRole("heading", { name: "Export", exact: true })).toBeVisible();
    
    // Check for Obsidian Vault card
    await expect(page.getByRole("heading", { name: /Obsidian Vault/i })).toBeVisible();
    await expect(page.getByRole("button", { name: /Export Obsidian Vault/i })).toBeVisible();
  });

  test("can search for the processes view in the command palette", async ({ page }) => {
    // Open command palette
    await page.keyboard.press("Control+k");
    
    // Type "process"
    await page.getByPlaceholder(/Type a command or search/i).fill("process");
    
    // Should see the process view command in results
    await expect(page.getByText(/Process Flows/i)).toBeVisible();
  });
});

import { defineConfig, devices } from "@playwright/test";

// Dedicated Playwright config for the `html-wiki.spec.ts` suite.
//
// That suite tests a static HTML file produced by `code-explorer generate html`
// by opening it via a `file://` URL — it has zero dependency on the Vite
// dev server. The default `playwright.config.ts` unconditionally starts
// `npm run build && npm run preview` which takes several minutes and is
// wasted work for a file-URL-only test. This slimmer config skips the
// webServer entirely so the spec can run in a few seconds.
//
// Usage (PowerShell):
//   $env:CODE_EXPLORER_HTML_PATH = "D:\taf\sample-app\.codeexplorer\docs\index.html"
//   npx playwright test --config=playwright.html-only.config.ts
//
// Usage (bash):
//   CODE_EXPLORER_HTML_PATH=/d/taf/sample-app/.codeexplorer/docs/index.html \
//     npx playwright test --config=playwright.html-only.config.ts
export default defineConfig({
  testDir: "./e2e",
  testMatch: "html-wiki.spec.ts",
  timeout: 60_000,
  fullyParallel: true,
  reporter: [["list"]],
  use: {
    trace: "retain-on-failure",
  },
  projects: [
    {
      name: "chromium",
      use: { ...devices["Desktop Chrome"] },
    },
  ],
});

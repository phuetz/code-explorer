/**
 * screenshot-audit.mjs — Launch a Chromium browser, navigate to the app,
 * and take screenshots of every mode/view.
 *
 * Usage:
 *   1. Start Vite: cd ui && npx vite --port 1420
 *   2. Run: node scripts/screenshot-audit.mjs
 *
 * Output: screenshots/audit-*.png in the repo root
 */

import { chromium } from "playwright";
import { existsSync, mkdirSync } from "fs";
import { resolve, dirname } from "path";
import { fileURLToPath } from "url";

const __dirname = dirname(fileURLToPath(import.meta.url));
const SCREENSHOT_DIR = resolve(__dirname, "../../../../screenshots");
const APP_URL = "http://localhost:1420";

if (!existsSync(SCREENSHOT_DIR)) mkdirSync(SCREENSHOT_DIR, { recursive: true });

const errors = [];
const warnings = [];

async function main() {
  console.log("Launching Chromium browser...");

  const browser = await chromium.launch({ headless: true });
  const context = await browser.newContext({ viewport: { width: 1400, height: 900 } });
  const page = await context.newPage();

  // Capture console errors
  page.on("console", (msg) => {
    if (msg.type() === "error") {
      const text = msg.text();
      // Skip known non-critical warnings
      if (text.includes("Not running in Tauri") || text.includes("Browser mode")) return;
      errors.push(`[console.error] ${text}`);
    }
    if (msg.type() === "warning" && msg.text().includes("[Code Explorer]")) {
      warnings.push(msg.text());
    }
  });
  page.on("pageerror", (err) => {
    errors.push(`[pageerror] ${err.message}`);
  });

  console.log(`Navigating to ${APP_URL}...`);
  await page.goto(APP_URL, { waitUntil: "networkidle", timeout: 15000 });
  await sleep(2000);
  console.log(`Page title: ${await page.title()}`);

  // Helper: screenshot
  async function shot(name) {
    const path = resolve(SCREENSHOT_DIR, `audit-${name}.png`);
    await page.screenshot({ path, type: "png" });
    console.log(`  [screenshot] ${name}`);
  }

  // Helper: click by text with fallback
  async function clickButton(name) {
    try {
      const btn = page.getByRole("button", { name });
      if (await btn.isVisible({ timeout: 2000 })) {
        await btn.click();
        await sleep(1000);
        return true;
      }
    } catch {
      // ignore
    }
    console.warn(`  [warn] Button "${name}" not found`);
    return false;
  }

  // ─── 1. Initial state (should be Chat mode) ─────────────────
  console.log("\n--- Initial State ---");
  await shot("01-initial");

  // Check if textarea is visible
  const textarea = page.locator("textarea");
  const textareaVisible = await textarea.isVisible().catch(() => false);
  console.log(`  Textarea visible: ${textareaVisible}`);
  if (!textareaVisible) {
    errors.push("[UI] Chat textarea is NOT visible on startup");
  }

  // ─── 2. Navigate to Manage ──────────────────────────────────
  console.log("\n--- Mode: Gerer ---");
  await clickButton("Gérer");
  await shot("02-manage");

  // ─── 3. Navigate to Explorer ────────────────────────────────
  console.log("\n--- Mode: Explorer ---");
  await clickButton("Explorateur");
  await sleep(1500);
  await shot("03-explorer");

  // ─── 4. Navigate to Analyze ─────────────────────────────────
  console.log("\n--- Mode: Analyser ---");
  await clickButton("Analyser");
  await sleep(1000);
  await shot("04-analyze-default");

  // Try clicking analyze sub-tabs
  const analyzeTabs = [
    "Vue d'ensemble", "Points chauds", "Couplage", "Ownership",
    "Couverture", "Processus", "Diagramme", "Rapport", "Santé"
  ];

  for (let i = 0; i < analyzeTabs.length; i++) {
    const tab = analyzeTabs[i];
    console.log(`  Analyze > ${tab}`);
    await clickButton(tab);
    const safeName = tab.replace(/[^a-zA-Z0-9]/g, "").toLowerCase();
    await shot(`05-analyze-${String(i).padStart(2, "0")}-${safeName}`);
  }

  // ─── 5. Navigate to Chat ────────────────────────────────────
  console.log("\n--- Mode: Chat ---");
  await clickButton("Chat");
  await sleep(1000);
  await shot("06-chat");

  // Final textarea check
  const textareaFinal = page.locator("textarea");
  const finalVisible = await textareaFinal.isVisible().catch(() => false);
  console.log(`  Textarea visible (final): ${finalVisible}`);
  if (!finalVisible) {
    errors.push("[UI] Chat textarea NOT visible after navigation");
  }

  // ─── Summary ────────────────────────────────────────────────
  console.log("\n=== AUDIT SUMMARY ===");
  console.log(`Screenshots saved to: ${SCREENSHOT_DIR}`);
  console.log(`Warnings: ${warnings.length}`);
  if (errors.length === 0) {
    console.log("PASS: No errors captured.");
  } else {
    console.log(`FAIL: ${errors.length} error(s):`);
    for (const err of errors) {
      console.log(`  - ${err}`);
    }
  }

  await browser.close();
  process.exit(errors.length > 0 ? 1 : 0);
}

function sleep(ms) {
  return new Promise((r) => setTimeout(r, ms));
}

main().catch((err) => {
  console.error("Fatal error:", err);
  process.exit(1);
});

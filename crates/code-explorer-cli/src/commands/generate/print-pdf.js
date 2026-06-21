#!/usr/bin/env node
/**
 * Code Explorer PDF Generator — Playwright/Chromium
 *
 * Converts a self-contained HTML file to a professional A4 PDF.
 * Waits for mermaid.js diagrams to render before capture.
 *
 * Usage: node print-pdf.js <input.html> <output.pdf>
 */

const path = require("path");
const fs = require("fs");
const { chromium } = require("playwright");

async function printPDF(htmlPath, pdfPath) {
  const browser = await chromium.launch({
    headless: true,
    args: ["--no-sandbox", "--disable-setuid-sandbox"],
  });

  const page = await browser.newPage();

  const fileUrl = `file://${path.resolve(htmlPath)}`;
  // networkidle: waits for mermaid.js CDN to load + render
  await page.goto(fileUrl, { waitUntil: "networkidle", timeout: 60000 });

  // Extra wait: ensure all mermaid diagrams have been processed
  await page.waitForFunction(() => {
    const ready = window.__codeExplorerMermaidReady === true;
    const pending = document.querySelectorAll(".mermaid[data-processed='false']");
    const raw = document.querySelectorAll("pre code.language-mermaid");
    return ready && pending.length === 0 && raw.length === 0;
  }, { timeout: 30000 }).catch(() => {
    // If mermaid never loads (offline), proceed anyway — diagrams will show as code blocks
  });

  await waitForPrintableAssets(page);

  await page.pdf({
    path: pdfPath,
    format: "A4",
    margin: { top: "2.5cm", right: "2cm", bottom: "2.5cm", left: "2.5cm" },
    printBackground: true,
    displayHeaderFooter: false,
    preferCSSPageSize: true,
  });

  await browser.close();

  const size = Math.round(fs.statSync(pdfPath).size / 1024);
  console.log(`OK ${path.basename(pdfPath)} (${size} Ko)`);
}

async function waitForPrintableAssets(page) {
  await page.evaluate(async () => {
    if (document.fonts && document.fonts.ready) {
      await document.fonts.ready.catch(() => undefined);
    }

    const images = Array.from(document.images);
    await Promise.all(
      images
        .filter((img) => !img.complete)
        .map(
          (img) =>
            new Promise((resolve) => {
              img.addEventListener("load", resolve, { once: true });
              img.addEventListener("error", resolve, { once: true });
            })
        )
    );
  });
}

const [,, htmlPath, pdfPath] = process.argv;
if (!htmlPath || !pdfPath) {
  console.error("Usage: node print-pdf.js <input.html> <output.pdf>");
  process.exit(1);
}

printPDF(htmlPath, pdfPath).catch(err => {
  console.error("ERROR", err.message);
  process.exit(1);
});

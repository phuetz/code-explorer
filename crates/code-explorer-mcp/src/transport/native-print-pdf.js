#!/usr/bin/env node
/**
 * Code Explorer native chat PDF renderer.
 *
 * Converts the self-contained printable chat HTML into a real PDF byte stream
 * using Playwright/Chromium. The UI already prepares rendered Mermaid SVGs and
 * source fallbacks, so this script focuses on reliable asset settling.
 */

const path = require("path");
const fs = require("fs");
const { chromium } = require("playwright");

async function printPdf(htmlPath, pdfPath) {
  const browser = await chromium.launch({
    headless: true,
    args: ["--no-sandbox", "--disable-setuid-sandbox"],
  });

  try {
    const page = await browser.newPage({ viewport: { width: 1240, height: 1754 } });
    const fileUrl = `file://${path.resolve(htmlPath)}`;
    await page.goto(fileUrl, { waitUntil: "networkidle", timeout: 60000 });
    await page.emulateMedia({ media: "print" });
    await waitForPrintableAssets(page);

    await page.pdf({
      path: pdfPath,
      format: "A4",
      printBackground: true,
      displayHeaderFooter: false,
      preferCSSPageSize: true,
    });
  } finally {
    await browser.close();
  }

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
              if (img.decode) img.decode().then(resolve, resolve);
            })
        )
    );
  });
}

const [, , htmlPath, pdfPath] = process.argv;
if (!htmlPath || !pdfPath) {
  console.error("Usage: node native-print-pdf.js <input.html> <output.pdf>");
  process.exit(1);
}

printPdf(htmlPath, pdfPath).catch((err) => {
  console.error("ERROR", err && err.message ? err.message : String(err));
  process.exit(1);
});

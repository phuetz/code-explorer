#!/usr/bin/env node
/** Offline PDF and high resolution Mermaid PNG renderer; requires local Playwright/Chromium. */
const path = require('path');
const fs = require('fs');
const { chromium } = require('playwright');

async function render(input, output, mermaidOnly) {
  const browser = await chromium.launch({
    headless: true,
    args: ['--no-sandbox', '--disable-setuid-sandbox', '--disable-background-networking'],
  });
  const attempted = [];
  try {
    const context = await browser.newContext({ deviceScaleFactor: 3, serviceWorkers: 'block' });
    // Never let a diagram, stylesheet or image disclose source to an HTTP service.
    await context.route(/^https?:\/\//i, route => {
      attempted.push(route.request().url());
      return route.abort('blockedbyclient');
    });
    const page = await context.newPage();
    const errors = [];
    page.on('pageerror', error => errors.push(error.message));
    if (mermaidOnly) {
      await page.setContent('<!doctype html><html><body style="margin:0;background:white"><div id="diagram" style="display:inline-block;padding:16px"></div></body></html>');
      await page.addScriptTag({ path: path.join(__dirname, 'mermaid.min.js') });
      const source = JSON.parse(fs.readFileSync(input, 'utf8'));
      await page.evaluate(async source => {
        mermaid.initialize({ startOnLoad: false, securityLevel: 'strict', theme: 'default' });
        const { svg } = await mermaid.render('local-diagram', source);
        document.getElementById('diagram').innerHTML = svg;
      }, source);
      await page.evaluate(() => document.fonts.ready);
      const diagram = page.locator('#diagram');
      const size = await diagram.boundingBox();
      await page.setViewportSize({ width: Math.ceil(size.width) + 32, height: Math.ceil(size.height) + 32 });
      await diagram.screenshot({ path: output, type: 'png', timeout: 30000 });
    } else {
      await page.goto(`file://${path.resolve(input)}`, { waitUntil: 'load', timeout: 60000 });
      await page.waitForFunction(() => window.__codeExplorerMermaidReady === true, null, { timeout: 30000 });
      await page.evaluate(() => document.fonts.ready);
      // Rasterize the locally rendered diagrams so PDFs contain embedded images too.
      for (const diagram of await page.locator('.mermaid:has(svg)').all()) {
        const png = await diagram.screenshot({ type: 'png' });
        await diagram.evaluate((element, base64) => {
          const image = document.createElement('img');
          image.alt = 'Diagramme Mermaid';
          image.src = 'data:image/png;base64,' + base64;
          image.style.cssText = 'max-width:100%;height:auto';
          element.replaceChildren(image);
        }, png.toString('base64'));
      }
      await page.evaluate(async () => {
        await Promise.all(Array.from(document.images).map(image => image.decode().catch(() => undefined)));
      });
      await page.pdf({ path: output, format: 'A4', printBackground: true, preferCSSPageSize: true });
    }
    if (errors.length) throw new Error('Browser script failed: ' + errors.join('; '));
    if (attempted.length) throw new Error('External requests blocked: ' + attempted.join(', '));
  } finally {
    if (process.env.CODE_EXPLORER_NETWORK_AUDIT) {
      fs.appendFileSync(process.env.CODE_EXPLORER_NETWORK_AUDIT, attempted.map(url => url + '\n').join(''));
    }
    await browser.close();
  }
}

const [,, input, output, mode] = process.argv;
if (!input || !output) {
  console.error('Usage: node print-pdf.js <input.html|diagram.json> <output.pdf|png> [--mermaid]');
  process.exit(1);
}
render(input, output, mode === '--mermaid').catch(error => {
  console.error('ERROR', error.message);
  process.exitCode = 1;
});

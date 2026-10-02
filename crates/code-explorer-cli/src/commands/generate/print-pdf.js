#!/usr/bin/env node
/** Offline PDF and high resolution Mermaid PNG renderer; requires local Playwright/Chromium. */
const path = require('path');
const fs = require('fs');
const { pathToFileURL } = require('url');
const { chromium } = require('playwright');

async function render(input, output, mermaidOnly) {
  const timeout = Number(process.env.CODE_EXPLORER_MERMAID_TIMEOUT_MS || 30000);
  if (!Number.isFinite(timeout) || timeout <= 0 || timeout > 300000) {
    throw new Error('CODE_EXPLORER_MERMAID_TIMEOUT_MS must be between 1 and 300000');
  }
  const browser = await chromium.launch({
    headless: true,
    args: ['--no-sandbox', '--disable-setuid-sandbox', '--disable-background-networking'],
  });
  const attempted = [];
  async function context() {
    const result = await browser.newContext({ deviceScaleFactor: 3, serviceWorkers: 'block' });
    await result.route(/^https?:\/\//i, route => {
      attempted.push(route.request().url());
      return route.abort('blockedbyclient');
    });
    return result;
  }
  async function rasterize(source, config) {
    // A fresh context owns each Mermaid queue. Closing it on timeout cancels
    // its rendering without leaving a blocked promise in the document's queue.
    const isolated = await context();
    let timer;
    try {
      const task = (async () => {
        const page = await isolated.newPage();
        page.setDefaultTimeout(timeout);
        const errors = [];
        page.on('pageerror', error => errors.push(error.message));
        await page.setContent('<!doctype html><html><body style="margin:0;background:white"><div id="diagram" style="display:inline-block;padding:16px"></div></body></html>');
        await page.addScriptTag({ path: path.join(__dirname, 'mermaid.min.js') });
        await page.evaluate(async ({ source, config }) => {
          mermaid.initialize(config);
          const { svg } = await mermaid.render('local-diagram', source);
          document.getElementById('diagram').innerHTML = svg;
        }, { source, config });
        await page.evaluate(() => document.fonts.ready);
        const diagram = page.locator('#diagram');
        const size = await diagram.boundingBox();
        await page.setViewportSize({ width: Math.ceil(size.width) + 32, height: Math.ceil(size.height) + 32 });
        const png = await diagram.screenshot({ type: 'png' });
        if (errors.length) throw new Error('Browser script failed: ' + errors.join('; '));
        return png;
      })();
      return await Promise.race([task, new Promise((_, reject) => {
        timer = setTimeout(() => reject(new Error(`local render timed out after ${timeout} ms`)), timeout);
      })]);
    } finally {
      clearTimeout(timer);
      await isolated.close();
    }
  }
  try {
    if (mermaidOnly) {
      const source = JSON.parse(fs.readFileSync(input, 'utf8'));
      fs.writeFileSync(output, await rasterize(source, { startOnLoad: false, securityLevel: 'strict', theme: 'default' }));
    } else {
      const documentContext = await context();
      const page = await documentContext.newPage();
      const errors = [];
      page.on('pageerror', error => errors.push(error.message));
      await page.goto(pathToFileURL(path.resolve(input)).href, { waitUntil: 'load', timeout: 60000 });
      await page.waitForFunction(() => window.__codeExplorerMermaidReady === true, null, { timeout: 30000 });
      const config = await page.evaluate(() => window.__codeExplorerMermaidConfig);
      await page.evaluate(() => document.fonts.ready);
      // Stable handles, not nth() locators into a :has(svg) selection whose
      // positions change every time an SVG is replaced with its PNG.
      const figures = await page.locator('.mermaid-figure').elementHandles();
      for (const [index, figure] of figures.entries()) {
        const source = await figure.$eval('.mermaid-source code', code => code.textContent);
        try {
          const png = await rasterize(source, config);
          await figure.evaluate((element, base64) => {
            const image = document.createElement('img');
            image.alt = 'Diagramme Mermaid';
            image.src = 'data:image/png;base64,' + base64;
            image.style.cssText = 'max-width:100%;height:auto';
            element.querySelector('.mermaid').replaceChildren(image);
          }, png.toString('base64'));
        } catch (error) {
          // Preserve this source visibly and continue with every later figure.
          await figure.evaluate(element => {
            element.classList.add('mermaid-error');
            element.querySelector('.mermaid').replaceChildren();
            element.querySelector('.mermaid-source').open = true;
          });
          console.warn(`Warning: Mermaid diagram ${index + 1} could not be rendered (${error.message.replace(/\s+/g, ' ').slice(0, 600)}). Source retained as a code block.`);
        }
      }
      await page.evaluate(async () => {
        await Promise.all(Array.from(document.images).map(image => image.decode().catch(() => undefined)));
      });
      if (errors.length) throw new Error('Browser script failed: ' + errors.join('; '));
      if (attempted.length) throw new Error('External requests blocked: ' + attempted.join(', '));
      await page.pdf({ path: output, format: 'A4', printBackground: true, preferCSSPageSize: true });
    }
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

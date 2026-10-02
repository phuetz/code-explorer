#!/usr/bin/env node
// Exercise the actual chat and desktop Markdown components, then decode their
// embedded/local PNGs in Chromium. Vite SSR runs in middleware mode, no server.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { pathToFileURL } = require('node:url');
const { createRequire } = require('node:module');
const { chromium } = require('playwright');
const png = 'iVBORw0KGgoAAAANSUhEUgAAAAIAAAACCAIAAAD91JpzAAAAEElEQVR4nGMwKNgARAwQCgAizgVBSgKCUQAAAABJRU5ErkJggg==';

async function main() {
  // Desktop store initialization reads preferences even for server rendering.
  // Keep those preferences isolated from the user's browser session.
  globalThis.localStorage = { getItem: () => null, setItem: () => {}, removeItem: () => {} };
  const repo = path.resolve(__dirname, '../..');
  const output = process.env.CODE_EXPLORER_MARKDOWN_ARTIFACT_DIR || fs.mkdtempSync(path.join(os.tmpdir(), 'ce-markdown-images-'));
  fs.mkdirSync(output, { recursive: true });
  fs.writeFileSync(path.join(output, 'pixel.png'), Buffer.from(png, 'base64'));
  const browser = await chromium.launch({ headless: true, args: ['--no-sandbox'] });
  try {
    const context = await browser.newContext({ bypassCSP: true, serviceWorkers: 'block' });
    const requests = [];
    await context.route(/^https?:\/\//i, route => { requests.push(route.request().url()); return route.abort(); });
    const proofs = {};
    for (const [name, directory, modulePath, exportName] of [
      ['chat', 'chat-ui', '/src/components/ui/Markdown.tsx', 'Markdown'],
      ['desktop', 'crates/code-explorer-desktop/ui', '/src/components/chat/ChatMarkdown.tsx', 'ChatMarkdown'],
    ]) {
      const project = path.join(repo, directory);
      const projectRequire = createRequire(path.join(project, 'package.json'));
      const { createServer } = await import(pathToFileURL(projectRequire.resolve('vite')));
      const vite = await createServer({ root: project, configFile: path.join(project, 'vite.config.ts'),
        server: { middlewareMode: true, hmr: false, watch: null },
        optimizeDeps: { noDiscovery: true, entries: [] },
      });
      let html;
      try {
        const module = await vite.ssrLoadModule(modulePath);
        const React = projectRequire('react');
        const { renderToStaticMarkup } = projectRequire('react-dom/server');
        const markdown = `![Capture intégrée](data:image/png;base64,${png})\n\n![Image locale](pixel.png)\n\n![Distante](https://example.test/pixel.png)\n\n![SVG refusé](data:image/svg+xml;base64,PHN2Zy8+)\n\n[Documentation](https://example.test/docs)`;
        const props = name === 'chat' ? { children: markdown } : { content: markdown };
        html = renderToStaticMarkup(React.createElement(module[exportName], props));
      } finally {
        await vite.close();
      }
      const file = path.join(output, `${name}.html`);
      fs.writeFileSync(file, '<!doctype html><meta charset="utf-8">' + html);
      const page = await context.newPage();
      await page.goto(pathToFileURL(file).href, { waitUntil: 'load' });
      assert.equal(await page.locator('img').count(), 2, `${name}: local and raster capture preserved, remote/SVG omitted`);
      const dimensions = await page.locator('img').evaluateAll(images => images.map(image => ({
        alt: image.alt, width: image.naturalWidth, height: image.naturalHeight,
      })));
      assert(dimensions.every(image => image.width === 2 && image.height === 2), `${name}: PNGs must decode`);
      assert.equal(await page.getByRole('link', { name: 'Documentation' }).getAttribute('href'), 'https://example.test/docs');
      assert.deepEqual(requests, [], `${name}: remote images must be omitted without CSP assistance`);
      proofs[name] = dimensions;
      await page.close();
      console.log(`PASS ${name}: actual Markdown component, data PNG + local PNG decoded (2×2), remote/SVG omitted, links preserved, no external requests.`);
    }
    fs.writeFileSync(path.join(output, 'proofs.json'), JSON.stringify(proofs, null, 2));
  } finally {
    await browser.close();
  }
}
main().catch(error => { console.error(error); process.exitCode = 1; });

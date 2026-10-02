#!/usr/bin/env node
// End-to-end regression: a refusing proxy observes CLI traffic and Playwright
// intercepts browser requests. Local Playwright + Chromium + Python are required.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const http = require('node:http');
const { spawn } = require('node:child_process');
const { chromium } = require('playwright');
const { pathToFileURL } = require('node:url');

async function auditHtmlWithoutCSP(browser, file) {
  // A CSP may hide a regression before requests reach Playwright routes.
  // Audit the generated pages with CSP bypassed, while still refusing traffic.
  const context = await browser.newContext({ bypassCSP: true, serviceWorkers: 'block' });
  const attempted = [];
  await context.route(/^https?:\/\//i, route => { attempted.push(route.request().url()); return route.abort(); });
  try {
    const page = await context.newPage();
    await page.goto(pathToFileURL(file).href, { waitUntil: 'load' });
    assert.deepEqual(attempted, [], 'HTML external requests detected with CSP bypassed');
    const pages = await page.evaluate(() => Object.keys(PAGES));
    for (const id of pages) {
      await page.evaluate(id => { currentPage = ''; document.getElementById('content').replaceChildren(); showPage(id); }, id);
      await page.waitForFunction(() => document.getElementById('content').childElementCount > 0);
      const expected = await page.evaluate(id => (PAGES[id].html.match(/language-mermaid/g) || []).length, id);
      if (expected) {
        await page.waitForFunction(count => document.querySelectorAll('.mermaid svg').length === count, expected);
      }
      // Inspect DOM references as well as traffic; anchors remain permitted.
      const references = await page.evaluate(() => Array.from(document.querySelectorAll(
        'img,script[src],link,source,video,audio,iframe,object,embed,image,use'
      )).flatMap(element => ['src', 'href', 'xlink:href', 'data', 'poster'].flatMap(attribute => {
        const value = element.getAttribute(attribute);
        if (!value) return [];
        const url = new URL(value, document.baseURI);
        return /^https?:$/.test(url.protocol) ? [url.href] : [];
      })));
      assert.deepEqual(references, [], `HTML page ${id} contains external resource references`);
      assert.deepEqual(attempted, [], 'HTML external requests detected with CSP bypassed');
    }
    return { pages: pages.length, attempted };
  } finally {
    await context.close();
  }
}

async function main() {
  const binary = path.resolve(process.argv[2]);
  const root = process.env.CODE_EXPLORER_OFFLINE_ARTIFACT_DIR || fs.mkdtempSync(path.join(os.tmpdir(), 'ce-offline-'));
  fs.mkdirSync(root, { recursive: true });
  const repo = path.join(root, 'demo');
  fs.mkdirSync(repo);
  fs.writeFileSync(path.join(repo, 'service.ts'), 'import { save } from "./store";\nexport function checkout() { return save(); }\n');
  fs.writeFileSync(path.join(repo, 'store.ts'), 'export function save() { return true; }\n');
  const attempts = [];
  const proxy = http.createServer((request, response) => {
    attempts.push(request.url);
    response.writeHead(502); response.end('Network disabled by offline export test');
  });
  proxy.on('connect', (request, socket) => { attempts.push(request.url); socket.destroy(); });
  await new Promise(resolve => proxy.listen(0, '127.0.0.1', resolve));
  const proxyUrl = `http://127.0.0.1:${proxy.address().port}`;
  const audit = path.join(root, 'network-audit.txt');
  fs.writeFileSync(audit, '');
  const env = { ...process.env, CODE_EXPLORER_HOME: path.join(root, 'home'),
    CODE_EXPLORER_NETWORK_AUDIT: audit, HTTP_PROXY: proxyUrl, HTTPS_PROXY: proxyUrl,
    ALL_PROXY: proxyUrl, http_proxy: proxyUrl, https_proxy: proxyUrl, all_proxy: proxyUrl,
    NO_PROXY: '', no_proxy: '' };
  delete env.CODE_EXPLORER_KROKI_URL;
  delete env.CODE_EXPLORER_MERMAID_PLACEHOLDER;
  async function run(program, args, log, runEnv = env) {
    return new Promise((resolve, reject) => {
      const child = spawn(program, args, { env: runEnv });
      let output = '';
      child.stdout.on('data', chunk => { output += chunk; });
      child.stderr.on('data', chunk => { output += chunk; });
      child.on('error', reject);
      child.on('close', code => {
        if (log) fs.writeFileSync(path.join(root, log), output);
        if (code) reject(new Error(`${program} ${args.join(' ')} failed (${code}):\n${output}`));
        else resolve(output);
      });
    });
  }
  let browser;
  try {
    await run(binary, ['analyze', repo, '--skip-git'], 'analyze.log');
    const docs = path.join(repo, '.codeexplorer', 'docs');
    await run(binary, ['generate', 'html', '--path', repo], 'html.log');
    // A real local image accepted by the Markdown converter must survive CSP.
    const pixel = Buffer.from('iVBORw0KGgoAAAANSUhEUgAAAAIAAAACCAIAAAD91JpzAAAAEElEQVR4nGMwKNgARAwQCgAizgVBSgKCUQAAAABJRU5ErkJggg==', 'base64');
    fs.writeFileSync(path.join(docs, 'pixel.png'), pixel);
    fs.appendFileSync(path.join(docs, 'overview.md'), '\n\n![Image locale de contrôle](pixel.png)\n');
    fs.writeFileSync(path.join(docs, 'rendering-checks.md'), [
      '# Vérifications Mermaid strict',
      '```mermaid', 'flowchart LR',
      'L["<b>Libellé HTML</b><br/>seconde ligne"] --> R[Sortie]',
      'click L showDetails "Détails"',
      'click R href "#local-details" "Documentation"',
      '```',
      '```mermaid', 'sequenceDiagram', 'Client->>Service: Appel local', 'Service-->>Client: Résultat', '```',
      '```mermaid', 'classDiagram', 'class Store {', '+save()', '}', 'Service --> Store', '```',
    ].join('\n'));
    await run(binary, ['generate', 'html', '--path', repo, '--enrich-only'], 'html-local-image.log');
    fs.copyFileSync(path.join(docs, 'index.html'), path.join(root, 'index.html'));
    fs.copyFileSync(path.join(docs, 'pixel.png'), path.join(root, 'pixel.png'));
    browser = await chromium.launch({ headless: true, args: ['--no-sandbox'], proxy: { server: proxyUrl } });
    const context = await browser.newContext({ serviceWorkers: 'block' });
    await context.route(/^https?:\/\//i, route => { attempts.push(route.request().url()); return route.abort(); });
    const page = await context.newPage();
    await page.addInitScript(() => {
      window.__mermaidCallbackCalls = 0;
      window.showDetails = () => { window.__mermaidCallbackCalls += 1; };
    });
    const errors = [];
    page.on('pageerror', error => errors.push(error.message));
    await page.goto(`file://${path.join(root, 'index.html')}`, { waitUntil: 'load' });
    assert.deepEqual(attempts, [], 'HTML attempted external resource requests');
    const image = page.getByRole('img', { name: 'Image locale de contrôle', exact: true });
    await image.waitFor();
    const dimensions = await image.evaluate(img => ({ width: img.naturalWidth, height: img.naturalHeight }));
    assert.deepEqual(dimensions, { width: 2, height: 2 }, 'CSP must preserve local repository images');
    fs.writeFileSync(path.join(root, 'local-image-proof.json'), JSON.stringify(dimensions, null, 2));
    // The same exported page must preserve same-origin images when served over HTTP.
    const site = http.createServer((request, response) => {
      const filename = request.url === '/pixel.png' ? 'pixel.png' : request.url === '/index.html' ? 'index.html' : null;
      if (!filename) { response.writeHead(404); response.end(); return; }
      response.writeHead(200, { 'Content-Type': filename.endsWith('.png') ? 'image/png' : 'text/html; charset=utf-8' });
      response.end(fs.readFileSync(path.join(root, filename)));
    });
    await new Promise(resolve => site.listen(0, '127.0.0.1', resolve));
    let siteContext;
    let siteBrowser;
    try {
      const origin = `http://127.0.0.1:${site.address().port}`;
      // Keep the refusing proxy for file exports. This browser can access only
      // the fixture's loopback origin; its routes still refuse every other URL.
      siteBrowser = await chromium.launch({ headless: true, args: ['--no-sandbox'] });
      siteContext = await siteBrowser.newContext({ serviceWorkers: 'block' });
      const external = [];
      await siteContext.route(/^https?:\/\//i, route => {
        if (new URL(route.request().url()).origin === origin) return route.continue();
        external.push(route.request().url()); return route.abort();
      });
      const served = await siteContext.newPage();
      const response = await served.goto(origin + '/index.html', { waitUntil: 'load' });
      assert.equal(response.status(), 200, 'The loopback fixture server must serve the HTML');
      const servedDimensions = await served.getByRole('img', { name: 'Image locale de contrôle', exact: true })
        .evaluate(img => ({ width: img.naturalWidth, height: img.naturalHeight }));
      assert.deepEqual(servedDimensions, { width: 2, height: 2 }, 'CSP must preserve same-origin HTTP images');
      assert.deepEqual(external, [], 'Served HTML attempted an external request');
      fs.writeFileSync(path.join(root, 'same-origin-image-proof.json'), JSON.stringify(servedDimensions, null, 2));
    } finally {
      if (siteContext) await siteContext.close();
      if (siteBrowser) await siteBrowser.close();
      await new Promise(resolve => site.close(resolve));
    }
    const unprotected = await auditHtmlWithoutCSP(browser, path.join(root, 'index.html'));
    fs.writeFileSync(path.join(root, 'audit-without-csp.json'), JSON.stringify(unprotected, null, 2));
    // Prove that this audit rejects a resource even when production CSP hides it.
    const mutantFile = path.join(root, 'mutant-external-image.html');
    const mutant = fs.readFileSync(path.join(root, 'index.html'), 'utf8')
      .replace(/img-src [^;]+;/, 'img-src data:;')
      .replace('\n<body>', '\n<body>\n<img src="https://example.test/masked.png" alt="Mutant">');
    fs.writeFileSync(mutantFile, mutant);
    const maskedContext = await browser.newContext({ serviceWorkers: 'block' });
    const maskedAttempts = [];
    try {
      await maskedContext.route(/^https?:\/\//i, route => { maskedAttempts.push(route.request().url()); return route.abort(); });
      const maskedPage = await maskedContext.newPage();
      await maskedPage.goto(pathToFileURL(mutantFile).href, { waitUntil: 'load' });
      assert.deepEqual(maskedAttempts, [], 'The negative control must be hidden by CSP');
    } finally {
      await maskedContext.close();
    }
    await assert.rejects(auditHtmlWithoutCSP(browser, mutantFile), /HTML external requests detected with CSP bypassed/);
    fs.writeFileSync(path.join(root, 'mutation-csp-proof.json'), JSON.stringify({
      protectedRequests: maskedAttempts, bypassedAuditRejects: true, externalResource: 'https://example.test/masked.png',
    }, null, 2));
    // The overview contains the dependency diagram; exercise all pages containing Mermaid.
    const diagramPages = await page.evaluate(() => Object.keys(PAGES).filter(id => PAGES[id].html.includes('language-mermaid')));
    assert(diagramPages.length > 0, 'The demo documentation must include diagrams');
    let rendered = 0;
    for (const id of diagramPages) {
      await page.evaluate(id => { currentPage = ''; document.getElementById('content').replaceChildren(); showPage(id); }, id);
      const expected = await page.evaluate(id => (PAGES[id].html.match(/language-mermaid/g) || []).length, id);
      await page.waitForFunction(count => document.querySelectorAll('.mermaid svg').length === count, expected);
      rendered += await page.locator('.mermaid svg').count();
      if (id === 'rendering-checks') {
        const strict = await page.locator('.mermaid svg').first().evaluate(svg => ({
          label: svg.querySelector('[id*="-flowchart-L-"]').textContent,
          htmlLabel: !!svg.querySelector('b'),
          lineBreak: !!svg.querySelector('br'),
          links: Array.from(svg.querySelectorAll('a')).map(a => a.getAttribute('href') || a.getAttribute('xlink:href')),
        }));
        assert(strict.label.includes('Libellé HTML') && strict.label.includes('seconde ligne'), 'Strict mode must preserve readable HTML label text');
        assert.equal(strict.htmlLabel, true, 'Bundled Mermaid strict mode preserves sanitized bold labels');
        assert.equal(strict.lineBreak, true, 'Bundled Mermaid strict mode preserves label line breaks');
        assert.deepEqual(strict.links, ['#local-details'], 'Strict mode preserves documentation links');
        await page.locator('.mermaid svg').first().locator('g.node[id*="-flowchart-L-"]').click();
        strict.callbackCalls = await page.evaluate(() => window.__mermaidCallbackCalls);
        assert.equal(strict.callbackCalls, 0, 'Strict mode disables JavaScript click callbacks');
        fs.writeFileSync(path.join(root, 'mermaid-strict-proof.json'), JSON.stringify(strict, null, 2));
      }
    }
    await page.screenshot({ path: path.join(root, 'html-diagram.png'), fullPage: true });
    assert.equal(errors.length, 0, `HTML script errors: ${errors.join('; ')}`);
    await run(binary, ['generate', 'docx', '--path', repo], 'docx.log');
    assert.deepEqual(attempts, [], 'DOCX attempted an external request (implicit Kroki)');
    fs.copyFileSync(path.join(docs, 'documentation.docx'), path.join(root, 'documentation.docx'));
    // Also exercise the standalone PDF renderer on the generated Markdown documents.
    await run(binary, ['generate', 'pdf', '--input', docs, '--output-dir', root], 'pdf.log');
    await run('python3', ['-c', `
import sys,zipfile,struct,re
from pathlib import Path
root=Path(sys.argv[1])
with zipfile.ZipFile(root/'documentation.docx') as z:
 images=[n for n in z.namelist() if re.fullmatch(r'word/media/image[0-9]+\\.png',n)]
 assert images, 'DOCX contains no diagram images (old Kroki fallback)'
 xml=z.read('word/document.xml')
 assert b'<w:drawing>' in xml
 assert b'Rendu Mermaid indisponible' not in xml, 'DOCX has a Mermaid fallback'
 for name in images:
  png=z.read(name)
  assert png[:8]==b'\\x89PNG\\r\\n\\x1a\\n'
  w,h=struct.unpack('>II',png[16:24]); assert w>300 and h>100,(w,h)
 (root/'local-diagram.png').write_bytes(z.read(images[0]))
 print('DOCX: %d embedded PNG diagram(s)'%len(images))
pdf=(root/'documentation.pdf').read_bytes()
assert pdf.startswith(b'%PDF-')
assert b'/Subtype /Image' in pdf, 'PDF contains no embedded diagram image'
print('PDF: embedded diagram images present')
`, root], 'images.log');
    const fallback = path.join(root, 'fallback');
    const warning = await run(binary, ['generate', 'docx', '--path', repo, '--output-dir', fallback], 'fallback.log', { ...env, PATH: '' });
    assert.match(warning, /Warning: Mermaid render failed/, 'Missing local engine must produce a clear warning');
    await run('python3', ['-c', `
import sys,zipfile
with zipfile.ZipFile(sys.argv[1]) as z:
 xml=z.read('word/document.xml')
 assert 'Rendu Mermaid indisponible'.encode() in xml
 assert b'graph TD' in xml, 'Fallback must retain Mermaid source'
 assert not any(n.startswith('word/media/') for n in z.namelist())
print('DOCX fallback without Node: source retained, clear warning, no images')
`, path.join(fallback, 'documentation.docx')], 'fallback-check.log');
    assert.equal(fs.readFileSync(audit, 'utf8'), '', 'Renderer attempted external requests');
    assert.deepEqual(attempts, [], 'Proxy/browser observed external requests');
    // A user-configured, self-hosted Kroki address remains usable. Test it only
    // after the zero-network/default assertions, with a loopback stub.
    const received = [];
    const kroki = http.createServer((request, response) => {
      let body = '';
      request.on('data', chunk => { body += chunk; });
      request.on('end', () => {
        received.push(body);
        response.writeHead(200, { 'Content-Type': 'image/png' });
        response.end(fs.readFileSync(path.join(root, 'local-diagram.png')));
      });
    });
    await new Promise(resolve => kroki.listen(0, '127.0.0.1', resolve));
    try {
      await run(binary, ['generate', 'docx', '--path', repo, '--output-dir', path.join(root, 'explicit-kroki')], 'explicit-kroki.log', {
        ...env, CODE_EXPLORER_KROKI_URL: `http://127.0.0.1:${kroki.address().port}/mermaid/png`,
        NO_PROXY: '127.0.0.1', no_proxy: '127.0.0.1',
      });
      assert(received.length > 0, 'Explicit Kroki must receive diagram source');
      assert(received.every(source => /^(graph\s|flowchart\s|sequenceDiagram)/.test(source.trim())), 'Kroki stub must receive Mermaid');
      assert.deepEqual(attempts, [], 'Only the configured loopback endpoint is allowed');
    } finally {
      await new Promise(resolve => kroki.close(resolve));
    }
    console.log(`PASS: HTML (${rendered} SVG diagrams), PDF and DOCX; zero external requests. Artifacts: ${root}`);
  } finally {
    fs.writeFileSync(path.join(root, 'proxy-attempts.json'), JSON.stringify(attempts, null, 2));
    if (browser) await browser.close();
    await new Promise(resolve => proxy.close(resolve));
    // Keep artifacts for failure diagnosis and when a delivery directory was supplied.
  }
}
main().catch(error => { console.error(error); process.exitCode = 1; });

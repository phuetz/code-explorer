#!/usr/bin/env node
// Real CLI regression for repository PDFs, valid batches and damaged diagrams.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { spawnSync } = require('node:child_process');

const binary = path.resolve(process.argv[2]);
const parent = process.env.CODE_EXPLORER_PDF_ARTIFACT_DIR || os.tmpdir();
fs.mkdirSync(parent, { recursive: true });
const root = fs.mkdtempSync(path.join(parent, 'ce-pdf-'));
const env = { ...process.env, CODE_EXPLORER_HOME: path.join(root, 'home'),
  CODE_EXPLORER_NETWORK_AUDIT: path.join(root, 'network-audit.txt') };
delete env.CODE_EXPLORER_KROKI_URL;
fs.writeFileSync(env.CODE_EXPLORER_NETWORK_AUDIT, '');
function run(args, name, extra = {}) {
  const result = spawnSync(binary, args, { env: { ...env, ...extra }, encoding: 'utf8', timeout: 120000 });
  const log = result.stdout + result.stderr;
  fs.writeFileSync(path.join(root, name + '.log'), log);
  assert.equal(result.status, 0, name + ' must produce a PDF:\n' + log);
  return log;
}
function inspect(file, expectedImages, texts = []) {
  const pdf = fs.readFileSync(file);
  assert(pdf.subarray(0, 5).equals(Buffer.from('%PDF-')));
  const listed = spawnSync('pdfimages', ['-list', file], { encoding: 'utf8' });
  assert.equal(listed.status, 0, listed.stderr);
  const count = listed.stdout.split('\n').filter(line => /^\s*\d+\s+\d+\s+image\s/.test(line)).length;
  assert.equal(count, expectedImages, 'Every valid diagram must have its own embedded PDF image');
  const extracted = spawnSync('pdftotext', [file, '-'], { encoding: 'utf8' });
  assert.equal(extracted.status, 0, extracted.stderr);
  for (const text of texts) assert(extracted.stdout.includes(text), 'Missing fallback source/content: ' + text);
  return { images: count, bytes: pdf.length };
}
try {
  const repo = path.join(root, 'repo');
  fs.mkdirSync(repo);
  for (const [name, target] of [['controllers', 'services'], ['services', 'storage'], ['storage', null]]) {
    const dir = path.join(repo, name); fs.mkdirSync(dir);
    fs.writeFileSync(path.join(dir, 'index.ts'), (target ? `import { execute as next } from "../${target}";\n` : '') +
      `export function execute() { return ${target ? 'next()' : 'true'}; }\n`);
  }
  run(['analyze', repo, '--skip-git'], 'analyze');
  run(['generate', 'pdf', '--path', repo], 'repository-pdf');
  const docs = path.join(repo, '.codeexplorer', 'docs');
  const index = JSON.parse(fs.readFileSync(path.join(docs, '_index.json'), 'utf8'));
  function contents(pages) { return pages.flatMap(page => [page.path && fs.existsSync(path.join(docs, page.path)) ? fs.readFileSync(path.join(docs, page.path), 'utf8') : '', ...contents(page.children || [])]); }
  const expected = contents(index.pages).join('\n').match(/```(?:mermaid|mmd)\b/g)?.length || 0;
  assert(expected >= 2, 'Repository fixture must generate multiple diagrams');
  const proof = { repository: inspect(path.join(docs, 'documentation.pdf'), expected) };
  const graph = n => `graph TD\nA${n}[Entry ${n}] --> B${n}[Exit ${n}]`;
  const fence = source => '```mermaid\n' + source + '\n```';
  for (const [name, sources, images, warning] of [
    ['valid-batch', [graph(1), graph(2), graph(3)], 3, false],
    ['invalid-middle', [graph(1), graph(2), 'graph TD\nBROKEN[Unclosed', graph(4)], 3, true],
    ['all-invalid', ['not_a_diagram\nBAD_SOURCE_1', 'graph TD\nBAD_SOURCE_2['], 0, true],
  ]) {
    const md = path.join(root, name + '.md');
    fs.writeFileSync(md, '# PDF regression ' + name + '\n\n' + sources.map(fence).join('\n\n') + '\n\nEND_OF_DOCUMENT\n');
    const log = run(['generate', 'pdf', '--input', md, '--output-dir', root], name);
    assert.equal(/Warning: Mermaid diagram/.test(log), warning, 'Each failed diagram must report a warning, healthy batches must stay quiet');
    proof[name] = inspect(path.join(root, name + '.pdf'), images, ['END_OF_DOCUMENT', ...(warning ? sources.filter(s => /BROKEN|BAD_SOURCE/.test(s)) : [])]);
  }
  // Fault injection changes only one real Mermaid invocation to never settle.
  // Other diagrams still use the installed Playwright and bundled Mermaid.
  const shim = path.join(root, 'timeout-shim', 'playwright');
  fs.mkdirSync(shim, { recursive: true });
  fs.writeFileSync(path.join(shim, 'index.js'), `
const actual = require(${JSON.stringify(require.resolve('playwright'))});
module.exports = { ...actual, chromium: { ...actual.chromium, launch: async options => {
  const browser = await actual.chromium.launch(options);
  const newContext = browser.newContext.bind(browser);
  browser.newContext = async options => {
    const context = await newContext(options);
    const newPage = context.newPage.bind(context);
    context.newPage = async () => {
      const page = await newPage();
      const addScriptTag = page.addScriptTag.bind(page);
      page.addScriptTag = async options => {
        const result = await addScriptTag(options);
        await page.evaluate(() => {
          const render = mermaid.render.bind(mermaid);
          mermaid.render = (id, source, ...rest) => source.includes('STALL_SOURCE')
            ? new Promise(() => {}) : render(id, source, ...rest);
        });
        return result;
      };
      return page;
    };
    return context;
  };
  return browser;
} } };
`);
  const stalled = path.join(root, 'stalled-middle.md');
  fs.writeFileSync(stalled, '# Stalled diagram\n\n' + [graph(1), graph(2), 'graph TD\nA[STALL_SOURCE] --> B', graph(4)].map(fence).join('\n\n') + '\n\nEND_OF_DOCUMENT\n');
  const started = Date.now();
  const warning = run(['generate', 'pdf', '--input', stalled, '--output-dir', root], 'stalled-middle', {
    NODE_PATH: path.dirname(shim), CODE_EXPLORER_MERMAID_TIMEOUT_MS: '10000',
  });
  assert.match(warning, /Warning: Mermaid diagram 3.*timed out after 10000 ms/);
  proof['stalled-middle'] = { ...inspect(path.join(root, 'stalled-middle.pdf'), 3, ['STALL_SOURCE', 'END_OF_DOCUMENT']), milliseconds: Date.now() - started };
  assert(proof['stalled-middle'].milliseconds < 60000, 'A stuck diagram must not leave a blocked rendering queue');
  assert.equal(fs.readFileSync(env.CODE_EXPLORER_NETWORK_AUDIT, 'utf8'), '', 'PDF rendering attempted an external request');
  fs.writeFileSync(path.join(root, 'proof.json'), JSON.stringify(proof, null, 2));
  console.log('PASS: repository + three valid diagrams + invalid middle + all invalid + timed-out middle; PDF images counted, source preserved, no external requests. Artifacts: ' + root);
} catch (error) {
  console.error('Artifacts: ' + root);
  throw error;
}

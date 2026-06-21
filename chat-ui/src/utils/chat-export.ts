import { mcpClient, type LlmConfigInfo } from '../api/mcp-client';
import type { Message, Session } from '../types/chat';
import { formatExportTimestamp } from './dates';
import { looksLikeMermaid, normalizeCodeFenceLanguage } from './markdown';
import { extractSourceReferences, groupSourceReferences, type SourceReferenceGroup } from './source-references';
import { sourceReferenceSummary } from './related-sources-export';

interface ExportMetadata {
  repo: string | null;
  llm: LlmConfigInfo | null;
}

export function conversationToMarkdown(session: Session, metadata: ExportMetadata): string {
  const messages = exportableMessages(session);
  const sourceGroups = collectSessionSourceGroups(messages);
  const lines: string[] = [
    `# ${session.title || 'Conversation Code Explorer'}`,
    '',
    `- Projet: ${metadata.repo ?? 'non sélectionné'}`,
    `- LLM: ${formatLlmLabel(metadata.llm)}`,
    `- Conversation créée: ${formatExportTimestamp(session.createdAt) || 'inconnue'}`,
    `- Dernière activité: ${formatExportTimestamp(session.updatedAt) || 'inconnue'}`,
    `- Messages exportés: ${messages.length}`,
    `- Export: ${formatExportTimestamp(Date.now())}`,
    '',
  ];

  if (sourceGroups.length > 0) {
    lines.push('### Fichiers sources cités', '');
    for (const group of sourceGroups) {
      lines.push(`- ${sourceReferenceSummary(group)}`);
    }
    lines.push('');
  }

  for (const message of messages) {
    lines.push(`## ${messageLabel(message)}`);
    lines.push('');
    const toolSummary = formatToolCalls(message);
    if (toolSummary) {
      lines.push(`_Outils: ${toolSummary}_`);
      lines.push('');
    }
    lines.push(sanitizeExportText(message.content).trim());
    lines.push('');
  }

  return lines.join('\n').trimEnd() + '\n';
}

export function exportMarkdown(session: Session, metadata: ExportMetadata) {
  downloadTextFile(exportFilename(session, metadata.repo, 'md'), conversationToMarkdown(session, metadata));
}

export function exportPrintableHtml(
  session: Session,
  metadata: ExportMetadata,
  renderedTranscript?: HTMLElement | null
) {
  const transcriptHtml = renderedTranscript
    ? transcriptHtmlFromRenderedElement(renderedTranscript)
    : fallbackTranscriptHtml(session);
  downloadTextFile(
    exportFilename(session, metadata.repo, 'html'),
    printableHtml(session, metadata, transcriptHtml),
    'text/html;charset=utf-8'
  );
}

export type PdfExportMode = 'native' | 'print';

export async function exportPdf(
  session: Session,
  metadata: ExportMetadata,
  renderedTranscript?: HTMLElement | null
): Promise<PdfExportMode> {
  const transcriptHtml = renderedTranscript
    ? transcriptHtmlFromRenderedElement(renderedTranscript)
    : fallbackTranscriptHtml(session);
  const html = printableHtml(session, metadata, transcriptHtml);
  const filename = exportFilename(session, metadata.repo, 'pdf');

  try {
    const pdf = await mcpClient.exportPdf({ filename, html });
    downloadBlobFile(filename, pdf);
    return 'native';
  } catch {
    await openPrintFallback(html);
    return 'print';
  }
}

async function openPrintFallback(html: string) {
  const popup = window.open('', '_blank', 'width=980,height=760');
  if (!popup) {
    throw new Error('Le navigateur a bloqué la fenêtre d’export PDF.');
  }

  popup.document.open();
  popup.document.write(html);
  popup.document.close();
  popup.focus();
  await waitForPrintReady(popup);
  popup.focus();
  popup.print();
}

export function exportFilename(
  session: Session,
  repo: string | null,
  extension: 'md' | 'pdf' | 'html'
): string {
  const base = [repo, session.title || 'conversation']
    .filter(Boolean)
    .join('-')
    .toLowerCase()
    .normalize('NFD')
    .replace(/[\u0300-\u036f]/g, '')
    .replace(/[^a-z0-9]+/g, '-')
    .replace(/^-+|-+$/g, '')
    .slice(0, 90);
  const stamp = new Date().toISOString().replace(/[-:]/g, '').replace(/\..+$/, '').replace('T', '-');
  return `code-explorer-${base || 'conversation'}-${stamp}.${extension}`;
}

function messageLabel(message: Message): string {
  const who =
    message.role === 'user' ? 'Vous' : message.role === 'assistant' ? 'Code Explorer' : 'Système';
  const timestamp = formatExportTimestamp(message.createdAt);
  return timestamp ? `${who} - ${timestamp}` : who;
}

function formatLlmLabel(llm: LlmConfigInfo | null): string {
  if (!llm?.configured) return 'non configuré';
  const provider = llm.provider ?? 'provider inconnu';
  const model = llm.model ?? 'modèle inconnu';
  const effort = llm.reasoningEffort ? `, raisonnement ${llm.reasoningEffort}` : '';
  const maxTokens =
    typeof llm.maxTokens === 'number' && Number.isFinite(llm.maxTokens)
      ? `, max ${llm.maxTokens} tokens`
      : '';
  return `${provider} / ${model}${effort}${maxTokens}`;
}

function formatToolCalls(message: Message): string {
  const calls = message.toolCalls ?? [];
  if (calls.length === 0) return '';
  return calls.map((call) => `${call.name} (${call.status})`).join(', ');
}

function downloadTextFile(
  filename: string,
  content: string,
  mimeType = 'text/markdown;charset=utf-8'
) {
  const blob = new Blob([content], { type: mimeType });
  const url = URL.createObjectURL(blob);
  const a = document.createElement('a');
  a.href = url;
  a.download = filename;
  document.body.appendChild(a);
  a.click();
  a.remove();
  URL.revokeObjectURL(url);
}

function downloadBlobFile(filename: string, blob: Blob) {
  const typedBlob =
    blob.type === 'application/pdf' ? blob : new Blob([blob], { type: 'application/pdf' });
  const url = URL.createObjectURL(typedBlob);
  const a = document.createElement('a');
  a.href = url;
  a.download = filename;
  document.body.appendChild(a);
  a.click();
  a.remove();
  URL.revokeObjectURL(url);
}

function fallbackTranscriptHtml(session: Session): string {
  return exportableMessages(session)
    .map((message) => {
      const toolSummary = formatToolCalls(message);
      return `
        <section class="print-message print-message-${escapeHtml(message.role)}">
          <h2>${escapeHtml(messageLabel(message))}</h2>
          ${toolSummary ? `<p class="print-tools">Outils: ${escapeHtml(toolSummary)}</p>` : ''}
          ${fallbackContentHtml(message.content)}
        </section>`;
    })
    .join('\n');
}

function printableHtml(session: Session, metadata: ExportMetadata, transcriptHtml: string): string {
  const messages = exportableMessages(session);
  const toc = collectPrintToc(messages);
  const sourceGroups = collectSessionSourceGroups(messages);
  return `<!doctype html>
<html lang="fr">
<head>
  <meta charset="utf-8" />
  <title>${escapeHtml(session.title || 'Conversation Code Explorer')}</title>
  <style>
    * {
      box-sizing: border-box;
      -webkit-print-color-adjust: exact;
      print-color-adjust: exact;
    }
    html {
      color-scheme: light;
    }
    body {
      margin: 0;
      background: #fff;
      color: #111827;
      font-family: ui-sans-serif, system-ui, -apple-system, "Segoe UI", sans-serif;
      line-height: 1.5;
    }
    body > header,
    body > main,
    .print-toc {
      margin: 0 auto;
      max-width: 980px;
      padding: 0 28px;
    }
    header {
      border-bottom: 1px solid #d1d5db;
      margin-bottom: 22px;
      padding-bottom: 16px;
      padding-top: 28px;
    }
    h1 {
      font-size: 24px;
      margin: 0 0 8px;
    }
    h2 {
      break-after: avoid;
      font-size: 14px;
      margin: 0 0 8px;
      color: #374151;
    }
    h3, h4 {
      break-after: avoid;
      color: #1f2937;
      margin: 14px 0 6px;
    }
    p {
      margin: 0 0 10px;
    }
    .print-cover {
      background: linear-gradient(135deg, #eef2ff, #f8fafc);
      border: 1px solid #dbe4ff;
      border-radius: 14px;
      margin-top: 28px;
      padding: 20px;
    }
    .print-kicker {
      color: #4f46e5;
      font-size: 11px;
      font-weight: 800;
      letter-spacing: .06em;
      margin-bottom: 8px;
      text-transform: uppercase;
    }
    .meta {
      color: #4b5563;
      font-size: 12px;
    }
    .print-toc {
      break-after: page;
      padding-bottom: 18px;
    }
    .print-toc h2 {
      color: #111827;
      font-size: 16px;
      margin: 18px 0 8px;
    }
    .print-toc ol {
      border: 1px solid #e2e8f0;
      border-radius: 10px;
      list-style: none;
      margin: 0;
      padding: 8px 12px;
    }
    .print-toc li {
      border-bottom: 1px solid #eef2f7;
      color: #334155;
      font-size: 11px;
      padding: 5px 0;
    }
    .print-toc li:last-child {
      border-bottom: 0;
    }
    .print-toc .toc-level-3 {
      padding-left: 14px;
    }
    .print-toc .toc-level-4 {
      padding-left: 28px;
    }
    .print-help {
      margin: 18px auto 0;
      max-width: 980px;
      padding: 10px 28px;
      color: #475569;
      font-size: 12px;
    }
    button, [role="button"], [aria-label*="Copier"], [aria-label*="Régénérer"] {
      display: none !important;
    }
    a {
      color: #1d4ed8;
      text-decoration: none;
    }
    a[href^="http"]::after {
      color: #64748b;
      content: " (" attr(href) ")";
      font-size: 10px;
      overflow-wrap: anywhere;
    }
    svg {
      max-width: 100%;
      height: auto;
    }
    pre {
      background: #f3f4f6;
      border: 1px solid #e5e7eb;
      border-radius: 6px;
      color: #111827;
      line-height: 1.45;
      overflow-wrap: anywhere;
      padding: 10px;
      white-space: pre-wrap;
      word-break: break-word;
    }
    code {
      color: #111827;
      font-family: ui-monospace, SFMono-Regular, Consolas, monospace;
      font-size: 11px;
    }
    blockquote {
      border-left: 3px solid #cbd5e1;
      color: #475569;
      margin: 12px 0;
      padding-left: 12px;
    }
    table {
      border-collapse: collapse;
      font-size: 11px;
      margin: 12px 0;
      width: 100%;
    }
    th, td {
      border: 1px solid #d1d5db;
      padding: 6px 8px;
      text-align: left;
      vertical-align: top;
    }
    th {
      background: #f1f5f9;
      font-weight: 700;
    }
    tr {
      break-inside: avoid;
    }
    details {
      border: 1px solid #e5e7eb;
      border-radius: 6px;
      margin: 12px 0;
      padding: 8px 10px;
    }
    summary {
      font-weight: 700;
    }
    figure {
      margin: 12px 0;
    }
    figcaption {
      color: #475569;
      font-size: 11px;
      font-weight: 700;
      margin-bottom: 6px;
      text-transform: uppercase;
    }
    .chat-transcript > *,
    .print-message {
      margin-bottom: 18px;
    }
    .print-message {
      border-left: 3px solid #e5e7eb;
      padding-left: 12px;
    }
    .print-message-user {
      border-left-color: #8b5cf6;
    }
    .print-message-assistant {
      border-left-color: #10b981;
    }
    .print-message h2,
    .print-message h3,
    .print-message h4 {
      break-after: avoid;
      page-break-after: avoid;
    }
    .print-tools {
      color: #4b5563;
      font-size: 12px;
      font-style: italic;
      margin: 0 0 8px;
    }
    .print-diagram,
    [data-testid="mermaid-block"] {
      break-inside: avoid;
      page-break-inside: avoid;
    }
    .print-code-block {
      break-inside: auto;
      page-break-inside: auto;
    }
    .print-code {
      padding: 0;
    }
    .print-code-line {
      display: grid;
      grid-template-columns: 3.5ch minmax(0, 1fr);
      min-height: 1.35em;
    }
    .print-code-line-number {
      background: #e2e8f0;
      border-right: 1px solid #cbd5e1;
      color: #64748b;
      padding: 0 6px 0 0;
      text-align: right;
      user-select: none;
    }
    .print-code-line-text {
      min-width: 0;
      padding-left: 10px;
      white-space: pre-wrap;
    }
    .print-code-keyword {
      color: #7c3aed;
      font-weight: 700;
    }
    .print-code-type {
      color: #0369a1;
      font-weight: 600;
    }
    .print-code-literal {
      color: #047857;
    }
    .print-diagram,
    [data-testid="mermaid-block"] {
      background: #fff;
      border: 1px solid #cbd5e1;
      border-radius: 8px;
      margin: 14px 0;
      overflow: hidden;
      padding: 10px;
    }
    [data-testid="mermaid-block"] > div:first-child,
    [data-testid="mermaid-block"] [data-testid="mermaid-loading"] {
      display: none !important;
    }
    [data-testid="mermaid-block"] svg {
      background: #fff;
      color: #111827;
      display: block;
      height: auto;
      margin: 0 auto;
      max-width: 100%;
    }
    [data-print-mermaid-source] {
      display: none !important;
    }
    [data-print-mermaid-source][data-print-visible="true"] {
      background: #fff7ed;
      border: 1px solid #fed7aa;
      color: #7c2d12;
      display: block !important;
      margin: 8px 0 0;
    }
    .print-source-ref {
      border: 1px solid #ddd6fe;
      border-radius: 3px;
      color: #5b21b6;
      font-family: ui-monospace, SFMono-Regular, Consolas, monospace;
      padding: 0 3px;
    }
    .print-related-sources {
      break-before: page;
      margin: 24px auto 0;
      max-width: 980px;
      padding: 0 28px 28px;
    }
    .print-related-sources h2 {
      color: #111827;
      font-size: 16px;
      margin-bottom: 10px;
    }
    .print-related-sources ul {
      border: 1px solid #e2e8f0;
      border-radius: 10px;
      list-style: none;
      margin: 0;
      padding: 8px 12px;
    }
    .print-related-sources li {
      border-bottom: 1px solid #eef2f7;
      color: #334155;
      font-family: ui-monospace, SFMono-Regular, Consolas, monospace;
      font-size: 10px;
      overflow-wrap: anywhere;
      padding: 6px 0;
    }
    .print-related-sources li:last-child {
      border-bottom: 0;
    }
    @page {
      margin: 18mm;
    }
    @media print {
      body > header,
      body > main,
      .print-toc,
      .print-related-sources {
        max-width: none;
        padding: 0;
      }
      header {
        padding-top: 0;
      }
      .print-help {
        display: none;
      }
    }
  </style>
</head>
<body>
  <header>
    <div class="print-cover">
      <div class="print-kicker">Code Explorer Chat</div>
      <h1>${escapeHtml(session.title || 'Conversation Code Explorer')}</h1>
      <div class="meta">Projet : ${escapeHtml(metadata.repo ?? 'non sélectionné')}</div>
      <div class="meta">LLM : ${escapeHtml(formatLlmLabel(metadata.llm))}</div>
      <div class="meta">Conversation créée : ${escapeHtml(formatExportTimestamp(session.createdAt) || 'inconnue')}</div>
      <div class="meta">Dernière activité : ${escapeHtml(formatExportTimestamp(session.updatedAt) || 'inconnue')}</div>
      <div class="meta">Messages exportés : ${messages.length}</div>
      <div class="meta">Export : ${escapeHtml(formatExportTimestamp(Date.now()))}</div>
    </div>
  </header>
  ${toc.length > 0 ? printTocHtml(toc) : ''}
  <div class="print-help">Aperçu PDF Code Explorer. Utilise "Enregistrer au format PDF" dans la fenêtre d'impression.</div>
  <main class="chat-transcript">${transcriptHtml}</main>
  ${sourceGroups.length > 0 ? printRelatedSourcesHtml(sourceGroups) : ''}
</body>
</html>`;
}

async function waitForPrintReady(popup: Window) {
  await popupDelay(popup, 350);
  await Promise.race([waitForPrintableAssets(popup.document), popupDelay(popup, 2800)]);
}

function popupDelay(popup: Window, ms: number): Promise<void> {
  return new Promise((resolve) => {
    popup.setTimeout(() => resolve(), ms);
  });
}

async function waitForPrintableAssets(document: Document) {
  const fontSet = document.fonts as FontFaceSet | undefined;
  try {
    await fontSet?.ready;
  } catch {
    // Printing still works with fallback fonts.
  }

  const imageElements = Array.from(document.images ?? []);
  await Promise.all(
    imageElements
      .filter((image) => !image.complete)
      .map(
        (image) =>
          new Promise<void>((resolve) => {
            const done = () => resolve();
            image.addEventListener('load', done, { once: true });
            image.addEventListener('error', done, { once: true });
            void image.decode?.().then(done, done);
          })
      )
  );
}

function exportableMessages(session: Session): Message[] {
  return session.messages.filter((message) => message.content.trim());
}

function transcriptHtmlFromRenderedElement(element: HTMLElement): string {
  const clone = element.cloneNode(true) as HTMLElement;
  sanitizeTextNodes(clone);
  replacePrintableButtons(clone);
  prepareMermaidFallbacks(clone);
  clone.querySelectorAll('[role="dialog"], [data-export-skip]').forEach((node) => node.remove());
  return clone.innerHTML;
}

function sanitizeTextNodes(node: Node) {
  if (node.nodeType === Node.TEXT_NODE) {
    node.nodeValue = sanitizeExportText(node.nodeValue ?? '');
    return;
  }

  node.childNodes.forEach((child) => sanitizeTextNodes(child));
}

function replacePrintableButtons(root: HTMLElement) {
  root.querySelectorAll('button').forEach((button) => {
    const text = sanitizeExportText(button.textContent ?? '').trim();
    if (text && !button.querySelector('svg')) {
      const span = button.ownerDocument.createElement('span');
      span.className = 'print-source-ref';
      span.textContent = text;
      button.replaceWith(span);
      return;
    }
    button.remove();
  });
}

function prepareMermaidFallbacks(root: HTMLElement) {
  root.querySelectorAll<HTMLElement>('[data-testid="mermaid-block"]').forEach((block) => {
    const hasSvg = !!block.querySelector('svg');
    const source = block.querySelector<HTMLElement>('[data-print-mermaid-source]');
    if (!hasSvg && source) {
      source.dataset.printVisible = 'true';
    }
  });
}

function fallbackContentHtml(content: string): string {
  return parseFallbackBlocks(sanitizeExportText(content.trim()))
    .map((block) => {
      if (block.kind === 'code') {
        return fallbackCodeHtml(block.text, block.language);
      }
      return fallbackTextHtml(block.text);
    })
    .join('\n');
}

type FallbackBlock =
  | { kind: 'text'; text: string }
  | { kind: 'code'; text: string; language: string | undefined };

function parseFallbackBlocks(content: string): FallbackBlock[] {
  const blocks: FallbackBlock[] = [];
  const textBuffer: string[] = [];
  const codeBuffer: string[] = [];
  let language: string | undefined;
  let inCode = false;

  const flushText = () => {
    const text = textBuffer.join('\n').trim();
    if (text) blocks.push({ kind: 'text', text });
    textBuffer.length = 0;
  };

  const flushCode = () => {
    blocks.push({ kind: 'code', language, text: codeBuffer.join('\n').replace(/\n$/, '') });
    codeBuffer.length = 0;
    language = undefined;
    inCode = false;
  };

  for (const line of content.split('\n')) {
    const opening = /^```([^\s`]*)?.*$/.exec(line.trim());
    if (opening && !inCode) {
      flushText();
      language = normalizeCodeFenceLanguage(opening[1]);
      inCode = true;
      continue;
    }

    if (line.trim() === '```' && inCode) {
      flushCode();
      continue;
    }

    if (inCode) {
      codeBuffer.push(line);
    } else {
      textBuffer.push(line);
    }
  }

  if (inCode) {
    flushCode();
  }
  flushText();
  return blocks;
}

function fallbackTextHtml(text: string): string {
  return text
    .split(/\n{2,}/)
    .map((chunk) => {
      const trimmed = chunk.trim();
      const heading = /^(#{1,4})\s+(.+)$/.exec(trimmed);
      if (heading) {
        const level = Math.min(4, heading[1].length + 2);
        return `<h${level}>${escapeHtml(heading[2])}</h${level}>`;
      }
      return `<p>${trimmed.split('\n').map(escapeHtml).join('<br />')}</p>`;
    })
    .join('\n');
}

function fallbackCodeHtml(code: string, language: string | undefined): string {
  const isMermaid = language === 'mermaid' || looksLikeMermaid(code);
  if (isMermaid) {
    return `<figure class="print-diagram print-diagram-mermaid">
    <figcaption>Diagramme Mermaid (source)</figcaption>
    <pre><code>${escapeHtml(code)}</code></pre>
  </figure>`;
  }

  const chunks = chunkCodeForPrint(code);
  const baseLabel = `Code${language ? ` ${language}` : ''}`;
  return chunks
    .map((chunk, index) => {
      const label =
        chunks.length === 1
          ? baseLabel
          : `${baseLabel} · lignes ${index * PRINT_CODE_CHUNK_SIZE + 1}-${index * PRINT_CODE_CHUNK_SIZE + chunk.split('\n').length}`;
      return `<figure class="print-code-block">
    <figcaption>${escapeHtml(label)}</figcaption>
    <pre class="print-code"><code>${printableCodeLinesHtml(chunk, language, index * PRINT_CODE_CHUNK_SIZE + 1)}</code></pre>
  </figure>`;
    })
    .join('\n');
}

const PRINT_CODE_CHUNK_SIZE = 42;
const PRINT_CODE_KEYWORDS = new Set([
  'abstract',
  'async',
  'await',
  'break',
  'case',
  'catch',
  'class',
  'const',
  'continue',
  'default',
  'delegate',
  'do',
  'else',
  'enum',
  'export',
  'extends',
  'finally',
  'for',
  'foreach',
  'from',
  'function',
  'if',
  'implements',
  'import',
  'in',
  'interface',
  'let',
  'match',
  'module',
  'namespace',
  'new',
  'private',
  'protected',
  'public',
  'readonly',
  'return',
  'sealed',
  'static',
  'struct',
  'switch',
  'throw',
  'try',
  'using',
  'var',
  'while',
]);
const PRINT_CODE_TYPES = new Set([
  'bool',
  'boolean',
  'char',
  'decimal',
  'double',
  'float',
  'int',
  'long',
  'number',
  'object',
  'record',
  'short',
  'string',
  'String',
  'void',
]);
const PRINT_CODE_LITERALS = new Set(['false', 'null', 'None', 'self', 'this', 'true']);

function chunkCodeForPrint(code: string): string[] {
  const lines = code.split('\n');
  const chunks: string[] = [];
  for (let index = 0; index < lines.length; index += PRINT_CODE_CHUNK_SIZE) {
    chunks.push(lines.slice(index, index + PRINT_CODE_CHUNK_SIZE).join('\n'));
  }
  return chunks.length > 0 ? chunks : [''];
}

function printableCodeLinesHtml(code: string, language: string | undefined, firstLine: number): string {
  return code
    .split('\n')
    .map((line, index) => {
      const lineNumber = firstLine + index;
      return `<span class="print-code-line"><span class="print-code-line-number">${lineNumber}</span><span class="print-code-line-text">${highlightPrintableCodeLine(line, language)}</span></span>`;
    })
    .join('');
}

function highlightPrintableCodeLine(line: string, language: string | undefined): string {
  const escaped = escapeHtml(line);
  if (!language || language === 'text' || language === 'plain') return escaped;
  return escaped.replace(/\b[A-Za-z_][A-Za-z0-9_]*\b/g, (token) => {
    if (PRINT_CODE_KEYWORDS.has(token)) return `<span class="print-code-keyword">${token}</span>`;
    if (PRINT_CODE_TYPES.has(token)) return `<span class="print-code-type">${token}</span>`;
    if (PRINT_CODE_LITERALS.has(token)) return `<span class="print-code-literal">${token}</span>`;
    return token;
  });
}

interface PrintTocEntry {
  level: number;
  title: string;
}

function collectPrintToc(messages: Message[]): PrintTocEntry[] {
  const entries: PrintTocEntry[] = [];
  for (const message of messages) {
    if (message.role !== 'assistant') continue;
    for (const line of message.content.split('\n')) {
      const match = /^(#{2,4})\s+(.+)$/.exec(line.trim());
      if (!match) continue;
      const title = stripInlineMarkdown(match[2]).replace(/\s+\{#[^}]+}\s*$/, '').trim();
      if (title) entries.push({ level: match[1].length, title });
      if (entries.length >= 40) return entries;
    }
  }
  return entries;
}

function printTocHtml(entries: PrintTocEntry[]): string {
  return `<section class="print-toc" aria-label="Sommaire">
    <h2>Sommaire</h2>
    <ol>
      ${entries
        .map(
          (entry) =>
            `<li class="toc-level-${entry.level}">${escapeHtml(entry.title)}</li>`
        )
        .join('\n')}
    </ol>
  </section>`;
}

function collectSessionSourceGroups(messages: Message[]): SourceReferenceGroup[] {
  return groupSourceReferences(
    messages.flatMap((message) =>
      message.role === 'assistant' ? extractSourceReferences(message.content) : []
    )
  );
}

function printRelatedSourcesHtml(groups: SourceReferenceGroup[]): string {
  return `<section class="print-related-sources" aria-label="Fichiers sources cités">
    <h2>Fichiers sources cités</h2>
    <ul>
      ${groups.map((group) => `<li>${escapeHtml(sourceReferenceSummary(group))}</li>`).join('\n')}
    </ul>
  </section>`;
}

function stripInlineMarkdown(value: string): string {
  return value
    .replace(/`([^`]+)`/g, '$1')
    .replace(/\*\*([^*]+)\*\*/g, '$1')
    .replace(/\*([^*]+)\*/g, '$1')
    .replace(/\[([^\]]+)\]\([^)]+\)/g, '$1');
}

function sanitizeExportText(value: string): string {
  return value
    .replace(/\r\n?/g, '\n')
    .replace(/\u200B/g, '')
    .replace(/\u200C/g, '')
    .replace(/\u200D/g, '')
    .replace(/\uFE0F/g, '');
}

function escapeHtml(value: string): string {
  return sanitizeExportText(value)
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
    .replace(/'/g, '&#39;');
}

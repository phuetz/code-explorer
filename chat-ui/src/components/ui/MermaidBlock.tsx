import { useEffect, useId, useState } from 'react';
import DOMPurify from 'dompurify';
import { Check, Code2, Copy, Download, Loader2, Maximize2, X } from 'lucide-react';
import { copyTextToClipboard } from '../../utils/clipboard';
import { repairMermaidSource } from '../../utils/mermaid';

/**
 * Renders a Mermaid diagram from raw text.
 *
 * Why a lazy dynamic import:
 *   `mermaid` ships ~500 KB minified. Most chat replies are plain prose, so
 *   loading the lib up-front would inflate every page load for no reason.
 *   We import it on first render and the chunk gets cached for the rest of
 *   the session.
 *
 * Why an `useId`-derived render target:
 *   `mermaid.render` accepts a target id and writes the SVG output as a
 *   string. The id has to start with a letter, hence the `m-` prefix; it
 *   has to be stable across re-renders so React's commit phase doesn't lose
 *   the reference to the element we just decorated.
 *
 * Defense in depth on the SVG output:
 *   - Mermaid is configured with `securityLevel: 'strict'`, which sanitizes
 *     anything the user smuggled via diagram labels (Mermaid's own
 *     DOMPurify pass).
 *   - We additionally run DOMPurify on the rendered SVG before injecting
 *     it. Belt and braces — if a future Mermaid version regresses on
 *     sanitization, our pass still strips `<script>` / `on*` handlers.
 *
 * Failure modes are explicit: a malformed graph keeps the original ```mermaid
 * source visible inside a `<pre>` so the user can copy-paste it elsewhere
 * (mermaid live editor, etc.) instead of staring at a blank box.
 */
interface Props {
  text: string;
}

type MermaidTheme = 'light' | 'dark';

const MERMAID_RENDER_TIMEOUT_MS = 6500;
let mermaidRenderQueue: Promise<void> = Promise.resolve();

export function MermaidBlock({ text }: Props) {
  const id = useId();
  const svgId = `m-${id.replace(/:/g, '')}`;
  const theme = useDocumentTheme();
  const [renderState, setRenderState] = useState<{
    text: string;
    theme: MermaidTheme;
    mode: 'mermaid' | 'fallback';
    svg: string | null;
    error: string | null;
  }>({ text: '', theme: 'light', mode: 'mermaid', svg: null, error: null });
  const [showSource, setShowSource] = useState(false);
  const [copied, setCopied] = useState(false);
  const [expanded, setExpanded] = useState(false);
  const isCurrentRender = renderState.text === text && renderState.theme === theme;
  const svg = isCurrentRender ? renderState.svg : null;
  const error = isCurrentRender ? renderState.error : null;

  useEffect(() => {
    let cancelled = false;

    void (async () => {
      try {
        const renderText = repairMermaidSource(text);
        const fallback = buildFlowchartFallbackSvg(renderText, svgId, theme);
        if (fallback && shouldPreferFlowchartFallback(fallback.stats)) {
          if (cancelled) return;
          const purified = DOMPurify.sanitize(fallback.svg, {
            USE_PROFILES: { svg: true, svgFilters: true },
            ADD_TAGS: ['foreignObject'],
          });
          setRenderState({ text, theme, mode: 'fallback', svg: purified, error: null });
          return;
        }

        const { default: mermaid } = await import('mermaid');
        const { svg: rendered } = await enqueueMermaidRender(async () => {
          mermaid.initialize({
            startOnLoad: false,
            theme: 'base',
            themeVariables: mermaidThemeVariables(theme),
            fontFamily: 'ui-sans-serif, system-ui, -apple-system, sans-serif',
            securityLevel: 'strict',
            flowchart: {
              curve: 'basis',
              padding: 18,
              useMaxWidth: false,
            },
          });
          return renderWithTimeout(
            mermaid.render(svgId, renderText.trim()),
            MERMAID_RENDER_TIMEOUT_MS
          );
        });
        if (cancelled) return;
        const purified = DOMPurify.sanitize(rendered, {
          USE_PROFILES: { svg: true, svgFilters: true },
          ADD_TAGS: ['foreignObject'],
        });
        setRenderState({ text, theme, mode: 'mermaid', svg: purified, error: null });
      } catch (e) {
        if (cancelled) return;
        setRenderState({
          text,
          theme,
          mode: 'mermaid',
          svg: null,
          error: e instanceof Error ? e.message : String(e),
        });
      }
    })();

    return () => {
      cancelled = true;
    };
  }, [text, svgId, theme]);

  const copySource = async () => {
    const ok = await copyTextToClipboard(text);
    if (ok) {
      setCopied(true);
      setTimeout(() => setCopied(false), 1500);
    }
  };

  const downloadSvg = () => {
    if (!svg) return;
    const blob = new Blob([ensureSvgNamespace(svg)], {
      type: 'image/svg+xml;charset=utf-8',
    });
    const url = URL.createObjectURL(blob);
    const link = document.createElement('a');
    link.href = url;
    link.download = 'gitnexus-diagram.svg';
    document.body.appendChild(link);
    link.click();
    link.remove();
    URL.revokeObjectURL(url);
  };

  useEffect(() => {
    if (!expanded) return undefined;
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Escape') setExpanded(false);
    };
    document.addEventListener('keydown', onKeyDown);
    return () => document.removeEventListener('keydown', onKeyDown);
  }, [expanded]);

  return (
    <div
      className="mermaid-card my-4 overflow-hidden rounded-lg border"
      data-testid="mermaid-block"
    >
      <div className="mermaid-toolbar flex items-center justify-between gap-3 border-b px-3 py-2 text-xs">
        <div className="flex min-w-0 items-center gap-2 text-neutral-400">
          {svg ? (
            <span className="h-2 w-2 rounded-full bg-emerald-400" aria-hidden="true" />
          ) : error ? (
            <span className="h-2 w-2 rounded-full bg-red-400" aria-hidden="true" />
          ) : (
            <Loader2 className="h-3.5 w-3.5 animate-spin text-amber-300" aria-hidden="true" />
          )}
          <span className="truncate font-medium text-neutral-300">
            {svg && renderState.mode === 'fallback' ? 'Mermaid simplifié' : 'Mermaid'}
          </span>
        </div>
        <div className="flex items-center gap-1">
          <button
            type="button"
            onClick={() => setExpanded(true)}
            disabled={!svg}
            className="icon-button rounded p-1.5 disabled:cursor-not-allowed disabled:opacity-40"
            aria-label="Agrandir le diagramme Mermaid"
            title="Agrandir"
          >
            <Maximize2 className="h-3.5 w-3.5" aria-hidden="true" />
          </button>
          <button
            type="button"
            onClick={downloadSvg}
            disabled={!svg}
            className="icon-button rounded p-1.5 disabled:cursor-not-allowed disabled:opacity-40"
            aria-label="Télécharger le diagramme Mermaid en SVG"
            title="Télécharger SVG"
          >
            <Download className="h-3.5 w-3.5" aria-hidden="true" />
          </button>
          <button
            type="button"
            onClick={() => setShowSource((value) => !value)}
            className="icon-button rounded p-1.5"
            aria-label={showSource ? 'Masquer la source Mermaid' : 'Afficher la source Mermaid'}
            aria-pressed={showSource}
            title={showSource ? 'Masquer la source' : 'Afficher la source'}
          >
            <Code2 className="h-3.5 w-3.5" aria-hidden="true" />
          </button>
          <button
            type="button"
            onClick={() => void copySource()}
            className="icon-button rounded p-1.5"
            aria-label="Copier la source Mermaid"
            title={copied ? 'Copié !' : 'Copier la source'}
          >
            {copied ? (
              <Check className="h-3.5 w-3.5" aria-hidden="true" />
            ) : (
              <Copy className="h-3.5 w-3.5" aria-hidden="true" />
            )}
          </button>
        </div>
      </div>

      {error ? (
        <div className="p-3 text-xs">
          <p className="mb-2 font-medium text-red-300">Rendu Mermaid impossible</p>
          <p className="mb-2 text-red-400/80">{error}</p>
          <SourceBlock text={text} />
        </div>
      ) : (
        <div className="mermaid-viewport flex min-h-44 justify-start overflow-auto p-4 sm:p-5">
          {svg ? (
            <div
              className="mermaid-canvas w-full min-w-[980px] text-neutral-100 [&_svg]:h-auto [&_svg]:w-full [&_svg]:max-w-none [&_svg]:min-w-[980px]"
              dangerouslySetInnerHTML={{ __html: svg }}
            />
          ) : (
            <div
              className="flex w-full items-center justify-center rounded-md border border-dashed border-neutral-800 bg-neutral-900/30 py-8 text-xs text-neutral-500"
              data-testid="mermaid-loading"
            >
              Rendu du diagramme...
            </div>
          )}
        </div>
      )}

      {showSource && !error && (
        <div className="border-t border-[var(--border)] p-3">
          <SourceBlock text={text} />
        </div>
      )}

      <pre data-print-mermaid-source style={{ display: 'none' }}>
        <code>{text}</code>
      </pre>

      {expanded && svg && (
        <div
          className="fixed inset-0 z-50 flex items-center justify-center bg-black/80 p-6"
          role="dialog"
          aria-modal="true"
          aria-label="Diagramme Mermaid agrandi"
          onClick={() => setExpanded(false)}
        >
          <button
            type="button"
            className="absolute right-5 top-5 rounded-md border border-neutral-700 bg-neutral-950 p-2 text-neutral-300 hover:bg-neutral-900 hover:text-white"
            onClick={() => setExpanded(false)}
            aria-label="Fermer le diagramme agrandi"
            title="Fermer"
          >
            <X className="h-4 w-4" aria-hidden="true" />
          </button>
          <div
            className="mermaid-card max-h-[88vh] max-w-[94vw] overflow-auto rounded-lg border p-6 text-neutral-100 shadow-2xl [&_svg]:h-auto [&_svg]:w-full [&_svg]:max-w-none [&_svg]:min-w-[1180px]"
            onClick={(event) => event.stopPropagation()}
            dangerouslySetInnerHTML={{ __html: svg }}
          />
        </div>
      )}
    </div>
  );
}

function ensureSvgNamespace(svg: string): string {
  const trimmed = svg.trimStart();
  const namespaced = /<svg\b[^>]*\sxmlns=/.test(trimmed)
    ? trimmed
    : trimmed.replace('<svg', '<svg xmlns="http://www.w3.org/2000/svg"');
  return `<?xml version="1.0" encoding="UTF-8"?>\n${namespaced}`;
}

function SourceBlock({ text }: { text: string }) {
  return (
    <pre className="source-code-view max-h-80 overflow-auto rounded p-2 text-xs">
      <code>{text}</code>
    </pre>
  );
}

function useDocumentTheme(): MermaidTheme {
  const [theme, setTheme] = useState<MermaidTheme>(() => readDocumentTheme());

  useEffect(() => {
    const root = document.documentElement;
    const syncTheme = () => setTheme(readDocumentTheme());
    const observer = new MutationObserver(syncTheme);
    observer.observe(root, { attributes: true, attributeFilter: ['data-theme'] });
    syncTheme();
    return () => observer.disconnect();
  }, []);

  return theme;
}

function readDocumentTheme(): MermaidTheme {
  if (typeof document === 'undefined') return 'light';
  return document.documentElement.dataset.theme === 'dark' ? 'dark' : 'light';
}

function mermaidThemeVariables(theme: MermaidTheme) {
  if (theme === 'dark') {
    return {
      background: '#0f172a',
      mainBkg: '#172033',
      secondBkg: '#102a3a',
      tertiaryColor: '#1f2937',
      primaryColor: '#172033',
      primaryBorderColor: '#8b5cf6',
      primaryTextColor: '#f8fafc',
      secondaryColor: '#102a3a',
      secondaryBorderColor: '#22d3ee',
      secondaryTextColor: '#f8fafc',
      tertiaryBorderColor: '#34d399',
      tertiaryTextColor: '#e2e8f0',
      nodeBorder: '#818cf8',
      clusterBkg: '#111827',
      clusterBorder: '#334155',
      lineColor: '#94a3b8',
      textColor: '#f8fafc',
      edgeLabelBackground: '#111827',
      labelTextColor: '#f8fafc',
      actorBkg: '#172033',
      actorBorder: '#8b5cf6',
      actorTextColor: '#f8fafc',
      signalColor: '#a5b4fc',
      signalTextColor: '#f8fafc',
      noteBkgColor: '#422006',
      noteTextColor: '#fef3c7',
      noteBorderColor: '#f59e0b',
    };
  }

  return {
    background: '#ffffff',
    mainBkg: '#eef2ff',
    secondBkg: '#e0f2fe',
    tertiaryColor: '#ecfdf5',
    primaryColor: '#eef2ff',
    primaryBorderColor: '#6366f1',
    primaryTextColor: '#0f172a',
    secondaryColor: '#e0f2fe',
    secondaryBorderColor: '#0284c7',
    secondaryTextColor: '#0f172a',
    tertiaryBorderColor: '#059669',
    tertiaryTextColor: '#0f172a',
    nodeBorder: '#6366f1',
    clusterBkg: '#f8fafc',
    clusterBorder: '#cbd5e1',
    lineColor: '#64748b',
    textColor: '#0f172a',
    edgeLabelBackground: '#ffffff',
    labelTextColor: '#0f172a',
    actorBkg: '#eef2ff',
    actorBorder: '#6366f1',
    actorTextColor: '#0f172a',
    signalColor: '#2563eb',
    signalTextColor: '#0f172a',
    noteBkgColor: '#fef3c7',
    noteTextColor: '#78350f',
    noteBorderColor: '#f59e0b',
  };
}

function enqueueMermaidRender<T>(render: () => Promise<T>): Promise<T> {
  const queued = mermaidRenderQueue.then(render, render);
  mermaidRenderQueue = queued.then(
    () => undefined,
    () => undefined
  );
  return queued;
}

function renderWithTimeout<T>(promise: Promise<T>, timeoutMs: number): Promise<T> {
  return new Promise<T>((resolve, reject) => {
    const timeout = window.setTimeout(() => {
      reject(new Error('Le rendu Mermaid a pris trop de temps. La source reste disponible.'));
    }, timeoutMs);

    promise.then(
      (value) => {
        window.clearTimeout(timeout);
        resolve(value);
      },
      (error) => {
        window.clearTimeout(timeout);
        reject(error);
      }
    );
  });
}

interface FlowchartFallback {
  svg: string;
  stats: {
    edgeCount: number;
    lineCount: number;
    nodeCount: number;
    sourceLength: number;
  };
}

interface FlowNode {
  id: string;
  label: string;
}

interface FlowEdge {
  source: string;
  target: string;
}

const FLOWCHART_HEADER_RE = /^\s*(?:flowchart|graph)\s+(TD|TB|BT|LR|RL)\b/im;
const EDGE_SPLIT_RE = /\s*(?:-->|---|-.->|==>|--x|--o|-x|-o|--[^-\n]+-->)\s*/;
const NODE_ID_RE = /^\s*\|[^|]*\|\s*([A-Za-z0-9_.$:-]+)|^\s*([A-Za-z0-9_.$:-]+)/;
const NODE_WITH_LABEL_RE =
  /([A-Za-z0-9_.$:-]+)\s*(\[\[[\s\S]*?\]\]|\[[\s\S]*?\]|\(\([\s\S]*?\)\)|\([\s\S]*?\)|\{\{[\s\S]*?\}\}|\{[\s\S]*?\})/g;

function shouldPreferFlowchartFallback(stats: FlowchartFallback['stats']): boolean {
  return (
    stats.edgeCount > 12 ||
    stats.nodeCount > 14 ||
    stats.lineCount > 18 ||
    stats.sourceLength > 1100
  );
}

function buildFlowchartFallbackSvg(
  source: string,
  svgId: string,
  theme: MermaidTheme
): FlowchartFallback | null {
  const header = FLOWCHART_HEADER_RE.exec(source);
  if (!header) return null;

  const nodes = new Map<string, FlowNode>();
  const edges: FlowEdge[] = [];
  const lines = source
    .replace(/\r\n?/g, '\n')
    .split('\n')
    .map((line) => line.trim())
    .filter((line) => line && !line.startsWith('%%'));

  for (const line of lines.slice(1)) {
    registerNodeLabels(line, nodes);
    const parts = line.split(EDGE_SPLIT_RE).filter(Boolean);
    if (parts.length < 2) continue;

    for (let index = 0; index < parts.length - 1; index += 1) {
      const sourceId = extractNodeId(parts[index]);
      const targetId = extractNodeId(parts[index + 1]);
      if (!sourceId || !targetId || sourceId === targetId) continue;
      ensureNode(nodes, sourceId);
      ensureNode(nodes, targetId);
      edges.push({ source: sourceId, target: targetId });
    }
  }

  if (nodes.size < 2 || edges.length < 1) return null;

  const limitedNodes = Array.from(nodes.values()).slice(0, 80);
  const allowed = new Set(limitedNodes.map((node) => node.id));
  const limitedEdges = edges
    .filter((edge) => allowed.has(edge.source) && allowed.has(edge.target))
    .slice(0, 130);

  return {
    svg: renderFallbackFlowchart(limitedNodes, limitedEdges, svgId, theme),
    stats: {
      edgeCount: edges.length,
      lineCount: lines.length,
      nodeCount: nodes.size,
      sourceLength: source.length,
    },
  };
}

function registerNodeLabels(line: string, nodes: Map<string, FlowNode>) {
  for (const match of line.matchAll(NODE_WITH_LABEL_RE)) {
    const [, id, rawLabel] = match;
    ensureNode(nodes, id, cleanFlowchartLabel(rawLabel));
  }
}

function extractNodeId(value: string): string | null {
  const match = NODE_ID_RE.exec(value);
  const id = match?.[1] ?? match?.[2] ?? null;
  return id?.replace(/:::.*/, '') ?? null;
}

function ensureNode(nodes: Map<string, FlowNode>, id: string, label?: string) {
  const current = nodes.get(id);
  if (current) {
    if (label && current.label === id) current.label = label;
    return;
  }
  nodes.set(id, { id, label: label || id });
}

function cleanFlowchartLabel(raw: string): string {
  let label = raw.trim();
  const pairs: Array<[string, string]> = [
    ['[[', ']]'],
    ['{{', '}}'],
    ['((', '))'],
    ['[', ']'],
    ['{', '}'],
    ['(', ')'],
  ];

  for (const [open, close] of pairs) {
    if (label.startsWith(open) && label.endsWith(close)) {
      label = label.slice(open.length, -close.length).trim();
      break;
    }
  }

  if (
    (label.startsWith('"') && label.endsWith('"')) ||
    (label.startsWith("'") && label.endsWith("'"))
  ) {
    label = label.slice(1, -1);
  }

  return label
    .replace(/<br\s*\/?>/gi, '\n')
    .replace(/\\"/g, '"')
    .replace(/\\\\/g, '\\');
}

function renderFallbackFlowchart(
  nodes: FlowNode[],
  edges: FlowEdge[],
  svgId: string,
  theme: MermaidTheme
): string {
  const palette = fallbackPalette(theme);
  const positions = layoutFallbackFlowchart(nodes, edges);
  const markerId = `arrow-${svgId.replace(/[^A-Za-z0-9_-]/g, '')}`;
  const width = positions.width;
  const height = positions.height;
  const edgeSet = edges
    .map((edge) => ({
      edge,
      source: positions.nodes.get(edge.source),
      target: positions.nodes.get(edge.target),
    }))
    .filter(
      (item): item is { edge: FlowEdge; source: PositionedFlowNode; target: PositionedFlowNode } =>
        Boolean(item.source && item.target)
    );

  return [
    `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 ${width} ${height}" role="img" aria-label="Diagramme Mermaid simplifie">`,
    '<defs>',
    `<marker id="${markerId}" markerWidth="10" markerHeight="10" refX="8" refY="3" orient="auto" markerUnits="strokeWidth">`,
    `<path d="M0,0 L0,6 L8,3 z" fill="${palette.edge}" />`,
    '</marker>',
    '</defs>',
    `<rect x="0" y="0" width="${width}" height="${height}" rx="18" fill="${palette.background}" />`,
    ...edgeSet.map(({ source, target }) => renderFallbackEdge(source, target, palette.edge, markerId)),
    ...nodes.map((node, index) => renderFallbackNode(node, positions.nodes.get(node.id), index, palette)),
    '</svg>',
  ].join('');
}

interface PositionedFlowNode {
  x: number;
  y: number;
}

function layoutFallbackFlowchart(nodes: FlowNode[], edges: FlowEdge[]) {
  const adjacency = new Map<string, string[]>();
  const indegree = new Map(nodes.map((node) => [node.id, 0]));
  for (const edge of edges) {
    adjacency.set(edge.source, [...(adjacency.get(edge.source) ?? []), edge.target]);
    indegree.set(edge.target, (indegree.get(edge.target) ?? 0) + 1);
  }

  const levels = new Map(nodes.map((node) => [node.id, 0]));
  const queue = nodes.filter((node) => (indegree.get(node.id) ?? 0) === 0).map((node) => node.id);
  if (queue.length === 0 && nodes[0]) queue.push(nodes[0].id);
  const visited = new Set<string>();

  while (queue.length > 0) {
    const current = queue.shift();
    if (!current || visited.has(current)) continue;
    visited.add(current);
    const currentLevel = levels.get(current) ?? 0;
    for (const target of adjacency.get(current) ?? []) {
      levels.set(target, Math.max(levels.get(target) ?? 0, currentLevel + 1));
      const nextIndegree = Math.max(0, (indegree.get(target) ?? 0) - 1);
      indegree.set(target, nextIndegree);
      if (nextIndegree === 0) queue.push(target);
    }
  }

  nodes.forEach((node, index) => {
    if (!visited.has(node.id)) levels.set(node.id, Math.max(0, Math.floor(index / 6)));
  });

  const grouped = new Map<number, FlowNode[]>();
  for (const node of nodes) {
    const level = Math.min(12, levels.get(node.id) ?? 0);
    grouped.set(level, [...(grouped.get(level) ?? []), node]);
  }

  const orderedLevels = Array.from(grouped.keys()).sort((a, b) => a - b);
  const maxInLevel = Math.max(...Array.from(grouped.values()).map((items) => items.length));
  const width = Math.max(980, maxInLevel * 260 + 120);
  const height = Math.max(260, orderedLevels.length * 116 + 80);
  const positioned = new Map<string, PositionedFlowNode>();

  orderedLevels.forEach((level, levelIndex) => {
    const items = grouped.get(level) ?? [];
    const rowWidth = (items.length - 1) * 250;
    const startX = width / 2 - rowWidth / 2;
    items.forEach((node, index) => {
      positioned.set(node.id, {
        x: startX + index * 250,
        y: 52 + levelIndex * 116,
      });
    });
  });

  return { height, nodes: positioned, width };
}

function renderFallbackEdge(
  source: PositionedFlowNode,
  target: PositionedFlowNode,
  color: string,
  markerId: string
): string {
  const startX = source.x;
  const startY = source.y + 28;
  const endX = target.x;
  const endY = target.y - 30;
  const midY = startY + Math.max(24, (endY - startY) / 2);
  const path = `M ${startX} ${startY} C ${startX} ${midY}, ${endX} ${midY}, ${endX} ${endY}`;
  return `<path d="${path}" fill="none" stroke="${color}" stroke-width="1.6" stroke-linecap="round" marker-end="url(#${markerId})" opacity="0.7" />`;
}

function renderFallbackNode(
  node: FlowNode,
  position: PositionedFlowNode | undefined,
  index: number,
  palette: ReturnType<typeof fallbackPalette>
): string {
  if (!position) return '';
  const colors = palette.nodes[index % palette.nodes.length];
  const labelLines = wrapLabel(node.label, 26).slice(0, 3);
  const textY = -(labelLines.length - 1) * 8;

  return [
    `<g transform="translate(${position.x} ${position.y})">`,
    `<title>${escapeXml(node.label.replace(/\n+/g, ' / '))}</title>`,
    `<rect x="-105" y="-30" width="210" height="60" rx="10" fill="${colors.fill}" stroke="${colors.stroke}" stroke-width="1.5" />`,
    ...labelLines.map(
      (line, lineIndex) =>
        `<text x="0" y="${textY + lineIndex * 16}" text-anchor="middle" dominant-baseline="middle" font-family="ui-sans-serif, system-ui, sans-serif" font-size="12" font-weight="600" fill="${palette.text}">${escapeXml(line)}</text>`
    ),
    '</g>',
  ].join('');
}

function wrapLabel(label: string, maxLength: number): string[] {
  const normalizedLines = label
    .split(/\n+/)
    .map((line) => line.replace(/\s+/g, ' ').trim())
    .filter(Boolean);
  if (normalizedLines.length === 0) return [' '];

  return normalizedLines.flatMap((line) => wrapLabelLine(line, maxLength));
}

function wrapLabelLine(line: string, maxLength: number): string[] {
  if (!line) return [];
  const words = line.split(' ');
  const lines: string[] = [];
  let current = '';

  for (const word of words) {
    if (!current) {
      current = word;
    } else if (`${current} ${word}`.length <= maxLength) {
      current = `${current} ${word}`;
    } else {
      lines.push(current);
      current = word;
    }
  }
  if (current) lines.push(current);

  return lines.flatMap((line) =>
    line.length <= maxLength ? [line] : line.match(new RegExp(`.{1,${maxLength}}`, 'g')) ?? [line]
  );
}

function fallbackPalette(theme: MermaidTheme) {
  if (theme === 'dark') {
    return {
      background: '#101827',
      edge: '#93a4bd',
      text: '#f8fafc',
      nodes: [
        { fill: '#1e293b', stroke: '#818cf8' },
        { fill: '#143246', stroke: '#22d3ee' },
        { fill: '#12322b', stroke: '#34d399' },
        { fill: '#3a2a14', stroke: '#fbbf24' },
      ],
    };
  }

  return {
    background: '#fbfdff',
    edge: '#64748b',
    text: '#0f172a',
    nodes: [
      { fill: '#eef2ff', stroke: '#6366f1' },
      { fill: '#e0f2fe', stroke: '#0284c7' },
      { fill: '#ecfdf5', stroke: '#059669' },
      { fill: '#fff7ed', stroke: '#f59e0b' },
    ],
  };
}

function escapeXml(value: string): string {
  return value
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;');
}

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

export function MermaidBlock({ text }: Props) {
  const id = useId();
  const svgId = `m-${id.replace(/:/g, '')}`;
  const [renderState, setRenderState] = useState<{
    text: string;
    svg: string | null;
    error: string | null;
  }>({ text: '', svg: null, error: null });
  const [showSource, setShowSource] = useState(false);
  const [copied, setCopied] = useState(false);
  const [expanded, setExpanded] = useState(false);
  const isCurrentRender = renderState.text === text;
  const svg = isCurrentRender ? renderState.svg : null;
  const error = isCurrentRender ? renderState.error : null;

  useEffect(() => {
    let cancelled = false;

    void (async () => {
      try {
        const { default: mermaid } = await import('mermaid');
        mermaid.initialize({
          startOnLoad: false,
          theme: 'dark',
          fontFamily: 'ui-sans-serif, system-ui, -apple-system, sans-serif',
          securityLevel: 'strict',
        });
        const renderText = repairMermaidSource(text);
        const { svg: rendered } = await mermaid.render(svgId, renderText.trim());
        if (cancelled) return;
        const purified = DOMPurify.sanitize(rendered, {
          USE_PROFILES: { svg: true, svgFilters: true },
          ADD_TAGS: ['foreignObject'],
        });
        setRenderState({ text, svg: purified, error: null });
      } catch (e) {
        if (cancelled) return;
        setRenderState({
          text,
          svg: null,
          error: e instanceof Error ? e.message : String(e),
        });
      }
    })();

    return () => {
      cancelled = true;
    };
  }, [text, svgId]);

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
      className="my-3 overflow-hidden rounded-md border border-neutral-800 bg-neutral-950/70"
      data-testid="mermaid-block"
    >
      <div className="flex items-center justify-between gap-3 border-b border-neutral-800 bg-neutral-900/55 px-3 py-2 text-xs">
        <div className="flex min-w-0 items-center gap-2 text-neutral-400">
          {svg ? (
            <span className="h-2 w-2 rounded-full bg-emerald-400" aria-hidden="true" />
          ) : error ? (
            <span className="h-2 w-2 rounded-full bg-red-400" aria-hidden="true" />
          ) : (
            <Loader2 className="h-3.5 w-3.5 animate-spin text-amber-300" aria-hidden="true" />
          )}
          <span className="truncate font-medium text-neutral-300">Mermaid</span>
        </div>
        <div className="flex items-center gap-1">
          <button
            type="button"
            onClick={() => setExpanded(true)}
            disabled={!svg}
            className="rounded p-1.5 text-neutral-500 hover:bg-neutral-800 hover:text-neutral-100 disabled:cursor-not-allowed disabled:opacity-40"
            aria-label="Agrandir le diagramme Mermaid"
            title="Agrandir"
          >
            <Maximize2 className="h-3.5 w-3.5" aria-hidden="true" />
          </button>
          <button
            type="button"
            onClick={downloadSvg}
            disabled={!svg}
            className="rounded p-1.5 text-neutral-500 hover:bg-neutral-800 hover:text-neutral-100 disabled:cursor-not-allowed disabled:opacity-40"
            aria-label="Télécharger le diagramme Mermaid en SVG"
            title="Télécharger SVG"
          >
            <Download className="h-3.5 w-3.5" aria-hidden="true" />
          </button>
          <button
            type="button"
            onClick={() => setShowSource((value) => !value)}
            className="rounded p-1.5 text-neutral-500 hover:bg-neutral-800 hover:text-neutral-100"
            aria-label={showSource ? 'Masquer la source Mermaid' : 'Afficher la source Mermaid'}
            aria-pressed={showSource}
            title={showSource ? 'Masquer la source' : 'Afficher la source'}
          >
            <Code2 className="h-3.5 w-3.5" aria-hidden="true" />
          </button>
          <button
            type="button"
            onClick={() => void copySource()}
            className="rounded p-1.5 text-neutral-500 hover:bg-neutral-800 hover:text-neutral-100"
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
        <div className="flex min-h-28 justify-start overflow-x-auto p-4">
          {svg ? (
            <div
              className="min-w-[860px] text-neutral-100 [&_svg]:h-auto [&_svg]:w-full [&_svg]:max-w-none [&_svg]:min-w-[860px]"
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
        <div className="border-t border-neutral-800 p-3">
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
            className="max-h-[88vh] max-w-[94vw] overflow-auto rounded-lg border border-neutral-800 bg-neutral-950 p-6 text-neutral-100 shadow-2xl [&_svg]:h-auto [&_svg]:w-full [&_svg]:max-w-none [&_svg]:min-w-[1100px]"
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
    <pre className="max-h-80 overflow-auto rounded bg-neutral-900 p-2 text-xs text-neutral-300">
      <code>{text}</code>
    </pre>
  );
}

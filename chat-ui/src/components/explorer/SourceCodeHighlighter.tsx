import { useEffect, useRef } from 'react';
import type { SourceContent } from '../../api/mcp-client';
import {
  codeThemeStyle,
  resolveCodeLanguage,
  SyntaxHighlighter,
  useDocumentCodeTheme,
} from '../ui/code-highlighting';

export interface SourceCodeHighlightState {
  searchLines: Set<number>;
  activeLine: number | null;
  targetStartLine?: number;
  targetEndLine?: number;
}

export function SourceCodeHighlighter({
  source,
  highlight,
}: {
  source: SourceContent;
  highlight: SourceCodeHighlightState;
}) {
  const theme = useDocumentCodeTheme();
  const language = resolveCodeLanguage(source.language, source.path);
  const containerRef = useRef<HTMLDivElement | null>(null);

  useEffect(() => {
    const lineNumber = highlight.activeLine ?? highlight.targetStartLine;
    if (!lineNumber) return;
    const line = containerRef.current?.querySelector<HTMLElement>(`[data-source-line="${lineNumber}"]`);
    line?.scrollIntoView?.({ block: 'center' });
  }, [highlight.activeLine, highlight.targetStartLine, source.path]);

  if (!language) {
    return <PlainSourceCode source={source} highlight={highlight} />;
  }

  return (
    <div
      ref={containerRef}
      className="source-code-view source-code-highlight min-h-0 flex-1 overflow-auto text-[11px]"
      data-testid="source-code-view"
    >
      <SyntaxHighlighter
        language={language}
        style={codeThemeStyle(theme)}
        showLineNumbers
        startingLineNumber={source.startLine || 1}
        wrapLines
        lineProps={(lineNumber) => ({
          className: sourceLineClass(lineNumber, highlight),
          'data-source-line': lineNumber,
        })}
        PreTag="div"
        customStyle={{
          background: 'var(--source-bg)',
          color: 'var(--source-text)',
          fontSize: '0.6875rem',
          lineHeight: 1.55,
          margin: 0,
          overflow: 'visible',
          padding: '0.35rem 0',
        }}
        codeTagProps={{
          style: {
            fontFamily:
              'ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, "Liberation Mono", monospace',
          },
        }}
        lineNumberStyle={{
          borderRight: '1px solid var(--source-border)',
          color: 'var(--source-line)',
          marginRight: '0.75rem',
          minWidth: '3rem',
          paddingRight: '0.75rem',
          textAlign: 'right',
          userSelect: 'none',
        }}
      >
        {source.content || ' '}
      </SyntaxHighlighter>
      {source.truncated && (
        <div className="source-truncated border-t px-3 py-2 text-xs">
          Extrait limite aux premieres lignes demandees.
        </div>
      )}
    </div>
  );
}

function PlainSourceCode({
  source,
  highlight,
}: {
  source: SourceContent;
  highlight: SourceCodeHighlightState;
}) {
  const containerRef = useRef<HTMLPreElement | null>(null);
  const lines = source.content ? source.content.split('\n') : [];
  const start = source.startLine || 1;

  useEffect(() => {
    const lineNumber = highlight.activeLine ?? highlight.targetStartLine;
    if (!lineNumber) return;
    const line = containerRef.current?.querySelector<HTMLElement>(`[data-source-line="${lineNumber}"]`);
    line?.scrollIntoView?.({ block: 'center' });
  }, [highlight.activeLine, highlight.targetStartLine, source.path]);

  return (
    <pre
      ref={containerRef}
      className="source-code-view min-h-0 flex-1 overflow-auto p-0 text-[11px] leading-5"
      data-testid="source-code-view"
    >
      <code>
        {lines.map((line, index) => (
          <div
            key={`${source.path}-${start + index}`}
            className={`source-code-row flex ${sourceLineClass(start + index, highlight)}`}
            data-source-line={start + index}
          >
            <span className="source-line-number w-12 shrink-0 select-none border-r pr-3 text-right">
              {start + index}
            </span>
            <span className="source-code-text min-w-0 flex-1 whitespace-pre px-3 font-mono">
              {line || ' '}
            </span>
          </div>
        ))}
        {source.truncated && (
          <div className="source-truncated border-t px-3 py-2 text-xs">
            Extrait limite aux premieres lignes demandees.
          </div>
        )}
      </code>
    </pre>
  );
}

function sourceLineClass(lineNumber: number, highlight: SourceCodeHighlightState): string {
  const classes = ['source-code-line'];
  if (isTargetLine(lineNumber, highlight)) classes.push('source-line-target');
  if (highlight.searchLines.has(lineNumber)) classes.push('source-line-match');
  if (highlight.activeLine === lineNumber) classes.push('source-line-active');
  return classes.join(' ');
}

function isTargetLine(lineNumber: number, highlight: SourceCodeHighlightState): boolean {
  if (!highlight.targetStartLine) return false;
  const end = highlight.targetEndLine ?? highlight.targetStartLine;
  return lineNumber >= highlight.targetStartLine && lineNumber <= end;
}

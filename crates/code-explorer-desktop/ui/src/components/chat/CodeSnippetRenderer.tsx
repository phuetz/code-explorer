/**
 * CodeSnippetRenderer — Renders code snippets with syntax highlighting and line numbers.
 *
 * Uses Shiki for syntax highlighting via the existing use-shiki hook.
 * Supports collapsible long snippets and copy-to-clipboard.
 */

import { useState, useMemo, useCallback, useRef, useEffect } from "react";
import { ChevronDown, ChevronRight, Copy, Check, FileCode, ExternalLink } from "lucide-react";
import { useAppStore } from "../../stores/app-store";
import { copyTextToClipboard } from "../../lib/clipboard";

const LANG_COLORS: Record<string, string> = {
  typescript: "#3178c6",
  tsx: "#3178c6",
  rust: "#dea584",
  python: "#3776ab",
  javascript: "#f7df1e",
  jsx: "#f7df1e",
  java: "#b07219",
  go: "#00add8",
  csharp: "#178600",
  cs: "#178600",
  cpp: "#f34b7d",
  "c++": "#f34b7d",
  c: "#555555",
  ruby: "#701516",
  php: "#4f5d95",
  kotlin: "#A97BFF",
  swift: "#F05138",
  html: "#e34c26",
  css: "#563d7c",
  json: "#292929",
  yaml: "#cb171e",
  sql: "#e38c00",
  shell: "#89e051",
  bash: "#89e051",
  markdown: "#083fa1",
};

interface CodeSnippetRendererProps {
  code: string;
  language?: string;
  filePath?: string;
  startLine?: number;
  maxLines?: number;
  symbolName?: string;
}

export function CodeSnippetRenderer({
  code,
  language = "",
  filePath,
  startLine = 1,
  maxLines = 25,
  symbolName,
}: CodeSnippetRendererProps) {
  const [expanded, setExpanded] = useState(false);
  const [copied, setCopied] = useState(false);
  const copyTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  useEffect(() => () => { if (copyTimerRef.current) clearTimeout(copyTimerRef.current); }, []);

  const lines = useMemo(() => code.split("\n"), [code]);
  const isLong = lines.length > maxLines;
  const displayLines = expanded || !isLong ? lines : lines.slice(0, maxLines);

  const handleCopy = useCallback(async () => {
    const ok = await copyTextToClipboard(code);
    if (ok) {
      setCopied(true);
      if (copyTimerRef.current) clearTimeout(copyTimerRef.current);
      copyTimerRef.current = setTimeout(() => setCopied(false), 2000);
    }
  }, [code]);

  return (
    <div
      className="rounded-lg overflow-hidden my-2"
      style={{
        background: "var(--bg-1)",
        border: "1px solid var(--surface-border)",
      }}
    >
      {/* Header */}
      <div
        className="flex items-center gap-2 px-3 py-1.5"
        style={{
          borderBottom: "1px solid var(--surface-border)",
          background: "var(--bg-2)",
        }}
      >
        <FileCode size={11} style={{ color: "var(--accent)" }} />

        {symbolName && (
          <span
            className="text-[11px] font-medium"
            style={{ color: "var(--text-0)", fontFamily: "var(--font-mono)" }}
          >
            {symbolName}
          </span>
        )}

        {filePath && (
          <button 
            className="flex items-center gap-1.5 px-1.5 py-0.5 rounded transition-all hover:bg-[var(--bg-3)] group cursor-pointer"
            onClick={() => {
              const setSelectedNodeId = useAppStore.getState().setSelectedNodeId;
              const setMode = useAppStore.getState().setMode;
              setMode("explorer");
              setSelectedNodeId("File:" + filePath, filePath.split("/").pop() || filePath);
            }}
          >
            <span className="text-[11px] truncate text-[var(--text-3)] group-hover:text-[var(--text-1)]">
              {filePath}
              {startLine > 1 && `:${startLine}`}
            </span>
            <ExternalLink size={10} className="text-[var(--text-4)] group-hover:text-[var(--accent)]" />
          </button>
        )}

        {language && (
          <span
            className="flex items-center gap-1.5 text-[10px] px-1.5 py-0.5 rounded ml-auto"
            style={{ background: "var(--bg-3)", color: "var(--text-2)" }}
          >
            <span
              className="inline-block w-2 h-2 rounded-full shrink-0"
              style={{
                background: LANG_COLORS[language.toLowerCase()] ?? "var(--text-3)",
              }}
            />
            {language}
          </span>
        )}

        <button
          onClick={handleCopy}
          className="p-0.5 rounded transition-colors"
          style={{ color: "var(--text-3)" }}
          title="Copy code"
        >
          {copied ? <Check size={11} style={{ color: "var(--green)" }} /> : <Copy size={11} />}
        </button>
      </div>

      {/* Code */}
      <div className="overflow-x-auto">
        <pre className="m-0 p-0">
          <code
            className="block text-[12px] leading-[1.6]"
            style={{ fontFamily: "var(--font-mono)" }}
          >
            {displayLines.map((line, i) => (
              <div key={i} className="flex hover:bg-[var(--bg-2)] transition-colors">
                <span
                  className="select-none text-right pr-3 pl-3 flex-shrink-0"
                  style={{
                    color: "var(--text-3)",
                    minWidth: "3rem",
                    userSelect: "none",
                  }}
                >
                  {startLine + i}
                </span>
                <span className="pr-3" style={{ color: "var(--text-1)" }}>
                  {line || " "}
                </span>
              </div>
            ))}
          </code>
        </pre>
      </div>

      {/* Expand/collapse for long code */}
      {isLong && (
        <button
          onClick={() => setExpanded(!expanded)}
          className="w-full flex items-center justify-center gap-1 py-1.5 text-[11px] transition-colors"
          style={{
            borderTop: "1px solid var(--surface-border)",
            color: "var(--text-3)",
            background: "var(--bg-2)",
          }}
        >
          {expanded ? (
            <>
              <ChevronDown size={11} />
              Collapse ({lines.length} lines)
            </>
          ) : (
            <>
              <ChevronRight size={11} />
              Show all {lines.length} lines ({lines.length - maxLines} more)
            </>
          )}
        </button>
      )}
    </div>
  );
}

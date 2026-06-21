/**
 * SourceReferences — Enhanced source citations with expandable code snippets.
 *
 * Replaces the simple SourcesList in ChatPanel with richer details:
 * - Expandable source cards with code preview
 * - Relationship context (callers, callees, community)
 * - Relevance score indicator
 * - Click to navigate to graph node
 */

import { useState } from "react";
import { useI18n } from "../../hooks/use-i18n";
import {
  FileCode,
  ChevronDown,
  ChevronRight,
  GitBranch,
  FolderTree,
  Star,
} from "lucide-react";
import type { ChatSource } from "../../lib/tauri-commands";
import { CodeSnippetRenderer } from "./CodeSnippetRenderer";

interface SourceReferencesProps {
  sources: ChatSource[];
  maxInitial?: number;
  onNavigateToNode?: (nodeId: string) => void;
}

export function SourceReferences({
  sources,
  maxInitial = 4,
  onNavigateToNode,
}: SourceReferencesProps) {
  const { t } = useI18n();
  const [showAll, setShowAll] = useState(false);
  const safeSources = sources ?? [];
  const displaySources = showAll ? safeSources : safeSources.slice(0, maxInitial);

  if (safeSources.length === 0) return null;

  return (
    <div className="mt-3">
      <div className="flex items-center gap-1.5 mb-2">
        <FileCode size={12} style={{ color: "var(--text-2)" }} />
        <span className="text-[12px] font-medium" style={{ color: "var(--text-2)" }}>
          {safeSources.length} {t("sources.title").toLowerCase()}
        </span>
      </div>

      <div className="space-y-1.5">
        {displaySources.map((source) => (
          <SourceCard
            key={`${source.nodeId}-${source.symbolName}`}
            source={source}
            onNavigate={onNavigateToNode}
          />
        ))}
      </div>

      {safeSources.length > maxInitial && (
        <button
          onClick={() => setShowAll(!showAll)}
          className="flex items-center gap-1 mt-2 text-[11px] transition-colors"
          style={{ color: "var(--text-3)" }}
        >
          {showAll ? (
            <>
              <ChevronDown size={11} />
              {t("sources.showFewer")}
            </>
          ) : (
            <>
              <ChevronRight size={11} />
              {t("sources.showMore").replace("{0}", String(safeSources.length - maxInitial))}
            </>
          )}
        </button>
      )}
    </div>
  );
}

// ─── SourceCard ─────────────────────────────────────────────────────

function SourceCard({
  source,
  onNavigate,
}: {
  source: ChatSource;
  onNavigate?: (nodeId: string) => void;
}) {
  const [expanded, setExpanded] = useState(false);

  const lang = (source.filePath ?? "").split(".").pop() || "";
  const relevanceWidth = Math.min(source.relevanceScore * 100, 100);

  const toggleExpanded = () => setExpanded(!expanded);

  return (
    <div
      className="rounded-lg overflow-hidden"
      style={{
        background: "var(--surface)",
        border: "1px solid var(--surface-border)",
      }}
    >
      {/* Header — role=button on a div because we need a nested button inside,
          and nested <button> elements are invalid HTML. */}
      <div
        role="button"
        tabIndex={0}
        aria-expanded={expanded}
        onClick={toggleExpanded}
        onKeyDown={(e) => {
          if (e.key === "Enter" || e.key === " ") {
            e.preventDefault();
            toggleExpanded();
          }
        }}
        className="w-full flex items-center gap-2 px-2.5 py-2 text-left cursor-pointer"
      >
        {/* Expand indicator */}
        {source.snippet ? (
          expanded ? (
            <ChevronDown size={11} style={{ color: "var(--text-3)", flexShrink: 0 }} />
          ) : (
            <ChevronRight size={11} style={{ color: "var(--text-3)", flexShrink: 0 }} />
          )
        ) : (
          <FileCode size={11} style={{ color: "var(--accent)", flexShrink: 0 }} />
        )}

        {/* Symbol name */}
        <span
          className="font-medium text-[12px]"
          style={{ color: "var(--text-0)", fontFamily: "var(--font-mono)" }}
        >
          {source.symbolName}
        </span>

        {/* Symbol type */}
        <span
          className="text-[10px] px-1 py-0.5 rounded"
          style={{ background: "var(--bg-3)", color: "var(--text-2)" }}
        >
          {source.symbolType}
        </span>

        {/* File path */}
        <span className="text-[11px] truncate flex-1" style={{ color: "var(--text-3)" }}>
          {source.filePath}
          {source.startLine != null && `:${source.startLine}`}
        </span>

        {/* Relevance indicator */}
        <div
          className="w-8 h-1 rounded-full flex-shrink-0"
          style={{ background: "var(--bg-3)" }}
          title={`Relevance: ${(source.relevanceScore * 100).toFixed(0)}%`}
        >
          <div
            className="h-full rounded-full"
            style={{
              width: `${relevanceWidth}%`,
              background: "var(--accent)",
            }}
          />
        </div>

        {/* Navigate button */}
        {onNavigate && (
          <button
            onClick={(e) => {
              e.stopPropagation();
              onNavigate(source.nodeId);
            }}
            className="p-0.5 rounded transition-colors"
            style={{ color: "var(--text-3)" }}
            title="Navigate to node in graph"
          >
            <Star size={10} />
          </button>
        )}
      </div>

      {/* Expanded details */}
      {expanded && (
        <div
          className="px-2.5 pb-2"
          style={{ borderTop: "1px solid var(--surface-border)" }}
        >
          {/* Relationships */}
          <div className="flex flex-wrap gap-x-4 gap-y-1 py-1.5 text-[11px]">
            {source.callers && source.callers.length > 0 && (
              <div className="flex items-center gap-1" style={{ color: "var(--text-2)" }}>
                <GitBranch size={10} style={{ color: "var(--green)" }} />
                <span>Called by: {source.callers.slice(0, 3).join(", ")}</span>
                {source.callers.length > 3 && (
                  <span style={{ color: "var(--text-3)" }}>+{source.callers.length - 3}</span>
                )}
              </div>
            )}
            {source.callees && source.callees.length > 0 && (
              <div className="flex items-center gap-1" style={{ color: "var(--text-2)" }}>
                <GitBranch size={10} style={{ color: "var(--accent)" }} />
                <span>Calls: {source.callees.slice(0, 3).join(", ")}</span>
                {source.callees.length > 3 && (
                  <span style={{ color: "var(--text-3)" }}>+{source.callees.length - 3}</span>
                )}
              </div>
            )}
            {source.community && (
              <div className="flex items-center gap-1" style={{ color: "var(--text-2)" }}>
                <FolderTree size={10} style={{ color: "var(--purple)" }} />
                <span>{source.community}</span>
              </div>
            )}
          </div>

          {/* Code snippet */}
          {source.snippet && (
            <CodeSnippetRenderer
              code={source.snippet}
              language={lang}
              filePath={source.filePath}
              startLine={source.startLine ?? 1}
              symbolName={source.symbolName}
              maxLines={20}
            />
          )}
        </div>
      )}
    </div>
  );
}

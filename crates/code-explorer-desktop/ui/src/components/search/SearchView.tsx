import { useState, useMemo } from "react";
import { Search } from "lucide-react";
import { useSearchSymbols } from "../../hooks/use-tauri-query";
import { useAppStore } from "../../stores/app-store";
import type { SearchResult } from "../../lib/tauri-commands";

/**
 * Re-rank search results to prioritize name matches over path-only matches.
 * Results whose `name` contains the query (case-insensitive) are boosted
 * ahead of results that only match on filePath.
 */
function rankResults(results: SearchResult[], query: string): SearchResult[] {
  if (!query) return results;
  const q = query.toLowerCase();
  return [...results].sort((a, b) => {
    const aNameMatch = a.name.toLowerCase().includes(q);
    const bNameMatch = b.name.toLowerCase().includes(q);
    const aExact = a.name.toLowerCase() === q;
    const bExact = b.name.toLowerCase() === q;
    // Exact name matches first
    if (aExact !== bExact) return aExact ? -1 : 1;
    // Name-contains matches next
    if (aNameMatch !== bNameMatch) return aNameMatch ? -1 : 1;
    // Preserve backend score order otherwise
    return b.score - a.score;
  });
}

export function SearchView() {
  const [query, setQuery] = useState("");
  const { data: results, isLoading } = useSearchSymbols(query, query.length >= 2);
  const setSelectedNodeId = useAppStore((s) => s.setSelectedNodeId);
  const setMode = useAppStore((s) => s.setMode);

  const rankedResults = useMemo(
    () => (results ? rankResults(results, query) : undefined),
    [results, query]
  );

  const handleSelect = (nodeId: string, name?: string) => {
    setSelectedNodeId(nodeId, name);
    setMode("explorer");
  };

  return (
    <div className="h-full flex flex-col p-4">
      {/* Search input */}
      <div className="relative mb-4">
        <Search
          size={16}
          className="absolute left-3 top-1/2 -translate-y-1/2 text-[var(--text-muted)]"
        />
        <input
          type="text"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          placeholder="Search symbols..."
          autoFocus
          className="w-full pl-9 pr-3 py-2 rounded-lg border border-[var(--border)] bg-[var(--bg-secondary)] text-[var(--text-primary)] placeholder-[var(--text-muted)] focus:outline-none focus:border-[var(--accent)]"
        />
      </div>

      {/* Result count */}
      {rankedResults && !isLoading && (
        <div className="text-[11px] mb-2" style={{ color: "var(--text-3)" }}>
          {rankedResults.length} {rankedResults.length === 1 ? "result" : "results"}
          {query.length >= 2 && ` for "${query}"`}
        </div>
      )}

      {/* Results */}
      <div className="flex-1 overflow-y-auto space-y-0.5">
        {isLoading && (
          <p className="text-[var(--text-muted)] text-center py-4">
            Searching...
          </p>
        )}
        {rankedResults?.map((r) => {
          // Highlight name matches vs path-only matches
          const nameMatch = query.length >= 2 && r.name.toLowerCase().includes(query.toLowerCase());
          return (
            <button
              key={r.nodeId}
              onClick={() => handleSelect(r.nodeId, r.name)}
              className="flex items-center gap-2 w-full px-3 py-2 rounded hover:bg-[var(--bg-tertiary)] transition-colors text-left"
            >
              <span className="text-[10px] px-1.5 py-0.5 rounded bg-[var(--bg-tertiary)] text-[var(--accent)] shrink-0">
                {r.label}
              </span>
              <span
                className="font-medium truncate"
                style={{ color: nameMatch ? "var(--accent)" : undefined }}
              >
                {r.name}
              </span>
              <span className="ml-auto text-[10px] text-[var(--text-muted)] truncate max-w-[200px]">
                {r.filePath}
                {r.startLine && `:${r.startLine}`}
              </span>
            </button>
          );
        })}
        {rankedResults && rankedResults.length === 0 && query.length >= 2 && (
          <p className="text-[var(--text-muted)] text-center py-4">
            No results found
          </p>
        )}
      </div>
    </div>
  );
}

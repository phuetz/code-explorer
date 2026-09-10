import { useState, useEffect, useRef, useCallback, useMemo } from "react";
import { Search, X, CornerDownLeft, ArrowUp, ArrowDown } from "lucide-react";
import { useSearchSymbols } from "../../hooks/use-tauri-query";
import { useAppStore } from "../../stores/app-store";
import { useI18n } from "../../hooks/use-i18n";
import { NodeIcon } from "../shared/NodeIcon";
import type { SearchResult } from "../../lib/tauri-commands";

type RankedResult = SearchResult & { _fuzzy: number };

/** Simple fuzzy match: checks if all query chars appear in name in order */
function fuzzyScore(name: string, query: string): number {
  const n = name.toLowerCase();
  const q = query.toLowerCase();
  if (n === q) return 100;
  if (n.startsWith(q)) return 90;
  if (n.includes(q)) return 80;

  // Fuzzy: check if all chars of query appear in order in name
  let qi = 0;
  let consecutiveBonus = 0;
  let lastMatchIdx = -2;
  for (let ni = 0; ni < n.length && qi < q.length; ni++) {
    if (n[ni] === q[qi]) {
      if (ni === lastMatchIdx + 1) consecutiveBonus += 5;
      lastMatchIdx = ni;
      qi++;
    }
  }
  if (qi < q.length) return 0; // not all chars matched
  return 50 + consecutiveBonus - (name.length - query.length) * 0.5;
}

/** Re-rank results: exact > starts-with > contains > fuzzy, with type filter */
function rankResults(results: SearchResult[], query: string, typeFilter?: string): RankedResult[] {
  if (!query) return results.map((r) => ({ ...r, _fuzzy: 0 }));

  let filtered = results;
  if (typeFilter) {
    filtered = results.filter((r) => r.label === typeFilter);
  }

  return filtered
    .map((r) => ({ ...r, _fuzzy: fuzzyScore(r.name, query) }))
    .filter((r) => r._fuzzy > 0)
    .sort((a, b) => b._fuzzy - a._fuzzy);
}

export function SearchModal() {
  const { t } = useI18n();
  const isOpen = useAppStore((s) => s.searchOpen);
  const setSearchOpen = useAppStore((s) => s.setSearchOpen);
  const storeQuery = useAppStore((s) => s.searchQuery);
  const setStoreQuery = useAppStore((s) => s.setSearchQuery);
  const setSelectedNodeId = useAppStore((s) => s.setSelectedNodeId);
  const setMode = useAppStore((s) => s.setMode);

  const [query, setQuery] = useState("");
  const [debouncedQuery, setDebouncedQuery] = useState("");
  const [selectedIndex, setSelectedIndex] = useState(0);
  const [typeFilter, setTypeFilter] = useState<string | undefined>(undefined);
  const inputRef = useRef<HTMLInputElement>(null);
  const debounceRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  const TYPE_FILTERS = ["Class", "Method", "Function", "Controller", "Service", "Interface"];

  // Clean up debounce timer on unmount
  useEffect(() => () => { if (debounceRef.current) clearTimeout(debounceRef.current); }, []);

  // Debounce search query (200ms)
  const handleQueryChange = useCallback((value: string) => {
    setQuery(value);
    if (debounceRef.current) clearTimeout(debounceRef.current);
    debounceRef.current = setTimeout(() => setDebouncedQuery(value), 200);
  }, []);

  const { data: rawResults, isLoading: isSearching } = useSearchSymbols(debouncedQuery, debouncedQuery.length >= 1);
  const results = useMemo(
    () => (rawResults ? rankResults(rawResults, query, typeFilter) : undefined),
    [rawResults, query, typeFilter]
  );

  const setSearchMatchIds = useAppStore((s) => s.setSearchMatchIds);

  useEffect(() => {
    if (isOpen && results && results.length > 0) {
      setSearchMatchIds(results.slice(0, 20).map((r) => r.nodeId));
    } else {
      setSearchMatchIds([]);
    }
  }, [results, isOpen, setSearchMatchIds]);

  // Sync local query from store (render-time adjustment for local state)
  const [prevIsOpen, setPrevIsOpen] = useState(isOpen);
  if (isOpen !== prevIsOpen) {
    setPrevIsOpen(isOpen);
    if (isOpen) {
      setQuery(storeQuery || "");
      setDebouncedQuery(storeQuery || "");
      setSelectedIndex(0);
    }
  }

  const [prevStoreQuery, setPrevStoreQuery] = useState(storeQuery);
  if (storeQuery !== prevStoreQuery) {
    setPrevStoreQuery(storeQuery);
    if (isOpen && storeQuery) {
      setQuery(storeQuery);
      setDebouncedQuery(storeQuery);
    }
  }

  // Clear global store query when modal closes (must be in effect — updates external store)
  const wasOpenRef = useRef(isOpen);
  useEffect(() => {
    if (!isOpen && wasOpenRef.current) {
      setStoreQuery("");
    }
    wasOpenRef.current = isOpen;
  }, [isOpen, setStoreQuery]);

  // Focus input after opening
  useEffect(() => {
    if (isOpen) {
      const timer = setTimeout(() => inputRef.current?.focus(), 50);
      return () => clearTimeout(timer);
    }
  }, [isOpen]);

  // Reset selection only when the *backend* results change, not on every keystroke.
  // The derived `results` memo also depends on `query`, so using it here would
  // wipe the user's keyboard selection on each character they type.
  const [prevRawResults, setPrevRawResults] = useState(rawResults);
  if (rawResults !== prevRawResults) {
    setPrevRawResults(rawResults);
    setSelectedIndex(0);
  }

  // Scroll selected result into view when selectedIndex changes
  useEffect(() => {
    const el = document.querySelector(`[data-search-index="${selectedIndex}"]`);
    el?.scrollIntoView({ block: "nearest" });
  }, [selectedIndex]);

  const selectResult = useCallback(
    (nodeId: string, name?: string) => {
      setSelectedNodeId(nodeId, name);
      setMode("explorer");
      setSearchMatchIds([]);
      setSearchOpen(false);
    },
    [setSelectedNodeId, setMode, setSearchMatchIds, setSearchOpen]
  );

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "ArrowDown") {
      e.preventDefault();
      setSelectedIndex((i) => Math.min(i + 1, Math.max((results?.length ?? 1) - 1, 0)));
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      setSelectedIndex((i) => Math.max(i - 1, 0));
    } else if (e.key === "Enter" && results?.[selectedIndex]) {
      selectResult(results[selectedIndex].nodeId, results[selectedIndex].name);
    } else if (e.key === "Escape") {
      setSearchOpen(false);
    }
  };

  if (!isOpen) return null;

  return (
    <div
      className="fixed inset-0 z-50 flex items-start justify-center pt-[15vh]"
      role="dialog"
      aria-modal="true"
      aria-label="Search"
      onClick={() => setSearchOpen(false)}
    >
      {/* Backdrop */}
      <div className="absolute inset-0" style={{ background: "rgba(0,0,0,0.6)", backdropFilter: "blur(4px)" }} />

      {/* Modal */}
      <div
        className="relative w-full max-w-[560px] rounded-xl overflow-hidden fade-in"
        style={{
          background: "var(--bg-2)",
          border: "1px solid var(--surface-border-hover)",
          boxShadow: "var(--shadow-lg), 0 0 0 1px rgba(0,0,0,0.1)",
        }}
        onClick={(e) => e.stopPropagation()}
      >
        {/* Input */}
        <div className="flex items-center gap-3 px-4 py-3" style={{ borderBottom: "1px solid var(--surface-border)" }}>
          <Search size={18} className="shrink-0" style={{ color: "var(--text-3)" }} />
          <input
            ref={inputRef}
            type="text"
            value={query}
            onChange={(e) => handleQueryChange(e.target.value)}
            onKeyDown={handleKeyDown}
            placeholder={t("search.placeholder")}
            aria-label="Search symbols"
            className="flex-1 bg-transparent outline-none text-[15px]"
            style={{ color: "var(--text-0)", fontFamily: "var(--font-body)" }}
          />
          <button
            onClick={() => setSearchOpen(false)}
            className="p-1 rounded-md"
            style={{ color: "var(--text-3)" }}
            aria-label={t("search.close")}
          >
            <X size={16} />
          </button>
        </div>

        {/* Type filter pills */}
        {query.length >= 1 && (
          <div className="flex items-center gap-1.5 px-4 py-2" style={{ borderBottom: "1px solid var(--surface-border)" }}>
            <button
              onClick={() => setTypeFilter(undefined)}
              className="text-[10px] font-medium px-2 py-1 rounded-full transition-colors"
              style={{
                background: !typeFilter ? "var(--accent)" : "var(--bg-3)",
                color: !typeFilter ? "white" : "var(--text-3)",
                border: "none",
                cursor: "pointer",
              }}
            >
              All
            </button>
            {TYPE_FILTERS.map((tf) => (
              <button
                key={tf}
                onClick={() => setTypeFilter(typeFilter === tf ? undefined : tf)}
                className="text-[10px] font-medium px-2 py-1 rounded-full transition-colors"
                style={{
                  background: typeFilter === tf ? "var(--accent)" : "var(--bg-3)",
                  color: typeFilter === tf ? "white" : "var(--text-3)",
                  border: "none",
                  cursor: "pointer",
                }}
              >
                {tf}
              </button>
            ))}
          </div>
        )}

        {/* Results */}
        <div className="max-h-[400px] overflow-y-auto py-1">
          {results && results.length > 0 ? (
            <div role="listbox">
              {results.map((r, i) => (
                <button
                  key={r.nodeId}
                  role="option"
                  aria-selected={selectedIndex === i}
                  data-search-index={i}
                  onClick={() => selectResult(r.nodeId, r.name)}
                  className="w-full flex items-center gap-3 px-4 py-2.5 text-left transition-colors"
                  style={{
                    background: i === selectedIndex ? "var(--accent-subtle)" : "transparent",
                  }}
                  onMouseEnter={() => setSelectedIndex(i)}
                >
                  <NodeIcon label={r.label} size={18} />
                  <div className="flex-1 min-w-0">
                    <div className="flex items-center gap-2">
                      <span
                        className="font-medium truncate text-[13px]"
                        style={{ color: i === selectedIndex ? "var(--text-0)" : "var(--text-1)" }}
                      >
                        {r.name}
                      </span>
                      <span
                        className="badge shrink-0"
                        style={{
                          background: "var(--bg-4)",
                          color: "var(--text-3)",
                          fontSize: 10,
                        }}
                      >
                        {r.label}
                      </span>
                    </div>
                    <span className="text-[11px] truncate block" style={{ color: "var(--text-3)" }}>
                      {r.filePath}
                      {r.startLine && `:${r.startLine}`}
                    </span>
                  </div>
                  {i === selectedIndex && (
                    <CornerDownLeft size={13} style={{ color: "var(--text-3)" }} />
                  )}
                </button>
              ))}
            </div>
          ) : query.length >= 1 && isSearching ? (
            <div className="py-12 text-center" style={{ color: "var(--text-4)" }}>
              <div className="shimmer" style={{ width: 120, height: 14, borderRadius: 6, background: "var(--bg-3)", margin: "0 auto" }} />
            </div>
          ) : query.length >= 1 ? (
            <div className="py-12 text-center" style={{ color: "var(--text-3)" }}>
              {t("search.noResults")}
            </div>
          ) : (
            <div className="py-12 text-center" style={{ color: "var(--text-3)" }}>
              {t("search.startTyping")}
            </div>
          )}
        </div>

        {/* Footer hints */}
        <div
          className="flex items-center gap-4 px-4 py-2 text-[11px]"
          style={{ borderTop: "1px solid var(--surface-border)", color: "var(--text-3)" }}
          aria-label="Keyboard shortcuts: Up/Down arrows to navigate, Enter to select, Escape to close"
        >
          <span className="flex items-center gap-1">
            <ArrowUp size={11} /><ArrowDown size={11} /> {t("search.navigate")}
          </span>
          <span className="flex items-center gap-1">
            <CornerDownLeft size={11} /> {t("search.open")}
          </span>
          <span className="flex items-center gap-1">
            <kbd className="font-mono text-[10px]">Esc</kbd> {t("search.close")}
          </span>
        </div>
      </div>
    </div>
  );
}

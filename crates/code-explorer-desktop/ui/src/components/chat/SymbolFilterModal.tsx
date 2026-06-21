/**
 * SymbolFilterModal — Ctrl+Shift+O style symbol picker for filtering chat context.
 *
 * Features:
 * - Fuzzy search across all indexed symbols
 * - Shows symbol kind, file path, and container
 * - Kind icons (function, class, method, etc.)
 * - Keyboard navigation
 */

import { useState, useEffect, useRef, useCallback } from "react";
import { useQuery } from "@tanstack/react-query";
import {
  Search, X, Check, Loader2,
  Braces, Box, Cog, Diamond, Hash,
  GitBranch, Type,
} from "lucide-react";
import { commands } from "../../lib/tauri-commands";
import { useI18n } from "../../hooks/use-i18n";
import type { SymbolQuickPick } from "../../lib/tauri-commands";
import { useChatStore } from "../../stores/chat-store";
import { useAppStore } from "../../stores/app-store";

interface SymbolFilterModalProps {
  open: boolean;
  onClose: () => void;
}

// Stable module-level empty array so the compare-in-render reset below
// doesn't see a new reference each render.
const EMPTY_RESULTS: SymbolQuickPick[] = [];

export function SymbolFilterModal({ open, onClose }: SymbolFilterModalProps) {
  const { t } = useI18n();
  const activeRepo = useAppStore((s) => s.activeRepo);
  const [query, setQuery] = useState("");
  const [selectedIndex, setSelectedIndex] = useState(0);
  const inputRef = useRef<HTMLInputElement>(null);
  const listRef = useRef<HTMLDivElement>(null);

  const { filters, addSymbolFilter, removeSymbolFilter } = useChatStore();

  // Scope by `activeRepo` so switching repos doesn't return the previous
  // repo's cached symbol picks for the same query.
  const { data, isLoading } = useQuery({
    queryKey: ["chatPickSymbols", activeRepo, query],
    queryFn: () => commands.chatPickSymbols(query, undefined, 40),
    enabled: open,
    staleTime: 2000,
  });
  const results = data ?? EMPTY_RESULTS;

  // Reset on open (render-time state adjustment)
  const [prevOpen, setPrevOpen] = useState(open);
  if (open !== prevOpen) {
    setPrevOpen(open);
    if (open) {
      setQuery("");
      setSelectedIndex(0);
    }
  }

  // Focus input after opening
  useEffect(() => {
    if (open) {
      const timer = setTimeout(() => inputRef.current?.focus(), 50);
      return () => clearTimeout(timer);
    }
  }, [open]);

  // Reset selection on results change (render-time state adjustment)
  const [prevResults, setPrevResults] = useState(results);
  if (results !== prevResults) {
    setPrevResults(results);
    setSelectedIndex(0);
  }

  useEffect(() => {
    const list = listRef.current;
    if (!list) return;
    const item = list.children[selectedIndex] as HTMLElement | undefined;
    item?.scrollIntoView({ block: "nearest" });
  }, [selectedIndex]);

  const toggleSymbol = useCallback(
    (name: string, keepOpen = false) => {
      const already = filters.symbols.includes(name);
      if (already) {
        removeSymbolFilter(name);
      } else {
        addSymbolFilter(name);
      }
      // Auto-close on add — Shift/Alt keeps the modal open for multi-select.
      if (!already && !keepOpen) onClose();
    },
    [filters.symbols, addSymbolFilter, removeSymbolFilter, onClose]
  );

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "ArrowDown") {
      e.preventDefault();
      setSelectedIndex((i) => Math.min(i + 1, Math.max(results.length - 1, 0)));
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      setSelectedIndex((i) => Math.max(i - 1, 0));
    } else if (e.key === "Enter" && results[selectedIndex]) {
      e.preventDefault();
      toggleSymbol(results[selectedIndex].name, e.shiftKey);
    } else if (e.key === "Escape") {
      onClose();
    }
  };

  if (!open) return null;

  return (
    <div
      className="fixed inset-0 z-50 flex items-start justify-center pt-[15vh]"
      onClick={onClose}
    >
      <div className="absolute inset-0" style={{ background: "rgba(0,0,0,0.4)" }} />

      <div
        className="relative w-[560px] max-h-[420px] rounded-xl shadow-2xl overflow-hidden flex flex-col"
        style={{ background: "var(--bg-0)", border: "1px solid var(--surface-border)" }}
        onClick={(e) => e.stopPropagation()}
      >
        {/* Search input */}
        <div
          className="flex items-center gap-2 px-3 py-2.5"
          style={{ borderBottom: "1px solid var(--surface-border)" }}
        >
          <Search size={14} style={{ color: "var(--text-3)", flexShrink: 0 }} />
          <input
            ref={inputRef}
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            onKeyDown={handleKeyDown}
            placeholder={t("filters.searchSymbols")}
            className="flex-1 bg-transparent outline-none focus:ring-1 focus:ring-[var(--accent)] text-[13px]"
            style={{ color: "var(--text-0)", fontFamily: "var(--font-body)" }}
          />
          {isLoading && <Loader2 size={14} className="animate-spin" style={{ color: "var(--text-3)" }} />}
          <button onClick={onClose} className="p-0.5 rounded" style={{ color: "var(--text-3)" }} aria-label="Close">
            <X size={14} />
          </button>
        </div>

        {/* Results */}
        <div ref={listRef} className="flex-1 overflow-y-auto py-1">
          {results.length === 0 && !isLoading && (
            <div className="px-4 py-8 text-center text-[13px]" style={{ color: "var(--text-3)" }}>
              {query ? t("filters.noSymbolsFound") : t("filters.typeToSearchSymbols")}
            </div>
          )}

          {results.map((sym, i) => (
            <SymbolItem
              key={`${sym.nodeId}`}
              symbol={sym}
              isSelected={i === selectedIndex}
              isActive={filters.symbols.includes(sym.name)}
              onClick={(e) => toggleSymbol(sym.name, e.altKey || e.shiftKey)}
              onMouseEnter={() => setSelectedIndex(i)}
            />
          ))}
        </div>

        {/* Footer */}
        <div
          className="flex items-center justify-between px-3 py-1.5 text-[11px]"
          style={{ borderTop: "1px solid var(--surface-border)", color: "var(--text-3)" }}
        >
          <span>
            {t("filter.symbolsSelected").replace("{0}", String(filters.symbols.length))}
          </span>
          <span>
            <kbd className="px-1 rounded" style={{ background: "var(--bg-3)" }}>Enter</kbd> {t("filter.toggle")}
            {" · "}
            <kbd className="px-1 rounded" style={{ background: "var(--bg-3)" }}>Esc</kbd> {t("filter.close")}
          </span>
        </div>
      </div>
    </div>
  );
}

// ─── SymbolItem ─────────────────────────────────────────────────────

function SymbolItem({
  symbol,
  isSelected,
  isActive,
  onClick,
  onMouseEnter,
}: {
  symbol: SymbolQuickPick;
  isSelected: boolean;
  isActive: boolean;
  onClick: (event: React.MouseEvent) => void;
  onMouseEnter: () => void;
}) {
  const KindIcon = KIND_ICONS[symbol.kind] || Braces;
  const kindColor = KIND_COLORS[symbol.kind] || "var(--text-3)";

  return (
    <button
      onClick={onClick}
      onMouseEnter={onMouseEnter}
      className="w-full flex items-center gap-2 px-3 py-1.5 text-left transition-colors"
      style={{ background: isSelected ? "var(--surface)" : "transparent" }}
    >
      <div className="w-4 h-4 flex items-center justify-center flex-shrink-0">
        {isActive ? (
          <Check size={12} style={{ color: "var(--accent)" }} />
        ) : (
          <KindIcon size={12} style={{ color: kindColor }} />
        )}
      </div>

      {/* Symbol name */}
      <span
        className="font-medium text-[13px]"
        style={{
          color: isActive ? "var(--accent)" : "var(--text-0)",
          fontFamily: "var(--font-mono)",
        }}
      >
        {symbol.name}
      </span>

      {/* Kind badge */}
      <span
        className="text-[10px] px-1 py-0.5 rounded flex-shrink-0"
        style={{
          background: `color-mix(in srgb, ${kindColor} 10%, transparent)`,
          color: kindColor,
        }}
      >
        {symbol.kind}
      </span>

      {/* Container */}
      {symbol.container && (
        <span className="text-[11px]" style={{ color: "var(--text-3)" }}>
          in {symbol.container}
        </span>
      )}

      {/* File path */}
      <span className="text-[11px] truncate flex-1 text-right" style={{ color: "var(--text-3)" }}>
        {symbol.filePath}
        {symbol.startLine != null && `:${symbol.startLine}`}
      </span>
    </button>
  );
}

// ─── Kind Icons & Colors ────────────────────────────────────────────

const KIND_ICONS: Record<string, typeof Braces> = {
  Function: Braces,
  Method: Cog,
  Constructor: Cog,
  Class: Box,
  Struct: Box,
  Interface: Diamond,
  Trait: Diamond,
  Enum: Hash,
  TypeAlias: Type,
  Variable: GitBranch,
  Constant: GitBranch,
};

const KIND_COLORS: Record<string, string> = {
  Function: "#b180d7",
  Method: "#b180d7",
  Constructor: "#b180d7",
  Class: "#e2c08d",
  Struct: "#e2c08d",
  Interface: "#75beff",
  Trait: "#75beff",
  Enum: "#4ec9b0",
  TypeAlias: "#4ec9b0",
  Variable: "#9cdcfe",
  Constant: "#4fc1ff",
};

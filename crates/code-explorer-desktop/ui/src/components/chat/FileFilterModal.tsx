/**
 * FileFilterModal — Ctrl+P style file picker for filtering chat context.
 *
 * Features:
 * - Fuzzy search across all indexed files
 * - Shows file language and symbol count
 * - Keyboard navigation (arrow keys + Enter)
 * - Inspired by VS Code's Quick Open
 */

import { useState, useEffect, useRef, useCallback } from "react";
import { useQuery } from "@tanstack/react-query";
import { FileCode, Search, X, Check, Loader2 } from "lucide-react";
import { commands } from "../../lib/tauri-commands";
import { useI18n } from "../../hooks/use-i18n";
import type { FileQuickPick } from "../../lib/tauri-commands";
import { useChatStore } from "../../stores/chat-store";
import { useAppStore } from "../../stores/app-store";

interface FileFilterModalProps {
  open: boolean;
  onClose: () => void;
}

// Stable module-level empty array so our compare-in-render reset
// (see `prevResults` below) doesn't see a new reference each render.
const EMPTY_RESULTS: FileQuickPick[] = [];

export function FileFilterModal({ open, onClose }: FileFilterModalProps) {
  const { t } = useI18n();
  const activeRepo = useAppStore((s) => s.activeRepo);
  const [query, setQuery] = useState("");
  const [selectedIndex, setSelectedIndex] = useState(0);
  const inputRef = useRef<HTMLInputElement>(null);
  const listRef = useRef<HTMLDivElement>(null);

  const { filters, addFileFilter, removeFileFilter } = useChatStore();

  // Fetch files matching the query.
  // Scope by `activeRepo`: two different repos may share the same query
  // string (e.g. empty) and would otherwise return cached picks from the
  // previously active repo.
  const { data, isLoading } = useQuery({
    queryKey: ["chatPickFiles", activeRepo, query],
    queryFn: () => commands.chatPickFiles(query, 30),
    enabled: open,
    staleTime: 2000,
  });
  // Stable default: use a module-level empty array when `data` is undefined
  // to avoid creating a new reference each render (which would trip the
  // compare-in-render reset below into an infinite loop).
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

  // Reset selection on results change (render-time state adjustment).
  // We compare by reference — `results` is stable between renders because
  // TanStack Query returns the same array reference until the data actually
  // changes, and our EMPTY_RESULTS sentinel covers the undefined case.
  const [prevResults, setPrevResults] = useState(results);
  if (results !== prevResults) {
    setPrevResults(results);
    setSelectedIndex(0);
  }

  // Scroll selected item into view
  useEffect(() => {
    const list = listRef.current;
    if (!list) return;
    const item = list.children[selectedIndex] as HTMLElement | undefined;
    item?.scrollIntoView({ block: "nearest" });
  }, [selectedIndex]);

  const toggleFile = useCallback(
    (path: string, keepOpen = false) => {
      const already = filters.files.includes(path);
      if (already) {
        removeFileFilter(path);
      } else {
        addFileFilter(path);
      }
      // Close the modal after adding a filter so the user can get back to
      // the chat input immediately. Hold Alt (or keepOpen=true) to stay
      // open for multi-select. Removing a filter also keeps the modal
      // open — we're not in an "add & go" flow there.
      if (!already && !keepOpen) onClose();
    },
    [filters.files, addFileFilter, removeFileFilter, onClose]
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
      // Shift+Enter keeps the modal open for multi-select.
      toggleFile(results[selectedIndex].path, e.shiftKey);
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
      {/* Backdrop */}
      <div className="absolute inset-0" style={{ background: "rgba(0,0,0,0.4)" }} />

      {/* Modal */}
      <div
        className="relative w-[520px] max-h-[400px] rounded-xl shadow-2xl overflow-hidden flex flex-col"
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
            placeholder={t("filters.searchFiles")}
            className="flex-1 bg-transparent outline-none focus:ring-1 focus:ring-[var(--accent)] text-[13px]"
            style={{ color: "var(--text-0)", fontFamily: "var(--font-body)" }}
          />
          {isLoading && <Loader2 size={14} className="animate-spin" style={{ color: "var(--text-3)" }} />}
          <button onClick={onClose} className="p-0.5 rounded" style={{ color: "var(--text-3)" }} aria-label="Close">
            <X size={14} />
          </button>
        </div>

        {/* Results list */}
        <div ref={listRef} className="flex-1 overflow-y-auto py-1">
          {results.length === 0 && !isLoading && (
            <div className="px-4 py-8 text-center text-[13px]" style={{ color: "var(--text-3)" }}>
              {query ? t("filters.noFilesFound") : t("filters.typeToSearchFiles")}
            </div>
          )}

          {results.map((file, i) => (
            <FileItem
              key={file.path}
              file={file}
              isSelected={i === selectedIndex}
              isActive={filters.files.includes(file.path)}
              onClick={(e) => toggleFile(file.path, e.altKey || e.shiftKey)}
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
            {t("filter.filesSelected").replace("{0}", String(filters.files.length))}
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

// ─── FileItem ───────────────────────────────────────────────────────

function FileItem({
  file,
  isSelected,
  isActive,
  onClick,
  onMouseEnter,
}: {
  file: FileQuickPick;
  isSelected: boolean;
  isActive: boolean;
  onClick: (event: React.MouseEvent) => void;
  onMouseEnter: () => void;
}) {
  const langColor = LANG_COLORS[file.language || ""] || "var(--text-3)";

  return (
    <button
      onClick={onClick}
      onMouseEnter={onMouseEnter}
      className="w-full flex items-center gap-2 px-3 py-1.5 text-left transition-colors"
      style={{
        background: isSelected ? "var(--surface)" : "transparent",
      }}
    >
      {/* Active indicator */}
      <div className="w-4 h-4 flex items-center justify-center flex-shrink-0">
        {isActive ? (
          <Check size={12} style={{ color: "var(--accent)" }} />
        ) : (
          <FileCode size={12} style={{ color: langColor }} />
        )}
      </div>

      {/* Filename */}
      <span
        className="font-medium text-[13px] truncate"
        style={{
          color: isActive ? "var(--accent)" : "var(--text-0)",
          fontFamily: "var(--font-mono)",
        }}
      >
        {file.name}
      </span>

      {/* Path */}
      <span
        className="text-[11px] truncate flex-1"
        style={{ color: "var(--text-3)" }}
      >
        {file.path}
      </span>

      {/* Language badge */}
      {file.language && (
        <span
          className="text-[10px] px-1.5 py-0.5 rounded flex-shrink-0"
          style={{
            background: `color-mix(in srgb, ${langColor} 10%, transparent)`,
            color: langColor,
          }}
        >
          {file.language}
        </span>
      )}

      {/* Symbol count */}
      <span
        className="text-[10px] flex-shrink-0"
        style={{ color: "var(--text-3)" }}
      >
        {file.symbolCount} sym
      </span>
    </button>
  );
}

// ─── Language Colors ────────────────────────────────────────────────

const LANG_COLORS: Record<string, string> = {
  rs: "#dea584",
  ts: "#3178c6",
  tsx: "#3178c6",
  js: "#f7df1e",
  jsx: "#f7df1e",
  py: "#3776ab",
  java: "#b07219",
  cs: "#178600",
  go: "#00add8",
  rb: "#cc342d",
  php: "#4f5d95",
  kt: "#a97bff",
  swift: "#f05138",
  c: "#555555",
  h: "#555555",
  cpp: "#f34b7d",
  hpp: "#f34b7d",
  cshtml: "#512bd4",
  razor: "#512bd4",
};

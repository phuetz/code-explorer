/**
 * ModuleFilterModal — Module/community picker for filtering chat context.
 *
 * Displays detected communities with member counts and descriptions.
 */

import { useState, useEffect, useRef, useCallback } from "react";
import { useQuery } from "@tanstack/react-query";
import { Search, X, Check, Loader2, FolderTree } from "lucide-react";
import { commands } from "../../lib/tauri-commands";
import type { ModuleQuickPick } from "../../lib/tauri-commands";
import { useChatStore } from "../../stores/chat-store";
import { useAppStore } from "../../stores/app-store";
import { useI18n } from "../../hooks/use-i18n";

interface ModuleFilterModalProps {
  open: boolean;
  onClose: () => void;
}

// Stable module-level empty array so the compare-in-render reset below
// doesn't see a new reference each render.
const EMPTY_RESULTS: ModuleQuickPick[] = [];

export function ModuleFilterModal({ open, onClose }: ModuleFilterModalProps) {
  const { t } = useI18n();
  const activeRepo = useAppStore((s) => s.activeRepo);
  const [query, setQuery] = useState("");
  const [selectedIndex, setSelectedIndex] = useState(0);
  const inputRef = useRef<HTMLInputElement>(null);
  const listRef = useRef<HTMLDivElement>(null);

  const { filters, addModuleFilter, removeModuleFilter } = useChatStore();

  // Scope by `activeRepo` so switching repos doesn't return the previous
  // repo's cached module picks for the same query.
  const { data, isLoading } = useQuery({
    queryKey: ["chatPickModules", activeRepo, query],
    queryFn: () => commands.chatPickModules(query, 30),
    enabled: open,
    staleTime: 5000,
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

  const toggleModule = useCallback(
    (name: string, keepOpen = false) => {
      const already = filters.modules.includes(name);
      if (already) {
        removeModuleFilter(name);
      } else {
        addModuleFilter(name);
      }
      if (!already && !keepOpen) onClose();
    },
    [filters.modules, addModuleFilter, removeModuleFilter, onClose]
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
      toggleModule(results[selectedIndex].name, e.shiftKey);
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
        className="relative w-[500px] max-h-[380px] rounded-xl shadow-2xl overflow-hidden flex flex-col"
        style={{ background: "var(--bg-0)", border: "1px solid var(--surface-border)" }}
        onClick={(e) => e.stopPropagation()}
      >
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
            placeholder={t("filters.searchModules")}
            className="flex-1 bg-transparent outline-none focus:ring-1 focus:ring-[var(--accent)] text-[13px]"
            style={{ color: "var(--text-0)" }}
          />
          {isLoading && <Loader2 size={14} className="animate-spin" style={{ color: "var(--text-3)" }} />}
          <button onClick={onClose} className="p-0.5 rounded" style={{ color: "var(--text-3)" }} aria-label="Close">
            <X size={14} />
          </button>
        </div>

        <div ref={listRef} className="flex-1 overflow-y-auto py-1">
          {results.length === 0 && !isLoading && (
            <div className="px-4 py-8 text-center text-[13px]" style={{ color: "var(--text-3)" }}>
              {query ? t("filters.noModulesFound") : t("filters.loadingModules")}
            </div>
          )}

          {results.map((mod, i) => (
            <ModuleItem
              key={mod.communityId}
              module={mod}
              isSelected={i === selectedIndex}
              isActive={filters.modules.includes(mod.name)}
              onClick={(e) => toggleModule(mod.name, e.altKey || e.shiftKey)}
              onMouseEnter={() => setSelectedIndex(i)}
            />
          ))}
        </div>

        <div
          className="flex items-center justify-between px-3 py-1.5 text-[11px]"
          style={{ borderTop: "1px solid var(--surface-border)", color: "var(--text-3)" }}
        >
          <span>
            {t("filter.modulesSelected").replace("{0}", String(filters.modules.length))}
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

function ModuleItem({
  module,
  isSelected,
  isActive,
  onClick,
  onMouseEnter,
}: {
  module: ModuleQuickPick;
  isSelected: boolean;
  isActive: boolean;
  onClick: (event: React.MouseEvent) => void;
  onMouseEnter: () => void;
}) {
  return (
    <button
      onClick={onClick}
      onMouseEnter={onMouseEnter}
      className="w-full flex items-center gap-2 px-3 py-2 text-left transition-colors"
      style={{ background: isSelected ? "var(--surface)" : "transparent" }}
    >
      <div className="w-4 h-4 flex items-center justify-center flex-shrink-0">
        {isActive ? (
          <Check size={12} style={{ color: "var(--accent)" }} />
        ) : (
          <FolderTree size={12} style={{ color: "var(--green)" }} />
        )}
      </div>

      <div className="flex-1 min-w-0">
        <div className="flex items-center gap-2">
          <span
            className="font-medium text-[13px]"
            style={{ color: isActive ? "var(--accent)" : "var(--text-0)" }}
          >
            {module.name}
          </span>
          <span className="text-[10px]" style={{ color: "var(--text-3)" }}>
            {module.memberCount} members
          </span>
        </div>
        {module.description && (
          <div
            className="text-[11px] truncate mt-0.5"
            style={{ color: "var(--text-3)" }}
          >
            {module.description}
          </div>
        )}
      </div>
    </button>
  );
}

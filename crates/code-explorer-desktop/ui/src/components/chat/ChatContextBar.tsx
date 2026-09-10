/**
 * ChatContextBar — Displays active filters as pills/chips above the chat input.
 *
 * Shows file, symbol, module, and language filters with remove buttons.
 * Includes quick-add buttons to open filter modals (Ctrl+P, Ctrl+Shift+O, etc.)
 */

import { useMemo } from "react";
import {
  X,
  FileCode,
  Code2,
  FolderTree,
  Globe,
  Filter,
  Plus,
  Microscope,
} from "lucide-react";
import { useChatStore } from "../../stores/chat-store";
import { useI18n } from "../../hooks/use-i18n";

export function ChatContextBar() {
  const { t } = useI18n();
  const {
    filters,
    removeFileFilter,
    removeSymbolFilter,
    removeModuleFilter,
    removeLanguageFilter,
    clearFilters,
    hasActiveFilters,
    openModal,
    deepResearchEnabled,
    toggleDeepResearch,
  } = useChatStore();

  const active = hasActiveFilters();

  const filterCount = useMemo(
    () =>
      filters.files.length +
      filters.symbols.length +
      filters.modules.length +
      filters.languages.length,
    [filters]
  );

  return (
    <div
      className="flex-shrink-0 px-4 py-2"
      style={{ borderBottom: "1px solid var(--surface-border)" }}
    >
      {/* Quick-add buttons row */}
      <div className="flex items-center gap-1.5 mb-1.5">
        <button
          onClick={() => openModal("files")}
          className="flex items-center gap-1 px-2 py-1 rounded-md text-[11px] transition-colors"
          style={{
            background: "var(--surface)",
            color: "var(--text-2)",
            border: "1px solid var(--surface-border)",
          }}
          aria-label="Filter by file (Ctrl+P)"
        >
          <FileCode size={11} />
          <span>File</span>
          <Plus size={9} style={{ opacity: 0.5 }} />
        </button>

        <button
          onClick={() => openModal("symbols")}
          className="flex items-center gap-1 px-2 py-1 rounded-md text-[11px] transition-colors"
          style={{
            background: "var(--surface)",
            color: "var(--text-2)",
            border: "1px solid var(--surface-border)",
          }}
          aria-label="Filter by symbol (Ctrl+Shift+O)"
        >
          <Code2 size={11} />
          <span>Symbol</span>
          <Plus size={9} style={{ opacity: 0.5 }} />
        </button>

        <button
          onClick={() => openModal("modules")}
          className="flex items-center gap-1 px-2 py-1 rounded-md text-[11px] transition-colors"
          style={{
            background: "var(--surface)",
            color: "var(--text-2)",
            border: "1px solid var(--surface-border)",
          }}
          aria-label="Filter by module"
        >
          <FolderTree size={11} />
          <span>Module</span>
          <Plus size={9} style={{ opacity: 0.5 }} />
        </button>

        <div className="flex-1" />

        {/* Deep Research toggle */}
        <button
          onClick={toggleDeepResearch}
          className="flex items-center gap-1 px-2 py-1 rounded-md text-[11px] transition-all"
          style={{
            background: deepResearchEnabled ? "var(--purple-subtle)" : "var(--surface)",
            color: deepResearchEnabled ? "var(--purple)" : "var(--text-3)",
            border: `1px solid ${deepResearchEnabled ? "var(--purple)" : "var(--surface-border)"}`,
          }}
          aria-label={t("chat.deepResearch.ariaLabel")}
        >
          <Microscope size={11} />
          <span>{t("chat.deepResearch.label")}</span>
        </button>

        {/* Clear all filters */}
        {active && (
          <button
            onClick={clearFilters}
            className="flex items-center gap-1 px-2 py-1 rounded-md text-[11px] transition-colors"
            style={{ color: "var(--text-3)" }}
            aria-label="Clear all filters"
          >
            <Filter size={11} />
            <span>{filterCount}</span>
            <X size={9} />
          </button>
        )}
      </div>

      {/* Active filter pills */}
      {active && (
        <div className="flex flex-wrap gap-1">
          {filters.files.map((f) => (
            <FilterPill
              key={`file-${f}`}
              icon={<FileCode size={10} />}
              label={f.split("/").pop() || f}
              title={f}
              color="var(--accent)"
              onRemove={() => removeFileFilter(f)}
            />
          ))}
          {filters.symbols.map((s) => (
            <FilterPill
              key={`sym-${s}`}
              icon={<Code2 size={10} />}
              label={s}
              color="var(--purple)"
              onRemove={() => removeSymbolFilter(s)}
            />
          ))}
          {filters.modules.map((m) => (
            <FilterPill
              key={`mod-${m}`}
              icon={<FolderTree size={10} />}
              label={m}
              color="var(--green)"
              onRemove={() => removeModuleFilter(m)}
            />
          ))}
          {filters.languages.map((l) => (
            <FilterPill
              key={`lang-${l}`}
              icon={<Globe size={10} />}
              label={l}
              color="var(--orange)"
              onRemove={() => removeLanguageFilter(l)}
            />
          ))}
        </div>
      )}
    </div>
  );
}

// ─── FilterPill ─────────────────────────────────────────────────────

interface FilterPillProps {
  icon: React.ReactNode;
  label: string;
  title?: string;
  color: string;
  onRemove: () => void;
}

function FilterPill({ icon, label, title, color, onRemove }: FilterPillProps) {
  return (
    <span
      className="inline-flex items-center gap-1 px-1.5 py-0.5 rounded text-[11px] font-medium"
      title={title || label}
      style={{
        background: `color-mix(in srgb, ${color} 10%, transparent)`,
        color,
        border: `1px solid color-mix(in srgb, ${color} 25%, transparent)`,
      }}
    >
      {icon}
      <span className="max-w-[120px] truncate">{label}</span>
      <button
        onClick={(e) => {
          e.stopPropagation();
          onRemove();
        }}
        className="ml-0.5 opacity-60 hover:opacity-100 transition-opacity"
      >
        <X size={9} />
      </button>
    </span>
  );
}

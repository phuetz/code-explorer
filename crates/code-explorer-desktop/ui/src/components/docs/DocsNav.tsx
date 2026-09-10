/**
 * DocsNav — hierarchical wiki navigation for the docs viewer.
 * Renders the _index.json structure as an expandable tree.
 */

import { useState, useMemo, useCallback } from "react";
import {
  Home,
  GitBranch,
  BookOpen,
  Layers,
  Box,
  ChevronRight,
  ChevronDown,
  RefreshCw,
  FileText,
  Search,
  X,
} from "lucide-react";
import { useI18n } from "../../hooks/use-i18n";

// ─── Types (mirror Rust DocIndex / DocPage) ─────────────────────────

export interface DocPage {
  id: string;
  title: string;
  path?: string;
  icon?: string;
  children?: DocPage[];
}

export interface DocStats {
  files: number;
  nodes: number;
  edges: number;
  modules: number;
}

export interface DocIndex {
  title: string;
  generatedAt: string;
  stats: DocStats;
  pages: DocPage[];
}

// ─── Icon mapping ───────────────────────────────────────────────────

const ICON_MAP: Record<string, React.ComponentType<{ size?: number }>> = {
  home: Home,
  "git-branch": GitBranch,
  "book-open": BookOpen,
  layers: Layers,
  box: Box,
  "file-text": FileText,
};

function PageIcon({ icon, size = 14 }: { icon?: string; size?: number }) {
  const IconComponent = icon ? ICON_MAP[icon] : FileText;
  const Comp = IconComponent || FileText;
  return <Comp size={size} />;
}

// ─── Props ──────────────────────────────────────────────────────────

interface DocsNavProps {
  index: DocIndex;
  activePath: string | null;
  onNavigate: (path: string) => void;
  onRegenerate: () => void;
  isRegenerating: boolean;
}

// ─── Component ──────────────────────────────────────────────────────

/** Recursively flatten all pages that match the search query. */
function filterPages(pages: DocPage[], query: string): DocPage[] {
  const q = query.toLowerCase();
  const result: DocPage[] = [];
  for (const page of pages) {
    const titleMatch = page.title.toLowerCase().includes(q);
    const childMatches = page.children ? filterPages(page.children, query) : [];
    if (titleMatch || childMatches.length > 0) {
      result.push({
        ...page,
        children: childMatches.length > 0 ? childMatches : page.children,
      });
    }
  }
  return result;
}

/** Highlight the first occurrence of `query` inside `text` with an accent span. */
function highlightMatch(text: string, query: string): React.ReactNode {
  if (!query) return text;
  const idx = text.toLowerCase().indexOf(query.toLowerCase());
  if (idx === -1) return text;
  return (
    <>
      {text.slice(0, idx)}
      <span style={{ background: "var(--accent-subtle)", color: "var(--accent)" }}>
        {text.slice(idx, idx + query.length)}
      </span>
      {text.slice(idx + query.length)}
    </>
  );
}

/** Count leaf pages (pages without children) in a filtered tree. */
function countFilteredPages(pages: DocPage[]): number {
  let count = 0;
  for (const page of pages) {
    if (page.children && page.children.length > 0) {
      count += countFilteredPages(page.children);
    } else {
      count++;
    }
  }
  return count;
}

export function DocsNav({ index, activePath, onNavigate, onRegenerate, isRegenerating }: DocsNavProps) {
  const { t } = useI18n();
  const [searchQuery, setSearchQuery] = useState("");

  const filteredPages = useMemo(() => {
    if (!searchQuery.trim()) return index.pages;
    return filterPages(index.pages, searchQuery.trim());
  }, [index.pages, searchQuery]);

  const clearSearch = useCallback(() => setSearchQuery(""), []);

  return (
    <div className="flex flex-col h-full">
      {/* Header */}
      <div className="px-4 pt-4 pb-3">
        <div className="flex items-center justify-between mb-1">
          <h3
            className="text-sm font-semibold truncate"
            style={{ fontFamily: "var(--font-display)", color: "var(--text-0)" }}
          >
            {index.title}
          </h3>
          <button
            onClick={onRegenerate}
            disabled={isRegenerating}
            className="p-1 rounded transition-colors"
            style={{ color: "var(--text-3)" }}
            title={t("docs.regenerateTitle")}
            aria-label={t("docs.regenerateTitle")}
          >
            <RefreshCw size={13} className={isRegenerating ? "animate-spin" : ""} />
          </button>
        </div>
        <div className="flex gap-3 text-[11px]" style={{ color: "var(--text-3)" }}>
          <span>{index.stats.files} {t("docs.statsFiles")}</span>
          <span>{index.stats.modules} {t("docs.statsModules")}</span>
        </div>
      </div>

      {/* Search */}
      <div className="px-3 pb-2">
        <div
          className="flex items-center gap-2 rounded-md"
          style={{
            padding: "5px 8px",
            background: "var(--surface-0)",
            border: "1px solid var(--surface-border)",
          }}
        >
          <Search size={13} style={{ color: "var(--text-3)", flexShrink: 0 }} />
          <input
            type="text"
            value={searchQuery}
            onChange={(e) => setSearchQuery(e.target.value)}
            placeholder={t("docs.searchPlaceholder")}
            className="flex-1 text-[12px] outline-none focus:ring-1 focus:ring-[var(--accent)] bg-transparent"
            style={{ color: "var(--text-1)", minWidth: 0 }}
            aria-label={t("docs.searchPlaceholder")}
          />
          {searchQuery && (
            <button
              onClick={clearSearch}
              className="rounded p-0.5 transition-colors"
              style={{ color: "var(--text-3)", background: "transparent", border: "none", cursor: "pointer" }}
              aria-label="Clear search"
            >
              <X size={12} />
            </button>
          )}
        </div>
        {searchQuery && (
          <div
            className="text-[11px] mt-1 px-1"
            style={{ color: "var(--text-3)" }}
          >
            {filteredPages.length === 0
              ? t("docs.noResults")
              : `${countFilteredPages(filteredPages)} page${countFilteredPages(filteredPages) === 1 ? "" : "s"} found`}
          </div>
        )}
      </div>

      {/* Separator */}
      <div className="mx-3 mb-1" style={{ borderBottom: "1px solid var(--surface-border)" }} />

      {/* Navigation tree */}
      <nav className="flex-1 overflow-y-auto px-2 pb-4">
        {filteredPages.length > 0 ? (
          filteredPages.map((page) => (
            <NavItem
              key={page.id}
              page={page}
              depth={0}
              activePath={activePath}
              onNavigate={onNavigate}
              searchQuery={searchQuery}
            />
          ))
        ) : (
          <div
            className="text-center text-[12px] py-6"
            style={{ color: "var(--text-3)" }}
          >
            {t("docs.noResults")}
          </div>
        )}
      </nav>
    </div>
  );
}

// ─── NavItem (recursive) ────────────────────────────────────────────

function NavItem({
  page,
  depth,
  activePath,
  onNavigate,
  searchQuery = "",
}: {
  page: DocPage;
  depth: number;
  activePath: string | null;
  onNavigate: (path: string) => void;
  searchQuery?: string;
}) {
  const hasChildren = page.children && page.children.length > 0;
  const [expanded, setExpanded] = useState(true); // default expanded

  const isActive = activePath === page.path;
  const isInActiveSubtree = useMemo(() => {
    if (!activePath || !hasChildren) return false;
    const containsActive = (pages: DocPage[]): boolean =>
      pages.some(
        (c) => c.path === activePath || (c.children ? containsActive(c.children) : false)
      );
    return containsActive(page.children!);
  }, [activePath, hasChildren, page.children]);

  const handleClick = () => {
    if (page.path) {
      onNavigate(page.path);
    }
    if (hasChildren) {
      setExpanded((prev) => !prev);
    }
  };

  return (
    <div>
      <button
        onClick={handleClick}
        className="w-full flex items-center gap-2 px-2 py-1.5 rounded-md text-left text-[13px] transition-colors group"
        style={{
          paddingLeft: `${8 + depth * 12}px`,
          background: isActive ? "var(--accent-subtle)" : "transparent",
          color: isActive ? "var(--accent)" : isInActiveSubtree ? "var(--text-1)" : "var(--text-2)",
        }}
      >
        {/* Expand/collapse icon for parents */}
        {hasChildren ? (
          <span className="flex-shrink-0 w-3.5">
            {expanded ? <ChevronDown size={13} /> : <ChevronRight size={13} />}
          </span>
        ) : (
          <span className="flex-shrink-0 w-3.5" />
        )}

        {/* Page icon */}
        <span className="flex-shrink-0 opacity-60">
          <PageIcon icon={page.icon} size={14} />
        </span>

        {/* Label */}
        <span className="truncate">
          {searchQuery ? highlightMatch(page.title, searchQuery) : page.title}
        </span>
      </button>

      {/* Children */}
      {hasChildren && expanded && (
        <div>
          {page.children!.map((child) => (
            <NavItem
              key={child.id}
              page={child}
              depth={depth + 1}
              activePath={activePath}
              onNavigate={onNavigate}
              searchQuery={searchQuery}
            />
          ))}
        </div>
      )}
    </div>
  );
}

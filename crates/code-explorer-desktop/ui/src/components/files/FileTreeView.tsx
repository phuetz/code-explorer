import { useState, useMemo, useEffect, useRef, useCallback } from "react";
import { ChevronRight, ChevronDown, File, Folder, Search, X } from "lucide-react";
import { useFileTree } from "../../hooks/use-tauri-query";
import { useAppStore } from "../../stores/app-store";
import { useI18n } from "../../hooks/use-i18n";
import type { FileTreeNode } from "../../lib/tauri-commands";

const LANGUAGE_COLORS: Record<string, string> = {
  js: "var(--cyan)",
  jsx: "var(--cyan)",
  ts: "var(--purple)",
  tsx: "var(--purple)",
  py: "var(--blue)",
  java: "var(--rose)",
  c: "var(--cyan)",
  cpp: "var(--purple)",
  cxx: "var(--purple)",
  cc: "var(--purple)",
  cs: "var(--green)",
  go: "var(--cyan)",
  rs: "var(--rose)",
  php: "var(--purple)",
  rb: "var(--rose)",
  kt: "var(--purple)",
  swift: "var(--rose)",
  json: "var(--amber)",
  yaml: "var(--cyan)",
  yml: "var(--cyan)",
  xml: "var(--amber)",
  html: "var(--rose)",
  css: "var(--cyan)",
  scss: "var(--cyan)",
  sass: "var(--cyan)",
};

function getFileColor(filename: string): string {
  const ext = filename.split(".").pop()?.toLowerCase() || "";
  return LANGUAGE_COLORS[ext] || "var(--text-3)";
}

function countFiles(nodes: FileTreeNode[]): number {
  let count = 0;
  for (const node of nodes) {
    if (!node.isDir) {
      count++;
    } else {
      count += countFiles(node.children);
    }
  }
  return count;
}

/** Recursively filter tree nodes by search query. A folder matches if its name matches or any descendant matches.
 *  Returns the filtered tree along with the count of *true file matches* (excluding
 *  unfiltered children of folders that were included only because their own name matched).
 */
function filterTree(
  nodes: FileTreeNode[],
  query: string
): { tree: FileTreeNode[]; matchCount: number } {
  const q = query.toLowerCase();
  const result: FileTreeNode[] = [];
  let matchCount = 0;
  for (const node of nodes) {
    const nameMatch = node.name.toLowerCase().includes(q);
    if (node.isDir) {
      const { tree: childMatches, matchCount: childCount } = filterTree(node.children, query);
      if (childMatches.length > 0) {
        // Folder kept because of descendant matches — keep the filtered children
        result.push({ ...node, children: childMatches });
        matchCount += childCount;
      } else if (nameMatch) {
        // Folder kept only because its own name matched — show its full subtree
        // for browsing, but do NOT count its descendants as "matches".
        result.push({ ...node, children: node.children });
      }
    } else if (nameMatch) {
      result.push(node);
      matchCount += 1;
    }
  }
  return { tree: result, matchCount };
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

function Breadcrumbs({ selectedNodeId }: { selectedNodeId: string | null }) {
  const setSelectedNodeId = useAppStore((s) => s.setSelectedNodeId);

  if (!selectedNodeId || !selectedNodeId.startsWith("File:")) return null;

  const filePath = selectedNodeId.slice(5);
  const segments = filePath.split(/[\\/]/);

  return (
    <div
      className="flex items-center gap-1 px-3 py-2 overflow-x-auto text-[11px]"
      style={{
        borderBottom: "1px solid var(--surface-border)",
        background: "var(--bg-1)",
        minHeight: 32,
      }}
    >
      {segments.map((segment, i) => {
        const isLast = i === segments.length - 1;
        const partialPath = segments.slice(0, i + 1).join("/");
        return (
          <span key={i} className="flex items-center gap-1 shrink-0">
            {i > 0 && (
              <ChevronRight
                size={10}
                style={{ color: "var(--text-4)", flexShrink: 0 }}
              />
            )}
            <button
              onClick={() => {
                if (!isLast) {
                  // Navigate to directory (no-op for now, but sets selection)
                  setSelectedNodeId(`File:${partialPath}`, segment);
                }
              }}
              className="hover-surface rounded px-1 py-0.5"
              style={{
                color: isLast ? "var(--text-0)" : "var(--text-3)",
                fontWeight: isLast ? 500 : 400,
                fontFamily: "var(--font-mono)",
                cursor: isLast ? "default" : "pointer",
              }}
            >
              {segment}
            </button>
          </span>
        );
      })}
    </div>
  );
}

export function FileTreeView() {
  const { t } = useI18n();
  const selectedNodeId = useAppStore((s) => s.selectedNodeId);
  const { data: tree, isLoading, error } = useFileTree(true);

  const [searchInput, setSearchInput] = useState("");
  const [searchQuery, setSearchQuery] = useState("");
  const searchInputRef = useRef<HTMLInputElement>(null);

  // Debounce search input by 150ms
  useEffect(() => {
    const timer = setTimeout(() => {
      setSearchQuery(searchInput.trim());
    }, 150);
    return () => clearTimeout(timer);
  }, [searchInput]);

  // Pressing "/" anywhere focuses the search input
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (
        e.key === "/" &&
        !["INPUT", "TEXTAREA", "SELECT"].includes((e.target as HTMLElement)?.tagName)
      ) {
        e.preventDefault();
        searchInputRef.current?.focus();
      }
    };
    document.addEventListener("keydown", handleKeyDown);
    return () => document.removeEventListener("keydown", handleKeyDown);
  }, []);

  const clearSearch = useCallback(() => {
    setSearchInput("");
    setSearchQuery("");
  }, []);

  const { filteredTree, filteredFileCount } = useMemo(() => {
    if (!tree || !searchQuery) {
      return { filteredTree: tree ?? [], filteredFileCount: null as number | null };
    }
    const { tree: filtered, matchCount } = filterTree(tree, searchQuery);
    return { filteredTree: filtered, filteredFileCount: matchCount };
  }, [tree, searchQuery]);

  if (isLoading) {
    return (
      <div
        className="h-full flex items-center justify-center shimmer"
        style={{ color: "var(--text-3)" }}
      >
        <div className="space-y-2 w-full px-4">
          <div
            className="h-4 rounded"
            style={{ backgroundColor: "var(--bg-2)", animation: "shimmer 2s infinite" }}
          />
          <div
            className="h-4 rounded"
            style={{ backgroundColor: "var(--bg-2)", animation: "shimmer 2s infinite" }}
          />
          <div
            className="h-4 rounded"
            style={{ backgroundColor: "var(--bg-2)", animation: "shimmer 2s infinite" }}
          />
        </div>
      </div>
    );
  }

  if (error) {
    return (
      <div
        className="h-full flex items-center justify-center p-4 text-center"
        style={{ color: "var(--rose)" }}
      >
        {t("files.errorLoadingTree")}
      </div>
    );
  }

  if (!tree || tree.length === 0) {
    return (
      <div
        className="h-full flex items-center justify-center p-4 text-center"
        style={{ color: "var(--text-3)" }}
      >
        {t("files.noFilesFound")}
      </div>
    );
  }

  const fileCount = countFiles(tree);

  return (
    <div
      className="h-full flex flex-col"
      style={{ backgroundColor: "var(--bg-0)" }}
    >
      {/* Header */}
      <div
        className="px-3 py-4 border-b flex items-center justify-between"
        style={{
          backgroundColor: "var(--bg-1)",
          borderColor: "var(--surface-border)",
        }}
      >
        <div className="flex items-center gap-2">
          <Folder size={16} style={{ color: "var(--amber)" }} />
          <h2
            className="text-sm font-semibold"
            style={{ color: "var(--text-0)" }}
          >
            {t("files.title")}
          </h2>
        </div>
        <span
          className="text-xs px-2 py-1 rounded"
          style={{
            backgroundColor: "var(--bg-2)",
            color: "var(--text-2)",
          }}
        >
          {fileCount}
        </span>
      </div>

      {/* Search input */}
      <div
        className="px-3 py-2"
        style={{ borderBottom: "1px solid var(--surface-border)", background: "var(--bg-1)" }}
      >
        <div
          className="flex items-center gap-2 rounded-md"
          style={{
            padding: "5px 8px",
            background: "var(--bg-2)",
            border: "1px solid var(--surface-border)",
          }}
        >
          <Search size={13} style={{ color: "var(--text-3)", flexShrink: 0 }} />
          <input
            ref={searchInputRef}
            type="text"
            value={searchInput}
            onChange={(e) => setSearchInput(e.target.value)}
            placeholder={t("files.searchPlaceholder")}
            className="flex-1 text-[12px] outline-none focus:ring-1 focus:ring-[var(--accent)] bg-transparent"
            style={{ color: "var(--text-1)", minWidth: 0 }}
            aria-label={t("files.searchFiles")}
          />
          {searchInput && (
            <button
              onClick={clearSearch}
              className="rounded p-0.5 transition-colors"
              style={{ color: "var(--text-3)", background: "transparent", border: "none", cursor: "pointer" }}
              aria-label={t("files.clearSearch")}
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
            {filteredFileCount === 0
              ? t("files.noMatchingFiles")
              : t("files.matchingFiles").replace("{0}", String(filteredFileCount))}
          </div>
        )}
      </div>

      {/* Breadcrumb */}
      <Breadcrumbs selectedNodeId={selectedNodeId} />

      {/* Tree container */}
      <div
        className="flex-1 overflow-y-auto px-3 py-4"
        style={{ backgroundColor: "var(--bg-0)" }}
      >
        <div role="tree" aria-label="File explorer" style={{ display: "flex", flexDirection: "column", gap: "0px" }}>
          {filteredTree.map((node) => (
            <TreeNode
              key={node.path}
              node={node}
              depth={0}
              parentPath=""
              searchQuery={searchQuery}
            />
          ))}
        </div>
      </div>
    </div>
  );
}

interface TreeNodeProps {
  node: FileTreeNode;
  depth: number;
  parentPath: string;
  searchQuery?: string;
}

function TreeNode({ node, depth, parentPath, searchQuery = "" }: TreeNodeProps) {
  // User-toggled expansion state; default expand first level
  const [userExpanded, setUserExpanded] = useState(depth < 1);
  // When searching, force-expand all directories; otherwise use user-toggled state
  const expanded = searchQuery && node.isDir ? true : userExpanded;
  const setExpanded = setUserExpanded;
  const selectedNodeId = useAppStore((s) => s.selectedNodeId);
  const setSelectedNodeId = useAppStore((s) => s.setSelectedNodeId);
  const setDetailTab = useAppStore((s) => s.setDetailTab);

  const fullPath = parentPath ? `${parentPath}/${node.name}` : node.name;
  const isSelected = selectedNodeId === `File:${fullPath}`;

  const handleClick = () => {
    if (node.isDir) {
      setExpanded(!expanded);
    } else {
      setSelectedNodeId(`File:${fullPath}`, node.name);
      setDetailTab("code");
    }
  };

  return (
    <div>
      <button
        role="treeitem"
        aria-expanded={node.isDir ? expanded : undefined}
        aria-selected={isSelected}
        onClick={handleClick}
        className="flex items-center gap-2 w-full rounded text-left text-[13px] transition-colors relative group"
        style={{
          paddingLeft: `${depth * 16 + 12}px`,
          paddingRight: "8px",
          paddingTop: "6px",
          paddingBottom: "6px",
          backgroundColor: isSelected
            ? "var(--accent-subtle)"
            : "transparent",
          color: isSelected ? "var(--accent)" : "var(--text-1)",
        }}
      >
        {/* Indentation guide lines */}
        {depth > 0 && (
          <div
            style={{
              position: "absolute",
              left: `${(depth - 1) * 16 + 20}px`,
              top: "0",
              bottom: "0",
              width: "1px",
              borderLeft: "1px dotted",
              borderColor: "var(--surface-border)",
              opacity: "0.5",
            }}
          />
        )}

        {/* Chevron or spacer */}
        {node.isDir ? (
          expanded ? (
            <ChevronDown
              size={14}
              className="shrink-0"
              style={{ color: "var(--text-2)" }}
            />
          ) : (
            <ChevronRight
              size={14}
              className="shrink-0"
              style={{ color: "var(--text-2)" }}
            />
          )
        ) : (
          <span style={{ width: "14px", height: "14px", flexShrink: 0 }} />
        )}

        {/* Icon */}
        {node.isDir ? (
          <Folder
            size={14}
            className="shrink-0"
            style={{ color: "var(--amber)" }}
          />
        ) : (
          <File
            size={14}
            className="shrink-0"
            style={{ color: getFileColor(node.name) }}
          />
        )}

        {/* Name */}
        <span className="truncate flex-1">
          {searchQuery ? highlightMatch(node.name, searchQuery) : node.name}
        </span>

        {/* Hover background indicator */}
        <div
          className="absolute inset-0 rounded pointer-events-none group-hover:opacity-100"
          style={{
            backgroundColor: "var(--surface-hover)",
            opacity: "0",
            transition: "opacity 0.15s ease-in-out",
            zIndex: -1,
          }}
        />
      </button>

      {/* Children */}
      {node.isDir && expanded && (
        <div role="group">
          {node.children.map((child) => (
            <TreeNode
              key={child.path}
              node={child}
              depth={depth + 1}
              parentPath={fullPath}
              searchQuery={searchQuery}
            />
          ))}
        </div>
      )}
    </div>
  );
}

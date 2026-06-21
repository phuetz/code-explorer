import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { useAppStore, type DetailTab } from "../../stores/app-store";
import { useSymbolContext } from "../../hooks/use-tauri-query";
import { useI18n } from "../../hooks/use-i18n";
import { CodePanel } from "../code/CodePanel";
import { LayersTab } from "../detail/LayersTab";
import { CodeHealthCard } from "../health/CodeHealthCard";
import { SymbolBreadcrumb } from "../shared/Breadcrumb";
import { BookmarkStar } from "./BookmarkStar";
import { CommentsThread } from "./CommentsThread";
import { SkeletonLine } from "../shared/motion";
import { commands } from "../../lib/tauri-commands";
import { ChevronDown, Code2 } from "lucide-react";
import { motion, AnimatePresence } from "framer-motion";

const TABS: { id: DetailTab; i18nKey: string }[] = [
  { id: "context", i18nKey: "detail.context" },
  { id: "code", i18nKey: "detail.code" },
  { id: "properties", i18nKey: "detail.codeProperties" },
  { id: "layers", i18nKey: "detail.layers" },
  { id: "health", i18nKey: "detail.health" },
];

const NODE_TYPE_COLORS: Record<string, string> = {
  Function: "var(--cyan)",
  Class: "var(--amber)",
  Method: "var(--cyan)",
  Property: "var(--purple)",
  Variable: "var(--text-2)",
  Interface: "var(--green)",
  Import: "var(--rose)",
  Export: "var(--green)",
  Enum: "var(--amber)",
  Struct: "var(--purple)",
  Constant: "var(--purple)",
  Module: "var(--cyan)",
};

function getNodeTypeColor(label: string): string {
  return NODE_TYPE_COLORS[label] || "var(--accent)";
}

export function DetailPanel() {
  const { t } = useI18n();
  const selectedNodeId = useAppStore((s) => s.selectedNodeId);
  const detailTab = useAppStore((s) => s.detailTab);
  const setDetailTab = useAppStore((s) => s.setDetailTab);

  if (!selectedNodeId) {
    return (
      <div
        className="h-full flex flex-col items-center justify-center p-6 text-center"
        style={{ backgroundColor: "var(--glass-bg)", backdropFilter: "blur(12px)", borderLeft: "1px solid var(--surface-border)" }}
      >
        <div
          className="w-12 h-12 rounded-lg mb-3 flex items-center justify-center"
          style={{ backgroundColor: "var(--bg-2)" }}
        >
          <span style={{ fontSize: "24px" }}>○</span>
        </div>
        <p
          className="text-sm font-medium"
          style={{ color: "var(--text-1)" }}
        >
          {t("detail.noSelection")}
        </p>
        <p
          className="text-xs mt-1"
          style={{ color: "var(--text-3)" }}
        >
          {t("detail.emptyHint")}
        </p>
      </div>
    );
  }

  return (
    <div
      className="h-full flex flex-col"
      style={{ backgroundColor: "var(--glass-bg)", backdropFilter: "blur(12px)", borderLeft: "1px solid var(--surface-border)" }}
    >
      {/* Breadcrumb + bookmark star */}
      <div
        className="px-4 pt-2 pb-0 flex items-center gap-2"
        style={{ background: "var(--bg-1)" }}
      >
        <div className="flex-1 min-w-0">
          <SymbolBreadcrumb />
        </div>
        <BookmarkStar />
      </div>

      {/* Tab bar */}
      <div
        className="flex gap-1 px-4 py-3 border-b"
        role="tablist"
        style={{
          backgroundColor: "var(--bg-1)",
          borderColor: "var(--surface-border)",
        }}
      >
        {TABS.map(({ id, i18nKey }) => (
          <button
            key={id}
            id={`detail-tab-${id}`}
            role="tab"
            aria-selected={detailTab === id}
            aria-controls="detail-tabpanel"
            onClick={() => setDetailTab(id)}
            className="px-3 py-1.5 text-xs font-medium rounded transition-all"
            style={{
              backgroundColor:
                detailTab === id ? "var(--accent)" : "var(--bg-2)",
              color:
                detailTab === id ? "white" : "var(--text-2)",
            }}
          >
            {t(i18nKey)}
          </button>
        ))}
      </div>

      {/* Tab content */}
      <div className="flex-1 min-h-0 overflow-auto" role="tabpanel" id="detail-tabpanel" aria-labelledby={`detail-tab-${detailTab}`}>
        {detailTab === "context" && <ContextTab />}
        {detailTab === "code" && <CodePanel />}
        {detailTab === "properties" && <PropertiesTab />}
        {detailTab === "layers" && <LayersTab />}
        {detailTab === "health" && (
          <div className="p-4 overflow-auto" style={{ backgroundColor: "var(--bg-0)" }}>
            <CodeHealthCard />
          </div>
        )}
        {/* Persistent notes / annotations for this node */}
        <CommentsThread />
      </div>
    </div>
  );
}

interface CollapsibleSectionProps {
  title: string;
  count: number;
  defaultExpanded?: boolean;
  children: React.ReactNode;
}

function CollapsibleSection({
  title,
  count,
  defaultExpanded = false,
  children,
}: CollapsibleSectionProps) {
  const [isExpanded, setIsExpanded] = useState(defaultExpanded);

  return (
    <div>
      <button
        onClick={() => setIsExpanded(!isExpanded)}
        className="flex items-center gap-2 w-full text-left mb-2 hover:opacity-80 transition-opacity"
        style={{ padding: "0" }}
      >
        <motion.span
          animate={{ rotate: isExpanded ? 0 : -90 }}
          transition={{ duration: 0.2, ease: "easeOut" }}
          style={{ display: "inline-flex", flexShrink: 0 }}
        >
          <ChevronDown size={16} style={{ color: "var(--text-2)" }} />
        </motion.span>
        <h3
          className="text-xs font-semibold uppercase tracking-wider"
          style={{ color: "var(--text-2)" }}
        >
          {title}{" "}
          <span style={{ color: "var(--text-3)" }}>({count})</span>
        </h3>
      </button>
      <AnimatePresence initial={false}>
        {isExpanded && (
          <motion.div
            initial={{ height: 0, opacity: 0 }}
            animate={{ height: "auto", opacity: 1 }}
            exit={{ height: 0, opacity: 0 }}
            transition={{ duration: 0.2, ease: "easeOut" }}
            style={{ overflow: "hidden" }}
          >
            {children}
          </motion.div>
        )}
      </AnimatePresence>
    </div>
  );
}

function ContextTab() {
  const { t } = useI18n();
  const selectedNodeId = useAppStore((s) => s.selectedNodeId);
  const setSelectedNodeId = useAppStore((s) => s.setSelectedNodeId);
  const { data: context, isLoading } = useSymbolContext(selectedNodeId);

  if (isLoading) {
    return (
      <div
        className="h-full flex items-center justify-center"
        style={{ color: "var(--text-3)" }}
      >
        {t("detail.loadingContext")}
      </div>
    );
  }

  if (!context) return null;

  return (
    <div
      className="h-full overflow-y-auto p-4 space-y-4"
      style={{ backgroundColor: "var(--bg-0)" }}
    >
      {/* Node header card */}
      <div
        className="rounded-lg p-3 border-l-4"
        style={{
          backgroundColor: "var(--bg-1)",
          borderColor: getNodeTypeColor(context.node.label),
        }}
      >
        <div className="flex items-center gap-2 mb-2 flex-wrap">
          <span
            className="px-2 py-1 rounded text-[11px] font-medium text-white whitespace-nowrap"
            style={{
              backgroundColor: getNodeTypeColor(context.node.label),
            }}
          >
            {context.node.label}
          </span>
          {context.node.isExported && (
            <span
              className="px-2 py-1 rounded text-[11px] font-medium text-white whitespace-nowrap"
              style={{ backgroundColor: "var(--green)" }}
            >
              {t("detail.exported")}
            </span>
          )}
          {context.node.layerType && (
            <span
              className="px-2 py-1 rounded text-[11px] font-medium whitespace-nowrap"
              style={{ backgroundColor: "var(--bg-3)", color: "var(--cyan)" }}
            >
              {context.node.layerType}
            </span>
          )}
          {context.node.entryPointScore != null && context.node.entryPointScore > 0 && (
            <span
              className="px-2 py-1 rounded text-[11px] font-medium whitespace-nowrap"
              style={{ backgroundColor: "var(--bg-3)", color: "var(--amber)" }}
            >
              {t("detail.entryPoint")}
            </span>
          )}
          {context.node.isTraced && (
            <span
              className="px-2 py-1 rounded text-[11px] font-medium whitespace-nowrap"
              style={{ backgroundColor: "var(--bg-3)", color: "var(--green)" }}
            >
              {t("detail.traced")}
            </span>
          )}
        </div>
        <h2
          className="text-base font-semibold mb-1"
          style={{ color: "var(--text-0)" }}
        >
          {context.node.name}
        </h2>
        <p
          className="text-xs"
          style={{ color: "var(--text-3)" }}
        >
          {context.node.filePath}
          {context.node.startLine && `:${context.node.startLine}`}
          {context.node.endLine && `-${context.node.endLine}`}
        </p>
      </div>

      {/* Inline code snippet */}
      {context.node.filePath && context.node.startLine && (
        <InlineCodeSnippet
          filePath={context.node.filePath}
          startLine={context.node.startLine}
          endLine={context.node.endLine}
        />
      )}

      {/* Description */}
      {context.node.description && (
        <div
          className="rounded-lg p-3"
          style={{
            backgroundColor: "var(--bg-1)",
            borderLeft: "3px solid var(--accent)",
          }}
        >
          <p
            className="text-xs"
            style={{ color: "var(--text-1)", lineHeight: 1.6 }}
          >
            {context.node.description}
          </p>
        </div>
      )}

      {/* Cyclomatic complexity */}
      {context.node.complexity != null && context.node.complexity > 1 && (
        <div className="rounded-lg p-3" style={{ backgroundColor: "var(--bg-1)" }}>
          <h3 className="text-xs font-semibold uppercase tracking-wider mb-2" style={{ color: "var(--text-2)" }}>
            {t("detail.cyclomaticComplexity")}
          </h3>
          <div className="flex items-center gap-2">
            <span className="text-2xl font-bold" style={{
              color: context.node.complexity > 20 ? "#ef4444" : context.node.complexity > 10 ? "#f59e0b" : "#22c55e"
            }}>
              {context.node.complexity}
            </span>
            <span className="text-xs" style={{ color: "var(--text-3)" }}>
              {context.node.complexity > 20 ? "Very complex — consider refactoring" :
               context.node.complexity > 10 ? "Complex — review for simplification" :
               "Acceptable complexity"}
            </span>
          </div>
        </div>
      )}

      {/* Dead code warning */}
      {context.node.isDeadCandidate && (
        <div className="rounded-lg p-3" style={{ backgroundColor: "var(--bg-1)", borderLeft: "3px solid #ef4444" }}>
          <p className="text-xs font-medium" style={{ color: "#ef4444" }}>
            ⚠ Dead Code Candidate
          </p>
          <p className="text-xs mt-1" style={{ color: "var(--text-3)" }}>
            No incoming calls detected. This method may be unused.
          </p>
        </div>
      )}

      {/* Architecture layer */}
      {context.node.layerType && (
        <div className="rounded-lg p-3" style={{ backgroundColor: "var(--bg-1)" }}>
          <h3 className="text-xs font-semibold uppercase tracking-wider mb-1" style={{ color: "var(--text-2)" }}>
            {t("detail.architectureLayer")}
          </h3>
          <span className="text-xs px-2 py-1 rounded-full font-medium" style={{
            backgroundColor: context.node.layerType === "Presentation" ? "#a855f720" :
                             context.node.layerType === "Business" ? "#06b6d420" : "#22c55e20",
            color: context.node.layerType === "Presentation" ? "#a855f7" :
                   context.node.layerType === "Business" ? "#06b6d4" : "#22c55e",
          }}>
            {context.node.layerType}
          </span>
        </div>
      )}

      {context.callers.length > 0 && (
        <CollapsibleSection title={t("detail.callers")} count={context.callers.length} defaultExpanded>
          <RelationSection title={t("detail.callers")} items={context.callers} onSelect={setSelectedNodeId} />
        </CollapsibleSection>
      )}
      {context.callees.length > 0 && (
        <CollapsibleSection title={t("detail.callees")} count={context.callees.length} defaultExpanded>
          <RelationSection title={t("detail.callees")} items={context.callees} onSelect={setSelectedNodeId} />
        </CollapsibleSection>
      )}
      {context.imports.length > 0 && (
        <CollapsibleSection title={t("detail.imports")} count={context.imports.length}>
          <RelationSection title={t("detail.imports")} items={context.imports} onSelect={setSelectedNodeId} />
        </CollapsibleSection>
      )}
      {context.importedBy.length > 0 && (
        <CollapsibleSection title={t("detail.importedBy")} count={context.importedBy.length}>
          <RelationSection title={t("detail.importedBy")} items={context.importedBy} onSelect={setSelectedNodeId} />
        </CollapsibleSection>
      )}
      {context.inherits.length > 0 && (
        <CollapsibleSection title={t("detail.inherits")} count={context.inherits.length}>
          <RelationSection title={t("detail.inherits")} items={context.inherits} onSelect={setSelectedNodeId} />
        </CollapsibleSection>
      )}
      {context.inheritedBy.length > 0 && (
        <CollapsibleSection title={t("detail.inheritedBy")} count={context.inheritedBy.length}>
          <RelationSection title={t("detail.inheritedBy")} items={context.inheritedBy} onSelect={setSelectedNodeId} />
        </CollapsibleSection>
      )}

      {context.community && (
        <CollapsibleSection title={t("detail.community")} count={1}>
          <div
            className="rounded-lg p-3 overflow-hidden"
            style={{
              backgroundColor: "var(--bg-1)",
              borderLeft: "4px solid",
              borderColor: "var(--purple)",
            }}
          >
            <p
              className="font-medium text-sm"
              style={{ color: "var(--text-0)" }}
            >
              {context.community.name}
            </p>
            {context.community.description && (
              <p
                className="text-xs mt-1"
                style={{ color: "var(--text-3)" }}
              >
                {context.community.description}
              </p>
            )}
            <div
              className="flex gap-3 mt-2 text-xs"
              style={{ color: "var(--text-2)" }}
            >
              {context.community.memberCount != null && (
                <span>{context.community.memberCount} {t("detail.members")}</span>
              )}
              {context.community.cohesion != null && (
                <span>{t("detail.cohesion")}: {context.community.cohesion.toFixed(2)}</span>
              )}
            </div>
          </div>
        </CollapsibleSection>
      )}
    </div>
  );
}

function PropertiesTab() {
  const selectedNodeId = useAppStore((s) => s.selectedNodeId);
  const { data: context, isLoading } = useSymbolContext(selectedNodeId);

  if (isLoading) {
    return (
      <div className="h-full overflow-y-auto p-4" style={{ backgroundColor: "var(--bg-0)" }}>
        <div className="grid gap-2">
          {Array.from({ length: 6 }, (_, i) => (
            <div key={i} className="rounded-lg p-3 border" style={{ backgroundColor: "var(--bg-1)", borderColor: "var(--surface-border)" }}>
              <SkeletonLine width="60px" height="10px" className="mb-2" />
              <SkeletonLine width={`${70 + (i % 3) * 15}%`} height="12px" />
            </div>
          ))}
        </div>
      </div>
    );
  }
  if (!context) return null;

  const node = context.node;
  const props: [string, string | undefined][] = [
    ["ID", node.id],
    ["Label", node.label],
    ["Name", node.name],
    ["File", node.filePath],
    ["Lines", node.startLine ? `${node.startLine}${node.endLine ? `-${node.endLine}` : ""}` : undefined],
    ["Language", node.language ?? undefined],
    ["Exported", node.isExported != null ? String(node.isExported) : undefined],
    ["Parameters", node.parameterCount != null ? String(node.parameterCount) : undefined],
    ["Return Type", node.returnType ?? undefined],
    ["Community", node.community ?? undefined],
    ["Description", node.description ?? undefined],
  ];

  const visibleProps = props.filter(([, v]) => v !== undefined);

  return (
    <div
      className="h-full overflow-y-auto p-4"
      style={{ backgroundColor: "var(--bg-0)" }}
    >
      <div className="grid gap-2">
        {visibleProps.map(([key, value]) => (
          <div
            key={key}
            className="rounded-lg p-3 border"
            style={{
              backgroundColor: "var(--bg-1)",
              borderColor: "var(--surface-border)",
            }}
          >
            <p
              className="text-xs font-medium mb-1"
              style={{ color: "var(--text-2)" }}
            >
              {key}
            </p>
            <p
              className="text-xs break-all"
              style={{ color: "var(--text-0)" }}
            >
              {value}
            </p>
          </div>
        ))}
      </div>
    </div>
  );
}

interface RelationItem {
  id: string;
  name: string;
  label: string;
  filePath: string;
}

interface RelationSectionProps {
  title: string;
  items: RelationItem[];
  onSelect: (id: string, name?: string) => void;
}

function RelationSection({
  title,
  items,
  onSelect,
}: RelationSectionProps) {
  return (
    <div className="space-y-1.5 max-h-64 overflow-y-auto">
        {title && (
          <p className="text-[11px] font-medium px-1" style={{ color: "var(--text-3)" }}>{title}</p>
        )}
        {items.map((item) => (
          <button
            key={item.id}
            onClick={() => onSelect(item.id, item.name)}
            className="w-full flex items-start gap-2 px-3 py-2 rounded-lg border transition-colors text-left hover:brightness-110"
            style={{
              backgroundColor: "var(--bg-1)",
              borderColor: "var(--surface-border)",
            }}
          >
            <span
              className="text-[10px] px-2 py-0.5 rounded shrink-0 font-medium whitespace-nowrap"
              style={{
                backgroundColor: "var(--bg-2)",
                color: "var(--text-2)",
              }}
            >
              {item.label}
            </span>
            <div className="flex-1 min-w-0 overflow-hidden">
              <p
                className="text-xs font-medium truncate"
                style={{ color: "var(--text-0)" }}
              >
                {item.name}
              </p>
              <p
                className="text-[10px] truncate"
                style={{ color: "var(--text-3)" }}
              >
                {item.filePath}
              </p>
            </div>
          </button>
        ))}
    </div>
  );
}

/** Collapsible inline code snippet showing the first N lines of a symbol */
function InlineCodeSnippet({
  filePath,
  startLine,
  endLine,
}: {
  filePath: string;
  startLine: number;
  endLine?: number;
}) {
  const { t } = useI18n();
  const activeRepo = useAppStore((s) => s.activeRepo);
  const [expanded, setExpanded] = useState(false);
  const maxPreviewLines = 8;
  const snippetEnd = endLine ?? startLine + maxPreviewLines;
  const previewEnd = Math.min(startLine + maxPreviewLines, snippetEnd);

  // Scope by `activeRepo` so two repos that share a relative `filePath`
  // (very common: `src/main.rs`, `package.json`, ...) don't return the
  // previously-opened repo's cached file contents.
  const { data: content } = useQuery({
    queryKey: ["inline-snippet", activeRepo, filePath, startLine, expanded ? snippetEnd : previewEnd],
    queryFn: () =>
      commands.readFileContent(filePath, startLine, expanded ? snippetEnd : previewEnd),
    staleTime: 60_000,
  });

  if (!content?.content) return null;

  const lines = content.content.split("\n");
  const hasMore = endLine != null && endLine - startLine > maxPreviewLines;

  return (
    <div
      className="rounded-lg overflow-hidden"
      style={{
        border: "1px solid var(--surface-border)",
        background: "var(--bg-2)",
      }}
    >
      <button
        onClick={() => setExpanded(!expanded)}
        className="flex items-center gap-1.5 w-full px-3 py-1.5 text-left"
        style={{
          background: "var(--bg-1)",
          borderBottom: "1px solid var(--surface-border)",
          color: "var(--text-2)",
          fontSize: 10,
          fontWeight: 500,
        }}
      >
        <Code2 size={10} />
        <span>{expanded ? t("detail.collapse") : t("detail.preview")}</span>
        {hasMore && !expanded && (
          <span style={{ color: "var(--text-3)", marginLeft: "auto" }}>
            {endLine != null ? endLine - startLine + 1 : "?"} lines
          </span>
        )}
      </button>
      <pre
        className="overflow-x-auto"
        style={{
          margin: 0,
          padding: "8px 12px",
          fontSize: 11,
          lineHeight: 1.5,
          fontFamily: "var(--font-mono)",
          color: "var(--text-1)",
          maxHeight: expanded ? 400 : 160,
          overflowY: "auto",
        }}
      >
        {lines.map((line, i) => (
          <div key={i} className="flex">
            <span
              style={{
                width: 36,
                flexShrink: 0,
                textAlign: "right",
                paddingRight: 12,
                color: "var(--text-4)",
                userSelect: "none",
              }}
            >
              {startLine + i}
            </span>
            <span>{line}</span>
          </div>
        ))}
      </pre>
    </div>
  );
}

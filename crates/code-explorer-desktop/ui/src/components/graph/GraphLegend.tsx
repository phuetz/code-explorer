import { useMemo } from "react";
import { X } from "lucide-react";
import { NODE_COLORS } from "../../lib/graph-adapter";
import { useI18n } from "../../hooks/use-i18n";
import type { CytoNode } from "../../lib/tauri-commands";

const LABEL_COLORS = NODE_COLORS;

interface GraphLegendProps {
  nodes: CytoNode[];
  expanded: boolean;
  onExpand: () => void;
  onCollapse: () => void;
  highlightedNodeType: string | null;
  onTypeClick: (type: string | null) => void;
}

export function GraphLegend({ 
  nodes, 
  expanded, 
  onExpand, 
  onCollapse, 
  highlightedNodeType, 
  onTypeClick 
}: GraphLegendProps) {
  const { t } = useI18n();

  const sortedEntries = useMemo(() => {
    const counts = new Map<string, number>();
    for (const node of nodes) {
      counts.set(node.label, (counts.get(node.label) || 0) + 1);
    }
    return Array.from(counts.entries()).sort(
      (a, b) => b[1] - a[1] || a[0].localeCompare(b[0]),
    );
  }, [nodes]);

  return (
    <div
      className="absolute z-15 pointer-events-auto"
      style={{
        bottom: "12px",
        right: "12px",
        maxHeight: "calc(100% - 24px)",
        overflow: "auto",
        borderRadius: "var(--radius-md)",
        backgroundColor: "var(--bg-2)",
        backdropFilter: "blur(12px)",
        border: "1px solid var(--surface-border)",
        padding: "8px 12px",
      }}
    >
      {!expanded ? (
        <button
          onClick={onExpand}
          className="uppercase text-[10px] font-semibold transition-colors hover:brightness-125"
          style={{ color: "var(--text-3)" }}
          aria-expanded={false}
          aria-label={t("graph.legend")}
        >
          {t("graph.legend")}
        </button>
      ) : (
        <div>
          <div className="flex items-center justify-between mb-2">
            <span
              className="uppercase text-[10px] font-semibold"
              style={{ color: "var(--text-3)" }}
            >
              {t("graph.legend")}
            </span>
            <button
              onClick={onCollapse}
              className="ml-2 transition-colors hover:brightness-125 p-0.5 rounded"
              style={{ color: "var(--text-3)" }}
              aria-label={t("graph.collapseLegend")}
            >
              <X size={12} />
            </button>
          </div>
          <div
            className="space-y-1 max-h-[calc(8*28px)] overflow-y-auto"
            style={{ maxWidth: "180px" }}
          >
            {sortedEntries.map(([type, count]) => (
              <button
                key={type}
                className="w-full flex items-center gap-2 cursor-pointer hover:bg-white/5 px-1 py-0.5 rounded transition-colors focus-visible:ring-2 focus-visible:ring-[var(--accent)] focus-visible:outline-none"
                style={{
                  padding: "4px 4px",
                  opacity: highlightedNodeType && highlightedNodeType !== type ? 0.4 : 1,
                  backgroundColor: highlightedNodeType === type ? "var(--bg-3)" : "transparent",
                  border: "none",
                }}
                aria-pressed={highlightedNodeType === type}
                onClick={() => onTypeClick(highlightedNodeType === type ? null : type)}
              >
                <span
                  className="w-2.5 h-2.5 rounded-full flex-shrink-0"
                  style={{
                    backgroundColor: LABEL_COLORS[type] || "#565f89",
                  }}
                />
                <span
                  className="text-[11px] truncate"
                  style={{ color: "var(--text-1)", fontWeight: highlightedNodeType === type ? 600 : 400 }}
                >
                  {type}
                </span>
                <span
                  className="text-[10px] ml-auto flex-shrink-0"
                  style={{ color: "var(--text-3)" }}
                >
                  {count}
                </span>
              </button>
            ))}
          </div>

          <div className="mt-4 pt-3 border-t border-[var(--surface-border)]">
            <span
              className="uppercase text-[9px] font-bold block mb-2"
              style={{ color: "var(--text-4)" }}
            >
              {t("graph.states") || "States"}
            </span>
            <div className="space-y-2">
              <div className="flex items-center gap-2">
                <span className="w-2.5 h-2.5 rounded-sm bg-[#f7768e] shrink-0" />
                <span className="text-[10px]" style={{ color: "var(--text-2)" }}>{t("graph.deadCode") || "Dead Code"}</span>
              </div>
              <div className="flex items-center gap-2">
                <span className="w-2.5 h-2.5 rounded-sm bg-[#ff9e64] shrink-0" />
                <span className="text-[10px]" style={{ color: "var(--text-2)" }}>{t("graph.hotspot") || "Hotspot"}</span>
              </div>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}

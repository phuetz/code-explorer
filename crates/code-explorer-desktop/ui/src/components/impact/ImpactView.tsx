import { useMemo, useState } from "react";
import { Zap, ArrowUp, ArrowDown, FileText } from "lucide-react";
import { useImpactAnalysis, useSearchSymbols } from "../../hooks/use-tauri-query";
import { useAppStore } from "../../stores/app-store";
import { useI18n } from "../../hooks/use-i18n";
import { AnimatedCounter, StaggerContainer, StaggerItem } from "../shared/motion";
import type { ImpactResult, ImpactNode } from "../../lib/tauri-commands";

type Direction = "upstream" | "downstream" | "both";

function getBarColor(depth: number, maxDepth: number): string {
  const ratio = depth / Math.max(maxDepth, 1);
  if (ratio < 0.33) return "var(--green)";
  if (ratio < 0.66) return "var(--amber)";
  return "var(--rose)";
}

export function ImpactView() {
  const { t } = useI18n();
  const selectedNodeId = useAppStore((s) => s.selectedNodeId);
  const setSelectedNodeId = useAppStore((s) => s.setSelectedNodeId);
  const [targetId, setTargetId] = useState<string | null>(selectedNodeId);
  const [direction, setDirection] = useState<Direction>("both");
  const [searchQuery, setSearchQuery] = useState("");

  const { data: searchResults } = useSearchSymbols(searchQuery, searchQuery.length >= 2);
  const { data: impact, isLoading, error } = useImpactAnalysis(
    targetId,
    direction,
    5
  );

  const handleSelectTarget = (nodeId: string) => {
    setTargetId(nodeId);
    setSearchQuery("");
  };

  return (
    <div className="h-full flex flex-col">
      {/* Header */}
      <div className="px-4 py-3 border-b border-[var(--border)] bg-[var(--bg-secondary)]">
        <h2 className="text-sm font-semibold text-[var(--text-primary)] mb-2 flex items-center gap-2">
          <Zap size={16} className="text-[var(--warning)]" />
          {t("impact.title")}
        </h2>

        {/* Target search */}
        <div className="relative mb-2">
          <input
            type="text"
            value={searchQuery}
            onChange={(e) => setSearchQuery(e.target.value)}
            placeholder={t("impact.placeholder")}
            aria-label="Search symbol for impact analysis"
            className="w-full px-3 py-1.5 rounded border border-[var(--border)] bg-[var(--bg-primary)] text-[var(--text-primary)] placeholder-[var(--text-muted)] text-xs focus:outline-none focus:border-[var(--accent)]"
          />
          {searchResults && searchResults.length > 0 && searchQuery.length >= 2 && (
            <div className="absolute z-10 top-full left-0 right-0 mt-1 rounded border border-[var(--border)] bg-[var(--bg-secondary)] max-h-40 overflow-y-auto shadow-lg">
              {searchResults.map((r) => (
                <button
                  key={r.nodeId}
                  onClick={() => handleSelectTarget(r.nodeId)}
                  className="flex items-center gap-2 w-full px-3 py-1.5 text-xs text-left hover:bg-[var(--bg-tertiary)]"
                >
                  <span className="px-1 rounded bg-[var(--bg-tertiary)] text-[var(--accent)]">
                    {r.label}
                  </span>
                  <span className="truncate">{r.name}</span>
                </button>
              ))}
            </div>
          )}
        </div>

        {/* Direction toggle */}
        <div className="flex rounded border border-[var(--border)] overflow-hidden">
          {(["upstream", "both", "downstream"] as Direction[]).map((d) => (
            <button
              key={d}
              onClick={() => setDirection(d)}
              className={`flex-1 px-2 py-1 text-[11px] transition-colors ${
                direction === d
                  ? "bg-[var(--accent)] text-white"
                  : "text-[var(--text-muted)] hover:bg-[var(--bg-tertiary)]"
              }`}
            >
              {d === "upstream" && <ArrowUp size={11} className="inline mr-1" />}
              {d === "downstream" && <ArrowDown size={11} className="inline mr-1" />}
              {d === "upstream" ? t("impact.directionUpstream") : d === "both" ? t("impact.directionBoth") : t("impact.directionDownstream")}
            </button>
          ))}
        </div>
      </div>

      {/* Results */}
      <div className="flex-1 overflow-y-auto p-3">
        {!targetId && (
          <p className="text-center text-[var(--text-muted)] py-8">
            {t("impact.searchAndSelect")}
          </p>
        )}

        {isLoading && (
          <p className="text-center text-[var(--text-muted)] py-8">
            {t("impact.analyzingImpact")}
          </p>
        )}

        {error && (
          <p className="text-center text-[var(--danger)] py-4">
            Error: {String(error)}
          </p>
        )}

        {impact && (
          <ImpactResults
            impact={impact}
            t={t}
            setSelectedNodeId={setSelectedNodeId}
          />
        )}
      </div>
    </div>
  );
}

function ImpactResults({
  impact,
  t,
  setSelectedNodeId,
}: {
  impact: ImpactResult;
  t: (key: string) => string;
  setSelectedNodeId: (id: string, name?: string) => void;
}) {
  // Merge upstream + downstream into a single sorted list for the bar chart
  const { impactNodes, maxDepth } = useMemo(() => {
    const all: (ImpactNode & { direction: "upstream" | "downstream" })[] = [
      ...impact.upstream.map((n) => ({ ...n, direction: "upstream" as const })),
      ...impact.downstream.map((n) => ({ ...n, direction: "downstream" as const })),
    ];
    // Sort by depth descending so deepest impacts appear first
    all.sort((a, b) => b.depth - a.depth);
    const max = all.reduce((m, n) => Math.max(m, n.depth), 0);
    return { impactNodes: all, maxDepth: max };
  }, [impact.upstream, impact.downstream]);

  return (
    <div className="space-y-4">
      {/* Summary stats */}
      <StaggerContainer className="grid grid-cols-3 gap-2">
        <StaggerItem>
          <StatCard
            label={t("impact.statUpstream")}
            value={impact.summary.upstreamCount}
            icon={<ArrowUp size={14} />}
            color="var(--accent)"
          />
        </StaggerItem>
        <StaggerItem>
          <StatCard
            label={t("impact.statDownstream")}
            value={impact.summary.downstreamCount}
            icon={<ArrowDown size={14} />}
            color="var(--warning)"
          />
        </StaggerItem>
        <StaggerItem>
          <StatCard
            label={t("impact.statFiles")}
            value={impact.summary.affectedFilesCount}
            icon={<FileText size={14} />}
            color="var(--success)"
          />
        </StaggerItem>
      </StaggerContainer>

      {/* Target info */}
      <div className="p-3 rounded border border-[var(--accent)] bg-[var(--accent)]/10">
        <div className="flex items-center gap-2">
          <span className="text-[10px] px-1.5 py-0.5 rounded bg-[var(--accent)] text-white shrink-0">
            {impact.target.label}
          </span>
          <span className="font-medium truncate">{impact.target.name}</span>
        </div>
        <p className="text-[10px] text-[var(--text-muted)] mt-1">
          {impact.target.filePath}
        </p>
      </div>

      {/* Impact Distribution bar chart */}
      {impactNodes.length > 0 && (
        <div>
          <h3 className="text-xs font-semibold text-[var(--text-secondary)] uppercase tracking-wider mb-2">
            {t("impact.impactDistribution")}
          </h3>
          <div className="space-y-1">
            {impactNodes.slice(0, 15).map((item) => {
              const barWidth = Math.max(10, (item.depth / maxDepth) * 100);
              return (
                <div
                  key={item.node.id}
                  className="flex items-center gap-2 cursor-pointer group"
                  onClick={() => setSelectedNodeId(item.node.id, item.node.name)}
                >
                  <span
                    className="text-[11px] truncate w-28 shrink-0 group-hover:text-[var(--accent)] transition-colors"
                    style={{ color: "var(--text-2)" }}
                  >
                    {item.node.name}
                  </span>
                  <div
                    className="flex-1 h-5 rounded overflow-hidden"
                    style={{ background: "var(--bg-3)" }}
                  >
                    <div
                      className="h-full rounded transition-all duration-300"
                      style={{
                        width: `${barWidth}%`,
                        background: getBarColor(item.depth, maxDepth),
                        opacity: 0.85,
                      }}
                    />
                  </div>
                  <span
                    className="text-[11px] w-6 text-right tabular-nums shrink-0"
                    style={{ color: "var(--text-3)" }}
                  >
                    {item.depth}
                  </span>
                </div>
              );
            })}
          </div>
          {impactNodes.length > 15 && (
            <p className="text-[10px] mt-1" style={{ color: "var(--text-3)" }}>
              {t("impact.moreItems").replace("{0}", String(impactNodes.length - 15))}
            </p>
          )}
        </div>
      )}

      {/* Affected files badges */}
      {impact.affectedFiles.length > 0 && (
        <div>
          <h3 className="text-xs font-semibold text-[var(--text-secondary)] uppercase tracking-wider mb-2">
            {t("impact.affectedFiles")} ({impact.affectedFiles.length})
          </h3>
          <div className="flex flex-wrap gap-1.5">
            {impact.affectedFiles.map((file) => (
              <span
                key={file}
                className="text-[11px] px-2 py-0.5 rounded-full"
                style={{
                  background: "var(--accent-subtle)",
                  color: "var(--accent)",
                }}
                title={file}
              >
                {file.split("/").pop()}
              </span>
            ))}
          </div>
        </div>
      )}

      {/* Upstream list */}
      {impact.upstream.length > 0 && (
        <ImpactSection
          title={t("impact.upstream")}
          nodes={impact.upstream}
          onSelect={(id, name) => {
            setSelectedNodeId(id, name);
          }}
        />
      )}

      {/* Downstream list */}
      {impact.downstream.length > 0 && (
        <ImpactSection
          title={t("impact.downstream")}
          nodes={impact.downstream}
          onSelect={(id, name) => {
            setSelectedNodeId(id, name);
          }}
        />
      )}
    </div>
  );
}

function StatCard({
  label,
  value,
  icon,
  color,
}: {
  label: string;
  value: number;
  icon: React.ReactNode;
  color: string;
}) {
  return (
    <div className="p-2 rounded border border-[var(--border)] bg-[var(--bg-secondary)] text-center">
      <div className="flex items-center justify-center gap-1 mb-1" style={{ color }}>
        {icon}
        <AnimatedCounter value={value} className="text-lg font-bold" />
      </div>
      <span className="text-[10px] text-[var(--text-muted)]">{label}</span>
    </div>
  );
}

function ImpactSection({
  title,
  nodes,
  onSelect,
}: {
  title: string;
  nodes: { node: { id: string; name: string; label: string; filePath: string }; depth: number }[];
  onSelect: (id: string, name?: string) => void;
}) {
  return (
    <div>
      <h3 className="text-xs font-semibold text-[var(--text-secondary)] uppercase tracking-wider mb-1">
        {title} ({nodes.length})
      </h3>
      <ul className="space-y-0.5">
        {nodes.map((item) => (
          <li
            key={item.node.id}
            onClick={() => onSelect(item.node.id, item.node.name)}
            className="flex items-center gap-2 px-2 py-1 rounded cursor-pointer hover:bg-[var(--bg-tertiary)] transition-colors text-xs"
          >
            <span
              className="w-5 h-5 rounded-full flex items-center justify-center text-[10px] font-bold shrink-0"
              style={{
                backgroundColor: `hsl(${Math.max(0, 200 - item.depth * 40)}, 70%, 50%)`,
                color: "white",
              }}
            >
              {item.depth}
            </span>
            <span className="px-1 rounded bg-[var(--bg-tertiary)] text-[var(--text-muted)] text-[10px]">
              {item.node.label}
            </span>
            <span className="truncate">{item.node.name}</span>
            <span className="ml-auto text-[10px] text-[var(--text-muted)] truncate max-w-[100px]">
              {item.node.filePath}
            </span>
          </li>
        ))}
      </ul>
    </div>
  );
}

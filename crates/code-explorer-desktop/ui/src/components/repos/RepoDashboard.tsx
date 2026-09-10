/**
 * RepoDashboard -- SonarQube / CodeScene-inspired overview screen.
 * Shows quality banner, metric cards, node type distribution, and top connected nodes.
 */

import { memo, useMemo } from "react";
import { useQuery } from "@tanstack/react-query";
import {
  Files,
  Code2,
  Boxes,
  Network,
  ArrowRightLeft,
  Languages,
  Zap,
  GitBranch,
  Shield,
} from "lucide-react";
import { useAppStore } from "../../stores/app-store";
import { useI18n } from "../../hooks/use-i18n";
import { CodeHealthCard } from "../health/CodeHealthCard";
import { commands } from "../../lib/tauri-commands";
import type { CytoNode, CytoEdge, GraphPayload, IndexMetrics, RepoOverview } from "../../lib/tauri-commands";
import {
  AnimatedCard,
  AnimatedCounter,
  StaggerContainer,
  StaggerItem,
  SkeletonBlock,
} from "../shared/motion";
import { DonutChart } from "../shared/DonutChart";
import { InsightsSection } from "../analyze/InsightsSection";
import { HealthRadarChart } from "../shared/HealthRadarChart";

// ─── Label Colors (mirrored from GraphExplorer) ─────────────────────

const LABEL_COLORS: Record<string, string> = {
  Function: "var(--accent)",
  Class: "#bb9af7",
  Method: "#7dcfff",
  Interface: "var(--amber)",
  Struct: "#ff9e64",
  Trait: "var(--green)",
  Enum: "var(--rose)",
  File: "#565f89",
  Folder: "#414868",
  Module: "#565f89",
  Package: "#414868",
  Variable: "#73daca",
  Type: "#c0caf5",
  Import: "#414868",
  Community: "var(--green)",
  Process: "var(--amber)",
  Constructor: "#7dcfff",
  Property: "#73daca",
  Route: "#ff9e64",
  Tool: "var(--amber)",
  Namespace: "#414868",
};

// ─── Helpers ────────────────────────────────────────────────────────

/** Count nodes matching a given label (case-insensitive). */
function countByLabel(nodes: CytoNode[], label: string): number {
  return nodes.filter(
    (n) => n.label.toLowerCase() === label.toLowerCase(),
  ).length;
}

/** Build a map of label -> count, sorted descending. */
function buildLabelDistribution(nodes: CytoNode[]): { label: string; count: number; color: string }[] {
  const map = new Map<string, number>();
  for (const n of nodes) {
    map.set(n.label, (map.get(n.label) ?? 0) + 1);
  }
  return Array.from(map.entries())
    .map(([label, count]) => ({
      label,
      count,
      color: LABEL_COLORS[label] ?? "#565f89",
    }))
    .sort((a, b) => b.count - a.count);
}

/** Compute degree (edge count) per node, return top N. */
function topConnectedNodes(
  nodes: CytoNode[],
  edges: CytoEdge[],
  limit: number,
): { node: CytoNode; degree: number }[] {
  const degreeMap = new Map<string, number>();
  for (const e of edges) {
    degreeMap.set(e.source, (degreeMap.get(e.source) ?? 0) + 1);
    degreeMap.set(e.target, (degreeMap.get(e.target) ?? 0) + 1);
  }
  const nodeMap = new Map<string, CytoNode>();
  for (const n of nodes) nodeMap.set(n.id, n);

  return Array.from(degreeMap.entries())
    .sort((a, b) => b[1] - a[1])
    .slice(0, limit)
    .map(([id, degree]) => ({ node: nodeMap.get(id), degree }))
    .filter((x): x is { node: CytoNode; degree: number } => x.node != null);
}

/** Unique language count from nodes. */
function uniqueLanguages(nodes: CytoNode[]): number {
  const langs = new Set<string>();
  for (const n of nodes) {
    if (n.language) langs.add(n.language);
  }
  return langs.size;
}

// ─── Metric Card Config ─────────────────────────────────────────────

interface MetricDef {
  key: string;
  label: string;
  icon: React.ComponentType<{ size?: number; style?: React.CSSProperties }>;
  color: string;
  getValue: (nodes: CytoNode[], edges: CytoEdge[]) => number;
  /** Real full-graph count from indexed stats, when available. The loaded node
   *  list is capped at 10k by importance, so countByLabel under-counts whole-graph
   *  types (File/Process/Module…). Prefer this; fall back to getValue. */
  getReal?: (ctx: { metrics: IndexMetrics | null | undefined; overview: RepoOverview | undefined }) => number | undefined;
}

const METRICS: MetricDef[] = [
  {
    key: "files",
    label: "Files",
    icon: Files,
    color: "#565f89",
    getValue: (nodes) => countByLabel(nodes, "File"),
    getReal: ({ overview, metrics }) => overview?.fileCount ?? metrics?.files,
  },
  {
    key: "functions",
    label: "Functions",
    icon: Code2,
    color: "var(--accent)",
    getValue: (nodes) => countByLabel(nodes, "Function"),
    getReal: ({ overview }) => overview?.functionCount,
  },
  {
    key: "classes",
    label: "Classes",
    icon: Boxes,
    color: "#bb9af7",
    getValue: (nodes) => countByLabel(nodes, "Class"),
    getReal: ({ overview }) => overview?.classCount,
  },
  {
    key: "modules",
    label: "Modules",
    icon: Network,
    color: "#565f89",
    getValue: (nodes) => countByLabel(nodes, "Module"),
  },
  {
    key: "relations",
    label: "Relations",
    icon: ArrowRightLeft,
    color: "#7dcfff",
    getValue: (_nodes, edges) => edges.length,
    getReal: ({ overview, metrics }) => overview?.edgeCount ?? metrics?.edges,
  },
  {
    key: "languages",
    label: "Languages",
    icon: Languages,
    color: "var(--green)",
    getValue: (nodes) => uniqueLanguages(nodes),
    getReal: ({ overview }) => overview?.languageBreakdown?.length,
  },
  {
    key: "entryPoints",
    label: "Entry Points",
    icon: Zap,
    color: "var(--amber)",
    getValue: (nodes) =>
      nodes.filter((n) => n.entryPointScore != null && n.entryPointScore > 0)
        .length,
  },
  {
    key: "processes",
    label: "Processes",
    icon: GitBranch,
    color: "#ff9e64",
    getValue: (nodes) => countByLabel(nodes, "Process"),
    getReal: ({ metrics }) => metrics?.processes,
  },
  {
    key: "traced",
    label: "Traced",
    icon: Shield,
    color: "#73daca",
    getValue: (nodes) =>
      nodes.filter((n) => n.isTraced === true).length,
    getReal: ({ overview }) => overview?.tracedCount,
  },
];

// ─── Quality level ──────────────────────────────────────────────────

function qualityGradient(totalNodes: number): {
  bg: string;
  border: string;
  badge: string;
  badgeText: string;
  text: string;
} {
  if (totalNodes >= 500) {
    return {
      bg: "linear-gradient(135deg, rgba(158,206,106,0.10) 0%, rgba(158,206,106,0.04) 100%)",
      border: "rgba(158,206,106,0.25)",
      badge: "rgba(158,206,106,0.18)",
      badgeText: "var(--green)",
      text: "dashboard.healthy",
    };
  }
  if (totalNodes >= 100) {
    return {
      bg: "linear-gradient(135deg, rgba(224,175,104,0.10) 0%, rgba(224,175,104,0.04) 100%)",
      border: "rgba(224,175,104,0.25)",
      badge: "rgba(224,175,104,0.18)",
      badgeText: "var(--amber)",
      text: "dashboard.growing",
    };
  }
  return {
    bg: "linear-gradient(135deg, rgba(86,95,137,0.10) 0%, rgba(86,95,137,0.04) 100%)",
    border: "rgba(86,95,137,0.25)",
    badge: "rgba(86,95,137,0.18)",
    badgeText: "var(--text-2)",
    text: "dashboard.small",
  };
}

// ─── Component ──────────────────────────────────────────────────────

export const RepoDashboard = memo(function RepoDashboard() {
  const { t } = useI18n();
  const activeRepo = useAppStore((s) => s.activeRepo);
  const setSelectedNodeId = useAppStore((s) => s.setSelectedNodeId);
  const setMode = useAppStore((s) => s.setMode);

  // Fetch graph data at symbol zoom level to get all nodes
  const { data, isLoading, error } = useQuery<GraphPayload>({
    queryKey: ["dashboard-graph", activeRepo],
    queryFn: () =>
      commands.getGraphData({
        zoomLevel: "symbol",
        maxNodes: 10_000,
      }),
    enabled: !!activeRepo,
    staleTime: 60_000,
  });

  const { data: healthData } = useQuery({
    queryKey: ["code-health-dashboard", activeRepo],
    queryFn: () => commands.getCodeHealth(),
    enabled: !!activeRepo,
    staleTime: 60_000,
  });

  const { data: metrics } = useQuery({
    queryKey: ["repo-metrics", activeRepo],
    queryFn: () => commands.repoMetrics(activeRepo!),
    enabled: !!activeRepo,
    staleTime: 60_000,
  });

  // Whole-graph per-type counts (the loaded node list is capped at 10k by
  // importance, so the metric cards must read real counts from here, not the sample).
  const { data: overviewList } = useQuery({
    queryKey: ["repos-overview-dashboard"],
    queryFn: () => commands.reposOverview(),
    enabled: !!activeRepo,
    staleTime: 60_000,
  });
  const overview = useMemo(
    () => overviewList?.find((r) => r.name === activeRepo),
    [overviewList, activeRepo],
  );

  const nodes = useMemo(() => data?.nodes ?? [], [data?.nodes]);
  const edges = useMemo(() => data?.edges ?? [], [data?.edges]);
  const totalNodes = data?.stats?.nodeCount ?? nodes.length;
  const totalEdges = data?.stats?.edgeCount ?? edges.length;

  const distribution = useMemo(() => buildLabelDistribution(nodes), [nodes]);
  const topNodes = useMemo(() => topConnectedNodes(nodes, edges, 8), [nodes, edges]);
  const quality = useMemo(() => qualityGradient(totalNodes), [totalNodes]);
  const totalFiles = useMemo(() => countByLabel(nodes, "File"), [nodes]);
  const distributionTotal = useMemo(
    () => distribution.reduce((sum, d) => sum + d.count, 0),
    [distribution],
  );

  // ── Loading state ───────────────────────────────────────────────
  if (!activeRepo) {
    return (
      <div
        style={{
          display: "flex",
          alignItems: "center",
          justifyContent: "center",
          height: "100%",
          color: "var(--text-4)",
          fontSize: 14,
        }}
      >
        Select a repository to view the dashboard.
      </div>
    );
  }

  if (isLoading) {
    return (
      <div style={{ padding: 24, display: "flex", flexDirection: "column", gap: 16 }}>
        <SkeletonBlock height="100px" />
        <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr 1fr", gap: 12 }}>
          {Array.from({ length: 6 }).map((_, i) => (
            <SkeletonBlock key={i} height="96px" />
          ))}
        </div>
        <SkeletonBlock height="48px" />
        <SkeletonBlock height="200px" />
      </div>
    );
  }

  if (error) {
    return (
      <div
        style={{
          display: "flex",
          alignItems: "center",
          justifyContent: "center",
          height: "100%",
          color: "var(--rose)",
          fontSize: 13,
          padding: 24,
          textAlign: "center",
        }}
      >
        Failed to load dashboard data.
      </div>
    );
  }

  // ── Navigate to graph with node selected ────────────────────────
  const navigateToNode = (nodeId: string, name: string) => {
    setSelectedNodeId(nodeId, name);
    setMode("explorer");
  };

  return (
    <div
      style={{
        padding: 24,
        overflowY: "auto",
        height: "100%",
        display: "flex",
        flexDirection: "column",
        gap: 20,
      }}
    >
      {/* ── Code Health Score ─────────────────────────────────── */}
      <CodeHealthCard />

      {/* ── Quality Banner ──────────────────────────────────────── */}
      <AnimatedCard
        style={{
          background: quality.bg,
          border: `1px solid ${quality.border}`,
          borderRadius: 14,
          padding: "20px 24px",
          display: "flex",
          alignItems: "center",
          justifyContent: "space-between",
          flexWrap: "wrap",
          gap: 12,
        }}
      >
        <div style={{ display: "flex", flexDirection: "column", gap: 4 }}>
          <div
            style={{
              fontSize: 18,
              fontWeight: 700,
              color: "var(--text-0)",
              letterSpacing: "-0.01em",
            }}
          >
            {activeRepo}
          </div>
          <div
            style={{
              display: "flex",
              alignItems: "center",
              gap: 16,
              fontSize: 12,
              color: "var(--text-3)",
            }}
          >
            <span>
              <strong style={{ color: "var(--text-1)" }}>
                {totalNodes.toLocaleString()}
              </strong>{" "}
              nodes
            </span>
            <span>
              <strong style={{ color: "var(--text-1)" }}>
                {totalEdges.toLocaleString()}
              </strong>{" "}
              edges
            </span>
            <span>
              <strong style={{ color: "var(--text-1)" }}>
                {totalFiles.toLocaleString()}
              </strong>{" "}
              files
            </span>
            {metrics?.totalDurationMs != null && (
              <span title="Last index duration">
                ⚡ indexed in{" "}
                <strong style={{ color: "var(--text-1)" }}>
                  {(metrics.totalDurationMs / 1000).toFixed(1)}s
                </strong>
              </span>
            )}
          </div>
        </div>
        <div
          style={{
            padding: "4px 12px",
            borderRadius: 20,
            background: quality.badge,
            color: quality.badgeText,
            fontSize: 11,
            fontWeight: 600,
            letterSpacing: "0.03em",
            textTransform: "uppercase",
          }}
        >
          {t(quality.text)}
        </div>
      </AnimatedCard>

      {/* ── Metric Cards Grid ───────────────────────────────────── */}
      <StaggerContainer
        style={{
          display: "grid",
          gridTemplateColumns: "repeat(3, 1fr)",
          gap: 12,
        }}
      >
        {METRICS.map((m) => {
          const real = m.getReal?.({ metrics, overview });
          const value = real ?? m.getValue(nodes, edges);
          const Icon = m.icon;
          return (
            <StaggerItem key={m.key}>
              <AnimatedCard
                style={{
                  background: "var(--surface)", boxShadow: "var(--shadow-sm)",
                  border: "1px solid var(--surface-border)",
                  borderRadius: 12,
                  padding: "16px 18px",
                  display: "flex",
                  alignItems: "center",
                  gap: 14,
                  cursor: "default",
                }}
              >
                <div
                  style={{
                    width: 40,
                    height: 40,
                    borderRadius: 10,
                    background: `color-mix(in srgb, ${m.color} 12%, transparent)`,
                    display: "flex",
                    alignItems: "center",
                    justifyContent: "center",
                    flexShrink: 0,
                  }}
                >
                  <Icon size={20} style={{ color: m.color }} />
                </div>
                <div style={{ minWidth: 0 }}>
                  <div
                    style={{
                      fontSize: 22,
                      fontWeight: 700,
                      color: "var(--text-0)",
                      lineHeight: 1.1,
                    }}
                  >
                    <AnimatedCounter value={value} />
                  </div>
                  <div
                    style={{
                      fontSize: 11,
                      color: "var(--text-3)",
                      marginTop: 2,
                      fontWeight: 500,
                    }}
                  >
                    {m.label}
                  </div>
                </div>
              </AnimatedCard>
            </StaggerItem>
          );
        })}
      </StaggerContainer>

      {/* ── Visual Analytics ────────────────────────────────── */}
      <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: 16 }}>
        {/* Node Type Distribution */}
        {distribution.length > 0 && (
          <div
            style={{
              background: "var(--surface)", boxShadow: "var(--shadow-sm)",
              border: "1px solid var(--surface-border)",
              borderRadius: 12,
              padding: "16px 18px",
            }}
          >
            <div
              style={{
                fontSize: 12,
                fontWeight: 600,
                color: "var(--text-2)",
                marginBottom: 12,
                textTransform: "uppercase",
                letterSpacing: "0.04em",
              }}
            >
              {t("dashboard.nodeTypeDistribution")}
            </div>

            <DonutChart
              segments={distribution.map((d) => ({ label: d.label, value: d.count, color: d.color }))}
              size={160}
              thickness={20}
              centerValue={String(distributionTotal)}
              centerLabel="symbols"
            />
          </div>
        )}

        {/* Health Radar Chart */}
        {healthData && (
          <div
            style={{
              background: "var(--surface)", boxShadow: "var(--shadow-sm)",
              border: "1px solid var(--surface-border)",
              borderRadius: 12,
              padding: "16px 18px",
            }}
          >
            <div
              style={{
                fontSize: 12,
                fontWeight: 600,
                color: "var(--text-2)",
                marginBottom: 12,
                textTransform: "uppercase",
                letterSpacing: "0.04em",
              }}
            >
              Health Radar
            </div>
            <HealthRadarChart health={healthData} />
          </div>
        )}
      </div>

      {/* ── Top Connected Nodes ─────────────────────────────────── */}
      {topNodes.length > 0 && (
        <div
          style={{
            background: "var(--surface)", boxShadow: "var(--shadow-sm)",
            border: "1px solid var(--surface-border)",
            borderRadius: 12,
            padding: "16px 18px",
          }}
        >
          <div
            style={{
              fontSize: 12,
              fontWeight: 600,
              color: "var(--text-2)",
              marginBottom: 12,
              textTransform: "uppercase",
              letterSpacing: "0.04em",
            }}
          >
            {t("dashboard.topConnectedNodes")}
          </div>

          <div style={{ display: "flex", flexDirection: "column", gap: 2 }}>
            {topNodes.map(({ node, degree }) => {
              const labelColor = LABEL_COLORS[node.label] ?? "#565f89";
              return (
                <button
                  key={node.id}
                  onClick={() => navigateToNode(node.id, node.name)}
                  style={{
                    display: "flex",
                    alignItems: "center",
                    gap: 10,
                    padding: "8px 10px",
                    borderRadius: 8,
                    cursor: "pointer",
                    transition: "background 0.15s",
                    border: "none",
                    background: "transparent",
                    width: "100%",
                    textAlign: "left",
                  }}
                  className="hover:brightness-110"
                >
                  {/* Icon dot */}
                  <div
                    style={{
                      width: 8,
                      height: 8,
                      borderRadius: "50%",
                      background: labelColor,
                      flexShrink: 0,
                    }}
                  />

                  {/* Name */}
                  <span
                    style={{
                      fontSize: 13,
                      fontWeight: 600,
                      color: "var(--text-1)",
                      flex: 1,
                      minWidth: 0,
                      overflow: "hidden",
                      textOverflow: "ellipsis",
                      whiteSpace: "nowrap",
                    }}
                  >
                    {node.name}
                  </span>

                  {/* Type badge */}
                  <span
                    style={{
                      fontSize: 10,
                      fontWeight: 600,
                      padding: "2px 8px",
                      borderRadius: 10,
                      background: `color-mix(in srgb, ${labelColor} 14%, transparent)`,
                      color: labelColor,
                      textTransform: "uppercase",
                      letterSpacing: "0.04em",
                      flexShrink: 0,
                    }}
                  >
                    {node.label}
                  </span>

                  {/* File path */}
                  <span
                    style={{
                      fontSize: 11,
                      color: "var(--text-4)",
                      maxWidth: 200,
                      overflow: "hidden",
                      textOverflow: "ellipsis",
                      whiteSpace: "nowrap",
                      flexShrink: 1,
                    }}
                  >
                    {node.filePath}
                  </span>

                  {/* Degree count */}
                  <span
                    style={{
                      fontSize: 11,
                      fontWeight: 700,
                      color: "var(--text-2)",
                      fontVariantNumeric: "tabular-nums",
                      flexShrink: 0,
                      minWidth: 32,
                      textAlign: "right",
                    }}
                  >
                    {degree}
                  </span>
                </button>
              );
            })}
          </div>
        </div>
      )}

      {/* ── Top Complex Functions ──────────────────────────────── */}
      {data && (() => {
        const complexNodes = data.nodes
          .filter(n => n.complexity && n.complexity > 5)
          .sort((a, b) => (b.complexity || 0) - (a.complexity || 0))
          .slice(0, 8);

        if (complexNodes.length === 0) return null;

        return (
          <AnimatedCard>
            <div style={{ padding: "16px 20px", borderRadius: "var(--radius-lg)", border: "1px solid var(--border)", background: "var(--surface)", boxShadow: "var(--shadow-sm)" }}>
              <h3 className="text-sm font-semibold" style={{ color: "var(--text-0)", marginBottom: 12 }}>
                {t("dashboard.mostComplexFunctions")}
              </h3>
              <div style={{ display: "flex", flexDirection: "column", gap: 6 }}>
                {complexNodes.map(n => (
                  <button
                    key={n.id}
                    onClick={() => { setSelectedNodeId(n.id, n.name); setMode("explorer"); }}
                    className="flex items-center gap-2 text-left rounded-md transition-colors hover:brightness-110"
                    style={{ padding: "6px 8px", fontSize: 11 }}
                  >
                    <span className="font-mono font-medium" style={{ color: "var(--text-0)", flex: 1, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>
                      {n.name}
                    </span>
                    <span className="text-[10px] font-medium px-1.5 py-0.5 rounded-full" style={{
                      background: (n.complexity || 0) > 20 ? "#ef444420" : (n.complexity || 0) > 10 ? "#f59e0b20" : "#22c55e20",
                      color: (n.complexity || 0) > 20 ? "#ef4444" : (n.complexity || 0) > 10 ? "#f59e0b" : "#22c55e",
                    }}>
                      CC:{n.complexity}
                    </span>
                  </button>
                ))}
              </div>
            </div>
          </AnimatedCard>
        );
      })()}

      {/* ── Insights ───────────────────────────────────────────── */}
      <AnimatedCard
        style={{
          padding: "20px 24px",
          borderRadius: 14,
          background: "var(--surface)",
          border: "1px solid var(--surface-border)",
        }}
      >
        <InsightsSection stats={data?.stats} health={healthData} />
      </AnimatedCard>
    </div>
  );
});

/**
 * MultiRepoOverview — at-a-glance dashboard for every indexed repo.
 *
 * Rows summarize node/edge/file counts, dead-code count, tracing coverage,
 * and language mix. Click a row → switch active repo. Sort by any column.
 */

import { useState, useMemo } from "react";
import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import { Folder, AlertCircle, RefreshCw, Trash2 } from "lucide-react";
import { commands } from "../../lib/tauri-commands";
import { useAppStore } from "../../stores/app-store";
import { useI18n } from "../../hooks/use-i18n";
import { confirm } from "../../lib/confirm";
import type { RepoOverview } from "../../lib/tauri-commands";
import { toast } from "sonner";

type SortKey = "name" | "nodeCount" | "deadCount" | "tracingCoverage" | "indexedAt";

export function MultiRepoOverview() {
  const { t } = useI18n();
  const queryClient = useQueryClient();
  const setActiveRepo = useAppStore((s) => s.setActiveRepo);
  const setMode = useAppStore((s) => s.setMode);
  const [sortKey, setSortKey] = useState<SortKey>("nodeCount");
  const [sortAsc, setSortAsc] = useState(false);

  const { data, isLoading, error, refetch } = useQuery({
    queryKey: ["repos-overview"],
    queryFn: () => commands.reposOverview(),
    staleTime: 60_000,
  });

  const unregisterMut = useMutation({
    mutationFn: (path: string) => commands.unregisterRepo(path),
    onSuccess: (_data, path) => {
      queryClient.invalidateQueries({ queryKey: ["repos-overview"] });
      queryClient.invalidateQueries({ queryKey: ["repos"] });
      toast.success(t("multiRepo.unregistered").replace("{0}", path));
    },
    onError: (e) => toast.error(`${(e as Error).message}`),
  });

  const sorted = useMemo(() => {
    if (!data) return [];
    const arr = [...data];
    arr.sort((a, b) => {
      const va = a[sortKey];
      const vb = b[sortKey];
      const cmp = typeof va === "string" ? va.localeCompare(vb as string) : Number(va) - Number(vb);
      return sortAsc ? cmp : -cmp;
    });
    return arr;
  }, [data, sortKey, sortAsc]);

  const totals = useMemo(() => {
    if (!data || data.length === 0) return null;
    return data.reduce(
      (acc, r) => ({
        nodes: acc.nodes + r.nodeCount,
        edges: acc.edges + r.edgeCount,
        files: acc.files + r.fileCount,
        functions: acc.functions + r.functionCount,
        dead: acc.dead + r.deadCount,
      }),
      { nodes: 0, edges: 0, files: 0, functions: 0, dead: 0 },
    );
  }, [data]);

  const flipSort = (key: SortKey) => {
    if (key === sortKey) {
      setSortAsc((v) => !v);
    } else {
      setSortKey(key);
      setSortAsc(false);
    }
  };

  if (isLoading) {
    return <div style={{ padding: 24, color: "var(--text-3)", fontSize: 12 }}>Loading repos…</div>;
  }
  if (error) {
    return (
      <div
        style={{
          padding: 16,
          color: "var(--rose)",
          fontSize: 12,
          display: "flex",
          alignItems: "center",
          gap: 8,
        }}
      >
        <AlertCircle size={14} />
        Failed to load: {(error as Error).message}
      </div>
    );
  }
  if (!data || data.length === 0) {
    return (
      <div style={{ padding: 24, fontSize: 12, color: "var(--text-3)" }}>
        {t("multiRepo.empty.prefix")} <code>code-explorer analyze</code> {t("multiRepo.empty.suffix")}
      </div>
    );
  }

  return (
    <div>
      {/* Aggregate header */}
      {totals && (
        <div
          style={{
            display: "grid",
            gridTemplateColumns: "repeat(auto-fill, minmax(120px, 1fr))",
            gap: 8,
            marginBottom: 16,
          }}
        >
          <Stat label="Repos" value={data.length} accent="var(--accent)" />
          <Stat label="Nodes" value={totals.nodes} />
          <Stat label="Edges" value={totals.edges} />
          <Stat label="Files" value={totals.files} />
          <Stat label="Functions" value={totals.functions} />
          <Stat label="Dead candidates" value={totals.dead} accent="var(--rose)" />
        </div>
      )}

      {/* Toolbar */}
      <div style={{ display: "flex", alignItems: "center", marginBottom: 8 }}>
        <span style={{ fontSize: 11, color: "var(--text-3)" }}>
          Click a row to switch active repo
        </span>
        <button
          onClick={() => {
            refetch();
            toast.success("Refreshed");
          }}
          aria-label="Refresh overview"
          style={{
            marginLeft: "auto",
            padding: "4px 8px",
            background: "transparent",
            border: "1px solid var(--surface-border)",
            borderRadius: 6,
            color: "var(--text-3)",
            cursor: "pointer",
            display: "inline-flex",
            alignItems: "center",
            gap: 4,
            fontSize: 11,
          }}
        >
          <RefreshCw size={11} /> Refresh
        </button>
      </div>

      {/* Table */}
      <div
        style={{
          border: "1px solid var(--surface-border)",
          borderRadius: 8,
          overflow: "hidden",
        }}
      >
        <table style={{ width: "100%", borderCollapse: "collapse", fontSize: 12 }}>
          <thead>
            <tr style={{ background: "var(--bg-2)" }}>
              <Th label="Repo" onClick={() => flipSort("name")} active={sortKey === "name"} asc={sortAsc} />
              <Th label="Nodes" onClick={() => flipSort("nodeCount")} active={sortKey === "nodeCount"} asc={sortAsc} numeric />
              <Th label="Files" onClick={() => flipSort("name")} active={false} asc={false} numeric />
              <Th label="Dead" onClick={() => flipSort("deadCount")} active={sortKey === "deadCount"} asc={sortAsc} numeric />
              <Th label="Traced" onClick={() => flipSort("tracingCoverage")} active={sortKey === "tracingCoverage"} asc={sortAsc} numeric />
              <Th label="Duration" onClick={() => {}} active={false} asc={false} numeric />
              <Th label="Top languages" onClick={() => {}} active={false} asc={false} />
              <Th label="Indexed" onClick={() => flipSort("indexedAt")} active={sortKey === "indexedAt"} asc={sortAsc} />
            </tr>
          </thead>
          <tbody>
            {sorted.map((r) => (
              <RepoRow
                key={r.path}
                repo={r}
                onPick={() => {
                  setActiveRepo(r.name);
                  setMode("explorer");
                  toast.success(`Switched to ${r.name}`);
                }}
                onRemove={async () => {
                  const ok = await confirm({
                    title: t("confirm.deleteTitle"),
                    message: t("multiRepo.unregisterConfirm").replace("{0}", r.name),
                    confirmLabel: t("multiRepo.unregister"),
                    danger: true,
                  });
                  if (ok) unregisterMut.mutate(r.path);
                }}
              />
            ))}
          </tbody>
        </table>
      </div>
    </div>
  );
}

function Stat({
  label,
  value,
  accent,
}: {
  label: string;
  value: number;
  accent?: string;
}) {
  return (
    <div
      style={{
        padding: "8px 12px",
        background: "var(--bg-2)",
        border: `1px solid var(--surface-border)`,
        borderLeft: accent ? `3px solid ${accent}` : "1px solid var(--surface-border)",
        borderRadius: 6,
      }}
    >
      <div style={{ fontSize: 11, fontWeight: 700, textTransform: "uppercase", color: "var(--text-3)" }}>
        {label}
      </div>
      <div style={{ fontSize: 18, fontWeight: 700, fontFamily: "var(--font-display)", color: accent ?? "var(--text-0)" }}>
        {value.toLocaleString()}
      </div>
    </div>
  );
}

function Th({
  label,
  onClick,
  active,
  asc,
  numeric,
}: {
  label: string;
  onClick: () => void;
  active: boolean;
  asc: boolean;
  numeric?: boolean;
}) {
  return (
    <th
      onClick={onClick}
      style={{
        padding: "8px 10px",
        textAlign: numeric ? "right" : "left",
        color: active ? "var(--accent)" : "var(--text-3)",
        fontWeight: 600,
        fontSize: 10,
        textTransform: "uppercase",
        cursor: "pointer",
        userSelect: "none",
        borderBottom: "1px solid var(--surface-border)",
        whiteSpace: "nowrap",
      }}
    >
      {label}
      {active && <span style={{ marginLeft: 4 }}>{asc ? "↑" : "↓"}</span>}
    </th>
  );
}

function RepoRow({
  repo,
  onPick,
  onRemove,
}: {
  repo: RepoOverview;
  onPick: () => void;
  onRemove: () => void;
}) {
  const { t } = useI18n();
  const tracingPct = Math.round(repo.tracingCoverage * 100);
  // Disable row-level click-to-select when the repo is broken: the row has
  // nothing useful to open, and we want the "Remove" button to be the
  // primary action.
  const disabled = !!repo.error;
  return (
    <tr
      onClick={disabled ? undefined : onPick}
      style={{
        cursor: disabled ? "default" : "pointer",
        background: "var(--bg-1)",
        borderBottom: "1px solid var(--surface-border)",
        opacity: disabled ? 0.7 : 1,
      }}
      onMouseEnter={(e) => {
        if (!disabled) e.currentTarget.style.background = "var(--surface-hover)";
      }}
      onMouseLeave={(e) => (e.currentTarget.style.background = "var(--bg-1)")}
    >
      <td style={tdStyle()}>
        <div style={{ display: "flex", alignItems: "center", gap: 6 }}>
          <Folder size={11} style={{ color: "var(--accent)", flexShrink: 0 }} />
          <div style={{ minWidth: 0, flex: 1 }}>
            <div style={{ fontWeight: 600 }}>{repo.name}</div>
            <div
              style={{
                fontSize: 10,
                color: "var(--text-3)",
                overflow: "hidden",
                textOverflow: "ellipsis",
                whiteSpace: "nowrap",
                maxWidth: 280,
              }}
            >
              {repo.path}
            </div>
            {repo.error && (
              <div
                style={{
                  fontSize: 10,
                  color: "var(--rose)",
                  display: "flex",
                  alignItems: "center",
                  gap: 6,
                  marginTop: 2,
                }}
              >
                <AlertCircle size={9} style={{ verticalAlign: -1, flexShrink: 0 }} />
                <span style={{ flex: 1 }}>{repo.error}</span>
                <button
                  onClick={(e) => {
                    e.stopPropagation();
                    onRemove();
                  }}
                  title={t("multiRepo.unregister")}
                  aria-label={t("multiRepo.unregister")}
                  style={{
                    display: "inline-flex",
                    alignItems: "center",
                    gap: 3,
                    padding: "2px 6px",
                    background: "transparent",
                    border: "1px solid var(--rose)",
                    borderRadius: 4,
                    color: "var(--rose)",
                    cursor: "pointer",
                    fontSize: 11,
                    fontWeight: 600,
                    flexShrink: 0,
                  }}
                >
                  <Trash2 size={9} />
                  {t("multiRepo.unregister")}
                </button>
              </div>
            )}
          </div>
        </div>
      </td>
      <td style={tdStyle("right")}>{repo.nodeCount.toLocaleString()}</td>
      <td style={tdStyle("right")}>{repo.fileCount.toLocaleString()}</td>
      <td style={tdStyle("right")}>
        <span style={{ color: repo.deadCount > 0 ? "var(--rose)" : "var(--text-3)" }}>
          {repo.deadCount}
        </span>
      </td>
      <td style={tdStyle("right")}>
        <span
          style={{
            color:
              tracingPct >= 70 ? "var(--green)" : tracingPct >= 40 ? "var(--amber)" : "var(--rose)",
          }}
        >
          {tracingPct}%
        </span>
      </td>
      <td style={tdStyle("right")}>
        <span style={{ color: "var(--text-3)" }}>
          {repo.indexDurationMs != null
            ? `${(repo.indexDurationMs / 1000).toFixed(1)}s`
            : "—"}
        </span>
      </td>
      <td style={tdStyle()}>
        <div style={{ display: "flex", flexWrap: "wrap", gap: 4 }}>
          {repo.languageBreakdown.slice(0, 3).map((l) => (
            <span
              key={l.language}
              style={{
                padding: "1px 6px",
                background: "var(--bg-3)",
                border: "1px solid var(--surface-border)",
                borderRadius: 4,
                fontSize: 11,
                color: "var(--text-2)",
              }}
            >
              {l.language} <span style={{ color: "var(--text-3)" }}>{l.fileCount}</span>
            </span>
          ))}
        </div>
      </td>
      <td style={tdStyle()}>
        <span style={{ fontSize: 10, color: "var(--text-3)" }}>{repo.indexedAt.slice(0, 10)}</span>
      </td>
    </tr>
  );
}

function tdStyle(align: "left" | "right" = "left"): React.CSSProperties {
  return {
    padding: "8px 10px",
    textAlign: align,
    color: "var(--text-1)",
    verticalAlign: "top",
    fontSize: 11,
    fontFamily: align === "right" ? "var(--font-mono)" : "inherit",
  };
}

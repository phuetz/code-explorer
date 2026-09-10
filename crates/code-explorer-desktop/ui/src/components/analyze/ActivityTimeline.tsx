/**
 * ActivityTimeline — horizontal sparkline of analyze runs.
 *
 * Bars are sized by node-count and tinted by dead-code drift. Hovering a
 * bar reveals the full snapshot stats. A "Snapshot now" button records the
 * current graph state on demand (no need to wait for analyze to do it).
 */

import { useMemo, useState } from "react";
import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import { Activity, Camera, Trash2 } from "lucide-react";
import { commands } from "../../lib/tauri-commands";
import { useAppStore } from "../../stores/app-store";
import { useI18n } from "../../hooks/use-i18n";
import { confirm } from "../../lib/confirm";
import type { ActivityEntry } from "../../lib/tauri-commands";
import { toast } from "sonner";

export function ActivityTimeline() {
  const { t } = useI18n();
  const queryClient = useQueryClient();
  const activeRepo = useAppStore((s) => s.activeRepo);
  const [hovered, setHovered] = useState<ActivityEntry | null>(null);

  const { data: entries = [] } = useQuery({
    queryKey: ["activity", activeRepo],
    queryFn: () => commands.activityList(),
    enabled: !!activeRepo,
    staleTime: 30_000,
  });

  const recordMut = useMutation({
    mutationFn: (note?: string) => commands.activityRecord(note),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["activity", activeRepo] });
      toast.success("Snapshot recorded");
    },
    onError: (e) => toast.error(`Snapshot failed: ${(e as Error).message}`),
  });

  const clearMut = useMutation({
    mutationFn: () => commands.activityClear(),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["activity", activeRepo] });
    },
  });

  const { maxNodes, maxDead } = useMemo(() => {
    let mn = 0;
    let md = 0;
    for (const e of entries) {
      if (e.nodeCount > mn) mn = e.nodeCount;
      if (e.deadCount > md) md = e.deadCount;
    }
    return { maxNodes: mn, maxDead: md };
  }, [entries]);

  if (!activeRepo) {
    return null;
  }

  return (
    <div
      style={{
        border: "1px solid var(--surface-border)",
        borderRadius: 8,
        background: "var(--surface)",
        padding: 12,
      }}
    >
      <div
        style={{
          display: "flex",
          alignItems: "center",
          gap: 8,
          marginBottom: 8,
        }}
      >
        <Activity size={14} style={{ color: "var(--accent)" }} />
        <span style={{ fontSize: 12, fontWeight: 600 }}>Activity timeline</span>
        <span style={{ fontSize: 10, color: "var(--text-3)" }}>
          {entries.length} entr{entries.length === 1 ? "y" : "ies"}
        </span>
        <div style={{ marginLeft: "auto", display: "flex", gap: 4 }}>
          <button
            onClick={() => {
              const note = window.prompt("Note (optional):") || undefined;
              recordMut.mutate(note);
            }}
            disabled={recordMut.isPending}
            title="Record current state as a new snapshot"
            style={btnStyle("var(--accent)")}
          >
            <Camera size={11} />
            <span>Snapshot now</span>
          </button>
          {entries.length > 0 && (
            <button
              onClick={async () => {
                const ok = await confirm({
                  title: t("confirm.deleteTitle"),
                  message: t("activity.clearConfirm"),
                  confirmLabel: t("activity.clear"),
                  danger: true,
                });
                if (ok) clearMut.mutate();
              }}
              title="Clear history"
              aria-label="Clear activity history"
              style={btnStyle()}
            >
              <Trash2 size={11} />
            </button>
          )}
        </div>
      </div>

      {entries.length === 0 ? (
        <div style={{ padding: "16px 0", fontSize: 11, color: "var(--text-3)", textAlign: "center" }}>
          No snapshots yet. Click "Snapshot now" to record the current state.
        </div>
      ) : (
        <>
          {/* Bars */}
          <div
            style={{
              display: "flex",
              alignItems: "flex-end",
              gap: 2,
              height: 64,
              padding: "0 4px",
              borderBottom: "1px solid var(--surface-border)",
            }}
          >
            {entries.map((e, i) => {
              const heightPct = maxNodes > 0 ? (e.nodeCount / maxNodes) * 100 : 0;
              const deadRatio = maxDead > 0 ? e.deadCount / maxDead : 0;
              // Tint from green (no dead) to red (max dead).
              const r = Math.round(158 + deadRatio * (247 - 158));
              const g = Math.round(206 + deadRatio * (118 - 206));
              const b = Math.round(106 + deadRatio * (142 - 106));
              return (
                <div
                  key={`${e.timestamp}-${i}`}
                  onMouseEnter={() => setHovered(e)}
                  onMouseLeave={() => setHovered(null)}
                  style={{
                    flex: 1,
                    minWidth: 4,
                    maxWidth: 24,
                    height: `${Math.max(2, heightPct)}%`,
                    background: `rgb(${r}, ${g}, ${b})`,
                    borderRadius: "3px 3px 0 0",
                    cursor: "pointer",
                    opacity: hovered && hovered.timestamp !== e.timestamp ? 0.4 : 1,
                    transition: "opacity 0.15s",
                  }}
                />
              );
            })}
          </div>

          {/* Legend / hover info */}
          <div style={{ marginTop: 6, fontSize: 10, color: "var(--text-3)", display: "flex", justifyContent: "space-between" }}>
            <span>{formatDate(entries[0].timestamp)}</span>
            <span>{formatDate(entries[entries.length - 1].timestamp)}</span>
          </div>

          {hovered && (
            <div
              style={{
                marginTop: 8,
                padding: "8px 10px",
                background: "var(--bg-2)",
                border: "1px solid var(--surface-border)",
                borderRadius: 6,
                fontSize: 11,
                display: "grid",
                gridTemplateColumns: "repeat(auto-fill, minmax(110px, 1fr))",
                gap: 6,
              }}
            >
              <Tile label="When" value={formatDateTime(hovered.timestamp)} />
              <Tile label="Nodes" value={hovered.nodeCount.toLocaleString()} />
              <Tile label="Edges" value={hovered.edgeCount.toLocaleString()} />
              <Tile label="Files" value={hovered.fileCount.toLocaleString()} />
              <Tile label="Functions" value={hovered.functionCount.toLocaleString()} />
              <Tile label="Dead" value={hovered.deadCount.toLocaleString()} accent="var(--rose)" />
              <Tile label="Traced" value={hovered.tracedCount.toLocaleString()} accent="var(--green)" />
              <Tile label="Communities" value={hovered.communityCount.toLocaleString()} />
              {hovered.note && (
                <div style={{ gridColumn: "1 / -1", color: "var(--text-2)", fontStyle: "italic" }}>
                  {hovered.note}
                </div>
              )}
            </div>
          )}
        </>
      )}
    </div>
  );
}

function Tile({
  label,
  value,
  accent,
}: {
  label: string;
  value: string;
  accent?: string;
}) {
  return (
    <div>
      <div
        style={{
          fontSize: 8,
          fontWeight: 700,
          textTransform: "uppercase",
          color: "var(--text-3)",
        }}
      >
        {label}
      </div>
      <div style={{ fontFamily: "var(--font-mono)", color: accent ?? "var(--text-1)" }}>
        {value}
      </div>
    </div>
  );
}

function btnStyle(color?: string): React.CSSProperties {
  return {
    padding: "3px 8px",
    background: color ?? "transparent",
    border: color ? "none" : "1px solid var(--surface-border)",
    borderRadius: 4,
    color: color ? "#fff" : "var(--text-3)",
    cursor: "pointer",
    fontSize: 10,
    fontWeight: 600,
    display: "inline-flex",
    alignItems: "center",
    gap: 4,
    fontFamily: "inherit",
  };
}

function formatDate(ms: number) {
  const d = new Date(ms);
  return d.toLocaleDateString([], { month: "short", day: "numeric" });
}
function formatDateTime(ms: number) {
  const d = new Date(ms);
  return d.toLocaleString([], { month: "short", day: "numeric", hour: "2-digit", minute: "2-digit" });
}

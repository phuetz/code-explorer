/**
 * DashboardPanel — user-defined dashboards of Cypher widgets.
 *
 * Three widget kinds:
 *   - metric: shows a single big number from the first row's value column
 *   - table: renders the result as a table
 *   - bar:   simple horizontal bar chart on (label, value) pairs
 *
 * Persistence: `<.codeexplorer>/dashboards/<id>.json`. List on the left, full
 * dashboard rendering on the right; edit mode lets the user reorder and
 * tweak widgets; saves are explicit.
 */

import { useState } from "react";
import {
  useMutation,
  useQuery,
  useQueryClient,
} from "@tanstack/react-query";
import {
  LayoutDashboard,
  Plus,
  Save,
  Trash2,
  RefreshCw,
  X,
  Hash,
  Table2,
  BarChart3,
  Pencil,
  Check as CheckIcon,
  ArrowUp,
  ArrowDown,
} from "lucide-react";
import { toast } from "sonner";
import { commands } from "../../lib/tauri-commands";
import { useAppStore } from "../../stores/app-store";
import { useI18n } from "../../hooks/use-i18n";
import { confirm } from "../../lib/confirm";
import type {
  Dashboard,
  DashboardSummary,
  DashboardWidget,
} from "../../lib/tauri-commands";

interface Props {
  open: boolean;
  onClose: () => void;
}

const STARTER_WIDGETS: DashboardWidget[] = [
  {
    id: "starter_metric",
    title: "Total functions",
    kind: "metric",
    cypher: "MATCH (n:Function) RETURN count(n) AS value",
    valueColumn: "value",
  },
  {
    id: "starter_bar",
    title: "Top files by complexity",
    kind: "bar",
    cypher:
      "MATCH (n:Function) WHERE n.complexity IS NOT NULL RETURN n.filePath AS label, n.complexity AS value ORDER BY n.complexity DESC LIMIT 8",
    labelColumn: "label",
    valueColumn: "value",
  },
];

function newWidget(): DashboardWidget {
  return {
    id: `w_${Date.now()}_${Math.random().toString(36).slice(2, 8)}`,
    title: "New widget",
    kind: "table",
    cypher: "MATCH (n:Function) RETURN n.name LIMIT 10",
  };
}

function blankDashboard(name: string): Dashboard {
  return {
    id: "",
    name,
    description: undefined,
    widgets: [...STARTER_WIDGETS],
    updatedAt: Date.now(),
  };
}

export function DashboardPanel({ open, onClose }: Props) {
  const { t } = useI18n();
  const queryClient = useQueryClient();
  const activeRepo = useAppStore((s) => s.activeRepo);
  const [active, setActive] = useState<Dashboard | null>(null);
  const [editing, setEditing] = useState(false);
  const [dirty, setDirty] = useState(false);

  const { data: list = [], refetch } = useQuery({
    queryKey: ["dashboards", activeRepo],
    queryFn: () => commands.dashboardList(),
    enabled: !!activeRepo && open,
    staleTime: 30_000,
  });

  const saveMut = useMutation({
    mutationFn: (d: Dashboard) => commands.dashboardSave(d),
    onSuccess: (summary) => {
      setActive((cur) => (cur ? { ...cur, id: summary.id, updatedAt: summary.updatedAt } : cur));
      queryClient.invalidateQueries({ queryKey: ["dashboards", activeRepo] });
      setDirty(false);
      toast.success(`Saved "${summary.name}"`);
    },
    onError: (e) => toast.error(`Save failed: ${(e as Error).message}`),
  });

  const deleteMut = useMutation({
    mutationFn: (id: string) => commands.dashboardDelete(id),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["dashboards", activeRepo] });
      setActive(null);
    },
  });

  // Reset state when the modal closes (derived-from-prop pattern per React 19 docs).
  const [prevOpen, setPrevOpen] = useState(open);
  if (prevOpen !== open) {
    setPrevOpen(open);
    if (!open) {
      setActive(null);
      setDirty(false);
      setEditing(false);
    }
  }

  const loadDashboard = async (id: string) => {
    try {
      const d = await commands.dashboardLoad(id);
      setActive(d);
      setDirty(false);
      setEditing(false);
    } catch (e) {
      toast.error(`Load failed: ${(e as Error).message}`);
    }
  };

  const newDashboard = () => {
    const name = window.prompt("Dashboard name:", "Untitled");
    if (!name) return;
    setActive(blankDashboard(name.trim()));
    setEditing(true);
    setDirty(true);
  };

  if (!open) return null;

  return (
    <div
      onClick={onClose}
      style={{
        position: "fixed",
        inset: 0,
        background: "rgba(0,0,0,0.5)",
        zIndex: 100,
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
      }}
    >
      <div
        onClick={(e) => e.stopPropagation()}
        style={{
          width: 1200,
          maxWidth: "95vw",
          height: "90vh",
          background: "var(--bg-1)",
          border: "1px solid var(--surface-border)",
          borderRadius: 12,
          boxShadow: "0 12px 48px rgba(0,0,0,0.6)",
          overflow: "hidden",
          display: "flex",
          position: "relative",
        }}
      >
        {/* Sidebar */}
        <div
          style={{
            width: 240,
            borderRight: "1px solid var(--surface-border)",
            background: "var(--bg-2)",
            display: "flex",
            flexDirection: "column",
          }}
        >
          <div
            style={{
              padding: "10px 12px",
              borderBottom: "1px solid var(--surface-border)",
              display: "flex",
              alignItems: "center",
              gap: 6,
            }}
          >
            <LayoutDashboard size={14} style={{ color: "var(--accent)" }} />
            <span style={{ fontSize: 12, fontWeight: 600 }}>{t("panel.dashboards.title")}</span>
            <button
              onClick={newDashboard}
              title={t("panel.dashboards.new")}
              aria-label={t("panel.dashboards.new")}
              style={iconBtn()}
            >
              <Plus size={11} />
            </button>
            <button
              onClick={() => refetch()}
              title="Refresh list"
              aria-label="Refresh dashboard list"
              style={iconBtn()}
            >
              <RefreshCw size={11} />
            </button>
          </div>
          <DashboardList
            list={list}
            activeId={active?.id ?? null}
            onPick={loadDashboard}
            onDelete={async (id) => {
              const ok = await confirm({
                title: t("confirm.deleteTitle"),
                message: t("panel.dashboards.deleteConfirm"),
                confirmLabel: t("confirm.delete"),
                danger: true,
              });
              if (ok) deleteMut.mutate(id);
            }}
          />
        </div>

        {/* Main area */}
        <div style={{ flex: 1, display: "flex", flexDirection: "column", overflow: "hidden" }}>
          {active ? (
            <DashboardEditor
              dashboard={active}
              editing={editing}
              dirty={dirty}
              onChange={(d) => {
                setActive(d);
                setDirty(true);
              }}
              onToggleEdit={() => setEditing((v) => !v)}
              onSave={() => active && saveMut.mutate(active)}
              isSaving={saveMut.isPending}
            />
          ) : (
            <div
              style={{
                flex: 1,
                display: "flex",
                flexDirection: "column",
                alignItems: "center",
                justifyContent: "center",
                color: "var(--text-3)",
                fontSize: 13,
                gap: 12,
              }}
            >
              <LayoutDashboard size={32} style={{ opacity: 0.4 }} />
              <div>{t("panel.dashboards.emptyHint")}</div>
              <button
                onClick={newDashboard}
                style={{
                  padding: "6px 14px",
                  background: "var(--accent)",
                  border: "none",
                  borderRadius: 6,
                  color: "#fff",
                  fontSize: 12,
                  fontWeight: 600,
                  cursor: "pointer",
                }}
              >
                + {t("panel.dashboards.new")}
              </button>
            </div>
          )}
        </div>

        <button
          onClick={onClose}
          aria-label="Close dashboards"
          style={{
            position: "absolute",
            top: 8,
            right: 8,
            padding: 6,
            background: "transparent",
            border: "none",
            color: "var(--text-3)",
            cursor: "pointer",
          }}
        >
          <X size={14} />
        </button>
      </div>
    </div>
  );
}

// ─── Sub-components ──────────────────────────────────────────────────

function DashboardList({
  list,
  activeId,
  onPick,
  onDelete,
}: {
  list: DashboardSummary[];
  activeId: string | null;
  onPick: (id: string) => void;
  onDelete: (id: string) => void;
}) {
  const { t } = useI18n();
  if (list.length === 0) {
    return (
      <div style={{ padding: 12, fontSize: 11, color: "var(--text-3)" }}>
        {t("panel.dashboards.empty")}
      </div>
    );
  }
  return (
    <div style={{ overflow: "auto" }}>
      {list.map((d) => (
        <div
          key={d.id}
          onClick={() => onPick(d.id)}
          style={{
            display: "flex",
            alignItems: "center",
            padding: "6px 12px",
            borderBottom: "1px solid var(--surface-border)",
            background: activeId === d.id ? "var(--surface-hover)" : "transparent",
            cursor: "pointer",
          }}
        >
          <div style={{ flex: 1, minWidth: 0 }}>
            <div style={{ fontSize: 12, fontWeight: 600 }}>{d.name}</div>
            <div style={{ fontSize: 10, color: "var(--text-3)" }}>
              {d.widgetCount} widget{d.widgetCount === 1 ? "" : "s"}
            </div>
          </div>
          <button
            onClick={(e) => {
              e.stopPropagation();
              onDelete(d.id);
            }}
            aria-label="Delete dashboard"
            style={iconBtn()}
          >
            <Trash2 size={11} />
          </button>
        </div>
      ))}
    </div>
  );
}

function DashboardEditor({
  dashboard,
  editing,
  dirty,
  onChange,
  onToggleEdit,
  onSave,
  isSaving,
}: {
  dashboard: Dashboard;
  editing: boolean;
  dirty: boolean;
  onChange: (d: Dashboard) => void;
  onToggleEdit: () => void;
  onSave: () => void;
  isSaving: boolean;
}) {
  const update = (patch: Partial<Dashboard>) => onChange({ ...dashboard, ...patch });
  const updateWidget = (idx: number, patch: Partial<DashboardWidget>) =>
    update({
      widgets: dashboard.widgets.map((w, i) => (i === idx ? { ...w, ...patch } : w)),
    });
  const addWidget = () => update({ widgets: [...dashboard.widgets, newWidget()] });
  const removeWidget = (idx: number) =>
    update({ widgets: dashboard.widgets.filter((_, i) => i !== idx) });
  const moveWidget = (idx: number, dir: -1 | 1) => {
    const target = idx + dir;
    if (target < 0 || target >= dashboard.widgets.length) return;
    const next = [...dashboard.widgets];
    [next[idx], next[target]] = [next[target], next[idx]];
    update({ widgets: next });
  };

  return (
    <>
      {/* Toolbar */}
      <div
        style={{
          padding: "10px 14px",
          borderBottom: "1px solid var(--surface-border)",
          display: "flex",
          alignItems: "center",
          gap: 8,
          background: "var(--bg-2)",
        }}
      >
        <span style={{ fontSize: 14, fontWeight: 700 }}>{dashboard.name}</span>
        {dirty && (
          <span
            style={{
              fontSize: 9,
              color: "var(--amber)",
              padding: "1px 6px",
              border: "1px solid #e0af68",
              borderRadius: 999,
              fontWeight: 600,
              textTransform: "uppercase",
            }}
          >
            unsaved
          </span>
        )}
        <div style={{ marginLeft: "auto", display: "flex", gap: 6 }}>
          {editing && (
            <button onClick={addWidget} title="Add widget" style={pillBtn()}>
              <Plus size={11} /> widget
            </button>
          )}
          <button
            onClick={onToggleEdit}
            title={editing ? "Done editing" : "Edit"}
            style={pillBtn(editing ? "var(--green)" : undefined)}
          >
            {editing ? <CheckIcon size={11} /> : <Pencil size={11} />}
            {editing ? "done" : "edit"}
          </button>
          <button onClick={onSave} disabled={isSaving} title="Save dashboard" style={pillBtn("var(--accent)")}>
            <Save size={11} />
            {isSaving ? "saving…" : "save"}
          </button>
        </div>
      </div>

      {/* Body */}
      <div
        style={{
          flex: 1,
          overflow: "auto",
          padding: 14,
          display: "grid",
          gridTemplateColumns: "repeat(auto-fill, minmax(360px, 1fr))",
          gap: 12,
          alignContent: "start",
        }}
      >
        {dashboard.widgets.map((w, idx) => (
          <WidgetCard
            key={w.id}
            widget={w}
            editing={editing}
            onChange={(patch) => updateWidget(idx, patch)}
            onMoveUp={() => moveWidget(idx, -1)}
            onMoveDown={() => moveWidget(idx, 1)}
            onRemove={() => removeWidget(idx)}
          />
        ))}
        {dashboard.widgets.length === 0 && (
          <div style={{ color: "var(--text-3)", fontSize: 12, textAlign: "center", padding: 32 }}>
            Empty dashboard. Click "edit" then "+ widget".
          </div>
        )}
      </div>
    </>
  );
}

function WidgetCard({
  widget,
  editing,
  onChange,
  onMoveUp,
  onMoveDown,
  onRemove,
}: {
  widget: DashboardWidget;
  editing: boolean;
  onChange: (patch: Partial<DashboardWidget>) => void;
  onMoveUp: () => void;
  onMoveDown: () => void;
  onRemove: () => void;
}) {
  const KindIcon =
    widget.kind === "metric" ? Hash : widget.kind === "bar" ? BarChart3 : Table2;

  // Run query when source / kind changes — keyed by cypher to avoid loops.
  const { data, isLoading, error, refetch } = useQuery({
    queryKey: ["widget", widget.id, widget.cypher],
    queryFn: () => commands.executeCypher(widget.cypher),
    enabled: !!widget.cypher.trim(),
    staleTime: 30_000,
    retry: false,
  });

  return (
    <div
      style={{
        border: "1px solid var(--surface-border)",
        borderRadius: 8,
        background: "var(--surface)",
        padding: 12,
        display: "flex",
        flexDirection: "column",
        gap: 8,
      }}
    >
      <div style={{ display: "flex", alignItems: "center", gap: 6 }}>
        <KindIcon size={12} style={{ color: "var(--accent)" }} />
        {editing ? (
          <input
            value={widget.title}
            onChange={(e) => onChange({ title: e.target.value })}
            style={{
              flex: 1,
              padding: "2px 6px",
              background: "var(--bg-2)",
              border: "1px solid var(--surface-border)",
              borderRadius: 4,
              color: "var(--text-0)",
              fontSize: 12,
              fontWeight: 600,
              outline: "none",
            }}
          />
        ) : (
          <span style={{ flex: 1, fontSize: 12, fontWeight: 600 }}>{widget.title}</span>
        )}
        <button onClick={() => refetch()} title="Re-run query" aria-label="Re-run" style={iconBtn()}>
          <RefreshCw size={10} />
        </button>
        {editing && (
          <>
            <button onClick={onMoveUp} title="Move up" aria-label="Move up" style={iconBtn()}>
              <ArrowUp size={10} />
            </button>
            <button onClick={onMoveDown} title="Move down" aria-label="Move down" style={iconBtn()}>
              <ArrowDown size={10} />
            </button>
            <button onClick={onRemove} title="Delete widget" aria-label="Delete widget" style={iconBtn("var(--rose)")}>
              <Trash2 size={10} />
            </button>
          </>
        )}
      </div>

      {editing && (
        <>
          <div style={{ display: "flex", gap: 4 }}>
            {(["metric", "table", "bar"] as const).map((k) => (
              <button
                key={k}
                onClick={() => onChange({ kind: k })}
                style={{
                  flex: 1,
                  padding: "3px 6px",
                  fontSize: 10,
                  fontWeight: 600,
                  borderRadius: 4,
                  border: "1px solid var(--surface-border)",
                  background: widget.kind === k ? "var(--accent)" : "transparent",
                  color: widget.kind === k ? "#fff" : "var(--text-3)",
                  cursor: "pointer",
                  fontFamily: "inherit",
                  textTransform: "uppercase",
                }}
              >
                {k}
              </button>
            ))}
          </div>
          <textarea
            value={widget.cypher}
            onChange={(e) => onChange({ cypher: e.target.value })}
            rows={3}
            style={{
              width: "100%",
              padding: "6px 8px",
              background: "var(--bg-2)",
              border: "1px solid var(--surface-border)",
              borderRadius: 4,
              color: "var(--text-0)",
              fontFamily: "var(--font-mono)",
              fontSize: 11,
              outline: "none",
              resize: "vertical",
            }}
          />
          {(widget.kind === "metric" || widget.kind === "bar") && (
            <input
              value={widget.valueColumn ?? ""}
              onChange={(e) => onChange({ valueColumn: e.target.value || undefined })}
              placeholder="value column (e.g. 'value')"
              style={miniInputStyle()}
            />
          )}
          {widget.kind === "bar" && (
            <input
              value={widget.labelColumn ?? ""}
              onChange={(e) => onChange({ labelColumn: e.target.value || undefined })}
              placeholder="label column (e.g. 'label')"
              style={miniInputStyle()}
            />
          )}
        </>
      )}

      <div style={{ minHeight: 80 }}>
        {isLoading && <div style={{ fontSize: 11, color: "var(--text-3)" }}>Loading…</div>}
        {error && (
          <div style={{ fontSize: 11, color: "var(--rose)", fontFamily: "var(--font-mono)" }}>
            {(error as Error).message}
          </div>
        )}
        {!isLoading && !error && data && <WidgetRenderer widget={widget} data={data} />}
      </div>
    </div>
  );
}

function WidgetRenderer({
  widget,
  data,
}: {
  widget: DashboardWidget;
  data: unknown;
}) {
  if (!Array.isArray(data) || data.length === 0) {
    return <div style={{ fontSize: 11, color: "var(--text-3)" }}>No rows.</div>;
  }
  const rows = data as Array<Record<string, unknown>>;

  if (widget.kind === "metric") {
    const valKey = widget.valueColumn ?? Object.keys(rows[0] ?? {})[0];
    const v = valKey ? rows[0][valKey] : null;
    return (
      <div
        style={{
          fontSize: 36,
          fontWeight: 700,
          fontFamily: "var(--font-display)",
          color: "var(--accent)",
          textAlign: "center",
        }}
      >
        {v == null ? "—" : typeof v === "number" ? v.toLocaleString() : String(v)}
      </div>
    );
  }

  if (widget.kind === "bar") {
    const labelKey = widget.labelColumn ?? Object.keys(rows[0] ?? {})[0];
    const valKey = widget.valueColumn ?? Object.keys(rows[0] ?? {})[1] ?? Object.keys(rows[0] ?? {})[0];
    const items = rows.slice(0, 12).map((r) => ({
      label: String(r[labelKey] ?? ""),
      value: Number(r[valKey] ?? 0),
    }));
    const max = items.reduce((m, it) => Math.max(m, it.value), 0) || 1;
    return (
      <div style={{ display: "flex", flexDirection: "column", gap: 3 }}>
        {items.map((it, i) => (
          <div key={i} style={{ display: "flex", alignItems: "center", gap: 6 }}>
            <span
              style={{
                width: 130,
                fontSize: 10,
                color: "var(--text-2)",
                whiteSpace: "nowrap",
                overflow: "hidden",
                textOverflow: "ellipsis",
              }}
              title={it.label}
            >
              {it.label}
            </span>
            <div style={{ flex: 1, height: 12, background: "var(--bg-2)", borderRadius: 2 }}>
              <div
                style={{
                  width: `${(it.value / max) * 100}%`,
                  height: "100%",
                  background: "var(--accent)",
                  borderRadius: 2,
                }}
              />
            </div>
            <span style={{ fontSize: 10, color: "var(--text-2)", fontFamily: "var(--font-mono)", minWidth: 40, textAlign: "right" }}>
              {it.value.toLocaleString()}
            </span>
          </div>
        ))}
      </div>
    );
  }

  // Default: table
  const keys = Object.keys(rows[0] ?? {});
  return (
    <div style={{ overflow: "auto", maxHeight: 220 }}>
      <table style={{ width: "100%", borderCollapse: "collapse", fontSize: 10 }}>
        <thead>
          <tr>
            {keys.map((k) => (
              <th key={k} style={{ textAlign: "left", padding: "3px 6px", color: "var(--text-3)", fontFamily: "var(--font-mono)", borderBottom: "1px solid var(--surface-border)", whiteSpace: "nowrap" }}>
                {k}
              </th>
            ))}
          </tr>
        </thead>
        <tbody>
          {rows.slice(0, 50).map((r, i) => (
            <tr key={i}>
              {keys.map((k) => (
                <td
                  key={k}
                  style={{
                    padding: "2px 6px",
                    color: "var(--text-1)",
                    fontFamily: "var(--font-mono)",
                    whiteSpace: "nowrap",
                    maxWidth: 180,
                    overflow: "hidden",
                    textOverflow: "ellipsis",
                    borderBottom: "1px solid var(--surface-border)",
                  }}
                >
                  {String(r[k] ?? "")}
                </td>
              ))}
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

function iconBtn(color?: string): React.CSSProperties {
  return {
    padding: 3,
    background: "transparent",
    border: "1px solid var(--surface-border)",
    borderRadius: 4,
    color: color ?? "var(--text-3)",
    cursor: "pointer",
    display: "inline-flex",
    alignItems: "center",
  };
}
function pillBtn(color?: string): React.CSSProperties {
  return {
    display: "inline-flex",
    alignItems: "center",
    gap: 4,
    padding: "3px 8px",
    background: color ?? "transparent",
    border: color ? "none" : "1px solid var(--surface-border)",
    borderRadius: 4,
    color: color ? "#fff" : "var(--text-2)",
    fontSize: 10,
    fontWeight: 600,
    cursor: "pointer",
    fontFamily: "inherit",
  };
}
function miniInputStyle(): React.CSSProperties {
  return {
    width: "100%",
    padding: "3px 6px",
    background: "var(--bg-2)",
    border: "1px solid var(--surface-border)",
    borderRadius: 4,
    color: "var(--text-0)",
    fontSize: 10,
    fontFamily: "var(--font-mono)",
    outline: "none",
  };
}

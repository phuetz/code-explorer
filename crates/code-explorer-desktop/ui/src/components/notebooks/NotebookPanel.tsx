/**
 * NotebookPanel — Cypher notebooks editor.
 *
 * Layout: vertical list of cells (markdown or cypher). Cypher cells can be
 * run individually (▶ button) or as a "Run all". Outputs are cached on the
 * cell and persisted with the notebook so the user can reopen without
 * re-running expensive queries.
 *
 * State strategy: working copy lives in component state; saves are explicit
 * (Save button) — so accidental tab-aways don't trash work-in-progress.
 */

import { useState, useCallback } from "react";
import {
  Plus,
  Play,
  PlayCircle,
  Trash2,
  Save,
  ArrowUp,
  ArrowDown,
  FileCode2,
  FileText as FileTextIcon,
  X,
  RefreshCw,
} from "lucide-react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";
import { commands } from "../../lib/tauri-commands";
import { useAppStore } from "../../stores/app-store";
import { useI18n } from "../../hooks/use-i18n";
import { confirm } from "../../lib/confirm";
import type {
  Notebook,
  NotebookCell,
  NotebookSummary,
} from "../../lib/tauri-commands";

interface Props {
  open: boolean;
  onClose: () => void;
}

function newCell(kind: "markdown" | "cypher"): NotebookCell {
  return {
    id: `c_${Date.now()}_${Math.random().toString(36).slice(2, 8)}`,
    kind,
    source:
      kind === "cypher"
        ? "MATCH (n:Function) RETURN n.name LIMIT 10"
        : "## New section\n\nWrite some context here.",
  };
}

function blankNotebook(name: string): Notebook {
  return {
    id: "",
    name,
    description: undefined,
    tags: [],
    cells: [newCell("markdown"), newCell("cypher")],
    updatedAt: Date.now(),
  };
}

export function NotebookPanel({ open, onClose }: Props) {
  const { t } = useI18n();
  const queryClient = useQueryClient();
  const activeRepo = useAppStore((s) => s.activeRepo);
  const [active, setActive] = useState<Notebook | null>(null);
  const [dirty, setDirty] = useState(false);

  const { data: list = [], refetch } = useQuery({
    queryKey: ["notebooks", activeRepo],
    queryFn: () => commands.notebookList(),
    enabled: !!activeRepo && open,
    staleTime: 30_000,
  });

  const saveMut = useMutation({
    mutationFn: (nb: Notebook) => commands.notebookSave(nb),
    onSuccess: (summary) => {
      // Backend assigned an id if this was a new notebook — sync it.
      setActive((cur) => (cur ? { ...cur, id: summary.id, updatedAt: summary.updatedAt } : cur));
      queryClient.invalidateQueries({ queryKey: ["notebooks", activeRepo] });
      setDirty(false);
      toast.success(`Saved "${summary.name}"`);
    },
    onError: (e) => toast.error(`Save failed: ${(e as Error).message}`),
  });

  const deleteMut = useMutation({
    mutationFn: (id: string) => commands.notebookDelete(id),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["notebooks", activeRepo] });
      setActive(null);
      toast.success("Notebook deleted");
    },
  });

  // Reset state when modal closes (derived-from-prop pattern per React 19 docs).
  const [prevOpen, setPrevOpen] = useState(open);
  if (prevOpen !== open) {
    setPrevOpen(open);
    if (!open) {
      setActive(null);
      setDirty(false);
    }
  }

  const loadNotebook = useCallback(async (id: string) => {
    try {
      const nb = await commands.notebookLoad(id);
      setActive(nb);
      setDirty(false);
    } catch (e) {
      toast.error(`Load failed: ${(e as Error).message}`);
    }
  }, []);

  const newNotebook = () => {
    const name = window.prompt("Notebook name:", "Untitled");
    if (!name) return;
    setActive(blankNotebook(name.trim()));
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
          width: 1100,
          maxWidth: "95vw",
          height: "85vh",
          background: "var(--bg-1)",
          border: "1px solid var(--surface-border)",
          borderRadius: 12,
          boxShadow: "0 12px 48px rgba(0,0,0,0.6)",
          overflow: "hidden",
          display: "flex",
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
            overflow: "hidden",
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
            <FileCode2 size={14} style={{ color: "var(--accent)" }} />
            <span style={{ fontSize: 12, fontWeight: 600 }}>{t("panel.notebooks.title")}</span>
            <button
              onClick={newNotebook}
              title={t("panel.notebooks.new")}
              aria-label={t("panel.notebooks.new")}
              style={{
                marginLeft: "auto",
                padding: 3,
                background: "transparent",
                border: "1px solid var(--surface-border)",
                borderRadius: 4,
                color: "var(--text-2)",
                cursor: "pointer",
              }}
            >
              <Plus size={11} />
            </button>
            <button
              onClick={() => refetch()}
              title="Refresh list"
              aria-label="Refresh notebook list"
              style={{
                padding: 3,
                background: "transparent",
                border: "1px solid var(--surface-border)",
                borderRadius: 4,
                color: "var(--text-2)",
                cursor: "pointer",
              }}
            >
              <RefreshCw size={11} />
            </button>
          </div>
          <NotebookList
            list={list}
            activeId={active?.id ?? null}
            onPick={loadNotebook}
            onDelete={(id) => deleteMut.mutate(id)}
          />
        </div>

        {/* Editor */}
        <div style={{ flex: 1, display: "flex", flexDirection: "column", overflow: "hidden" }}>
          {active ? (
            <NotebookEditor
              notebook={active}
              dirty={dirty}
              onChange={(nb) => {
                setActive(nb);
                setDirty(true);
              }}
              onSave={() => active && saveMut.mutate(active)}
              isSaving={saveMut.isPending}
              onClose={onClose}
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
              <FileCode2 size={32} style={{ opacity: 0.4 }} />
              <div>{t("panel.notebooks.emptyHint")}</div>
              <button
                onClick={newNotebook}
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
                + {t("panel.notebooks.new")}
              </button>
            </div>
          )}
        </div>

        {/* Close button */}
        <button
          onClick={onClose}
          aria-label="Close notebooks"
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

function NotebookList({
  list,
  activeId,
  onPick,
  onDelete,
}: {
  list: NotebookSummary[];
  activeId: string | null;
  onPick: (id: string) => void;
  onDelete: (id: string) => void;
}) {
  const { t } = useI18n();
  if (list.length === 0) {
    return (
      <div style={{ padding: 12, fontSize: 11, color: "var(--text-3)" }}>
        {t("panel.notebooks.empty")}
      </div>
    );
  }
  return (
    <div style={{ overflow: "auto" }}>
      {list.map((nb) => (
        <div
          key={nb.id}
          style={{
            display: "flex",
            alignItems: "center",
            padding: "6px 12px",
            borderBottom: "1px solid var(--surface-border)",
            background: activeId === nb.id ? "var(--surface-hover)" : "transparent",
            cursor: "pointer",
          }}
          onClick={() => onPick(nb.id)}
        >
          <div style={{ flex: 1, minWidth: 0 }}>
            <div style={{ fontSize: 12, fontWeight: 600 }}>{nb.name}</div>
            <div style={{ fontSize: 10, color: "var(--text-3)" }}>
              {nb.cellCount} cell{nb.cellCount === 1 ? "" : "s"}
            </div>
          </div>
          <button
            onClick={async (e) => {
              e.stopPropagation();
              const ok = await confirm({
                title: t("confirm.deleteTitle"),
                message: t("panel.notebooks.deleteConfirm").replace("{0}", nb.name),
                confirmLabel: t("confirm.delete"),
                danger: true,
              });
              if (ok) onDelete(nb.id);
            }}
            aria-label={t("panel.notebooks.delete")}
            style={{
              padding: 4,
              background: "transparent",
              border: "none",
              color: "var(--text-3)",
              cursor: "pointer",
            }}
          >
            <Trash2 size={11} />
          </button>
        </div>
      ))}
    </div>
  );
}

function NotebookEditor({
  notebook,
  dirty,
  onChange,
  onSave,
  isSaving,
}: {
  notebook: Notebook;
  dirty: boolean;
  onChange: (nb: Notebook) => void;
  onSave: () => void;
  isSaving: boolean;
  onClose: () => void;
}) {
  const updateCell = (idx: number, patch: Partial<NotebookCell>) => {
    onChange({
      ...notebook,
      cells: notebook.cells.map((c, i) => (i === idx ? { ...c, ...patch } : c)),
    });
  };
  const addCell = (kind: "markdown" | "cypher") => {
    onChange({ ...notebook, cells: [...notebook.cells, newCell(kind)] });
  };
  const removeCell = (idx: number) => {
    onChange({ ...notebook, cells: notebook.cells.filter((_, i) => i !== idx) });
  };
  const moveCell = (idx: number, dir: -1 | 1) => {
    const target = idx + dir;
    if (target < 0 || target >= notebook.cells.length) return;
    const next = [...notebook.cells];
    [next[idx], next[target]] = [next[target], next[idx]];
    onChange({ ...notebook, cells: next });
  };
  const renameNotebook = () => {
    const name = window.prompt("Rename notebook:", notebook.name);
    if (!name) return;
    onChange({ ...notebook, name: name.trim() });
  };

  const runCell = async (idx: number) => {
    const cell = notebook.cells[idx];
    if (cell.kind !== "cypher" || !cell.source.trim()) return;
    const t0 = performance.now();
    try {
      const out = await commands.executeCypher(cell.source);
      updateCell(idx, {
        cachedOutput: out,
        lastRunMs: Math.round(performance.now() - t0),
      });
    } catch (e) {
      updateCell(idx, {
        cachedOutput: { error: (e as Error).message },
        lastRunMs: Math.round(performance.now() - t0),
      });
    }
  };

  const runAll = async () => {
    for (let i = 0; i < notebook.cells.length; i++) {
      if (notebook.cells[i].kind === "cypher") {
        await runCell(i);
      }
    }
    toast.success("Ran all cells");
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
        <button
          onClick={renameNotebook}
          style={{
            background: "transparent",
            border: "none",
            color: "var(--text-0)",
            fontSize: 14,
            fontWeight: 700,
            cursor: "pointer",
            padding: 0,
            fontFamily: "inherit",
          }}
        >
          {notebook.name}
        </button>
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
          <button
            onClick={() => addCell("markdown")}
            title="Add Markdown cell"
            style={iconBtnStyle()}
          >
            <FileTextIcon size={11} />
            md
          </button>
          <button
            onClick={() => addCell("cypher")}
            title="Add Cypher cell"
            style={iconBtnStyle()}
          >
            <FileCode2 size={11} />
            cypher
          </button>
          <button onClick={runAll} title="Run all Cypher cells" style={iconBtnStyle("var(--green)")}>
            <PlayCircle size={11} />
            run all
          </button>
          <button
            onClick={onSave}
            disabled={isSaving}
            title="Save notebook"
            style={iconBtnStyle("var(--accent)")}
          >
            <Save size={11} />
            {isSaving ? "saving…" : "save"}
          </button>
        </div>
      </div>

      {/* Cells */}
      <div style={{ flex: 1, overflow: "auto", padding: 14 }}>
        {notebook.cells.map((cell, idx) => (
          <CellView
            key={cell.id}
            cell={cell}
            onChange={(patch) => updateCell(idx, patch)}
            onRun={() => runCell(idx)}
            onRemove={() => removeCell(idx)}
            onMoveUp={() => moveCell(idx, -1)}
            onMoveDown={() => moveCell(idx, 1)}
          />
        ))}
        {notebook.cells.length === 0 && (
          <div style={{ textAlign: "center", color: "var(--text-3)", fontSize: 12, padding: 32 }}>
            Empty notebook. Add a Markdown or Cypher cell.
          </div>
        )}
      </div>
    </>
  );
}

function CellView({
  cell,
  onChange,
  onRun,
  onRemove,
  onMoveUp,
  onMoveDown,
}: {
  cell: NotebookCell;
  onChange: (patch: Partial<NotebookCell>) => void;
  onRun: () => void;
  onRemove: () => void;
  onMoveUp: () => void;
  onMoveDown: () => void;
}) {
  const isCypher = cell.kind === "cypher";
  return (
    <div
      style={{
        marginBottom: 10,
        border: "1px solid var(--surface-border)",
        borderRadius: 8,
        background: "var(--bg-2)",
        overflow: "hidden",
      }}
    >
      <div
        style={{
          padding: "4px 8px",
          background: isCypher ? "rgba(122,162,247,0.08)" : "rgba(187,154,247,0.08)",
          borderBottom: "1px solid var(--surface-border)",
          fontSize: 9,
          fontWeight: 700,
          textTransform: "uppercase",
          color: isCypher ? "var(--accent)" : "#bb9af7",
          display: "flex",
          alignItems: "center",
          gap: 6,
        }}
      >
        {cell.kind}
        {cell.lastRunMs != null && (
          <span style={{ color: "var(--text-3)", fontWeight: 400, textTransform: "none" }}>
            · {cell.lastRunMs}ms
          </span>
        )}
        <div style={{ marginLeft: "auto", display: "flex", gap: 4 }}>
          {isCypher && (
            <button onClick={onRun} title="Run cell" aria-label="Run cell" style={cellBtnStyle("var(--green)")}>
              <Play size={10} />
            </button>
          )}
          <button onClick={onMoveUp} title="Move up" aria-label="Move cell up" style={cellBtnStyle()}>
            <ArrowUp size={10} />
          </button>
          <button onClick={onMoveDown} title="Move down" aria-label="Move cell down" style={cellBtnStyle()}>
            <ArrowDown size={10} />
          </button>
          <button onClick={onRemove} title="Delete cell" aria-label="Delete cell" style={cellBtnStyle("var(--rose)")}>
            <Trash2 size={10} />
          </button>
        </div>
      </div>
      <textarea
        value={cell.source}
        onChange={(e) => onChange({ source: e.target.value })}
        rows={isCypher ? 4 : 3}
        spellCheck={!isCypher}
        style={{
          width: "100%",
          padding: "8px 10px",
          background: "var(--bg-2)",
          border: "none",
          color: "var(--text-0)",
          fontFamily: isCypher ? "var(--font-mono)" : "inherit",
          fontSize: 12,
          resize: "vertical",
          outline: "none",
        }}
        onKeyDown={(e) => {
          if (isCypher && (e.ctrlKey || e.metaKey) && e.key === "Enter") {
            e.preventDefault();
            onRun();
          }
        }}
      />
      {isCypher && cell.cachedOutput != null && <CellOutput value={cell.cachedOutput} />}
    </div>
  );
}

function CellOutput({ value }: { value: unknown }) {
  if (
    value != null &&
    typeof value === "object" &&
    "error" in (value as Record<string, unknown>)
  ) {
    return (
      <div
        style={{
          padding: "6px 10px",
          borderTop: "1px solid var(--surface-border)",
          background: "rgba(247,118,142,0.08)",
          color: "var(--rose)",
          fontSize: 11,
          fontFamily: "var(--font-mono)",
        }}
      >
        Error: {String((value as { error: string }).error)}
      </div>
    );
  }
  if (Array.isArray(value)) {
    if (value.length === 0) {
      return (
        <div
          style={{
            padding: "6px 10px",
            borderTop: "1px solid var(--surface-border)",
            color: "var(--text-3)",
            fontSize: 11,
          }}
        >
          (empty result set)
        </div>
      );
    }
    if (typeof value[0] === "object" && value[0] != null) {
      const keys = Object.keys(value[0] as Record<string, unknown>);
      const rows = value.slice(0, 50) as Array<Record<string, unknown>>;
      return (
        <div
          style={{
            borderTop: "1px solid var(--surface-border)",
            background: "var(--bg-1)",
            maxHeight: 240,
            overflow: "auto",
          }}
        >
          <table style={{ width: "100%", borderCollapse: "collapse", fontSize: 11 }}>
            <thead>
              <tr>
                {keys.map((k) => (
                  <th
                    key={k}
                    style={{
                      padding: "4px 8px",
                      textAlign: "left",
                      color: "var(--text-3)",
                      fontWeight: 600,
                      borderBottom: "1px solid var(--surface-border)",
                      fontFamily: "var(--font-mono)",
                      whiteSpace: "nowrap",
                    }}
                  >
                    {k}
                  </th>
                ))}
              </tr>
            </thead>
            <tbody>
              {rows.map((row, i) => (
                <tr key={i}>
                  {keys.map((k) => (
                    <td
                      key={k}
                      style={{
                        padding: "3px 8px",
                        color: "var(--text-1)",
                        fontFamily: "var(--font-mono)",
                        fontSize: 10,
                        borderBottom: "1px solid var(--surface-border)",
                        whiteSpace: "nowrap",
                        maxWidth: 240,
                        overflow: "hidden",
                        textOverflow: "ellipsis",
                      }}
                    >
                      {String(row[k] ?? "")}
                    </td>
                  ))}
                </tr>
              ))}
            </tbody>
          </table>
          {value.length > 50 && (
            <div style={{ padding: "4px 8px", fontSize: 10, color: "var(--text-3)" }}>
              showing first 50 of {value.length} rows
            </div>
          )}
        </div>
      );
    }
  }
  return (
    <pre
      style={{
        padding: "6px 10px",
        borderTop: "1px solid var(--surface-border)",
        background: "var(--bg-1)",
        color: "var(--text-1)",
        fontFamily: "var(--font-mono)",
        fontSize: 11,
        whiteSpace: "pre-wrap",
        margin: 0,
        maxHeight: 200,
        overflow: "auto",
      }}
    >
      {JSON.stringify(value, null, 2)}
    </pre>
  );
}

function iconBtnStyle(color?: string): React.CSSProperties {
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

function cellBtnStyle(color?: string): React.CSSProperties {
  return {
    padding: 3,
    background: "transparent",
    border: "none",
    color: color ?? "var(--text-3)",
    cursor: "pointer",
    display: "inline-flex",
    alignItems: "center",
  };
}

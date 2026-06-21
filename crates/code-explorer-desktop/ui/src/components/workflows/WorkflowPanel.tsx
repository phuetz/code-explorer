/**
 * WorkflowPanel — DAG-style pipeline editor for chained graph operations.
 *
 * Each step has a typed kind (search / cypher / impact / read_file / llm)
 * and a JSON params object. Step outputs (text + structured JSON) are made
 * available to subsequent steps via `{{step_N.text}}` or
 * `{{step_N.json.path}}` placeholders that the backend resolves before
 * each step runs.
 *
 * UI: sidebar (workflow list) + main area (steps as cards, run-all button,
 * per-step result inline).
 */

import { useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import {
  Workflow as WorkflowIcon,
  Plus,
  Save,
  Play,
  Trash2,
  RefreshCw,
  X,
  ArrowUp,
  ArrowDown,
  Loader2,
  Search,
  Code2,
  Zap,
  FileText,
  Sparkles,
  CheckCircle2,
  AlertCircle,
} from "lucide-react";
import { toast } from "sonner";
import { commands } from "../../lib/tauri-commands";
import { useAppStore } from "../../stores/app-store";
import { useI18n } from "../../hooks/use-i18n";
import { confirm } from "../../lib/confirm";
import type {
  Workflow,
  WorkflowStep,
  WorkflowSummary,
  StepRun,
  WorkflowRunResult,
} from "../../lib/tauri-commands";

interface Props {
  open: boolean;
  onClose: () => void;
}

const STEP_KINDS = [
  { key: "search", label: "search", icon: Search, color: "var(--accent)", defaults: { query: "auth" } },
  { key: "cypher", label: "cypher", icon: Code2, color: "#bb9af7", defaults: { query: "MATCH (n:Function) RETURN n.name LIMIT 10" } },
  { key: "impact", label: "impact", icon: Zap, color: "var(--amber)", defaults: { target: "FooService", maxDepth: 3 } },
  { key: "read_file", label: "read_file", icon: FileText, color: "var(--green)", defaults: { path: "src/main.rs", maxBytes: 4000 } },
  { key: "llm", label: "llm", icon: Sparkles, color: "var(--rose)", defaults: { prompt: "Summarize the prior step outputs in 3 bullet points.", system: "You are a senior code reviewer." } },
] as const;

function newStep(kind: (typeof STEP_KINDS)[number]["key"]): WorkflowStep {
  const def = STEP_KINDS.find((k) => k.key === kind)!;
  return {
    id: `s_${Date.now()}_${Math.random().toString(36).slice(2, 8)}`,
    kind,
    label: def.label,
    params: { ...def.defaults },
  };
}

function blankWorkflow(name: string): Workflow {
  return {
    id: "",
    name,
    description: undefined,
    steps: [newStep("search"), newStep("llm")],
    updatedAt: Date.now(),
  };
}

export function WorkflowPanel({ open, onClose }: Props) {
  const { t } = useI18n();
  const queryClient = useQueryClient();
  const activeRepo = useAppStore((s) => s.activeRepo);
  const [active, setActive] = useState<Workflow | null>(null);
  const [dirty, setDirty] = useState(false);
  const [runResult, setRunResult] = useState<WorkflowRunResult | null>(null);

  const { data: list = [], refetch } = useQuery({
    queryKey: ["workflows", activeRepo],
    queryFn: () => commands.workflowList(),
    enabled: !!activeRepo && open,
    staleTime: 30_000,
  });

  const saveMut = useMutation({
    mutationFn: (wf: Workflow) => commands.workflowSave(wf),
    onSuccess: (summary) => {
      setActive((cur) => (cur ? { ...cur, id: summary.id, updatedAt: summary.updatedAt } : cur));
      queryClient.invalidateQueries({ queryKey: ["workflows", activeRepo] });
      setDirty(false);
      toast.success(`Saved "${summary.name}"`);
    },
    onError: (e) => toast.error(`Save failed: ${(e as Error).message}`),
  });

  const deleteMut = useMutation({
    mutationFn: (id: string) => commands.workflowDelete(id),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["workflows", activeRepo] });
      setActive(null);
      setRunResult(null);
    },
  });

  const runMut = useMutation({
    mutationFn: (wf: Workflow) => commands.workflowRun(wf),
    onSuccess: (result) => {
      setRunResult(result);
      const errs = result.steps.filter((s) => s.status === "error").length;
      if (errs > 0) toast.error(`Workflow ran with ${errs} error(s)`);
      else toast.success(`Workflow done in ${result.totalMs}ms`);
    },
    onError: (e) => toast.error(`Run failed: ${(e as Error).message}`),
  });

  // Reset state when modal closes (derived-from-prop pattern per React 19 docs).
  const [prevOpen, setPrevOpen] = useState(open);
  if (prevOpen !== open) {
    setPrevOpen(open);
    if (!open) {
      setActive(null);
      setDirty(false);
      setRunResult(null);
    }
  }

  const loadWorkflow = async (id: string) => {
    try {
      const wf = await commands.workflowLoad(id);
      setActive(wf);
      setDirty(false);
      setRunResult(null);
    } catch (e) {
      toast.error(`Load failed: ${(e as Error).message}`);
    }
  };

  const newWorkflow = () => {
    const name = window.prompt("Workflow name:", "Untitled");
    if (!name) return;
    setActive(blankWorkflow(name.trim()));
    setDirty(true);
    setRunResult(null);
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
            <WorkflowIcon size={14} style={{ color: "var(--accent)" }} />
            <span style={{ fontSize: 12, fontWeight: 600 }}>{t("panel.workflows.title")}</span>
            <button onClick={newWorkflow} title={t("panel.workflows.new")} aria-label={t("panel.workflows.new")} style={iconBtn()}>
              <Plus size={11} />
            </button>
            <button onClick={() => refetch()} title="Refresh" aria-label="Refresh" style={iconBtn()}>
              <RefreshCw size={11} />
            </button>
          </div>
          <WorkflowList
            list={list}
            activeId={active?.id ?? null}
            onPick={loadWorkflow}
            onDelete={async (id) => {
              const ok = await confirm({
                title: t("confirm.deleteTitle"),
                message: t("panel.workflows.deleteConfirm"),
                confirmLabel: t("confirm.delete"),
                danger: true,
              });
              if (ok) deleteMut.mutate(id);
            }}
          />
        </div>

        {/* Editor */}
        <div style={{ flex: 1, display: "flex", flexDirection: "column", overflow: "hidden" }}>
          {active ? (
            <WorkflowEditor
              workflow={active}
              dirty={dirty}
              isRunning={runMut.isPending}
              runResult={runResult}
              onChange={(wf) => {
                setActive(wf);
                setDirty(true);
              }}
              onRun={() => active && runMut.mutate(active)}
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
              <WorkflowIcon size={32} style={{ opacity: 0.4 }} />
              <div>{t("panel.workflows.emptyHint")}</div>
              <button
                onClick={newWorkflow}
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
                + {t("panel.workflows.new")}
              </button>
            </div>
          )}
        </div>

        <button
          onClick={onClose}
          aria-label="Close workflows"
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

function WorkflowList({
  list,
  activeId,
  onPick,
  onDelete,
}: {
  list: WorkflowSummary[];
  activeId: string | null;
  onPick: (id: string) => void;
  onDelete: (id: string) => void;
}) {
  const { t } = useI18n();
  if (list.length === 0) {
    return <div style={{ padding: 12, fontSize: 11, color: "var(--text-3)" }}>{t("panel.workflows.empty")}</div>;
  }
  return (
    <div style={{ overflow: "auto" }}>
      {list.map((wf) => (
        <div
          key={wf.id}
          onClick={() => onPick(wf.id)}
          style={{
            display: "flex",
            alignItems: "center",
            padding: "6px 12px",
            borderBottom: "1px solid var(--surface-border)",
            background: activeId === wf.id ? "var(--surface-hover)" : "transparent",
            cursor: "pointer",
          }}
        >
          <div style={{ flex: 1, minWidth: 0 }}>
            <div style={{ fontSize: 12, fontWeight: 600 }}>{wf.name}</div>
            <div style={{ fontSize: 10, color: "var(--text-3)" }}>
              {wf.stepCount} step{wf.stepCount === 1 ? "" : "s"}
            </div>
          </div>
          <button
            onClick={(e) => {
              e.stopPropagation();
              onDelete(wf.id);
            }}
            style={iconBtn()}
            aria-label="Delete workflow"
          >
            <Trash2 size={11} />
          </button>
        </div>
      ))}
    </div>
  );
}

function WorkflowEditor({
  workflow,
  dirty,
  isRunning,
  runResult,
  onChange,
  onRun,
  onSave,
  isSaving,
}: {
  workflow: Workflow;
  dirty: boolean;
  isRunning: boolean;
  runResult: WorkflowRunResult | null;
  onChange: (wf: Workflow) => void;
  onRun: () => void;
  onSave: () => void;
  isSaving: boolean;
}) {
  const renameWorkflow = () => {
    const name = window.prompt("Rename workflow:", workflow.name);
    if (!name) return;
    onChange({ ...workflow, name: name.trim() });
  };

  const updateStep = (idx: number, patch: Partial<WorkflowStep>) => {
    onChange({
      ...workflow,
      steps: workflow.steps.map((s, i) => (i === idx ? { ...s, ...patch } : s)),
    });
  };
  const addStep = (kind: (typeof STEP_KINDS)[number]["key"]) => {
    onChange({ ...workflow, steps: [...workflow.steps, newStep(kind)] });
  };
  const removeStep = (idx: number) =>
    onChange({ ...workflow, steps: workflow.steps.filter((_, i) => i !== idx) });
  const moveStep = (idx: number, dir: -1 | 1) => {
    const target = idx + dir;
    if (target < 0 || target >= workflow.steps.length) return;
    const next = [...workflow.steps];
    [next[idx], next[target]] = [next[target], next[idx]];
    onChange({ ...workflow, steps: next });
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
          onClick={renameWorkflow}
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
          {workflow.name}
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
          {/* Add step kind chips */}
          {STEP_KINDS.map((k) => {
            const Icon = k.icon;
            return (
              <button
                key={k.key}
                onClick={() => addStep(k.key)}
                title={`Add ${k.label} step`}
                style={pillBtn(k.color)}
              >
                <Icon size={11} />
                {k.label}
              </button>
            );
          })}
          <button onClick={onRun} disabled={isRunning} style={pillBtn("var(--green)")}>
            {isRunning ? <Loader2 size={11} className="animate-spin" /> : <Play size={11} />}
            {isRunning ? "running…" : "run all"}
          </button>
          <button onClick={onSave} disabled={isSaving} style={pillBtn("var(--accent)")}>
            <Save size={11} />
            {isSaving ? "saving…" : "save"}
          </button>
        </div>
      </div>

      {/* Steps */}
      <div style={{ flex: 1, overflow: "auto", padding: 14 }}>
        {workflow.steps.map((step, idx) => (
          <StepCard
            key={step.id}
            idx={idx}
            step={step}
            run={runResult?.steps[idx] ?? null}
            onChange={(patch) => updateStep(idx, patch)}
            onMoveUp={() => moveStep(idx, -1)}
            onMoveDown={() => moveStep(idx, 1)}
            onRemove={() => removeStep(idx)}
          />
        ))}
        {workflow.steps.length === 0 && (
          <div
            style={{
              padding: 32,
              textAlign: "center",
              color: "var(--text-3)",
              fontSize: 12,
            }}
          >
            Empty workflow. Add steps using the chips above.
          </div>
        )}
        {runResult && (
          <div
            style={{
              marginTop: 8,
              padding: "6px 10px",
              fontSize: 11,
              color: "var(--text-3)",
              borderTop: "1px solid var(--surface-border)",
            }}
          >
            Total: {runResult.totalMs}ms · {runResult.steps.length} step(s)
          </div>
        )}
      </div>
    </>
  );
}

function StepCard({
  idx,
  step,
  run,
  onChange,
  onMoveUp,
  onMoveDown,
  onRemove,
}: {
  idx: number;
  step: WorkflowStep;
  run: StepRun | null;
  onChange: (patch: Partial<WorkflowStep>) => void;
  onMoveUp: () => void;
  onMoveDown: () => void;
  onRemove: () => void;
}) {
  const def = STEP_KINDS.find((k) => k.key === step.kind);
  const Icon = def?.icon ?? Code2;
  const color = def?.color ?? "var(--accent)";

  const updateParam = (key: string, value: unknown) => {
    onChange({ params: { ...step.params, [key]: value } });
  };

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
          padding: "5px 10px",
          background: `${color}14`,
          borderBottom: "1px solid var(--surface-border)",
          display: "flex",
          alignItems: "center",
          gap: 6,
        }}
      >
        <span
          style={{
            fontSize: 9,
            fontWeight: 700,
            color: "var(--text-3)",
            fontFamily: "var(--font-mono)",
          }}
        >
          step_{idx + 1}
        </span>
        <Icon size={12} style={{ color }} />
        <input
          value={step.label}
          onChange={(e) => onChange({ label: e.target.value })}
          style={{
            flex: 1,
            padding: "1px 6px",
            background: "transparent",
            border: "none",
            color: "var(--text-0)",
            fontSize: 12,
            fontWeight: 600,
            outline: "none",
          }}
        />
        {run && (
          <span
            style={{
              fontSize: 9,
              color: run.status === "ok" ? "var(--green)" : run.status === "error" ? "var(--rose)" : "var(--text-3)",
              display: "inline-flex",
              alignItems: "center",
              gap: 3,
            }}
          >
            {run.status === "ok" ? <CheckCircle2 size={10} /> : <AlertCircle size={10} />}
            {run.durationMs}ms
          </span>
        )}
        <button onClick={onMoveUp} title="Move up" style={iconBtn()}>
          <ArrowUp size={10} />
        </button>
        <button onClick={onMoveDown} title="Move down" style={iconBtn()}>
          <ArrowDown size={10} />
        </button>
        <button onClick={onRemove} title="Delete step" style={iconBtn("var(--rose)")}>
          <Trash2 size={10} />
        </button>
      </div>

      {/* Params editor — kind-aware */}
      <div style={{ padding: "8px 10px" }}>
        <ParamsEditor step={step} updateParam={updateParam} />
      </div>

      {run && (
        <div
          style={{
            padding: "6px 10px",
            borderTop: "1px solid var(--surface-border)",
            background: run.status === "error" ? "rgba(247,118,142,0.08)" : "var(--bg-1)",
            fontSize: 11,
          }}
        >
          {run.error ? (
            <div style={{ color: "var(--rose)", fontFamily: "var(--font-mono)" }}>{run.error}</div>
          ) : (
            <pre
              style={{
                margin: 0,
                whiteSpace: "pre-wrap",
                fontFamily: "var(--font-mono)",
                color: "var(--text-1)",
                maxHeight: 200,
                overflow: "auto",
              }}
            >
              {run.text}
            </pre>
          )}
        </div>
      )}
    </div>
  );
}

function ParamsEditor({
  step,
  updateParam,
}: {
  step: WorkflowStep;
  updateParam: (key: string, value: unknown) => void;
}) {
  const params = step.params as Record<string, unknown>;
  const fields: Array<{ key: string; placeholder: string; multiline?: boolean }> = (() => {
    switch (step.kind) {
      case "search":
        return [{ key: "query", placeholder: "Search query (substring on name)" }];
      case "cypher":
        return [{ key: "query", placeholder: "MATCH (n) RETURN n LIMIT 10", multiline: true }];
      case "impact":
        return [
          { key: "target", placeholder: "Symbol name or node ID" },
          { key: "maxDepth", placeholder: "Max BFS depth (default 3)" },
        ];
      case "read_file":
        return [
          { key: "path", placeholder: "Relative path within repo" },
          { key: "maxBytes", placeholder: "Max bytes (default 8000)" },
        ];
      case "llm":
        return [
          { key: "system", placeholder: "System prompt (role)", multiline: true },
          { key: "prompt", placeholder: "User prompt — supports {{step_N.text}}", multiline: true },
        ];
      default:
        return [];
    }
  })();
  return (
    <div style={{ display: "flex", flexDirection: "column", gap: 4 }}>
      {fields.map((f) => {
        const value = String(params[f.key] ?? "");
        if (f.multiline) {
          return (
            <textarea
              key={f.key}
              value={value}
              onChange={(e) => updateParam(f.key, e.target.value)}
              placeholder={f.placeholder}
              rows={3}
              style={{
                width: "100%",
                padding: "4px 6px",
                background: "var(--bg-1)",
                border: "1px solid var(--surface-border)",
                borderRadius: 4,
                color: "var(--text-0)",
                fontFamily: "var(--font-mono)",
                fontSize: 11,
                outline: "none",
                resize: "vertical",
              }}
            />
          );
        }
        return (
          <input
            key={f.key}
            value={value}
            onChange={(e) => {
              const v = e.target.value;
              // Coerce number-ish keys (maxDepth, maxBytes) when applicable.
              if (f.key === "maxDepth" || f.key === "maxBytes") {
                const n = Number(v);
                updateParam(f.key, Number.isFinite(n) ? n : v);
              } else {
                updateParam(f.key, v);
              }
            }}
            placeholder={f.placeholder}
            style={{
              width: "100%",
              padding: "4px 6px",
              background: "var(--bg-1)",
              border: "1px solid var(--surface-border)",
              borderRadius: 4,
              color: "var(--text-0)",
              fontFamily: "var(--font-mono)",
              fontSize: 11,
              outline: "none",
            }}
          />
        );
      })}
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

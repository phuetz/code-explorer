/**
 * ResearchPlanViewer — Displays the multi-step research plan execution.
 *
 * Shows:
 * - Query analysis summary (complexity badge, tools, reasoning)
 * - Step-by-step progress with live status updates
 * - Collapsible step details with results
 * - Duration and source count per step
 *
 * Inspired by Manus AI's step execution UI.
 */

import { useState, useMemo } from "react";
import {
  ChevronDown,
  ChevronRight,
  Check,
  Loader2,
  XCircle,
  SkipForward,
  Clock,
  Braces,
  Search,
  GitBranch,
  FileCode,
  Database,
  Microscope,
  Zap,
  AlertTriangle,
} from "lucide-react";
import type { ResearchPlan, ResearchStep, StepStatus, QueryComplexity } from "../../lib/tauri-commands";
import { useI18n } from "../../hooks/use-i18n";

interface ResearchPlanViewerProps {
  plan: ResearchPlan;
  compact?: boolean;
}

export function ResearchPlanViewer({ plan, compact = false }: ResearchPlanViewerProps) {
  const { t } = useI18n();
  const completedSteps = plan.steps.filter((s) => s.status === "completed").length;
  const totalSteps = plan.steps.length;
  const progressPercent = totalSteps > 0 ? (completedSteps / totalSteps) * 100 : 0;

  const totalDuration = useMemo(
    () =>
      plan.steps.reduce(
        (sum, s) => sum + (s.result?.durationMs ?? 0),
        0
      ),
    [plan.steps]
  );

  return (
    <div
      className="rounded-lg overflow-hidden"
      style={{ background: "var(--surface)", border: "1px solid var(--surface-border)" }}
    >
      {/* Header */}
      <div className="px-3 py-2 flex items-center gap-2">
        <Microscope size={14} style={{ color: "var(--purple)" }} />
        <span className="text-[12px] font-medium" style={{ color: "var(--text-0)" }}>
          {t("research.planTitle")}
        </span>
        <ComplexityBadge complexity={plan.analysis.complexity} />
        <span className="text-[11px]" style={{ color: "var(--text-3)" }}>
          {completedSteps}/{totalSteps} steps
        </span>
        {totalDuration > 0 && (
          <span className="text-[11px] flex items-center gap-1 ml-auto" style={{ color: "var(--text-3)" }}>
            <Clock size={10} />
            {formatDuration(totalDuration)}
          </span>
        )}
      </div>

      {/* Progress bar */}
      <div className="h-1" style={{ background: "var(--bg-3)" }}>
        <div
          className="h-full transition-all duration-500"
          style={{
            width: `${progressPercent}%`,
            background: plan.status === "failed" ? "var(--red)" : "var(--purple)",
          }}
        />
      </div>

      {/* Analysis summary (collapsible) */}
      {!compact && (
        <div
          className="px-3 py-2 text-[11px]"
          style={{ borderBottom: "1px solid var(--surface-border)", color: "var(--text-2)" }}
        >
          <div className="flex items-center gap-1.5 mb-1">
            <Zap size={10} style={{ color: "var(--accent)" }} />
            <span>{plan.analysis.reasoning}</span>
          </div>
          <div className="flex gap-1 flex-wrap">
            {plan.analysis.suggestedTools.map((tool) => (
              <span
                key={tool}
                className="px-1.5 py-0.5 rounded text-[10px]"
                style={{
                  background: "var(--bg-3)",
                  color: "var(--text-2)",
                }}
              >
                {TOOL_LABELS[tool] || tool}
              </span>
            ))}
          </div>
        </div>
      )}

      {/* Steps */}
      <div className="divide-y" style={{ borderColor: "var(--surface-border)" }}>
        {plan.steps.map((step) => (
          <StepRow key={step.id} step={step} compact={compact} />
        ))}
      </div>
    </div>
  );
}

// ─── StepRow ────────────────────────────────────────────────────────

function StepRow({ step, compact }: { step: ResearchStep; compact: boolean }) {
  const [expanded, setExpanded] = useState(false);
  const ToolIcon = TOOL_ICONS[step.tool] || Search;

  return (
    <div>
      <button
        onClick={() => setExpanded(!expanded)}
        className="w-full flex items-center gap-2 px-3 py-2 text-left transition-colors hover:bg-[var(--bg-1)]"
      >
        {/* Status indicator */}
        <StepStatusIcon status={step.status} />

        {/* Tool icon */}
        <ToolIcon size={12} style={{ color: "var(--text-2)", flexShrink: 0 }} />

        {/* Description */}
        <span
          className="flex-1 text-[12px] truncate"
          style={{
            color: step.status === "skipped" ? "var(--text-3)" : "var(--text-1)",
            textDecoration: step.status === "skipped" ? "line-through" : "none",
          }}
        >
          {step.description}
        </span>

        {/* Duration */}
        {step.result?.durationMs != null && step.result.durationMs > 0 && (
          <span className="text-[10px] flex-shrink-0" style={{ color: "var(--text-3)" }}>
            {formatDuration(step.result.durationMs)}
          </span>
        )}

        {/* Source count */}
        {step.result && step.result.sources.length > 0 && (
          <span className="text-[10px] flex-shrink-0" style={{ color: "var(--text-3)" }}>
            {step.result.sources.length} src
          </span>
        )}

        {/* Expand chevron */}
        {step.result && !compact && (
          expanded ? (
            <ChevronDown size={12} style={{ color: "var(--text-3)" }} />
          ) : (
            <ChevronRight size={12} style={{ color: "var(--text-3)" }} />
          )
        )}
      </button>

      {/* Expanded details */}
      {expanded && step.result && (
        <div
          className="px-3 pb-2 pl-8 text-[11px]"
          style={{ color: "var(--text-2)" }}
        >
          <p className="mb-1">{step.result.summary}</p>

          {/* Sources list */}
          {step.result.sources.length > 0 && (
            <div className="mt-1 space-y-0.5">
              {step.result.sources.slice(0, 5).map((source) => (
                <div key={`${source.nodeId}-${source.symbolName}`} className="flex items-center gap-1.5">
                  <FileCode size={9} style={{ color: "var(--accent)" }} />
                  <span style={{ fontFamily: "var(--font-mono)", color: "var(--text-1)" }}>
                    {source.symbolName}
                  </span>
                  <span style={{ color: "var(--text-3)" }}>
                    {source.symbolType} · {source.filePath}
                  </span>
                </div>
              ))}
              {step.result.sources.length > 5 && (
                <span style={{ color: "var(--text-3)" }}>
                  ...and {step.result.sources.length - 5} more
                </span>
              )}
            </div>
          )}
        </div>
      )}
    </div>
  );
}

// ─── Status Icon ────────────────────────────────────────────────────

function StepStatusIcon({ status }: { status: StepStatus }) {
  switch (status) {
    case "completed":
      return <Check size={12} style={{ color: "var(--green)", flexShrink: 0 }} />;
    case "running":
      return <Loader2 size={12} className="animate-spin" style={{ color: "var(--purple)", flexShrink: 0 }} />;
    case "failed":
      return <XCircle size={12} style={{ color: "var(--red)", flexShrink: 0 }} />;
    case "skipped":
      return <SkipForward size={12} style={{ color: "var(--text-3)", flexShrink: 0 }} />;
    default:
      return (
        <div
          className="w-3 h-3 rounded-full flex-shrink-0"
          style={{ border: "1.5px solid var(--text-3)" }}
        />
      );
  }
}

// ─── Complexity Badge ───────────────────────────────────────────────

function ComplexityBadge({ complexity }: { complexity: QueryComplexity }) {
  const configs: Record<string, { label: string; color: string; icon: typeof Zap }> = {
    simple: { label: "Simple", color: "var(--green)", icon: Zap },
    medium: { label: "Medium", color: "var(--orange)", icon: AlertTriangle },
    complex: { label: "Complex", color: "var(--purple)", icon: Microscope },
  };
  const config = configs[complexity] ?? configs.simple;

  const Icon = config.icon;

  return (
    <span
      className="inline-flex items-center gap-0.5 px-1.5 py-0.5 rounded text-[10px] font-medium"
      style={{
        background: `color-mix(in srgb, ${config.color} 10%, transparent)`,
        color: config.color,
      }}
    >
      <Icon size={9} />
      {config.label}
    </span>
  );
}

// ─── Helpers ────────────────────────────────────────────────────────

function formatDuration(ms: number): string {
  if (ms < 1000) return `${ms}ms`;
  return `${(ms / 1000).toFixed(1)}s`;
}

const TOOL_ICONS: Record<string, typeof Search> = {
  search_symbols: Search,
  get_symbol_context: GitBranch,
  get_impact_analysis: Braces,
  read_file_content: FileCode,
  execute_cypher: Database,
};

// Tool labels are resolved via i18n at render time (see TOOL_LABELS usage in the component).
// Fallbacks used here are the English defaults when i18n key is missing.
const TOOL_LABELS: Record<string, string> = {
  search_symbols: "Symbol Search",
  get_symbol_context: "Context Analysis",
  get_impact_analysis: "Impact Analysis",
  read_file_content: "File Reader",
  execute_cypher: "Graph Query",
};

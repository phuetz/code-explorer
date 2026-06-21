import { useState, useEffect } from "react";
import { FolderOpen, Loader2, CheckCircle2, AlertCircle, X } from "lucide-react";
import { isTauri } from "../../lib/tauri-env";
import { useI18n } from "../../hooks/use-i18n";
import type { PipelineProgress, PipelinePhase } from "../../lib/tauri-commands";
import { PHASE_WEIGHTS } from "../../lib/tauri-commands";

/** Ordered phases for the stepper display */
const PIPELINE_STEPS: PipelinePhase[] = [
  "structure",
  "parsing",
  "imports",
  "calls",
  "heritage",
  "communities",
  "processes",
];

/** Compute overall progress (0–100) based on phase weights */
function computeOverallProgress(phase: PipelinePhase, phasePercent: number): number {
  if (phase === "complete") return 100;
  if (phase === "error") return 0;

  let accumulated = 0;
  for (const step of PIPELINE_STEPS) {
    const weight = PHASE_WEIGHTS[step] ?? 0;
    if (step === phase) {
      return accumulated + (weight * phasePercent) / 100;
    }
    accumulated += weight;
  }
  return accumulated;
}

interface AnalyzeProgressProps {
  isAnalyzing: boolean;
  repoPath: string | null;
  onComplete: () => void;
  onDismiss: () => void;
}

export function AnalyzeProgress({ isAnalyzing, repoPath, onComplete, onDismiss }: AnalyzeProgressProps) {
  const { t } = useI18n();
  const [progress, setProgress] = useState<PipelineProgress | null>(null);
  const [overallPercent, setOverallPercent] = useState(0);
  const [completed, setCompleted] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // Listen to pipeline-progress events from Tauri backend
  useEffect(() => {
    if (!isAnalyzing) return;

    setProgress(null);
    setOverallPercent(0);
    setCompleted(false);
    setError(null);

    if (!isTauri()) return;

    let cancelled = false;
    let unlistenFn: (() => void) | null = null;
    let completeTimer: ReturnType<typeof setTimeout> | null = null;

    import("@tauri-apps/api/event").then((mod) =>
      mod.listen<PipelineProgress>("pipeline-progress", (event) => {
        if (cancelled) return;
        const p = event.payload;
        setProgress(p);

        if (p.phase === "complete") {
          setOverallPercent(100);
          setCompleted(true);
          completeTimer = setTimeout(() => { if (!cancelled) onComplete(); }, 1500);
        } else if (p.phase === "error") {
          setError(p.message);
        } else {
          setOverallPercent(computeOverallProgress(p.phase, p.percent));
        }
      })
    ).then((fn) => {
      if (cancelled) {
        fn(); // Already unmounted, immediately unlisten
      } else {
        unlistenFn = fn;
      }
    }).catch((err) => {
      if (!cancelled) setError(String(err));
    });

    return () => {
      cancelled = true;
      if (unlistenFn) unlistenFn();
      if (completeTimer) clearTimeout(completeTimer);
    };
  }, [isAnalyzing, onComplete]);

  if (!isAnalyzing && !completed && !error) return null;

  const repoName = repoPath?.split(/[\\/]/).pop() ?? "repository";

  return (
    <div
      className="rounded-xl overflow-hidden fade-in"
      style={{
        background: "var(--surface)",
        border: `1px solid ${
          error ? "var(--rose)" : completed ? "var(--green)" : "var(--accent)"
        }`,
        boxShadow: "var(--shadow-md)",
      }}
    >
      {/* Header */}
      <div
        className="flex items-center"
        style={{ gap: 12, paddingLeft: 20, paddingRight: 20, paddingTop: 12, paddingBottom: 12, borderBottom: "1px solid var(--surface-border)" }}
      >
        <div
          className="w-8 h-8 rounded-lg flex items-center justify-center shrink-0"
          style={{
            background: error
              ? "var(--rose-subtle)"
              : completed
              ? "var(--green-subtle)"
              : "color-mix(in srgb, var(--accent) 15%, transparent)",
          }}
        >
          {error ? (
            <AlertCircle size={16} style={{ color: "var(--rose)" }} />
          ) : completed ? (
            <CheckCircle2 size={16} style={{ color: "var(--green)" }} />
          ) : (
            <Loader2 size={16} className="animate-spin" style={{ color: "var(--accent)" }} />
          )}
        </div>

        <div className="flex-1 min-w-0">
          <p className="text-sm font-medium" style={{ color: "var(--text-0)", fontFamily: "var(--font-display)" }}>
            {error
              ? t("analyze.analysisFailed")
              : completed
              ? t("analyze.analysisComplete")
              : t("analyze.analyzingRepo").replace("{name}", repoName)}
          </p>
          <p className="text-[11px] truncate" style={{ color: "var(--text-3)", fontFamily: "var(--font-mono)" }}>
            {repoPath}
          </p>
        </div>

        {(completed || error) && (
          <button
            onClick={onDismiss}
            className="rounded-md hover-bg4"
            style={{ padding: 4, color: "var(--text-3)" }}
          >
            <X size={14} />
          </button>
        )}
      </div>

      {/* Progress bar */}
      <div style={{ paddingLeft: 20, paddingRight: 20, paddingTop: 12, paddingBottom: 12 }}>
        {/* Overall progress bar */}
        <div className="flex items-center gap-3 mb-3">
          <div
            className="flex-1 h-2 rounded-full overflow-hidden"
            style={{ background: "var(--bg-3)" }}
          >
            <div
              className={`h-full rounded-full transition-all duration-500 ease-out ${!error && !completed && overallPercent > 0 ? "progress-glow" : ""}`}
              style={{
                width: `${overallPercent}%`,
                background: error
                  ? "var(--rose)"
                  : completed
                  ? "var(--green)"
                  : "var(--accent)",
              }}
            />
          </div>
          <span
            className="text-xs font-mono tabular-nums shrink-0"
            style={{ color: "var(--text-2)", minWidth: 36, textAlign: "right" }}
          >
            {Math.round(overallPercent)}%
          </span>
        </div>

        {/* Phase stepper */}
        <div className="flex gap-1">
          {PIPELINE_STEPS.map((step) => {
            const isActive = progress?.phase === step;
            const isPast =
              completed ||
              (progress && PIPELINE_STEPS.indexOf(step) < PIPELINE_STEPS.indexOf(progress.phase));
            return (
              <div key={step} className="flex-1 flex flex-col items-center gap-1">
                <div
                  className="w-full h-1 rounded-full transition-all duration-300"
                  style={{
                    background: isPast
                      ? "var(--accent)"
                      : isActive
                      ? "color-mix(in srgb, var(--accent) 60%, transparent)"
                      : "var(--bg-4)",
                  }}
                />
                <span
                  className="text-[9px] font-medium transition-colors duration-200"
                  style={{
                    color: isActive
                      ? "var(--accent)"
                      : isPast
                      ? "var(--text-2)"
                      : "var(--text-4)",
                  }}
                >
                  {t(`analyze.phase.${step}`)}
                </span>
              </div>
            );
          })}
        </div>

        {/* Current phase detail message */}
        {progress && !completed && !error && (
          <p
            className="text-[11px] mt-2.5 truncate"
            style={{ color: "var(--text-3)" }}
          >
            {progress.message}
            {progress.stats && (
              <span style={{ color: "var(--text-4)" }}>
                {" "}— {progress.stats.filesProcessed}/{progress.stats.totalFiles} {t("analyze.files")},{" "}
                {progress.stats.nodesCreated} {t("analyze.nodes")}
              </span>
            )}
          </p>
        )}

        {/* Error message */}
        {error && (
          <p className="text-[11px] mt-2 leading-relaxed" style={{ color: "var(--rose)" }}>
            {error}
          </p>
        )}

        {/* Completion stats */}
        {completed && progress?.stats && (
          <p className="text-[11px] mt-2" style={{ color: "var(--green)" }}>
            {progress.message}
          </p>
        )}
      </div>
    </div>
  );
}

/** Button to trigger folder selection and analysis */
export function AnalyzeButton({
  onClick,
  disabled,
}: {
  onClick: () => void;
  disabled?: boolean;
}) {
  const { t } = useI18n();
  return (
    <button
      onClick={onClick}
      disabled={disabled}
      className="flex items-center rounded-lg text-xs font-medium hover-brighten"
      style={{
        gap: 8,
        paddingLeft: 16,
        paddingRight: 16,
        paddingTop: 8,
        paddingBottom: 8,
        background: disabled ? "var(--bg-3)" : "var(--accent)",
        color: disabled ? "var(--text-3)" : "#fff",
        opacity: disabled ? 0.7 : 1,
        cursor: disabled ? "not-allowed" : "pointer",
      }}
    >
      <FolderOpen size={14} />
      {t("analyze.analyzeProject")}
    </button>
  );
}

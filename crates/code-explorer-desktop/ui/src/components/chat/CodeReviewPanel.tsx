/**
 * CodeReviewPanel — renders the output of a code_review run.
 *
 * In-chat equivalent of the document Claude's code-review skill produces:
 * objective signals (risk, affected count, hotspots, untraced) + the LLM's
 * confidence-filtered issues.
 */

import { lazy, Suspense, useState } from "react";
import {
  ShieldCheck,
  AlertTriangle,
  CheckCircle2,
  Copy,
  FileDown,
  Check,
  Flame,
  EyeOff,
  Skull,
} from "lucide-react";
import { toast } from "sonner";
import type {
  CodeReviewArtifact,
  ReviewIssue,
} from "../../lib/tauri-commands";
import { copyTextToClipboard } from "../../lib/clipboard";

const ChatMarkdown = lazy(() =>
  import("./ChatMarkdown").then((m) => ({ default: m.ChatMarkdown })),
);

interface Props {
  artifact: CodeReviewArtifact;
}

export function CodeReviewPanel({ artifact }: Props) {
  const [showSignals, setShowSignals] = useState(true);

  const copy = async () => {
    const ok = await copyTextToClipboard(artifact.markdown);
    if (ok) {
      toast.success("Review copied to clipboard");
    } else {
      toast.error("Failed to copy review");
    }
  };

  const download = () => {
    const blob = new Blob([artifact.markdown], { type: "text/markdown" });
    const url = URL.createObjectURL(blob);
    const a = document.createElement("a");
    a.href = url;
    a.download = `code-review-${artifact.id}.md`;
    a.click();
    URL.revokeObjectURL(url);
  };

  return (
    <div
      style={{
        border: "1px solid var(--surface-border)",
        borderRadius: 12,
        background: "var(--surface)",
        marginTop: 12,
        overflow: "hidden",
      }}
    >
      {/* Header */}
      <div
        style={{
          padding: "12px 16px",
          borderBottom: "1px solid var(--surface-border)",
          display: "flex",
          alignItems: "center",
          justifyContent: "space-between",
          background: "var(--surface-hover)",
        }}
      >
        <div style={{ display: "flex", alignItems: "center", gap: 10 }}>
          <ShieldCheck size={16} style={{ color: "var(--accent)" }} />
          <div>
            <div style={{ fontSize: 13, fontWeight: 600 }}>Code Review</div>
            <div style={{ fontSize: 11, color: "var(--text-3)", marginTop: 2 }}>
              {artifact.scopeSummary} · {Math.round(artifact.durationMs / 10) / 100}s
            </div>
          </div>
        </div>
        <div style={{ display: "flex", gap: 6, alignItems: "center" }}>
          <RiskBadge level={artifact.signals.riskLevel} />
          <VerdictBadge verdict={artifact.review.verdict} />
          <IconButton icon={Copy} onClick={copy} title="Copy as Markdown" />
          <IconButton icon={FileDown} onClick={download} title="Download .md" />
        </div>
      </div>

      {/* Signals panel (collapsible) */}
      <div style={{ padding: "10px 16px", borderBottom: "1px solid var(--surface-border)" }}>
        <button
          onClick={() => setShowSignals((v) => !v)}
          style={{
            fontSize: 11,
            color: "var(--text-3)",
            background: "transparent",
            border: "none",
            cursor: "pointer",
            padding: 0,
            fontFamily: "inherit",
          }}
        >
          {showSignals ? "▼" : "▶"} Graph signals
        </button>
        {showSignals && <SignalGrid signals={artifact.signals} />}
      </div>

      {/* Review content */}
      <div style={{ padding: "12px 16px" }}>
        {artifact.review.issues.length === 0 ? (
          <div
            style={{
              display: "flex",
              alignItems: "center",
              gap: 8,
              padding: "10px 12px",
              background: "rgba(158,206,106,0.1)",
              border: "1px solid #9ece6a",
              borderRadius: 8,
              fontSize: 12,
            }}
          >
            <CheckCircle2 size={14} style={{ color: "var(--green)" }} />
            <span>No high-confidence issues found. Ready to commit.</span>
          </div>
        ) : (
          <IssuesList issues={artifact.review.issues} />
        )}

        {artifact.review.predictedImpact && (
          <div style={{ marginTop: 12, fontSize: 12 }}>
            <div
              style={{
                fontSize: 10,
                fontWeight: 600,
                textTransform: "uppercase",
                color: "var(--text-3)",
                marginBottom: 4,
              }}
            >
              Predicted impact
            </div>
            <Suspense fallback={<div>{artifact.review.predictedImpact}</div>}>
              <ChatMarkdown content={artifact.review.predictedImpact} />
            </Suspense>
          </div>
        )}
      </div>
    </div>
  );
}

// ─── Sub-components ──────────────────────────────────────────────────

function IconButton({
  icon: Icon,
  onClick,
  title,
}: {
  icon: typeof Copy;
  onClick: () => void;
  title: string;
}) {
  const [clicked, setClicked] = useState(false);
  return (
    <button
      onClick={() => {
        onClick();
        setClicked(true);
        setTimeout(() => setClicked(false), 1200);
      }}
      title={title}
      style={{
        padding: 6,
        background: "transparent",
        border: "1px solid var(--surface-border)",
        borderRadius: 6,
        cursor: "pointer",
        color: "var(--text-2)",
        display: "flex",
        alignItems: "center",
      }}
    >
      {clicked ? <Check size={12} /> : <Icon size={12} />}
    </button>
  );
}

function RiskBadge({ level }: { level: string }) {
  const config: Record<string, { color: string; bg: string }> = {
    none: { color: "var(--green)", bg: "rgba(158,206,106,0.12)" },
    low: { color: "var(--accent)", bg: "rgba(122,162,247,0.12)" },
    medium: { color: "var(--amber)", bg: "rgba(224,175,104,0.12)" },
    high: { color: "var(--rose)", bg: "rgba(247,118,142,0.12)" },
  };
  const c = config[level] ?? config.medium;
  return (
    <span
      style={{
        padding: "2px 8px",
        background: c.bg,
        border: `1px solid ${c.color}`,
        borderRadius: 999,
        fontSize: 10,
        fontWeight: 700,
        textTransform: "uppercase",
        color: c.color,
      }}
    >
      risk: {level}
    </span>
  );
}

function VerdictBadge({ verdict }: { verdict: string }) {
  const v = verdict.toLowerCase();
  const config =
    v === "ready"
      ? { icon: CheckCircle2, color: "var(--green)", bg: "rgba(158,206,106,0.12)", label: "ready" }
      : v === "blocked"
        ? { icon: AlertTriangle, color: "var(--rose)", bg: "rgba(247,118,142,0.12)", label: "blocked" }
        : { icon: AlertTriangle, color: "var(--amber)", bg: "rgba(224,175,104,0.12)", label: "needs revisions" };
  const Icon = config.icon;
  return (
    <span
      style={{
        display: "inline-flex",
        alignItems: "center",
        gap: 4,
        padding: "2px 8px",
        background: config.bg,
        border: `1px solid ${config.color}`,
        borderRadius: 999,
        fontSize: 10,
        fontWeight: 700,
        textTransform: "uppercase",
        color: config.color,
      }}
    >
      <Icon size={10} />
      {config.label}
    </span>
  );
}

function SignalGrid({
  signals,
}: {
  signals: CodeReviewArtifact["signals"];
}) {
  return (
    <div
      style={{
        display: "grid",
        gridTemplateColumns: "repeat(auto-fill, minmax(180px, 1fr))",
        gap: 8,
        marginTop: 8,
      }}
    >
      <Stat label="Changed files" value={signals.changedFiles.length} />
      <Stat label="Changed symbols" value={signals.changedSymbols.length} />
      <Stat label="Affected (transitive)" value={signals.affectedCount} />
      <Stat label="Processes touched" value={signals.affectedProcesses.length} />
      <Stat label="Hotspot files" value={signals.hotspotFiles.length} icon={Flame} />
      <Stat label="Untraced symbols" value={signals.untracedSymbols.length} icon={EyeOff} />
      <Stat label="Dead candidates" value={signals.deadCandidates.length} icon={Skull} />
    </div>
  );
}

function Stat({
  label,
  value,
  icon: Icon,
}: {
  label: string;
  value: number;
  icon?: typeof Copy;
}) {
  const hot = value > 0;
  return (
    <div
      style={{
        padding: "6px 10px",
        border: "1px solid var(--surface-border)",
        borderRadius: 6,
        background: hot ? "rgba(247,118,142,0.08)" : "var(--surface-hover)",
        fontSize: 11,
      }}
    >
      <div style={{ display: "flex", alignItems: "center", gap: 4, color: "var(--text-3)" }}>
        {Icon && <Icon size={10} style={{ color: hot ? "var(--rose)" : "var(--text-3)" }} />}
        <span style={{ fontSize: 9, textTransform: "uppercase", fontWeight: 600 }}>{label}</span>
      </div>
      <div style={{ fontSize: 18, fontWeight: 700, marginTop: 2 }}>{value}</div>
    </div>
  );
}

function IssuesList({ issues }: { issues: ReviewIssue[] }) {
  return (
    <div style={{ display: "flex", flexDirection: "column", gap: 6 }}>
      {issues.map((issue, i) => {
        const sevColor =
          issue.severity === "high"
            ? "var(--rose)"
            : issue.severity === "medium"
              ? "var(--amber)"
              : "var(--accent)";
        return (
          <div
            key={i}
            style={{
              padding: "8px 12px",
              border: `1px solid ${sevColor}`,
              borderLeftWidth: 4,
              borderRadius: 6,
              background: "var(--surface-hover)",
              fontSize: 12,
            }}
          >
            <div style={{ display: "flex", gap: 6, alignItems: "baseline" }}>
              <span
                style={{
                  fontSize: 9,
                  fontWeight: 700,
                  textTransform: "uppercase",
                  color: sevColor,
                }}
              >
                {issue.severity}
              </span>
              <span style={{ fontSize: 10, color: "var(--text-3)" }}>
                conf {issue.confidence.toFixed(2)}
              </span>
              <span style={{ fontWeight: 600 }}>{issue.title}</span>
            </div>
            {issue.detail && (
              <div style={{ marginTop: 3, color: "var(--text-2)" }}>{issue.detail}</div>
            )}
          </div>
        );
      })}
    </div>
  );
}

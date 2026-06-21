/**
 * ArtifactPanel — renders the structured output of a feature-dev run.
 *
 * Three sections stream in one at a time (explorer → architect → reviewer).
 * Each section is collapsible and copy/export-friendly. This is the in-chat
 * analog of the document produced by Claude's feature-dev skill.
 */

import { lazy, Suspense, useMemo, useState } from "react";
import {
  Compass,
  Hammer,
  ShieldCheck,
  ChevronRight,
  Copy,
  FileDown,
  Check,
  Loader2,
  AlertTriangle,
  CheckCircle2,
} from "lucide-react";
import { toast } from "sonner";
import type {
  FeatureDevArtifact,
  FeatureDevPhase,
  FeatureDevSection,
  ReviewIssue,
} from "../../lib/tauri-commands";
import { copyTextToClipboard } from "../../lib/clipboard";

const ChatMarkdown = lazy(() =>
  import("./ChatMarkdown").then((m) => ({ default: m.ChatMarkdown })),
);

interface Props {
  artifact: FeatureDevArtifact;
  activePhase?: FeatureDevPhase | null;
  onClose?: () => void;
}

const PHASE_META: Record<
  FeatureDevPhase,
  { label: string; icon: typeof Compass; color: string }
> = {
  explorer: { label: "Explorer", icon: Compass, color: "var(--accent)" },
  architect: { label: "Architect", icon: Hammer, color: "var(--amber)" },
  reviewer: { label: "Reviewer", icon: ShieldCheck, color: "var(--green)" },
};

export function ArtifactPanel({ artifact, activePhase }: Props) {
  const [collapsed, setCollapsed] = useState<Record<string, boolean>>({});

  const toggle = (phase: FeatureDevPhase) =>
    setCollapsed((c) => ({ ...c, [phase]: !c[phase] }));

  const fullMarkdown = useMemo(() => buildFullMarkdown(artifact), [artifact]);

  const copyAll = async () => {
    const copied = await copyTextToClipboard(fullMarkdown);
    if (copied) {
      toast.success("Artifact copied to clipboard");
    } else {
      toast.error("Failed to copy artifact");
    }
  };

  const downloadAll = () => {
    const blob = new Blob([fullMarkdown], { type: "text/markdown" });
    const url = URL.createObjectURL(blob);
    const a = document.createElement("a");
    a.href = url;
    a.download = `feature-dev-${artifact.id}.md`;
    a.click();
    setTimeout(() => URL.revokeObjectURL(url), 1000);
  };

  // Render placeholder cards for phases not yet run.
  const allPhases: FeatureDevPhase[] = ["explorer", "architect", "reviewer"];
  const sectionByPhase = new Map<FeatureDevPhase, FeatureDevSection>();
  for (const s of artifact.sections) sectionByPhase.set(s.phase, s);

  return (
    <div
      style={{
        border: "1px solid var(--surface-border)",
        borderRadius: 12,
        background: "var(--surface)",
        padding: 0,
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
          <Hammer size={16} style={{ color: "var(--accent)" }} />
          <div>
            <div style={{ fontSize: 13, fontWeight: 600 }}>Feature Dev artifact</div>
            <div style={{ fontSize: 11, color: "var(--text-3)", marginTop: 2 }}>
              {truncate(artifact.featureDescription, 120)}
            </div>
          </div>
        </div>
        <div style={{ display: "flex", gap: 6 }}>
          <IconButton icon={Copy} onClick={copyAll} title="Copy all as Markdown" />
          <IconButton icon={FileDown} onClick={downloadAll} title="Download .md" />
        </div>
      </div>

      {/* Sections */}
      {allPhases.map((phase) => {
        const section = sectionByPhase.get(phase);
        const isActive = activePhase === phase && !section;
        const isCollapsed = collapsed[phase] ?? false;
        const meta = PHASE_META[phase];
        const Icon = meta.icon;

        return (
          <div
            key={phase}
            style={{ borderTop: "1px solid var(--surface-border)" }}
          >
            <button
              onClick={() => section && toggle(phase)}
              disabled={!section}
              style={{
                width: "100%",
                padding: "10px 16px",
                display: "flex",
                alignItems: "center",
                gap: 10,
                background: "transparent",
                border: "none",
                textAlign: "left",
                cursor: section ? "pointer" : "default",
                color: "inherit",
                fontFamily: "inherit",
              }}
            >
              <ChevronRight
                size={14}
                style={{
                  transform: section && !isCollapsed ? "rotate(90deg)" : "rotate(0deg)",
                  transition: "transform 0.15s",
                  color: "var(--text-3)",
                  opacity: section ? 1 : 0.3,
                }}
              />
              <Icon size={14} style={{ color: meta.color }} />
              <span style={{ fontSize: 12, fontWeight: 600 }}>{meta.label}</span>
              {isActive && (
                <Loader2
                  size={12}
                  className="animate-spin"
                  style={{ color: meta.color }}
                />
              )}
              {section && (
                <span style={{ fontSize: 10, color: "var(--text-3)", marginLeft: "auto" }}>
                  {Math.round(section.durationMs / 10) / 100}s
                </span>
              )}
              {!section && !isActive && (
                <span style={{ fontSize: 10, color: "var(--text-3)", marginLeft: "auto" }}>
                  waiting
                </span>
              )}
            </button>

            {section && !isCollapsed && (
              <div style={{ padding: "4px 16px 14px 40px", fontSize: 12 }}>
                {section.phase === "reviewer" && section.review && (
                  <ReviewBadge verdict={section.review.verdict} />
                )}
                <Suspense
                  fallback={<pre style={{ whiteSpace: "pre-wrap" }}>{section.markdown}</pre>}
                >
                  <ChatMarkdown content={section.markdown} />
                </Suspense>
                {section.phase === "reviewer" && section.review && section.review.issues.length > 0 && (
                  <IssuesList issues={section.review.issues} />
                )}
                {section.phase === "architect" && section.blueprint && section.blueprint.buildSequence.length > 0 && (
                  <CopyAsTodos steps={section.blueprint.buildSequence} />
                )}
              </div>
            )}
          </div>
        );
      })}
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
  const handle = () => {
    onClick();
    setClicked(true);
    setTimeout(() => setClicked(false), 1200);
  };
  return (
    <button
      onClick={handle}
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

function ReviewBadge({ verdict }: { verdict: string }) {
  const v = verdict.toLowerCase();
  const config =
    v === "ready"
      ? {
          icon: CheckCircle2,
          label: "Ready to implement",
          color: "var(--green)",
          bg: "rgba(158,206,106,0.12)",
        }
      : v === "blocked"
        ? {
            icon: AlertTriangle,
            label: "Blocked",
            color: "var(--rose)",
            bg: "rgba(247,118,142,0.12)",
          }
        : {
            icon: AlertTriangle,
            label: "Needs revisions",
            color: "var(--amber)",
            bg: "rgba(224,175,104,0.12)",
          };
  const Icon = config.icon;
  return (
    <div
      style={{
        display: "inline-flex",
        alignItems: "center",
        gap: 6,
        padding: "4px 10px",
        background: config.bg,
        border: `1px solid ${config.color}`,
        borderRadius: 999,
        fontSize: 11,
        fontWeight: 600,
        color: config.color,
        marginBottom: 8,
      }}
    >
      <Icon size={12} />
      {config.label}
    </div>
  );
}

function IssuesList({ issues }: { issues: ReviewIssue[] }) {
  return (
    <div style={{ marginTop: 8, display: "flex", flexDirection: "column", gap: 6 }}>
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
              padding: "6px 10px",
              border: `1px solid ${sevColor}`,
              borderLeftWidth: 3,
              borderRadius: 6,
              background: "var(--surface-hover)",
              fontSize: 11,
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
                confidence {issue.confidence.toFixed(2)}
              </span>
              <span style={{ fontWeight: 600 }}>{issue.title}</span>
            </div>
            {issue.detail && (
              <div style={{ marginTop: 2, color: "var(--text-2)" }}>{issue.detail}</div>
            )}
          </div>
        );
      })}
    </div>
  );
}

function CopyAsTodos({ steps }: { steps: string[] }) {
  const [copied, setCopied] = useState(false);
  const todos = steps.map((s, i) => `- [ ] ${i + 1}. ${s}`).join("\n");
  const copy = async () => {
    const ok = await copyTextToClipboard(todos);
    if (ok) {
      setCopied(true);
      toast.success("Build sequence copied as TODOs");
      setTimeout(() => setCopied(false), 1200);
    } else {
      toast.error("Failed to copy build sequence");
    }
  };
  return (
    <button
      onClick={copy}
      style={{
        marginTop: 8,
        padding: "4px 10px",
        background: "var(--surface-hover)",
        border: "1px solid var(--surface-border)",
        borderRadius: 6,
        fontSize: 10,
        cursor: "pointer",
        color: "var(--text-2)",
        display: "inline-flex",
        alignItems: "center",
        gap: 6,
      }}
    >
      {copied ? <Check size={10} /> : <Copy size={10} />}
      Copy build sequence as TODOs
    </button>
  );
}

// ─── Helpers ─────────────────────────────────────────────────────────

function buildFullMarkdown(a: FeatureDevArtifact): string {
  let md = `# Feature Dev: ${a.featureDescription}\n\n`;
  for (const s of a.sections) {
    md += `## ${PHASE_META[s.phase].label}: ${s.title}\n\n${s.markdown}\n\n`;
  }
  return md;
}

function truncate(s: string, n: number) {
  return s.length > n ? s.slice(0, n) + "…" : s;
}

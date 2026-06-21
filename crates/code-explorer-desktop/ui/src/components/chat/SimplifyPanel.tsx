/**
 * SimplifyPanel — renders the output of a simplify run.
 *
 * Compact format: list of refactor proposals (kind chip + target + rationale),
 * plus collapsible signals (complex hotspots, dead candidates, duplicate
 * names). Same UX language as CodeReviewPanel — confidence-tagged.
 */

import { useState } from "react";
import {
  Sparkles,
  Copy,
  FileDown,
  Check,
  ChevronDown,
  ChevronRight,
} from "lucide-react";
import { toast } from "sonner";
import type {
  SimplifyArtifact,
  SimplifyProposal,
} from "../../lib/tauri-commands";
import { copyTextToClipboard } from "../../lib/clipboard";

interface Props {
  artifact: SimplifyArtifact;
}

const KIND_COLORS: Record<string, string> = {
  delete: "var(--rose)",
  extract: "var(--accent)",
  merge: "#bb9af7",
  inline: "var(--green)",
  rename: "var(--amber)",
};

export function SimplifyPanel({ artifact }: Props) {
  const [showSignals, setShowSignals] = useState(false);

  const copy = async () => {
    const ok = await copyTextToClipboard(artifact.markdown);
    if (ok) {
      toast.success("Simplify report copied");
    } else {
      toast.error("Failed to copy simplify report");
    }
  };
  const download = () => {
    const blob = new Blob([artifact.markdown], { type: "text/markdown" });
    const url = URL.createObjectURL(blob);
    const a = document.createElement("a");
    a.href = url;
    a.download = `simplify-${artifact.id}.md`;
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
          <Sparkles size={16} style={{ color: "#bb9af7" }} />
          <div>
            <div style={{ fontSize: 13, fontWeight: 600 }}>Simplify</div>
            <div style={{ fontSize: 11, color: "var(--text-3)", marginTop: 2 }}>
              {artifact.signals.scope} · {artifact.proposals.length} proposal
              {artifact.proposals.length === 1 ? "" : "s"} ·{" "}
              {Math.round(artifact.durationMs / 10) / 100}s
            </div>
          </div>
        </div>
        <div style={{ display: "flex", gap: 6 }}>
          <IconButton icon={Copy} onClick={copy} title="Copy as Markdown" />
          <IconButton icon={FileDown} onClick={download} title="Download .md" />
        </div>
      </div>

      {/* Proposals */}
      <div style={{ padding: "12px 16px" }}>
        {artifact.proposals.length === 0 ? (
          <div style={{ fontSize: 12, color: "var(--text-3)" }}>
            _No high-value refactor moves identified._
          </div>
        ) : (
          <div style={{ display: "flex", flexDirection: "column", gap: 6 }}>
            {artifact.proposals.map((p, i) => (
              <ProposalRow key={i} proposal={p} />
            ))}
          </div>
        )}

        {/* Collapsible signals */}
        <button
          onClick={() => setShowSignals((v) => !v)}
          style={{
            marginTop: 12,
            fontSize: 11,
            color: "var(--text-3)",
            background: "transparent",
            border: "none",
            cursor: "pointer",
            padding: 0,
            display: "inline-flex",
            alignItems: "center",
            gap: 4,
            fontFamily: "inherit",
          }}
        >
          {showSignals ? <ChevronDown size={11} /> : <ChevronRight size={11} />}
          Detected signals ({artifact.signals.complexSymbols.length} complex,{" "}
          {artifact.signals.deadCandidates.length} dead,{" "}
          {artifact.signals.duplicateGroups.length} dupes)
        </button>

        {showSignals && (
          <div style={{ marginTop: 8, fontSize: 11 }}>
            {artifact.signals.complexSymbols.length > 0 && (
              <SignalSection title="Complexity hotspots">
                {artifact.signals.complexSymbols.slice(0, 8).map((c, i) => (
                  <li key={i}>
                    <code>{c.name}</code> — complexity {c.complexity} in{" "}
                    <code>{c.filePath}</code>
                  </li>
                ))}
              </SignalSection>
            )}
            {artifact.signals.deadCandidates.length > 0 && (
              <SignalSection title="Dead candidates">
                {artifact.signals.deadCandidates.slice(0, 10).map((d, i) => (
                  <li key={i}>
                    <code>{d}</code>
                  </li>
                ))}
              </SignalSection>
            )}
            {artifact.signals.duplicateGroups.length > 0 && (
              <SignalSection title="Duplicate names">
                {artifact.signals.duplicateGroups.slice(0, 8).map((d, i) => (
                  <li key={i}>
                    <code>{d.name}</code> ×{d.occurrences}
                  </li>
                ))}
              </SignalSection>
            )}
          </div>
        )}
      </div>
    </div>
  );
}

function ProposalRow({ proposal }: { proposal: SimplifyProposal }) {
  const color = KIND_COLORS[proposal.kind] ?? "var(--text-3)";
  return (
    <div
      style={{
        padding: "8px 12px",
        border: `1px solid ${color}`,
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
            color,
            padding: "1px 6px",
            border: `1px solid ${color}`,
            borderRadius: 4,
          }}
        >
          {proposal.kind}
        </span>
        <span style={{ fontSize: 10, color: "var(--text-3)" }}>
          conf {proposal.confidence.toFixed(2)}
        </span>
        <code style={{ fontWeight: 600 }}>{proposal.target}</code>
      </div>
      {proposal.rationale && (
        <div style={{ marginTop: 3, color: "var(--text-2)" }}>{proposal.rationale}</div>
      )}
    </div>
  );
}

function SignalSection({
  title,
  children,
}: {
  title: string;
  children: React.ReactNode;
}) {
  return (
    <div style={{ marginTop: 6 }}>
      <div
        style={{
          fontSize: 9,
          fontWeight: 700,
          textTransform: "uppercase",
          color: "var(--text-3)",
          marginBottom: 2,
        }}
      >
        {title}
      </div>
      <ul style={{ margin: 0, paddingLeft: 16, color: "var(--text-2)" }}>{children}</ul>
    </div>
  );
}

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

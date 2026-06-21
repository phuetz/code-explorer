/**
 * Code Health Score card — circular gauge with sub-scores.
 */

import { useQuery } from "@tanstack/react-query";
import { AlertCircle } from "lucide-react";
import { commands } from "../../lib/tauri-commands";
import { AnimatedCard } from "../shared/motion";
import { useI18n } from "../../hooks/use-i18n";
import { useAppStore } from "../../stores/app-store";

function gradeColor(grade: string): string {
  switch (grade) {
    case "A":
      return "var(--green)";
    case "B":
      return "var(--accent)";
    case "C":
      return "var(--amber)";
    case "D":
      return "#ff9e64";
    default:
      return "var(--rose)";
  }
}

function ScoreBar({
  label,
  value,
  color,
}: {
  label: string;
  value: number;
  color: string;
}) {
  return (
    <div style={{ display: "flex", alignItems: "center", gap: 8, fontSize: 11 }}>
      <span
        style={{
          width: 80,
          color: "var(--text-2)",
          flexShrink: 0,
        }}
      >
        {label}
      </span>
      <div
        style={{
          flex: 1,
          height: 6,
          borderRadius: 3,
          background: "var(--bg-3)",
          overflow: "hidden",
        }}
      >
        <div
          style={{
            width: `${value * 100}%`,
            height: "100%",
            borderRadius: 3,
            background: color,
            transition: "width 0.5s ease",
          }}
        />
      </div>
      <span
        style={{
          width: 32,
          textAlign: "right",
          color: "var(--text-2)",
          fontVariantNumeric: "tabular-nums",
        }}
      >
        {Math.round(value * 100)}%
      </span>
    </div>
  );
}

export function CodeHealthCard() {
  const { t } = useI18n();
  const activeRepo = useAppStore((s) => s.activeRepo);
  // Scope by `activeRepo` so switching repos refetches instead of showing
  // stale health metrics from the previously-active repo.
  const { data: health, isLoading, error } = useQuery({
    queryKey: ["code-health", activeRepo],
    queryFn: () => commands.getCodeHealth(),
    staleTime: 60_000,
  });

  if (error) {
    return (
      <div className="flex flex-col items-center justify-center h-full p-8 text-center">
        <AlertCircle size={40} style={{ color: "var(--rose)", marginBottom: 16 }} />
        <p style={{ fontSize: 13, color: "var(--text-3)", maxWidth: 400, lineHeight: 1.5 }}>
          {String(error)}
        </p>
      </div>
    );
  }

  if (isLoading) {
    return (
      <div style={{ padding: "20px", display: "flex", alignItems: "center", justifyContent: "center" }}>
        <div className="shimmer" style={{ width: 80, height: 80, borderRadius: "50%", background: "var(--bg-3)" }} />
      </div>
    );
  }
  if (!health) return null;

  const color = gradeColor(health.grade);

  // SVG arc for circular gauge
  const radius = 36;
  const circumference = 2 * Math.PI * radius;
  const progress = (health.overallScore / 100) * circumference;

  return (
    <AnimatedCard>
      <div
        style={{
          padding: "16px 20px",
          borderRadius: "var(--radius-lg)",
          border: "1px solid var(--border)",
          background: "var(--bg-1)",
          display: "flex",
          gap: 20,
          alignItems: "center",
        }}
      >
        {/* Circular gauge */}
        <div style={{ position: "relative", width: 84, height: 84, flexShrink: 0 }}>
          <svg width={84} height={84} style={{ transform: "rotate(-90deg)" }}>
            <circle
              cx={42}
              cy={42}
              r={radius}
              fill="none"
              stroke="var(--bg-3)"
              strokeWidth={6}
            />
            <circle
              cx={42}
              cy={42}
              r={radius}
              fill="none"
              stroke={color}
              strokeWidth={6}
              strokeDasharray={circumference}
              strokeDashoffset={circumference - progress}
              strokeLinecap="round"
              style={{ transition: "stroke-dashoffset 0.8s ease" }}
            />
          </svg>
          <div
            style={{
              position: "absolute",
              inset: 0,
              display: "flex",
              flexDirection: "column",
              alignItems: "center",
              justifyContent: "center",
            }}
          >
            <span
              style={{
                fontSize: 22,
                fontWeight: 700,
                color,
                lineHeight: 1,
              }}
            >
              {health.grade}
            </span>
            <span
              style={{
                fontSize: 10,
                color: "var(--text-3)",
              }}
            >
              {Math.round(health.overallScore)}/100
            </span>
          </div>
        </div>

        {/* Sub-scores */}
        <div style={{ flex: 1, display: "flex", flexDirection: "column", gap: 6 }}>
          <div
            style={{
              fontSize: 13,
              fontWeight: 600,
              color: "var(--text-0)",
              marginBottom: 4,
            }}
          >
            {t("health.title")}
          </div>
          <ScoreBar label={t("health.hotspots")} value={health.hotspotScore} color="var(--green)" />
          <ScoreBar label={t("health.cohesion")} value={health.cohesionScore} color="var(--accent)" />
          <ScoreBar label={t("health.tracing")} value={health.tracingCoverage} color="#73daca" />
          <ScoreBar label={t("health.ownership")} value={health.ownershipScore} color="#bb9af7" />
          <ScoreBar label={t("health.complexity")} value={Math.max(0, Math.min(1 - (health.avgComplexity / 30), 1))} color="#f59e0b" />
        </div>

        {/* Complexity detail */}
        {health.maxComplexity > 0 && (
          <div style={{ position: "relative", width: 84, flexShrink: 0, display: "flex", alignItems: "center", justifyContent: "center" }}>
            <div style={{ fontSize: 10, color: "var(--text-3)", textAlign: "center" }}>
              Max CC: {health.maxComplexity} · Avg: {health.avgComplexity.toFixed(1)}
            </div>
          </div>
        )}
      </div>
    </AnimatedCard>
  );
}

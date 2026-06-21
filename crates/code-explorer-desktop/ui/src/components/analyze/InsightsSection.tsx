/**
 * AI-generated insights based on code health and graph metrics.
 */

import { Lightbulb, ArrowRight } from "lucide-react";
import type { GraphPayload, CodeHealth } from "../../lib/tauri-commands";
import { useAppStore } from "../../stores/app-store";
import { useI18n } from "../../hooks/use-i18n";

interface Insight {
  id: string;
  message: string;
  severity: "info" | "warn" | "error";
  action?: { label: string; view: string };
}

function deriveInsights(
  stats: GraphPayload["stats"] | undefined,
  health: CodeHealth | undefined,
): Insight[] {
  const insights: Insight[] = [];
  if (!stats && !health) return insights;

  // Health score
  if (health && health.overallScore < 60) {
    insights.push({
      id: "low-health",
      message: `Code health score is ${health.overallScore}/100 (${health.grade})`,
      severity: "error",
      action: { label: "View Health", view: "health" },
    });
  }

  // High complexity
  if (health && health.maxComplexity > 20) {
    insights.push({
      id: "high-complexity",
      message: `Max cyclomatic complexity is ${health.maxComplexity} (avg ${health.avgComplexity.toFixed(1)})`,
      severity: "warn",
      action: { label: "View Health", view: "health" },
    });
  }

  // Low cohesion
  if (health && health.cohesionScore < 0.5) {
    insights.push({
      id: "low-cohesion",
      message: `Module cohesion is low (${Math.round(health.cohesionScore * 100)}%)`,
      severity: "warn",
    });
  }

  // All good
  if (insights.length === 0) {
    insights.push({
      id: "all-good",
      message: "Codebase looks healthy — no critical issues detected",
      severity: "info",
    });
  }

  return insights;
}

const SEVERITY_COLORS = {
  info: "var(--green)",
  warn: "var(--amber)",
  error: "var(--rose)",
};

interface Props {
  stats?: GraphPayload["stats"];
  health?: CodeHealth;
}

export function InsightsSection({ stats, health }: Props) {
  const { t } = useI18n();
  const setAnalyzeView = useAppStore((s) => s.setAnalyzeView);
  const insights = deriveInsights(stats, health);

  return (
    <div>
      <div
        className="flex items-center gap-2 mb-3"
        style={{
          fontFamily: "var(--font-display)",
          fontSize: 13,
          fontWeight: 600,
          color: "var(--text-1)",
          textTransform: "uppercase",
          letterSpacing: "0.04em",
        }}
      >
        <Lightbulb size={14} style={{ color: "var(--amber)" }} />
        {t("analyze.insights") || "Insights"}
      </div>

      <div style={{ display: "flex", flexDirection: "column", gap: 8 }}>
        {insights.map((insight) => (
          <div
            key={insight.id}
            className="flex items-center gap-3 rounded-lg"
            style={{
              padding: "10px 14px",
              background: "var(--surface)",
              border: "1px solid var(--surface-border)",
              fontSize: 12,
              color: "var(--text-2)",
            }}
          >
            <span
              style={{
                width: 6,
                height: 6,
                borderRadius: "50%",
                background: SEVERITY_COLORS[insight.severity],
                flexShrink: 0,
              }}
            />
            <span style={{ flex: 1 }}>{insight.message}</span>
            {insight.action && (
              <button
                onClick={() => setAnalyzeView(insight.action!.view as "coverage" | "health")}
                className="flex items-center gap-1 rounded-md transition-colors"
                style={{
                  padding: "4px 10px",
                  fontSize: 11,
                  fontWeight: 600,
                  color: "var(--accent)",
                  background: "var(--accent-subtle)",
                  border: "none",
                  cursor: "pointer",
                  whiteSpace: "nowrap",
                }}
              >
                {insight.action.label}
                <ArrowRight size={11} />
              </button>
            )}
          </div>
        ))}
      </div>
    </div>
  );
}

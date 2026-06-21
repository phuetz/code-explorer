import { useQuery } from "@tanstack/react-query";
import { Activity, AlertCircle } from "lucide-react";
import { commands } from "../../lib/tauri-commands";
import { useI18n } from "../../hooks/use-i18n";
import { useAppStore } from "../../stores/app-store";
import { LoadingOrbs } from "../shared/LoadingOrbs";

export function ComplexityView() {
  const { t } = useI18n();
  const activeRepo = useAppStore((s) => s.activeRepo);
  const setSelectedNodeId = useAppStore((s) => s.setSelectedNodeId);
  const setMode = useAppStore((s) => s.setMode);

  const { data, isLoading, error } = useQuery({
    queryKey: ["complexity", activeRepo],
    queryFn: () => commands.getComplexityReport(0, 50),
    staleTime: 60_000,
  });

  if (error) {
    return (
      <div className="flex flex-col items-center justify-center h-full p-8 text-center">
        <AlertCircle size={40} style={{ color: "var(--rose)", marginBottom: 16 }} />
        <p style={{ fontSize: 13, color: "var(--text-3)" }}>{String(error)}</p>
      </div>
    );
  }

  if (isLoading || !data) {
    return (
      <div className="flex items-center justify-center h-full">
        <LoadingOrbs />
      </div>
    );
  }

  if (data.measuredSymbols === 0) {
    return (
      <div
        className="flex items-center gap-2"
        style={{
          margin: 24,
          color: "var(--text-2)",
          padding: 16,
          background: "var(--bg-1)",
          borderRadius: "var(--radius-lg)",
        }}
      >
        <AlertCircle size={16} />
        {t("complexity.none")}
      </div>
    );
  }

  return (
    <div className="h-full overflow-auto" style={{ padding: 24 }}>
      <div style={{ marginBottom: 20 }}>
        <h2 className="text-lg font-semibold" style={{ color: "var(--text-0)" }}>
          <Activity size={18} style={{ display: "inline", marginRight: 8, verticalAlign: "text-bottom" }} />
          {t("complexity.title")}
        </h2>
        <p style={{ fontSize: 12, color: "var(--text-3)", marginTop: 4 }}>{t("complexity.subtitle")}</p>
      </div>

      {/* Stats grid */}
      <div style={{ display: "grid", gridTemplateColumns: "repeat(auto-fit, minmax(120px, 1fr))", gap: 10, marginBottom: 24 }}>
        <StatCard label={t("complexity.avg")} value={data.avgComplexity.toFixed(1)} />
        <StatCard label={t("complexity.max")} value={data.maxComplexity} color="var(--rose)" />
        <StatCard label={t("complexity.p50")} value={data.p50} />
        <StatCard label={t("complexity.p90")} value={data.p90} color="#e9c46a" />
        <StatCard label={t("complexity.p99")} value={data.p99} color="var(--rose)" />
      </div>

      {/* Severity buckets */}
      <div style={{ display: "grid", gridTemplateColumns: "repeat(auto-fit, minmax(170px, 1fr))", gap: 10, marginBottom: 24 }}>
        <SeverityTile label={t("complexity.severity.low")} value={data.severityCounts.low} color="var(--green, #73daca)" />
        <SeverityTile label={t("complexity.severity.medium")} value={data.severityCounts.medium} color="#e9c46a" />
        <SeverityTile label={t("complexity.severity.high")} value={data.severityCounts.high} color="#f28c28" />
        <SeverityTile label={t("complexity.severity.critical")} value={data.severityCounts.critical} color="var(--rose)" />
      </div>

      {/* Top symbols */}
      <h3 className="text-sm font-semibold" style={{ color: "var(--text-1)", marginBottom: 10 }}>
        {t("complexity.topSymbols")}
      </h3>
      <div style={{ borderRadius: "var(--radius-lg)", border: "1px solid var(--surface-border)", overflow: "hidden", marginBottom: 24 }}>
        <table style={{ width: "100%", borderCollapse: "collapse", fontSize: 12 }}>
          <thead>
            <tr style={{ background: "var(--bg-2)" }}>
              <th style={thStyle}>{t("complexity.name")}</th>
              <th style={thStyle}>{t("todos.file")}</th>
              <th style={{ ...thStyle, textAlign: "right" }}>{t("complexity.cc")}</th>
              <th style={thStyle}>{t("cycles.severity")}</th>
            </tr>
          </thead>
          <tbody>
            {data.topSymbols.map((sym) => (
              <tr
                key={sym.nodeId}
                style={{ borderTop: "1px solid var(--surface-border)", cursor: "pointer" }}
                onClick={() => {
                  setSelectedNodeId(sym.nodeId, sym.name);
                  setMode("explorer");
                }}
              >
                <td style={{ ...tdStyle, color: "var(--accent)", fontFamily: "var(--font-mono)" }}>{sym.name}</td>
                <td
                  style={{
                    ...tdStyle,
                    color: "var(--text-3)",
                    fontFamily: "var(--font-mono)",
                    maxWidth: 320,
                    overflow: "hidden",
                    textOverflow: "ellipsis",
                    whiteSpace: "nowrap",
                  }}
                  title={sym.filePath}
                >
                  {sym.filePath}
                </td>
                <td style={{ ...tdStyle, textAlign: "right", color: "var(--text-0)", fontWeight: 600, fontVariantNumeric: "tabular-nums" }}>
                  {sym.complexity}
                </td>
                <td style={tdStyle}>
                  <SeverityBadge severity={sym.severity} />
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>

      {/* By module */}
      {data.byModule.length > 0 && (
        <>
          <h3 className="text-sm font-semibold" style={{ color: "var(--text-1)", marginBottom: 10 }}>
            {t("complexity.byModule")}
          </h3>
          <div style={{ borderRadius: "var(--radius-lg)", border: "1px solid var(--surface-border)", overflow: "hidden" }}>
            <table style={{ width: "100%", borderCollapse: "collapse", fontSize: 12 }}>
              <thead>
                <tr style={{ background: "var(--bg-2)" }}>
                  <th style={thStyle}>Module</th>
                  <th style={{ ...thStyle, textAlign: "right" }}>Count</th>
                  <th style={{ ...thStyle, textAlign: "right" }}>{t("complexity.avg")}</th>
                  <th style={{ ...thStyle, textAlign: "right" }}>{t("complexity.max")}</th>
                </tr>
              </thead>
              <tbody>
                {data.byModule.slice(0, 20).map((m) => (
                  <tr key={m.module} style={{ borderTop: "1px solid var(--surface-border)" }}>
                    <td style={{ ...tdStyle, color: "var(--text-0)", fontFamily: "var(--font-mono)" }}>{m.module}</td>
                    <td style={{ ...tdStyle, textAlign: "right", color: "var(--text-2)", fontVariantNumeric: "tabular-nums" }}>{m.symbolCount}</td>
                    <td style={{ ...tdStyle, textAlign: "right", color: "var(--text-0)", fontVariantNumeric: "tabular-nums" }}>
                      {m.avgComplexity.toFixed(1)}
                    </td>
                    <td style={{ ...tdStyle, textAlign: "right", color: "var(--rose)", fontVariantNumeric: "tabular-nums" }}>{m.maxComplexity}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </>
      )}
    </div>
  );
}

const thStyle: React.CSSProperties = { padding: "8px 12px", textAlign: "left", color: "var(--text-2)", fontWeight: 600 };
const tdStyle: React.CSSProperties = { padding: "6px 12px" };

function StatCard({ label, value, color }: { label: string; value: number | string; color?: string }) {
  return (
    <div
      style={{
        padding: "12px 14px",
        borderRadius: "var(--radius-lg)",
        border: "1px solid var(--surface-border)",
        background: "var(--bg-1)",
      }}
    >
      <div style={{ fontSize: 11, color: "var(--text-3)", marginBottom: 4 }}>{label}</div>
      <div style={{ fontSize: 20, fontWeight: 700, color: color || "var(--text-0)", fontVariantNumeric: "tabular-nums" }}>
        {value}
      </div>
    </div>
  );
}

function SeverityTile({ label, value, color }: { label: string; value: number; color: string }) {
  return (
    <div
      style={{
        padding: "12px 14px",
        borderRadius: "var(--radius-lg)",
        border: `1px solid ${color}44`,
        background: `${color}11`,
      }}
    >
      <div style={{ fontSize: 10, color, fontWeight: 600, textTransform: "uppercase", letterSpacing: 0.5, marginBottom: 4 }}>
        {label}
      </div>
      <div style={{ fontSize: 22, fontWeight: 700, color, fontVariantNumeric: "tabular-nums" }}>{value}</div>
    </div>
  );
}

function SeverityBadge({ severity }: { severity: string }) {
  const color =
    severity === "critical"
      ? "var(--rose)"
      : severity === "high"
      ? "#f28c28"
      : severity === "medium"
      ? "#e9c46a"
      : "var(--green, #73daca)";
  return (
    <span
      style={{
        fontSize: 10,
        fontWeight: 600,
        textTransform: "uppercase",
        padding: "2px 8px",
        borderRadius: "var(--radius-sm)",
        background: `${color}22`,
        color,
        letterSpacing: 0.5,
      }}
    >
      {severity}
    </span>
  );
}

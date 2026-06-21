import { useQuery } from "@tanstack/react-query";
import { ShieldCheck, AlertTriangle, CheckCircle, AlertCircle } from "lucide-react";
import { commands } from "../../lib/tauri-commands";
import { useI18n } from "../../hooks/use-i18n";
import { useAppStore } from "../../stores/app-store";
import { LoadingOrbs } from "../shared/LoadingOrbs";

export function CoverageView() {
  const { t } = useI18n();
  const activeRepo = useAppStore((s) => s.activeRepo);
  const setSelectedNodeId = useAppStore((s) => s.setSelectedNodeId);
  const setMode = useAppStore((s) => s.setMode);

  // Scope the query key to `activeRepo` so switching repos refetches instead
  // of serving cached coverage stats from the previously-opened repo for the
  // full `staleTime` window.
  const { data: stats, isLoading, error } = useQuery({
    queryKey: ["coverage-stats", activeRepo],
    queryFn: () => commands.getCoverageStats(),
    staleTime: 60_000,
  });

  if (error) return <ViewError error={error} />;

  if (isLoading) {
    return (
      <div className="flex items-center justify-center h-full">
        <LoadingOrbs />
      </div>
    );
  }

  if (!stats) return null;

  return (
    <div className="h-full overflow-auto" style={{ padding: 24 }}>
      <h2 className="text-lg font-semibold" style={{ color: "var(--text-0)", marginBottom: 20 }}>
        <ShieldCheck size={20} style={{ display: "inline", marginRight: 8, verticalAlign: "text-bottom" }} />
        {t("sidebar.coverage")}
      </h2>

      {/* Stats grid */}
      <div style={{ display: "grid", gridTemplateColumns: "repeat(auto-fit, minmax(150px, 1fr))", gap: 12, marginBottom: 24 }}>
        <StatCard label={t("coverage.totalMethods")} value={stats.totalMethods} />
        <StatCard label={t("health.tracing")} value={stats.tracedMethods} color="#73daca" />
        <StatCard label={t("coverage.deadCode")} value={stats.deadCodeCandidates} color="var(--rose)" />
        <StatCard label={t("coverage.coverageLabel")} value={`${stats.coveragePct}%`} color="var(--green)" />
      </div>

      {/* Dead code candidates */}
      {stats.deadMethods.length > 0 && (
        <div>
          <h3
            className="text-sm font-semibold"
            style={{ color: "var(--text-1)", marginBottom: 12, display: "flex", alignItems: "center", gap: 6 }}
          >
            <AlertTriangle size={14} style={{ color: "var(--rose)" }} />
            {t("coverage.deadCandidates")} ({stats.deadCodeCandidates})
          </h3>
          <div
            style={{
              borderRadius: "var(--radius-lg)",
              border: "1px solid var(--surface-border)",
              overflow: "hidden",
            }}
          >
            <table style={{ width: "100%", borderCollapse: "collapse", fontSize: 12 }}>
              <thead>
                <tr style={{ background: "var(--bg-2)" }}>
                  <th style={{ padding: "8px 12px", textAlign: "left", color: "var(--text-2)" }}>{t("coverage.method")}</th>
                  <th style={{ padding: "8px 12px", textAlign: "left", color: "var(--text-2)" }}>{t("coverage.class")}</th>
                  <th style={{ padding: "8px 12px", textAlign: "left", color: "var(--text-2)" }}>{t("coverage.file")}</th>
                </tr>
              </thead>
              <tbody>
                {stats.deadMethods.map((m) => (
                  <tr
                    key={m.nodeId}
                    className="cursor-pointer transition-colors hover:brightness-110"
                    style={{ borderTop: "1px solid var(--surface-border)" }}
                    onClick={() => {
                      setSelectedNodeId(m.nodeId, m.name);
                      setMode("explorer");
                    }}
                  >
                    <td style={{ padding: "6px 12px", color: "var(--text-0)", fontFamily: "var(--font-mono)" }}>
                      {m.name}
                    </td>
                    <td style={{ padding: "6px 12px", color: "var(--text-2)" }}>
                      {m.className || "—"}
                    </td>
                    <td
                      style={{
                        padding: "6px 12px",
                        color: "var(--text-3)",
                        maxWidth: 300,
                        overflow: "hidden",
                        textOverflow: "ellipsis",
                        whiteSpace: "nowrap",
                      }}
                    >
                      {m.filePath}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </div>
      )}

      {stats.deadMethods.length === 0 && (
        <div
          className="flex items-center gap-2"
          style={{ color: "var(--green)", padding: 16, background: "var(--bg-1)", borderRadius: "var(--radius-lg)" }}
        >
          <CheckCircle size={16} />
          {t("coverage.noDead")}
        </div>
      )}
    </div>
  );
}

function ViewError({ error }: { error: unknown }) {
  return (
    <div className="flex flex-col items-center justify-center h-full p-8 text-center">
      <AlertCircle size={40} style={{ color: "var(--rose)", marginBottom: 16 }} />
      <p style={{ fontSize: 13, color: "var(--text-3)", maxWidth: 400, lineHeight: 1.5 }}>
        {String(error)}
      </p>
    </div>
  );
}

function StatCard({ label, value, color }: { label: string; value: number | string; color?: string }) {
  return (
    <div
      style={{
        padding: "14px 16px",
        borderRadius: "var(--radius-lg)",
        border: "1px solid var(--surface-border)",
        background: "var(--bg-1)",
      }}
    >
      <div style={{ fontSize: 11, color: "var(--text-3)", marginBottom: 4 }}>{label}</div>
      <div style={{ fontSize: 22, fontWeight: 700, color: color || "var(--text-0)", fontVariantNumeric: "tabular-nums" }}>
        {value}
      </div>
    </div>
  );
}

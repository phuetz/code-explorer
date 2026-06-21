import { useMemo, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { RotateCcw, AlertCircle, CheckCircle } from "lucide-react";
import { commands } from "../../lib/tauri-commands";
import { useI18n } from "../../hooks/use-i18n";
import { useAppStore } from "../../stores/app-store";
import { LoadingOrbs } from "../shared/LoadingOrbs";

type Scope = "imports" | "calls";

export function CyclesView() {
  const { t } = useI18n();
  const activeRepo = useAppStore((s) => s.activeRepo);
  const setSelectedNodeId = useAppStore((s) => s.setSelectedNodeId);
  const setMode = useAppStore((s) => s.setMode);
  const [scope, setScope] = useState<Scope>("imports");

  const { data, isLoading, error } = useQuery({
    queryKey: ["cycles", activeRepo, scope],
    queryFn: () => commands.detectCycles(scope),
    staleTime: 60_000,
  });

  const cycles = useMemo(() => data ?? [], [data]);

  if (error) {
    return (
      <div className="flex flex-col items-center justify-center h-full p-8 text-center">
        <AlertCircle size={40} style={{ color: "var(--rose)", marginBottom: 16 }} />
        <p style={{ fontSize: 13, color: "var(--text-3)" }}>{String(error)}</p>
      </div>
    );
  }

  return (
    <div className="h-full overflow-auto" style={{ padding: 24 }}>
      <div className="flex items-center justify-between" style={{ marginBottom: 16 }}>
        <div>
          <h2 className="text-lg font-semibold" style={{ color: "var(--text-0)" }}>
            <RotateCcw size={18} style={{ display: "inline", marginRight: 8, verticalAlign: "text-bottom" }} />
            {t("cycles.title")}
          </h2>
          <p style={{ fontSize: 12, color: "var(--text-3)", marginTop: 4 }}>{t("cycles.subtitle")}</p>
        </div>
        <div className="flex gap-2">
          <ScopeToggle value={scope} onChange={setScope} t={t} />
        </div>
      </div>

      {isLoading && (
        <div className="flex items-center justify-center" style={{ padding: 48 }}>
          <LoadingOrbs />
        </div>
      )}

      {!isLoading && cycles.length === 0 && (
        <div
          className="flex items-center gap-2"
          style={{
            color: "var(--green)",
            padding: 16,
            background: "var(--bg-1)",
            borderRadius: "var(--radius-lg)",
          }}
        >
          <CheckCircle size={16} />
          {t("cycles.none")}
        </div>
      )}

      {!isLoading && cycles.length > 0 && (
        <>
          <div style={{ fontSize: 13, color: "var(--text-2)", marginBottom: 12 }}>
            {t("cycles.count").replace("{count}", String(cycles.length))}
          </div>
          <div style={{ display: "grid", gap: 12 }}>
            {cycles.map((c, idx) => (
              <div
                key={idx}
                style={{
                  border: "1px solid var(--surface-border)",
                  borderRadius: "var(--radius-lg)",
                  padding: 14,
                  background: "var(--bg-1)",
                }}
              >
                <div className="flex items-center gap-3" style={{ marginBottom: 8 }}>
                  <SeverityBadge severity={c.severity} />
                  <span style={{ fontSize: 12, color: "var(--text-3)" }}>
                    {t("cycles.length")}: <b style={{ color: "var(--text-0)" }}>{c.length}</b>
                  </span>
                </div>
                <div style={{ fontSize: 12, color: "var(--text-2)", lineHeight: 1.8 }}>
                  {c.names.map((name, i) => (
                    <span key={i}>
                      <button
                        onClick={() => {
                          setSelectedNodeId(c.nodes[i], name);
                          setMode("explorer");
                        }}
                        style={{
                          color: "var(--accent)",
                          fontFamily: "var(--font-mono)",
                          background: "transparent",
                          border: "none",
                          cursor: "pointer",
                          padding: 0,
                          textDecoration: "underline",
                        }}
                      >
                        {name || c.nodes[i]}
                      </button>
                      {i < c.names.length - 1 && <span style={{ color: "var(--text-3)" }}> → </span>}
                    </span>
                  ))}
                  <span style={{ color: "var(--text-3)" }}>
                    {" → "}
                    <em>{c.names[0] || c.nodes[0]}</em>
                  </span>
                </div>
                <div style={{ fontSize: 10, color: "var(--text-3)", marginTop: 8, fontFamily: "var(--font-mono)" }}>
                  {c.filePaths.filter((p) => p).join(", ")}
                </div>
              </div>
            ))}
          </div>
        </>
      )}
    </div>
  );
}

function ScopeToggle({ value, onChange, t }: { value: Scope; onChange: (s: Scope) => void; t: (k: string) => string }) {
  return (
    <div style={{ display: "inline-flex", borderRadius: "var(--radius-md)", border: "1px solid var(--surface-border)", overflow: "hidden" }}>
      <button
        onClick={() => onChange("imports")}
        style={{
          padding: "6px 12px",
          fontSize: 12,
          background: value === "imports" ? "var(--accent-subtle)" : "transparent",
          color: value === "imports" ? "var(--accent)" : "var(--text-2)",
          border: "none",
          cursor: "pointer",
        }}
      >
        {t("cycles.scope.imports")}
      </button>
      <button
        onClick={() => onChange("calls")}
        style={{
          padding: "6px 12px",
          fontSize: 12,
          background: value === "calls" ? "var(--accent-subtle)" : "transparent",
          color: value === "calls" ? "var(--accent)" : "var(--text-2)",
          border: "none",
          cursor: "pointer",
          borderLeft: "1px solid var(--surface-border)",
        }}
      >
        {t("cycles.scope.calls")}
      </button>
    </div>
  );
}

function SeverityBadge({ severity }: { severity: string }) {
  const color =
    severity === "high" ? "var(--rose)" : severity === "medium" ? "var(--yellow, #e9c46a)" : "var(--text-3)";
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

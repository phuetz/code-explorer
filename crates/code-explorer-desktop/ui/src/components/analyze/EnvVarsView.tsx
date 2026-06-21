import { useMemo, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { KeyRound, AlertCircle, Info } from "lucide-react";
import { commands } from "../../lib/tauri-commands";
import { useI18n } from "../../hooks/use-i18n";
import { useAppStore } from "../../stores/app-store";
import { LoadingOrbs } from "../shared/LoadingOrbs";

type StatusFilter = "all" | "unused" | "undeclared";

export function EnvVarsView() {
  const { t } = useI18n();
  const activeRepo = useAppStore((s) => s.activeRepo);
  const [filter, setFilter] = useState<StatusFilter>("all");

  const { data, isLoading, error } = useQuery({
    queryKey: ["env-vars", activeRepo],
    // Always fetch all; filter client-side for fast toggles.
    queryFn: () => commands.listEnvVars(false),
    staleTime: 60_000,
  });

  const all = useMemo(() => data ?? [], [data]);
  const filtered = useMemo(() => {
    if (filter === "all") return all;
    if (filter === "unused") return all.filter((v) => v.unused);
    return all.filter((v) => v.undeclared);
  }, [all, filter]);

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
      <div className="flex items-center justify-between" style={{ marginBottom: 16, flexWrap: "wrap", gap: 12 }}>
        <div>
          <h2 className="text-lg font-semibold" style={{ color: "var(--text-0)" }}>
            <KeyRound size={18} style={{ display: "inline", marginRight: 8, verticalAlign: "text-bottom" }} />
            {t("envvars.title")}
          </h2>
          <p style={{ fontSize: 12, color: "var(--text-3)", marginTop: 4 }}>{t("envvars.subtitle")}</p>
        </div>
        <div style={{ display: "inline-flex", borderRadius: "var(--radius-md)", border: "1px solid var(--surface-border)", overflow: "hidden" }}>
          {(["all", "unused", "undeclared"] as const).map((f) => (
            <button
              key={f}
              onClick={() => setFilter(f)}
              style={{
                padding: "6px 12px",
                fontSize: 12,
                background: filter === f ? "var(--accent-subtle)" : "transparent",
                color: filter === f ? "var(--accent)" : "var(--text-2)",
                border: "none",
                cursor: "pointer",
                borderLeft: f === "all" ? "none" : "1px solid var(--surface-border)",
              }}
            >
              {f === "all"
                ? t("envvars.all")
                : f === "unused"
                ? t("envvars.statusUnused")
                : t("envvars.statusUndeclared")}
            </button>
          ))}
        </div>
      </div>

      {isLoading && (
        <div className="flex items-center justify-center" style={{ padding: 48 }}>
          <LoadingOrbs />
        </div>
      )}

      {!isLoading && filtered.length === 0 && (
        <div
          className="flex items-center gap-2"
          style={{ color: "var(--text-2)", padding: 16, background: "var(--bg-1)", borderRadius: "var(--radius-lg)" }}
        >
          <Info size={16} />
          {t("envvars.none")}
        </div>
      )}

      {!isLoading && filtered.length > 0 && (
        <>
          <div style={{ fontSize: 13, color: "var(--text-2)", marginBottom: 12 }}>
            {t("envvars.count").replace("{count}", String(filtered.length))}
          </div>
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
                  <th style={thStyle}>{t("envvars.name")}</th>
                  <th style={thStyle}>{t("envvars.status")}</th>
                  <th style={thStyle}>{t("envvars.declaredIn")}</th>
                  <th style={{ ...thStyle, textAlign: "right" }}>{t("envvars.usedInCount")}</th>
                </tr>
              </thead>
              <tbody>
                {filtered.map((v) => (
                  <tr key={v.nodeId} style={{ borderTop: "1px solid var(--surface-border)" }}>
                    <td style={{ ...tdStyle, fontFamily: "var(--font-mono)", color: "var(--text-0)" }}>{v.name}</td>
                    <td style={tdStyle}>
                      <StatusBadge unused={v.unused} undeclared={v.undeclared} tOk={t("envvars.statusOk")} tUnused={t("envvars.statusUnused")} tUndeclared={t("envvars.statusUndeclared")} />
                    </td>
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
                      title={v.declaredIn ?? ""}
                    >
                      {v.declaredIn || <span style={{ color: "var(--text-3)" }}>—</span>}
                    </td>
                    <td style={{ ...tdStyle, textAlign: "right", color: "var(--text-2)", fontFamily: "var(--font-mono)" }}>
                      {v.usedInCount}
                    </td>
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

function StatusBadge({
  unused,
  undeclared,
  tOk,
  tUnused,
  tUndeclared,
}: {
  unused: boolean;
  undeclared: boolean;
  tOk: string;
  tUnused: string;
  tUndeclared: string;
}) {
  const [label, color] = undeclared
    ? [tUndeclared, "var(--rose)"]
    : unused
    ? [tUnused, "var(--yellow, #e9c46a)"]
    : [tOk, "var(--green, #34d399)"];
  return (
    <span
      style={{
        fontSize: 10,
        fontWeight: 600,
        letterSpacing: 0.5,
        padding: "2px 8px",
        borderRadius: "var(--radius-sm)",
        background: `${color}22`,
        color,
        fontFamily: "var(--font-mono)",
      }}
    >
      {label}
    </span>
  );
}

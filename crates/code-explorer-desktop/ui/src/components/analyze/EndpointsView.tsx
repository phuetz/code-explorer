import { useMemo, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { Globe, AlertCircle, Info } from "lucide-react";
import { commands } from "../../lib/tauri-commands";
import { useI18n } from "../../hooks/use-i18n";
import { useAppStore } from "../../stores/app-store";
import { LoadingOrbs } from "../shared/LoadingOrbs";

const METHODS = ["all", "GET", "POST", "PUT", "DELETE", "PATCH"] as const;
type MethodFilter = (typeof METHODS)[number];

export function EndpointsView() {
  const { t } = useI18n();
  const activeRepo = useAppStore((s) => s.activeRepo);
  const [method, setMethod] = useState<MethodFilter>("all");
  const [query, setQuery] = useState("");

  const { data, isLoading, error } = useQuery({
    queryKey: ["endpoints", activeRepo],
    queryFn: () => commands.listEndpoints(),
    staleTime: 60_000,
  });

  const all = useMemo(() => data ?? [], [data]);
  const filtered = useMemo(() => {
    const q = query.trim().toLowerCase();
    return all.filter((e) => {
      if (method !== "all" && e.httpMethod.toUpperCase() !== method) return false;
      if (q && !e.route.toLowerCase().includes(q)) return false;
      return true;
    });
  }, [all, method, query]);

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
      <div
        className="flex items-center justify-between"
        style={{ marginBottom: 16, flexWrap: "wrap", gap: 12 }}
      >
        <div>
          <h2 className="text-lg font-semibold" style={{ color: "var(--text-0)" }}>
            <Globe size={18} style={{ display: "inline", marginRight: 8, verticalAlign: "text-bottom" }} />
            {t("endpoints.title")}
          </h2>
          <p style={{ fontSize: 12, color: "var(--text-3)", marginTop: 4 }}>{t("endpoints.subtitle")}</p>
        </div>
        <div style={{ display: "flex", gap: 8, alignItems: "center", flexWrap: "wrap" }}>
          <input
            type="text"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            placeholder={t("endpoints.filterPlaceholder")}
            style={{
              padding: "6px 10px",
              fontSize: 12,
              border: "1px solid var(--surface-border)",
              borderRadius: "var(--radius-md)",
              background: "var(--bg-1)",
              color: "var(--text-0)",
              width: 220,
            }}
          />
          <div style={{ display: "inline-flex", borderRadius: "var(--radius-md)", border: "1px solid var(--surface-border)", overflow: "hidden" }}>
            {METHODS.map((m) => (
              <button
                key={m}
                onClick={() => setMethod(m)}
                style={{
                  padding: "6px 10px",
                  fontSize: 12,
                  background: method === m ? "var(--accent-subtle)" : "transparent",
                  color: method === m ? "var(--accent)" : "var(--text-2)",
                  border: "none",
                  cursor: "pointer",
                  borderLeft: m === "all" ? "none" : "1px solid var(--surface-border)",
                  fontFamily: "var(--font-mono)",
                }}
              >
                {m === "all" ? t("endpoints.allMethods") : m}
              </button>
            ))}
          </div>
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
          {t("endpoints.none")}
        </div>
      )}

      {!isLoading && filtered.length > 0 && (
        <>
          <div style={{ fontSize: 13, color: "var(--text-2)", marginBottom: 12 }}>
            {t("endpoints.count").replace("{count}", String(filtered.length))}
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
                  <th style={thStyle}>{t("endpoints.method")}</th>
                  <th style={thStyle}>{t("endpoints.route")}</th>
                  <th style={thStyle}>{t("endpoints.framework")}</th>
                  <th style={thStyle}>{t("endpoints.handler")}</th>
                  <th style={thStyle}>{t("endpoints.file")}</th>
                </tr>
              </thead>
              <tbody>
                {filtered.map((ep) => (
                  <tr key={ep.nodeId} style={{ borderTop: "1px solid var(--surface-border)" }}>
                    <td style={tdStyle}>
                      <MethodBadge method={ep.httpMethod} />
                    </td>
                    <td style={{ ...tdStyle, color: "var(--text-0)", fontFamily: "var(--font-mono)" }}>{ep.route}</td>
                    <td style={{ ...tdStyle, color: "var(--text-3)", fontSize: 11 }}>
                      {ep.framework ?? "—"}
                    </td>
                    <td style={{ ...tdStyle, color: "var(--text-2)", fontFamily: "var(--font-mono)" }}>
                      {ep.handlerName || <span style={{ color: "var(--text-3)" }}>—</span>}
                    </td>
                    <td
                      style={{
                        ...tdStyle,
                        color: "var(--text-2)",
                        fontFamily: "var(--font-mono)",
                        maxWidth: 280,
                        overflow: "hidden",
                        textOverflow: "ellipsis",
                        whiteSpace: "nowrap",
                      }}
                      title={ep.filePath}
                    >
                      {ep.filePath}
                      {ep.startLine ? <span style={{ color: "var(--text-3)" }}>:{ep.startLine}</span> : null}
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

function MethodBadge({ method }: { method: string }) {
  const palette: Record<string, string> = {
    GET: "var(--accent)",
    POST: "var(--green, #34d399)",
    PUT: "var(--yellow, #e9c46a)",
    DELETE: "var(--rose)",
    PATCH: "var(--lavender, #b794f6)",
  };
  const color = palette[method.toUpperCase()] ?? "var(--text-3)";
  return (
    <span
      style={{
        fontSize: 10,
        fontWeight: 700,
        letterSpacing: 0.5,
        padding: "2px 8px",
        borderRadius: "var(--radius-sm)",
        background: `${color}22`,
        color,
        fontFamily: "var(--font-mono)",
      }}
    >
      {method}
    </span>
  );
}

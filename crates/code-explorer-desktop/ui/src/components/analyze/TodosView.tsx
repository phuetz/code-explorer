import { useMemo, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { CheckSquare, AlertCircle, CheckCircle } from "lucide-react";
import { commands } from "../../lib/tauri-commands";
import { useI18n } from "../../hooks/use-i18n";
import { useAppStore } from "../../stores/app-store";
import { LoadingOrbs } from "../shared/LoadingOrbs";

const KINDS = ["all", "TODO", "FIXME", "HACK", "XXX"] as const;
type KindFilter = (typeof KINDS)[number];

export function TodosView() {
  const { t } = useI18n();
  const activeRepo = useAppStore((s) => s.activeRepo);
  const [filter, setFilter] = useState<KindFilter>("all");

  const { data, isLoading, error } = useQuery({
    queryKey: ["todos", activeRepo],
    // Always fetch all; filter client-side for fast toggles.
    queryFn: () => commands.listTodos(undefined, 500),
    staleTime: 60_000,
  });

  const all = useMemo(() => data ?? [], [data]);
  const filtered = useMemo(
    () => (filter === "all" ? all : all.filter((t) => t.kind === filter)),
    [all, filter],
  );

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
            <CheckSquare size={18} style={{ display: "inline", marginRight: 8, verticalAlign: "text-bottom" }} />
            {t("todos.title")}
          </h2>
          <p style={{ fontSize: 12, color: "var(--text-3)", marginTop: 4 }}>{t("todos.subtitle")}</p>
        </div>
        <div style={{ display: "inline-flex", borderRadius: "var(--radius-md)", border: "1px solid var(--surface-border)", overflow: "hidden" }}>
          {KINDS.map((k) => (
            <button
              key={k}
              onClick={() => setFilter(k)}
              style={{
                padding: "6px 12px",
                fontSize: 12,
                background: filter === k ? "var(--accent-subtle)" : "transparent",
                color: filter === k ? "var(--accent)" : "var(--text-2)",
                border: "none",
                cursor: "pointer",
                borderLeft: k === "all" ? "none" : "1px solid var(--surface-border)",
              }}
            >
              {k === "all" ? t("todos.all") : k}
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
          style={{ color: "var(--green)", padding: 16, background: "var(--bg-1)", borderRadius: "var(--radius-lg)" }}
        >
          <CheckCircle size={16} />
          {t("todos.none")}
        </div>
      )}

      {!isLoading && filtered.length > 0 && (
        <>
          <div style={{ fontSize: 13, color: "var(--text-2)", marginBottom: 12 }}>
            {t("todos.count").replace("{count}", String(filtered.length))}
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
                  <th style={thStyle}>{t("todos.kind")}</th>
                  <th style={thStyle}>{t("todos.text")}</th>
                  <th style={thStyle}>{t("todos.file")}</th>
                  <th style={{ ...thStyle, textAlign: "right" }}>{t("todos.line")}</th>
                </tr>
              </thead>
              <tbody>
                {filtered.map((entry) => (
                  <tr key={entry.nodeId} style={{ borderTop: "1px solid var(--surface-border)" }}>
                    <td style={tdStyle}>
                      <KindBadge kind={entry.kind} />
                    </td>
                    <td
                      style={{
                        ...tdStyle,
                        color: "var(--text-0)",
                        maxWidth: 420,
                        overflow: "hidden",
                        textOverflow: "ellipsis",
                        whiteSpace: "nowrap",
                      }}
                      title={entry.text ?? ""}
                    >
                      {entry.text || <span style={{ color: "var(--text-3)" }}>—</span>}
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
                      title={entry.filePath}
                    >
                      {entry.filePath}
                    </td>
                    <td style={{ ...tdStyle, textAlign: "right", color: "var(--text-3)", fontFamily: "var(--font-mono)" }}>
                      {entry.line ?? "—"}
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

function KindBadge({ kind }: { kind: string }) {
  const color =
    kind === "FIXME"
      ? "var(--rose)"
      : kind === "HACK"
      ? "var(--yellow, #e9c46a)"
      : kind === "XXX"
      ? "var(--text-3)"
      : "var(--accent)";
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
      {kind}
    </span>
  );
}

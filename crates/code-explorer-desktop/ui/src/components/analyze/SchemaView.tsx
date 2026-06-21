import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { Database, AlertCircle, Info, ChevronDown, ChevronRight, Key } from "lucide-react";
import { commands } from "../../lib/tauri-commands";
import { useI18n } from "../../hooks/use-i18n";
import { useAppStore } from "../../stores/app-store";
import { LoadingOrbs } from "../shared/LoadingOrbs";

export function SchemaView() {
  const { t } = useI18n();
  const activeRepo = useAppStore((s) => s.activeRepo);
  const [expanded, setExpanded] = useState<Set<string>>(new Set());

  const { data, isLoading, error } = useQuery({
    queryKey: ["db-tables", activeRepo],
    queryFn: () => commands.listDbTables(),
    staleTime: 60_000,
  });

  const tables = data ?? [];
  const toggle = (id: string) => {
    setExpanded((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  };

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
      <div style={{ marginBottom: 16 }}>
        <h2 className="text-lg font-semibold" style={{ color: "var(--text-0)" }}>
          <Database size={18} style={{ display: "inline", marginRight: 8, verticalAlign: "text-bottom" }} />
          {t("schema.title")}
        </h2>
        <p style={{ fontSize: 12, color: "var(--text-3)", marginTop: 4 }}>{t("schema.subtitle")}</p>
      </div>

      {isLoading && (
        <div className="flex items-center justify-center" style={{ padding: 48 }}>
          <LoadingOrbs />
        </div>
      )}

      {!isLoading && tables.length === 0 && (
        <div
          className="flex items-center gap-2"
          style={{ color: "var(--text-2)", padding: 16, background: "var(--bg-1)", borderRadius: "var(--radius-lg)" }}
        >
          <Info size={16} />
          {t("schema.none")}
        </div>
      )}

      {!isLoading && tables.length > 0 && (
        <>
          <div style={{ fontSize: 13, color: "var(--text-2)", marginBottom: 12 }}>
            {t("schema.count").replace("{count}", String(tables.length))}
          </div>
          <div style={{ display: "flex", flexDirection: "column", gap: 8 }}>
            {tables.map((table) => {
              const isOpen = expanded.has(table.nodeId);
              return (
                <div
                  key={table.nodeId}
                  style={{
                    borderRadius: "var(--radius-lg)",
                    border: "1px solid var(--surface-border)",
                    overflow: "hidden",
                    background: "var(--bg-1)",
                  }}
                >
                  <button
                    onClick={() => toggle(table.nodeId)}
                    style={{
                      display: "flex",
                      alignItems: "center",
                      gap: 10,
                      width: "100%",
                      padding: "10px 14px",
                      background: "var(--bg-2)",
                      border: "none",
                      cursor: "pointer",
                      textAlign: "left",
                      color: "var(--text-0)",
                    }}
                  >
                    {isOpen ? <ChevronDown size={14} /> : <ChevronRight size={14} />}
                    <strong style={{ fontFamily: "var(--font-mono)", fontSize: 13 }}>{table.name}</strong>
                    <span style={{ flex: 1 }} />
                    <span style={{ fontSize: 11, color: "var(--text-3)" }}>
                      {table.columnCount} {t("schema.columnCount").toLowerCase()}
                    </span>
                    {table.fkCount > 0 && (
                      <span style={{ fontSize: 11, color: "var(--text-3)" }}>
                        · {table.fkCount} {t("schema.fkCount").toLowerCase()}
                      </span>
                    )}
                    {table.filePath && (
                      <span
                        style={{
                          fontSize: 11,
                          color: "var(--text-3)",
                          fontFamily: "var(--font-mono)",
                          maxWidth: 260,
                          overflow: "hidden",
                          textOverflow: "ellipsis",
                          whiteSpace: "nowrap",
                        }}
                        title={table.filePath}
                      >
                        {table.filePath}
                      </span>
                    )}
                  </button>
                  {isOpen && table.columns.length > 0 && (
                    <table style={{ width: "100%", borderCollapse: "collapse", fontSize: 12 }}>
                      <thead>
                        <tr style={{ background: "var(--bg-1)" }}>
                          <th style={thStyle}>{t("schema.column")}</th>
                          <th style={thStyle}>{t("schema.type")}</th>
                          <th style={{ ...thStyle, width: 60, textAlign: "center" }}>{t("schema.pk")}</th>
                          <th style={{ ...thStyle, width: 80, textAlign: "center" }}>{t("schema.nullable")}</th>
                        </tr>
                      </thead>
                      <tbody>
                        {table.columns.map((col) => (
                          <tr key={col.nodeId} style={{ borderTop: "1px solid var(--surface-border)" }}>
                            <td style={{ ...tdStyle, fontFamily: "var(--font-mono)", color: "var(--text-0)" }}>
                              {col.name}
                            </td>
                            <td style={{ ...tdStyle, fontFamily: "var(--font-mono)", color: "var(--text-2)" }}>
                              {col.columnType ?? "—"}
                            </td>
                            <td style={{ ...tdStyle, textAlign: "center" }}>
                              {col.isPrimaryKey ? <Key size={12} style={{ color: "var(--accent)" }} /> : null}
                            </td>
                            <td style={{ ...tdStyle, textAlign: "center", color: "var(--text-3)" }}>
                              {col.isNullable ? "✓" : ""}
                            </td>
                          </tr>
                        ))}
                      </tbody>
                    </table>
                  )}
                </div>
              );
            })}
          </div>
        </>
      )}
    </div>
  );
}

const thStyle: React.CSSProperties = { padding: "8px 12px", textAlign: "left", color: "var(--text-2)", fontWeight: 600 };
const tdStyle: React.CSSProperties = { padding: "6px 12px" };

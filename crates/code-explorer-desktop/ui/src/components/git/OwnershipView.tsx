/**
 * Code Ownership view — author distribution per file.
 */

import { useMemo } from "react";
import { Users } from "lucide-react";
import type { GitOwnership } from "../../lib/tauri-commands";
import { useI18n } from "../../hooks/use-i18n";
import { LoadingOrbs } from "../shared/LoadingOrbs";
import { EmptyState } from "../shared/EmptyState";

interface Props {
  data: GitOwnership[];
  loading: boolean;
}

const AUTHOR_COLORS = [
  "var(--accent)",
  "var(--green)",
  "#bb9af7",
  "var(--amber)",
  "var(--rose)",
  "#7dcfff",
  "#73daca",
  "#ff9e64",
];

export function OwnershipView({ data, loading }: Props) {
  const { t } = useI18n();

  // All hooks must be called before any early return (rules-of-hooks)
  const authorSummary = useMemo(() => {
    if (data.length === 0) return [];
    const map = new Map<string, number>();
    for (const o of data) {
      map.set(o.primaryAuthor, (map.get(o.primaryAuthor) || 0) + 1);
    }
    return Array.from(map.entries())
      .sort((a, b) => b[1] - a[1]);
  }, [data]);

  const orphans = useMemo(() => data.filter((o) => o.ownershipPct < 50), [data]);

  if (loading) return <LoadingOrbs label={t("ownership.loading")} />;

  if (data.length === 0) {
    return (
      <EmptyState
        icon={Users}
        title={t("ownership.noData")}
        description={t("ownership.noDataHint")}
      />
    );
  }

  return (
    <div style={{ padding: 16 }}>
      {/* Author summary */}
      <div style={{ marginBottom: 20 }}>
        <div
          style={{
            fontSize: 12,
            fontWeight: 600,
            color: "var(--text-1)",
            marginBottom: 8,
          }}
        >
          {t("ownership.authors").replace("{0}", String(authorSummary.length))}
        </div>
        <div style={{ display: "flex", flexWrap: "wrap", gap: 8 }}>
          {authorSummary.slice(0, 10).map(([author, count], i) => (
            <div
              key={author}
              style={{
                padding: "4px 10px",
                borderRadius: 6,
                border: "1px solid var(--border)",
                background: "var(--bg-1)",
                fontSize: 11,
                display: "flex",
                alignItems: "center",
                gap: 6,
              }}
            >
              <span
                style={{
                  width: 8,
                  height: 8,
                  borderRadius: "50%",
                  background: AUTHOR_COLORS[i % AUTHOR_COLORS.length],
                }}
              />
              <span style={{ color: "var(--text-1)" }}>{author}</span>
              <span style={{ color: "var(--text-3)", fontSize: 10 }}>
                {count} {t("ownership.files")}
              </span>
            </div>
          ))}
        </div>
      </div>

      {/* Orphan warning */}
      {orphans.length > 0 && (
        <div
          style={{
            padding: "8px 12px",
            borderRadius: 8,
            border: "1px solid color-mix(in srgb, var(--rose) 30%, transparent)",
            background: "color-mix(in srgb, var(--rose) 5%, transparent)",
            marginBottom: 16,
            fontSize: 12,
            color: "var(--rose)",
          }}
        >
          {t("ownership.orphanWarning").replace("{0}", String(orphans.length))}
        </div>
      )}

      {/* Files table — sorted by author_count desc */}
      <table style={{ width: "100%", borderCollapse: "collapse", fontSize: 12 }}>
        <thead>
          <tr
            style={{
              borderBottom: "1px solid var(--border)",
              color: "var(--text-2)",
              textAlign: "left",
            }}
          >
            <th style={{ padding: "6px 8px", fontWeight: 600 }}>{t("ownership.colFile")}</th>
            <th style={{ padding: "6px 8px", fontWeight: 600 }}>{t("ownership.colPrimaryAuthor")}</th>
            <th style={{ padding: "6px 8px", fontWeight: 600, width: 120 }}>{t("ownership.colOwnership")}</th>
            <th style={{ padding: "6px 8px", fontWeight: 600, textAlign: "right" }}>{t("ownership.colAuthors")}</th>
          </tr>
        </thead>
        <tbody>
          {data.slice(0, 30).map((o) => (
            <tr
              key={o.path}
              className="hover:brightness-110"
              style={{
                borderBottom: "1px solid var(--border)",
                color: "var(--text-1)",
              }}
            >
              <td
                style={{
                  padding: "6px 8px",
                  fontFamily: "var(--font-mono)",
                  fontSize: 11,
                  maxWidth: 300,
                  overflow: "hidden",
                  textOverflow: "ellipsis",
                  whiteSpace: "nowrap",
                }}
              >
                {o.path.replace(/\\/g, "/")}
              </td>
              <td style={{ padding: "6px 8px" }}>{o.primaryAuthor}</td>
              <td style={{ padding: "6px 8px" }}>
                <div style={{ display: "flex", alignItems: "center", gap: 6 }}>
                  {/* Stacked bar of authors */}
                  <div
                    style={{
                      flex: 1,
                      height: 8,
                      borderRadius: 4,
                      background: "var(--bg-3)",
                      overflow: "hidden",
                      display: "flex",
                    }}
                  >
                    {o.authors.slice(0, 4).map((a, j) => (
                      <div
                        key={a.name}
                        title={`${a.name}: ${Math.round(a.pct)}%`}
                        style={{
                          width: `${a.pct}%`,
                          height: "100%",
                          background: AUTHOR_COLORS[j % AUTHOR_COLORS.length],
                        }}
                      />
                    ))}
                  </div>
                  <span
                    style={{
                      fontSize: 10,
                      color: o.ownershipPct < 50 ? "var(--rose)" : "var(--text-2)",
                      minWidth: 32,
                      textAlign: "right",
                    }}
                  >
                    {Math.round(o.ownershipPct)}%
                  </span>
                </div>
              </td>
              <td style={{ padding: "6px 8px", textAlign: "right" }}>
                {o.authorCount}
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

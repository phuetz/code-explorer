/**
 * Temporal Coupling view — file pairs that change together.
 */

import { Link2 } from "lucide-react";
import type { GitCoupling } from "../../lib/tauri-commands";
import { useI18n } from "../../hooks/use-i18n";
import { LoadingOrbs } from "../shared/LoadingOrbs";
import { EmptyState } from "../shared/EmptyState";

interface Props {
  data: GitCoupling[];
  loading: boolean;
}

function strengthColor(s: number): string {
  if (s >= 0.7) return "var(--rose)";
  if (s >= 0.4) return "var(--amber)";
  return "var(--green)";
}

export function CouplingView({ data, loading }: Props) {
  const { t } = useI18n();

  if (loading) return <LoadingOrbs label={t("coupling.loading")} />;

  if (data.length === 0) {
    return (
      <EmptyState
        icon={Link2}
        title={t("coupling.noData")}
        description={t("coupling.noDataHint")}
      />
    );
  }

  const strong = data.filter((c) => c.couplingStrength > 0.7).length;

  return (
    <div style={{ padding: 16 }}>
      <div
        style={{
          fontSize: 11,
          color: "var(--text-3)",
          marginBottom: 12,
          display: "flex",
          gap: 16,
        }}
      >
        <span>{t("coupling.pairsDetected").replace("{0}", String(data.length))}</span>
        {strong > 0 && (
          <span style={{ color: "var(--rose)" }}>
            {t("coupling.stronglyCoupled").replace("{0}", String(strong))}
          </span>
        )}
      </div>

      <table style={{ width: "100%", borderCollapse: "collapse", fontSize: 12 }}>
        <thead>
          <tr
            style={{
              borderBottom: "1px solid var(--border)",
              color: "var(--text-2)",
              textAlign: "left",
            }}
          >
            <th style={{ padding: "6px 8px", fontWeight: 600 }}>{t("coupling.colRank")}</th>
            <th style={{ padding: "6px 8px", fontWeight: 600 }}>{t("coupling.colFileA")}</th>
            <th style={{ padding: "6px 8px", fontWeight: 600 }}>{t("coupling.colFileB")}</th>
            <th style={{ padding: "6px 8px", fontWeight: 600, textAlign: "right" }}>{t("coupling.colShared")}</th>
            <th style={{ padding: "6px 8px", fontWeight: 600, width: 120 }}>{t("coupling.colStrength")}</th>
          </tr>
        </thead>
        <tbody>
          {data.slice(0, 30).map((c, i) => (
            <tr
              key={`${c.fileA}-${c.fileB}`}
              className="hover:brightness-110"
              style={{
                borderBottom: "1px solid var(--border)",
                color: "var(--text-1)",
              }}
            >
              <td style={{ padding: "6px 8px", color: "var(--text-3)" }}>
                {i + 1}
              </td>
              <td
                style={{
                  padding: "6px 8px",
                  fontFamily: "var(--font-mono)",
                  fontSize: 11,
                  maxWidth: 250,
                  overflow: "hidden",
                  textOverflow: "ellipsis",
                  whiteSpace: "nowrap",
                }}
              >
                {c.fileA.replace(/\\/g, "/")}
              </td>
              <td
                style={{
                  padding: "6px 8px",
                  fontFamily: "var(--font-mono)",
                  fontSize: 11,
                  maxWidth: 250,
                  overflow: "hidden",
                  textOverflow: "ellipsis",
                  whiteSpace: "nowrap",
                }}
              >
                {c.fileB.replace(/\\/g, "/")}
              </td>
              <td style={{ padding: "6px 8px", textAlign: "right" }}>
                {c.sharedCommits}
              </td>
              <td style={{ padding: "6px 8px" }}>
                <div style={{ display: "flex", alignItems: "center", gap: 6 }}>
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
                        width: `${c.couplingStrength * 100}%`,
                        height: "100%",
                        borderRadius: 3,
                        background: strengthColor(c.couplingStrength),
                        transition: "width 0.3s ease",
                      }}
                    />
                  </div>
                  <span
                    style={{
                      fontSize: 10,
                      fontWeight: 600,
                      color: strengthColor(c.couplingStrength),
                      minWidth: 32,
                      textAlign: "right",
                    }}
                  >
                    {Math.round(c.couplingStrength * 100)}%
                  </span>
                </div>
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

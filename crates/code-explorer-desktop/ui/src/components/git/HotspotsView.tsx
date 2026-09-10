/**
 * Hotspots view — most changed files with risk scoring.
 */

import { useState, useMemo } from "react";
import { Flame, ArrowUpDown } from "lucide-react";
import type { GitHotspot } from "../../lib/tauri-commands";
import { useI18n } from "../../hooks/use-i18n";
import { LoadingOrbs } from "../shared/LoadingOrbs";
import { EmptyState } from "../shared/EmptyState";

type SortKey = "score" | "commitCount" | "authorCount" | "path";
type SortDir = "asc" | "desc";

interface Props {
  data: GitHotspot[];
  loading: boolean;
}

function scoreColor(score: number): string {
  if (score >= 0.7) return "var(--rose)";
  if (score >= 0.4) return "var(--amber)";
  return "var(--green)";
}

export function HotspotsView({ data, loading }: Props) {
  const { t } = useI18n();
  const [sortKey, setSortKey] = useState<SortKey>("score");
  const [sortDir, setSortDir] = useState<SortDir>("desc");

  const toggleSort = (key: SortKey) => {
    if (sortKey === key) setSortDir(d => d === "asc" ? "desc" : "asc");
    else { setSortKey(key); setSortDir("desc"); }
  };

  const sorted = useMemo(() => {
    const copy = [...data];
    copy.sort((a, b) => {
      let cmp = 0;
      switch (sortKey) {
        case "score": cmp = a.score - b.score; break;
        case "commitCount": cmp = a.commitCount - b.commitCount; break;
        case "authorCount": cmp = a.authorCount - b.authorCount; break;
        case "path": cmp = a.path.localeCompare(b.path); break;
      }
      return sortDir === "desc" ? -cmp : cmp;
    });
    return copy.slice(0, 50);
  }, [data, sortKey, sortDir]);

  if (loading) return <LoadingOrbs label={t("hotspots.loading")} />;

  if (data.length === 0) {
    return (
      <EmptyState
        icon={Flame}
        title={t("hotspots.noData")}
        description={t("hotspots.noDataHint")}
      />
    );
  }

  return (
    <div style={{ padding: 16 }}>
      <div
        style={{
          fontSize: 11,
          color: "var(--text-3)",
          marginBottom: 12,
        }}
      >
        {t("hotspots.filesAnalyzed").replace("{0}", String(data.length))}
      </div>

      {/* Table */}
      <table style={{ width: "100%", borderCollapse: "collapse", fontSize: 12 }}>
        <thead>
          <tr
            style={{
              borderBottom: "1px solid var(--border)",
              color: "var(--text-2)",
              textAlign: "left",
            }}
          >
            <th style={{ padding: "6px 8px", fontWeight: 600 }}>#</th>
            <th onClick={() => toggleSort("path")} style={{ padding: "6px 8px", fontWeight: 600, cursor: "pointer" }}>
              <span className="inline-flex items-center gap-1">{t("hotspots.colFile")} <ArrowUpDown size={10} style={{ opacity: sortKey === "path" ? 1 : 0.3 }} /></span>
            </th>
            <th onClick={() => toggleSort("commitCount")} style={{ padding: "6px 8px", fontWeight: 600, textAlign: "right", cursor: "pointer" }}>
              <span className="inline-flex items-center gap-1">{t("hotspots.colCommits")} <ArrowUpDown size={10} style={{ opacity: sortKey === "commitCount" ? 1 : 0.3 }} /></span>
            </th>
            <th style={{ padding: "6px 8px", fontWeight: 600, textAlign: "right" }}>{t("hotspots.colChurn")}</th>
            <th onClick={() => toggleSort("authorCount")} style={{ padding: "6px 8px", fontWeight: 600, textAlign: "right", cursor: "pointer" }}>
              <span className="inline-flex items-center gap-1">{t("hotspots.colAuthors")} <ArrowUpDown size={10} style={{ opacity: sortKey === "authorCount" ? 1 : 0.3 }} /></span>
            </th>
            <th onClick={() => toggleSort("score")} style={{ padding: "6px 8px", fontWeight: 600, width: 120, cursor: "pointer" }}>
              <span className="inline-flex items-center gap-1">{t("hotspots.colScore")} <ArrowUpDown size={10} style={{ opacity: sortKey === "score" ? 1 : 0.3 }} /></span>
            </th>
          </tr>
        </thead>
        <tbody>
          {sorted.map((h, i) => (
            <tr
              key={h.path}
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
                title={h.path.replace(/\\/g, "/")}
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
                {h.path.replace(/\\/g, "/")}
              </td>
              <td style={{ padding: "6px 8px", textAlign: "right" }}>
                {h.commitCount}
              </td>
              <td style={{ padding: "6px 8px", textAlign: "right", color: "var(--text-2)" }}>
                +{h.linesAdded}/-{h.linesRemoved}
              </td>
              <td style={{ padding: "6px 8px", textAlign: "right" }}>
                {h.authorCount}
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
                        width: `${h.score * 100}%`,
                        height: "100%",
                        borderRadius: 3,
                        background: scoreColor(h.score),
                        transition: "width 0.3s ease",
                      }}
                    />
                  </div>
                  <span
                    style={{
                      fontSize: 10,
                      fontWeight: 600,
                      color: scoreColor(h.score),
                      minWidth: 32,
                      textAlign: "right",
                    }}
                  >
                    {Math.round(h.score * 100)}%
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

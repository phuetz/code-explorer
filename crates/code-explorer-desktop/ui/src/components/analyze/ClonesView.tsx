import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { Copy, AlertCircle, CheckCircle } from "lucide-react";
import { commands } from "../../lib/tauri-commands";
import { useI18n } from "../../hooks/use-i18n";
import { useAppStore } from "../../stores/app-store";
import { LoadingOrbs } from "../shared/LoadingOrbs";

export function ClonesView() {
  const { t } = useI18n();
  const activeRepo = useAppStore((s) => s.activeRepo);
  const setSelectedNodeId = useAppStore((s) => s.setSelectedNodeId);
  const setMode = useAppStore((s) => s.setMode);
  const [minTokens, setMinTokens] = useState(30);
  const [threshold, setThreshold] = useState(0.9);

  const { data, isLoading, error, refetch, isFetching } = useQuery({
    queryKey: ["clones", activeRepo, minTokens, threshold],
    queryFn: () => commands.findClones(minTokens, threshold, 50),
    staleTime: 60_000,
  });

  const clusters = data ?? [];

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
            <Copy size={18} style={{ display: "inline", marginRight: 8, verticalAlign: "text-bottom" }} />
            {t("clones.title")}
          </h2>
          <p style={{ fontSize: 12, color: "var(--text-3)", marginTop: 4 }}>{t("clones.subtitle")}</p>
        </div>
        <div className="flex gap-3" style={{ alignItems: "center" }}>
          <label style={{ fontSize: 12, color: "var(--text-2)", display: "flex", gap: 6, alignItems: "center" }}>
            {t("clones.minTokens")}
            <input
              type="number"
              value={minTokens}
              min={5}
              max={500}
              onChange={(e) => setMinTokens(Number(e.target.value))}
              style={{
                width: 70,
                padding: "4px 8px",
                fontSize: 12,
                border: "1px solid var(--surface-border)",
                borderRadius: "var(--radius-md)",
                background: "var(--bg-1)",
                color: "var(--text-0)",
              }}
            />
          </label>
          <label style={{ fontSize: 12, color: "var(--text-2)", display: "flex", gap: 6, alignItems: "center" }}>
            {t("clones.threshold")}
            <input
              type="number"
              step={0.05}
              value={threshold}
              min={0}
              max={1}
              onChange={(e) => setThreshold(Number(e.target.value))}
              style={{
                width: 70,
                padding: "4px 8px",
                fontSize: 12,
                border: "1px solid var(--surface-border)",
                borderRadius: "var(--radius-md)",
                background: "var(--bg-1)",
                color: "var(--text-0)",
              }}
            />
          </label>
          <button
            onClick={() => refetch()}
            disabled={isFetching}
            style={{
              padding: "6px 12px",
              fontSize: 12,
              background: "var(--accent-subtle)",
              color: "var(--accent)",
              border: "none",
              borderRadius: "var(--radius-md)",
              cursor: isFetching ? "not-allowed" : "pointer",
            }}
          >
            {isFetching ? "…" : "Refresh"}
          </button>
        </div>
      </div>

      {isLoading && (
        <div className="flex items-center justify-center" style={{ padding: 48 }}>
          <LoadingOrbs />
        </div>
      )}

      {!isLoading && clusters.length === 0 && (
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
          {t("clones.none")}
        </div>
      )}

      {!isLoading && clusters.length > 0 && (
        <>
          <div style={{ fontSize: 13, color: "var(--text-2)", marginBottom: 12 }}>
            {t("clones.clusters").replace("{count}", String(clusters.length))}
          </div>
          <div style={{ display: "grid", gap: 16 }}>
            {clusters.map((cluster) => (
              <div
                key={cluster.clusterId}
                style={{
                  border: "1px solid var(--surface-border)",
                  borderRadius: "var(--radius-lg)",
                  padding: 14,
                  background: "var(--bg-1)",
                }}
              >
                <div className="flex items-center gap-3" style={{ marginBottom: 12 }}>
                  <span
                    style={{
                      fontSize: 11,
                      color: "var(--accent)",
                      background: "var(--accent-subtle)",
                      padding: "2px 8px",
                      borderRadius: "var(--radius-sm)",
                      fontFamily: "var(--font-mono)",
                    }}
                  >
                    {cluster.members.length}× clones
                  </span>
                  <span style={{ fontSize: 12, color: "var(--text-3)" }}>
                    {t("clones.similarity")}: <b style={{ color: "var(--text-0)" }}>{(cluster.similarity * 100).toFixed(1)}%</b>
                  </span>
                </div>
                <div style={{ display: "grid", gridTemplateColumns: "repeat(auto-fit, minmax(280px, 1fr))", gap: 10 }}>
                  {cluster.members.map((m) => (
                    <div
                      key={m.nodeId}
                      style={{
                        border: "1px solid var(--surface-border)",
                        borderRadius: "var(--radius-md)",
                        padding: 10,
                        background: "var(--bg-0)",
                      }}
                    >
                      <button
                        onClick={() => {
                          setSelectedNodeId(m.nodeId, m.name);
                          setMode("explorer");
                        }}
                        style={{
                          fontSize: 13,
                          fontWeight: 600,
                          color: "var(--accent)",
                          fontFamily: "var(--font-mono)",
                          background: "transparent",
                          border: "none",
                          cursor: "pointer",
                          padding: 0,
                          textDecoration: "underline",
                          marginBottom: 4,
                        }}
                      >
                        {m.name}
                      </button>
                      <div style={{ fontSize: 10, color: "var(--text-3)", fontFamily: "var(--font-mono)" }}>
                        {m.filePath}:{m.startLine ?? "?"}–{m.endLine ?? "?"}
                      </div>
                      <div style={{ fontSize: 10, color: "var(--text-3)", marginTop: 2 }}>
                        {m.tokenCount} {t("clones.tokens")}
                      </div>
                      <pre
                        style={{
                          fontSize: 10,
                          color: "var(--text-2)",
                          background: "var(--bg-2)",
                          padding: "6px 8px",
                          borderRadius: "var(--radius-sm)",
                          marginTop: 6,
                          overflow: "auto",
                          maxHeight: 120,
                          fontFamily: "var(--font-mono)",
                        }}
                      >
                        {m.snippet}
                      </pre>
                    </div>
                  ))}
                </div>
              </div>
            ))}
          </div>
        </>
      )}
    </div>
  );
}

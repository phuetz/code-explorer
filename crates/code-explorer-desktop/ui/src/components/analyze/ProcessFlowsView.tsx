import { useState, useRef, useEffect, memo } from "react";
import { useQuery } from "@tanstack/react-query";
import { Workflow, ChevronRight, Activity, FileText } from "lucide-react";
import { commands } from "../../lib/tauri-commands";
import { useI18n } from "../../hooks/use-i18n";
import { useAppStore } from "../../stores/app-store";
import { LoadingOrbs } from "../shared/LoadingOrbs";

const MermaidDiagram = memo(function MermaidDiagram({ definition, id }: { definition: string; id: string }) {
  const containerRef = useRef<HTMLDivElement>(null);
  const initialized = useRef(false);

  useEffect(() => {
    if (!definition || !containerRef.current) return;
    let cancelled = false;

    (async () => {
      try {
        const mermaid = await import("mermaid");
        if (!initialized.current) {
          mermaid.default.initialize({
            startOnLoad: false,
            theme: "dark",
            securityLevel: "loose",
            themeVariables: {
              primaryColor: "var(--accent)",
              primaryTextColor: "#c0caf5",
              lineColor: "#565f89",
              secondaryColor: "#bb9af7",
            },
          });
          initialized.current = true;
        }

        const renderId = `mermaid-flow-${id}-${Date.now()}`;
        const { svg } = await mermaid.default.render(renderId, definition);
        if (!cancelled && containerRef.current) {
          containerRef.current.innerHTML = svg;
        }
      } catch (err) {
        console.error("Mermaid render error:", err, "\nDefinition:", definition);
        if (!cancelled && containerRef.current) {
          const errMsg = err instanceof Error ? err.message : String(err);
          // Show error details + raw mermaid source for debugging
          const container = containerRef.current;
          container.textContent = "";
          const details = document.createElement("details");
          const summary = document.createElement("summary");
          summary.textContent = "Error rendering diagram — click to see details";
          summary.style.cssText = "cursor:pointer; color:var(--rose); font-size:11px;";
          details.appendChild(summary);
          const pre = document.createElement("pre");
          pre.textContent = errMsg + "\n\n--- Mermaid source ---\n" + definition;
          pre.style.cssText = "margin-top:8px; padding:8px; background:var(--bg-2); border-radius:4px; overflow:auto; max-height:200px; white-space:pre-wrap; font-size:10px; color:var(--text-2);";
          details.appendChild(pre);
          container.appendChild(details);
        }
      }
    })();

    return () => { cancelled = true; };
  }, [definition, id]);

  return (
    <div
      ref={containerRef}
      className="mermaid"
      style={{
        width: "100%",
        display: "flex",
        justifyContent: "center",
        padding: "20px 0",
        background: "var(--bg-1)",
        borderRadius: "var(--radius-md)",
        overflow: "auto",
        minHeight: 100,
      }}
    />
  );
});

export function ProcessFlowsView() {
  const { t } = useI18n();
  const activeRepo = useAppStore((s) => s.activeRepo);
  const setSelectedNodeId = useAppStore((s) => s.setSelectedNodeId);
  const setMode = useAppStore((s) => s.setMode);

  const { data: flows, isLoading } = useQuery({
    queryKey: ["process-flows", activeRepo],
    queryFn: () => commands.getProcessFlows(),
    staleTime: 300_000,
  });

  const [expandedId, setFlowExpanded] = useState<string | null>(null);

  if (isLoading) {
    return (
      <div className="flex items-center justify-center h-full">
        <LoadingOrbs />
      </div>
    );
  }

  if (!flows || flows.length === 0) {
    return (
      <div className="flex flex-col items-center justify-center h-full p-8 text-center">
        <Activity size={48} style={{ color: "var(--text-2)", marginBottom: 16, opacity: 0.6 }} />
        <h3 style={{ fontSize: 18, fontWeight: 600, color: "var(--text-1)", marginBottom: 8 }}>
          {t("analyze.noFlowsTitle")}
        </h3>
        <p style={{ fontSize: 14, color: "var(--text-3)", maxWidth: 450 }}>
          {t("analyze.noFlowsDesc")}
        </p>
      </div>
    );
  }

  return (
    <div className="p-6" style={{ maxWidth: 1000, margin: "0 auto" }}>
      <div className="flex items-center gap-3 mb-6">
        <div style={{ padding: 8, background: "var(--accent-subtle)", color: "var(--accent)", borderRadius: 8 }}>
          <Workflow size={24} />
        </div>
        <div>
          <h2 style={{ fontFamily: "var(--font-display)", fontSize: 22, fontWeight: 600, color: "var(--text-0)" }}>
            {t("analyze.processFlows")}
          </h2>
          <p style={{ fontSize: 13, color: "var(--text-3)" }}>
            {t("analyze.flowsDesc").replace("{count}", (flows?.length || 0).toString())}
          </p>
        </div>
      </div>

      <div className="grid gap-4">
        {flows.map((flow) => (
          <div
            key={flow.id}
            style={{
              background: "var(--surface)",
              border: "1px solid var(--surface-border)",
              borderRadius: "var(--radius-lg)",
              overflow: "hidden",
              transition: "box-shadow 0.2s ease",
            }}
            className="hover:shadow-lg"
          >
            {/* Summary Row */}
            <button
              onClick={() => setFlowExpanded(expandedId === flow.id ? null : flow.id)}
              style={{
                width: "100%",
                display: "flex",
                alignItems: "center",
                gap: 12,
                padding: "16px 20px",
                textAlign: "left",
                border: "none",
                background: "none",
                cursor: "pointer",
              }}
            >
              <ChevronRight
                size={18}
                style={{
                  color: "var(--text-3)",
                  transform: expandedId === flow.id ? "rotate(90deg)" : "none",
                  transition: "transform 0.2s ease",
                }}
              />
              <div className="flex-1 min-w-0">
                <div className="flex items-center gap-2">
                  <h4 style={{ fontSize: 15, fontWeight: 600, color: "var(--text-1)" }}>{flow.name}</h4>
                  <span
                    style={{
                      fontSize: 10,
                      fontWeight: 700,
                      textTransform: "uppercase",
                      padding: "2px 6px",
                      borderRadius: 4,
                      background: "var(--bg-2)",
                      color: "var(--text-3)",
                    }}
                  >
                    {flow.processType}
                  </span>
                </div>
                <div style={{ fontSize: 12, color: "var(--text-3)", marginTop: 2 }}>
                  {t("analyze.stepCount").replace("{count}", (flow.stepCount || 0).toString())}
                </div>
              </div>
            </button>

            {/* Expanded Content */}
            {expandedId === flow.id && (
              <div style={{ padding: "0 20px 20px 20px", borderTop: "1px solid var(--glass-border)" }}>
                <div style={{ marginTop: 20 }}>
                  <h5 style={{ fontSize: 12, fontWeight: 600, color: "var(--text-3)", marginBottom: 12, textTransform: "uppercase", letterSpacing: "0.05em" }}>
                    {t("analyze.flowDiagram")}
                  </h5>
                  <MermaidDiagram id={flow.id} definition={flow.mermaid} />
                </div>

                <div style={{ marginTop: 24 }}>
                  <h5 style={{ fontSize: 12, fontWeight: 600, color: "var(--text-3)", marginBottom: 12, textTransform: "uppercase", letterSpacing: "0.05em" }}>
                    {t("analyze.flowSteps")}
                  </h5>
                  <div style={{ display: "flex", flexDirection: "column", gap: 8 }}>
                    {flow.steps.length === 0 && (
                      <div style={{ padding: "12px", fontSize: 13, color: "var(--text-3)", fontStyle: "italic" }}>
                        {t("analyze.noStepsMessage")}
                      </div>
                    )}
                    {flow.steps.map((step, idx) => (
                      <div
                        key={`${flow.id}-step-${idx}`}
                        style={{
                          display: "flex",
                          alignItems: "center",
                          gap: 12,
                          padding: "10px 12px",
                          background: "var(--bg-2)",
                          borderRadius: "var(--radius-md)",
                          border: "1px solid var(--surface-border)",
                        }}
                      >
                        <div style={{ width: 20, height: 20, borderRadius: 10, background: "var(--bg-3)", display: "flex", alignItems: "center", justifyContent: "center", fontSize: 10, fontWeight: 700, color: "var(--text-2)" }}>
                          {idx + 1}
                        </div>
                        <div className="flex-1 min-w-0">
                          <div style={{ fontSize: 13, fontWeight: 500, color: "var(--text-1)" }}>{step.name}</div>
                          <div style={{ fontSize: 11, color: "var(--text-3)", display: "flex", alignItems: "center", gap: 4 }}>
                            <FileText size={10} />
                            {step.filePath}
                          </div>
                        </div>
                        <button
                          onClick={(e) => {
                            e.stopPropagation();
                            setSelectedNodeId(step.nodeId, step.name);
                            setMode("explorer");
                          }}
                          style={{
                            fontSize: 11,
                            padding: "4px 8px",
                            borderRadius: 4,
                            background: "var(--accent-subtle)",
                            color: "var(--accent)",
                            border: "none",
                            cursor: "pointer",
                          }}
                        >
                          {t("analyze.viewCode")}
                        </button>
                      </div>
                    ))}
                  </div>
                </div>
              </div>
            )}
          </div>
        ))}
      </div>
    </div>
  );
}

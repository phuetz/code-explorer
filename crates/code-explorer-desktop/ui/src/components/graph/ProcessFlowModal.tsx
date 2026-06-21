/**
 * Modal displaying execution flows as Mermaid flowcharts.
 */

import { useState, useEffect, useRef } from "react";
import { motion, AnimatePresence } from "framer-motion";
import { X, GitBranch, ChevronRight } from "lucide-react";
import { useQuery } from "@tanstack/react-query";
import { commands, type ProcessFlow } from "../../lib/tauri-commands";
import { useAppStore } from "../../stores/app-store";

interface Props {
  open: boolean;
  onClose: () => void;
}

export function ProcessFlowModal({ open, onClose }: Props) {
  const activeRepo = useAppStore((s) => s.activeRepo);
  const [selectedFlow, setSelectedFlow] = useState<ProcessFlow | null>(null);
  const mermaidRef = useRef<HTMLDivElement>(null);
  const mermaidInitialized = useRef(false);

  // Close on Escape
  useEffect(() => {
    if (!open) return;
    const handler = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    document.addEventListener("keydown", handler);
    return () => document.removeEventListener("keydown", handler);
  }, [open, onClose]);

  const { data: flows } = useQuery({
    queryKey: ["process-flows", activeRepo],
    queryFn: () => commands.getProcessFlows(),
    enabled: open,
    staleTime: Infinity,
  });

  // Render Mermaid when selection changes
  useEffect(() => {
    if (!selectedFlow || !mermaidRef.current) return;

    let cancelled = false;
    (async () => {
      try {
        const mermaid = await import("mermaid");
        if (!mermaidInitialized.current) {
          mermaid.default.initialize({
            startOnLoad: false,
            theme: "dark",
            themeVariables: {
              primaryColor: "var(--accent)",
              primaryTextColor: "#c0caf5",
              lineColor: "#565f89",
              secondaryColor: "#bb9af7",
            },
          });
          mermaidInitialized.current = true;
        }

        const id = `mermaid-${Date.now()}`;
        const { svg } = await mermaid.default.render(id, selectedFlow.mermaid);
        if (!cancelled && mermaidRef.current) {
          mermaidRef.current.innerHTML = svg;
        }
      } catch {
        if (!cancelled && mermaidRef.current) {
          // Use textContent on a freshly-created <pre> so the raw mermaid
          // source can never execute as HTML/JS — node `name` strings can
          // contain markup-like characters in some languages.
          mermaidRef.current.replaceChildren();
          const pre = document.createElement("pre");
          pre.style.color = "var(--text-2)";
          pre.style.fontSize = "12px";
          pre.textContent = selectedFlow.mermaid;
          mermaidRef.current.appendChild(pre);
        }
      }
    })();

    return () => {
      cancelled = true;
    };
  }, [selectedFlow]);

  return (
    <AnimatePresence>
      {open && (
        <motion.div
          initial={{ opacity: 0 }}
          animate={{ opacity: 1 }}
          exit={{ opacity: 0 }}
          style={{
            position: "fixed",
            inset: 0,
            zIndex: 100,
            background: "rgba(0,0,0,0.6)",
            backdropFilter: "blur(4px)",
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
          }}
          onClick={onClose}
        >
          <motion.div
            role="dialog"
            aria-modal="true"
            aria-label="Process Flows"
            initial={{ scale: 0.95, opacity: 0 }}
            animate={{ scale: 1, opacity: 1 }}
            exit={{ scale: 0.95, opacity: 0 }}
            onClick={(e) => e.stopPropagation()}
            style={{
              width: "85vw",
              maxWidth: 900,
              height: "75vh",
              borderRadius: 16,
              border: "1px solid var(--border)",
              background: "var(--bg-0)",
              boxShadow: "0 16px 64px rgba(0,0,0,0.5)",
              display: "flex",
              overflow: "hidden",
            }}
          >
            {/* Sidebar: flow list */}
            <div
              style={{
                width: 260,
                borderRight: "1px solid var(--border)",
                overflow: "auto",
                flexShrink: 0,
              }}
            >
              <div
                style={{
                  padding: "14px 16px",
                  borderBottom: "1px solid var(--border)",
                  display: "flex",
                  alignItems: "center",
                  gap: 8,
                }}
              >
                <GitBranch size={16} style={{ color: "var(--accent)" }} />
                <span
                  style={{
                    fontSize: 13,
                    fontWeight: 600,
                    color: "var(--text-0)",
                  }}
                >
                  Process Flows
                </span>
                <span
                  style={{
                    fontSize: 11,
                    color: "var(--text-3)",
                    marginLeft: "auto",
                  }}
                >
                  {flows?.length || 0}
                </span>
              </div>

              {flows?.map((flow) => (
                <button
                  key={flow.id}
                  onClick={() => setSelectedFlow(flow)}
                  style={{
                    width: "100%",
                    padding: "10px 16px",
                    background:
                      selectedFlow?.id === flow.id
                        ? "var(--bg-2)"
                        : "transparent",
                    border: "none",
                    borderBottom: "1px solid var(--border)",
                    color: "var(--text-1)",
                    fontSize: 12,
                    cursor: "pointer",
                    textAlign: "left",
                    display: "flex",
                    alignItems: "center",
                    gap: 8,
                  }}
                >
                  <ChevronRight size={12} style={{ flexShrink: 0 }} />
                  <div style={{ flex: 1, overflow: "hidden" }}>
                    <div
                      style={{
                        fontWeight: 500,
                        overflow: "hidden",
                        textOverflow: "ellipsis",
                        whiteSpace: "nowrap",
                      }}
                    >
                      {flow.name}
                    </div>
                    <div
                      style={{
                        fontSize: 10,
                        color: "var(--text-3)",
                        marginTop: 2,
                      }}
                    >
                      {flow.stepCount} steps · {flow.processType}
                    </div>
                  </div>
                </button>
              ))}

              {(!flows || flows.length === 0) && (
                <div
                  style={{
                    padding: 20,
                    textAlign: "center",
                    color: "var(--text-3)",
                    fontSize: 12,
                  }}
                >
                  No process flows detected.
                </div>
              )}
            </div>

            {/* Main: Mermaid diagram */}
            <div style={{ flex: 1, display: "flex", flexDirection: "column" }}>
              <div
                style={{
                  padding: "10px 16px",
                  borderBottom: "1px solid var(--border)",
                  display: "flex",
                  alignItems: "center",
                }}
              >
                <span
                  style={{
                    fontSize: 13,
                    fontWeight: 500,
                    color: "var(--text-0)",
                    flex: 1,
                  }}
                >
                  {selectedFlow?.name || "Select a flow"}
                </span>
                <button
                  onClick={onClose}
                  aria-label="Close process flows"
                  style={{
                    background: "none",
                    border: "none",
                    color: "var(--text-3)",
                    cursor: "pointer",
                    padding: 4,
                    display: "flex",
                  }}
                >
                  <X size={18} />
                </button>
              </div>

              <div
                ref={mermaidRef}
                style={{
                  flex: 1,
                  overflow: "auto",
                  padding: 20,
                  display: "flex",
                  alignItems: "center",
                  justifyContent: "center",
                }}
              >
                {!selectedFlow && (
                  <div
                    style={{
                      color: "var(--text-3)",
                      fontSize: 13,
                    }}
                  >
                    Select a process flow to view its execution diagram
                  </div>
                )}
              </div>
            </div>
          </motion.div>
        </motion.div>
      )}
    </AnimatePresence>
  );
}

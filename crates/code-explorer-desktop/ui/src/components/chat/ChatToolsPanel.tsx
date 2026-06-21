/**
 * ChatToolsPanel — Inventory of the agent tools the chat LLM can invoke.
 *
 * Theme B: renders a lightweight right-rail panel grouped by category. Data
 * comes from the backend via `list_chat_tools` (static descriptor list) so
 * it stays in sync with `execute_mcp_tool` without duplicating logic in TS.
 */
import { useQuery } from "@tanstack/react-query";
import { useMemo, useState } from "react";
import { ChevronDown, ChevronRight, Wrench, Loader2, AlertCircle } from "lucide-react";
import { commands, type ChatToolDescriptor } from "../../lib/tauri-commands";
import { useI18n } from "../../hooks/use-i18n";

export interface ChatToolsPanelProps {
  /** Optional — embedded panels close when the user hits Esc or the header button. */
  onClose?: () => void;
}

function prettyArgs(params: unknown): string {
  try {
    return JSON.stringify(params, null, 2);
  } catch {
    return String(params ?? "");
  }
}

function groupByCategory(tools: ChatToolDescriptor[]): Record<string, ChatToolDescriptor[]> {
  const grouped: Record<string, ChatToolDescriptor[]> = {};
  for (const tool of tools) {
    const key = tool.category || "other";
    if (!grouped[key]) grouped[key] = [];
    grouped[key].push(tool);
  }
  return grouped;
}

export function ChatToolsPanel({ onClose }: ChatToolsPanelProps) {
  const { t } = useI18n();
  const [expanded, setExpanded] = useState<Record<string, boolean>>({});

  const { data: tools, isLoading, error } = useQuery({
    queryKey: ["chat-tools-list"],
    queryFn: () => commands.listChatTools(),
    staleTime: 5 * 60_000, // tools are static; cache aggressively
  });

  const grouped = useMemo(() => groupByCategory(tools ?? []), [tools]);

  return (
    <div
      className="h-full flex flex-col w-full"
      style={{
        background: "var(--bg-0)",
        borderLeft: "1px solid var(--surface-border)",
      }}
    >
      {/* Header */}
      <div
        className="shrink-0 flex items-center justify-between px-4 py-3"
        style={{ borderBottom: "1px solid var(--surface-border)" }}
      >
        <div className="flex items-center gap-2">
          <Wrench size={14} style={{ color: "var(--accent)" }} />
          <h3
            style={{
              fontFamily: "var(--font-display)",
              fontSize: 13,
              fontWeight: 600,
              color: "var(--text-1)",
            }}
          >
            {t("chat.tools.title") || "Agent tools"}
          </h3>
          <span
            style={{
              fontSize: 11,
              color: "var(--text-3)",
            }}
          >
            {tools ? tools.length : ""}
          </span>
        </div>
        {onClose && (
          <button
            onClick={onClose}
            style={{
              fontSize: 11,
              color: "var(--text-3)",
              background: "transparent",
              border: "none",
              cursor: "pointer",
            }}
            title={t("common.close") || "Close"}
          >
            ✕
          </button>
        )}
      </div>

      {/* Body */}
      <div className="flex-1 overflow-y-auto px-3 py-3" style={{ gap: 10, display: "flex", flexDirection: "column" }}>
        {isLoading && (
          <div className="flex items-center gap-2" style={{ color: "var(--text-3)", fontSize: 12 }}>
            <Loader2 size={12} className="animate-spin" />
            {t("chat.tools.loading") || "Loading tools…"}
          </div>
        )}
        {!!error && (
          <div style={{ color: "var(--rose, #f7768e)", fontSize: 12, display: "flex", gap: 6, alignItems: "center" }}>
            <AlertCircle size={12} />
            {(error as Error).message || "Failed to load tools"}
          </div>
        )}

        {tools && tools.length === 0 && (
          <div style={{ color: "var(--text-3)", fontSize: 12 }}>
            {t("chat.tools.empty") || "No tools registered."}
          </div>
        )}

        {Object.entries(grouped).map(([category, list]) => {
          const isOpen = expanded[category] ?? true;
          return (
            <div
              key={category}
              style={{
                background: "var(--bg-1)",
                border: "1px solid var(--surface-border)",
                borderRadius: 8,
              }}
            >
              <button
                onClick={() => setExpanded((s) => ({ ...s, [category]: !isOpen }))}
                style={{
                  width: "100%",
                  padding: "8px 10px",
                  background: "transparent",
                  border: "none",
                  display: "flex",
                  alignItems: "center",
                  gap: 6,
                  cursor: "pointer",
                  color: "var(--text-1)",
                  fontSize: 12,
                  fontWeight: 600,
                }}
              >
                {isOpen ? <ChevronDown size={12} /> : <ChevronRight size={12} />}
                <span style={{ textTransform: "capitalize" }}>{category}</span>
                <span style={{ color: "var(--text-3)", fontWeight: 400 }}>· {list.length}</span>
              </button>
              {isOpen && (
                <ul style={{ listStyle: "none", padding: "0 10px 10px 10px", margin: 0 }}>
                  {list.map((tool) => (
                    <li key={tool.name} style={{ padding: "6px 0", borderTop: "1px solid var(--surface-border)" }}>
                      <ToolRow tool={tool} />
                    </li>
                  ))}
                </ul>
              )}
            </div>
          );
        })}
      </div>
    </div>
  );
}

function ToolRow({ tool }: { tool: ChatToolDescriptor }) {
  const [showSchema, setShowSchema] = useState(false);
  return (
    <div style={{ display: "flex", flexDirection: "column", gap: 3 }}>
      <div style={{ display: "flex", alignItems: "center", gap: 6 }}>
        <code
          style={{
            fontFamily: "var(--font-mono)",
            fontSize: 11,
            color: "var(--accent)",
            background: "var(--bg-3)",
            padding: "1px 6px",
            borderRadius: 4,
          }}
        >
          {tool.name}
        </code>
        <button
          onClick={() => setShowSchema((s) => !s)}
          style={{
            fontSize: 10,
            color: "var(--text-3)",
            background: "transparent",
            border: "none",
            cursor: "pointer",
            marginLeft: "auto",
          }}
        >
          {showSchema ? "hide schema" : "schema"}
        </button>
      </div>
      <p style={{ fontSize: 11, color: "var(--text-2)", margin: 0, lineHeight: 1.4 }}>
        {tool.description}
      </p>
      {showSchema && (
        <pre
          style={{
            fontSize: 10,
            margin: 0,
            padding: 6,
            background: "var(--bg-0)",
            border: "1px solid var(--surface-border)",
            borderRadius: 4,
            color: "var(--text-2)",
            overflow: "auto",
            maxHeight: 140,
          }}
        >
          {prettyArgs(tool.parameters)}
        </pre>
      )}
    </div>
  );
}

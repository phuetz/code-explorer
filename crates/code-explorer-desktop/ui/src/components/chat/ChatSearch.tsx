/**
 * ChatSearch — Cross-session substring search modal.
 *
 * Theme B: lets users find any message across every chat session (across
 * repos), with role + timestamp filters. Uses the persisted
 * `chat-session-store` — no backend call required.
 *
 * Opened via Ctrl+Shift+F (registered in `CommandPalette.tsx`).
 */
import { useMemo, useState, useEffect, useRef } from "react";
import { Search, MessageSquare, User, Bot, X } from "lucide-react";
import { AnimatedModal } from "../shared/motion";
import { useI18n } from "../../hooks/use-i18n";
import { useChatSessionStore, type Message } from "../../stores/chat-session-store";
import { useAppStore } from "../../stores/app-store";

type RoleFilter = "all" | "user" | "assistant";
type WhenFilter = "all" | "24h" | "7d" | "30d";

interface MatchRow {
  sessionId: string;
  sessionTitle: string;
  repo: string;
  message: Message;
  /** Index of the first match within `message.content`, used for preview. */
  matchStart: number;
}

const WHEN_TO_MS: Record<WhenFilter, number> = {
  all: 0,
  "24h": 24 * 60 * 60 * 1000,
  "7d": 7 * 24 * 60 * 60 * 1000,
  "30d": 30 * 24 * 60 * 60 * 1000,
};

function formatTimestamp(ts: number): string {
  const d = new Date(ts);
  return d.toLocaleString();
}

function previewSnippet(content: string, matchStart: number, needleLen: number): {
  before: string;
  match: string;
  after: string;
} {
  const radius = 60;
  const start = Math.max(0, matchStart - radius);
  const end = Math.min(content.length, matchStart + needleLen + radius);
  return {
    before: (start > 0 ? "… " : "") + content.slice(start, matchStart),
    match: content.slice(matchStart, matchStart + needleLen),
    after: content.slice(matchStart + needleLen, end) + (end < content.length ? " …" : ""),
  };
}

export interface ChatSearchProps {
  open: boolean;
  onClose: () => void;
  /** Invoked when a result is activated — parent can switch sessions. */
  onSelect?: (sessionId: string, messageId: string) => void;
}

export function ChatSearch({ open, onClose, onSelect }: ChatSearchProps) {
  const { t } = useI18n();
  const sessions = useChatSessionStore((s) => s.sessions);
  const setActiveSession = useChatSessionStore((s) => s.setActiveSession);
  const setActiveRepo = useAppStore((s) => s.setActiveRepo);
  const setMode = useAppStore((s) => s.setMode);

  const [query, setQuery] = useState("");
  const [role, setRole] = useState<RoleFilter>("all");
  const [when, setWhen] = useState<WhenFilter>("all");
  // Re-anchor "now" each time the modal opens. State (not ref) so render
  // sees the current value without tripping the react-hooks/refs and
  // react-hooks/purity lints.
  const [nowAnchor, setNowAnchor] = useState(0);
  const inputRef = useRef<HTMLInputElement>(null);

  // Focus the input each time the modal opens and reset the query so the
  // UX mirrors VS Code's Ctrl+Shift+F (clean slate on every open). The
  // external trigger (modal open/close) drives local state reset — this
  // is one of the legitimate setState-in-effect patterns.
  useEffect(() => {
    if (open) {
      // eslint-disable-next-line react-hooks/set-state-in-effect -- resetting local state when the modal visibility flips is the core purpose of this effect
      setQuery("");
      setNowAnchor(Date.now());
      setTimeout(() => inputRef.current?.focus(), 10);
    }
  }, [open]);

  // Global Escape-to-close is already handled by AnimatedModal.

  const results = useMemo<MatchRow[]>(() => {
    const needle = query.trim().toLowerCase();
    if (!needle) return [];
    const cutoff = WHEN_TO_MS[when];
    const hasTimeWindow = cutoff > 0 && nowAnchor > 0;
    const out: MatchRow[] = [];
    for (const session of sessions) {
      for (const msg of session.messages) {
        if (role !== "all" && msg.role !== role) continue;
        if (hasTimeWindow && nowAnchor - msg.timestamp > cutoff) continue;
        const content = msg.content || "";
        const idx = content.toLowerCase().indexOf(needle);
        if (idx === -1) continue;
        out.push({
          sessionId: session.id,
          sessionTitle: session.title,
          repo: session.repo,
          message: msg,
          matchStart: idx,
        });
        if (out.length >= 200) break; // hard cap — keep the list responsive
      }
      if (out.length >= 200) break;
    }
    // Most recent first.
    out.sort((a, b) => b.message.timestamp - a.message.timestamp);
    return out;
  }, [query, role, when, sessions, nowAnchor]);

  const handleActivate = (row: MatchRow) => {
    // Switch the app to chat mode + repo + session before closing so the
    // jump feels instantaneous; ChatPanel picks the session up through
    // `activeSessionId` + `activeRepo`.
    setActiveRepo(row.repo);
    setActiveSession(row.sessionId);
    setMode("chat");
    onSelect?.(row.sessionId, row.message.id);
    onClose();
  };

  const roleLabel = (r: RoleFilter): string => {
    if (r === "user") return t("chat.search.role.user") || "User";
    if (r === "assistant") return t("chat.search.role.assistant") || "Assistant";
    return t("chat.search.role.all") || "All";
  };

  const whenLabel = (w: WhenFilter): string => {
    if (w === "24h") return t("chat.search.when.24h") || "Last 24h";
    if (w === "7d") return t("chat.search.when.7d") || "Last 7 days";
    if (w === "30d") return t("chat.search.when.30d") || "Last 30 days";
    return t("chat.search.when.all") || "Any time";
  };

  return (
    <AnimatedModal isOpen={open} onClose={onClose}>
      <div
        role="dialog"
        aria-modal="true"
        aria-label="Search chat history"
        style={{
          width: "min(680px, calc(100vw - 32px))",
          background: "var(--bg-1)",
          border: "1px solid var(--surface-border)",
          borderRadius: 12,
          overflow: "hidden",
          boxShadow: "0 25px 50px -12px rgba(0, 0, 0, 0.5)",
        }}
      >
        {/* Header / input row */}
        <div
          style={{
            padding: "12px 16px",
            borderBottom: "1px solid var(--surface-border)",
            display: "flex",
            alignItems: "center",
            gap: 10,
          }}
        >
          <Search size={16} style={{ color: "var(--text-3)" }} />
          <input
            ref={inputRef}
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            placeholder={t("chat.search.placeholder") || "Search across every chat session…"}
            style={{
              flex: 1,
              background: "transparent",
              border: "none",
              outline: "none",
              color: "var(--text-0)",
              fontSize: 14,
              fontFamily: "var(--font-body)",
            }}
          />
          <button
            onClick={onClose}
            className="p-1 rounded"
            style={{ color: "var(--text-3)" }}
            aria-label="Close"
            title="Esc"
          >
            <X size={14} />
          </button>
        </div>

        {/* Filter chips */}
        <div
          style={{
            padding: "8px 16px",
            display: "flex",
            flexWrap: "wrap",
            gap: 8,
            borderBottom: "1px solid var(--surface-border)",
            alignItems: "center",
          }}
        >
          <span style={{ fontSize: 11, color: "var(--text-3)" }}>Role:</span>
          {(["all", "user", "assistant"] as RoleFilter[]).map((r) => (
            <button
              key={r}
              onClick={() => setRole(r)}
              style={{
                fontSize: 11,
                padding: "2px 8px",
                borderRadius: 999,
                border: "1px solid var(--surface-border)",
                background: role === r ? "var(--accent)" : "transparent",
                color: role === r ? "#fff" : "var(--text-2)",
                cursor: "pointer",
              }}
            >
              {roleLabel(r)}
            </button>
          ))}
          <span style={{ fontSize: 11, color: "var(--text-3)", marginLeft: 8 }}>When:</span>
          {(["all", "24h", "7d", "30d"] as WhenFilter[]).map((w) => (
            <button
              key={w}
              onClick={() => setWhen(w)}
              style={{
                fontSize: 11,
                padding: "2px 8px",
                borderRadius: 999,
                border: "1px solid var(--surface-border)",
                background: when === w ? "var(--accent)" : "transparent",
                color: when === w ? "#fff" : "var(--text-2)",
                cursor: "pointer",
              }}
            >
              {whenLabel(w)}
            </button>
          ))}
        </div>

        {/* Results list */}
        <div
          style={{
            maxHeight: 440,
            overflowY: "auto",
            padding: "8px",
          }}
        >
          {query.trim() === "" ? (
            <div style={{ padding: "40px 16px", textAlign: "center", color: "var(--text-3)", fontSize: 13 }}>
              {t("chat.search.hint") || "Type to search messages, tool calls, and citations across every session."}
            </div>
          ) : results.length === 0 ? (
            <div style={{ padding: "40px 16px", textAlign: "center", color: "var(--text-3)", fontSize: 13 }}>
              {t("search.noResults") || "No matches."}
            </div>
          ) : (
            <ul style={{ listStyle: "none", margin: 0, padding: 0 }}>
              {results.map((row) => {
                const snippet = previewSnippet(row.message.content, row.matchStart, query.trim().length);
                const Icon = row.message.role === "user" ? User : Bot;
                return (
                  <li key={`${row.sessionId}-${row.message.id}`}>
                    <button
                      onClick={() => handleActivate(row)}
                      style={{
                        width: "100%",
                        textAlign: "left",
                        padding: "10px 12px",
                        borderRadius: 8,
                        background: "transparent",
                        border: "1px solid transparent",
                        cursor: "pointer",
                        color: "var(--text-1)",
                        display: "flex",
                        flexDirection: "column",
                        gap: 4,
                      }}
                      onMouseEnter={(e) => {
                        e.currentTarget.style.background = "var(--bg-2)";
                        e.currentTarget.style.borderColor = "var(--surface-border)";
                      }}
                      onMouseLeave={(e) => {
                        e.currentTarget.style.background = "transparent";
                        e.currentTarget.style.borderColor = "transparent";
                      }}
                    >
                      <div style={{ display: "flex", alignItems: "center", gap: 8, fontSize: 12 }}>
                        <Icon size={12} style={{ color: "var(--text-3)" }} />
                        <span style={{ color: "var(--text-3)" }}>{row.message.role === "user" ? "You" : "Code Explorer"}</span>
                        <span style={{ color: "var(--text-3)" }}>·</span>
                        <MessageSquare size={11} style={{ color: "var(--text-3)" }} />
                        <span style={{ color: "var(--text-2)", fontWeight: 500 }}>{row.sessionTitle}</span>
                        <span style={{ color: "var(--text-3)" }}>·</span>
                        <span style={{ color: "var(--text-3)" }}>{row.repo}</span>
                        <span style={{ marginLeft: "auto", color: "var(--text-3)", fontSize: 11 }}>
                          {formatTimestamp(row.message.timestamp)}
                        </span>
                      </div>
                      <div
                        style={{
                          fontSize: 12,
                          lineHeight: 1.5,
                          color: "var(--text-2)",
                          whiteSpace: "pre-wrap",
                          overflow: "hidden",
                          textOverflow: "ellipsis",
                        }}
                      >
                        {snippet.before}
                        <mark
                          style={{
                            background: "color-mix(in srgb, var(--accent) 40%, transparent)",
                            color: "var(--text-0)",
                            padding: "0 2px",
                            borderRadius: 2,
                          }}
                        >
                          {snippet.match}
                        </mark>
                        {snippet.after}
                      </div>
                    </button>
                  </li>
                );
              })}
            </ul>
          )}
        </div>

        {/* Footer — count + shortcut hint */}
        <div
          style={{
            padding: "8px 16px",
            borderTop: "1px solid var(--surface-border)",
            display: "flex",
            alignItems: "center",
            justifyContent: "space-between",
            fontSize: 11,
            color: "var(--text-3)",
            fontFamily: "var(--font-mono)",
          }}
        >
          <span>{results.length > 0 ? `${results.length} match${results.length === 1 ? "" : "es"}` : ""}</span>
          <span>
            <kbd style={{ padding: "1px 4px", borderRadius: 3, background: "var(--bg-3)", border: "1px solid var(--surface-border)" }}>Esc</kbd>
            {" "}close
          </span>
        </div>
      </div>
    </AnimatedModal>
  );
}

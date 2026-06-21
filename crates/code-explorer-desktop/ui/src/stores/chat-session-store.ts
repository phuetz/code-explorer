import { create } from "zustand";
import { persist } from "zustand/middleware";
import type {
  ChatSource as ChatSourceType,
  ResearchPlan,
  QueryComplexity,
  FeatureDevArtifact,
  CodeReviewArtifact,
  SimplifyArtifact,
} from "../lib/tauri-commands";

// ─── Tool-call support (Theme B) ─────────────────────────────────────

/** Status of a tool-call inside a chat message. */
export type ToolCallStatus = "pending" | "running" | "success" | "error";

/**
 * A single tool invocation attached to an assistant message.
 *
 * The chat executor emits these as tools fire; the UI renders each as a
 * collapsible block with a "Retry" button and inline JSON args editor.
 * Fields are serializable (`persist` middleware stores them alongside
 * the message).
 */
export interface ToolCall {
  id: string;
  name: string;
  /** Arguments serialized as JSON string (what the LLM actually sent). */
  args: string;
  /** Stringified tool output (markdown allowed). */
  result?: string;
  /** Execution time in milliseconds. */
  durationMs?: number;
  status: ToolCallStatus;
  /** Unix timestamp (ms) of the most recent invocation. */
  invokedAt?: number;
  /** Error message when status === "error". */
  error?: string;
}

export interface Message {
  id: string;
  role: "user" | "assistant";
  content: string;
  sources?: ChatSourceType[];
  model?: string | null;
  plan?: ResearchPlan;
  /** When present, the message was produced by a feature-dev run. */
  artifact?: FeatureDevArtifact;
  /** When present, the message was produced by a code_review run. */
  reviewArtifact?: CodeReviewArtifact;
  /** When present, the message was produced by a simplify run. */
  simplifyArtifact?: SimplifyArtifact;
  complexity?: QueryComplexity;
  timestamp: number;
  /** Tool invocations attached to this message (Theme B — retry + visibility). */
  toolCalls?: ToolCall[];
  /** User-pinned messages surface in the sidebar's "Pinned" filter. */
  pinned?: boolean;
}

export interface ChatSession {
  id: string;
  repo: string;
  title: string;
  updatedAt: number;
  messages: Message[];
  /** Parent session id when this session was forked from another. */
  parentId?: string;
  /** Anchor message id in the parent session where the fork began. */
  branchFromMessageId?: string;
}

interface ChatSessionState {
  sessions: ChatSession[];
  activeSessionId: string | null;

  createSession: (repo: string, initialTitle?: string) => string;
  deleteSession: (id: string) => void;
  setActiveSession: (id: string | null) => void;

  // Gets the active session for the current repo
  getActiveSession: (repo: string) => ChatSession | null;

  // Updates messages in the active session
  addMessage: (repo: string, message: Message) => void;
  updateMessage: (repo: string, messageId: string, update: Partial<Message>) => void;
  setMessages: (repo: string, messages: Message[]) => void;
  clearSessionMessages: (repo: string) => void;

  renameSession: (id: string, title: string) => void;
  getSessionsForRepo: (repo: string) => ChatSession[];

  // ── Theme B additions ─────────────────────────────────────────
  /**
   * Fork a session at a given message. Clones every message up to
   * and including `fromMessageId`, creates a child session with
   * `parentId` + `branchFromMessageId` set, renames default
   * `"{originalTitle} (fork)"` and selects it. Returns the new id,
   * or null if the source session/message couldn't be resolved.
   */
  forkSession: (sessionId: string, fromMessageId: string, overrideTitle?: string) => string | null;
  /** Toggle the `pinned` flag on a message (any session). */
  pinMessage: (sessionId: string, messageId: string, pinned?: boolean) => void;
  /** Append a tool-call (or replace an existing one by id). */
  addToolCall: (sessionId: string, messageId: string, toolCall: ToolCall) => void;
  /** Replace a tool-call by id — used when a retry produces new args/result. */
  updateToolCall: (
    sessionId: string,
    messageId: string,
    toolCallId: string,
    update: Partial<ToolCall>,
  ) => void;
}

function makeId(): string {
  return `chat-${Date.now()}-${Math.random().toString(36).slice(2, 11)}`;
}

export const useChatSessionStore = create<ChatSessionState>()(
  persist(
    (set, get) => ({
      sessions: [],
      activeSessionId: null,

      createSession: (repo, initialTitle = "New Chat") => {
        const newSession: ChatSession = {
          id: makeId(),
          repo,
          title: initialTitle,
          updatedAt: Date.now(),
          messages: [],
        };
        set((state) => ({
          sessions: [newSession, ...state.sessions],
          activeSessionId: newSession.id,
        }));
        return newSession.id;
      },

      deleteSession: (id) =>
        set((state) => ({
          sessions: state.sessions.filter((s) => s.id !== id),
          activeSessionId: state.activeSessionId === id ? null : state.activeSessionId,
        })),

      setActiveSession: (id) => set({ activeSessionId: id }),

      renameSession: (id, title) =>
        set((state) => ({
          sessions: state.sessions.map((s) =>
            s.id === id ? { ...s, title, updatedAt: Date.now() } : s
          ),
        })),

      getActiveSession: (repo) => {
        const { sessions, activeSessionId } = get();
        if (activeSessionId) {
          const session = sessions.find((s) => s.id === activeSessionId && s.repo === repo);
          if (session) return session;
        }
        // Fallback: find the most recent session for this repo (pure getter, no side-effect)
        const repoSessions = sessions.filter((s) => s.repo === repo).sort((a, b) => b.updatedAt - a.updatedAt);
        return repoSessions[0] ?? null;
      },

      getSessionsForRepo: (repo) => {
        return get().sessions.filter((s) => s.repo === repo).sort((a, b) => b.updatedAt - a.updatedAt);
      },

      addMessage: (repo, message) => {
        set((state) => {
          let sessionId = state.activeSessionId;
          let sessions = [...state.sessions];

          // Auto-create session if none active
          if (!sessionId || !sessions.find(s => s.id === sessionId && s.repo === repo)) {
             const newSession: ChatSession = {
               id: makeId(),
               repo,
               title: message.role === 'user' ? message.content.slice(0, 30) + "..." : "New Chat",
               updatedAt: Date.now(),
               messages: [],
             };
             sessions = [newSession, ...sessions];
             sessionId = newSession.id;
          }

          return {
            activeSessionId: sessionId,
            sessions: sessions.map(s => {
              if (s.id === sessionId) {
                const newTitle = s.messages.length === 0 && message.role === 'user'
                   ? message.content.slice(0, 30) + "..."
                   : s.title;
                return {
                  ...s,
                  title: newTitle,
                  updatedAt: Date.now(),
                  messages: [...s.messages, message]
                };
              }
              return s;
            })
          };
        });
      },

      updateMessage: (repo, messageId, update) => {
        set((state) => {
          const sessionId = state.activeSessionId;
          if (!sessionId) return state;

          return {
            sessions: state.sessions.map(s => {
              if (s.id === sessionId && s.repo === repo) {
                return {
                  ...s,
                  updatedAt: Date.now(),
                  messages: s.messages.map(m => m.id === messageId ? { ...m, ...update } : m)
                };
              }
              return s;
            })
          };
        });
      },

      setMessages: (repo, messages) => {
        set((state) => {
          const sessionId = state.activeSessionId;
          if (!sessionId) return state;

          return {
            sessions: state.sessions.map(s => {
              if (s.id === sessionId && s.repo === repo) {
                return {
                  ...s,
                  updatedAt: Date.now(),
                  messages
                };
              }
              return s;
            })
          };
        });
      },

      clearSessionMessages: (repo) => {
        set((state) => {
          const sessionId = state.activeSessionId;
          if (!sessionId) return state;

          return {
            sessions: state.sessions.map(s => {
              if (s.id === sessionId && s.repo === repo) {
                return {
                  ...s,
                  updatedAt: Date.now(),
                  messages: []
                };
              }
              return s;
            })
          };
        });
      },

      // ── Theme B: fork / pin / tool-call actions ───────────────
      forkSession: (sessionId, fromMessageId, overrideTitle) => {
        const state = get();
        const source = state.sessions.find((s) => s.id === sessionId);
        if (!source) return null;
        const cutoff = source.messages.findIndex((m) => m.id === fromMessageId);
        if (cutoff === -1) return null;

        // Clone messages up to and including the anchor; give each a fresh
        // id to keep the child's timeline independent (reactions, pinning).
        const clonedMessages: Message[] = source.messages
          .slice(0, cutoff + 1)
          .map((m, idx) => ({
            ...m,
            id: `msg-${Date.now()}-${idx}-${Math.random().toString(36).slice(2, 8)}`,
            // Strip runtime-only nested artifacts that shouldn't re-fire.
            // Keep the markdown content + sources so the fork reads fluently.
          }));

        const forkTitle = overrideTitle ?? `${source.title} (fork)`;
        const child: ChatSession = {
          id: makeId(),
          repo: source.repo,
          title: forkTitle,
          updatedAt: Date.now(),
          messages: clonedMessages,
          parentId: source.id,
          branchFromMessageId: fromMessageId,
        };

        set((s) => ({
          sessions: [child, ...s.sessions],
          activeSessionId: child.id,
        }));
        return child.id;
      },

      pinMessage: (sessionId, messageId, pinned) => {
        set((state) => ({
          sessions: state.sessions.map((s) => {
            if (s.id !== sessionId) return s;
            return {
              ...s,
              updatedAt: Date.now(),
              messages: s.messages.map((m) =>
                m.id === messageId
                  ? { ...m, pinned: pinned ?? !m.pinned }
                  : m,
              ),
            };
          }),
        }));
      },

      addToolCall: (sessionId, messageId, toolCall) => {
        set((state) => ({
          sessions: state.sessions.map((s) => {
            if (s.id !== sessionId) return s;
            return {
              ...s,
              updatedAt: Date.now(),
              messages: s.messages.map((m) => {
                if (m.id !== messageId) return m;
                const existing = m.toolCalls ?? [];
                const idx = existing.findIndex((tc) => tc.id === toolCall.id);
                const next = idx >= 0
                  ? existing.map((tc, i) => (i === idx ? toolCall : tc))
                  : [...existing, toolCall];
                return { ...m, toolCalls: next };
              }),
            };
          }),
        }));
      },

      updateToolCall: (sessionId, messageId, toolCallId, update) => {
        set((state) => ({
          sessions: state.sessions.map((s) => {
            if (s.id !== sessionId) return s;
            return {
              ...s,
              updatedAt: Date.now(),
              messages: s.messages.map((m) => {
                if (m.id !== messageId) return m;
                const existing = m.toolCalls ?? [];
                return {
                  ...m,
                  toolCalls: existing.map((tc) =>
                    tc.id === toolCallId ? { ...tc, ...update } : tc,
                  ),
                };
              }),
            };
          }),
        }));
      },
    }),
    {
      name: "code-explorer-chat-sessions",
      // Bump on breaking changes to the persisted shape. Current additions
      // (parentId/branchFromMessageId/toolCalls/pinned) are all optional,
      // so legacy sessions deserialize cleanly — no migration needed.
      version: 2,
    }
  )
);

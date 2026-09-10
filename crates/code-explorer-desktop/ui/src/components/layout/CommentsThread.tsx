/**
 * CommentsThread — annotation thread attached to the currently-selected node.
 *
 * Slots into the DetailPanel as a small section. Persistence in
 * <.codeexplorer>/comments.json so notes follow the repo.
 */

import { useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { MessageSquare, Send, Trash2 } from "lucide-react";
import { useI18n } from "../../hooks/use-i18n";
import { toast } from "sonner";
import { commands } from "../../lib/tauri-commands";
import { useAppStore } from "../../stores/app-store";

export function CommentsThread() {
  const { t } = useI18n();
  const queryClient = useQueryClient();
  const activeRepo = useAppStore((s) => s.activeRepo);
  const selectedNodeId = useAppStore((s) => s.selectedNodeId);
  const [draft, setDraft] = useState("");

  const { data: comments = [] } = useQuery({
    queryKey: ["comments", activeRepo, selectedNodeId],
    queryFn: () => commands.commentsForNode(selectedNodeId!),
    enabled: !!selectedNodeId && !!activeRepo,
    staleTime: 30_000,
  });

  const addMut = useMutation({
    mutationFn: (body: string) =>
      commands.commentsAdd(selectedNodeId!, deriveAuthor(), body),
    onSuccess: (next) => {
      queryClient.setQueryData(["comments", activeRepo, selectedNodeId], next);
      setDraft("");
    },
    onError: (e) => toast.error(`Add failed: ${(e as Error).message}`),
  });

  const removeMut = useMutation({
    mutationFn: (commentId: string) =>
      commands.commentsRemove(selectedNodeId!, commentId),
    onSuccess: (next) =>
      queryClient.setQueryData(["comments", activeRepo, selectedNodeId], next),
  });

  if (!selectedNodeId) return null;

  const submit = () => {
    if (!draft.trim() || addMut.isPending) return;
    addMut.mutate(draft.trim());
  };

  return (
    <div style={{ padding: "12px 16px", borderTop: "1px solid var(--surface-border)" }}>
      <div
        style={{
          display: "flex",
          alignItems: "center",
          gap: 6,
          fontSize: 11,
          fontWeight: 600,
          textTransform: "uppercase",
          color: "var(--text-3)",
          letterSpacing: 0.5,
          marginBottom: 8,
        }}
      >
        <MessageSquare size={11} />
        Notes ({comments.length})
      </div>

      {comments.length === 0 && (
        <div style={{ fontSize: 11, color: "var(--text-3)", marginBottom: 8 }}>
          {t("comments.emptyHint")}
        </div>
      )}

      <div style={{ display: "flex", flexDirection: "column", gap: 4, marginBottom: 8 }}>
        {comments.map((c) => (
          <div
            key={c.id}
            style={{
              padding: "6px 8px",
              background: "var(--bg-2)",
              border: "1px solid var(--surface-border)",
              borderRadius: 6,
              fontSize: 11,
            }}
          >
            <div
              style={{
                display: "flex",
                alignItems: "center",
                gap: 6,
                color: "var(--text-3)",
                fontSize: 9,
                marginBottom: 2,
              }}
            >
              <span style={{ fontWeight: 600 }}>{c.author}</span>
              <span>·</span>
              <span>{formatTime(c.createdAt)}</span>
              <button
                onClick={() => removeMut.mutate(c.id)}
                title="Delete note"
                aria-label="Delete note"
                style={{
                  marginLeft: "auto",
                  padding: 2,
                  background: "transparent",
                  border: "none",
                  color: "var(--text-3)",
                  cursor: "pointer",
                }}
              >
                <Trash2 size={9} />
              </button>
            </div>
            <div style={{ whiteSpace: "pre-wrap", color: "var(--text-1)" }}>{c.body}</div>
          </div>
        ))}
      </div>

      <div style={{ display: "flex", gap: 4 }}>
        <input
          value={draft}
          onChange={(e) => setDraft(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter" && !e.shiftKey) {
              e.preventDefault();
              submit();
            }
          }}
          placeholder="Add a note for the team…"
          style={{
            flex: 1,
            padding: "4px 8px",
            background: "var(--bg-2)",
            border: "1px solid var(--surface-border)",
            borderRadius: 6,
            color: "var(--text-0)",
            fontSize: 11,
            outline: "none",
          }}
        />
        <button
          onClick={submit}
          disabled={!draft.trim() || addMut.isPending}
          aria-label="Send note"
          title="Send note (Enter)"
          style={{
            padding: "4px 10px",
            background: draft.trim() ? "var(--accent)" : "var(--bg-3)",
            color: "#fff",
            border: "none",
            borderRadius: 6,
            cursor: draft.trim() ? "pointer" : "not-allowed",
            display: "inline-flex",
            alignItems: "center",
          }}
        >
          <Send size={11} />
        </button>
      </div>
    </div>
  );
}

function deriveAuthor(): string {
  // Tauri exposes neither USER nor a profile API in a stable way at this
  // point — fall back to a session-local nickname stored in localStorage.
  const stored = localStorage.getItem("code-explorer.commentAuthor");
  if (stored) return stored;
  const nickname = "you";
  localStorage.setItem("code-explorer.commentAuthor", nickname);
  return nickname;
}

function formatTime(ms: number): string {
  const d = new Date(ms);
  const today = new Date();
  if (d.toDateString() === today.toDateString()) {
    return d.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
  }
  return d.toLocaleDateString([], { month: "short", day: "numeric" });
}

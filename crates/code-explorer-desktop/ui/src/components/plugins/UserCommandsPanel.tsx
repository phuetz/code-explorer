/**
 * UserCommandsPanel — manage user-defined slash commands.
 *
 * Each command pairs a name (typed as `/<name>` in the chat input) with a
 * template that gets sent to the chat. Templates use `{{args}}` for the
 * tokens after the command name. Optional mode picks the chat mode the
 * command should activate (qa / deep_research / feature_dev / code_review /
 * simplify).
 */

import { useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Plus, Trash2, RefreshCw, X, Slash, Save } from "lucide-react";
import { toast } from "sonner";
import { commands } from "../../lib/tauri-commands";
import { useI18n } from "../../hooks/use-i18n";
import { confirm } from "../../lib/confirm";
import { useAppStore } from "../../stores/app-store";
import type { UserCommand } from "../../lib/tauri-commands";

interface Props {
  open: boolean;
  onClose: () => void;
}

const MODES = ["qa", "deep_research", "feature_dev", "code_review", "simplify"] as const;

function blank(): UserCommand {
  return {
    id: "",
    name: "explain",
    template: "Explain `{{args}}` step by step.",
    mode: "qa",
    description: undefined,
    updatedAt: Date.now(),
  };
}

export function UserCommandsPanel({ open, onClose }: Props) {
  const { t } = useI18n();
  const queryClient = useQueryClient();
  const activeRepo = useAppStore((s) => s.activeRepo);
  const [editing, setEditing] = useState<UserCommand | null>(null);

  const { data: list = [], refetch } = useQuery({
    queryKey: ["user-commands", activeRepo],
    queryFn: () => commands.userCommandsList(),
    enabled: !!activeRepo && open,
    staleTime: 30_000,
  });

  const saveMut = useMutation({
    mutationFn: (cmd: UserCommand) => commands.userCommandsSave(cmd),
    onSuccess: (next) => {
      queryClient.setQueryData(["user-commands", activeRepo], next);
      setEditing(null);
      toast.success("Command saved");
    },
    onError: (e) => toast.error(`Save failed: ${(e as Error).message}`),
  });

  const deleteMut = useMutation({
    mutationFn: (id: string) => commands.userCommandsDelete(id),
    onSuccess: (next) =>
      queryClient.setQueryData(["user-commands", activeRepo], next),
  });

  if (!open) return null;

  return (
    <div
      onClick={onClose}
      style={{
        position: "fixed",
        inset: 0,
        background: "rgba(0,0,0,0.5)",
        zIndex: 100,
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
      }}
    >
      <div
        onClick={(e) => e.stopPropagation()}
        style={{
          width: 720,
          maxWidth: "92vw",
          maxHeight: "85vh",
          background: "var(--bg-1)",
          border: "1px solid var(--surface-border)",
          borderRadius: 12,
          boxShadow: "0 12px 48px rgba(0,0,0,0.6)",
          overflow: "hidden",
          display: "flex",
          flexDirection: "column",
        }}
      >
        {/* Header */}
        <div
          style={{
            padding: "10px 14px",
            borderBottom: "1px solid var(--surface-border)",
            background: "var(--bg-2)",
            display: "flex",
            alignItems: "center",
            gap: 8,
          }}
        >
          <Slash size={14} style={{ color: "var(--accent)" }} />
          <span style={{ fontSize: 13, fontWeight: 600 }}>User slash commands</span>
          <span style={{ fontSize: 10, color: "var(--text-3)" }}>
            {list.length} defined
          </span>
          <div style={{ marginLeft: "auto", display: "flex", gap: 6 }}>
            <button
              onClick={() => setEditing(blank())}
              title="New command"
              aria-label="New command"
              style={pillBtn("var(--accent)")}
            >
              <Plus size={11} /> new
            </button>
            <button
              onClick={() => refetch()}
              title="Refresh"
              aria-label="Refresh"
              style={iconBtn()}
            >
              <RefreshCw size={11} />
            </button>
            <button
              onClick={onClose}
              aria-label="Close"
              style={iconBtn()}
            >
              <X size={11} />
            </button>
          </div>
        </div>

        {/* Body */}
        <div style={{ flex: 1, overflow: "auto", padding: 14 }}>
          {editing ? (
            <CommandEditor
              command={editing}
              onCancel={() => setEditing(null)}
              onSave={(c) => saveMut.mutate(c)}
              isSaving={saveMut.isPending}
            />
          ) : list.length === 0 ? (
            <div style={{ textAlign: "center", padding: 32, fontSize: 12, color: "var(--text-3)" }}>
              No slash commands yet. Click "+ new" to create your first one.
              <br />
              <span style={{ fontSize: 11 }}>
                Then type <code style={{ color: "var(--accent)" }}>/&lt;name&gt; args</code> in the chat input.
              </span>
            </div>
          ) : (
            <ul style={{ listStyle: "none", margin: 0, padding: 0 }}>
              {list.map((cmd) => (
                <li
                  key={cmd.id}
                  style={{
                    padding: "10px 12px",
                    border: "1px solid var(--surface-border)",
                    borderRadius: 8,
                    background: "var(--bg-2)",
                    marginBottom: 6,
                    display: "flex",
                    alignItems: "flex-start",
                    gap: 8,
                  }}
                >
                  <div style={{ flex: 1, minWidth: 0 }}>
                    <div style={{ display: "flex", alignItems: "baseline", gap: 6 }}>
                      <code
                        style={{
                          fontSize: 12,
                          fontWeight: 700,
                          color: "var(--accent)",
                        }}
                      >
                        /{cmd.name}
                      </code>
                      <span
                        style={{
                          fontSize: 9,
                          padding: "1px 5px",
                          borderRadius: 999,
                          background: "var(--bg-3)",
                          color: "var(--text-3)",
                          fontWeight: 600,
                          textTransform: "uppercase",
                        }}
                      >
                        {cmd.mode || "qa"}
                      </span>
                      {cmd.description && (
                        <span style={{ fontSize: 11, color: "var(--text-3)" }}>{cmd.description}</span>
                      )}
                    </div>
                    <pre
                      style={{
                        margin: "4px 0 0 0",
                        fontSize: 11,
                        color: "var(--text-2)",
                        fontFamily: "var(--font-mono)",
                        whiteSpace: "pre-wrap",
                        wordBreak: "break-word",
                      }}
                    >
                      {cmd.template}
                    </pre>
                  </div>
                  <div style={{ display: "flex", flexDirection: "column", gap: 4 }}>
                    <button onClick={() => setEditing(cmd)} style={pillBtn()}>
                      edit
                    </button>
                    <button
                      onClick={async () => {
                        const ok = await confirm({
                          title: t("confirm.deleteTitle"),
                          message: t("userCommand.deleteConfirm").replace("{0}", cmd.name),
                          confirmLabel: t("confirm.delete"),
                          danger: true,
                        });
                        if (ok) deleteMut.mutate(cmd.id);
                      }}
                      style={pillBtn("var(--rose)")}
                    >
                      <Trash2 size={10} />
                    </button>
                  </div>
                </li>
              ))}
            </ul>
          )}
        </div>
      </div>
    </div>
  );
}

function CommandEditor({
  command,
  onCancel,
  onSave,
  isSaving,
}: {
  command: UserCommand;
  onCancel: () => void;
  onSave: (cmd: UserCommand) => void;
  isSaving: boolean;
}) {
  const [draft, setDraft] = useState<UserCommand>(command);

  const update = (patch: Partial<UserCommand>) => setDraft({ ...draft, ...patch });

  return (
    <div style={{ display: "flex", flexDirection: "column", gap: 10 }}>
      <Field label="Name (after the slash)">
        <input
          value={draft.name}
          onChange={(e) => update({ name: e.target.value })}
          placeholder="e.g. explain"
          style={inputStyle()}
        />
      </Field>
      <Field label="Description (optional)">
        <input
          value={draft.description ?? ""}
          onChange={(e) => update({ description: e.target.value || undefined })}
          placeholder="Shown next to the command in the list"
          style={inputStyle()}
        />
      </Field>
      <Field label="Mode">
        <select
          value={draft.mode || "qa"}
          onChange={(e) => update({ mode: e.target.value })}
          style={{
            ...inputStyle(),
            cursor: "pointer",
          }}
        >
          {MODES.map((m) => (
            <option key={m} value={m}>
              {m}
            </option>
          ))}
        </select>
      </Field>
      <Field label="Template">
        <textarea
          value={draft.template}
          onChange={(e) => update({ template: e.target.value })}
          rows={5}
          placeholder="Use {{args}} to receive the user-typed text after the command name."
          style={{ ...inputStyle(), fontFamily: "var(--font-mono)" }}
        />
        <div style={{ fontSize: 10, color: "var(--text-3)", marginTop: 4 }}>
          Tip: <code>/{draft.name || "name"} <em>UserService</em></code> with template{" "}
          <code>"Explain &#123;&#123;args&#125;&#125;"</code> sends "Explain UserService".
        </div>
      </Field>
      <div style={{ display: "flex", gap: 6, justifyContent: "flex-end" }}>
        <button onClick={onCancel} style={pillBtn()}>
          cancel
        </button>
        <button
          onClick={() => onSave(draft)}
          disabled={isSaving || !draft.name.trim() || !draft.template.trim()}
          style={pillBtn("var(--accent)")}
        >
          <Save size={11} />
          {isSaving ? "saving…" : "save"}
        </button>
      </div>
    </div>
  );
}

function Field({
  label,
  children,
}: {
  label: string;
  children: React.ReactNode;
}) {
  return (
    <label style={{ display: "flex", flexDirection: "column", gap: 3 }}>
      <span
        style={{
          fontSize: 9,
          fontWeight: 700,
          textTransform: "uppercase",
          color: "var(--text-3)",
        }}
      >
        {label}
      </span>
      {children}
    </label>
  );
}

function inputStyle(): React.CSSProperties {
  return {
    width: "100%",
    padding: "5px 8px",
    background: "var(--bg-2)",
    border: "1px solid var(--surface-border)",
    borderRadius: 4,
    color: "var(--text-0)",
    fontSize: 12,
    outline: "none",
  };
}
function iconBtn(color?: string): React.CSSProperties {
  return {
    padding: 3,
    background: "transparent",
    border: "1px solid var(--surface-border)",
    borderRadius: 4,
    color: color ?? "var(--text-3)",
    cursor: "pointer",
    display: "inline-flex",
    alignItems: "center",
  };
}
function pillBtn(color?: string): React.CSSProperties {
  return {
    display: "inline-flex",
    alignItems: "center",
    gap: 4,
    padding: "3px 8px",
    background: color ?? "transparent",
    border: color ? "none" : "1px solid var(--surface-border)",
    borderRadius: 4,
    color: color ? "#fff" : "var(--text-2)",
    fontSize: 10,
    fontWeight: 600,
    cursor: "pointer",
    fontFamily: "inherit",
  };
}

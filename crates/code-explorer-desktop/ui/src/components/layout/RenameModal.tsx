/**
 * RenameModal — preview & apply a multi-file rename refactor.
 *
 * Two-step flow:
 *   1. Dry run → user reviews graph_edits (auto-applicable) +
 *      text_search_edits (review-only).
 *   2. Apply → re-runs with dry_run=false; only graph_edits get patched.
 *
 * Triggered from the AI context menu's "Rename" action (TODO wire) or
 * directly via the rename command palette entry.
 */

import { useState, useMemo } from "react";
import { useMutation } from "@tanstack/react-query";
import { motion, AnimatePresence } from "framer-motion";
import { X, Replace, AlertTriangle, Check } from "lucide-react";
import { toast } from "sonner";
import { useI18n } from "../../hooks/use-i18n";
import { commands } from "../../lib/tauri-commands";
import type { RenameEdit, RenameResult } from "../../lib/tauri-commands";

interface Props {
  open: boolean;
  initialTarget?: string;
  onClose: () => void;
}

export function RenameModal({ open, initialTarget, onClose }: Props) {
  const { t } = useI18n();
  const [target, setTarget] = useState(initialTarget ?? "");
  const [newName, setNewName] = useState("");
  const [preview, setPreview] = useState<RenameResult | null>(null);

  // Reset state when the modal opens (or initialTarget changes while open) —
  // derived-from-prop pattern per React 19 docs, avoids setState-in-effect.
  const [prevOpen, setPrevOpen] = useState(open);
  const [prevInitialTarget, setPrevInitialTarget] = useState(initialTarget);
  if (prevOpen !== open || prevInitialTarget !== initialTarget) {
    setPrevOpen(open);
    setPrevInitialTarget(initialTarget);
    if (open) {
      setTarget(initialTarget ?? "");
      setNewName("");
      setPreview(null);
    }
  }

  const previewMut = useMutation({
    mutationFn: () =>
      commands.renameRun({ target: target.trim(), newName: newName.trim(), dryRun: true }),
    onSuccess: (r) => setPreview(r),
    onError: (e) => toast.error(`Preview failed: ${(e as Error).message}`),
  });

  const applyMut = useMutation({
    mutationFn: () =>
      commands.renameRun({ target: target.trim(), newName: newName.trim(), dryRun: false }),
    onSuccess: (r) => {
      setPreview(r);
      const total = r.graphEdits.length;
      toast.success(`Applied ${total} edit${total === 1 ? "" : "s"} across ${r.filesAffected} file(s)`);
    },
    onError: (e) => toast.error(`Apply failed: ${(e as Error).message}`),
  });

  const canPreview = target.trim().length > 0 && newName.trim().length > 0 && target.trim() !== newName.trim();

  if (!open) return null;

  return (
    <AnimatePresence>
      <motion.div
        initial={{ opacity: 0 }}
        animate={{ opacity: 1 }}
        exit={{ opacity: 0 }}
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
        <motion.div
          initial={{ scale: 0.95, y: 10 }}
          animate={{ scale: 1, y: 0 }}
          exit={{ scale: 0.95, y: 10 }}
          transition={{ duration: 0.15 }}
          onClick={(e) => e.stopPropagation()}
          style={{
            width: 720,
            maxWidth: "90vw",
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
              padding: "12px 16px",
              borderBottom: "1px solid var(--surface-border)",
              display: "flex",
              alignItems: "center",
              gap: 10,
              background: "var(--bg-2)",
            }}
          >
            <Replace size={16} style={{ color: "var(--accent)" }} />
            <span style={{ fontSize: 13, fontWeight: 600 }}>Rename refactor</span>
            <button
              onClick={onClose}
              aria-label="Close"
              style={{
                marginLeft: "auto",
                padding: 4,
                background: "transparent",
                border: "none",
                color: "var(--text-3)",
                cursor: "pointer",
              }}
            >
              <X size={14} />
            </button>
          </div>

          {/* Form */}
          <div style={{ padding: "12px 16px", display: "flex", gap: 10 }}>
            <FormField label="Target symbol" value={target} onChange={setTarget} placeholder="e.g. UserService" />
            <FormField label="New name" value={newName} onChange={setNewName} placeholder="e.g. AccountService" />
            <button
              onClick={() => previewMut.mutate()}
              disabled={!canPreview || previewMut.isPending}
              style={{
                alignSelf: "flex-end",
                padding: "6px 14px",
                background: canPreview ? "var(--accent)" : "var(--bg-3)",
                color: "#fff",
                border: "none",
                borderRadius: 6,
                fontSize: 12,
                fontWeight: 600,
                cursor: canPreview ? "pointer" : "not-allowed",
              }}
            >
              {previewMut.isPending ? t("rename.searching") : t("rename.preview")}
            </button>
          </div>

          {/* Body — edits list */}
          <div style={{ flex: 1, overflow: "auto", padding: "0 16px 12px" }}>
            {preview ? (
              <PreviewList result={preview} />
            ) : (
              <div style={{ padding: 32, textAlign: "center", color: "var(--text-3)", fontSize: 12 }}>
                Enter a target symbol and a new name, then click Preview.
              </div>
            )}
          </div>

          {/* Footer */}
          {preview && (
            <div
              style={{
                padding: "10px 16px",
                borderTop: "1px solid var(--surface-border)",
                background: "var(--bg-2)",
                display: "flex",
                alignItems: "center",
                gap: 12,
                fontSize: 11,
              }}
            >
              <span style={{ color: "var(--text-2)" }}>
                {preview.graphEdits.length} graph edit
                {preview.graphEdits.length === 1 ? "" : "s"} ·{" "}
                {preview.textSearchEdits.length} text-search edit
                {preview.textSearchEdits.length === 1 ? "" : "s"} ·{" "}
                {preview.filesAffected} file
                {preview.filesAffected === 1 ? "" : "s"}
              </span>
              {preview.textSearchEdits.length > 0 && (
                <span
                  style={{
                    display: "inline-flex",
                    alignItems: "center",
                    gap: 4,
                    color: "var(--amber)",
                  }}
                >
                  <AlertTriangle size={11} /> Text-search edits are NOT auto-applied — review manually.
                </span>
              )}
              <button
                onClick={() => applyMut.mutate()}
                disabled={preview.graphEdits.length === 0 || applyMut.isPending}
                style={{
                  marginLeft: "auto",
                  padding: "6px 14px",
                  background:
                    preview.graphEdits.length > 0 ? "var(--green)" : "var(--bg-3)",
                  color: "#000",
                  border: "none",
                  borderRadius: 6,
                  fontSize: 12,
                  fontWeight: 700,
                  cursor: preview.graphEdits.length > 0 ? "pointer" : "not-allowed",
                  display: "inline-flex",
                  alignItems: "center",
                  gap: 4,
                }}
              >
                <Check size={11} />
                {applyMut.isPending ? "Applying…" : `Apply ${preview.graphEdits.length}`}
              </button>
            </div>
          )}
        </motion.div>
      </motion.div>
    </AnimatePresence>
  );
}

function FormField({
  label,
  value,
  onChange,
  placeholder,
}: {
  label: string;
  value: string;
  onChange: (v: string) => void;
  placeholder: string;
}) {
  return (
    <div style={{ flex: 1 }}>
      <label
        style={{
          display: "block",
          fontSize: 9,
          fontWeight: 700,
          textTransform: "uppercase",
          color: "var(--text-3)",
          marginBottom: 4,
        }}
      >
        {label}
      </label>
      <input
        value={value}
        onChange={(e) => onChange(e.target.value)}
        placeholder={placeholder}
        style={{
          width: "100%",
          padding: "6px 10px",
          background: "var(--bg-2)",
          border: "1px solid var(--surface-border)",
          borderRadius: 6,
          color: "var(--text-0)",
          fontFamily: "var(--font-mono)",
          fontSize: 12,
          outline: "none",
        }}
      />
    </div>
  );
}

function PreviewList({ result }: { result: RenameResult }) {
  const allEdits = useMemo(() => {
    const grouped: Record<string, RenameEdit[]> = {};
    for (const e of [...result.graphEdits, ...result.textSearchEdits]) {
      (grouped[e.file] ??= []).push(e);
    }
    return Object.entries(grouped).sort(([a], [b]) => a.localeCompare(b));
  }, [result]);

  if (allEdits.length === 0) {
    return (
      <div style={{ padding: 24, textAlign: "center", color: "var(--text-3)", fontSize: 12 }}>
        No occurrences found. Either the target name is unique or the graph has no references.
      </div>
    );
  }

  return (
    <div style={{ marginTop: 8 }}>
      {allEdits.map(([file, edits]) => (
        <div key={file} style={{ marginBottom: 12 }}>
          <div
            style={{
              fontSize: 11,
              fontWeight: 600,
              color: "var(--text-2)",
              fontFamily: "var(--font-mono)",
              marginBottom: 4,
            }}
          >
            {file} <span style={{ color: "var(--text-3)" }}>({edits.length})</span>
          </div>
          {edits.map((e, i) => (
            <div
              key={i}
              style={{
                padding: "4px 10px",
                fontFamily: "var(--font-mono)",
                fontSize: 11,
                borderLeft: `3px solid ${
                  e.confidence >= 0.9 ? "var(--green)" : e.confidence >= 0.7 ? "var(--amber)" : "var(--accent)"
                }`,
                background: "var(--bg-2)",
                marginBottom: 2,
              }}
            >
              <span style={{ color: "var(--text-3)" }}>L{e.line}:{e.col}</span>{" "}
              <span style={{ color: "var(--text-2)" }}>{e.snippet}</span>
              <span style={{ color: "var(--text-3)", marginLeft: 8, fontSize: 9 }}>
                {e.reason} · {e.confidence.toFixed(2)}
              </span>
            </div>
          ))}
        </div>
      ))}
    </div>
  );
}

/**
 * ConfirmDialogProvider — mount once at the root of the app. Renders a
 * styled modal whenever `confirm()` (from lib/confirm.ts) is called.
 *
 * Replaces `window.confirm()` with a proper React modal that:
 *  - matches the app's glass-panel look,
 *  - supports i18n (default confirm/cancel labels pulled from useI18n()),
 *  - styles the confirm button in the danger color when `danger: true`,
 *  - closes on Escape + backdrop click (equivalent to Cancel).
 */

import { useEffect } from "react";
import { motion, AnimatePresence } from "framer-motion";
import { AlertTriangle } from "lucide-react";
import { useConfirmStore } from "../../lib/confirm";
import { useI18n } from "../../hooks/use-i18n";

export function ConfirmDialogProvider() {
  const { t } = useI18n();
  const pending = useConfirmStore((s) => s.pending);
  const resolve = useConfirmStore((s) => s.resolve);

  // Escape closes the dialog as "cancel".
  useEffect(() => {
    if (!pending) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault();
        resolve(false);
      } else if (e.key === "Enter" && !e.shiftKey) {
        e.preventDefault();
        resolve(true);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [pending, resolve]);

  return (
    <AnimatePresence>
      {pending && (
        <motion.div
          key="confirm-backdrop"
          initial={{ opacity: 0 }}
          animate={{ opacity: 1 }}
          exit={{ opacity: 0 }}
          onClick={() => resolve(false)}
          style={{
            position: "fixed",
            inset: 0,
            background: "rgba(0,0,0,0.55)",
            zIndex: 200,
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
          }}
          role="presentation"
        >
          <motion.div
            initial={{ scale: 0.95, y: 10 }}
            animate={{ scale: 1, y: 0 }}
            exit={{ scale: 0.95, y: 10 }}
            transition={{ duration: 0.15 }}
            onClick={(e) => e.stopPropagation()}
            role="alertdialog"
            aria-modal="true"
            aria-labelledby="confirm-title"
            aria-describedby="confirm-message"
            style={{
              width: 440,
              maxWidth: "90vw",
              background: "var(--bg-1)",
              border: "1px solid var(--surface-border)",
              borderRadius: 12,
              boxShadow: "0 16px 48px rgba(0,0,0,0.6)",
              overflow: "hidden",
              display: "flex",
              flexDirection: "column",
            }}
          >
            <div
              style={{
                padding: "14px 16px",
                borderBottom: "1px solid var(--surface-border)",
                display: "flex",
                alignItems: "center",
                gap: 10,
                background: "var(--bg-2)",
              }}
            >
              {pending.danger && (
                <AlertTriangle size={16} style={{ color: "var(--rose)" }} />
              )}
              <span id="confirm-title" style={{ fontSize: 13, fontWeight: 600 }}>
                {pending.title}
              </span>
            </div>

            <div
              id="confirm-message"
              style={{
                padding: "16px",
                fontSize: 13,
                color: "var(--text-2)",
                lineHeight: 1.55,
              }}
            >
              {pending.message}
            </div>

            <div
              style={{
                padding: "12px 16px",
                borderTop: "1px solid var(--surface-border)",
                display: "flex",
                justifyContent: "flex-end",
                gap: 8,
                background: "var(--bg-2)",
              }}
            >
              <button
                onClick={() => resolve(false)}
                autoFocus={!pending.danger}
                style={{
                  padding: "6px 14px",
                  fontSize: 12,
                  fontWeight: 500,
                  background: "transparent",
                  border: "1px solid var(--surface-border)",
                  borderRadius: 6,
                  color: "var(--text-2)",
                  cursor: "pointer",
                }}
              >
                {pending.cancelLabel || t("confirm.cancel")}
              </button>
              <button
                onClick={() => resolve(true)}
                autoFocus={pending.danger}
                style={{
                  padding: "6px 14px",
                  fontSize: 12,
                  fontWeight: 600,
                  background: pending.danger ? "var(--rose)" : "var(--accent)",
                  border: "none",
                  borderRadius: 6,
                  color: "#fff",
                  cursor: "pointer",
                }}
              >
                {pending.confirmLabel || t("confirm.ok")}
              </button>
            </div>
          </motion.div>
        </motion.div>
      )}
    </AnimatePresence>
  );
}

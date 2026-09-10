import { forwardRef, useRef, useEffect, useCallback, useState } from "react";
import { Settings2, Send, Loader2, Microscope, Square, SlidersHorizontal } from "lucide-react";
import { useI18n } from "../../hooks/use-i18n";

const SLASH_COMPLETIONS = [
  { cmd: "/expliquer ",   hint: "Expliquer un module ou symbole" },
  { cmd: "/algorithme ",  hint: "Décrire l'algorithme étape par étape" },
  { cmd: "/impact ",      hint: "Analyser le blast radius" },
  { cmd: "/architecture ", hint: "Vue d'ensemble de l'architecture" },
  { cmd: "/diagramme ",   hint: "Générer un diagramme Mermaid" },
  { cmd: "/explain ",     hint: "Explain a module or symbol" },
];

interface ChatInputProps {
  value: string;
  onChange: (value: string) => void;
  onSend: () => void;
  onKeyDown: (e: React.KeyboardEvent) => void;
  isPending: boolean;
  onOpenSettings?: () => void;
  deepResearch: boolean;
  hasFilters: boolean;
  /** Called when the user clicks the Stop button during streaming */
  onCancel?: () => void;
  /** True while tokens are actively streaming (shows Stop button) */
  isStreaming?: boolean;
}

export const ChatInput = forwardRef<HTMLTextAreaElement, ChatInputProps>(
  ({ value, onChange, onSend, onKeyDown, isPending, onOpenSettings, deepResearch, hasFilters, onCancel, isStreaming }, ref) => {
    const { t } = useI18n();
    const internalRef = useRef<HTMLTextAreaElement | null>(null);
    const [slashMatches, setSlashMatches] = useState<typeof SLASH_COMPLETIONS>([]);
    const [selectedIdx, setSelectedIdx] = useState(0);
    const placeholder = deepResearch
      ? t("chat.placeholder.deepResearch")
      : hasFilters
      ? t("chat.placeholder.filtered")
      : t("chat.placeholder.default");

    // Auto-resize textarea on input change (min 56px to prevent shrink)
    useEffect(() => {
      const el = internalRef.current;
      if (!el) return;
      el.style.height = "56px";
      el.style.height = `${Math.max(56, Math.min(el.scrollHeight, 200))}px`;
    }, [value]);

    // Merge forwarded ref with internal ref
    const setRefs = useCallback(
      (node: HTMLTextAreaElement | null) => {
        internalRef.current = node;
        if (typeof ref === "function") {
          ref(node);
        } else if (ref) {
          (ref as React.MutableRefObject<HTMLTextAreaElement | null>).current = node;
        }
      },
      [ref],
    );

    return (
      <div className="flex-shrink-0 px-4 py-3" style={{ borderTop: "1px solid var(--surface-border)", background: "var(--bg-0)" }}>
        <div
          className="chat-input-container relative flex items-end gap-3 rounded-xl px-3 py-2 transition-all"
          style={{
            background: "var(--bg-2)",
            border: deepResearch
              ? "1px solid var(--purple)"
              : "1px solid var(--surface-border)",
            boxShadow: deepResearch
              ? "0 0 0 3px color-mix(in srgb, var(--purple) 12%, transparent)"
              : "0 1px 0 rgba(255,255,255,0.03)",
          }}
        >
          {/* Deep research indicator */}
          {deepResearch && (
            <Microscope
              size={16}
              className="mb-1 flex-shrink-0"
              style={{ color: "var(--purple)" }}
            />
          )}

          {/* Slash command completion popup */}
          {slashMatches.length > 0 && (
            <div
              className="absolute bottom-full left-0 mb-2 w-[min(420px,calc(100vw-48px))] rounded-lg overflow-hidden shadow-lg z-50"
              style={{ background: "var(--bg-2)", border: "1px solid var(--surface-border)" }}
            >
              {slashMatches.map((m, i) => (
                <button
                  key={m.cmd}
                  className="w-full text-left px-3 py-2 text-[13px] flex justify-between items-center gap-3"
                  style={{
                    background: i === selectedIdx ? "var(--accent)" : "transparent",
                    color: i === selectedIdx ? "#fff" : "var(--text-0)",
                  }}
                  onMouseDown={(e) => { e.preventDefault(); onChange(m.cmd); setSlashMatches([]); internalRef.current?.focus(); }}
                >
                  <span className="font-mono font-semibold whitespace-nowrap">{m.cmd.trim()}</span>
                  <span className="text-[11px] opacity-70 truncate">{m.hint}</span>
                </button>
              ))}
            </div>
          )}
          <textarea
            ref={setRefs}
            value={value}
            onChange={(e) => {
              const v = e.target.value;
              onChange(v);
              // Slash completion: show when value starts with /
              if (v.startsWith("/") && !v.includes(" ")) {
                const q = v.toLowerCase();
                setSlashMatches(SLASH_COMPLETIONS.filter(c => c.cmd.startsWith(q)));
                setSelectedIdx(0);
              } else {
                setSlashMatches([]);
              }
            }}
            onKeyDown={(e) => {
              // Navigate and confirm slash completions
              if (slashMatches.length > 0) {
                if (e.key === "ArrowDown") { e.preventDefault(); setSelectedIdx(i => Math.min(i + 1, slashMatches.length - 1)); return; }
                if (e.key === "ArrowUp")   { e.preventDefault(); setSelectedIdx(i => Math.max(i - 1, 0)); return; }
                if (e.key === "Tab" || e.key === "Enter") {
                  e.preventDefault();
                  onChange(slashMatches[selectedIdx].cmd);
                  setSlashMatches([]);
                  return;
                }
                if (e.key === "Escape") { setSlashMatches([]); return; }
              }
              onKeyDown(e);
            }}
            placeholder={placeholder}
            aria-label={t("chat.inputLabel") || "Ask a question about the code"}
            rows={1}
            className="flex-1 bg-transparent resize-none text-[14px] outline-none min-h-[40px] max-h-[200px] leading-relaxed"
            onInput={(e) => { const el = e.currentTarget; el.style.height = "auto"; el.style.height = `${Math.min(el.scrollHeight, 200)}px`; }}
            style={{
              color: "var(--text-0)",
              fontFamily: "var(--font-body)",
            }}
          />
          <div className="flex items-center gap-1.5">
            {onOpenSettings && (
              <button
                onClick={onOpenSettings}
                className="flex h-9 w-9 items-center justify-center rounded-lg transition-colors"
                style={{
                  color: "var(--text-3)",
                  background: "var(--surface)",
                  border: "1px solid var(--surface-border)",
                }}
                aria-label={t("chat.settings") || "Chat Settings"}
                title={t("chat.settings") || "Chat Settings"}
              >
                <Settings2 size={15} />
              </button>
            )}
            {isStreaming && onCancel ? (
              <button
                onClick={onCancel}
                aria-label="Stop streaming"
                title="Stop"
                className="flex h-9 w-9 items-center justify-center rounded-lg transition-all"
                style={{
                  background: "rgba(239,68,68,0.15)",
                  border: "1px solid rgba(239,68,68,0.4)",
                  color: "rgb(239,68,68)",
                }}
              >
                <Square size={14} />
              </button>
            ) : (
              <button
                onClick={onSend}
                disabled={!value.trim() || isPending}
                aria-label={isPending ? "Sending..." : "Send message"}
                className="flex h-9 w-9 items-center justify-center rounded-lg transition-all"
                style={{
                  background: value.trim() && !isPending
                    ? deepResearch ? "var(--purple)" : "var(--accent)"
                    : "var(--bg-3)",
                  color: value.trim() && !isPending ? "#fff" : "var(--text-3)",
                  border: "1px solid transparent",
                }}
              >
                {isPending ? <Loader2 size={16} className="animate-spin" /> : <Send size={16} />}
              </button>
            )}
          </div>
        </div>
        <div className="mt-2 flex items-center justify-between gap-2 text-[11px]" style={{ color: "var(--text-3)" }}>
          <span className="inline-flex min-w-0 items-center gap-1 truncate">
            <SlidersHorizontal size={11} />
            <span className="truncate">
              {hasFilters ? t("chat.placeholder.filtered") : t("chat.placeholder.default")}
            </span>
          </span>
          <span
            className="shrink-0 rounded px-2 py-0.5"
            style={{
              background: deepResearch
                ? "color-mix(in srgb, var(--purple) 12%, transparent)"
                : "var(--surface)",
              color: deepResearch ? "var(--purple)" : "var(--text-3)",
            }}
          >
            {deepResearch ? t("chat.deepResearch") : t("chat.quickAnswer")}
          </span>
        </div>
      </div>
    );
  }
);

ChatInput.displayName = "ChatInput";

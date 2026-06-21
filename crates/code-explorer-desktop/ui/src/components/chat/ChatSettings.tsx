/**
 * ChatSettings — LLM provider configuration panel.
 */

import { useState } from "react";
import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";
import { X, Check, Loader2, Zap } from "lucide-react";
import { commands, type ChatConfig } from "../../lib/tauri-commands";
import { useI18n } from "../../hooks/use-i18n";

interface ChatSettingsProps {
  onClose: () => void;
}

const PRESETS: { label: string; config: Partial<ChatConfig> }[] = [
  {
    label: "Ollama (Local)",
    config: {
      provider: "ollama",
      baseUrl: "http://localhost:11434/v1",
      model: "llama3.2",
      apiKey: "",
    },
  },
  {
    label: "OpenAI",
    config: {
      provider: "openai",
      baseUrl: "https://api.openai.com/v1",
      model: "gpt-4o-mini",
    },
  },
  {
    label: "ChatGPT (Codex)",
    config: {
      provider: "chatgpt",
      baseUrl: "https://chatgpt.com/backend-api/codex",
      model: "gpt-5.5",
      apiKey: "",
      maxTokens: 8192,
      reasoningEffort: "high",
    },
  },
  {
    label: "Anthropic (via proxy)",
    config: {
      provider: "anthropic",
      baseUrl: "https://api.anthropic.com/v1",
      model: "claude-sonnet-4-20250514",
    },
  },
  {
    label: "OpenRouter",
    config: {
      provider: "openrouter",
      baseUrl: "https://openrouter.ai/api/v1",
      model: "anthropic/claude-sonnet-4",
    },
  },
  {
    label: "Gemini Flash Lite",
    config: {
      provider: "gemini",
      baseUrl: "https://generativelanguage.googleapis.com/v1beta/openai/",
      model: "gemini-2.5-flash-lite",
      reasoningEffort: "high",
    },
  },
];

const REASONING_LEVELS = ["none", "minimal", "low", "medium", "high", "xhigh"] as const;

export function ChatSettings({ onClose }: ChatSettingsProps) {
  const { t } = useI18n();
  const queryClient = useQueryClient();
  const { data: config, isLoading } = useQuery({
    queryKey: ["chat-config"],
    queryFn: () => commands.chatGetConfig(),
  });

  const [form, setForm] = useState<ChatConfig>({
    provider: "ollama",
    apiKey: "",
    baseUrl: "http://localhost:11434/v1",
    model: "llama3.2",
    maxTokens: 4096,
    reasoningEffort: "",
  });

  // Sync form from loaded config (render-time state adjustment)
  const [prevConfig, setPrevConfig] = useState(config);
  if (config !== prevConfig) {
    setPrevConfig(config);
    if (config) setForm(config);
  }

  const saveMutation = useMutation({
    mutationFn: (cfg: ChatConfig) => commands.chatSetConfig(cfg),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["chat-config"] });
      onClose();
    },
    onError: (err) => {
      toast.error(`Failed to save: ${err instanceof Error ? err.message : String(err)}`);
    },
  });

  // Test Connection — pings the LLM endpoint with a trivial prompt so the
  // user can validate their api_key + base_url + model without committing.
  // We feed it the current form state (not the persisted config) so changes
  // can be tested before save.
  const testMutation = useMutation({
    mutationFn: (cfg: ChatConfig) => commands.chatTestConnection(cfg),
    onSuccess: (result) => {
      if (result.ok) {
        toast.success(
          t("chatSettings.testSuccess")
            .replace("{0}", String(result.status))
            .replace("{1}", String(result.latencyMs)),
        );
      } else {
        toast.error(
          t("chatSettings.testFailed")
            .replace("{0}", String(result.status))
            .replace("{1}", result.message || "no body"),
        );
      }
    },
    onError: (err) => {
      toast.error(`${err instanceof Error ? err.message : String(err)}`);
    },
  });

  const applyPreset = (preset: (typeof PRESETS)[number]) => {
    setForm((prev) => ({ ...prev, ...preset.config }));
  };

  const isChatGpt = form.provider.toLowerCase() === "chatgpt";
  const selectedReasoning = form.reasoningEffort || "";

  if (isLoading) {
    return (
      <div
        className="fixed inset-0 z-50 flex items-center justify-center"
        style={{ background: "rgba(0,0,0,0.6)" }}
        onClick={(e) => e.target === e.currentTarget && onClose()}
      >
        <div
          className="rounded-xl p-6 shadow-lg"
          style={{ width: "min(480px, calc(100vw - 32px))", background: "var(--bg-2)", border: "1px solid var(--surface-border)" }}
        >
          <div className="shimmer" style={{ width: 160, height: 20, borderRadius: 8, background: "var(--bg-3)", marginBottom: 20 }} />
          <div className="shimmer" style={{ width: "100%", height: 36, borderRadius: 8, background: "var(--bg-3)", marginBottom: 12 }} />
          <div className="shimmer" style={{ width: "100%", height: 36, borderRadius: 8, background: "var(--bg-3)", marginBottom: 12 }} />
          <div className="shimmer" style={{ width: "100%", height: 36, borderRadius: 8, background: "var(--bg-3)" }} />
        </div>
      </div>
    );
  }

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center"
      style={{ background: "rgba(0,0,0,0.6)" }}
      onClick={(e) => e.target === e.currentTarget && onClose()}
      role="dialog"
      aria-modal="true"
      aria-label="Chat AI Settings"
    >
      <div
        className="rounded-xl p-6 shadow-lg fade-in"
        style={{
          width: "min(480px, calc(100vw - 32px))",
          background: "var(--bg-2)",
          border: "1px solid var(--surface-border)",
        }}
      >
        {/* Header */}
        <div className="flex items-center justify-between mb-5">
          <h2
            className="text-base font-semibold"
            style={{ fontFamily: "var(--font-display)", color: "var(--text-0)" }}
          >
            {t("settings.chatAiTitle")}
          </h2>
          <button onClick={onClose} className="p-1" style={{ color: "var(--text-3)" }} aria-label="Close settings">
            <X size={16} />
          </button>
        </div>

        {/* Presets */}
        <div className="mb-4">
          <label className="text-[12px] font-medium mb-2 block" style={{ color: "var(--text-2)" }}>
            {t("settings.quickSetup")}
          </label>
          <div className="flex flex-wrap gap-2">
            {PRESETS.map((preset) => (
              <button
                key={preset.label}
                onClick={() => applyPreset(preset)}
                className="px-3 py-1.5 rounded-lg text-[12px] transition-all"
                style={{
                  background:
                    form.provider === preset.config.provider
                      ? "var(--accent-subtle)"
                      : "var(--surface)",
                  color:
                    form.provider === preset.config.provider
                      ? "var(--accent)"
                      : "var(--text-2)",
                  border: `1px solid ${
                    form.provider === preset.config.provider
                      ? "var(--accent-border)"
                      : "var(--surface-border)"
                  }`,
                }}
              >
                {preset.label}
              </button>
            ))}
          </div>
        </div>

        {/* Form fields */}
        <div className="space-y-3">
          <Field label={t("settings.baseUrl")} value={form.baseUrl} onChange={(v) => setForm((f) => ({ ...f, baseUrl: v }))} />
          <Field label={t("settings.model")} value={form.model} onChange={(v) => setForm((f) => ({ ...f, model: v }))} />
          <Field
            label={t("settings.apiKey")}
            value={form.apiKey}
            onChange={(v) => setForm((f) => ({ ...f, apiKey: v }))}
            type="password"
            placeholder={isChatGpt ? "code-explorer login" : t("chat.apiKeyPlaceholder")}
            disabled={isChatGpt}
          />
          <p className="text-[10px] -mt-2" style={{ color: "var(--text-4)" }}>
            {isChatGpt
              ? "ChatGPT uses the Codex OAuth login stored by the CLI; no API key is written here."
              : "For security, the API key is kept in memory for this session and is not written to disk. Use an environment variable for persistent secrets."}
          </p>
          <Field
            label={t("settings.maxTokens")}
            value={String(form.maxTokens)}
            onChange={(v) => setForm((f) => ({ ...f, maxTokens: parseInt(v) || 4096 }))}
          />

          {/* Reasoning Effort */}
          <div>
            <label className="text-[12px] font-medium mb-1 block" style={{ color: "var(--text-2)" }}>
              {t("settings.thinking")}
            </label>
            <div className="grid grid-cols-3 gap-1.5">
              {REASONING_LEVELS.map((level) => {
                const value = level === "none" ? "" : level;
                const isActive = selectedReasoning === value;

                return (
                  <button
                    key={level}
                    onClick={() => setForm((f) => ({ ...f, reasoningEffort: value }))}
                    className="px-2 py-1.5 rounded-lg text-[12px] transition-all"
                    style={{
                      background: isActive ? "var(--accent-subtle)" : "var(--surface)",
                      color: isActive ? "var(--accent)" : "var(--text-3)",
                      border: `1px solid ${isActive ? "var(--accent-border)" : "var(--surface-border)"}`,
                    }}
                  >
                    {level}
                  </button>
                );
              })}
            </div>
            <p className="text-[10px] mt-1" style={{ color: "var(--text-4)" }}>
              {t("settings.thinkingHint")}
            </p>
          </div>
        </div>

        {/* Actions — Test Connection on the left, Cancel + Save on the right */}
        <div className="flex justify-between items-center gap-2 mt-6">
          <button
            onClick={() => testMutation.mutate(form)}
            disabled={testMutation.isPending || saveMutation.isPending}
            className="flex items-center gap-2 px-3 py-2 rounded-lg text-[12px] font-medium"
            style={{
              background: "transparent",
              border: "1px solid var(--surface-border)",
              color: "var(--text-2)",
            }}
            title={t("chatSettings.testConnection.tooltip")}
          >
            {testMutation.isPending ? (
              <Loader2 size={12} className="animate-spin" />
            ) : (
              <Zap size={12} />
            )}
            {t("chatSettings.testConnection")}
          </button>

          <div className="flex items-center gap-2">
            <button
              onClick={onClose}
              className="px-4 py-2 rounded-lg text-[13px]"
              style={{ color: "var(--text-2)" }}
            >
              {t("settings.cancel")}
            </button>
            <button
              onClick={() => saveMutation.mutate(form)}
              disabled={saveMutation.isPending}
              className="flex items-center gap-2 px-4 py-2 rounded-lg text-[13px] font-medium"
              style={{ background: "var(--accent)", color: "#fff" }}
            >
              {saveMutation.isPending ? (
                <Loader2 size={14} className="animate-spin" />
              ) : (
                <Check size={14} />
              )}
              {t("settings.save")}
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}

// ─── Field Component ────────────────────────────────────────────────

function Field({
  label,
  value,
  onChange,
  type = "text",
  placeholder,
  disabled = false,
}: {
  label: string;
  value: string;
  onChange: (v: string) => void;
  type?: string;
  placeholder?: string;
  disabled?: boolean;
}) {
  return (
    <div>
      <label className="text-[12px] font-medium mb-1 block" style={{ color: "var(--text-2)" }}>
        {label}
      </label>
      <input
        type={type}
        value={value}
        onChange={(e) => onChange(e.target.value)}
        placeholder={placeholder}
        disabled={disabled}
        className="w-full px-3 py-2 rounded-lg text-[13px] outline-none focus:ring-1 focus:ring-[var(--accent)] transition-all"
        style={{
          background: "var(--surface)",
          border: "1px solid var(--surface-border)",
          color: "var(--text-0)",
          cursor: disabled ? "not-allowed" : "text",
          opacity: disabled ? 0.65 : 1,
          fontFamily: type === "password" ? "var(--font-mono)" : "var(--font-body)",
        }}
      />
    </div>
  );
}

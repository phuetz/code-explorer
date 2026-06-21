import { useCallback, useMemo, useState } from 'react';
import { createPortal } from 'react-dom';
import {
  Check,
  Copy,
  Cpu,
  KeyRound,
  RefreshCw,
  Save,
  ShieldCheck,
  SlidersHorizontal,
} from 'lucide-react';
import clsx from 'clsx';
import type { LlmConfigState } from '../../hooks/use-llm-config';
import { useAnchoredPopover } from '../../hooks/use-anchored-popover';
import { copyTextToClipboard } from '../../utils/clipboard';

interface Props {
  llm: LlmConfigState;
}

interface ProviderPreset {
  id: string;
  label: string;
  badge: string;
  privacy: string;
  provider: string;
  baseUrl: string;
  model: string;
  reasoningEffort: string;
  maxTokens: number;
  requiresKey: boolean;
  probeAvailability?: boolean;
  tailnetHostName?: string;
}

type ProviderAvailability = 'checking' | 'available' | 'unavailable';

interface LlmSettingsForm {
  provider: string;
  baseUrl: string;
  model: string;
  apiKey: string;
  reasoningEffort: string;
  maxTokens: string;
}

const PROVIDER_PRESETS: ProviderPreset[] = [
  {
    id: 'chatgpt',
    label: 'ChatGPT Pro',
    badge: 'Actuel',
    privacy: 'OAuth local Code Explorer, aucun secret affiche dans le navigateur.',
    provider: 'chatgpt',
    baseUrl: 'https://chatgpt.com/backend-api/codex',
    model: 'gpt-5.5',
    reasoningEffort: 'high',
    maxTokens: 8192,
    requiresKey: false,
  },
  {
    id: 'ollama',
    label: 'Ollama local',
    badge: 'Local',
    privacy: 'Inference sur la machine si Ollama tourne localement.',
    provider: 'ollama',
    baseUrl: 'http://localhost:11434/v1',
    model: '',
    reasoningEffort: 'none',
    maxTokens: 4096,
    requiresKey: false,
  },
  {
    id: 'darkstar',
    label: 'DARKSTAR Ollama',
    badge: '2x3090',
    privacy: 'Inference locale sur DARKSTAR via Tailscale, ideale pour les gros modeles.',
    provider: 'darkstar-ollama',
    baseUrl: 'http://100.73.222.64:11434/v1',
    model: '',
    reasoningEffort: 'none',
    maxTokens: 8192,
    requiresKey: false,
    probeAvailability: true,
    tailnetHostName: 'DARKSTAR',
  },
  {
    id: 'ministar-linux',
    label: 'Ministar Linux Ollama',
    badge: 'Hub',
    privacy: 'Provider local reseau pour le serveur always-on, quand Ollama y est expose.',
    provider: 'ministar-linux-ollama',
    baseUrl: 'http://100.98.18.76:11434/v1',
    model: '',
    reasoningEffort: 'none',
    maxTokens: 8192,
    requiresKey: false,
    probeAvailability: true,
    tailnetHostName: 'ministar-linux',
  },
  {
    id: 'lm-studio',
    label: 'LM Studio local',
    badge: 'Local',
    privacy: 'Provider local OpenAI-compatible; les modèles viennent de LM Studio.',
    provider: 'lm-studio',
    baseUrl: 'http://localhost:1234/v1',
    model: '',
    reasoningEffort: 'none',
    maxTokens: 4096,
    requiresKey: false,
  },
  {
    id: 'openai',
    label: 'OpenAI API',
    badge: 'API',
    privacy: 'Endpoint distant compatible OpenAI; verifier la politique du compte.',
    provider: 'openai',
    baseUrl: 'https://api.openai.com/v1',
    model: 'gpt-4o-mini',
    reasoningEffort: 'medium',
    maxTokens: 8192,
    requiresKey: true,
  },
  {
    id: 'openrouter',
    label: 'OpenRouter',
    badge: 'Multi',
    privacy: 'Route vers plusieurs modeles; choisir un modele avec les garanties voulues.',
    provider: 'openrouter',
    baseUrl: 'https://openrouter.ai/api/v1',
    model: 'anthropic/claude-sonnet-4',
    reasoningEffort: 'medium',
    maxTokens: 8192,
    requiresKey: true,
  },
  {
    id: 'gemini',
    label: 'Gemini compatible',
    badge: 'Google',
    privacy: 'API distante via endpoint OpenAI-compatible.',
    provider: 'gemini',
    baseUrl: 'https://generativelanguage.googleapis.com/v1beta/openai',
    model: 'gemini-2.5-flash-lite',
    reasoningEffort: 'medium',
    maxTokens: 8192,
    requiresKey: true,
  },
  {
    id: 'compatible',
    label: 'Compatible OpenAI',
    badge: 'Custom',
    privacy: 'Pour LiteLLM, LM Studio, vLLM ou un fournisseur sans retention contractuelle.',
    provider: 'compatible',
    baseUrl: 'http://localhost:1234/v1',
    model: '',
    reasoningEffort: 'none',
    maxTokens: 4096,
    requiresKey: false,
  },
];

const REASONING_PRESETS = [
  { value: 'none', label: 'None', hint: 'compatibilité' },
  { value: 'low', label: 'Low', hint: 'rapide' },
  { value: 'medium', label: 'Medium', hint: 'équilibré' },
  { value: 'high', label: 'High', hint: 'approfondi' },
  { value: 'xhigh', label: 'XHigh', hint: 'maximum' },
] as const;

const FIELD_CLASS =
  'w-full rounded-md border border-[var(--border-strong)] bg-[var(--panel-bg-muted)] px-2 py-1.5 text-xs text-[var(--text-primary)] outline-none transition focus:border-[var(--border-emphasis)] focus:bg-[var(--panel-bg-strong)]';

export function LlmStatus({ llm }: Props) {
  const { status, config, message, refresh, save, test: testProvider, listModels, tailnetHosts } = llm;
  const [expanded, setExpanded] = useState(false);
  const [copiedPreset, setCopiedPreset] = useState<string | null>(null);
  const [form, setForm] = useState<LlmSettingsForm>(() => formFromConfig(config));
  const [saveError, setSaveError] = useState<string | null>(null);
  const [testMessage, setTestMessage] = useState<string | null>(null);
  const [availableModels, setAvailableModels] = useState<string[]>([]);
  const [modelsLoaded, setModelsLoaded] = useState(false);
  const [modelsLoading, setModelsLoading] = useState(false);
  const [modelsError, setModelsError] = useState<string | null>(null);
  const [providerAvailability, setProviderAvailability] = useState<
    Record<string, ProviderAvailability>
  >({});
  const [saved, setSaved] = useState(false);
  const [saving, setSaving] = useState(false);
  const [testing, setTesting] = useState(false);
  const closePopover = useCallback(() => setExpanded(false), []);
  const { anchorRef, popoverRef, position, updatePosition } = useAnchoredPopover<
    HTMLButtonElement,
    HTMLDivElement
  >({
    maxWidth: 620,
    minWidth: 420,
    onClose: closePopover,
    open: expanded,
  });

  const label =
    status === 'ready'
      ? `${config?.provider ?? 'LLM'} · ${config?.model ?? 'modèle ?'}`
      : status === 'missing'
        ? 'LLM non configuré'
        : status === 'checking'
          ? 'LLM...'
          : 'LLM indisponible';

  const detail = [
    message,
    config?.reasoningEffort ? `Raisonnement: ${config.reasoningEffort}` : null,
    config?.maxTokens ? `Max tokens: ${config.maxTokens}` : null,
  ]
    .filter(Boolean)
    .join('\n');

  const activePreset = useMemo(() => {
    const formProvider = form.provider.toLowerCase();
    const formBaseUrl = normalizeBaseUrl(form.baseUrl);
    return (
      PROVIDER_PRESETS.find(
        (preset) =>
          preset.provider === formProvider && normalizeBaseUrl(preset.baseUrl) === formBaseUrl
      ) ??
      PROVIDER_PRESETS.find((preset) => preset.provider === formProvider) ??
      PROVIDER_PRESETS[PROVIDER_PRESETS.length - 1]
    );
  }, [form.baseUrl, form.provider]);
  const visibleProviderPresets = useMemo(
    () =>
      PROVIDER_PRESETS.filter(
        (preset) => !preset.probeAvailability || providerAvailability[preset.id] === 'available'
      ),
    [providerAvailability]
  );
  const probingTailnetProviders = PROVIDER_PRESETS.some(
    (preset) => preset.probeAvailability && providerAvailability[preset.id] === 'checking'
  );
  const normalizedProvider = form.provider.trim().toLowerCase();
  const isChatGptForm = normalizedProvider === 'chatgpt';
  const activeReasoning = form.reasoningEffort.toLowerCase();
  const hasModel = form.model.trim().length > 0;
  const previewCommand = useMemo(
    () => (isChatGptForm ? buildChatGptCommand(form, 'xhigh') : ''),
    [form, isChatGptForm]
  );
  const providerAdvice = providerCapabilityHint(form, availableModels, modelsLoaded);

  const loadAvailableModels = useCallback(
    async (target: LlmSettingsForm) => {
      if (!canListModelsFor(target)) {
        setAvailableModels([]);
        setModelsLoaded(false);
        setModelsError(null);
        return;
      }
      setModelsLoading(true);
      setModelsLoaded(false);
      setModelsError(null);
      try {
        const models = await listModels(formToUpdate(target));
        setAvailableModels(models);
        setModelsLoaded(true);
        if (models.length > 0) {
          const preferred = preferredModel(models, target.model);
          setForm((current) =>
            current.provider === target.provider && normalizeBaseUrl(current.baseUrl) === normalizeBaseUrl(target.baseUrl)
              ? { ...current, model: models.includes(current.model) ? current.model : preferred }
              : current
          );
        }
      } catch (e) {
        setAvailableModels([]);
        setModelsLoaded(false);
        setModelsError(e instanceof Error ? e.message : String(e));
      } finally {
        setModelsLoading(false);
      }
    },
    [listModels]
  );

  const probeTailnetProviders = useCallback(async () => {
    const presets = PROVIDER_PRESETS.filter((preset) => preset.probeAvailability);
    setProviderAvailability((current) => ({
      ...current,
      ...Object.fromEntries(presets.map((preset) => [preset.id, 'checking'])),
    }));
    try {
      const result = await tailnetHosts();
      const onlineHosts = new Set(
        result.hosts
          .filter((host) => host.online)
          .map((host) => normalizeTailnetHostName(host.hostName))
      );
      setProviderAvailability((current) => ({
        ...current,
        ...Object.fromEntries(
          presets.map((preset) => [
            preset.id,
            onlineHosts.has(normalizeTailnetHostName(preset.tailnetHostName ?? preset.label))
              ? 'available'
              : 'unavailable',
          ])
        ),
      }));
    } catch {
      setProviderAvailability((current) => ({
        ...current,
        ...Object.fromEntries(presets.map((preset) => [preset.id, 'unavailable'])),
      }));
    }
  }, [tailnetHosts]);

  const applyPreset = (preset: ProviderPreset) => {
    setSaveError(null);
    setTestMessage(null);
    setModelsError(null);
    setAvailableModels([]);
    setModelsLoaded(false);
    setSaved(false);
    const nextForm = {
      provider: preset.provider,
      baseUrl: preset.baseUrl,
      model: preset.model,
      apiKey: '',
      reasoningEffort: preset.reasoningEffort,
      maxTokens: String(preset.maxTokens),
    };
    setForm(nextForm);
    void loadAvailableModels(nextForm);
  };

  const handleReasoningClick = async (reasoning: string) => {
    setForm((current) => ({ ...current, reasoningEffort: reasoning }));
    if (!isChatGptForm) {
      return;
    }
    const ok = await copyTextToClipboard(buildChatGptCommand(form, reasoning));
    if (ok) {
      setCopiedPreset(reasoning);
      window.setTimeout(() => setCopiedPreset(null), 1500);
    }
  };

  const handleSave = async () => {
    setSaving(true);
    setSaveError(null);
    setSaved(false);
    try {
      await save(formToUpdate(form));
      setSaved(true);
      setForm((current) => ({ ...current, apiKey: '' }));
      window.setTimeout(() => setSaved(false), 1800);
    } catch (e) {
      setSaveError(e instanceof Error ? e.message : String(e));
    } finally {
      setSaving(false);
    }
  };

  const handleTest = async () => {
    setTesting(true);
    setSaveError(null);
    setTestMessage(null);
    try {
      const result = await testProvider(formToUpdate(form));
      setTestMessage(
        `Test OK en ${result.elapsedMs} ms - ${result.provider} / ${result.model}${
          result.responsePreview ? `: ${result.responsePreview}` : ''
        }`
      );
    } catch (e) {
      setSaveError(e instanceof Error ? e.message : String(e));
    } finally {
      setTesting(false);
    }
  };

  const toggle = () => {
    const next = !expanded;
    if (next) {
      const nextForm = formFromConfig(config);
      setForm(nextForm);
      setSaveError(null);
      setTestMessage(null);
      setModelsError(null);
      setAvailableModels([]);
      setModelsLoaded(false);
      setSaved(false);
      updatePosition();
      probeTailnetProviders();
      void loadAvailableModels(nextForm);
    }
    setExpanded(next);
  };

  const popover =
    expanded && position
      ? createPortal(
          <div
            ref={popoverRef}
            role="dialog"
            aria-label="Détails de la configuration LLM"
            className="popover-panel fixed z-[100] max-h-[min(760px,calc(100vh-1rem))] overflow-y-auto rounded-lg border p-3 text-xs"
            style={position}
          >
            <div className="mb-3 flex items-center justify-between gap-3">
              <div className="flex min-w-0 items-center gap-2 font-medium text-[var(--text-primary)]">
                <SlidersHorizontal className="h-3.5 w-3.5 text-[var(--accent)]" aria-hidden />
                <span>Configuration LLM</span>
              </div>
              <button
                type="button"
                onClick={() => {
                  void refresh();
                  probeTailnetProviders();
                }}
                className="control-button inline-flex items-center gap-1 rounded-md border px-2 py-1"
              >
                <RefreshCw className="h-3 w-3" aria-hidden />
                Rafraîchir
              </button>
            </div>

            <dl className="mb-3 grid grid-cols-[100px_1fr] gap-x-3 gap-y-1.5 rounded-md border border-[var(--border)] bg-[var(--panel-bg-muted)] p-2">
              <dt className="text-[var(--text-muted)]">État</dt>
              <dd className="text-[var(--text-primary)]">{statusLabel(status)}</dd>
              <dt className="text-[var(--text-muted)]">Fournisseur</dt>
              <dd className="text-[var(--text-primary)]">{config?.provider ?? 'non configuré'}</dd>
              <dt className="text-[var(--text-muted)]">Modèle</dt>
              <dd className="text-[var(--text-primary)]">{config?.model ?? 'modèle ?'}</dd>
              <dt className="text-[var(--text-muted)]">Réflexion</dt>
              <dd className="uppercase text-[var(--text-primary)]">
                {config?.reasoningEffort ?? 'non renseigné'}
              </dd>
            </dl>

            <div className="mb-3">
              <div className="mb-2 flex items-center gap-2 text-[11px] font-medium uppercase tracking-wide text-[var(--text-muted)]">
                <ShieldCheck className="h-3.5 w-3.5" aria-hidden />
                Choix du fournisseur
              </div>
              <div className="grid grid-cols-2 gap-2">
                {visibleProviderPresets.map((preset) => {
                  const active = activePreset.id === preset.id && form.provider === preset.provider;
                  return (
                    <button
                      key={preset.id}
                      type="button"
                      onClick={() => applyPreset(preset)}
                      className={clsx(
                        'rounded-md border p-2 text-left transition',
                        active
                          ? 'border-[var(--border-emphasis)] bg-[var(--control-bg-active)] text-[var(--text-primary)]'
                          : 'border-[var(--border)] bg-[var(--panel-bg-muted)] text-[var(--text-secondary)] hover:border-[var(--border-emphasis)] hover:bg-[var(--panel-bg-strong)]'
                      )}
                      aria-label={`Choisir ${preset.label}`}
                    >
                      <div className="mb-1 flex items-center justify-between gap-2">
                        <span className="font-medium">{preset.label}</span>
                        <span className="rounded bg-[var(--accent-soft)] px-1.5 py-0.5 text-[10px] text-[var(--accent-strong)]">
                          {preset.badge}
                        </span>
                      </div>
                      <div className="line-clamp-2 text-[10px] text-[var(--text-muted)]">
                        {preset.privacy}
                      </div>
                    </button>
                  );
                })}
              </div>
              {probingTailnetProviders && (
                <div className="mt-2 text-[10px] text-[var(--text-muted)]">
                  Détection des machines Tailscale disponibles...
                </div>
              )}
            </div>

            <div className="mb-3 grid grid-cols-2 gap-2">
              <label className="block">
                <span className="mb-1 block text-[11px] text-[var(--text-muted)]">Provider</span>
                <input
                  className={FIELD_CLASS}
                  value={form.provider}
                  onChange={(event) => {
                    setAvailableModels([]);
                    setModelsLoaded(false);
                    setForm((current) => ({ ...current, provider: event.target.value }));
                  }}
                />
              </label>
              <label className="block">
                <span className="mb-1 flex items-center justify-between gap-2 text-[11px] text-[var(--text-muted)]">
                  Modèle
                  {canListModelsFor(form) && (
                    <button
                      type="button"
                      onClick={() => void loadAvailableModels(form)}
                      disabled={modelsLoading}
                      className="control-button inline-flex items-center gap-1 rounded border px-1.5 py-0.5 text-[10px] disabled:opacity-60"
                      title="Actualiser les modèles disponibles"
                    >
                      <RefreshCw
                        className={clsx('h-3 w-3', modelsLoading && 'animate-spin')}
                        aria-hidden
                      />
                      Modèles
                    </button>
                  )}
                </span>
                {availableModels.length > 0 ? (
                  <select
                    className={FIELD_CLASS}
                    value={form.model}
                    onChange={(event) => setForm((current) => ({ ...current, model: event.target.value }))}
                    aria-label="Modèle"
                  >
                    {availableModels.map((model) => (
                      <option key={model} value={model}>
                        {model}
                      </option>
                    ))}
                  </select>
                ) : (
                  <input
                    className={FIELD_CLASS}
                    value={form.model}
                    onChange={(event) => setForm((current) => ({ ...current, model: event.target.value }))}
                    aria-label="Modèle"
                  />
                )}
              </label>
              <label className="col-span-2 block">
                <span className="mb-1 block text-[11px] text-[var(--text-muted)]">URL OpenAI-compatible</span>
                <input
                  className={FIELD_CLASS}
                  value={form.baseUrl}
                  onChange={(event) => {
                    setAvailableModels([]);
                    setModelsLoaded(false);
                    setForm((current) => ({ ...current, baseUrl: event.target.value }));
                  }}
                />
              </label>
              <label className="block">
                <span className="mb-1 flex items-center gap-1 text-[11px] text-[var(--text-muted)]">
                  <KeyRound className="h-3 w-3" aria-hidden />
                  Clé API
                </span>
                <input
                  className={FIELD_CLASS}
                  type="password"
                  value={form.apiKey}
                  placeholder={activePreset.requiresKey ? 'laisser vide pour conserver/env' : 'facultatif'}
                  onChange={(event) => setForm((current) => ({ ...current, apiKey: event.target.value }))}
                />
              </label>
              <label className="block">
                <span className="mb-1 block text-[11px] text-[var(--text-muted)]">Max tokens</span>
                <input
                  className={FIELD_CLASS}
                  inputMode="numeric"
                  value={form.maxTokens}
                  onChange={(event) => setForm((current) => ({ ...current, maxTokens: event.target.value }))}
                />
              </label>
            </div>

            {providerAdvice && (
              <div className="mb-3 rounded-md border border-amber-500/30 bg-amber-500/10 px-2 py-1.5 text-[11px] text-amber-700">
                {providerAdvice}
              </div>
            )}

            <div className="mb-3 rounded-md border border-[var(--border)] bg-[var(--panel-bg-muted)] p-2">
              <div className="mb-2 flex items-center justify-between gap-2">
                <div className="text-[11px] uppercase tracking-wide text-[var(--text-muted)]">
                  Niveau de réflexion
                </div>
                {isChatGptForm ? (
                  <code className="truncate font-mono text-[10px] text-[var(--text-faint)]">
                    {previewCommand}
                  </code>
                ) : (
                  <span className="text-[10px] text-[var(--text-faint)]">
                    None recommandé pour les providers locaux
                  </span>
                )}
              </div>
              <div className="grid grid-cols-5 gap-1.5">
                {REASONING_PRESETS.map((preset) => {
                  const active = activeReasoning === preset.value;
                  const copied = copiedPreset === preset.value;
                  return (
                    <button
                      key={preset.value}
                      type="button"
                      onClick={() => void handleReasoningClick(preset.value)}
                      className={clsx(
                        'flex items-center justify-between gap-1 rounded-md border px-2 py-1.5 text-left transition',
                        active
                          ? 'border-[var(--border-emphasis)] bg-[var(--control-bg-active)] text-[var(--text-primary)]'
                          : 'border-[var(--border)] bg-[var(--panel-bg-strong)] text-[var(--text-secondary)] hover:bg-[var(--control-bg-hover)]'
                      )}
                      aria-label={
                        isChatGptForm
                          ? `Copier la commande de configuration LLM en ${preset.value}`
                          : `Choisir le niveau de réflexion ${preset.value}`
                      }
                      title={
                        isChatGptForm
                          ? `Copier la commande ${preset.value}`
                          : `Choisir ${preset.value}`
                      }
                    >
                      <span className="min-w-0">
                        <span className="block text-xs font-medium">{preset.label}</span>
                        <span className="block text-[10px] text-[var(--text-muted)]">{preset.hint}</span>
                      </span>
                      {copied ? (
                        <Check className="h-3.5 w-3.5 shrink-0 text-[var(--success)]" aria-hidden />
                      ) : !isChatGptForm && active ? (
                        <Check className="h-3.5 w-3.5 shrink-0 text-[var(--success)]" aria-hidden />
                      ) : !isChatGptForm ? (
                        <span className="h-3.5 w-3.5 shrink-0" aria-hidden />
                      ) : (
                        <Copy className="h-3.5 w-3.5 shrink-0 text-[var(--text-faint)]" aria-hidden />
                      )}
                    </button>
                  );
                })}
              </div>
            </div>

            {saveError && (
              <div className="mb-2 rounded-md border border-red-500/30 bg-red-500/10 px-2 py-1.5 text-[var(--danger)]">
                {saveError}
              </div>
            )}
            {modelsError && (
              <div className="mb-2 rounded-md border border-amber-500/30 bg-amber-500/10 px-2 py-1.5 text-amber-700">
                Modèles indisponibles: {modelsError}
              </div>
            )}
            {modelsLoading && (
              <div className="mb-2 rounded-md border border-[var(--border)] bg-[var(--panel-bg-muted)] px-2 py-1.5 text-[var(--text-muted)]">
                Lecture des modèles disponibles...
              </div>
            )}
            {modelsLoaded && availableModels.length === 0 && (
              <div className="mb-2 rounded-md border border-amber-500/30 bg-amber-500/10 px-2 py-1.5 text-amber-700">
                Aucun modèle chat détecté sur ce provider. Si tu utilises LM Studio, charge un modèle de chat et relance Modèles.
              </div>
            )}
            {testMessage && (
              <div className="mb-2 rounded-md border border-emerald-500/30 bg-emerald-500/10 px-2 py-1.5 text-[var(--success)]">
                {testMessage}
              </div>
            )}
            {saved && (
              <div className="mb-2 rounded-md border border-emerald-500/30 bg-emerald-500/10 px-2 py-1.5 text-[var(--success)]">
                Configuration enregistrée. Le prochain message utilisera ce fournisseur.
              </div>
            )}

            <div className="flex justify-end gap-2">
              <button
                type="button"
                onClick={closePopover}
                className="control-button rounded-md border px-2 py-1 text-xs"
              >
                Fermer
              </button>
              <button
                type="button"
                onClick={() => void handleTest()}
                disabled={testing || isChatGptForm || modelsLoading || !hasModel}
                className="control-button inline-flex items-center gap-1 rounded-md border px-2 py-1 text-xs disabled:opacity-60"
                title={
                  isChatGptForm
                    ? 'ChatGPT OAuth se vérifie via le diagnostic/login'
                    : !hasModel
                      ? 'Choisis un modèle disponible avant de tester'
                    : 'Tester sans enregistrer'
                }
              >
                {testing ? (
                  <RefreshCw className="h-3 w-3 animate-spin" aria-hidden />
                ) : (
                  <Check className="h-3 w-3" aria-hidden />
                )}
                Tester
              </button>
              <button
                type="button"
                onClick={() => void handleSave()}
                disabled={saving || modelsLoading || !hasModel}
                className="primary-action inline-flex items-center gap-1 rounded-md px-2 py-1 text-xs disabled:opacity-60"
                title={!hasModel ? 'Choisis un modèle disponible avant d’enregistrer' : undefined}
              >
                {saving ? (
                  <RefreshCw className="h-3 w-3 animate-spin" aria-hidden />
                ) : (
                  <Save className="h-3 w-3" aria-hidden />
                )}
                Enregistrer
              </button>
            </div>
          </div>,
          document.body
        )
      : null;

  return (
    <div className="relative">
      <button
        ref={anchorRef}
        type="button"
        onClick={toggle}
        className={clsx(
          'control-button flex max-w-[280px] items-center gap-1.5 rounded-md border px-2 py-1 text-xs transition',
          status === 'ready' && 'border-[var(--border-strong)]',
          status === 'checking' && 'border-amber-500/50 text-amber-600',
          (status === 'missing' || status === 'error') && 'border-red-500/50 text-red-600'
        )}
        aria-label={`Configuration LLM : ${detail || label}. Cliquer pour les détails.`}
        aria-expanded={expanded}
        title={detail || label}
      >
        {status === 'checking' ? (
          <RefreshCw className="h-3.5 w-3.5 animate-spin" aria-hidden />
        ) : (
          <Cpu className="h-3.5 w-3.5" aria-hidden />
        )}
        <span className="hidden truncate lg:inline">{label}</span>
        {config?.reasoningEffort && (
          <span className="hidden rounded bg-[var(--accent-soft)] px-1.5 py-0.5 text-[10px] uppercase tracking-wide text-[var(--accent-strong)] xl:inline">
            {config.reasoningEffort}
          </span>
        )}
      </button>
      {popover}
    </div>
  );
}

function statusLabel(status: LlmConfigState['status']): string {
  if (status === 'ready') return 'Prêt';
  if (status === 'checking') return 'Vérification';
  if (status === 'missing') return 'Non configuré';
  return 'Indisponible';
}

function formFromConfig(config: LlmConfigState['config']): LlmSettingsForm {
  const provider = config?.provider?.trim().toLowerCase();
  const preset =
    PROVIDER_PRESETS.find((candidate) => candidate.provider === provider) ?? PROVIDER_PRESETS[0];
  return {
    provider: provider || preset.provider,
    baseUrl: preset.baseUrl,
    model: config?.model?.trim() || preset.model,
    apiKey: '',
    reasoningEffort: config?.reasoningEffort?.trim().toLowerCase() || preset.reasoningEffort,
    maxTokens: String(config?.maxTokens ?? preset.maxTokens),
  };
}

function formToUpdate(form: LlmSettingsForm) {
  const maxTokens = Number.parseInt(form.maxTokens, 10);
  return {
    provider: form.provider.trim(),
    baseUrl: form.baseUrl.trim(),
    model: form.model.trim(),
    apiKey: form.apiKey.trim() ? form.apiKey.trim() : undefined,
    reasoningEffort: form.reasoningEffort.trim() || undefined,
    maxTokens: Number.isFinite(maxTokens) ? maxTokens : undefined,
  };
}

function buildChatGptCommand(form: LlmSettingsForm, reasoning: string): string {
  const model = form.model.trim() || 'gpt-5.5';
  const maxTokens = Number.parseInt(form.maxTokens, 10) || 8192;
  return `.\\config-chatgpt.cmd -Model ${model} -Reasoning ${reasoning} -MaxTokens ${maxTokens}`;
}

function canListModelsFor(form: LlmSettingsForm): boolean {
  return form.provider.trim().toLowerCase() !== 'chatgpt' && form.baseUrl.trim().length > 0;
}

function preferredModel(models: string[], current: string): string {
  if (models.includes(current)) return current;
  const sizedModels = models
    .map((model) => ({ model, size: modelSizeInBillions(model) }))
    .filter((entry): entry is { model: string; size: number } => entry.size !== null)
    .sort((left, right) => left.size - right.size);
  if (sizedModels.length > 0) return sizedModels[0].model;
  return models[0] ?? current;
}

function modelSizeInBillions(model: string): number | null {
  const match = model.toLowerCase().match(/(?:^|[:\\/-])(?:[a-z]+)?(\d+(?:\.\d+)?)b(?:\b|[-_:])/);
  return match ? Number.parseFloat(match[1]) : null;
}

function providerCapabilityHint(
  form: LlmSettingsForm,
  models: string[],
  modelsLoaded: boolean
): string | null {
  const provider = form.provider.trim().toLowerCase();
  if (provider === 'chatgpt') {
    return 'Mode agent complet: recommandé pour les analyses Sample avec recherche de code et fichiers concernés.';
  }
  const baseUrl = normalizeBaseUrl(form.baseUrl);
  if (
    provider.includes('ollama') ||
    provider.includes('lm-studio') ||
    provider.includes('lmstudio') ||
    baseUrl.includes('11434') ||
    baseUrl.includes('1234')
  ) {
    if (models.length > 0) {
      return `${models.length} modèle${models.length > 1 ? 's' : ''} détecté${models.length > 1 ? 's' : ''} depuis le provider local; choisis dans la liste réelle exposée par /v1/models. Les appels outils restent à valider modèle par modèle.`;
    }
    if (modelsLoaded) {
      return 'Provider local joignable, mais aucun modèle chat n’est exposé par /v1/models.';
    }
    return 'Mode local: clique sur Modèles pour interroger /v1/models, puis privilégie None en réflexion.';
  }
  return null;
}

function normalizeBaseUrl(value: string): string {
  return value.trim().replace(/\/+$/, '').toLowerCase();
}

function normalizeTailnetHostName(value: string): string {
  return value.trim().replace(/\.$/, '').split('.')[0]?.toLowerCase() ?? '';
}

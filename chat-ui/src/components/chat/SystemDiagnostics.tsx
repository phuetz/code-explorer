import { useCallback, useState } from 'react';
import { createPortal } from 'react-dom';
import { Activity, Check, Copy, RefreshCw, ShieldCheck, TriangleAlert, X } from 'lucide-react';
import clsx from 'clsx';
import { mcpClient, type DiagnosticsInfo } from '../../api/mcp-client';
import { useAnchoredPopover } from '../../hooks/use-anchored-popover';
import { useChatStore } from '../../stores/chat-store';
import { formatExportTimestamp, parseIndexedAt } from '../../utils/dates';
import { copyTextToClipboard } from '../../utils/clipboard';

type DiagnosticsStatus = 'idle' | 'loading' | 'ready' | 'error';

export function SystemDiagnostics() {
  const selectedRepo = useChatStore((s) => s.selectedRepo);
  const selectedRepoName = useChatStore((s) => s.selectedRepoName);
  const [open, setOpen] = useState(false);
  const [status, setStatus] = useState<DiagnosticsStatus>('idle');
  const [diagnostics, setDiagnostics] = useState<DiagnosticsInfo | null>(null);
  const [error, setError] = useState('');
  const [copied, setCopied] = useState(false);
  const closePopover = useCallback(() => setOpen(false), []);
  const { anchorRef, popoverRef, position, updatePosition } = useAnchoredPopover<
    HTMLButtonElement,
    HTMLDivElement
  >({
    maxWidth: 480,
    minWidth: 360,
    onClose: closePopover,
    open,
  });

  const refresh = async () => {
    setStatus('loading');
    setError('');
    try {
      const next = await mcpClient.diagnostics();
      setDiagnostics(next);
      setStatus('ready');
    } catch (e) {
      setDiagnostics(null);
      setStatus('error');
      setError(e instanceof Error ? e.message : String(e));
    }
  };

  const toggle = () => {
    const nextOpen = !open;
    if (nextOpen) updatePosition();
    setOpen(nextOpen);
    if (nextOpen && status === 'idle') {
      void refresh();
    }
  };

  const copyReport = async () => {
    if (!diagnostics) return;
    const ok = await copyTextToClipboard(
      formatDiagnosticsReport({
        diagnostics,
        selectedRepo,
        selectedRepoName,
        frontend: window.location.host || 'navigateur local',
        backend: mcpClient.baseUrl || 'proxy Vite / même origine',
      })
    );
    if (ok) {
      setCopied(true);
      setTimeout(() => setCopied(false), 1800);
    }
  };

  const popover =
    open && position
      ? createPortal(
          <div
            ref={popoverRef}
            role="dialog"
            aria-label="Diagnostic système GitNexus"
            className="popover-panel fixed z-[100] overflow-y-auto rounded-lg border p-4 text-xs text-neutral-300 shadow-2xl"
            style={position}
          >
            <div className="mb-3 flex items-start justify-between gap-3">
              <div>
                <div className="font-medium text-neutral-100">Diagnostic GitNexus</div>
                <div className="mt-1 text-neutral-500">{statusLabel(status, diagnostics)}</div>
              </div>
              <div className="flex items-center gap-1">
                <button
                  type="button"
                  onClick={() => void copyReport()}
                  disabled={!diagnostics}
                  className="rounded-md border border-neutral-800 p-1.5 text-neutral-400 hover:bg-neutral-900 hover:text-neutral-100 disabled:cursor-not-allowed disabled:opacity-40"
                  aria-label="Copier le rapport de diagnostic"
                  title={copied ? 'Copié' : 'Copier le rapport'}
                >
                  {copied ? (
                    <Check className="h-3.5 w-3.5 text-emerald-300" aria-hidden />
                  ) : (
                    <Copy className="h-3.5 w-3.5" aria-hidden />
                  )}
                </button>
                <button
                  type="button"
                  onClick={() => void refresh()}
                  className="rounded-md border border-neutral-800 p-1.5 text-neutral-400 hover:bg-neutral-900 hover:text-neutral-100"
                  aria-label="Rafraîchir le diagnostic"
                  title="Rafraîchir"
                >
                  <RefreshCw
                    className={clsx('h-3.5 w-3.5', status === 'loading' && 'animate-spin')}
                  />
                </button>
                <button
                  type="button"
                  onClick={closePopover}
                  className="rounded-md border border-neutral-800 p-1.5 text-neutral-400 hover:bg-neutral-900 hover:text-neutral-100"
                  aria-label="Fermer le diagnostic"
                  title="Fermer"
                >
                  <X className="h-3.5 w-3.5" />
                </button>
              </div>
            </div>

            {status === 'error' ? (
              <div className="rounded-md border border-red-900/60 bg-red-950/20 p-3 text-red-200">
                <div className="mb-1 flex items-center gap-2 font-medium">
                  <TriangleAlert className="h-4 w-4" aria-hidden />
                  Diagnostic indisponible
                </div>
                <div className="leading-relaxed text-red-200/80">{error}</div>
              </div>
            ) : (
              <DiagnosticsBody
                diagnostics={diagnostics}
                selectedRepo={selectedRepo}
                selectedRepoName={selectedRepoName}
              />
            )}
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
          'flex items-center gap-1.5 rounded-md border px-2 py-1 text-xs transition hover:bg-neutral-900',
          status === 'error'
            ? 'border-red-900/70 bg-red-950/20 text-red-300'
            : 'border-neutral-800 bg-neutral-900/60 text-neutral-300'
        )}
        aria-label="Ouvrir le diagnostic système"
        aria-expanded={open}
        title="Diagnostic système"
      >
        {status === 'loading' ? (
          <RefreshCw className="h-3.5 w-3.5 animate-spin" aria-hidden />
        ) : (
          <Activity className="h-3.5 w-3.5" aria-hidden />
        )}
        <span className="hidden xl:inline">Diagnostic</span>
      </button>
      {popover}
    </div>
  );
}

function DiagnosticsBody({
  diagnostics,
  selectedRepo,
  selectedRepoName,
}: {
  diagnostics: DiagnosticsInfo | null;
  selectedRepo: string | null;
  selectedRepoName: string | null;
}) {
  const frontend = window.location.host || 'navigateur local';
  const backend = mcpClient.baseUrl || 'proxy Vite / même origine';
  const llm = diagnostics?.llm;
  const oauth = diagnostics?.auth?.chatgptOAuth;
  const repos = diagnostics?.repos.names ?? [];
  const activeRepoMissing = Boolean(
    diagnostics &&
      selectedRepo &&
      !repos.some((repo) => repo.id === selectedRepo || repo.name === selectedRepo)
  );

  return (
    <div className="space-y-3">
      <div className="grid gap-2 sm:grid-cols-2">
        <DiagnosticItem label="Frontend" value={frontend} />
        <DiagnosticItem label="Backend" value={backend} />
        <DiagnosticItem
          label="Service"
          value={diagnostics ? `${diagnostics.service} ${diagnostics.version}` : 'lecture...'}
        />
        <DiagnosticItem
          label="Projet actif"
          value={selectedRepoName ?? selectedRepo ?? 'aucun projet sélectionné'}
        />
      </div>

      <div className="grid gap-2 sm:grid-cols-3">
        <BooleanItem
          label="Auth HTTP"
          value={diagnostics ? diagnostics.httpAuthRequired : null}
          trueText="requise"
          falseText="non requise"
        />
        <BooleanItem
          label="Chemins repo"
          value={diagnostics ? diagnostics.repoPathsExposed : null}
          trueText="exposés"
          falseText="masqués"
        />
        <DiagnosticItem
          label="Repos indexés"
          value={diagnostics ? String(diagnostics.repos.count) : '...'}
        />
      </div>

      {activeRepoMissing && (
        <div className="rounded-md border border-amber-900/60 bg-amber-950/20 px-3 py-2 text-amber-200">
          Projet actif absent de /api/diagnostics. Rafraîchis la liste des projets ou relance le
          backend avec <code className="rounded bg-neutral-900 px-1">.\gitnexus.cmd chat -RestartBackend</code>.
        </div>
      )}

      <div className="rounded-md border border-neutral-800 bg-neutral-900/40 p-3">
        <div className="mb-2 flex items-center justify-between gap-3">
          <div className="font-medium text-neutral-100">Index local</div>
          <div className="text-[11px] text-neutral-500">
            {diagnostics ? `${diagnostics.repos.count} projet(s)` : 'lecture...'}
          </div>
        </div>
        {repos.length > 0 ? (
          <div className="space-y-1.5">
            {repos.slice(0, 6).map((repo) => (
              <div
                key={repo.id}
                className="flex items-center justify-between gap-3 rounded border border-neutral-800 bg-neutral-950/40 px-2 py-1.5"
              >
                <div className="min-w-0">
                  <div className="truncate font-medium text-neutral-200" title={repo.name}>
                    {repo.name}
                  </div>
                  <div className="truncate font-mono text-[10px] text-neutral-600" title={repo.id}>
                    {repo.id}
                  </div>
                </div>
                <div className="shrink-0 text-right text-[11px] text-neutral-500">
                  {formatIndexedAt(repo.indexedAt)}
                </div>
              </div>
            ))}
            {repos.length > 6 && (
              <div className="text-[11px] text-neutral-500">
                +{repos.length - 6} autre(s) projet(s)
              </div>
            )}
          </div>
        ) : (
          <div className="rounded border border-dashed border-neutral-800 px-2 py-3 text-center text-neutral-500">
            {diagnostics ? 'Aucun projet indexé' : 'Lecture des projets...'}
          </div>
        )}
      </div>

      <div className="rounded-md border border-neutral-800 bg-neutral-900/40 p-3">
        <div className="mb-2 flex items-center gap-2 font-medium text-neutral-100">
          <ShieldCheck className="h-4 w-4 text-emerald-300" aria-hidden />
          LLM
        </div>
        <div className="grid gap-2 sm:grid-cols-2">
          <DiagnosticItem
            label="Provider"
            value={llm?.configured ? (llm.provider ?? 'inconnu') : 'non configuré'}
          />
          <DiagnosticItem label="Modèle" value={llm?.model ?? 'non défini'} />
          <DiagnosticItem label="Raisonnement" value={llm?.reasoningEffort ?? 'défaut'} />
          <DiagnosticItem
            label="Max tokens"
            value={llm?.maxTokens ? String(llm.maxTokens) : 'défaut'}
          />
          <DiagnosticItem label="OAuth ChatGPT" value={oauthStatusLabel(oauth?.status)} />
          <DiagnosticItem
            label="Dernier refresh"
            value={formatOAuthRefresh(oauth?.lastRefresh)}
          />
        </div>
      </div>
    </div>
  );
}

function DiagnosticItem({ label, value }: { label: string; value: string }) {
  return (
    <div className="min-w-0 rounded-md border border-neutral-800 bg-neutral-900/40 px-3 py-2">
      <div className="text-[11px] uppercase text-neutral-500">{label}</div>
      <div className="mt-1 truncate font-mono text-neutral-200" title={value}>
        {value}
      </div>
    </div>
  );
}

function BooleanItem({
  label,
  value,
  trueText,
  falseText,
}: {
  label: string;
  value: boolean | null;
  trueText: string;
  falseText: string;
}) {
  const text = value === null ? '...' : value ? trueText : falseText;
  const tone =
    value === null
      ? 'text-neutral-400'
      : value
        ? 'text-amber-300'
        : 'text-emerald-300';
  return (
    <div className="rounded-md border border-neutral-800 bg-neutral-900/40 px-3 py-2">
      <div className="text-[11px] uppercase text-neutral-500">{label}</div>
      <div className={clsx('mt-1 font-medium', tone)}>{text}</div>
    </div>
  );
}

function statusLabel(status: DiagnosticsStatus, diagnostics: DiagnosticsInfo | null): string {
  if (status === 'loading') return 'Lecture en cours...';
  if (status === 'error') return 'Erreur de diagnostic';
  if (!diagnostics) return 'Pas encore rafraîchi';
  return `Généré ${formatExportTimestamp(diagnostics.generatedAtUnixMs)}`;
}

function oauthStatusLabel(status: string | undefined): string {
  switch (status) {
    case 'logged_in':
      return 'connecté';
    case 'missing':
      return 'non détecté';
    case 'incomplete':
      return 'incomplet';
    case 'invalid':
      return 'fichier invalide';
    case 'unreadable':
      return 'illisible';
    default:
      return status ?? 'inconnu';
  }
}

function formatOAuthRefresh(raw: string | null | undefined): string {
  if (!raw) return 'non défini';
  const timestamp = new Date(raw).getTime();
  if (Number.isNaN(timestamp)) return raw;
  return formatExportTimestamp(timestamp);
}

function formatIndexedAt(raw: string | null | undefined): string {
  const date = parseIndexedAt(raw);
  if (!date) return 'date inconnue';
  return date.toLocaleDateString();
}

function formatDiagnosticsReport({
  diagnostics,
  selectedRepo,
  selectedRepoName,
  frontend,
  backend,
}: {
  diagnostics: DiagnosticsInfo;
  selectedRepo: string | null;
  selectedRepoName: string | null;
  frontend: string;
  backend: string;
}): string {
  const llm = diagnostics.llm;
  const oauth = diagnostics.auth?.chatgptOAuth;
  const repos = diagnostics.repos.names;
  const repoLines =
    repos.length > 0
      ? repos
          .slice(0, 10)
          .map((repo) => `- ${repo.name} (${repo.id}) index=${formatIndexedAt(repo.indexedAt)}`)
          .join('\n')
      : '- aucun projet indexé';

  return [
    '# Diagnostic GitNexus',
    '',
    `Généré: ${formatExportTimestamp(diagnostics.generatedAtUnixMs)}`,
    `Frontend: ${frontend}`,
    `Backend: ${backend}`,
    `Service: ${diagnostics.service} ${diagnostics.version}`,
    `Projet actif: ${selectedRepoName ?? selectedRepo ?? 'aucun projet sélectionné'}`,
    `Auth HTTP: ${diagnostics.httpAuthRequired ? 'requise' : 'non requise'}`,
    `Chemins repo: ${diagnostics.repoPathsExposed ? 'exposés' : 'masqués'}`,
    '',
    '## LLM',
    `Provider: ${llm.configured ? (llm.provider ?? 'inconnu') : 'non configuré'}`,
    `Modèle: ${llm.model ?? 'non défini'}`,
    `Raisonnement: ${llm.reasoningEffort ?? 'défaut'}`,
    `Max tokens: ${llm.maxTokens ? String(llm.maxTokens) : 'défaut'}`,
    `OAuth ChatGPT: ${oauthStatusLabel(oauth?.status)}`,
    `Dernier refresh: ${formatOAuthRefresh(oauth?.lastRefresh)}`,
    '',
    `## Index local (${diagnostics.repos.count} projet(s))`,
    repoLines,
  ].join('\n');
}

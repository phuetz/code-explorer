import { useMemo, useState } from 'react';
import clsx from 'clsx';
import { User, Bot, Copy, RotateCcw, Check, Loader2, Wrench, X, ShieldCheck, ClipboardCheck } from 'lucide-react';
import type { Message, ToolCall } from '../../types/chat';
import type { LlmConfigInfo } from '../../api/mcp-client';
import { Markdown } from '../ui/Markdown';
import {
  extractSourceReferences,
  groupSourceReferences,
  type SourceReference,
  type SourceValidationState,
} from '../../utils/source-references';
import { formatMessageTimestamp } from '../../utils/dates';
import { copyTextToClipboard } from '../../utils/clipboard';
import { EMPTY_ASSISTANT_RESPONSE_MESSAGE } from '../../utils/empty-assistant-response';
import { RelatedSourceFiles } from './RelatedSourceFiles';

interface Props {
  message: Message;
  llm?: LlmConfigInfo | null;
  onRegenerate?: (messageId: string) => void;
  canRegenerate?: boolean;
  onOpenSourceReference?: (reference: SourceReference) => void;
  repo?: string | null;
  repoName?: string | null;
  validSourcePaths?: ReadonlySet<string> | null;
  sourceValidationState?: SourceValidationState;
}

export function ChatMessage({
  message,
  llm,
  onRegenerate,
  canRegenerate,
  onOpenSourceReference,
  repo,
  repoName,
  validSourcePaths,
  sourceValidationState,
}: Props) {
  const isUser = message.role === 'user';
  const isEmptyAssistant = !isUser && message.content.trim().length === 0;
  const displayContent = isEmptyAssistant ? EMPTY_ASSISTANT_RESPONSE_MESSAGE : message.content;
  const [copied, setCopied] = useState(false);
  const timestamp = formatMessageTimestamp(message.createdAt);
  const llmLabel = isUser ? null : formatLlmBadge(llm);
  const evidence = useMemo(
    () => summarizeEvidence(message, validSourcePaths, sourceValidationState),
    [message, sourceValidationState, validSourcePaths]
  );

  const handleCopy = async () => {
    const ok = await copyTextToClipboard(displayContent);
    if (ok) {
      setCopied(true);
      setTimeout(() => setCopied(false), 1500);
    }
  };

  return (
    <div
      className={clsx(
        'group flex',
        isUser ? 'justify-end' : 'justify-start'
      )}
    >
      <div
        className={clsx(
          'flex gap-3',
          isUser ? 'max-w-[76%] flex-row-reverse' : 'w-full max-w-none'
        )}
      >
        <div
          className={clsx(
            'flex h-8 w-8 shrink-0 items-center justify-center rounded-md border',
            isUser
              ? 'message-avatar-user'
              : 'message-avatar-assistant'
          )}
        >
          {isUser ? <User size={16} /> : <Bot size={16} />}
        </div>
        <div
          className={clsx(
            'min-w-0 flex-1 rounded-lg border px-4 py-3',
            isUser
              ? 'message-card-user'
              : 'message-card-assistant'
          )}
        >
          <div className="mb-2 flex items-center gap-2">
            <span className="text-xs font-medium text-neutral-500">
              {isUser ? 'Vous' : 'Code Explorer'}
            </span>
            {timestamp && (
              <time
                dateTime={new Date(message.createdAt).toISOString()}
                className="text-[11px] text-neutral-600"
                title={timestamp}
              >
                {timestamp}
              </time>
            )}
            {llmLabel && (
              <span
                className="status-pill max-w-[16rem] truncate rounded px-1.5 py-0.5 text-[10px] uppercase tracking-wide"
                title={`LLM actif : ${llmLabel}`}
              >
                {llmLabel}
              </span>
            )}
            {!isUser && message.verified && (
              <span
                className="inline-flex items-center gap-1 rounded border border-emerald-500/35 bg-emerald-500/10 px-1.5 py-0.5 text-[10px] font-medium uppercase tracking-wide text-emerald-500"
                title="Réponse demandée en mode vérifié"
              >
                <ShieldCheck className="h-3 w-3" aria-hidden />
                Vérifié
              </span>
            )}
            {displayContent && (
              <div
                className={clsx(
                  'ml-auto flex items-center gap-1 transition-opacity focus-within:opacity-100',
                  isEmptyAssistant ? 'opacity-100' : 'opacity-0 group-hover:opacity-100'
                )}
              >
                <button
                  type="button"
                  onClick={handleCopy}
                  className="icon-button rounded p-1"
                  aria-label={isUser ? 'Copier le message' : 'Copier la réponse'}
                  title={copied ? 'Copié !' : 'Copier'}
                >
                  {copied ? <Check size={12} /> : <Copy size={12} />}
                </button>
                {!isUser && onRegenerate && canRegenerate && (
                  <button
                    type="button"
                    onClick={() => onRegenerate(message.id)}
                    className="icon-button rounded p-1"
                    aria-label="Régénérer la réponse"
                    title="Régénérer"
                  >
                    <RotateCcw size={12} />
                  </button>
                )}
              </div>
            )}
          </div>
          {!isUser && message.toolCalls && message.toolCalls.length > 0 && (
            <div className="mb-3 flex flex-wrap gap-1.5" aria-label="Outils invoqués par l'agent">
              {message.toolCalls.map((tc) => (
                <ToolCallBadge key={tc.id} toolCall={tc} />
              ))}
            </div>
          )}
          {isUser ? (
            <div className="whitespace-pre-wrap text-sm leading-6 text-[var(--text-primary)]">
              {message.content}
            </div>
          ) : (
            <>
              <Markdown
                onOpenSourceReference={onOpenSourceReference}
                validSourcePaths={validSourcePaths}
                sourceValidationState={sourceValidationState}
              >
                {displayContent}
              </Markdown>
              <RelatedSourceFiles
                content={message.content}
                messageCreatedAt={message.createdAt}
                repo={repo ?? null}
                repoName={repoName ?? null}
                onOpenSourceReference={onOpenSourceReference}
                validSourcePaths={validSourcePaths}
                sourceValidationState={sourceValidationState}
              />
              {evidence && <EvidencePanel evidence={evidence} />}
            </>
          )}
        </div>
      </div>
    </div>
  );
}

interface EvidenceSummary {
  toolDone: number;
  toolErrors: number;
  sourceTotal: number;
  sourceValid: number;
  sourceRejected: number;
  sourcePending: boolean;
}

function summarizeEvidence(
  message: Message,
  validSourcePaths: ReadonlySet<string> | null | undefined,
  sourceValidationState: SourceValidationState | undefined
): EvidenceSummary | null {
  const toolCalls = message.toolCalls ?? [];
  const groups = groupSourceReferences(extractSourceReferences(message.content));
  if (toolCalls.length === 0 && groups.length === 0 && !message.verified) return null;

  const normalizedPaths = validSourcePaths
    ? new Set(Array.from(validSourcePaths).map((path) => path.replace(/\\/g, '/').toLowerCase()))
    : null;
  const sourcePending = sourceValidationState === 'pending';
  const sourceValid =
    sourceValidationState === 'ready' && normalizedPaths
      ? groups.filter((group) => normalizedPaths.has(group.path.replace(/\\/g, '/').toLowerCase())).length
      : sourcePending
        ? 0
        : groups.length;
  const sourceRejected =
    sourceValidationState === 'ready' && normalizedPaths ? groups.length - sourceValid : 0;

  return {
    toolDone: toolCalls.filter((tool) => tool.status === 'done').length,
    toolErrors: toolCalls.filter((tool) => tool.status === 'error').length,
    sourceTotal: groups.length,
    sourceValid,
    sourceRejected,
    sourcePending,
  };
}

function EvidencePanel({ evidence }: { evidence: EvidenceSummary }) {
  return (
    <div className="evidence-panel mt-3 rounded-lg border px-3 py-2 text-[11px]" aria-label="Preuves de la réponse">
      <div className="mb-1 flex items-center gap-1.5 font-medium text-[var(--text-primary)]">
        <ClipboardCheck className="h-3.5 w-3.5 text-[var(--success)]" aria-hidden />
        Preuves
      </div>
      <div className="flex flex-wrap gap-1.5 text-[var(--text-muted)]">
        <span className="rounded border border-[var(--border)] px-1.5 py-0.5">
          {evidence.toolDone} outil{evidence.toolDone > 1 ? 's' : ''} OK
        </span>
        {evidence.toolErrors > 0 && (
          <span className="rounded border border-red-400/35 px-1.5 py-0.5 text-red-500">
            {evidence.toolErrors} outil{evidence.toolErrors > 1 ? 's' : ''} en erreur
          </span>
        )}
        {evidence.sourceTotal > 0 && (
          <span className="rounded border border-[var(--border)] px-1.5 py-0.5">
            {evidence.sourcePending
              ? `${evidence.sourceTotal} source${evidence.sourceTotal > 1 ? 's' : ''} en validation`
              : `${evidence.sourceValid}/${evidence.sourceTotal} source${evidence.sourceTotal > 1 ? 's' : ''} validée${evidence.sourceValid > 1 ? 's' : ''}`}
          </span>
        )}
        {evidence.sourceRejected > 0 && (
          <span className="rounded border border-amber-400/35 px-1.5 py-0.5 text-amber-600">
            {evidence.sourceRejected} rejetée{evidence.sourceRejected > 1 ? 's' : ''}
          </span>
        )}
      </div>
    </div>
  );
}

function formatLlmBadge(llm: LlmConfigInfo | null | undefined): string | null {
  if (!llm?.configured) return null;
  const parts = [llm.provider, llm.model, llm.reasoningEffort].filter(
    (part): part is string => typeof part === 'string' && part.trim().length > 0
  );
  return parts.length > 0 ? parts.join(' · ') : null;
}

function ToolCallBadge({ toolCall }: { toolCall: ToolCall }) {
  const { name, status } = toolCall;
  const tone =
    status === 'running'
      ? 'border-amber-800/60 bg-amber-950/30 text-amber-300'
      : status === 'done'
        ? 'border-emerald-800/60 bg-emerald-950/30 text-emerald-300'
        : status === 'error'
          ? 'border-red-800/60 bg-red-950/30 text-red-300'
          : 'border-neutral-800 bg-neutral-900 text-neutral-400';
  const Icon =
    status === 'running' ? Loader2 : status === 'error' ? X : status === 'done' ? Check : Wrench;
  return (
    <span
      className={clsx(
        'inline-flex items-center gap-1.5 rounded-md border px-2 py-0.5 text-[11px] font-medium',
        tone
      )}
      title={`${name} — ${status}`}
    >
      <Icon size={11} className={status === 'running' ? 'animate-spin' : ''} aria-hidden />
      <code className="font-mono">{name}</code>
    </span>
  );
}

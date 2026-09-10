import { useEffect, useMemo, useRef, type KeyboardEvent } from 'react';
import { Database, Send, ShieldCheck, Sparkles, Square } from 'lucide-react';
import clsx from 'clsx';
import { useChat } from '../../hooks/use-chat';
import { useChatStore } from '../../stores/chat-store';
import type { LlmConfigInfo } from '../../api/mcp-client';
import { reformulateChatPrompt } from '../../utils/prompt-rewrite';

const MIN_HEIGHT = 44;
const MAX_HEIGHT = 200;

interface Props {
  llm?: LlmConfigInfo | null;
  wide?: boolean;
}

export function ChatInput({ llm = null, wide = false }: Props) {
  const { sendMessage, cancel, isStreaming } = useChat(llm);
  const selectedRepo = useChatStore((s) => s.selectedRepo);
  const selectedRepoName = useChatStore((s) => s.selectedRepoName);
  const value = useChatStore((s) => s.inputDraft);
  const setValue = useChatStore((s) => s.setInputDraft);
  const verifiedMode = useChatStore((s) => s.verifiedMode);
  const setVerifiedMode = useChatStore((s) => s.setVerifiedMode);
  const session = useChatStore((s) => s.getCurrentSession());
  const taRef = useRef<HTMLTextAreaElement>(null);
  const historyIndexRef = useRef(-1);
  const draftBeforeHistoryRef = useRef('');
  const repoLabel = selectedRepoName ?? selectedRepo;
  const promptHistory = useMemo(() => {
    const seen = new Set<string>();
    return [...(session?.messages ?? [])]
      .reverse()
      .filter((message) => message.role === 'user')
      .map((message) => message.content.trim())
      .filter((content) => {
        if (!content || seen.has(content)) return false;
        seen.add(content);
        return true;
      });
  }, [session?.messages]);

  useEffect(() => {
    const ta = taRef.current;
    if (!ta) return;
    ta.style.height = 'auto';
    ta.style.height = `${Math.min(MAX_HEIGHT, Math.max(MIN_HEIGHT, ta.scrollHeight))}px`;
  }, [value]);

  const submit = () => {
    if (!value.trim() || isStreaming) return;
    historyIndexRef.current = -1;
    draftBeforeHistoryRef.current = '';
    void sendMessage(value);
    setValue('');
    // Re-focus textarea après envoi (productivité clavier).
    requestAnimationFrame(() => taRef.current?.focus());
  };

  const focusTextareaAtEnd = () => {
    requestAnimationFrame(() => {
      const ta = taRef.current;
      if (!ta) return;
      ta.focus();
      const end = ta.value.length;
      ta.setSelectionRange(end, end);
    });
  };

  const restoreHistoryEntry = (nextIndex: number) => {
    historyIndexRef.current = nextIndex;
    setValue(promptHistory[nextIndex] ?? '');
    focusTextareaAtEnd();
  };

  const resetHistoryNavigation = (nextValue: string) => {
    historyIndexRef.current = -1;
    draftBeforeHistoryRef.current = nextValue;
    setValue(nextValue);
  };

  const reformulateDraft = () => {
    const rewritten = reformulateChatPrompt(value, repoLabel);
    if (!rewritten || rewritten === value.trim()) return;
    resetHistoryNavigation(rewritten);
    focusTextareaAtEnd();
  };

  const onKeyDown = (e: KeyboardEvent<HTMLTextAreaElement>) => {
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault();
      submit();
      return;
    }

    if (e.key === 'ArrowUp' && promptHistory.length > 0) {
      const browsingHistory = historyIndexRef.current >= 0;
      if (value.trim() === '' || browsingHistory) {
        e.preventDefault();
        if (!browsingHistory) draftBeforeHistoryRef.current = value;
        restoreHistoryEntry(Math.min(historyIndexRef.current + 1, promptHistory.length - 1));
      }
      return;
    }

    if (e.key === 'ArrowDown' && historyIndexRef.current >= 0) {
      e.preventDefault();
      const nextIndex = historyIndexRef.current - 1;
      if (nextIndex >= 0) {
        restoreHistoryEntry(nextIndex);
      } else {
        historyIndexRef.current = -1;
        setValue(draftBeforeHistoryRef.current);
      }
    }
  };

  return (
    <div className="composer-panel border-t p-4">
      <div
        className={clsx(
          'w-full px-0 sm:px-1',
          wide ? 'max-w-none lg:px-6 2xl:px-8' : 'mx-auto max-w-[86rem] lg:px-3'
        )}
      >
        <div className="mb-2 flex items-center justify-between gap-3 text-xs text-neutral-500">
          <span className="inline-flex min-w-0 items-center gap-1.5 truncate">
            <Database size={12} aria-hidden="true" />
            <span className="truncate">{repoLabel ?? 'Aucun projet sélectionné'}</span>
          </span>
          <div className="flex shrink-0 items-center gap-2">
            <button
              type="button"
              onClick={() => setVerifiedMode(!verifiedMode)}
              aria-pressed={verifiedMode}
              className={clsx(
                'control-button flex items-center gap-1 rounded-md border px-2 py-1 text-[11px]',
                verifiedMode && 'control-button-active'
              )}
              title="Forcer une réponse fondée uniquement sur des fichiers lus et des sources validées"
            >
              <ShieldCheck size={12} aria-hidden="true" />
              <span>Vérifié</span>
            </button>
            <span className={selectedRepo ? 'text-emerald-400' : 'text-neutral-600'}>
              {selectedRepo ? 'Prêt' : 'Projet requis'}
            </span>
          </div>
        </div>
        <div className="composer-shell flex items-end gap-2 rounded-lg border p-2">
          <textarea
            ref={taRef}
            value={value}
            onChange={(e) => resetHistoryNavigation(e.target.value)}
            onKeyDown={onKeyDown}
            placeholder={
              selectedRepo
                ? `Pose ta question sur ${repoLabel}…`
                : 'Sélectionne un projet en haut à droite avant de poser ta question…'
            }
            aria-label="Message à envoyer au chat"
            aria-busy={isStreaming}
            className="max-h-[200px] flex-1 resize-none bg-transparent px-2 py-2 text-sm leading-6 text-[var(--text-primary)] outline-none placeholder:text-[var(--text-faint)] disabled:cursor-not-allowed disabled:text-[var(--text-faint)]"
            style={{ minHeight: MIN_HEIGHT }}
            disabled={isStreaming || !selectedRepo}
          />
          <button
            type="button"
            onClick={reformulateDraft}
            disabled={!value.trim() || isStreaming || !selectedRepo}
            aria-label="Reformuler le prompt"
            className="control-button flex h-10 w-10 items-center justify-center rounded-md border transition disabled:cursor-not-allowed"
            title="Reformuler pour obtenir une réponse plus précise et sourcée"
          >
            <Sparkles size={15} aria-hidden="true" />
          </button>
          {isStreaming ? (
            <button
              type="button"
              onClick={cancel}
              aria-label="Annuler la requête en cours"
              className="danger-action flex h-10 w-10 items-center justify-center rounded-md border text-white transition"
              title="Annuler"
            >
              <Square size={14} fill="currentColor" aria-hidden="true" />
            </button>
          ) : (
            <button
              type="button"
              onClick={submit}
              disabled={!value.trim() || !selectedRepo}
              aria-label="Envoyer le message"
              className="send-action flex h-10 w-10 items-center justify-center rounded-md border text-white transition disabled:cursor-not-allowed"
              title="Envoyer (Entrée)"
            >
              <Send size={16} aria-hidden="true" />
            </button>
          )}
        </div>
      </div>
    </div>
  );
}

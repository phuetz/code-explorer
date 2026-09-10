import { type UIEvent, useCallback, useEffect, useRef, useState } from 'react';
import {
  ArrowDown,
  BarChart3,
  GitBranch,
  Network,
  Plug,
  MessageSquareText,
} from 'lucide-react';
import clsx from 'clsx';
import { useChatStore } from '../../stores/chat-store';
import { useChat } from '../../hooks/use-chat';
import { ChatMessage } from './ChatMessage';
import type { LlmConfigInfo } from '../../api/mcp-client';
import type { SourceReference, SourceValidationState } from '../../utils/source-references';

const SUGGESTIONS = [
  {
    icon: GitBranch,
    label: 'Flux Mermaid',
    prompt:
      'Trace un flux métier important avec un diagramme Mermaid flowchart TD, puis détaille les étapes, les fichiers et les méthodes sources.',
  },
  {
    icon: GitBranch,
    label: 'Séquence Mermaid',
    prompt:
      'Génère un diagramme Mermaid sequenceDiagram pour un flux applicatif représentatif, avec les couches MVC, services, règles, persistence et les appels clés.',
  },
  {
    icon: Network,
    label: 'Classes Mermaid',
    prompt:
      'Génère un diagramme Mermaid classDiagram des classes principales d’un module important, puis explique les responsabilités et dépendances.',
  },
  {
    icon: Network,
    label: 'Architecture',
    prompt: 'Donne-moi une vue d’ensemble de l’architecture du projet en 5 points clés, avec les frontières entre couches et modules.',
  },
  {
    icon: Plug,
    label: 'Endpoints',
    prompt: 'Liste les endpoints HTTP exposés par ce projet, leurs handlers, services appelés et les risques d’intégration.',
  },
  {
    icon: BarChart3,
    label: 'Risques',
    prompt:
      'Identifie les zones les plus risquées. Si l’historique Git est disponible, utilise les hotspots; sinon base-toi sur le graphe d’appels, les dépendances et le code mort.',
  },
];

interface Props {
  llm?: LlmConfigInfo | null;
  wide?: boolean;
  onOpenSourceReference?: (reference: SourceReference) => void;
  validSourcePaths?: ReadonlySet<string> | null;
  sourceValidationState?: SourceValidationState;
}

export function ChatMessages({
  llm = null,
  wide = false,
  onOpenSourceReference,
  validSourcePaths = null,
  sourceValidationState,
}: Props) {
  const session = useChatStore((s) => s.getCurrentSession());
  const isStreaming = useChatStore((s) => s.isStreaming);
  const selectedRepo = useChatStore((s) => s.selectedRepo);
  const selectedRepoName = useChatStore((s) => s.selectedRepoName);
  const setInputDraft = useChatStore((s) => s.setInputDraft);
  const { regenerate } = useChat(llm);
  const scrollRef = useRef<HTMLDivElement>(null);
  const [isNearBottom, setIsNearBottom] = useState(true);
  const repoLabel = selectedRepoName ?? selectedRepo;

  const scrollToBottom = useCallback((behavior: ScrollBehavior = 'smooth') => {
    const container = scrollRef.current;
    if (!container) return;
    if (typeof container.scrollTo === 'function') {
      container.scrollTo({ top: container.scrollHeight, behavior });
    } else {
      container.scrollTop = container.scrollHeight;
    }
  }, []);

  const updateNearBottom = useCallback((container: HTMLDivElement) => {
    const distance = container.scrollHeight - container.scrollTop - container.clientHeight;
    setIsNearBottom(distance < 140);
  }, []);

  const handleScroll = useCallback(
    (event: UIEvent<HTMLDivElement>) => {
      updateNearBottom(event.currentTarget);
    },
    [updateNearBottom]
  );

  useEffect(() => {
    const frame = window.requestAnimationFrame(() => {
      scrollToBottom('auto');
      setIsNearBottom(true);
    });
    return () => window.cancelAnimationFrame(frame);
  }, [session?.id, scrollToBottom]);

  useEffect(() => {
    if (isNearBottom) {
      scrollToBottom('smooth');
    }
  }, [session?.messages.length, isStreaming, isNearBottom, scrollToBottom]);

  if (!session || session.messages.length === 0) {
    return (
      <div className="welcome-panel flex h-full items-center justify-center px-6 py-8">
        <div className="w-full max-w-5xl">
          <div className="mb-7 flex items-center gap-4">
            <div className="brand-tile flex h-12 w-12 shrink-0 items-center justify-center rounded-lg border">
              <MessageSquareText size={22} aria-hidden="true" />
            </div>
            <div className="min-w-0 text-left">
              <h2 className="text-xl font-medium text-[var(--text-primary)]">Code Explorer Chat</h2>
              <p className="mt-1 text-sm text-neutral-500">
                {selectedRepo
                  ? `Contexte actif : ${repoLabel}`
                  : 'Aucun projet sélectionné'}
              </p>
            </div>
          </div>
          <p className="mb-3 text-sm text-purple-300">
            Code Explorer donne à l'IA tout ton code sous forme de graphe — des réponses
            précises pour une fraction du contexte habituel.
          </p>
          <p className="mb-4 text-sm text-neutral-500">
            {selectedRepo
              ? `Pose une question sur ${repoLabel} ou choisis une suggestion :`
              : 'Sélectionne un projet en haut à droite, puis pose ta question.'}
          </p>
          {selectedRepo && (
            <div className="grid gap-2 sm:grid-cols-2">
              {SUGGESTIONS.map(({ icon: Icon, label, prompt }) => (
                <button
                  key={label}
                  type="button"
                  onClick={() => setInputDraft(prompt)}
                  aria-label={`Suggestion : ${prompt}`}
                  className="suggestion-card group flex min-h-24 items-start gap-3 rounded-lg border p-4 text-left transition"
                >
                  <Icon size={18} className="mt-0.5 shrink-0 text-purple-400" aria-hidden="true" />
                  <div className="min-w-0">
                    <div className="text-sm font-medium text-neutral-200">{label}</div>
                    <div className="mt-1 text-xs text-neutral-500 line-clamp-2 group-hover:text-neutral-400">
                      {prompt}
                    </div>
                  </div>
                </button>
              ))}
            </div>
          )}
        </div>
      </div>
    );
  }

  return (
    <div
      ref={scrollRef}
      className="relative h-full overflow-y-auto"
      role="log"
      aria-live="polite"
      aria-relevant="additions text"
      onScroll={handleScroll}
    >
      <div
        id="code-explorer-chat-export-source"
        className={clsx(
          'flex w-full flex-col gap-4 py-5',
          wide
            ? 'px-4 sm:px-5 lg:px-6 2xl:px-8'
            : 'mx-auto max-w-[86rem] px-4 sm:px-5 lg:px-7'
        )}
      >
        {session.messages.map((m, i) => (
          <ChatMessage
            key={m.id}
            message={m}
            llm={m.llm ?? inferLegacyMessageLlm(m.content) ?? llm}
            onRegenerate={regenerate}
            onOpenSourceReference={onOpenSourceReference}
            repo={selectedRepo}
            repoName={selectedRepoName}
            validSourcePaths={validSourcePaths}
            sourceValidationState={sourceValidationState}
            canRegenerate={
              m.role === 'assistant' &&
              i === session.messages.length - 1 &&
              !isStreaming
            }
          />
        ))}
        {isStreaming && (
          <div className="streaming-card flex gap-3 rounded-lg border px-4 py-4">
            <div className="flex h-8 w-8 shrink-0 items-center justify-center rounded-md bg-emerald-600/15 text-emerald-300">
              <div className="h-2 w-2 animate-pulse rounded-full bg-current" />
            </div>
            <div className="text-sm text-neutral-500">Code Explorer réfléchit…</div>
          </div>
        )}
      </div>
      {!isNearBottom && (
        <button
          type="button"
          onClick={() => {
            scrollToBottom('smooth');
            setIsNearBottom(true);
          }}
          className="control-button sticky bottom-4 z-10 ml-auto mr-6 mb-4 flex h-9 w-9 items-center justify-center rounded-full border shadow-lg transition"
          aria-label="Aller au dernier message"
          title="Aller au dernier message"
        >
          <ArrowDown size={16} aria-hidden />
        </button>
      )}
    </div>
  );
}

function inferLegacyMessageLlm(content: string): LlmConfigInfo | null {
  const lower = content.toLowerCase();
  if (lower.includes('localhost:11434') || lower.includes('127.0.0.1:11434')) {
    return {
      configured: true,
      provider: 'ollama',
      model: 'ancien message local',
      reasoningEffort: 'none',
    };
  }
  if (lower.includes('localhost:1234') || lower.includes('127.0.0.1:1234')) {
    return {
      configured: true,
      provider: 'lm-studio',
      model: 'ancien message local',
      reasoningEffort: 'none',
    };
  }
  return null;
}

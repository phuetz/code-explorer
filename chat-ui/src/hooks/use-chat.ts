import { useCallback, useRef } from 'react';
import { useChatStore } from '../stores/chat-store';
import {
  mcpClient,
  ChatStreamError,
  type LlmConfigInfo,
  type ChatHistoryMessage,
  type ToolCallStreamEvent,
} from '../api/mcp-client';
import type { Message } from '../types/chat';
import { EMPTY_ASSISTANT_RESPONSE_MESSAGE } from '../utils/empty-assistant-response';

const newId = () => crypto.randomUUID();
const DEFAULT_SESSION_TITLE = 'Nouvelle conversation';

function titleFromMessage(content: string): string {
  return content.replace(/\s+/g, ' ').trim().slice(0, 80) || DEFAULT_SESSION_TITLE;
}

function snapshotLlm(llm: LlmConfigInfo | null | undefined): LlmConfigInfo | null {
  if (!llm?.configured) return null;
  return {
    configured: true,
    provider: llm.provider,
    model: llm.model,
    reasoningEffort: llm.reasoningEffort,
    maxTokens: llm.maxTokens,
    bigContextModel: llm.bigContextModel,
  };
}

export interface SendMessageResult {
  ok: boolean;
  sessionId: string;
  userMessageId?: string;
  assistantMessageId: string;
  content: string;
}

export interface SendMessageOptions {
  sessionId?: string | null;
  title?: string;
  includeHistory?: boolean;
  repository?: string | null;
}

export function useChat(activeLlm: LlmConfigInfo | null = null) {
  const createSession = useChatStore((s) => s.createSession);
  const renameSession = useChatStore((s) => s.renameSession);
  const appendMessage = useChatStore((s) => s.appendMessage);
  const updateMessage = useChatStore((s) => s.updateMessage);
  const upsertToolCall = useChatStore((s) => s.upsertToolCall);
  const setStreaming = useChatStore((s) => s.setStreaming);
  const isStreaming = useChatStore((s) => s.isStreaming);
  const getCurrentSession = useChatStore((s) => s.getCurrentSession);

  const abortRef = useRef<AbortController | null>(null);

  const cancel = useCallback(() => {
    abortRef.current?.abort();
    abortRef.current = null;
    setStreaming(false);
  }, [setStreaming]);

  const removeMessagesFrom = useChatStore((s) => s.removeMessagesFrom);

  const sendMessage = useCallback(
    async (
      content: string,
      options: SendMessageOptions = {}
    ): Promise<SendMessageResult | null> => {
      const trimmed = content.trim();
      const liveState = useChatStore.getState();
      if (!trimmed || liveState.isStreaming) return null;

      const repository = options.repository ?? liveState.selectedRepo;
      const useVerifiedMode = liveState.verifiedMode;
      let sessionId = options.sessionId ?? liveState.currentSessionId;

      if (!repository) {
        const errMsg: Message = {
          id: newId(),
          role: 'assistant',
          content:
            '_Aucun projet sélectionné._ Choisis un repo dans la barre du haut avant de poser ta question.',
          createdAt: Date.now(),
        };
        if (!sessionId) sessionId = createSession(DEFAULT_SESSION_TITLE);
        appendMessage(sessionId, errMsg);
        return {
          ok: false,
          sessionId,
          assistantMessageId: errMsg.id,
          content: errMsg.content,
        };
      }

      const sessionTitle = options.title ?? titleFromMessage(trimmed);
      if (!sessionId) sessionId = createSession(sessionTitle);

      const previous = useChatStore
        .getState()
        .sessions.find((session) => session.id === sessionId);
      if (
        previous &&
        previous.id === sessionId &&
        previous.messages.length === 0 &&
        previous.title === DEFAULT_SESSION_TITLE
      ) {
        renameSession(sessionId, sessionTitle);
      }
      const history: ChatHistoryMessage[] =
        options.includeHistory === false
          ? []
          : (previous?.messages ?? [])
              .filter((m) => m.role === 'user' || m.role === 'assistant')
              .map((m) => ({ role: m.role as 'user' | 'assistant', content: m.content }));

      const userMessage: Message = {
        id: newId(),
        role: 'user',
        content: trimmed,
        createdAt: Date.now(),
      };
      appendMessage(sessionId, userMessage);

      const assistantId = newId();
      const assistantMessage: Message = {
        id: assistantId,
        role: 'assistant',
        content: '',
        createdAt: Date.now(),
        llm: snapshotLlm(activeLlm),
        ...(useVerifiedMode ? { verified: true } : {}),
      };
      appendMessage(sessionId, assistantMessage);

      const ctrl = new AbortController();
      abortRef.current = ctrl;
      setStreaming(true);

      let acc = '';
      let ok = true;
      let finalContent: string | undefined;
      const onToolCall = (event: ToolCallStreamEvent) => {
        if (event.phase === 'start') {
          let parsedArgs: Record<string, unknown>;
          try {
            parsedArgs = JSON.parse(event.args) as Record<string, unknown>;
          } catch {
            parsedArgs = { raw: event.args };
          }
          upsertToolCall(sessionId!, assistantId, {
            id: event.id,
            name: event.name,
            args: parsedArgs,
            status: 'running',
          });
        } else {
          upsertToolCall(sessionId!, assistantId, {
            id: event.id,
            name: event.name,
            args: {},
            status: event.success ? 'done' : 'error',
          });
        }
      };
      try {
        await mcpClient.chatStream(
          repository,
          useVerifiedMode ? verifiedQuestion(trimmed) : trimmed,
          history,
          (delta) => {
            acc += delta;
            updateMessage(sessionId!, assistantId, acc);
          },
          ctrl.signal,
          onToolCall
        );
        if (!acc) {
          updateMessage(sessionId, assistantId, EMPTY_ASSISTANT_RESPONSE_MESSAGE);
          finalContent = EMPTY_ASSISTANT_RESPONSE_MESSAGE;
        } else {
          finalContent = acc;
        }
      } catch (err) {
        ok = false;
        const aborted = err instanceof DOMException && err.name === 'AbortError';
        const isStreamErr = err instanceof ChatStreamError;
        const msg = err instanceof Error ? err.message : String(err);
        const reason = aborted
          ? '> ⚠️ _Requête annulée._'
          : isStreamErr
            ? `> ❌ **Erreur serveur** : ${msg}`
            : `> ❌ **Erreur** : ${msg}\n>\n> Vérifie le backend avec \`.\\code-explorer.cmd doctor\`, puis relance le chat avec \`.\\code-explorer.cmd chat -RestartBackend\` si le port/proxy est bloqué.`;
        finalContent = acc ? `${acc}\n\n${reason}` : reason;
        updateMessage(sessionId, assistantId, finalContent);
      } finally {
        abortRef.current = null;
        setStreaming(false);
      }
      return {
        ok,
        sessionId,
        userMessageId: userMessage.id,
        assistantMessageId: assistantId,
        content: finalContent || acc || EMPTY_ASSISTANT_RESPONSE_MESSAGE,
      };
    },
    [
      appendMessage,
      createSession,
      renameSession,
      setStreaming,
      updateMessage,
      upsertToolCall,
      activeLlm,
    ]
  );

  /**
   * Drop the assistant message with `assistantMessageId` (and anything after
   * it), then re-fire the user message that prompted it. Conversation history
   * up to that point is preserved.
   */
  const regenerate = useCallback(
    async (assistantMessageId: string) => {
      if (isStreaming) return;
      const session = getCurrentSession();
      if (!session) return;
      const idx = session.messages.findIndex((m) => m.id === assistantMessageId);
      if (idx === -1 || idx === 0) return;
      const previousUser = [...session.messages.slice(0, idx)]
        .reverse()
        .find((m) => m.role === 'user');
      if (!previousUser) return;
      removeMessagesFrom(session.id, previousUser.id);
      await sendMessage(previousUser.content);
    },
    [getCurrentSession, isStreaming, removeMessagesFrom, sendMessage]
  );

  return { sendMessage, regenerate, cancel, isStreaming };
}

function verifiedQuestion(question: string): string {
  return [
    'MODE RÉPONSE VÉRIFIÉE CODE EXPLORER.',
    'Réponds uniquement avec des informations confirmées par les outils Code Explorer et des fichiers réellement lus.',
    "Si un symbole, chemin ou fichier n'est pas retrouvé dans l'index ou n'a pas été lu, dis-le explicitement au lieu de l'inventer.",
    'Cite les fichiers exacts validés par les outils dans la section Sources.',
    '',
    question,
  ].join('\n');
}

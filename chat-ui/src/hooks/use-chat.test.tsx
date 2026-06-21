import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { mcpClient } from '../api/mcp-client';
import type { LlmConfigInfo } from '../api/mcp-client';
import { useChatStore } from '../stores/chat-store';
import { useChat } from './use-chat';

function SendProbe({ llm = null }: { llm?: LlmConfigInfo | null }) {
  const { sendMessage } = useChat(llm);
  return (
    <button type="button" onClick={() => void sendMessage('Trace le flux courrier')}>
      Envoyer
    </button>
  );
}

function OptionsProbe() {
  const { sendMessage } = useChat();
  return (
    <button
      type="button"
      onClick={() =>
        void (async () => {
          const first = await sendMessage('Question 1', {
            title: 'Atelier Questions.docx',
            includeHistory: false,
          });
          if (first) {
            await sendMessage('Question 2', {
              sessionId: first.sessionId,
              includeHistory: false,
            });
          }
        })()
      }
    >
      Traiter document
    </button>
  );
}

function RepositoryOverrideProbe() {
  const { sendMessage } = useChat();
  return (
    <button
      type="button"
      onClick={() =>
        void sendMessage('Question du lot verrouillé', {
          repository: 'repo_locked',
          title: 'Atelier verrouillé',
          includeHistory: false,
        })
      }
    >
      Traiter sur repo verrouillé
    </button>
  );
}

describe('useChat', () => {
  beforeEach(() => {
    localStorage.clear();
    useChatStore.setState({
      sessions: [],
      currentSessionId: null,
      isStreaming: false,
      selectedRepo: 'repo_alise',
      selectedRepoName: 'sample-app',
      inputDraft: '',
      isSfdPanelOpen: false,
      isWorkDocumentsPanelOpen: false,
      isWorkspaceOpen: false,
      workspaceTab: 'sources',
      verifiedMode: false,
      workDocuments: [],
      currentWorkDocumentId: null,
    });
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  it('keeps transport errors actionable for the local chat launcher', async () => {
    vi.spyOn(mcpClient, 'chatStream').mockRejectedValue(new Error('backend down'));

    render(<SendProbe />);
    fireEvent.click(screen.getByRole('button', { name: 'Envoyer' }));

    await waitFor(() => {
      const session = useChatStore.getState().sessions[0];
      const assistant = session?.messages.find((message) => message.role === 'assistant');
      expect(assistant?.content).toContain('.\\code-explorer.cmd doctor');
      expect(assistant?.content).toContain('.\\code-explorer.cmd chat -RestartBackend');
      expect(useChatStore.getState().isStreaming).toBe(false);
    });
  });

  it('renames an empty default session from the first user message', async () => {
    useChatStore.setState({
      sessions: [
        {
          id: 's1',
          title: 'Nouvelle conversation',
          createdAt: 1778000000000,
          updatedAt: 1778000000000,
          messages: [],
        },
      ],
      currentSessionId: 's1',
    });
    vi.spyOn(mcpClient, 'chatStream').mockImplementation(
      async (_repo, _question, _history, onDelta) => {
        onDelta('Réponse OK');
      }
    );

    render(<SendProbe />);
    fireEvent.click(screen.getByRole('button', { name: 'Envoyer' }));

    await waitFor(() => {
      const session = useChatStore.getState().sessions[0];
      expect(session.title).toBe('Trace le flux courrier');
      expect(session.messages.map((message) => message.role)).toEqual(['user', 'assistant']);
    });
  });

  it('stores the active LLM snapshot on new assistant messages', async () => {
    vi.spyOn(mcpClient, 'chatStream').mockImplementation(
      async (_repo, _question, _history, onDelta) => {
        onDelta('Réponse OK');
      }
    );

    render(
      <SendProbe
        llm={{
          configured: true,
          provider: 'lm-studio',
          model: 'mistralai/ministral-3-3b',
          reasoningEffort: 'none',
          maxTokens: 2048,
        }}
      />
    );
    fireEvent.click(screen.getByRole('button', { name: 'Envoyer' }));

    await waitFor(() => {
      const session = useChatStore.getState().sessions[0];
      const assistant = session?.messages.find((message) => message.role === 'assistant');
      expect(assistant?.llm).toMatchObject({
        configured: true,
        provider: 'lm-studio',
        model: 'mistralai/ministral-3-3b',
        reasoningEffort: 'none',
        maxTokens: 2048,
      });
    });
  });

  it('adds grounding instructions to the backend request in verified mode', async () => {
    const chatStream = vi.spyOn(mcpClient, 'chatStream').mockImplementation(
      async (_repo, _question, _history, onDelta) => {
        onDelta('Réponse vérifiée');
      }
    );
    useChatStore.setState({ verifiedMode: true });

    render(<SendProbe />);
    fireEvent.click(screen.getByRole('button', { name: 'Envoyer' }));

    await waitFor(() => {
      expect(chatStream).toHaveBeenCalled();
    });
    expect(chatStream.mock.calls[0][1]).toContain('MODE RÉPONSE VÉRIFIÉE CODE EXPLORER');
    expect(chatStream.mock.calls[0][1]).toContain('Trace le flux courrier');
    expect(useChatStore.getState().sessions[0].messages[0].content).toBe('Trace le flux courrier');
    expect(useChatStore.getState().sessions[0].messages[1].verified).toBe(true);
  });

  it('can pin several generated questions to one session without replaying chat history', async () => {
    const chatStream = vi.spyOn(mcpClient, 'chatStream').mockImplementation(
      async (_repo, question, history, onDelta) => {
        expect(history).toEqual([]);
        onDelta(`Réponse à ${question}`);
      }
    );

    render(<OptionsProbe />);
    fireEvent.click(screen.getByRole('button', { name: 'Traiter document' }));

    await waitFor(() => {
      expect(chatStream).toHaveBeenCalledTimes(2);
      const sessions = useChatStore.getState().sessions;
      expect(sessions).toHaveLength(1);
      expect(sessions[0].title).toBe('Atelier Questions.docx');
      expect(sessions[0].messages.map((message) => message.role)).toEqual([
        'user',
        'assistant',
        'user',
        'assistant',
      ]);
    });
  });

  it('uses an explicit repository override for workshop-generated questions', async () => {
    const chatStream = vi.spyOn(mcpClient, 'chatStream').mockImplementation(
      async (_repo, _question, _history, onDelta) => {
        onDelta('Réponse atelier');
      }
    );
    useChatStore.setState({
      selectedRepo: null,
      selectedRepoName: null,
    });

    render(<RepositoryOverrideProbe />);
    fireEvent.click(screen.getByRole('button', { name: 'Traiter sur repo verrouillé' }));

    await waitFor(() => expect(chatStream).toHaveBeenCalledTimes(1));
    expect(chatStream.mock.calls[0][0]).toBe('repo_locked');
    expect(useChatStore.getState().sessions[0].title).toBe('Atelier verrouillé');
    expect(useChatStore.getState().sessions[0].messages[1].content).toBe('Réponse atelier');
  });
});

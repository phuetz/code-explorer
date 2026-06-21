import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { useChatStore } from '../../stores/chat-store';
import { ChatMessages } from './ChatMessages';

describe('ChatMessages', () => {
  afterEach(() => {
    vi.restoreAllMocks();
    vi.unstubAllGlobals();
  });

  beforeEach(() => {
    localStorage.clear();
    useChatStore.setState({
      sessions: [
        {
          id: 's1',
          title: 'Flux courrier',
          createdAt: 1774507049000,
          updatedAt: 1774507059000,
          messages: [
            {
              id: 'm1',
              role: 'user',
              content: 'Trace le flux courrier',
              createdAt: 1774507049000,
            },
            {
              id: 'm2',
              role: 'assistant',
              content: 'Réponse longue avec diagramme et sources.',
              createdAt: 1774507059000,
            },
          ],
        },
      ],
      currentSessionId: 's1',
      isStreaming: false,
      selectedRepo: 'repo_alise',
      selectedRepoName: 'sample-app',
      inputDraft: '',
      isSfdPanelOpen: false,
    });
  });

  it('lets readers jump back to the latest message after scrolling away', () => {
    render(<ChatMessages />);

    const log = screen.getByRole('log');
    const scrollTo = vi.fn();
    Object.defineProperties(log, {
      scrollHeight: { configurable: true, value: 1000 },
      clientHeight: { configurable: true, value: 300 },
      scrollTop: { configurable: true, writable: true, value: 100 },
      scrollTo: { configurable: true, value: scrollTo },
    });

    fireEvent.scroll(log);
    fireEvent.click(screen.getByRole('button', { name: /aller au dernier message/i }));

    expect(scrollTo).toHaveBeenCalledWith({ top: 1000, behavior: 'smooth' });
  });

  it('shows the active LLM on assistant messages', () => {
    render(
      <ChatMessages
        llm={{
          configured: true,
          provider: 'chatgpt',
          model: 'gpt-5.5',
          reasoningEffort: 'high',
        }}
      />
    );

    expect(screen.getByText('chatgpt · gpt-5.5 · high')).toBeTruthy();
  });

  it('makes an empty assistant response visible and regenerable', () => {
    useChatStore.setState({
      sessions: [
        {
          id: 's1',
          title: 'Réponse interrompue',
          createdAt: 1774507049000,
          updatedAt: 1774507059000,
          messages: [
            {
              id: 'm1',
              role: 'user',
              content: 'Explique StackLogger',
              createdAt: 1774507049000,
            },
            {
              id: 'm2',
              role: 'assistant',
              content: '',
              createdAt: 1774507059000,
            },
          ],
        },
      ],
      currentSessionId: 's1',
      isStreaming: false,
    });

    render(<ChatMessages />);

    expect(screen.getByText(/Réponse vide ou interrompue/i)).toBeTruthy();
    expect(screen.getByRole('button', { name: /régénérer la réponse/i })).toBeTruthy();
  });

  it('shows verified and evidence badges on grounded assistant messages', () => {
    useChatStore.setState({
      sessions: [
        {
          id: 's1',
          title: 'Réponse vérifiée',
          createdAt: 1774507049000,
          updatedAt: 1774507059000,
          messages: [
            {
              id: 'm1',
              role: 'assistant',
              content: 'Source: Controllers/CourrierController.cs:42',
              createdAt: 1774507059000,
              verified: true,
              toolCalls: [
                { id: 't1', name: 'search_code', args: {}, status: 'done' },
                { id: 't2', name: 'read_file', args: {}, status: 'done' },
              ],
            },
          ],
        },
      ],
      currentSessionId: 's1',
    });

    render(
      <ChatMessages
        validSourcePaths={new Set(['controllers/courriercontroller.cs'])}
        sourceValidationState="ready"
      />
    );

    expect(screen.getByText('Vérifié')).toBeTruthy();
    expect(screen.getByLabelText('Preuves de la réponse')).toBeTruthy();
    expect(screen.getByText('2 outils OK')).toBeTruthy();
    expect(screen.getByText('1/1 source validée')).toBeTruthy();
  });

  it('prefers the assistant message LLM snapshot over the active LLM', () => {
    useChatStore.setState({
      sessions: [
        {
          id: 's1',
          title: 'Provider historique',
          createdAt: 1774507049000,
          updatedAt: 1774507059000,
          messages: [
            {
              id: 'm1',
              role: 'assistant',
              content: 'Réponse créée avec Ollama.',
              createdAt: 1774507059000,
              llm: {
                configured: true,
                provider: 'ollama',
                model: 'mistral-local',
                reasoningEffort: 'none',
              },
            },
          ],
        },
      ],
      currentSessionId: 's1',
    });

    render(
      <ChatMessages
        llm={{
          configured: true,
          provider: 'chatgpt',
          model: 'gpt-5.5',
          reasoningEffort: 'high',
        }}
      />
    );

    expect(screen.getByText('ollama · mistral-local · none')).toBeTruthy();
  });

  it('labels legacy Ollama error messages without reusing the active LLM badge', () => {
    useChatStore.setState({
      sessions: [
        {
          id: 's1',
          title: 'Erreur locale',
          createdAt: 1774507049000,
          updatedAt: 1774507059000,
          messages: [
            {
              id: 'm1',
              role: 'assistant',
              content:
                '> ❌ **Erreur serveur** : error sending request for url (http://localhost:11434/v1/chat/completions)',
              createdAt: 1774507059000,
            },
          ],
        },
      ],
      currentSessionId: 's1',
    });

    render(
      <ChatMessages
        llm={{
          configured: true,
          provider: 'chatgpt',
          model: 'gpt-5.5',
          reasoningEffort: 'high',
        }}
      />
    );

    expect(screen.getByText('ollama · ancien message local · none')).toBeTruthy();
  });

  it('offers Mermaid sequence and class prompts from the empty chat state', () => {
    useChatStore.setState({
      sessions: [],
      currentSessionId: null,
      selectedRepo: 'repo_alise',
      selectedRepoName: 'sample-app',
      inputDraft: '',
    });

    render(<ChatMessages />);

    fireEvent.click(screen.getByRole('button', { name: /sequenceDiagram/i }));
    expect(useChatStore.getState().inputDraft).toContain('sequenceDiagram');

    fireEvent.click(screen.getByRole('button', { name: /classDiagram/i }));
    expect(useChatStore.getState().inputDraft).toContain('classDiagram');
  });

  it('lists related files, opens them, and exports their contents', async () => {
    useChatStore.setState({
      sessions: [
        {
          id: 's1',
          title: 'Flux courrier',
          createdAt: 1774507049000,
          updatedAt: 1774507059000,
          messages: [
            {
              id: 'm1',
              role: 'user',
              content: 'Quels fichiers ?',
              createdAt: 1774507049000,
            },
            {
              id: 'm2',
              role: 'assistant',
              content: 'Sources\n- `Controllers/CourrierController.cs:42`\n- BAL/CourriersService.cs:5-8',
              createdAt: 1774507059000,
            },
          ],
        },
      ],
      currentSessionId: 's1',
      selectedRepo: 'repo_alise',
      selectedRepoName: 'sample-app',
      isStreaming: false,
    });
    vi.stubGlobal(
      'fetch',
      vi.fn(async (input: RequestInfo | URL) => {
        const url = String(input);
        const path = new URL(url, 'http://localhost').searchParams.get('path') ?? '';
        return new Response(
          JSON.stringify({
            path,
            content: `// contenu ${path}`,
            language: 'csharp',
            totalLines: 1,
            startLine: 1,
            endLine: 1,
            truncated: false,
          }),
          { status: 200, headers: { 'content-type': 'application/json' } }
        );
      })
    );
    const blobs: Blob[] = [];
    Object.defineProperty(URL, 'createObjectURL', {
      configurable: true,
      value: vi.fn((blob: Blob) => {
        blobs.push(blob);
        return 'blob:related-sources';
      }),
    });
    Object.defineProperty(URL, 'revokeObjectURL', {
      configurable: true,
      value: vi.fn(),
    });
    const click = vi.spyOn(HTMLAnchorElement.prototype, 'click').mockImplementation(() => undefined);
    const onOpen = vi.fn();

    render(<ChatMessages onOpenSourceReference={onOpen} />);

    expect(screen.getByText('Fichiers concernés')).toBeTruthy();
    fireEvent.click(screen.getByText('Controllers/CourrierController.cs'));
    expect(onOpen).toHaveBeenCalledWith({
      path: 'Controllers/CourrierController.cs',
      startLine: 42,
      endLine: 42,
    });

    fireEvent.click(screen.getByRole('button', { name: /exporter les fichiers concernés/i }));

    await waitFor(() => expect(click).toHaveBeenCalledTimes(1));
    const exported = await blobs[0].text();
    expect(exported).toContain('# Fichiers concernés');
    expect(exported).toContain('## Controllers/CourrierController.cs');
    expect(exported).toContain('// contenu Controllers/CourrierController.cs');
    expect(exported).toContain('## BAL/CourriersService.cs');
  });

  it('keeps hallucinated source paths visible as warnings but not clickable/exportable', () => {
    useChatStore.setState({
      sessions: [
        {
          id: 's1',
          title: 'Sources hallucinated',
          createdAt: 1774507049000,
          updatedAt: 1774507059000,
          messages: [
            {
              id: 'm1',
              role: 'assistant',
              content: 'Source supposée: src/utils/debug-logger.ts:1',
              createdAt: 1774507059000,
            },
          ],
        },
      ],
      currentSessionId: 's1',
      selectedRepo: 'repo_alise',
      selectedRepoName: 'sample-app',
      isStreaming: false,
    });
    const onOpen = vi.fn();

    render(
      <ChatMessages
        onOpenSourceReference={onOpen}
        validSourcePaths={new Set(['ccas.alise.commundll/commun/stacklogger.cs'])}
      />
    );

    expect(screen.getByText(/1 citation ignorée/i)).toBeTruthy();
    expect(screen.getByText(/Aucun chemin cité n'a été validé/i)).toBeTruthy();
    expect(screen.queryByRole('button', { name: /src\/utils\/debug-logger\.ts/i })).toBeNull();
    expect(screen.getByRole('button', { name: /exporter les fichiers concernés/i }).hasAttribute('disabled')).toBe(true);
  });

  it('holds source actions until repository file validation completes', () => {
    useChatStore.setState({
      sessions: [
        {
          id: 's1',
          title: 'Sources pending',
          createdAt: 1774507049000,
          updatedAt: 1774507059000,
          messages: [
            {
              id: 'm1',
              role: 'assistant',
              content: 'Source candidate: Acme.Sample.CommunDll/Commun/StackLogger.cs:10',
              createdAt: 1774507059000,
            },
          ],
        },
      ],
      currentSessionId: 's1',
      selectedRepo: 'repo_alise',
      selectedRepoName: 'sample-app',
      isStreaming: false,
    });
    const onOpen = vi.fn();

    render(
      <ChatMessages
        onOpenSourceReference={onOpen}
        sourceValidationState="pending"
      />
    );

    expect(screen.getByText(/Validation des chemins cités/i)).toBeTruthy();
    expect(screen.queryByRole('button', { name: /StackLogger\.cs:10/i })).toBeNull();
    expect(screen.getByRole('button', { name: /exporter les fichiers concernés/i }).hasAttribute('disabled')).toBe(true);
  });
});

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
      selectedRepoName: 'Alise_v2',
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

  it('offers Mermaid sequence and class prompts from the empty chat state', () => {
    useChatStore.setState({
      sessions: [],
      currentSessionId: null,
      selectedRepo: 'repo_alise',
      selectedRepoName: 'Alise_v2',
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
      selectedRepoName: 'Alise_v2',
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
});

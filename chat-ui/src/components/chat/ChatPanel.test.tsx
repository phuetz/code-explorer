import { fireEvent, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { ChatPanel } from './ChatPanel';
import { useChatStore } from '../../stores/chat-store';

function jsonResponse(body: unknown, init?: ResponseInit) {
  return new Response(JSON.stringify(body), {
    status: 200,
    headers: { 'content-type': 'application/json' },
    ...init,
  });
}

describe('ChatPanel', () => {
  beforeEach(() => {
    localStorage.clear();
    useChatStore.setState({
      sessions: [],
      currentSessionId: null,
      isStreaming: false,
      selectedRepo: null,
      selectedRepoName: null,
      inputDraft: '',
      isSfdPanelOpen: false,
    });

    vi.stubGlobal(
      'fetch',
      vi.fn(async (input: RequestInfo | URL) => {
        const url = String(input);
        if (url.endsWith('/health')) {
          return jsonResponse({ status: 'ok', service: 'gitnexus', version: 'test' });
        }
        if (url.endsWith('/api/repos')) {
          return jsonResponse({
            repos: [{ id: 'repo-test', name: 'Repo test', path: 'D:/RepoTest' }],
          });
        }
        if (url.endsWith('/api/llm-config')) {
          return jsonResponse({
            configured: true,
            provider: 'chatgpt',
            model: 'gpt-5.5',
            reasoningEffort: 'high',
            maxTokens: 8192,
          });
        }
        if (url.includes('/api/repos/repo-test/files')) {
          return jsonResponse({
            files: [
              {
                name: 'src',
                path: 'src',
                isDir: true,
                children: [
                  {
                    name: 'App.tsx',
                    path: 'src/App.tsx',
                    isDir: false,
                    children: [],
                  },
                ],
              },
            ],
          });
        }
        if (url.includes('/api/repos/repo-test/source')) {
          return jsonResponse({
            path: 'src/App.tsx',
            content: 'const app = true;',
            language: 'tsx',
            totalLines: 30,
            startLine: 12,
            endLine: 12,
            truncated: false,
          });
        }
        if (url.includes('/api/repos/repo-test/symbols')) {
          return jsonResponse({
            symbols: [
              {
                nodeId: 'node-app',
                name: 'App',
                label: 'Component',
                filePath: 'src/App.tsx',
                score: 1,
                startLine: 12,
                endLine: 12,
              },
            ],
          });
        }
        if (url.includes('/api/repos/repo-test/graph/neighborhood')) {
          return jsonResponse({
            nodes: [
              {
                id: 'node-app',
                name: 'App',
                label: 'Component',
                filePath: 'src/App.tsx',
                startLine: 12,
                endLine: 12,
                depth: 0,
              },
            ],
            edges: [],
            stats: { nodeCount: 1, edgeCount: 0, truncated: false },
          });
        }
        return jsonResponse({}, { status: 404 });
      })
    );
  });

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it('renders without entering a recursive store update loop', () => {
    render(<ChatPanel />);

    expect(screen.getAllByText('GitNexus Chat').length).toBeGreaterThan(0);
    expect(screen.getByText('Aucune conversation. Crée-en une pour démarrer.')).toBeTruthy();
  });

  it('surfaces active conversation metadata in the header', () => {
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
    });

    render(<ChatPanel />);

    expect(screen.getAllByText('Flux courrier').length).toBeGreaterThan(0);
    expect(screen.getByText(/2 messages - Dernière activité/i)).toBeTruthy();
  });

  it('opens the Explorer panel from a source reference in an assistant message', async () => {
    useChatStore.setState({
      selectedRepo: 'repo-test',
      selectedRepoName: 'Repo test',
      sessions: [
        {
          id: 's1',
          title: 'Sources',
          createdAt: 1774507049000,
          updatedAt: 1774507059000,
          messages: [
            {
              id: 'm1',
              role: 'assistant',
              content: 'Source principale: src/App.tsx:12',
              createdAt: 1774507059000,
            },
          ],
        },
      ],
      currentSessionId: 's1',
    });

    render(<ChatPanel />);

    expect(screen.getByRole('button', { name: /1 fichier concerné/i })).toBeTruthy();

    fireEvent.click(screen.getByRole('button', { name: /src\/App\.tsx:12/ }));

    expect(await screen.findByText('const app = true;')).toBeTruthy();
    expect(screen.getByText('Explorateur')).toBeTruthy();
  });

  it('opens quick search with Ctrl+K and jumps to a graph symbol', async () => {
    useChatStore.setState({
      selectedRepo: 'repo-test',
      selectedRepoName: 'Repo test',
    });

    render(<ChatPanel />);

    fireEvent.keyDown(window, { key: 'k', ctrlKey: true });
    expect(await screen.findByRole('dialog', { name: /recherche rapide gitnexus/i })).toBeTruthy();

    fireEvent.change(screen.getByPlaceholderText(/chercher un fichier/i), {
      target: { value: 'App' },
    });
    fireEvent.click(await screen.findByRole('button', { name: /ouvrir le symbole app/i }));

    expect(await screen.findByText('Voisinage visuel')).toBeTruthy();
    expect(screen.getAllByText('App').length).toBeGreaterThan(0);
  });
});

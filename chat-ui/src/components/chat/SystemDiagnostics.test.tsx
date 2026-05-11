import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { SystemDiagnostics } from './SystemDiagnostics';
import { useChatStore } from '../../stores/chat-store';

function jsonResponse(body: unknown, init?: ResponseInit) {
  return new Response(JSON.stringify(body), {
    status: 200,
    headers: { 'content-type': 'application/json' },
    ...init,
  });
}

describe('SystemDiagnostics', () => {
  beforeEach(() => {
    localStorage.clear();
    useChatStore.setState({
      selectedRepo: 'repo_alise',
      selectedRepoName: 'Alise_v2',
    });
  });

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it('shows backend, project and LLM diagnostics on demand', async () => {
    const fetchMock = vi.fn(async (input: RequestInfo | URL) => {
      const url = String(input);
      if (url.endsWith('/api/diagnostics')) {
        return jsonResponse({
          service: 'gitnexus',
          version: '0.1.0',
          generatedAtUnixMs: 1778000000000,
          httpAuthRequired: false,
          repoPathsExposed: false,
          repos: {
            count: 1,
            names: [
              {
                id: 'repo_alise',
                name: 'Alise_v2',
                indexedAt: '2026-05-06T05:00:00Z',
                pathExposed: false,
              },
            ],
          },
          llm: {
            configured: true,
            provider: 'chatgpt',
            model: 'gpt-5.5',
            reasoningEffort: 'high',
            maxTokens: 8192,
          },
          auth: {
            chatgptOAuth: {
              loggedIn: true,
              status: 'logged_in',
              tokenFilePresent: true,
              tokenFileReadable: true,
              refreshTokenPresent: true,
              lastRefresh: '2026-05-06T20:00:00Z',
              storage: 'dpapi',
            },
          },
        });
      }
      return jsonResponse({}, { status: 404 });
    });
    vi.stubGlobal('fetch', fetchMock);

    render(<SystemDiagnostics />);
    fireEvent.click(screen.getByRole('button', { name: /ouvrir le diagnostic système/i }));

    await waitFor(() => {
      const dialog = screen.getByRole('dialog', { name: /diagnostic système gitnexus/i });
      expect(dialog.className).toContain('fixed');
      expect(dialog.className).toContain('z-[100]');
      expect(screen.getByText('Diagnostic GitNexus')).toBeTruthy();
      expect(screen.getByText('gitnexus 0.1.0')).toBeTruthy();
      expect(screen.getAllByText('Alise_v2').length).toBeGreaterThanOrEqual(1);
      expect(screen.getByText('chatgpt')).toBeTruthy();
      expect(screen.getByText('gpt-5.5')).toBeTruthy();
      expect(screen.getByText('masqués')).toBeTruthy();
      expect(screen.getByText('connecté')).toBeTruthy();
      expect(screen.getByText('Index local')).toBeTruthy();
      expect(screen.getByText('repo_alise')).toBeTruthy();
    });

    expect(fetchMock).toHaveBeenCalledTimes(1);
  });

  it('warns when the active project is absent from backend diagnostics', async () => {
    useChatStore.setState({
      selectedRepo: 'repo_missing',
      selectedRepoName: 'Projet manquant',
    });

    vi.stubGlobal(
      'fetch',
      vi.fn(async () =>
        jsonResponse({
          service: 'gitnexus',
          version: '0.1.0',
          generatedAtUnixMs: 1778000000000,
          httpAuthRequired: false,
          repoPathsExposed: false,
          repos: {
            count: 1,
            names: [{ id: 'repo_alise', name: 'Alise_v2', pathExposed: false }],
          },
          llm: { configured: false },
        })
      )
    );

    render(<SystemDiagnostics />);
    fireEvent.click(screen.getByRole('button', { name: /ouvrir le diagnostic système/i }));

    await waitFor(() => {
      expect(screen.getByText(/Projet actif absent de \/api\/diagnostics/i)).toBeTruthy();
      expect(screen.getByText(/RestartBackend/)).toBeTruthy();
    });
  });

  it('copies a compact diagnostic report', async () => {
    const writeText = vi.fn<(text: string) => Promise<void>>(() => Promise.resolve());
    Object.defineProperty(navigator, 'clipboard', {
      configurable: true,
      value: { writeText },
    });

    vi.stubGlobal(
      'fetch',
      vi.fn(async () =>
        jsonResponse({
          service: 'gitnexus',
          version: '0.1.0',
          generatedAtUnixMs: 1778000000000,
          httpAuthRequired: false,
          repoPathsExposed: false,
          repos: {
            count: 1,
            names: [
              {
                id: 'repo_alise',
                name: 'Alise_v2',
                indexedAt: '2026-05-06T05:00:00Z',
                pathExposed: false,
              },
            ],
          },
          llm: {
            configured: true,
            provider: 'chatgpt',
            model: 'gpt-5.5',
            reasoningEffort: 'high',
            maxTokens: 8192,
          },
          auth: {
            chatgptOAuth: {
              loggedIn: true,
              status: 'logged_in',
              tokenFilePresent: true,
              tokenFileReadable: true,
              refreshTokenPresent: true,
              lastRefresh: '2026-05-06T20:00:00Z',
              storage: 'dpapi',
            },
          },
        })
      )
    );

    render(<SystemDiagnostics />);
    fireEvent.click(screen.getByRole('button', { name: /ouvrir le diagnostic système/i }));

    await waitFor(() => {
      expect(screen.getByText('gitnexus 0.1.0')).toBeTruthy();
    });

    fireEvent.click(screen.getByRole('button', { name: /copier le rapport de diagnostic/i }));

    expect(writeText).toHaveBeenCalledTimes(1);
    const report = writeText.mock.calls[0][0] as string;
    expect(report).toContain('# Diagnostic GitNexus');
    expect(report).toContain('Projet actif: Alise_v2');
    expect(report).toContain('Provider: chatgpt');
    expect(report).toContain('Modèle: gpt-5.5');
    expect(report).toContain('OAuth ChatGPT: connecté');
    expect(report).toContain('- Alise_v2 (repo_alise)');
  });

  it('keeps backend failures actionable inside the panel', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn(async () => new Response('Bad Gateway', { status: 502, statusText: 'Bad Gateway' }))
    );

    render(<SystemDiagnostics />);
    fireEvent.click(screen.getByRole('button', { name: /ouvrir le diagnostic système/i }));

    await waitFor(() => {
      expect(screen.getByText('Diagnostic indisponible')).toBeTruthy();
      expect(screen.getByText(/HTTP 502 Bad Gateway/)).toBeTruthy();
    });
  });
});

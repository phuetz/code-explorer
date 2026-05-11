import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { WorkspacePanel } from './WorkspacePanel';
import { useChatStore } from '../../stores/chat-store';

function jsonResponse(body: unknown, init?: ResponseInit) {
  return new Response(JSON.stringify(body), {
    status: 200,
    headers: { 'content-type': 'application/json' },
    ...init,
  });
}

describe('WorkspacePanel', () => {
  beforeEach(() => {
    localStorage.clear();
    useChatStore.setState({
      sessions: [],
      currentSessionId: null,
      isStreaming: false,
      selectedRepo: 'repo_alise',
      selectedRepoName: 'Alise_v2',
      inputDraft: '',
      isSfdPanelOpen: false,
    });

    vi.stubGlobal(
      'fetch',
      vi.fn(async (input: RequestInfo | URL) => {
        const url = String(input);
        if (url.includes('/files')) {
          return jsonResponse({
            files: [
              {
                name: 'Controllers',
                path: 'Controllers',
                isDir: true,
                children: [
                  {
                    name: 'CourrierController.cs',
                    path: 'Controllers/CourrierController.cs',
                    isDir: false,
                    children: [],
                  },
                ],
              },
              {
                name: 'Models',
                path: 'Models',
                isDir: true,
                children: [
                  {
                    name: 'Autre.cs',
                    path: 'Models/Autre.cs',
                    isDir: false,
                    children: [],
                  },
                ],
              },
            ],
          });
        }
        if (url.includes('/source')) {
          return jsonResponse({
            path: 'Controllers/CourrierController.cs',
            content: [
              'public class CourrierController {',
              '  public string Title { get; set; }',
              '  public void Index() {}',
              '}',
            ].join('\n'),
            language: 'csharp',
            totalLines: 4,
            startLine: 1,
            endLine: 4,
            truncated: false,
          });
        }
        if (url.includes('/symbols')) {
          return jsonResponse({
            symbols: [
              {
                nodeId: 'node-controller',
                name: 'CourrierController',
                label: 'Controller',
                filePath: 'Controllers/CourrierController.cs',
                score: 1,
                startLine: 10,
                endLine: 42,
              },
            ],
          });
        }
        if (url.includes('/graph/neighborhood')) {
          return jsonResponse({
            nodes: [
              {
                id: 'node-controller',
                name: 'CourrierController',
                label: 'Controller',
                filePath: 'Controllers/CourrierController.cs',
                startLine: 10,
                endLine: 42,
                depth: 0,
                isTraced: true,
              },
              {
                id: 'node-service',
                name: 'CourriersService',
                label: 'Service',
                filePath: 'BAL/CourriersService.cs',
                depth: 1,
              },
            ],
            edges: [
              {
                id: 'edge-1',
                source: 'node-controller',
                target: 'node-service',
                relType: 'Calls',
                confidence: 0.9,
              },
            ],
            stats: { nodeCount: 2, edgeCount: 1, truncated: false },
          });
        }
        return jsonResponse({}, { status: 404 });
      })
    );
  });

  afterEach(() => {
    vi.restoreAllMocks();
    vi.unstubAllGlobals();
  });

  it('highlights related files in the file tree', async () => {
    render(<WorkspacePanel onClose={() => {}} highlightedSourcePaths={['Controllers/CourrierController.cs']} />);

    const fileButton = await screen.findByRole('button', { name: /CourrierController\.cs/i });

    expect(fileButton.className).toContain('file-tree-related');
    expect(fileButton.getAttribute('title')).toContain('fichier concern');
  });

  it('lets folders collapse and expand in the file tree', async () => {
    render(<WorkspacePanel onClose={() => {}} />);

    expect(await screen.findByText('CourrierController.cs')).toBeTruthy();

    fireEvent.click(screen.getByRole('button', { name: 'Controllers' }));
    expect(screen.queryByText('CourrierController.cs')).toBeNull();

    fireEvent.click(screen.getByRole('button', { name: 'Controllers' }));
    expect(await screen.findByText('CourrierController.cs')).toBeTruthy();
  });

  it('shows a related files navigator and exports an analysis pack', async () => {
    const blobs: Blob[] = [];
    Object.defineProperty(URL, 'createObjectURL', {
      configurable: true,
      value: vi.fn((blob: Blob) => {
        blobs.push(blob);
        return 'blob:workspace-related-sources';
      }),
    });
    Object.defineProperty(URL, 'revokeObjectURL', {
      configurable: true,
      value: vi.fn(),
    });
    const click = vi.spyOn(HTMLAnchorElement.prototype, 'click').mockImplementation(() => undefined);

    render(
      <WorkspacePanel
        onClose={() => {}}
        relatedSourceGroups={[
          {
            path: 'Controllers/CourrierController.cs',
            references: [
              {
                path: 'Controllers/CourrierController.cs',
                startLine: 42,
                endLine: 42,
              },
            ],
          },
        ]}
      />
    );

    expect(await screen.findByRole('button', { name: /Controllers - 1 fichier concern/i })).toBeTruthy();
    fireEvent.click(screen.getByText('Controllers/CourrierController.cs'));

    await waitFor(() => {
      expect(screen.getByTestId('source-code-view').textContent).toContain('CourrierController');
    });
    expect(screen.getByText('Fichier concerné')).toBeTruthy();
    expect(screen.getByRole('button', { name: 'L42' })).toBeTruthy();
    expect(screen.getByText('Autre.cs')).toBeTruthy();

    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(navigator, 'clipboard', {
      configurable: true,
      value: { writeText },
    });
    fireEvent.click(screen.getByRole('button', { name: /Copier la liste des fichiers concernés/i }));
    expect(writeText).toHaveBeenCalledWith('Controllers/CourrierController.cs:42');

    fireEvent.click(screen.getByRole('button', { name: /Afficher seulement les fichiers concernés/i }));
    expect(screen.getByText('Arbre filtré sur les fichiers concernés.')).toBeTruthy();
    expect(screen.queryByText('Autre.cs')).toBeNull();

    fireEvent.click(screen.getByRole('button', { name: /Exporter le pack d'analyse/i }));

    await waitFor(() => expect(click).toHaveBeenCalledTimes(1));
    const exported = await blobs[0].text();
    expect(exported).toContain("# Pack d’analyse GitNexus");
    expect(exported).toContain('## Controllers/CourrierController.cs');
    expect(exported).toContain('CourrierController');
  });

  it('saves, reopens, and deletes related-file analysis snapshots', async () => {
    useChatStore.setState({
      sessions: [
        {
          id: 's1',
          title: 'Analyse courriers',
          createdAt: 1000,
          updatedAt: 2000,
          messages: [
            {
              id: 'm1',
              role: 'assistant',
              content: 'Voir Controllers/CourrierController.cs:42 et ```mermaid\ngraph TD\nA-->B\n```',
              createdAt: 2000,
              toolCalls: [{ id: 'tc1', name: 'search_code', args: {}, status: 'done' }],
            },
          ],
        },
      ],
      currentSessionId: 's1',
    });

    render(
      <WorkspacePanel
        onClose={() => {}}
        relatedSourceGroups={[
          {
            path: 'Controllers/CourrierController.cs',
            references: [
              {
                path: 'Controllers/CourrierController.cs',
                startLine: 42,
                endLine: 42,
              },
            ],
          },
        ]}
      />
    );

    fireEvent.click(await screen.findByRole('button', { name: /Sauvegarder l'analyse courante/i }));

    expect(await screen.findByText('1 sauvegarde')).toBeTruthy();
    expect(useChatStore.getState().sessions[0].analyses?.[0]).toMatchObject({
      title: expect.stringContaining('Analyse courriers'),
      repo: 'repo_alise',
      repoName: 'Alise_v2',
      summary: {
        fileCount: 1,
        diagramCount: 1,
        toolCallCount: 1,
        decisionCount: 0,
      },
    });

    fireEvent.click(screen.getByText(/Analyse courriers -/i));
    expect(screen.getByText('Analyse chargée')).toBeTruthy();

    fireEvent.click(screen.getByText('Supprimer'));
    expect(await screen.findByText('0 sauvegarde')).toBeTruthy();
    expect(useChatStore.getState().sessions[0].analyses).toEqual([]);
  });

  it('opens a source file and can send that context back to the chat draft', async () => {
    render(<WorkspacePanel onClose={() => {}} />);

    fireEvent.click(await screen.findByText('CourrierController.cs'));

    await waitFor(() => {
      const sourceView = screen.getByTestId('source-code-view');
      expect(sourceView.textContent).toContain('CourrierController');
    });
    fireEvent.click(screen.getByTitle('Envoyer ce contexte au chat'));

    await waitFor(() => {
      expect(useChatStore.getState().inputDraft).toContain('Controllers/CourrierController.cs');
    });
  });

  it('searches the graph and opens a selected node source', async () => {
    render(<WorkspacePanel onClose={() => {}} />);

    fireEvent.click(screen.getByText('Graphe'));
    fireEvent.change(screen.getByPlaceholderText('Chercher une classe, methode, action...'), {
      target: { value: 'Courrier' },
    });
    fireEvent.click(screen.getByText('Chercher'));

    fireEvent.click(await screen.findByText('CourrierController'));

    expect(await screen.findByText('Voisinage visuel')).toBeTruthy();
    fireEvent.click(screen.getAllByText('Source')[0]);

    await waitFor(() => {
      const sourceView = screen.getByTestId('source-code-view');
      expect(sourceView.textContent).toContain('CourrierController');
    });
  });

  it('searches within a source file and jumps from source to graph', async () => {
    render(<WorkspacePanel onClose={() => {}} />);

    fireEvent.click(await screen.findByText('CourrierController.cs'));

    await waitFor(() => {
      expect(screen.getByTestId('source-code-view').textContent).toContain('CourrierController');
    });

    fireEvent.change(screen.getByPlaceholderText('Rechercher dans ce fichier...'), {
      target: { value: 'Courrier' },
    });

    expect(screen.getByText('1 / 1')).toBeTruthy();
    fireEvent.click(screen.getByRole('button', { name: /ouvrir le fichier dans le graphe/i }));

    expect(await screen.findByText('Voisinage visuel')).toBeTruthy();
  });

  it('offers graph symbols from the opened source file', async () => {
    render(<WorkspacePanel onClose={() => {}} />);

    fireEvent.click(await screen.findByText('CourrierController.cs'));

    expect(await screen.findByText('Symboles graphe')).toBeTruthy();
    fireEvent.click(await screen.findByTitle(/Voir le voisinage graphe de CourrierController/i));

    expect(await screen.findByText('Voisinage visuel')).toBeTruthy();
  });

  it('shows a file outline and jumps to a selected symbol line', async () => {
    render(<WorkspacePanel onClose={() => {}} />);

    fireEvent.click(await screen.findByText('CourrierController.cs'));

    expect(await screen.findByText('Plan')).toBeTruthy();
    const outlineButton = screen.getByRole('button', { name: /Index.*3/ });
    expect(outlineButton).toBeTruthy();

    fireEvent.click(outlineButton);

    await waitFor(() => {
      const activeLine = document.querySelector('[data-source-line="3"].source-line-active');
      expect(activeLine).toBeTruthy();
    });
  });
});

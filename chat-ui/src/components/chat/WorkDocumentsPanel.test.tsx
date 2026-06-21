import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { mcpClient } from '../../api/mcp-client';
import { useChatStore } from '../../stores/chat-store';
import type { WorkDocument } from '../../types/chat';
import { useChat } from '../../hooks/use-chat';
import { downloadBlobFile } from '../../utils/related-sources-export';
import { saveWorkDocumentSource } from '../../utils/workdoc-source-store';
import {
  listWorkDocumentStateSummaries,
  loadWorkDocumentState,
  saveWorkDocumentState,
} from '../../utils/workdoc-state-store';
import { WorkDocumentsPanel } from './WorkDocumentsPanel';

vi.mock('../../hooks/use-chat', () => ({
  useChat: vi.fn(),
}));

vi.mock('../../utils/workdoc-source-store', () => ({
  deleteWorkDocumentSource: vi.fn(async () => undefined),
  loadWorkDocumentSource: vi.fn(async () => null),
  saveWorkDocumentSource: vi.fn(async () => undefined),
}));

vi.mock('../../utils/workdoc-state-store', () => ({
  deleteWorkDocumentState: vi.fn(async () => undefined),
  listWorkDocumentStateSummaries: vi.fn(async () => []),
  loadWorkDocumentState: vi.fn(async () => null),
  saveWorkDocumentState: vi.fn(async () => undefined),
}));

vi.mock('../../utils/related-sources-export', async () => {
  const actual = await vi.importActual<typeof import('../../utils/related-sources-export')>(
    '../../utils/related-sources-export'
  );
  return {
    ...actual,
    downloadBlobFile: vi.fn(),
    downloadTextFile: vi.fn(),
  };
});

vi.mock('../ui/MermaidBlock', () => ({
  MermaidBlock: ({ text }: { text: string }) => (
    <div data-testid="workdoc-mermaid-render">{text}</div>
  ),
}));

function makeDocument(): WorkDocument {
  return {
    id: 'doc-1',
    filename: 'Questions Sample.docx',
    importedAt: 1774500000000,
    repo: 'repo_alise',
    repoName: 'sample-app',
    sourceBytes: 1200,
    markdownChars: 4200,
    sourceMarkdown: '# Questions Sample',
    questions: [
      {
        id: 'q-1',
        order: 1,
        label: 'Q1.1',
        text: 'À quoi sert StackLogger ?',
        context: 'Chapitre journalisation',
        status: 'answered',
        answer: [
          'StackLogger trace les méthodes métier et relie les exécutions au code.',
          '',
          '## Sources',
          '- Acme.Sample/Logging/StackLogger.cs',
        ].join('\n'),
      },
      {
        id: 'q-2',
        order: 2,
        label: 'Q1.2',
        text: 'Pourquoi le plafond est-il recalculé ?',
        context: 'Chapitre calcul',
        status: 'pending',
      },
    ],
  };
}

function makePendingDocument(count: number): WorkDocument {
  return {
    ...makeDocument(),
    questions: Array.from({ length: count }, (_, index) => ({
      id: `q-${index + 1}`,
      order: index + 1,
      label: `Q${index + 1}`,
      text: `Question ${index + 1} ?`,
      context: `Contexte ${index + 1}`,
      status: 'pending' as const,
    })),
  };
}

describe('WorkDocumentsPanel', () => {
  beforeEach(() => {
    localStorage.clear();
    vi.spyOn(crypto, 'randomUUID').mockReturnValue('00000000-0000-4000-8000-000000000001');
    vi.mocked(listWorkDocumentStateSummaries).mockResolvedValue([]);
    vi.mocked(loadWorkDocumentState).mockResolvedValue(null);
    vi.mocked(saveWorkDocumentState).mockResolvedValue(undefined);
    vi.mocked(useChat).mockReturnValue({
      sendMessage: vi.fn(),
      regenerate: vi.fn(),
      cancel: vi.fn(),
      isStreaming: false,
    });
    useChatStore.setState({
      sessions: [],
      currentSessionId: null,
      isStreaming: false,
      selectedRepo: 'repo_alise',
      selectedRepoName: 'sample-app',
      inputDraft: '',
      isSfdPanelOpen: false,
      isWorkDocumentsPanelOpen: true,
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

  it('renders the technical-book workshop with progress and per-question actions', () => {
    useChatStore.setState({
      workDocuments: [makeDocument()],
      currentWorkDocumentId: 'doc-1',
    });

    render(<WorkDocumentsPanel />);

    expect(screen.getByText('Atelier Word Code Explorer')).toBeTruthy();
    expect(screen.getByText('Livrable Code Explorer - Questions Sample.docx')).toBeTruthy();
    expect(screen.getAllByText('Brouillon à relire').length).toBeGreaterThan(0);
    expect(screen.getAllByText(/1\/2 réponses/).length).toBeGreaterThan(0);
    expect(screen.getByRole('button', { name: 'Traiter 1 question' })).toBeTruthy();
    expect(screen.getByRole('button', { name: 'Copier la réponse Q1.1' })).toBeTruthy();
    expect(screen.getByRole('button', { name: 'DOCX' })).toBeTruthy();
  });

  it('toggles the Word workshop fullscreen layout', () => {
    useChatStore.setState({
      workDocuments: [makeDocument()],
      currentWorkDocumentId: 'doc-1',
    });

    render(<WorkDocumentsPanel />);

    const panel = screen.getByLabelText('Documents de travail');
    expect(panel.className).toContain('max-w-5xl');

    fireEvent.click(
      screen.getByRole('button', { name: 'Afficher l’atelier Word DOCX en plein écran' })
    );

    expect(panel.className).toContain('fixed');
    expect(panel.className).toContain('max-w-none');
    expect(screen.getByRole('button', { name: 'Réduire l’atelier Word DOCX' })).toBeTruthy();
  });

  it('filters questions by processing status', () => {
    useChatStore.setState({
      workDocuments: [makeDocument()],
      currentWorkDocumentId: 'doc-1',
    });

    render(<WorkDocumentsPanel />);

    fireEvent.click(screen.getByRole('button', { name: 'À traiter · 1' }));
    expect(screen.queryByRole('button', { name: 'Copier la réponse Q1.1' })).toBeNull();
    expect(screen.getAllByText('Pourquoi le plafond est-il recalculé ?').length).toBeGreaterThan(0);

    fireEvent.click(screen.getByRole('button', { name: 'Répondues · 1' }));
    expect(screen.getByRole('button', { name: 'Copier la réponse Q1.1' })).toBeTruthy();
    expect(screen.queryByRole('button', { name: 'Répondre' })).toBeNull();
  });

  it('renders detailed answers with Markdown and Mermaid diagrams', () => {
    const document = makeDocument();
    document.questions[0].answer = [
      '```mermaid',
      'flowchart TD',
      '  A[Question] --> B[Réponse]',
      '```',
      '',
      'Synthèse courte.',
      '',
      '## Sources',
      '- Acme.Sample/Flux/Question.cs',
    ].join('\n');
    useChatStore.setState({
      workDocuments: [document],
      currentWorkDocumentId: 'doc-1',
    });

    render(<WorkDocumentsPanel />);

    expect(screen.getByText('Réponse détaillée avec graphiques')).toBeTruthy();
    expect(screen.getByTestId('workdoc-answer-preview-q-1').textContent).toBe('Synthèse courte.');
    expect(screen.getByTestId('workdoc-mermaid-render').textContent).toContain('flowchart TD');
    expect(screen.getByRole('heading', { name: 'Sources' })).toBeTruthy();
  });

  it('shows source buttons for answered questions and opens the explorer callback', () => {
    const onOpenSourceReference = vi.fn();
    const document = makeDocument();
    document.questions[0].answer = [
      'La règle est portée par Acme.Sample/Flux/Question.cs:42.',
      '',
      '## Sources',
      '- Acme.Sample/Flux/Question.cs:42',
    ].join('\n');
    useChatStore.setState({
      workDocuments: [document],
      currentWorkDocumentId: 'doc-1',
    });

    render(
      <WorkDocumentsPanel
        onOpenSourceReference={onOpenSourceReference}
        validSourcePaths={new Set(['ccas.alise/flux/question.cs'])}
        sourceValidationState="ready"
      />
    );

    fireEvent.click(
      screen.getByRole('button', {
        name: "Ouvrir Acme.Sample/Flux/Question.cs dans l'explorateur sources",
      })
    );

    expect(onOpenSourceReference).toHaveBeenCalledWith({
      path: 'Acme.Sample/Flux/Question.cs',
      startLine: 42,
      endLine: 42,
    });
    expect(screen.getByLabelText('Documents de travail')).toBeTruthy();
  });

  it('imports a DOCX through the work-document endpoint and stores extracted questions', async () => {
    vi.spyOn(mcpClient, 'extractWorkDocument').mockResolvedValue({
      document: {
        filename: 'Questions importées.docx',
        bytes: 2048,
        markdownChars: 6000,
      },
      questions: [
        {
          id: 'q-imported',
          order: 1,
          label: 'Q1',
          text: 'A quoi correspondent ces paramétrages ?',
          context: 'Paramétrage Création Groupe d’Aide',
        },
      ],
      sourceMarkdown: '# Source convertie',
    });

    const { container } = render(<WorkDocumentsPanel />);
    const input = container.querySelector('input[type="file"]') as HTMLInputElement;
    const file = new File(['fake-docx'], 'Questions importées.docx', {
      type: 'application/vnd.openxmlformats-officedocument.wordprocessingml.document',
    });

    fireEvent.change(input, { target: { files: [file] } });

    await waitFor(() => {
      expect(mcpClient.extractWorkDocument).toHaveBeenCalledWith(file);
      expect(useChatStore.getState().workDocuments).toHaveLength(1);
    });
    expect(saveWorkDocumentSource).toHaveBeenCalledWith(
      '00000000-0000-4000-8000-000000000001',
      '# Source convertie'
    );
    expect(saveWorkDocumentState).toHaveBeenCalledWith(
      expect.objectContaining({
        id: '00000000-0000-4000-8000-000000000001',
        filename: 'Questions importées.docx',
      })
    );
    expect(useChatStore.getState().workDocuments[0].questions[0]).toMatchObject({
      label: 'Q1',
      text: 'A quoi correspondent ces paramétrages ?',
      status: 'pending',
    });
  });

  it('asks which colored question group to import when a DOCX contains several groups', async () => {
    vi.spyOn(mcpClient, 'extractWorkDocument').mockResolvedValue({
      document: {
        filename: 'Questions couleurs.docx',
        bytes: 4096,
        markdownChars: 9000,
      },
      questions: [
        {
          id: 'q-old',
          order: 1,
          label: 'Q1',
          text: 'Ancienne question à oublier ?',
          context: 'Contexte vert',
        },
        {
          id: 'q-new-1',
          order: 2,
          label: 'Q2',
          text: 'Nouvelle question à traiter ?',
          context: 'Contexte bleu',
        },
        {
          id: 'q-new-2',
          order: 3,
          label: 'Q3',
          text: 'Autre nouvelle question ?',
          context: 'Contexte bleu',
        },
      ],
      questionGroups: [
        {
          id: 'color-green-00b050',
          label: 'Vert',
          color: { family: 'green', label: 'Vert', value: '00B050' },
          questionCount: 1,
          questionIds: ['q-old'],
        },
        {
          id: 'color-blue-0070c0',
          label: 'Bleu',
          color: { family: 'blue', label: 'Bleu', value: '0070C0' },
          questionCount: 2,
          questionIds: ['q-new-1', 'q-new-2'],
        },
      ],
      sourceMarkdown: '# Source convertie',
    });

    const { container } = render(<WorkDocumentsPanel />);
    const input = container.querySelector('input[type="file"]') as HTMLInputElement;
    const file = new File(['fake-docx'], 'Questions couleurs.docx', {
      type: 'application/vnd.openxmlformats-officedocument.wordprocessingml.document',
    });

    fireEvent.change(input, { target: { files: [file] } });

    await waitFor(() => {
      expect(screen.getByRole('dialog', { name: 'Choisir le groupe de questions' })).toBeTruthy();
      expect(useChatStore.getState().workDocuments).toHaveLength(0);
    });

    fireEvent.click(
      screen.getByRole('button', { name: /Importer le groupe Bleu avec 2 question/ })
    );

    await waitFor(() => expect(useChatStore.getState().workDocuments).toHaveLength(1));
    const imported = useChatStore.getState().workDocuments[0];
    expect(imported.filename).toBe('Questions couleurs - Bleu.docx');
    expect(imported.questions.map((question) => question.text)).toEqual([
      'Nouvelle question à traiter ?',
      'Autre nouvelle question ?',
    ]);
    expect(imported.questions.map((question) => question.order)).toEqual([1, 2]);
  });

  it('restores persisted work documents from SQLite when local state is empty', async () => {
    const restoredDocument = {
      ...makeDocument(),
      id: 'doc-restored',
      filename: 'Questions restaurées.docx',
    };
    vi.mocked(listWorkDocumentStateSummaries).mockResolvedValueOnce([
      {
        id: 'doc-restored',
        filename: 'Questions restaurées.docx',
        repo: 'repo_alise',
        repoName: 'sample-app',
        importedAt: 1774500000000,
        savedAtUnixMs: 1774500100000,
        questionCount: 2,
      },
    ]);
    vi.mocked(loadWorkDocumentState).mockResolvedValueOnce(restoredDocument);

    render(<WorkDocumentsPanel />);

    await waitFor(() => {
      expect(useChatStore.getState().workDocuments[0]).toMatchObject({
        id: 'doc-restored',
        filename: 'Questions restaurées.docx',
      });
    });
    expect(screen.getByText('Livrable Code Explorer - Questions restaurées.docx')).toBeTruthy();
  });

  it('exports the current work document as a native DOCX payload', async () => {
    const blob = new Blob(['docx'], {
      type: 'application/vnd.openxmlformats-officedocument.wordprocessingml.document',
    });
    const exportDocx = vi.spyOn(mcpClient, 'exportWorkDocumentDocx').mockResolvedValue(blob);
    useChatStore.setState({
      workDocuments: [makeDocument()],
      currentWorkDocumentId: 'doc-1',
    });

    render(<WorkDocumentsPanel />);
    fireEvent.click(screen.getByRole('button', { name: 'DOCX' }));

    await waitFor(() => expect(exportDocx).toHaveBeenCalledTimes(1));
    const payload = exportDocx.mock.calls[0][0];
    expect(payload.title).toBe('Livrable Code Explorer (Brouillon à relire) - Questions Sample.docx');
    expect(payload.markdown).toContain('| Questions répondues | 1 |');
    expect(payload.markdown).toContain('| Statut du livrable | Brouillon à relire |');
    expect(payload.markdown).toContain('Acme.Sample/Logging/StackLogger.cs');
    expect(downloadBlobFile).toHaveBeenCalledWith(
      expect.stringMatching(/\.docx$/),
      blob,
      'application/vnd.openxmlformats-officedocument.wordprocessingml.document'
    );
  });

  it('exports the current work document as a native PDF payload', async () => {
    const blob = new Blob(['pdf'], { type: 'application/pdf' });
    const exportPdf = vi.spyOn(mcpClient, 'exportPdf').mockResolvedValue(blob);
    useChatStore.setState({
      workDocuments: [makeDocument()],
      currentWorkDocumentId: 'doc-1',
    });

    render(<WorkDocumentsPanel />);
    fireEvent.click(screen.getByRole('button', { name: 'PDF' }));

    await waitFor(() => expect(exportPdf).toHaveBeenCalledTimes(1));
    const payload = exportPdf.mock.calls[0][0];
    expect(payload.filename).toMatch(/\.pdf$/);
    expect(payload.html).toContain('Livrable Code Explorer (Brouillon à relire)');
    expect(payload.html).toContain('Questions Sample.docx');
    expect(payload.html).toContain('StackLogger trace les méthodes métier');
    expect(payload.html).toContain('Acme.Sample/Logging/StackLogger.cs');
    expect(downloadBlobFile).toHaveBeenCalledWith(
      expect.stringMatching(/\.pdf$/),
      blob,
      'application/pdf'
    );
  });

  it('does not mark questions as failed when no project is selected before batch launch', () => {
    const sendMessage = vi.fn();
    vi.mocked(useChat).mockReturnValue({
      sendMessage,
      regenerate: vi.fn(),
      cancel: vi.fn(),
      isStreaming: false,
    });
    useChatStore.setState({
      selectedRepo: null,
      selectedRepoName: null,
      workDocuments: [makeDocument()],
      currentWorkDocumentId: 'doc-1',
    });

    render(<WorkDocumentsPanel />);
    fireEvent.click(screen.getByRole('button', { name: 'Traiter 1 question' }));

    expect(
      screen.getByText('Sélectionne un projet dans le chat React avant de lancer le traitement.')
    ).toBeTruthy();
    expect(sendMessage).not.toHaveBeenCalled();
    const pendingQuestion = useChatStore.getState().workDocuments[0].questions[1];
    expect(pendingQuestion).toMatchObject({ status: 'pending' });
    expect(pendingQuestion).not.toHaveProperty('error');
  });

  it('anchors an imported document to the active React chat project when answering', async () => {
    const sendMessage = vi.fn().mockResolvedValue({
      ok: true,
      content: [
        'Réponse vérifiée avec <true> pour activer le recalcul.',
        '## . Synthèse courte',
        '- `` : valeur produite vide.',
        '',
        '```mermaid',
        'flowchart TD',
        '  Decision -->|| Suite',
        '```',
        '',
        '## Sources',
        '- Acme.Sample/Calcul/Plafond.cs',
      ].join('\n'),
      sessionId: '00000000-0000-4000-8000-000000000001',
      userMessageId: 'user-2',
      assistantMessageId: 'assistant-2',
    });
    vi.mocked(useChat).mockReturnValue({
      sendMessage,
      regenerate: vi.fn(),
      cancel: vi.fn(),
      isStreaming: false,
    });
    useChatStore.setState({
      workDocuments: [
        {
          ...makeDocument(),
          repo: null,
          repoName: null,
        },
      ],
      currentWorkDocumentId: 'doc-1',
    });

    render(<WorkDocumentsPanel />);
    fireEvent.click(screen.getByRole('button', { name: 'Répondre' }));

    await waitFor(() => expect(sendMessage).toHaveBeenCalledTimes(1));
    expect(sendMessage.mock.calls[0][1]).toMatchObject({ repository: 'repo_alise' });
    expect(useChatStore.getState().workDocuments[0]).toMatchObject({
      repo: 'repo_alise',
      repoName: 'sample-app',
    });
    const storedAnswer = useChatStore.getState().workDocuments[0].questions[1].answer ?? '';
    expect(storedAnswer).toContain('`true`');
    expect(storedAnswer).toContain('## Synthèse courte');
    expect(storedAnswer).not.toContain('## . Synthèse courte');
    expect(storedAnswer).toContain('`valeur à vérifier` : valeur produite vide');
    expect(storedAnswer).toContain('Decision --> Suite');
    expect(storedAnswer).not.toContain('<true>');
    expect(storedAnswer).not.toContain('`` :');
    expect(storedAnswer).not.toContain('-->||');
    expect(saveWorkDocumentState).toHaveBeenCalledWith(
      expect.objectContaining({
        id: 'doc-1',
        repo: 'repo_alise',
        questions: expect.arrayContaining([
          expect.objectContaining({
            id: 'q-2',
            status: 'answered',
            answer: expect.stringContaining('`true`'),
          }),
        ]),
      })
    );
  });

  it('opens the generated atelier chat thread from the document panel', async () => {
    useChatStore.setState({
      sessions: [
        {
          id: 'session-document',
          title: 'Atelier Questions Sample.docx',
          createdAt: 1774500000000,
          updatedAt: 1774500000000,
          messages: [],
        },
        {
          id: 'session-other',
          title: 'Autre conversation',
          createdAt: 1774490000000,
          updatedAt: 1774490000000,
          messages: [],
        },
      ],
      currentSessionId: 'session-other',
      workDocuments: [{ ...makeDocument(), sessionId: 'session-document' }],
      currentWorkDocumentId: 'doc-1',
    });

    render(<WorkDocumentsPanel />);
    fireEvent.click(screen.getByRole('button', { name: 'Ouvrir le fil chat de ce document' }));

    expect(useChatStore.getState().currentSessionId).toBe('session-document');
    await waitFor(() => expect(screen.queryByLabelText('Documents de travail')).toBeNull());
  });

  it('keeps a batch pinned to the launch project if the selected repo changes mid-run', async () => {
    const sendMessage = vi
      .fn()
      .mockImplementationOnce(async () => {
        useChatStore.setState({
          selectedRepo: 'repo_other',
          selectedRepoName: 'Autre projet',
        });
        return {
          ok: true,
          content: 'Réponse Q1.\n\n## Sources\n- Acme.Sample/Flux/Q1.cs',
          sessionId: 'atelier-session',
          userMessageId: 'user-1',
          assistantMessageId: 'assistant-1',
        };
      })
      .mockResolvedValueOnce({
        ok: true,
        content: 'Réponse Q2.\n\n## Sources\n- Acme.Sample/Flux/Q2.cs',
        sessionId: 'atelier-session',
        userMessageId: 'user-2',
        assistantMessageId: 'assistant-2',
      });
    vi.mocked(useChat).mockReturnValue({
      sendMessage,
      regenerate: vi.fn(),
      cancel: vi.fn(),
      isStreaming: false,
    });
    const document = makeDocument();
    document.questions = document.questions.map((question) => ({
      ...question,
      status: 'pending',
      answer: undefined,
      error: undefined,
    }));
    useChatStore.setState({
      workDocuments: [document],
      currentWorkDocumentId: 'doc-1',
    });

    render(<WorkDocumentsPanel />);
    fireEvent.click(screen.getByRole('button', { name: 'Traiter 2 questions' }));

    await waitFor(() => expect(sendMessage).toHaveBeenCalledTimes(2));
    expect(sendMessage.mock.calls.map((call) => call[1]?.repository)).toEqual([
      'repo_alise',
      'repo_alise',
    ]);
    expect(useChatStore.getState().workDocuments[0]).toMatchObject({
      repo: 'repo_alise',
      repoName: 'sample-app',
    });
  });

  it('processes long documents in resumable chunks from the React panel', async () => {
    const sendMessage = vi.fn().mockImplementation(async (_prompt: string, options) => ({
      ok: true,
      content: `Réponse ${sendMessage.mock.calls.length}.\n\n## Sources\n- Acme.Sample/Flux/Q${sendMessage.mock.calls.length}.cs`,
      sessionId: options?.sessionId ?? 'atelier-session',
      userMessageId: `user-${sendMessage.mock.calls.length}`,
      assistantMessageId: `assistant-${sendMessage.mock.calls.length}`,
    }));
    vi.mocked(useChat).mockReturnValue({
      sendMessage,
      regenerate: vi.fn(),
      cancel: vi.fn(),
      isStreaming: false,
    });
    useChatStore.setState({
      workDocuments: [makePendingDocument(6)],
      currentWorkDocumentId: 'doc-1',
    });

    render(<WorkDocumentsPanel />);

    expect(screen.getByRole('button', { name: 'Traiter 3 questions' })).toBeTruthy();
    fireEvent.click(screen.getByRole('button', { name: 'Traiter 3 questions' }));

    await waitFor(() => expect(sendMessage).toHaveBeenCalledTimes(3));
    await waitFor(() => {
      expect(
        screen.getByText('Lot terminé: 3 réponse(s), 0 échec(s). 3 question(s) restante(s).')
      ).toBeTruthy();
    });
    expect(
      useChatStore
        .getState()
        .workDocuments[0].questions.filter((question) => question.status === 'answered')
    ).toHaveLength(3);

    fireEvent.click(screen.getByRole('button', { name: 'Lot de 1' }));
    fireEvent.click(screen.getByRole('button', { name: 'Traiter 1 question' }));

    await waitFor(() => expect(sendMessage).toHaveBeenCalledTimes(4));
    expect(
      useChatStore
        .getState()
        .workDocuments[0].questions.filter((question) => question.status === 'answered')
    ).toHaveLength(4);
  });

  it('shows live batch progress while a question is still running', async () => {
    let resolveFirst:
      | ((value: Awaited<ReturnType<ReturnType<typeof useChat>['sendMessage']>>) => void)
      | undefined;
    const sendMessage = vi
      .fn()
      .mockImplementationOnce(
        () =>
          new Promise((resolve) => {
            resolveFirst = resolve;
          })
      )
      .mockResolvedValueOnce({
        ok: true,
        content: 'Réponse Q2.\n\n## Sources\n- Acme.Sample/Flux/Q2.cs',
        sessionId: 'atelier-session',
        userMessageId: 'user-2',
        assistantMessageId: 'assistant-2',
      });
    vi.mocked(useChat).mockReturnValue({
      sendMessage,
      regenerate: vi.fn(),
      cancel: vi.fn(),
      isStreaming: false,
    });
    const document = makeDocument();
    document.questions = document.questions.map((question) => ({
      ...question,
      status: 'pending',
      answer: undefined,
      error: undefined,
    }));
    useChatStore.setState({
      workDocuments: [document],
      currentWorkDocumentId: 'doc-1',
    });

    render(<WorkDocumentsPanel />);
    fireEvent.click(screen.getByRole('button', { name: 'Traiter 2 questions' }));

    await waitFor(() => {
      expect(screen.getByText('Traitement en cours: Q1.1 (1/2)')).toBeTruthy();
      expect(screen.getByText('0/2 traitée(s)')).toBeTruthy();
      expect(screen.getByText('0 réponse(s)')).toBeTruthy();
      expect(screen.getByText('0 échec(s)')).toBeTruthy();
    });

    resolveFirst?.({
      ok: true,
      content: 'Réponse Q1.\n\n## Sources\n- Acme.Sample/Flux/Q1.cs',
      sessionId: 'atelier-session',
      userMessageId: 'user-1',
      assistantMessageId: 'assistant-1',
    });

    await waitFor(() => expect(sendMessage).toHaveBeenCalledTimes(2));
    await waitFor(() => {
      expect(screen.getByText('Traitement terminé: 2 réponse(s), 0 échec(s).')).toBeTruthy();
      expect(screen.getByText('2/2 traitée(s)')).toBeTruthy();
    });
  });

  it('keeps processing remaining questions after an isolated answer failure', async () => {
    const sendMessage = vi.fn().mockResolvedValueOnce(null).mockResolvedValueOnce({
      ok: true,
      content: 'Réponse vérifiée.\n\n## Sources\n- Acme.Sample/Calcul/Plafond.cs',
      sessionId: 'atelier-session',
      userMessageId: 'user-2',
      assistantMessageId: 'assistant-2',
    });
    vi.mocked(useChat).mockReturnValue({
      sendMessage,
      regenerate: vi.fn(),
      cancel: vi.fn(),
      isStreaming: false,
    });
    const document = makeDocument();
    document.questions = document.questions.map((question) => ({
      ...question,
      status: 'pending',
      answer: undefined,
      error: undefined,
    }));
    useChatStore.setState({
      workDocuments: [document],
      currentWorkDocumentId: 'doc-1',
    });

    render(<WorkDocumentsPanel />);
    fireEvent.click(screen.getByRole('button', { name: 'Traiter 2 questions' }));

    await waitFor(() => expect(sendMessage).toHaveBeenCalledTimes(2));
    expect(screen.getByText('Traitement terminé: 1 réponse(s), 1 échec(s).')).toBeTruthy();
    const questions = useChatStore.getState().workDocuments[0].questions;
    expect(questions[0]).toMatchObject({ status: 'error' });
    expect(questions[1]).toMatchObject({
      status: 'answered',
      answer: expect.stringContaining('Réponse vérifiée'),
    });
  });
});

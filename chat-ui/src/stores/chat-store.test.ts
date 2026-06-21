import { afterEach, describe, expect, it, vi } from 'vitest';
import { migratePersistedChatState, useChatStore } from './chat-store';

afterEach(() => {
  vi.useRealTimers();
});

describe('chat-store persistence', () => {
  it('does not persist transient UI and streaming state', () => {
    const options = useChatStore.persist.getOptions();
    const partialized = options.partialize?.({
      ...useChatStore.getState(),
      isStreaming: true,
      isSfdPanelOpen: true,
      isWorkDocumentsPanelOpen: true,
      inputDraft: 'Trace un flux',
    });

    expect(partialized).toMatchObject({
      inputDraft: 'Trace un flux',
      isWorkspaceOpen: useChatStore.getState().isWorkspaceOpen,
      workspaceTab: useChatStore.getState().workspaceTab,
      verifiedMode: useChatStore.getState().verifiedMode,
    });
    expect(partialized).not.toHaveProperty('isStreaming');
    expect(partialized).not.toHaveProperty('isSfdPanelOpen');
    expect(partialized).not.toHaveProperty('isWorkDocumentsPanelOpen');
  });

  it('migrates older persisted state while clearing volatile fields', () => {
    const migrated = migratePersistedChatState({
      sessions: [],
      currentSessionId: 's1',
      selectedRepo: 'repo_alise',
      selectedRepoName: 'sample-app',
      inputDraft: 'Question en cours',
      isStreaming: true,
      isSfdPanelOpen: true,
      isWorkDocumentsPanelOpen: true,
    });

    expect(migrated).toEqual({
      sessions: [],
      currentSessionId: null,
      selectedRepo: 'repo_alise',
      selectedRepoName: 'sample-app',
      inputDraft: 'Question en cours',
      isWorkspaceOpen: false,
      workspaceTab: 'sources',
      verifiedMode: false,
      workDocuments: [],
      currentWorkDocumentId: null,
    });
    expect(migrated).not.toHaveProperty('isStreaming');
    expect(migrated).not.toHaveProperty('isSfdPanelOpen');
    expect(migrated).not.toHaveProperty('isWorkDocumentsPanelOpen');
  });

  it('recovers usable chat history from malformed persisted state', () => {
    const migrated = migratePersistedChatState({
      sessions: [
        {
          id: 's1',
          createdAt: '1000',
          updatedAt: '2000',
          analyses: [
            {
              id: 'a1',
              title: 'Analyse courriers',
              repo: 'repo_alise',
              repoName: 'sample-app',
              createdAt: '3500',
              updatedAt: '3600',
              sourceReferences: [
                { path: 'Controllers\\CourrierController.cs', startLine: '42', endLine: '44' },
                { path: 'Controllers/CourrierController.cs', startLine: 42, endLine: 44 },
                { path: '', startLine: 1 },
                null,
              ],
              summary: {
                fileCount: '1',
                diagramCount: '2',
                toolCallCount: 3,
                decisionCount: -1,
              },
            },
            { id: '', title: 'Cassée' },
          ],
          messages: [
            {
              id: 'm1',
              role: 'assistant',
              content: 'Réponse conservée',
              createdAt: '3000',
              verified: true,
              toolCalls: [
                {
                  id: 'tc1',
                  name: 'query_repo',
                  args: { q: 'courrier' },
                  result: { count: 2 },
                  status: 'done',
                },
                { id: 'tc2', name: 'broken_tool', args: [], status: 'unknown' },
              ],
            },
            { id: 'm2', role: 'assistant', content: 42, createdAt: 4000 },
            { id: 'm3', role: 'alien', content: 'à ignorer', createdAt: 5000 },
          ],
        },
        { id: 42, title: 'Session cassée', messages: [] },
        null,
      ],
      currentSessionId: 'missing',
      selectedRepo: 123,
      selectedRepoName: 'Nom orphelin',
      inputDraft: 123,
    });

    expect(migrated).toEqual({
      sessions: [
        {
          id: 's1',
          title: 'Conversation récupérée',
          createdAt: 1000,
          updatedAt: 2000,
          analyses: [
            {
              id: 'a1',
              title: 'Analyse courriers',
              repo: 'repo_alise',
              repoName: 'sample-app',
              createdAt: 3500,
              updatedAt: 3600,
              sourceReferences: [
                {
                  path: 'Controllers/CourrierController.cs',
                  startLine: 42,
                  endLine: 44,
                },
              ],
              summary: {
                fileCount: 1,
                diagramCount: 2,
                toolCallCount: 3,
                decisionCount: 0,
              },
            },
          ],
          messages: [
            {
              id: 'm1',
              role: 'assistant',
              content: 'Réponse conservée',
              createdAt: 3000,
              verified: true,
              toolCalls: [
                {
                  id: 'tc1',
                  name: 'query_repo',
                  args: { q: 'courrier' },
                  result: { count: 2 },
                  status: 'done',
                },
              ],
            },
          ],
        },
      ],
      currentSessionId: 's1',
      selectedRepo: null,
      selectedRepoName: null,
      inputDraft: '',
      isWorkspaceOpen: false,
      workspaceTab: 'sources',
      verifiedMode: false,
      workDocuments: [],
      currentWorkDocumentId: null,
    });
  });

  it('keeps valid persisted LLM snapshots on messages', () => {
    const migrated = migratePersistedChatState({
      sessions: [
        {
          id: 's1',
          title: 'LLM snapshot',
          createdAt: 1000,
          updatedAt: 2000,
          messages: [
            {
              id: 'm1',
              role: 'assistant',
              content: 'Réponse locale',
              createdAt: 3000,
              llm: {
                configured: true,
                provider: 'lm-studio',
                model: 'mistralai/ministral-3-3b',
                reasoningEffort: 'none',
                maxTokens: 2048,
              },
            },
          ],
        },
      ],
      currentSessionId: 's1',
    });

    expect(migrated.sessions[0].messages[0].llm).toMatchObject({
      configured: true,
      provider: 'lm-studio',
      model: 'mistralai/ministral-3-3b',
      reasoningEffort: 'none',
      maxTokens: 2048,
    });
  });

  it('repairs interrupted empty assistant messages during migration', () => {
    const migrated = migratePersistedChatState({
      sessions: [
        {
          id: 's1',
          title: 'Interrompue',
          createdAt: 1000,
          updatedAt: 2000,
          messages: [
            { id: 'm1', role: 'user', content: 'Explique StackLogger', createdAt: 1000 },
            { id: 'm2', role: 'assistant', content: '', createdAt: 2000 },
          ],
        },
      ],
      currentSessionId: 's1',
    });

    expect(migrated.sessions[0].messages[1].content).toContain('Réponse vide ou interrompue');
  });

  it('orders persisted and updated sessions by recent activity', () => {
    const migrated = migratePersistedChatState({
      sessions: [
        { id: 'old', title: 'Ancienne', createdAt: 1000, updatedAt: 1000, messages: [] },
        { id: 'new', title: 'Récente', createdAt: 1000, updatedAt: 2000, messages: [] },
      ],
      currentSessionId: 'missing',
    });

    expect(migrated.sessions.map((session) => session.id)).toEqual(['new', 'old']);
    expect(migrated.currentSessionId).toBe('new');

    vi.useFakeTimers();
    vi.setSystemTime(5000);
    useChatStore.setState({
      sessions: migrated.sessions,
      currentSessionId: 'old',
      selectedRepo: null,
      selectedRepoName: null,
      inputDraft: '',
      isStreaming: false,
      isSfdPanelOpen: false,
      isWorkDocumentsPanelOpen: false,
      workDocuments: [],
      currentWorkDocumentId: null,
    });

    useChatStore.getState().appendMessage('old', {
      id: 'm1',
      role: 'user',
      content: 'Nouvelle question',
      createdAt: 5000,
    });

    expect(useChatStore.getState().sessions.map((session) => session.id)).toEqual(['old', 'new']);
    expect(useChatStore.getState().sessions[0].updatedAt).toBe(5000);
  });

  it('saves, updates, caps, and deletes analysis snapshots on sessions', () => {
    vi.useFakeTimers();
    vi.setSystemTime(10_000);
    useChatStore.setState({
      sessions: [
        {
          id: 's1',
          title: 'Courriers',
          createdAt: 1000,
          updatedAt: 1000,
          messages: [],
        },
      ],
      currentSessionId: 's1',
      selectedRepo: null,
      selectedRepoName: null,
      inputDraft: '',
      isStreaming: false,
      isSfdPanelOpen: false,
      isWorkDocumentsPanelOpen: false,
      workDocuments: [],
      currentWorkDocumentId: null,
    });

    useChatStore.getState().saveAnalysisSnapshot('s1', {
      id: 'a1',
      title: 'Analyse initiale',
      repo: 'repo_alise',
      repoName: 'sample-app',
      createdAt: 9000,
      updatedAt: 9000,
      sourceReferences: [{ path: 'Controllers/CourrierController.cs', startLine: 42 }],
      summary: {
        fileCount: 1,
        diagramCount: 1,
        toolCallCount: 2,
        decisionCount: 0,
      },
    });

    expect(useChatStore.getState().sessions[0].analyses).toHaveLength(1);
    expect(useChatStore.getState().sessions[0].analyses?.[0]).toMatchObject({
      id: 'a1',
      updatedAt: 10_000,
      summary: { fileCount: 1 },
    });

    vi.setSystemTime(20_000);
    for (let index = 2; index <= 14; index += 1) {
      useChatStore.getState().saveAnalysisSnapshot('s1', {
        id: `a${index}`,
        title: `Analyse ${index}`,
        repo: null,
        repoName: null,
        createdAt: index,
        updatedAt: index,
        sourceReferences: [{ path: `src/file-${index}.ts` }],
        summary: {
          fileCount: 1,
          diagramCount: 0,
          toolCallCount: 0,
          decisionCount: 0,
        },
      });
    }

    const analyses = useChatStore.getState().sessions[0].analyses ?? [];
    expect(analyses).toHaveLength(12);
    expect(analyses[0].id).toBe('a14');
    expect(analyses.some((snapshot) => snapshot.id === 'a1')).toBe(false);

    useChatStore.getState().deleteAnalysisSnapshot('s1', 'a14');
    expect(
      useChatStore.getState().sessions[0].analyses?.some((snapshot) => snapshot.id === 'a14')
    ).toBe(false);
  });

  it('persists document workbench questions safely', () => {
    const migrated = migratePersistedChatState({
      workDocuments: [
        {
          id: 'doc1',
          filename: 'Questions.docx',
          importedAt: '1000',
          repo: 'repo_alise',
          repoName: 'sample-app',
          sessionId: 'session-document',
          sourceBytes: '2048',
          markdownChars: 4096,
          questions: [
            {
              id: 'q2',
              order: 2,
              label: 'Q2',
              text: 'Comment payer ?',
              context: 'Contexte paiement',
              status: 'answered',
              answer: 'Réponse',
              messageIds: { user: 'u1', assistant: 'a1' },
            },
            {
              id: 'q1',
              order: 1,
              label: 'Q1',
              text: 'Que fait StackLogger ?',
              status: 'unknown',
            },
          ],
        },
      ],
      currentWorkDocumentId: 'doc1',
    });

    expect(migrated.workDocuments).toHaveLength(1);
    expect(migrated.currentWorkDocumentId).toBe('doc1');
    expect(migrated.workDocuments[0].sessionId).toBe('session-document');
    expect(migrated.workDocuments[0].questions.map((question) => question.id)).toEqual([
      'q1',
      'q2',
    ]);
    expect(migrated.workDocuments[0].questions[0].status).toBe('pending');
    expect(migrated.workDocuments[0].questions[1].messageIds).toEqual({
      user: 'u1',
      assistant: 'a1',
    });
  });
});

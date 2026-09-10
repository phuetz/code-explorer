import { create } from 'zustand';
import { persist } from 'zustand/middleware';
import type { LlmConfigInfo } from '../api/mcp-client';
import type {
  AnalysisSnapshot,
  Message,
  Role,
  Session,
  ToolCall,
  WorkDocument,
  WorkDocumentQuestion,
  WorkDocumentQuestionStatus,
  WorkspaceTabPreference,
} from '../types/chat';
import { EMPTY_ASSISTANT_RESPONSE_MESSAGE } from '../utils/empty-assistant-response';

interface ChatState {
  sessions: Session[];
  currentSessionId: string | null;
  isStreaming: boolean;
  selectedRepo: string | null;
  selectedRepoName: string | null;
  inputDraft: string;
  isSfdPanelOpen: boolean;
  isWorkDocumentsPanelOpen: boolean;
  isWorkspaceOpen: boolean;
  workspaceTab: WorkspaceTabPreference;
  verifiedMode: boolean;
  workDocuments: WorkDocument[];
  currentWorkDocumentId: string | null;

  createSession: (title?: string) => string;
  selectSession: (id: string) => void;
  deleteSession: (id: string) => void;
  renameSession: (id: string, title: string) => void;
  saveAnalysisSnapshot: (sessionId: string, snapshot: AnalysisSnapshot) => void;
  deleteAnalysisSnapshot: (sessionId: string, snapshotId: string) => void;
  addWorkDocument: (document: WorkDocument) => void;
  updateWorkDocument: (documentId: string, patch: Partial<WorkDocument>) => void;
  deleteWorkDocument: (documentId: string) => void;
  selectWorkDocument: (documentId: string | null) => void;
  updateWorkDocumentQuestion: (
    documentId: string,
    questionId: string,
    patch: Partial<WorkDocumentQuestion>
  ) => void;

  appendMessage: (sessionId: string, message: Message) => void;
  updateMessage: (sessionId: string, messageId: string, content: string) => void;
  upsertToolCall: (sessionId: string, messageId: string, toolCall: ToolCall) => void;
  removeMessagesFrom: (sessionId: string, messageId: string) => void;
  setStreaming: (streaming: boolean) => void;
  setSelectedRepo: (repo: string | null, displayName?: string | null) => void;
  setInputDraft: (text: string) => void;
  setSfdPanelOpen: (open: boolean) => void;
  setWorkDocumentsPanelOpen: (open: boolean) => void;
  setWorkspaceOpen: (open: boolean) => void;
  setWorkspaceTab: (tab: WorkspaceTabPreference) => void;
  setVerifiedMode: (enabled: boolean) => void;

  getCurrentSession: () => Session | null;
}

type PersistedChatState = Pick<
  ChatState,
  | 'sessions'
  | 'currentSessionId'
  | 'selectedRepo'
  | 'selectedRepoName'
  | 'inputDraft'
  | 'isWorkspaceOpen'
  | 'workspaceTab'
  | 'verifiedMode'
  | 'workDocuments'
  | 'currentWorkDocumentId'
>;

const newId = () => crypto.randomUUID();
const MESSAGE_ROLES: Role[] = ['user', 'assistant', 'system'];
const TOOL_CALL_STATUSES: ToolCall['status'][] = ['pending', 'running', 'done', 'error'];
const WORK_DOCUMENT_QUESTION_STATUSES: WorkDocumentQuestionStatus[] = [
  'pending',
  'answering',
  'answered',
  'error',
];

function persistedChatState(state: ChatState): PersistedChatState {
  return {
    sessions: state.sessions,
    currentSessionId: state.currentSessionId,
    selectedRepo: state.selectedRepo,
    selectedRepoName: state.selectedRepoName,
    inputDraft: state.inputDraft,
    isWorkspaceOpen: state.isWorkspaceOpen,
    workspaceTab: state.workspaceTab,
    verifiedMode: state.verifiedMode,
    workDocuments: persistableWorkDocuments(state.workDocuments),
    currentWorkDocumentId: state.currentWorkDocumentId,
  };
}

function persistableWorkDocuments(workDocuments: WorkDocument[]): WorkDocument[] {
  return workDocuments.map((document) => ({
    id: document.id,
    filename: document.filename,
    importedAt: document.importedAt,
    repo: document.repo,
    repoName: document.repoName,
    sessionId: document.sessionId,
    sourceBytes: document.sourceBytes,
    markdownChars: document.markdownChars,
    questions: document.questions,
  }));
}

function byRecentActivity(sessions: Session[]): Session[] {
  return [...sessions].sort((a, b) => b.updatedAt - a.updatedAt);
}

function updateSessionByRecentActivity(
  sessions: Session[],
  sessionId: string,
  update: (session: Session, now: number) => Session
): Session[] {
  const now = Date.now();
  return byRecentActivity(
    sessions.map((session) => (session.id === sessionId ? update(session, now) : session))
  );
}

export function migratePersistedChatState(persistedState: unknown): PersistedChatState {
  const state = asRecord(persistedState) ?? {};
  const sessions = byRecentActivity(sanitizeSessions(state.sessions));
  const selectedRepo = readString(state.selectedRepo);
  const workDocuments = sanitizeWorkDocuments(state.workDocuments);

  return {
    sessions,
    currentSessionId: sanitizeCurrentSessionId(state.currentSessionId, sessions),
    selectedRepo,
    selectedRepoName: selectedRepo ? (readString(state.selectedRepoName) ?? selectedRepo) : null,
    inputDraft: readString(state.inputDraft) ?? '',
    isWorkspaceOpen: readBoolean(state.isWorkspaceOpen) ?? false,
    workspaceTab: readWorkspaceTab(state.workspaceTab),
    verifiedMode: readBoolean(state.verifiedMode) ?? false,
    workDocuments,
    currentWorkDocumentId: sanitizeCurrentWorkDocumentId(
      state.currentWorkDocumentId,
      workDocuments
    ),
  };
}

function sanitizeCurrentSessionId(value: unknown, sessions: Session[]): string | null {
  const id = readString(value);
  if (id && sessions.some((session) => session.id === id)) {
    return id;
  }
  return sessions[0]?.id ?? null;
}

function sanitizeSessions(value: unknown): Session[] {
  if (!Array.isArray(value)) return [];

  const sessions: Session[] = [];
  for (const rawSession of value) {
    const session = asRecord(rawSession);
    const id = readString(session?.id);
    if (!session || !id) continue;

    const title = readString(session.title) ?? 'Conversation récupérée';
    const createdAt = readTimestamp(session.createdAt) ?? 0;
    const updatedAt = readTimestamp(session.updatedAt) ?? createdAt;

    const analyses = sanitizeAnalysisSnapshots(session.analyses);
    sessions.push({
      id,
      title,
      createdAt,
      updatedAt,
      messages: sanitizeMessages(session.messages),
      ...(analyses.length > 0 ? { analyses } : {}),
    });
  }
  return sessions;
}

function sanitizeAnalysisSnapshots(value: unknown): AnalysisSnapshot[] {
  if (!Array.isArray(value)) return [];

  const snapshots: AnalysisSnapshot[] = [];
  for (const rawSnapshot of value) {
    const snapshot = asRecord(rawSnapshot);
    const id = readString(snapshot?.id);
    const title = readString(snapshot?.title);
    if (!snapshot || !id || !title) continue;

    snapshots.push({
      id,
      title,
      repo: readString(snapshot.repo),
      repoName: readString(snapshot.repoName),
      createdAt: readTimestamp(snapshot.createdAt) ?? 0,
      updatedAt: readTimestamp(snapshot.updatedAt) ?? readTimestamp(snapshot.createdAt) ?? 0,
      sourceReferences: sanitizeAnalysisSourceReferences(snapshot.sourceReferences),
      summary: sanitizeAnalysisSummary(snapshot.summary),
    });
  }
  return snapshots.sort((a, b) => b.updatedAt - a.updatedAt).slice(0, 12);
}

function sanitizeAnalysisSourceReferences(value: unknown): AnalysisSnapshot['sourceReferences'] {
  if (!Array.isArray(value)) return [];
  const references: AnalysisSnapshot['sourceReferences'] = [];
  const seen = new Set<string>();
  for (const rawReference of value) {
    const reference = asRecord(rawReference);
    const path = readString(reference?.path);
    if (!reference || !path) continue;
    const item = {
      path: path.replace(/\\/g, '/'),
      startLine: readPositiveInteger(reference.startLine),
      endLine: readPositiveInteger(reference.endLine),
    };
    const key = `${item.path}:${item.startLine ?? ''}:${item.endLine ?? ''}`;
    if (seen.has(key)) continue;
    seen.add(key);
    references.push(item);
  }
  return references;
}

function sanitizeAnalysisSummary(value: unknown): AnalysisSnapshot['summary'] {
  const summary = asRecord(value) ?? {};
  return {
    fileCount: readNonNegativeInteger(summary.fileCount) ?? 0,
    diagramCount: readNonNegativeInteger(summary.diagramCount) ?? 0,
    toolCallCount: readNonNegativeInteger(summary.toolCallCount) ?? 0,
    decisionCount: readNonNegativeInteger(summary.decisionCount) ?? 0,
  };
}

function sanitizeCurrentWorkDocumentId(
  value: unknown,
  workDocuments: WorkDocument[]
): string | null {
  const id = readString(value);
  if (id && workDocuments.some((document) => document.id === id)) {
    return id;
  }
  return workDocuments[0]?.id ?? null;
}

function sanitizeWorkDocuments(value: unknown): WorkDocument[] {
  if (!Array.isArray(value)) return [];

  const documents: WorkDocument[] = [];
  for (const rawDocument of value) {
    const document = asRecord(rawDocument);
    const id = readString(document?.id);
    const filename = readString(document?.filename);
    if (!document || !id || !filename) continue;

    const questions = sanitizeWorkDocumentQuestions(document.questions);
    documents.push({
      id,
      filename,
      importedAt: readTimestamp(document.importedAt) ?? 0,
      repo: readString(document.repo),
      repoName: readString(document.repoName),
      ...(readString(document.sessionId) ? { sessionId: readString(document.sessionId)! } : {}),
      sourceBytes: readNonNegativeInteger(document.sourceBytes) ?? 0,
      markdownChars: readNonNegativeInteger(document.markdownChars) ?? 0,
      questions,
    });
  }
  return documents.sort((a, b) => b.importedAt - a.importedAt).slice(0, 8);
}

function sanitizeWorkDocumentQuestions(value: unknown): WorkDocumentQuestion[] {
  if (!Array.isArray(value)) return [];

  const questions: WorkDocumentQuestion[] = [];
  for (const rawQuestion of value.slice(0, 120)) {
    const question = asRecord(rawQuestion);
    const id = readString(question?.id);
    const text = readString(question?.text);
    if (!question || !id || !text) continue;

    const messageIds = sanitizeMessageIds(question.messageIds);
    questions.push({
      id,
      order: readPositiveInteger(question.order) ?? questions.length + 1,
      label: readString(question.label) ?? `Q${questions.length + 1}`,
      text,
      context: readString(question.context) ?? '',
      status: readWorkDocumentQuestionStatus(question.status) ?? 'pending',
      ...(readString(question.prompt) ? { prompt: readString(question.prompt)! } : {}),
      ...(readString(question.answer) ? { answer: readString(question.answer)! } : {}),
      ...(readString(question.error) ? { error: readString(question.error)! } : {}),
      ...(readTimestamp(question.answeredAt) !== null
        ? { answeredAt: readTimestamp(question.answeredAt)! }
        : {}),
      ...(messageIds ? { messageIds } : {}),
    });
  }
  return questions.sort((a, b) => a.order - b.order);
}

function sanitizeMessageIds(value: unknown): WorkDocumentQuestion['messageIds'] | null {
  const raw = asRecord(value);
  if (!raw) return null;
  const user = readString(raw.user);
  const assistant = readString(raw.assistant);
  if (!user && !assistant) return null;
  return {
    ...(user ? { user } : {}),
    ...(assistant ? { assistant } : {}),
  };
}

function sanitizeMessages(value: unknown): Message[] {
  if (!Array.isArray(value)) return [];

  const messages: Message[] = [];
  for (const rawMessage of value) {
    const message = asRecord(rawMessage);
    const id = readString(message?.id);
    const role = readRole(message?.role);
    let content = readString(message?.content);
    if (!message || !id || !role || content === null) continue;
    if (role === 'assistant' && content.trim().length === 0) {
      content = EMPTY_ASSISTANT_RESPONSE_MESSAGE;
    }

    const toolCalls = sanitizeToolCalls(message.toolCalls);
    const llm = sanitizeLlmConfig(message.llm);
    messages.push({
      id,
      role,
      content,
      createdAt: readTimestamp(message.createdAt) ?? 0,
      ...(llm ? { llm } : {}),
      ...(readBoolean(message.verified) ? { verified: true } : {}),
      ...(toolCalls.length > 0 ? { toolCalls } : {}),
    });
  }
  return messages;
}

function sanitizeLlmConfig(value: unknown): LlmConfigInfo | null {
  const raw = asRecord(value);
  if (!raw || raw.configured !== true) return null;
  const maxTokens = readNonNegativeInteger(raw.maxTokens);
  return {
    configured: true,
    ...(readString(raw.provider) ? { provider: readString(raw.provider)! } : {}),
    ...(readString(raw.model) ? { model: readString(raw.model)! } : {}),
    ...(readString(raw.reasoningEffort)
      ? { reasoningEffort: readString(raw.reasoningEffort)! }
      : {}),
    ...(maxTokens !== null ? { maxTokens } : {}),
    ...(readString(raw.bigContextModel)
      ? { bigContextModel: readString(raw.bigContextModel)! }
      : {}),
  };
}

function sanitizeToolCalls(value: unknown): ToolCall[] {
  if (!Array.isArray(value)) return [];

  const toolCalls: ToolCall[] = [];
  for (const rawToolCall of value) {
    const toolCall = asRecord(rawToolCall);
    const id = readString(toolCall?.id);
    const name = readString(toolCall?.name);
    const status = readToolCallStatus(toolCall?.status);
    if (!toolCall || !id || !name || !status) continue;

    toolCalls.push({
      id,
      name,
      status,
      args: asRecord(toolCall.args) ?? {},
      ...('result' in toolCall ? { result: toolCall.result } : {}),
    });
  }
  return toolCalls;
}

function readRole(value: unknown): Role | null {
  const role = readString(value);
  return role && MESSAGE_ROLES.includes(role as Role) ? (role as Role) : null;
}

function readWorkspaceTab(value: unknown): WorkspaceTabPreference {
  return value === 'graph' ? 'graph' : 'sources';
}

function readToolCallStatus(value: unknown): ToolCall['status'] | null {
  const status = readString(value);
  return status && TOOL_CALL_STATUSES.includes(status as ToolCall['status'])
    ? (status as ToolCall['status'])
    : null;
}

function readWorkDocumentQuestionStatus(value: unknown): WorkDocumentQuestionStatus | null {
  const status = readString(value);
  return status && WORK_DOCUMENT_QUESTION_STATUSES.includes(status as WorkDocumentQuestionStatus)
    ? (status as WorkDocumentQuestionStatus)
    : null;
}

function readTimestamp(value: unknown): number | null {
  if (typeof value === 'number' && Number.isFinite(value)) return value;
  if (typeof value === 'string' && value.trim().length > 0) {
    const parsed = Number(value);
    return Number.isFinite(parsed) ? parsed : null;
  }
  return null;
}

function readPositiveInteger(value: unknown): number | undefined {
  const number =
    typeof value === 'number' ? value : typeof value === 'string' ? Number(value) : NaN;
  return Number.isInteger(number) && number > 0 ? number : undefined;
}

function readNonNegativeInteger(value: unknown): number | null {
  const number =
    typeof value === 'number' ? value : typeof value === 'string' ? Number(value) : NaN;
  return Number.isInteger(number) && number >= 0 ? number : null;
}

function readString(value: unknown): string | null {
  return typeof value === 'string' ? value : null;
}

function readBoolean(value: unknown): boolean | null {
  return typeof value === 'boolean' ? value : null;
}

function asRecord(value: unknown): Record<string, unknown> | null {
  return typeof value === 'object' && value !== null && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : null;
}

export const useChatStore = create<ChatState>()(
  persist(
    (set, get) => ({
      sessions: [],
      currentSessionId: null,
      isStreaming: false,
      selectedRepo: null,
      selectedRepoName: null,
      inputDraft: '',
      isSfdPanelOpen: false,
      isWorkDocumentsPanelOpen: false,
      isWorkspaceOpen: false,
      workspaceTab: 'sources',
      verifiedMode: false,
      workDocuments: [],
      currentWorkDocumentId: null,

      createSession: (title = 'Nouvelle conversation') => {
        const id = newId();
        const now = Date.now();
        const session: Session = {
          id,
          title,
          createdAt: now,
          updatedAt: now,
          messages: [],
        };
        set((s) => ({
          sessions: [session, ...s.sessions],
          currentSessionId: id,
        }));
        return id;
      },

      selectSession: (id) => set({ currentSessionId: id }),

      deleteSession: (id) =>
        set((s) => {
          const sessions = s.sessions.filter((sess) => sess.id !== id);
          const currentSessionId =
            s.currentSessionId === id ? (sessions[0]?.id ?? null) : s.currentSessionId;
          return { sessions, currentSessionId };
        }),

      renameSession: (id, title) =>
        set((s) => ({
          sessions: updateSessionByRecentActivity(s.sessions, id, (sess, now) => ({
            ...sess,
            title,
            updatedAt: now,
          })),
        })),

      saveAnalysisSnapshot: (sessionId, snapshot) =>
        set((s) => ({
          sessions: updateSessionByRecentActivity(s.sessions, sessionId, (sess, now) => {
            const existing = sess.analyses ?? [];
            const nextSnapshot = { ...snapshot, updatedAt: now };
            const next = [nextSnapshot, ...existing.filter((item) => item.id !== snapshot.id)]
              .sort((a, b) => b.updatedAt - a.updatedAt)
              .slice(0, 12);
            return {
              ...sess,
              analyses: next,
              updatedAt: now,
            };
          }),
        })),

      deleteAnalysisSnapshot: (sessionId, snapshotId) =>
        set((s) => ({
          sessions: updateSessionByRecentActivity(s.sessions, sessionId, (sess, now) => ({
            ...sess,
            analyses: (sess.analyses ?? []).filter((snapshot) => snapshot.id !== snapshotId),
            updatedAt: now,
          })),
        })),

      addWorkDocument: (document) =>
        set((s) => ({
          workDocuments: [
            document,
            ...s.workDocuments.filter((existing) => existing.id !== document.id),
          ].slice(0, 8),
          currentWorkDocumentId: document.id,
          isWorkDocumentsPanelOpen: true,
        })),

      updateWorkDocument: (documentId, patch) =>
        set((s) => ({
          workDocuments: s.workDocuments.map((document) =>
            document.id === documentId ? { ...document, ...patch } : document
          ),
        })),

      deleteWorkDocument: (documentId) =>
        set((s) => {
          const workDocuments = s.workDocuments.filter((document) => document.id !== documentId);
          const currentWorkDocumentId =
            s.currentWorkDocumentId === documentId
              ? (workDocuments[0]?.id ?? null)
              : s.currentWorkDocumentId;
          return { workDocuments, currentWorkDocumentId };
        }),

      selectWorkDocument: (documentId) =>
        set((s) => ({
          currentWorkDocumentId:
            documentId && s.workDocuments.some((document) => document.id === documentId)
              ? documentId
              : null,
        })),

      updateWorkDocumentQuestion: (documentId, questionId, patch) =>
        set((s) => ({
          workDocuments: s.workDocuments.map((document) =>
            document.id === documentId
              ? {
                  ...document,
                  questions: document.questions.map((question) =>
                    question.id === questionId ? { ...question, ...patch } : question
                  ),
                }
              : document
          ),
        })),

      appendMessage: (sessionId, message) =>
        set((s) => ({
          sessions: updateSessionByRecentActivity(s.sessions, sessionId, (sess, now) => ({
            ...sess,
            messages: [...sess.messages, message],
            updatedAt: now,
          })),
        })),

      updateMessage: (sessionId, messageId, content) =>
        set((s) => ({
          sessions: updateSessionByRecentActivity(s.sessions, sessionId, (sess, now) => ({
            ...sess,
            messages: sess.messages.map((m) => (m.id === messageId ? { ...m, content } : m)),
            updatedAt: now,
          })),
        })),

      removeMessagesFrom: (sessionId, messageId) =>
        set((s) => ({
          sessions: updateSessionByRecentActivity(s.sessions, sessionId, (sess, now) => {
            const idx = sess.messages.findIndex((m) => m.id === messageId);
            if (idx === -1) return sess;
            return {
              ...sess,
              messages: sess.messages.slice(0, idx),
              updatedAt: now,
            };
          }),
        })),

      upsertToolCall: (sessionId, messageId, toolCall) =>
        set((s) => ({
          sessions: updateSessionByRecentActivity(s.sessions, sessionId, (sess, now) => ({
            ...sess,
            messages: sess.messages.map((m) => {
              if (m.id !== messageId) return m;
              const existing = m.toolCalls ?? [];
              const idx = existing.findIndex((tc) => tc.id === toolCall.id);
              const next =
                idx === -1
                  ? [...existing, toolCall]
                  : existing.map((tc, i) => (i === idx ? { ...tc, ...toolCall } : tc));
              return { ...m, toolCalls: next };
            }),
            updatedAt: now,
          })),
        })),

      setStreaming: (streaming) => set({ isStreaming: streaming }),
      setSelectedRepo: (repo, displayName) =>
        set({
          selectedRepo: repo,
          selectedRepoName: repo ? (displayName ?? repo) : null,
        }),
      setInputDraft: (text) => set({ inputDraft: text }),
      setSfdPanelOpen: (open) => set({ isSfdPanelOpen: open }),
      setWorkDocumentsPanelOpen: (open) => set({ isWorkDocumentsPanelOpen: open }),
      setWorkspaceOpen: (open) => set({ isWorkspaceOpen: open }),
      setWorkspaceTab: (tab) => set({ workspaceTab: tab }),
      setVerifiedMode: (enabled) => set({ verifiedMode: enabled }),

      getCurrentSession: () => {
        const { sessions, currentSessionId } = get();
        return sessions.find((s) => s.id === currentSessionId) ?? null;
      },
    }),
    {
      name: 'code-explorer-chat-store',
      version: 6,
      partialize: persistedChatState,
      migrate: migratePersistedChatState,
    }
  )
);

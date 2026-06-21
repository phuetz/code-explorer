import { useCallback, useEffect, useMemo, useRef, useState, type ChangeEvent } from 'react';
import {
  AlertCircle,
  BookOpen,
  CheckCircle2,
  ClipboardCopy,
  Download,
  FileCode2,
  FileQuestion,
  ListChecks,
  Loader2,
  Maximize2,
  MessageSquareText,
  Minimize2,
  Play,
  Square,
  Trash2,
  Upload,
  X,
} from 'lucide-react';
import clsx from 'clsx';
import {
  mcpClient,
  type LlmConfigInfo,
  type WorkdocExtractResult,
  type WorkdocQuestionGroup,
} from '../../api/mcp-client';
import { useChatStore } from '../../stores/chat-store';
import type { WorkDocument, WorkDocumentQuestion } from '../../types/chat';
import { copyTextToClipboard } from '../../utils/clipboard';
import { downloadBlobFile, downloadTextFile } from '../../utils/related-sources-export';
import {
  auditWorkDocumentQuality,
  buildWorkDocumentPrintableHtml,
  buildWorkDocumentMarkdown,
  workDocumentExportTitle,
  buildWorkQuestionPrompt,
  normalizeWorkDocumentAnswer,
  workDocumentExportFilename,
  workDocumentExportFilenameFor,
  workDocumentReadinessLabel,
} from '../../utils/workdoc';
import { Markdown } from '../ui/Markdown';
import {
  deleteWorkDocumentSource,
  loadWorkDocumentSource,
  saveWorkDocumentSource,
} from '../../utils/workdoc-source-store';
import {
  deleteWorkDocumentState,
  listWorkDocumentStateSummaries,
  loadWorkDocumentState,
  saveWorkDocumentState,
} from '../../utils/workdoc-state-store';
import {
  extractSourceReferences,
  groupSourceReferences,
  type SourceReference,
  type SourceReferenceGroup,
  type SourceValidationState,
} from '../../utils/source-references';
import { useChat } from '../../hooks/use-chat';

const newId = () => crypto.randomUUID();
type QuestionFilter = 'all' | 'pending' | 'answered' | 'error';
type BatchLimit = 1 | 3 | 5 | 'all';

const BATCH_LIMITS: BatchLimit[] = [1, 3, 5, 'all'];

interface Props {
  llm?: LlmConfigInfo | null;
  onOpenSourceReference?: (reference: SourceReference) => void;
  validSourcePaths?: ReadonlySet<string> | null;
  sourceValidationState?: SourceValidationState;
}

interface AnswerRunContext {
  repository: string;
  repositoryName: string;
}

interface BatchProgress {
  total: number;
  processed: number;
  answered: number;
  failed: number;
  currentLabel: string | null;
}

interface PendingQuestionGroupImport {
  result: WorkdocExtractResult;
  groups: WorkdocQuestionGroup[];
  repo: string | null;
  repoName: string | null;
}

export function WorkDocumentsPanel({
  llm = null,
  onOpenSourceReference,
  validSourcePaths = null,
  sourceValidationState = 'unavailable',
}: Props) {
  const open = useChatStore((s) => s.isWorkDocumentsPanelOpen);
  const setOpen = useChatStore((s) => s.setWorkDocumentsPanelOpen);
  const selectedRepo = useChatStore((s) => s.selectedRepo);
  const selectedRepoName = useChatStore((s) => s.selectedRepoName);
  const documents = useChatStore((s) => s.workDocuments);
  const currentDocumentId = useChatStore((s) => s.currentWorkDocumentId);
  const addWorkDocument = useChatStore((s) => s.addWorkDocument);
  const updateWorkDocument = useChatStore((s) => s.updateWorkDocument);
  const deleteWorkDocument = useChatStore((s) => s.deleteWorkDocument);
  const selectWorkDocument = useChatStore((s) => s.selectWorkDocument);
  const selectSession = useChatStore((s) => s.selectSession);
  const updateQuestion = useChatStore((s) => s.updateWorkDocumentQuestion);
  const { sendMessage, cancel, isStreaming } = useChat(llm);
  const inputRef = useRef<HTMLInputElement>(null);
  const batchAbortRef = useRef(false);
  const hydratedDocumentIdsRef = useRef<Set<string>>(new Set());
  const restoredPersistedDocumentsRef = useRef(false);
  const [busyImport, setBusyImport] = useState(false);
  const [batchRunning, setBatchRunning] = useState(false);
  const [exportBusy, setExportBusy] = useState<'docx' | 'pdf' | null>(null);
  const [panelError, setPanelError] = useState<string | null>(null);
  const [batchNotice, setBatchNotice] = useState<string | null>(null);
  const [activeBatchQuestionLabel, setActiveBatchQuestionLabel] = useState<string | null>(null);
  const [batchProgress, setBatchProgress] = useState<BatchProgress | null>(null);
  const [batchLimit, setBatchLimit] = useState<BatchLimit>(3);
  const [questionFilter, setQuestionFilter] = useState<QuestionFilter>('all');
  const [copyState, setCopyState] = useState<'idle' | 'copied' | 'failed'>('idle');
  const [copiedQuestionId, setCopiedQuestionId] = useState<string | null>(null);
  const [isFullscreen, setIsFullscreen] = useState(false);
  const [pendingQuestionGroupImport, setPendingQuestionGroupImport] =
    useState<PendingQuestionGroupImport | null>(null);

  const currentDocument = useMemo(() => {
    return documents.find((document) => document.id === currentDocumentId) ?? documents[0] ?? null;
  }, [currentDocumentId, documents]);

  const stats = useMemo(() => documentStats(currentDocument), [currentDocument]);
  const visibleQuestions = useMemo(
    () => filterQuestions(currentDocument?.questions ?? [], questionFilter),
    [currentDocument, questionFilter]
  );
  const remainingQuestionCount = useMemo(
    () =>
      currentDocument?.questions.filter((question) => question.status !== 'answered').length ?? 0,
    [currentDocument]
  );
  const close = useCallback(() => {
    setIsFullscreen(false);
    setOpen(false);
  }, [setOpen]);
  const openQuestionSource = useCallback(
    (reference: SourceReference) => {
      onOpenSourceReference?.(reference);
      setIsFullscreen(false);
    },
    [onOpenSourceReference]
  );

  const currentDocumentSourceMarkdown = currentDocument?.sourceMarkdown;
  const currentDocumentLoadId = currentDocument?.id ?? null;

  useEffect(() => {
    if (!open || documents.length > 0 || restoredPersistedDocumentsRef.current) return;
    restoredPersistedDocumentsRef.current = true;
    let active = true;
    void listWorkDocumentStateSummaries()
      .then(async (summaries) => {
        const recent = summaries.slice(0, 8).reverse();
        for (const summary of recent) {
          if (!active) return;
          const storedDocument = await loadWorkDocumentState(summary.id);
          if (active && storedDocument) {
            addWorkDocument(storedDocument);
          }
        }
      })
      .catch(() => {
        if (active) {
          setPanelError(
            'Les documents SQLite existants n’ont pas pu être restaurés automatiquement.'
          );
        }
      });
    return () => {
      active = false;
    };
  }, [addWorkDocument, documents.length, open]);

  useEffect(() => {
    if (!currentDocumentLoadId || hydratedDocumentIdsRef.current.has(currentDocumentLoadId)) return;
    hydratedDocumentIdsRef.current.add(currentDocumentLoadId);
    let active = true;
    void loadWorkDocumentState(currentDocumentLoadId)
      .then(async (storedDocument) => {
        if (!active) return;
        if (storedDocument) {
          updateWorkDocument(currentDocumentLoadId, storedDocument);
          if (storedDocument.sourceMarkdown) return;
        }
        if (currentDocumentSourceMarkdown) return;
        const sourceMarkdown = await loadWorkDocumentSource(currentDocumentLoadId);
        if (active && sourceMarkdown) {
          updateWorkDocument(currentDocumentLoadId, { sourceMarkdown });
        }
      })
      .catch(() => {
        if (active) {
          setPanelError(
            'Le document source enrichi n’a pas pu être rechargé. Les questions restent disponibles.'
          );
        }
      });
    return () => {
      active = false;
    };
  }, [currentDocumentLoadId, currentDocumentSourceMarkdown, updateWorkDocument]);

  const persistWorkDocumentState = useCallback((documentId: string) => {
    const snapshot = useChatStore
      .getState()
      .workDocuments.find((document) => document.id === documentId);
    if (!snapshot) return;
    void saveWorkDocumentState(snapshot).catch(() => {
      setPanelError(
        (current) =>
          current ??
          'Le document reste utilisable, mais sa sauvegarde SQLite a échoué. Relance le backend puis réessaie.'
      );
    });
  }, []);

  const finalizeImportedWorkDocument = useCallback(
    async (result: WorkdocExtractResult, repo: string | null, repoName: string | null) => {
      const document = createWorkDocument(result, repo, repoName);
      try {
        await saveWorkDocumentSource(document.id, document.sourceMarkdown);
      } catch {
        setPanelError(
          'Document importé, mais la sauvegarde locale du document source complet a échoué.'
        );
      }
      try {
        await saveWorkDocumentState(document);
      } catch {
        setPanelError(
          'Document importé, mais la sauvegarde SQLite du document de travail a échoué.'
        );
      }
      addWorkDocument(document);
    },
    [addWorkDocument]
  );

  const importFile = useCallback(
    async (file: File) => {
      setPanelError(null);
      setPendingQuestionGroupImport(null);
      setBusyImport(true);
      try {
        const result = await mcpClient.extractWorkDocument(file);
        if (result.questions.length === 0) {
          setPanelError(
            "Aucune question détectée dans ce document. Vérifie qu'il contient des titres Q1.1, Question 1 ou des phrases interrogatives."
          );
          return;
        }
        const selectableGroups = selectableWorkdocQuestionGroups(result);
        if (selectableGroups.length > 1) {
          setPendingQuestionGroupImport({
            result,
            groups: selectableGroups,
            repo: selectedRepo,
            repoName: selectedRepoName,
          });
          return;
        }
        await finalizeImportedWorkDocument(result, selectedRepo, selectedRepoName);
      } catch (error) {
        setPanelError(error instanceof Error ? error.message : String(error));
      } finally {
        setBusyImport(false);
      }
    },
    [finalizeImportedWorkDocument, selectedRepo, selectedRepoName]
  );

  const selectQuestionGroupImport = useCallback(
    (group: WorkdocQuestionGroup) => {
      const pending = pendingQuestionGroupImport;
      if (!pending) return;
      setPanelError(null);
      const filteredResult = filterWorkdocExtractResultByGroup(pending.result, group);
      setPendingQuestionGroupImport(null);
      void finalizeImportedWorkDocument(filteredResult, pending.repo, pending.repoName);
    },
    [finalizeImportedWorkDocument, pendingQuestionGroupImport]
  );

  const onFileChange = (event: ChangeEvent<HTMLInputElement>) => {
    const file = event.target.files?.[0] ?? null;
    event.target.value = '';
    if (!file) return;
    void importFile(file);
  };

  const answerQuestion = useCallback(
    async (
      documentId: string,
      questionId: string,
      runContext?: AnswerRunContext
    ): Promise<boolean> => {
      const state = useChatStore.getState();
      const document = state.workDocuments.find((item) => item.id === documentId);
      const question = document?.questions.find((item) => item.id === questionId);
      if (!document || !question) return false;

      const activeRepo = runContext?.repository ?? state.selectedRepo;
      if (!activeRepo) {
        setPanelError('Sélectionne un projet dans le chat React avant de lancer le traitement.');
        return false;
      }

      const activeRepoName = runContext?.repositoryName ?? state.selectedRepoName ?? activeRepo;
      if (document.repo !== activeRepo || document.repoName !== activeRepoName) {
        updateWorkDocument(documentId, {
          repo: activeRepo,
          repoName: activeRepoName,
        });
      }

      const prompt = buildWorkQuestionPrompt({
        document,
        question,
        repositoryName: activeRepoName,
      });
      updateQuestion(documentId, questionId, {
        status: 'answering',
        error: undefined,
        prompt,
      });

      const sessionTitle = `Atelier ${document.filename}`;
      const sessionId = document.sessionId ?? state.createSession(sessionTitle);
      if (!document.sessionId) {
        updateWorkDocument(documentId, { sessionId });
      }

      const result = await sendMessage(prompt, {
        sessionId,
        title: sessionTitle,
        includeHistory: false,
        repository: activeRepo,
      });
      if (!result) {
        updateQuestion(documentId, questionId, {
          status: 'error',
          error: 'Le chat est déjà occupé ou la question est vide.',
        });
        persistWorkDocumentState(documentId);
        return false;
      }

      updateWorkDocument(documentId, { sessionId: result.sessionId });

      updateQuestion(documentId, questionId, {
        status: result.ok ? 'answered' : 'error',
        answer: result.ok ? normalizeWorkDocumentAnswer(result.content) : result.content,
        error: result.ok ? undefined : 'La génération a échoué ou a été interrompue.',
        answeredAt: Date.now(),
        messageIds: {
          user: result.userMessageId,
          assistant: result.assistantMessageId,
        },
      });
      persistWorkDocumentState(documentId);
      return result.ok;
    },
    [persistWorkDocumentState, sendMessage, updateQuestion, updateWorkDocument]
  );

  const answerAll = useCallback(async () => {
    if (!currentDocument || batchRunning) return;
    const state = useChatStore.getState();
    if (!state.selectedRepo) {
      setPanelError('Sélectionne un projet dans le chat React avant de lancer le traitement.');
      return;
    }
    const runContext: AnswerRunContext = {
      repository: state.selectedRepo,
      repositoryName: state.selectedRepoName ?? state.selectedRepo,
    };
    batchAbortRef.current = false;
    setBatchRunning(true);
    setPanelError(null);
    setBatchNotice(null);
    setActiveBatchQuestionLabel(null);
    const remainingQuestions = currentDocument.questions.filter(
      (question) => question.status !== 'answered'
    );
    const total =
      batchLimit === 'all'
        ? remainingQuestions.length
        : Math.min(batchLimit, remainingQuestions.length);
    const plannedQuestionIds = new Set(
      remainingQuestions.slice(0, total).map((question) => question.id)
    );
    setBatchProgress({
      total,
      processed: 0,
      answered: 0,
      failed: 0,
      currentLabel: null,
    });
    let consecutiveFailures = 0;
    let attempted = 0;
    let answered = 0;
    let failed = 0;
    try {
      for (const question of currentDocument.questions) {
        if (batchAbortRef.current) break;
        if (!plannedQuestionIds.has(question.id)) continue;
        const latest = useChatStore
          .getState()
          .workDocuments.find((document) => document.id === currentDocument.id)
          ?.questions.find((item) => item.id === question.id);
        if (!latest || latest.status === 'answered') continue;
        attempted += 1;
        setActiveBatchQuestionLabel(latest.label);
        setBatchProgress({
          total,
          processed: attempted - 1,
          answered,
          failed,
          currentLabel: latest.label,
        });
        const ok = await answerQuestion(currentDocument.id, latest.id, runContext);
        if (!ok) {
          failed += 1;
          consecutiveFailures += 1;
          setBatchProgress({
            total,
            processed: attempted,
            answered,
            failed,
            currentLabel: latest.label,
          });
          if (consecutiveFailures >= 3) {
            setPanelError(
              'Traitement arrêté après 3 échecs consécutifs. Les questions déjà traitées restent sauvegardées; relance le lot après correction.'
            );
            break;
          }
          continue;
        }
        answered += 1;
        consecutiveFailures = 0;
        setBatchProgress({
          total,
          processed: attempted,
          answered,
          failed,
          currentLabel: latest.label,
        });
      }
    } finally {
      const stopped = batchAbortRef.current;
      setBatchRunning(false);
      batchAbortRef.current = false;
      setActiveBatchQuestionLabel(null);
      setBatchProgress((progress) =>
        progress
          ? {
              ...progress,
              processed: attempted,
              answered,
              failed,
              currentLabel: null,
            }
          : null
      );
      const latestDocument = useChatStore
        .getState()
        .workDocuments.find((document) => document.id === currentDocument.id);
      const remainingTodoAfter =
        latestDocument?.questions.filter(
          (question) => question.status === 'pending' || question.status === 'answering'
        ).length ?? 0;
      setBatchNotice(
        stopped
          ? `Traitement interrompu: ${answered} réponse(s), ${failed} échec(s).`
          : attempted === 0
            ? 'Aucune question restante à traiter.'
            : remainingTodoAfter > 0
              ? `Lot terminé: ${answered} réponse(s), ${failed} échec(s). ${remainingTodoAfter} question(s) restante(s).`
              : `Traitement terminé: ${answered} réponse(s), ${failed} échec(s).`
      );
    }
  }, [answerQuestion, batchLimit, batchRunning, currentDocument]);

  const stopBatch = useCallback(() => {
    batchAbortRef.current = true;
    setBatchNotice('Arrêt demandé, la question en cours va se terminer ou être annulée.');
    cancel();
  }, [cancel]);

  const copyLivrable = useCallback(async () => {
    if (!currentDocument) return;
    const ok = await copyTextToClipboard(buildWorkDocumentMarkdown(currentDocument));
    setCopyState(ok ? 'copied' : 'failed');
    window.setTimeout(() => setCopyState('idle'), 1800);
  }, [currentDocument]);

  const copyQuestionAnswer = useCallback(async (question: WorkDocumentQuestion) => {
    if (!question.answer?.trim()) return;
    const ok = await copyTextToClipboard(question.answer);
    if (ok) {
      setCopiedQuestionId(question.id);
      window.setTimeout(() => setCopiedQuestionId(null), 1600);
    } else {
      setPanelError('Copie de la réponse impossible dans ce navigateur.');
    }
  }, []);

  const downloadLivrable = useCallback(() => {
    if (!currentDocument) return;
    downloadTextFile(
      workDocumentExportFilename(currentDocument),
      buildWorkDocumentMarkdown(currentDocument)
    );
  }, [currentDocument]);

  const downloadHtmlLivrable = useCallback(() => {
    if (!currentDocument) return;
    downloadTextFile(
      workDocumentExportFilenameFor(currentDocument, 'html'),
      buildWorkDocumentPrintableHtml(currentDocument)
    );
  }, [currentDocument]);

  const downloadDocxLivrable = useCallback(async () => {
    if (!currentDocument || exportBusy) return;
    setPanelError(null);
    setExportBusy('docx');
    try {
      const filename = workDocumentExportFilenameFor(currentDocument, 'docx');
      const blob = await mcpClient.exportWorkDocumentDocx({
        filename,
        title: workDocumentExportTitle(currentDocument),
        markdown: buildWorkDocumentMarkdown(currentDocument),
      });
      downloadBlobFile(
        filename,
        blob,
        'application/vnd.openxmlformats-officedocument.wordprocessingml.document'
      );
    } catch (error) {
      setPanelError(error instanceof Error ? error.message : String(error));
    } finally {
      setExportBusy(null);
    }
  }, [currentDocument, exportBusy]);

  const downloadPdfLivrable = useCallback(async () => {
    if (!currentDocument || exportBusy) return;
    setPanelError(null);
    setExportBusy('pdf');
    try {
      const filename = workDocumentExportFilenameFor(currentDocument, 'pdf');
      const blob = await mcpClient.exportPdf({
        filename,
        html: buildWorkDocumentPrintableHtml(currentDocument),
      });
      downloadBlobFile(filename, blob, 'application/pdf');
    } catch (error) {
      setPanelError(error instanceof Error ? error.message : String(error));
    } finally {
      setExportBusy(null);
    }
  }, [currentDocument, exportBusy]);

  const removeCurrentDocument = useCallback(() => {
    if (!currentDocument) return;
    const documentId = currentDocument.id;
    deleteWorkDocument(documentId);
    void Promise.all([
      deleteWorkDocumentSource(documentId),
      deleteWorkDocumentState(documentId),
    ]).catch(() => {
      setPanelError(
        'Le document a été retiré, mais un cache local ou SQLite n’a pas pu être supprimé.'
      );
    });
  }, [currentDocument, deleteWorkDocument]);

  const openDocumentSession = useCallback(() => {
    const sessionId = currentDocument?.sessionId;
    if (!sessionId) return;
    const exists = useChatStore.getState().sessions.some((session) => session.id === sessionId);
    if (!exists) {
      setPanelError("Le fil de chat de cet atelier n'est plus disponible.");
      return;
    }
    selectSession(sessionId);
    setOpen(false);
  }, [currentDocument?.sessionId, selectSession, setOpen]);

  if (!open) return null;

  return (
    <aside
      className={clsx(
        'flex w-full flex-col bg-[#f8fafc] shadow-2xl dark:bg-[var(--panel-bg-strong)]',
        isFullscreen
          ? 'fixed inset-0 z-50 h-screen max-w-none'
          : 'absolute right-0 top-14 z-30 h-[calc(100%-3.5rem)] max-w-5xl border-l border-[var(--border)]'
      )}
      aria-label="Documents de travail"
    >
      <header className="flex items-center gap-3 border-b border-[var(--border)] bg-white px-5 py-4 dark:bg-[var(--panel-bg-strong)]">
        <div className="flex h-9 w-9 items-center justify-center rounded-md border border-[#d8dee9] bg-[#f7f9fc] text-[#1f4e79] dark:border-[var(--border)] dark:bg-[var(--panel-bg-muted)] dark:text-[var(--accent)]">
          <BookOpen className="h-4 w-4" aria-hidden />
        </div>
        <div className="min-w-0">
          <p className="text-[10px] font-semibold uppercase tracking-[0.18em] text-[#8a94a3]">
            Documentation technique
          </p>
          <h2 className="truncate font-serif text-lg font-semibold text-[#1f4e79] dark:text-[var(--text-primary)]">
            Atelier Word Code Explorer
          </h2>
          <p className="truncate text-xs text-[#475569] dark:text-[var(--text-muted)]">
            Questions extraites, réponses vérifiées et livre technique final
          </p>
        </div>
        <button
          type="button"
          onClick={() => setIsFullscreen((value) => !value)}
          className="control-button ml-auto flex h-8 w-8 items-center justify-center rounded-md border"
          aria-label={
            isFullscreen
              ? 'Réduire l’atelier Word DOCX'
              : 'Afficher l’atelier Word DOCX en plein écran'
          }
          title={isFullscreen ? 'Réduire' : 'Plein écran'}
          aria-pressed={isFullscreen}
        >
          {isFullscreen ? (
            <Minimize2 className="h-4 w-4" aria-hidden />
          ) : (
            <Maximize2 className="h-4 w-4" aria-hidden />
          )}
        </button>
        <button
          type="button"
          onClick={close}
          className="control-button flex h-8 w-8 items-center justify-center rounded-md border"
          aria-label="Fermer le panneau Documents"
          title="Fermer"
        >
          <X className="h-4 w-4" aria-hidden />
        </button>
      </header>

      <div className="flex flex-wrap items-center gap-2 border-b border-[var(--border)] bg-white px-5 py-3 dark:bg-[var(--panel-bg-strong)]">
        <input
          ref={inputRef}
          type="file"
          accept=".docx,application/vnd.openxmlformats-officedocument.wordprocessingml.document"
          className="hidden"
          onChange={onFileChange}
        />
        <button
          type="button"
          onClick={() => inputRef.current?.click()}
          disabled={busyImport}
          className="primary-action inline-flex items-center gap-2 rounded-md border px-3 py-2 text-xs font-medium disabled:cursor-not-allowed disabled:opacity-60"
        >
          {busyImport ? (
            <Loader2 className="h-4 w-4 animate-spin" />
          ) : (
            <Upload className="h-4 w-4" />
          )}
          Importer un DOCX
        </button>
        <button
          type="button"
          onClick={batchRunning ? stopBatch : answerAll}
          disabled={
            !currentDocument || (!batchRunning && (isStreaming || remainingQuestionCount === 0))
          }
          title="Lance les questions restantes une par une dans le chat React"
          className="control-button inline-flex items-center gap-2 rounded-md border px-3 py-2 text-xs disabled:cursor-not-allowed disabled:opacity-50"
        >
          {batchRunning ? <Square className="h-4 w-4" /> : <Play className="h-4 w-4" />}
          {batchButtonLabel(batchRunning, remainingQuestionCount, batchLimit)}
        </button>
        <div
          className="inline-flex items-center rounded-md border border-[#d8dee9] bg-[#f8fafc] p-0.5 text-xs dark:border-[var(--border)] dark:bg-[var(--panel-bg-muted)]"
          aria-label="Taille du prochain lot"
        >
          {BATCH_LIMITS.map((limit) => (
            <button
              key={String(limit)}
              type="button"
              onClick={() => setBatchLimit(limit)}
              disabled={batchRunning}
              aria-pressed={batchLimit === limit}
              aria-label={`Lot de ${batchLimitOptionLabel(limit)}`}
              className={clsx(
                'rounded px-2 py-1 font-medium transition-colors disabled:cursor-not-allowed disabled:opacity-50',
                batchLimit === limit
                  ? 'bg-[#1f4e79] text-white'
                  : 'text-[#475569] hover:bg-white dark:text-[var(--text-muted)] dark:hover:bg-[var(--panel-bg)]'
              )}
            >
              {batchLimitOptionLabel(limit)}
            </button>
          ))}
        </div>
        <button
          type="button"
          onClick={downloadLivrable}
          disabled={!currentDocument || stats.answered === 0}
          className="control-button ml-auto inline-flex items-center gap-2 rounded-md border px-3 py-2 text-xs disabled:cursor-not-allowed disabled:opacity-50"
        >
          <Download className="h-4 w-4" />
          Exporter MD
        </button>
        <button
          type="button"
          onClick={downloadHtmlLivrable}
          disabled={!currentDocument || stats.answered === 0}
          className="control-button inline-flex items-center gap-2 rounded-md border px-3 py-2 text-xs disabled:cursor-not-allowed disabled:opacity-50"
          title="Exporter le HTML imprimable utilisé pour le PDF"
        >
          <Download className="h-4 w-4" />
          HTML
        </button>
        <button
          type="button"
          onClick={() => void downloadDocxLivrable()}
          disabled={!currentDocument || stats.answered === 0 || exportBusy !== null}
          className="control-button inline-flex items-center gap-2 rounded-md border px-3 py-2 text-xs disabled:cursor-not-allowed disabled:opacity-50"
        >
          {exportBusy === 'docx' ? (
            <Loader2 className="h-4 w-4 animate-spin" />
          ) : (
            <Download className="h-4 w-4" />
          )}
          DOCX
        </button>
        <button
          type="button"
          onClick={() => void downloadPdfLivrable()}
          disabled={!currentDocument || stats.answered === 0 || exportBusy !== null}
          className="control-button inline-flex items-center gap-2 rounded-md border px-3 py-2 text-xs disabled:cursor-not-allowed disabled:opacity-50"
        >
          {exportBusy === 'pdf' ? (
            <Loader2 className="h-4 w-4 animate-spin" />
          ) : (
            <Download className="h-4 w-4" />
          )}
          PDF
        </button>
      </div>

      {pendingQuestionGroupImport && (
        <div
          role="dialog"
          aria-label="Choisir le groupe de questions"
          className="mx-4 mt-3 border border-[#bfdbfe] bg-white px-4 py-3 text-xs shadow-sm dark:border-[var(--border)] dark:bg-[var(--panel-bg)]"
        >
          <div className="flex items-start gap-3">
            <AlertCircle className="mt-0.5 h-4 w-4 shrink-0 text-[#1f4e79]" aria-hidden />
            <div className="min-w-0 flex-1">
              <p className="font-semibold text-[#1f4e79] dark:text-[var(--text-primary)]">
                Plusieurs groupes de questions détectés
              </p>
              <p className="mt-1 text-[var(--text-muted)]">
                Choisis le groupe à traiter pour éviter de mélanger anciennes et nouvelles
                questions.
              </p>
              <div className="mt-3 flex flex-wrap gap-2">
                {pendingQuestionGroupImport.groups.map((group) => (
                  <button
                    key={group.id}
                    type="button"
                    onClick={() => selectQuestionGroupImport(group)}
                    className="control-button inline-flex items-center gap-2 rounded-md border px-3 py-2 text-xs font-medium"
                    aria-label={`Importer le groupe ${group.label} avec ${group.questionCount} question(s)`}
                  >
                    <span
                      className="h-3 w-3 rounded-sm border border-black/10"
                      style={{ background: workdocQuestionGroupSwatch(group) }}
                      aria-hidden
                    />
                    {group.label} · {group.questionCount} question(s)
                  </button>
                ))}
              </div>
            </div>
            <button
              type="button"
              onClick={() => setPendingQuestionGroupImport(null)}
              className="control-button flex h-7 w-7 shrink-0 items-center justify-center rounded-md border"
              aria-label="Annuler le choix du groupe de questions"
              title="Annuler"
            >
              <X className="h-3.5 w-3.5" aria-hidden />
            </button>
          </div>
        </div>
      )}

      {panelError && (
        <div className="mx-4 mt-3 rounded-md border border-red-300 bg-red-50 px-3 py-2 text-xs text-red-700">
          {panelError}
        </div>
      )}

      {(batchNotice || activeBatchQuestionLabel) && (
        <div className="mx-4 mt-3 rounded-md border border-[#bfdbfe] bg-[#eff6ff] px-3 py-2 text-xs text-[#1e3a8a] dark:border-[var(--border)] dark:bg-[var(--panel-bg-muted)] dark:text-[var(--text-secondary)]">
          {batchRunning && activeBatchQuestionLabel
            ? batchProgressLabel(batchProgress, activeBatchQuestionLabel)
            : batchNotice}
          {batchProgress && (
            <div className="mt-2">
              <div className="h-1.5 overflow-hidden rounded-full bg-[#bfdbfe] dark:bg-[var(--border)]">
                <div
                  className="h-full rounded-full bg-[#1f4e79] transition-all"
                  style={{ width: `${batchProgressPercent(batchProgress)}%` }}
                />
              </div>
              <div className="mt-1 flex flex-wrap gap-x-3 gap-y-1 text-[11px] text-[#475569] dark:text-[var(--text-muted)]">
                <span>
                  {batchProgress.processed}/{batchProgress.total} traitée(s)
                </span>
                <span>{batchProgress.answered} réponse(s)</span>
                <span>{batchProgress.failed} échec(s)</span>
              </div>
            </div>
          )}
        </div>
      )}

      {!currentDocument ? (
        <EmptyState />
      ) : (
        <div className="flex min-h-0 flex-1">
          <nav
            className={clsx(
              'shrink-0 overflow-y-auto border-r border-[var(--border)] bg-[#f3f6fa] p-3 dark:bg-[var(--panel-bg-muted)]',
              isFullscreen ? 'w-72' : 'w-52'
            )}
          >
            <p className="mb-2 text-[11px] font-medium uppercase tracking-[0.12em] text-[var(--text-muted)]">
              Documents
            </p>
            <div className="space-y-2">
              {documents.map((document) => (
                <button
                  key={document.id}
                  type="button"
                  onClick={() => selectWorkDocument(document.id)}
                  className={clsx(
                    'w-full rounded-md border px-3 py-2 text-left text-xs',
                    document.id === currentDocument.id
                      ? 'border-[#1f4e79] bg-white text-[#1f4e79] shadow-sm dark:border-[var(--accent)] dark:bg-[var(--panel-bg)] dark:text-[var(--text-primary)]'
                      : 'border-[#d8dee9] bg-white/70 text-[var(--text-secondary)] hover:bg-white dark:border-[var(--border)] dark:bg-[var(--panel-bg)] dark:hover:bg-[var(--panel-bg-muted)]'
                  )}
                >
                  <span className="block truncate font-medium">{document.filename}</span>
                  <span className="mt-1 block text-[11px] text-[var(--text-muted)]">
                    {documentStats(document).answered}/{document.questions.length} réponses
                  </span>
                  <span
                    className={clsx(
                      'mt-2 inline-flex rounded px-1.5 py-0.5 text-[10px] font-semibold',
                      qualityBadgeClass(documentStats(document).quality.level)
                    )}
                  >
                    {workDocumentReadinessLabel(document)}
                  </span>
                </button>
              ))}
            </div>
          </nav>

          <section className="flex min-w-0 flex-1 flex-col">
            <div className="border-b border-[var(--border)] bg-white px-5 py-4 dark:bg-[var(--panel-bg-strong)]">
              <div className="flex items-start gap-3">
                <div className="min-w-0 flex-1">
                  <h3 className="truncate font-serif text-lg font-semibold text-[#1f4e79] dark:text-[var(--text-primary)]">
                    {currentDocument.filename}
                  </h3>
                  <p className="mt-1 text-xs text-[var(--text-muted)]">
                    {currentDocument.questions.length} question(s), {stats.answered} réponse(s),{' '}
                    {stats.errors} erreur(s)
                  </p>
                  <div className="mt-2 flex flex-wrap items-center gap-2">
                    <span
                      className={clsx(
                        'inline-flex rounded-md px-2 py-1 text-[11px] font-semibold',
                        qualityBadgeClass(stats.quality.level)
                      )}
                    >
                      {workDocumentReadinessLabel(currentDocument)}
                    </span>
                    {stats.quality.issues.slice(0, 2).map((issue) => (
                      <span
                        key={issue.id}
                        className="rounded-md border border-[var(--border)] bg-[var(--panel-bg-muted)] px-2 py-1 text-[11px] text-[var(--text-muted)]"
                      >
                        {issue.label}
                      </span>
                    ))}
                    <span className="rounded-md border border-[var(--border)] bg-[var(--panel-bg-muted)] px-2 py-1 text-[11px] text-[var(--text-muted)]">
                      {stats.quality.summary.sourceFiles} fichier(s) source
                    </span>
                    <span className="rounded-md border border-[var(--border)] bg-[var(--panel-bg-muted)] px-2 py-1 text-[11px] text-[var(--text-muted)]">
                      {stats.quality.summary.diagrams} diagramme(s)
                    </span>
                    <span className="rounded-md border border-[var(--border)] bg-[var(--panel-bg-muted)] px-2 py-1 text-[11px] text-[var(--text-muted)]">
                      {stats.quality.summary.codeBlocks} bloc(s) code
                    </span>
                  </div>
                </div>
                <button
                  type="button"
                  onClick={copyLivrable}
                  disabled={stats.answered === 0}
                  className="control-button flex h-8 w-8 items-center justify-center rounded-md border disabled:opacity-50"
                  aria-label="Copier le livrable Markdown"
                  title={copyState === 'copied' ? 'Copié' : 'Copier le livrable Markdown'}
                >
                  <ClipboardCopy className="h-4 w-4" />
                </button>
                {currentDocument.sessionId && (
                  <button
                    type="button"
                    onClick={openDocumentSession}
                    className="control-button flex h-8 items-center gap-1 rounded-md border px-2 text-xs"
                    aria-label="Ouvrir le fil chat de ce document"
                    title="Ouvrir le fil chat de ce document"
                  >
                    <MessageSquareText className="h-4 w-4" />
                    Chat
                  </button>
                )}
                <button
                  type="button"
                  onClick={removeCurrentDocument}
                  className="control-button flex h-8 w-8 items-center justify-center rounded-md border"
                  aria-label="Supprimer ce document de travail"
                  title="Supprimer"
                >
                  <Trash2 className="h-4 w-4" />
                </button>
              </div>
              {copyState === 'copied' && (
                <p className="mt-2 text-xs text-emerald-600">Livrable copié.</p>
              )}
              {copyState === 'failed' && (
                <p className="mt-2 text-xs text-red-600">Copie impossible dans ce navigateur.</p>
              )}
            </div>

            <div className="min-h-0 flex-1 overflow-y-auto bg-[#eef3f8] p-5 dark:bg-[var(--app-bg)]">
              <BookPreview document={currentDocument} stats={stats} />
              <div className="mt-5 flex flex-wrap items-center gap-2 border-b border-[#d8dee9] pb-3 text-xs dark:border-[var(--border)]">
                {QUESTION_FILTERS.map((filter) => (
                  <button
                    key={filter}
                    type="button"
                    onClick={() => setQuestionFilter(filter)}
                    className={clsx(
                      'rounded-md border px-3 py-1.5 font-medium transition-colors',
                      questionFilter === filter
                        ? 'border-[#1f4e79] bg-[#1f4e79] text-white'
                        : 'border-[#d8dee9] bg-white text-[#334155] hover:border-[#1f4e79] dark:border-[var(--border)] dark:bg-[var(--panel-bg)] dark:text-[var(--text-secondary)]'
                    )}
                    aria-pressed={questionFilter === filter}
                  >
                    {questionFilterLabel(filter)} ·{' '}
                    {filterQuestions(currentDocument.questions, filter).length}
                  </button>
                ))}
              </div>
              <div className="mt-5 space-y-3">
                {visibleQuestions.map((question) => (
                  <QuestionCard
                    key={question.id}
                    question={question}
                    busy={batchRunning || isStreaming || question.status === 'answering'}
                    copied={copiedQuestionId === question.id}
                    onAnswer={() => answerQuestion(currentDocument.id, question.id)}
                    onCopyAnswer={() => void copyQuestionAnswer(question)}
                    onOpenSourceReference={onOpenSourceReference ? openQuestionSource : undefined}
                    validSourcePaths={validSourcePaths}
                    sourceValidationState={sourceValidationState}
                  />
                ))}
                {visibleQuestions.length === 0 && (
                  <div className="rounded-md border border-[#d8dee9] bg-white px-4 py-6 text-center text-xs text-[var(--text-muted)] dark:border-[var(--border)] dark:bg-[var(--panel-bg)]">
                    Aucune question dans ce filtre.
                  </div>
                )}
              </div>
            </div>
          </section>
        </div>
      )}
    </aside>
  );
}

function createWorkDocument(
  result: WorkdocExtractResult,
  repo: string | null,
  repoName: string | null
): WorkDocument {
  return {
    id: newId(),
    filename: result.document.filename,
    importedAt: Date.now(),
    repo,
    repoName,
    sourceBytes: result.document.bytes,
    markdownChars: result.document.markdownChars,
    sourceMarkdown: result.sourceMarkdown,
    questions: result.questions.map((question) => ({
      id: question.id,
      order: question.order,
      label: question.label,
      text: question.text,
      context: question.context,
      status: 'pending',
    })),
  };
}

function selectableWorkdocQuestionGroups(result: WorkdocExtractResult): WorkdocQuestionGroup[] {
  const questionIds = new Set(result.questions.map((question) => question.id));
  return (result.questionGroups ?? []).filter((group) => {
    const usableIds = group.questionIds.filter((id) => questionIds.has(id));
    return group.questionCount > 0 && usableIds.length > 0;
  });
}

function filterWorkdocExtractResultByGroup(
  result: WorkdocExtractResult,
  group: WorkdocQuestionGroup
): WorkdocExtractResult {
  const groupIds = new Set(group.questionIds);
  const questions = result.questions
    .filter((question) => groupIds.has(question.id))
    .map((question, index) => ({
      ...question,
      id: `q-${String(index + 1).padStart(3, '0')}`,
      order: index + 1,
    }));

  return {
    ...result,
    document: {
      ...result.document,
      filename: workdocFilenameWithQuestionGroup(result.document.filename, group.label),
    },
    questions,
    questionGroups: [
      {
        ...group,
        questionCount: questions.length,
        questionIds: questions.map((question) => question.id),
      },
    ],
  };
}

function workdocFilenameWithQuestionGroup(filename: string, groupLabel: string): string {
  return filename.toLowerCase().endsWith('.docx')
    ? `${filename.slice(0, -5)} - ${groupLabel}.docx`
    : `${filename} - ${groupLabel}`;
}

function workdocQuestionGroupSwatch(group: WorkdocQuestionGroup): string {
  const value = group.color.value.trim().replace(/^#/, '');
  if (/^[0-9a-fA-F]{6}$/.test(value)) {
    return `#${value}`;
  }
  if (group.color.family === 'blue') return '#0070c0';
  if (group.color.family === 'green') return '#00b050';
  if (group.color.family === 'red') return '#c00000';
  if (group.color.family === 'yellow') return '#facc15';
  if (group.color.family === 'purple') return '#7030a0';
  if (group.color.family === 'orange') return '#ed7d31';
  return '#64748b';
}

function documentStats(document: WorkDocument | null) {
  const questions = document?.questions ?? [];
  const quality = document
    ? auditWorkDocumentQuality(document)
    : auditWorkDocumentQuality({
        id: 'empty',
        filename: 'document',
        importedAt: Date.now(),
        repo: null,
        repoName: null,
        sourceBytes: 0,
        markdownChars: 0,
        questions: [],
      });
  return {
    answered: questions.filter((question) => question.status === 'answered').length,
    errors: questions.filter((question) => question.status === 'error').length,
    pending: questions.filter((question) => question.status === 'pending').length,
    quality,
  };
}

function qualityBadgeClass(level: ReturnType<typeof auditWorkDocumentQuality>['level']): string {
  if (level === 'ready') return 'bg-emerald-50 text-emerald-700 ring-1 ring-emerald-200';
  if (level === 'blocked') return 'bg-red-50 text-red-700 ring-1 ring-red-200';
  return 'bg-amber-50 text-amber-700 ring-1 ring-amber-200';
}

function batchButtonLabel(running: boolean, remaining: number, limit: BatchLimit): string {
  if (running) return 'Stopper';
  if (remaining <= 0) return 'Tout traité';
  const nextBatch = limit === 'all' ? remaining : Math.min(limit, remaining);
  if (nextBatch === 1) return 'Traiter 1 question';
  return `Traiter ${nextBatch} questions`;
}

function batchLimitOptionLabel(limit: BatchLimit): string {
  return limit === 'all' ? 'Tout' : String(limit);
}

function batchProgressPercent(progress: BatchProgress): number {
  if (progress.total <= 0) return 100;
  return Math.round((progress.processed / progress.total) * 100);
}

function batchProgressLabel(progress: BatchProgress | null, fallbackLabel: string): string {
  if (!progress) return `Traitement en cours: ${fallbackLabel}`;
  const current = progress.currentLabel ?? fallbackLabel;
  return `Traitement en cours: ${current} (${progress.processed + 1}/${progress.total})`;
}

const QUESTION_FILTERS: QuestionFilter[] = ['all', 'pending', 'answered', 'error'];

function questionFilterLabel(filter: QuestionFilter): string {
  if (filter === 'pending') return 'À traiter';
  if (filter === 'answered') return 'Répondues';
  if (filter === 'error') return 'Erreurs';
  return 'Toutes';
}

function filterQuestions(
  questions: WorkDocumentQuestion[],
  filter: QuestionFilter
): WorkDocumentQuestion[] {
  if (filter === 'answered') {
    return questions.filter((question) => question.status === 'answered');
  }
  if (filter === 'error') {
    return questions.filter((question) => question.status === 'error');
  }
  if (filter === 'pending') {
    return questions.filter(
      (question) => question.status === 'pending' || question.status === 'answering'
    );
  }
  return questions;
}

function BookPreview({
  document,
  stats,
}: {
  document: WorkDocument;
  stats: ReturnType<typeof documentStats>;
}) {
  const firstQuestions = document.questions.slice(0, 6);
  const progress = document.questions.length
    ? Math.round((stats.answered / document.questions.length) * 100)
    : 0;
  return (
    <section className="border border-[#d8dee9] bg-white px-8 py-7 shadow-sm dark:border-[var(--border)] dark:bg-[var(--panel-bg-strong)]">
      <div className="mx-auto max-w-2xl">
        <p className="text-center text-[10px] font-semibold uppercase tracking-[0.28em] text-[#9aa3af]">
          Documentation technique
        </p>
        <h4 className="mt-3 text-center font-serif text-2xl font-semibold leading-tight text-[#1f4e79] dark:text-[var(--text-primary)]">
          Livrable Code Explorer - {document.filename}
        </h4>
        <div
          className={clsx(
            'mx-auto mt-3 w-fit rounded border px-3 py-1 text-[10px] font-semibold uppercase tracking-[0.14em]',
            qualityBadgeClass(stats.quality.level)
          )}
        >
          {workDocumentReadinessLabel(document)}
        </div>
        <p className="mt-2 text-center text-xs italic text-[#475569] dark:text-[var(--text-muted)]">
          Questions extraites du document Word et réponses vérifiées dans le code
        </p>
        <div className="mx-auto my-6 h-px w-28 bg-[#1f4e79]" />
        <div className="grid gap-2 border border-[#d8dee9] text-xs text-[#334155] dark:border-[var(--border)] dark:text-[var(--text-secondary)] sm:grid-cols-2">
          <div className="border-b border-[#d8dee9] px-3 py-2 font-semibold dark:border-[var(--border)]">
            Projet
          </div>
          <div className="border-b border-[#d8dee9] px-3 py-2 dark:border-[var(--border)]">
            {document.repoName ?? document.repo ?? 'non sélectionné'}
          </div>
          <div className="border-b border-[#d8dee9] px-3 py-2 font-semibold dark:border-[var(--border)]">
            Progression
          </div>
          <div className="border-b border-[#d8dee9] px-3 py-2 dark:border-[var(--border)]">
            {stats.answered}/{document.questions.length} réponses,{' '}
            {workDocumentReadinessLabel(document)}
          </div>
          <div className="px-3 py-2 font-semibold">Sources citées</div>
          <div className="px-3 py-2">{stats.quality.summary.sourceFiles} fichier(s)</div>
        </div>
        <div className="mt-5">
          <div className="mb-1 flex items-center justify-between text-[11px] font-medium text-[#475569] dark:text-[var(--text-muted)]">
            <span>Avancement du traitement</span>
            <span>{progress}%</span>
          </div>
          <div className="h-2 overflow-hidden rounded-full bg-[#e2e8f0]">
            <div className="h-full rounded-full bg-[#1f4e79]" style={{ width: `${progress}%` }} />
          </div>
        </div>
        <div className="mt-6">
          <div className="mb-2 flex items-center gap-2 text-xs font-semibold uppercase tracking-[0.12em] text-[#1f4e79] dark:text-[var(--accent)]">
            <ListChecks className="h-3.5 w-3.5" />
            Table des questions
          </div>
          <ol className="space-y-1 text-xs leading-5 text-[#1f4e79] dark:text-[var(--text-secondary)]">
            {firstQuestions.map((question) => (
              <li key={question.id} className="flex gap-2">
                <span className="w-10 shrink-0 tabular-nums">{question.label}</span>
                <span className="min-w-0 truncate">{question.text}</span>
              </li>
            ))}
          </ol>
          {document.questions.length > firstQuestions.length && (
            <p className="mt-2 text-xs text-[#64748b] dark:text-[var(--text-muted)]">
              + {document.questions.length - firstQuestions.length} question(s) dans le document
            </p>
          )}
        </div>
      </div>
    </section>
  );
}

function QuestionCard({
  question,
  busy,
  copied,
  onAnswer,
  onCopyAnswer,
  onOpenSourceReference,
  validSourcePaths,
  sourceValidationState,
}: {
  question: WorkDocumentQuestion;
  busy: boolean;
  copied: boolean;
  onAnswer: () => void;
  onCopyAnswer: () => void;
  onOpenSourceReference?: (reference: SourceReference) => void;
  validSourcePaths?: ReadonlySet<string> | null;
  sourceValidationState: SourceValidationState;
}) {
  const icon =
    question.status === 'answered' ? (
      <CheckCircle2 className="h-4 w-4 text-emerald-500" />
    ) : question.status === 'error' ? (
      <AlertCircle className="h-4 w-4 text-red-500" />
    ) : question.status === 'answering' ? (
      <Loader2 className="h-4 w-4 animate-spin text-[var(--accent)]" />
    ) : (
      <FileQuestion className="h-4 w-4 text-[var(--text-muted)]" />
    );
  const answerPreview = question.answer ? buildAnswerPreview(question.answer) : null;
  const sourceGroups = useMemo(
    () => extractQuestionSourceGroups(question.answer, validSourcePaths, sourceValidationState),
    [question.answer, sourceValidationState, validSourcePaths]
  );

  return (
    <article className="rounded-md border border-[#d8dee9] bg-white p-4 shadow-sm dark:border-[var(--border)] dark:bg-[var(--panel-bg)]">
      <div className="flex items-start gap-3">
        <div className="mt-0.5">{icon}</div>
        <div className="min-w-0 flex-1">
          <div className="flex items-center gap-2">
            <span className="rounded bg-[#e9f1f8] px-1.5 py-0.5 text-[11px] font-semibold text-[#1f4e79] dark:bg-[var(--accent-soft)] dark:text-[var(--accent)]">
              {question.label}
            </span>
            <span className="text-[11px] text-[var(--text-muted)]">#{question.order}</span>
          </div>
          <p className="mt-2 text-sm font-medium leading-5 text-[var(--text-primary)]">
            {question.text}
          </p>
          {question.context && (
            <details className="mt-2 text-xs text-[var(--text-muted)]">
              <summary className="cursor-pointer">Contexte extrait</summary>
              <p className="mt-1 whitespace-pre-wrap rounded-md bg-[var(--panel-bg-muted)] p-2">
                {question.context}
              </p>
            </details>
          )}
          {question.answer && (
            <div className="mt-3 space-y-2">
              <p
                className="line-clamp-2 text-xs leading-5 text-[var(--text-secondary)]"
                data-testid={`workdoc-answer-preview-${question.id}`}
              >
                {answerPreview}
              </p>
              <QuestionSourceButtons
                groups={sourceGroups}
                onOpenSourceReference={onOpenSourceReference}
                sourceValidationState={sourceValidationState}
              />
              <details className="rounded-md border border-[#d8dee9] bg-[#f8fafc] text-xs dark:border-[var(--border)] dark:bg-[var(--panel-bg-muted)]">
                <summary className="cursor-pointer px-3 py-2 font-medium text-[#1f4e79] dark:text-[var(--accent)]">
                  Réponse détaillée avec graphiques
                </summary>
                <div className="border-t border-[#d8dee9] px-3 py-3 dark:border-[var(--border)]">
                  <Markdown
                    onOpenSourceReference={onOpenSourceReference}
                    validSourcePaths={validSourcePaths}
                    sourceValidationState={sourceValidationState}
                  >
                    {question.answer}
                  </Markdown>
                </div>
              </details>
            </div>
          )}
          {question.error && <p className="mt-2 text-xs text-red-600">{question.error}</p>}
        </div>
        <div className="flex shrink-0 items-center gap-2">
          {question.answer && (
            <button
              type="button"
              onClick={onCopyAnswer}
              disabled={busy}
              className="control-button flex h-8 w-8 items-center justify-center rounded-md border disabled:cursor-not-allowed disabled:opacity-50"
              aria-label={`Copier la réponse ${question.label}`}
              title={copied ? 'Copié' : 'Copier la réponse'}
            >
              <ClipboardCopy className="h-3.5 w-3.5" />
            </button>
          )}
          <button
            type="button"
            onClick={onAnswer}
            disabled={busy}
            className="control-button flex h-8 items-center gap-1 rounded-md border px-2 text-xs disabled:cursor-not-allowed disabled:opacity-50"
            title="Générer une réponse Code Explorer pour cette question"
          >
            {question.status === 'answering' ? (
              <Loader2 className="h-3.5 w-3.5 animate-spin" />
            ) : (
              <Play className="h-3.5 w-3.5" />
            )}
            {question.status === 'answered'
              ? 'Relancer'
              : question.status === 'error'
                ? 'Réessayer'
                : question.status === 'answering'
                  ? 'En cours'
                  : 'Répondre'}
          </button>
        </div>
      </div>
    </article>
  );
}

function QuestionSourceButtons({
  groups,
  onOpenSourceReference,
  sourceValidationState,
}: {
  groups: SourceReferenceGroup[];
  onOpenSourceReference?: (reference: SourceReference) => void;
  sourceValidationState: SourceValidationState;
}) {
  if (groups.length === 0) return null;
  const visibleGroups = groups.slice(0, 8);
  const disabled = !onOpenSourceReference || sourceValidationState === 'pending';

  return (
    <div
      className="rounded-md border border-[#d8dee9] bg-[#f8fafc] px-3 py-2 dark:border-[var(--border)] dark:bg-[var(--panel-bg-muted)]"
      aria-label="Sources concernées par la réponse"
    >
      <div className="mb-2 flex items-center gap-1.5 text-[11px] font-semibold uppercase tracking-[0.08em] text-[#1f4e79] dark:text-[var(--accent)]">
        <FileCode2 className="h-3.5 w-3.5" aria-hidden />
        Sources concernées
      </div>
      <div className="flex flex-wrap gap-2">
        {visibleGroups.map((group) => (
          <QuestionSourceButton
            key={group.path}
            group={group}
            disabled={disabled}
            onOpenSourceReference={onOpenSourceReference}
          />
        ))}
        {groups.length > visibleGroups.length && (
          <span className="rounded-md border border-[var(--border)] bg-white px-2 py-1 text-[11px] text-[var(--text-muted)] dark:bg-[var(--panel-bg)]">
            +{groups.length - visibleGroups.length} autre(s)
          </span>
        )}
      </div>
      {sourceValidationState === 'pending' && (
        <p className="mt-2 text-[11px] text-[var(--text-muted)]">
          Validation des chemins source en cours.
        </p>
      )}
    </div>
  );
}

function QuestionSourceButton({
  group,
  disabled,
  onOpenSourceReference,
}: {
  group: SourceReferenceGroup;
  disabled: boolean;
  onOpenSourceReference?: (reference: SourceReference) => void;
}) {
  const first = group.references[0] ?? { path: group.path };
  return (
    <button
      type="button"
      onClick={() => onOpenSourceReference?.(first)}
      disabled={disabled}
      className="related-source-chip max-w-full rounded-md border px-2 py-1 text-left font-mono text-[11px] disabled:cursor-not-allowed disabled:opacity-60"
      title={`Ouvrir ${group.path} dans l'explorateur sources`}
      aria-label={`Ouvrir ${group.path} dans l'explorateur sources`}
    >
      <span className="block max-w-[18rem] truncate">{group.path}</span>
      <span className="block text-[10px] text-[var(--text-muted)]">
        {group.references.length} référence{group.references.length > 1 ? 's' : ''}
        {first.startLine ? ` · ligne ${first.startLine}` : ''}
      </span>
    </button>
  );
}

function extractQuestionSourceGroups(
  answer: string | undefined,
  validSourcePaths: ReadonlySet<string> | null | undefined,
  sourceValidationState: SourceValidationState
): SourceReferenceGroup[] {
  if (!answer) return [];
  const groups = groupSourceReferences(extractSourceReferences(answer));
  if (sourceValidationState !== 'ready' || !validSourcePaths) return groups;
  return groups.filter((group) => validSourcePaths.has(normalizeSourcePath(group.path)));
}

function normalizeSourcePath(path: string): string {
  return path.replace(/\\/g, '/').toLowerCase();
}

function buildAnswerPreview(answer: string): string {
  const withoutCodeBlocks = answer.replace(/```[\s\S]*?```/g, ' ');
  const compact = withoutCodeBlocks
    .split(/\r?\n/)
    .map((line) => line.replace(/^#{1,6}\s+/, '').trim())
    .filter((line) => line && !line.startsWith('- ') && !/^sources?$/i.test(line))
    .join(' ')
    .replace(/\s+/g, ' ')
    .trim();

  if (!compact) {
    return 'Réponse prête: ouvrir le détail pour consulter les diagrammes et les sources.';
  }

  return compact.length > 420 ? `${compact.slice(0, 420).trimEnd()}...` : compact;
}

function EmptyState() {
  return (
    <div className="flex flex-1 flex-col items-center justify-center gap-3 px-8 text-center">
      <FileQuestion className="h-9 w-9 text-[var(--accent)]" />
      <div>
        <p className="text-sm font-semibold text-[var(--text-primary)]">Aucun document importé</p>
        <p className="mt-1 max-w-sm text-xs leading-5 text-[var(--text-muted)]">
          Importe un fichier DOCX contenant des questions. Code Explorer extraira les questions et les
          traitera une par une dans la conversation courante.
        </p>
      </div>
    </div>
  );
}

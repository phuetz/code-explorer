import { useCallback, useEffect, useMemo, useState } from 'react';
import { Braces, Command, FileQuestion, FileText, MessageSquareText } from 'lucide-react';
import { ChatSidebar } from './ChatSidebar';
import { ChatMessages } from './ChatMessages';
import { ChatInput } from './ChatInput';
import { ProjectSelector } from './ProjectSelector';
import { BackendStatus } from './BackendStatus';
import { SfdDraftsPanel } from './SfdDraftsPanel';
import { WorkDocumentsPanel } from './WorkDocumentsPanel';
import { ChatExports } from './ChatExports';
import { LlmStatus } from './LlmStatus';
import { SystemDiagnostics } from './SystemDiagnostics';
import { QuickOpen } from './QuickOpen';
import { mcpClient, type FileTreeNode } from '../../api/mcp-client';
import { useChatStore } from '../../stores/chat-store';
import { useLlmConfig } from '../../hooks/use-llm-config';
import { useTheme } from '../../hooks/use-theme';
import { formatMessageTimestamp } from '../../utils/dates';
import {
  WorkspacePanel,
  type GraphTarget,
  type SourceTarget,
  type WorkspaceTab,
} from '../explorer/WorkspacePanel';
import { currentSessionSourceGroups } from '../../utils/analysis-snapshots';
import type {
  SourceReference,
  SourceReferenceGroup,
  SourceValidationState,
} from '../../utils/source-references';
import { ThemeToggle } from './ThemeToggle';

export function ChatPanel() {
  const session = useChatStore((s) => s.getCurrentSession());
  const isSfdOpen = useChatStore((s) => s.isSfdPanelOpen);
  const setSfdOpen = useChatStore((s) => s.setSfdPanelOpen);
  const isWorkDocumentsOpen = useChatStore((s) => s.isWorkDocumentsPanelOpen);
  const setWorkDocumentsOpen = useChatStore((s) => s.setWorkDocumentsPanelOpen);
  const workDocuments = useChatStore((s) => s.workDocuments);
  const selectedRepo = useChatStore((s) => s.selectedRepo);
  const selectedRepoName = useChatStore((s) => s.selectedRepoName);
  const isWorkspaceOpen = useChatStore((s) => s.isWorkspaceOpen);
  const setWorkspaceOpen = useChatStore((s) => s.setWorkspaceOpen);
  const workspaceTab = useChatStore((s) => s.workspaceTab);
  const setWorkspaceTab = useChatStore((s) => s.setWorkspaceTab);
  const [isQuickOpen, setQuickOpen] = useState(false);
  const [workspaceSeed, setWorkspaceSeed] = useState<{
    key: number;
    sourceTarget: SourceTarget | null;
    graphTarget: GraphTarget | null;
    tab: WorkspaceTab;
  }>({ key: 0, sourceTarget: null, graphTarget: null, tab: workspaceTab });
  const [sourceIndex, setSourceIndex] = useState<{
    repo: string | null;
    paths: Set<string> | null;
    error: string | null;
  }>({ repo: null, paths: null, error: null });
  const llm = useLlmConfig();
  const { theme, toggleTheme } = useTheme();
  const sessionTitle = session?.title.trim() || 'Code Explorer Chat';
  const sessionSubtitle = session
    ? `${session.messages.length} message${session.messages.length > 1 ? 's' : ''} - Dernière activité ${formatMessageTimestamp(session.updatedAt) || 'inconnue'}`
    : 'Analyse de code et recherche outillée';
  const relatedSourceGroups = useMemo(() => {
    return currentSessionSourceGroups(session);
  }, [session]);
  const sourceIndexForSelectedRepo =
    !!selectedRepo && sourceIndex.repo === selectedRepo && !sourceIndex.error;
  const sourcePathIndexReady = sourceIndexForSelectedRepo && !!sourceIndex.paths;
  const sourceValidationState: SourceValidationState =
    !selectedRepo || sourceIndex.error ? 'unavailable' : sourcePathIndexReady ? 'ready' : 'pending';
  const validSourcePaths = sourcePathIndexReady ? sourceIndex.paths : null;
  const verifiedRelatedSourceGroups = useMemo(
    () =>
      filterSourceGroupsByKnownPaths(relatedSourceGroups, validSourcePaths, sourceValidationState),
    [relatedSourceGroups, validSourcePaths, sourceValidationState]
  );
  const relatedSourcePaths = useMemo(() => {
    return verifiedRelatedSourceGroups.map((group) => group.path);
  }, [verifiedRelatedSourceGroups]);
  const relatedSourceCountLabel =
    verifiedRelatedSourceGroups.length > 0
      ? `${verifiedRelatedSourceGroups.length} fichier${verifiedRelatedSourceGroups.length > 1 ? 's' : ''} concerné${
          verifiedRelatedSourceGroups.length > 1 ? 's' : ''
        }`
      : '';
  const sourceValidationLabel =
    sourceValidationState === 'pending'
      ? 'sources en vérification'
      : sourceValidationState === 'ready'
        ? 'sources validées'
        : 'sources non vérifiées';
  const workQuestionCount = workDocuments.reduce(
    (count, document) => count + document.questions.length,
    0
  );

  useEffect(() => {
    let alive = true;
    if (!selectedRepo) {
      queueMicrotask(() => {
        if (alive) setSourceIndex({ repo: null, paths: null, error: null });
      });
      return () => {
        alive = false;
      };
    }

    queueMicrotask(() => {
      if (!alive) return;
      setSourceIndex((current) =>
        current.repo === selectedRepo && current.paths
          ? current
          : { repo: selectedRepo, paths: null, error: null }
      );
    });
    void mcpClient
      .fileTree(selectedRepo)
      .then((files) => {
        if (alive) {
          setSourceIndex({ repo: selectedRepo, paths: collectFilePathSet(files), error: null });
        }
      })
      .catch((error) => {
        if (alive) {
          setSourceIndex({
            repo: selectedRepo,
            paths: null,
            error: error instanceof Error ? error.message : String(error),
          });
        }
      });
    return () => {
      alive = false;
    };
  }, [selectedRepo]);

  const openSourceReference = useCallback(
    (reference: SourceReference) => {
      setWorkspaceTab('sources');
      setWorkspaceSeed((current) => ({
        key: current.key + 1,
        sourceTarget: {
          path: reference.path,
          startLine: reference.startLine,
          endLine: reference.endLine,
        },
        graphTarget: null,
        tab: 'sources',
      }));
      setWorkspaceOpen(true);
    },
    [setWorkspaceOpen, setWorkspaceTab]
  );

  const openSourceTarget = useCallback(
    (target: SourceTarget) => {
      setWorkspaceTab('sources');
      setWorkspaceSeed((current) => ({
        key: current.key + 1,
        sourceTarget: target,
        graphTarget: null,
        tab: 'sources',
      }));
      setWorkspaceOpen(true);
      setQuickOpen(false);
    },
    [setWorkspaceOpen, setWorkspaceTab]
  );

  const openGraphTarget = useCallback(
    (target: GraphTarget) => {
      setWorkspaceTab('graph');
      setWorkspaceSeed((current) => ({
        key: current.key + 1,
        sourceTarget: null,
        graphTarget: target,
        tab: 'graph',
      }));
      setWorkspaceOpen(true);
      setQuickOpen(false);
    },
    [setWorkspaceOpen, setWorkspaceTab]
  );

  const changeWorkspaceTab = useCallback(
    (tab: WorkspaceTab) => {
      setWorkspaceTab(tab);
      setWorkspaceSeed((current) => ({ ...current, tab }));
    },
    [setWorkspaceTab]
  );

  const detachWorkspace = useCallback(() => {
    const url = new URL(window.location.href);
    url.searchParams.set('codeExplorerPanel', 'workspace');
    url.searchParams.set('tab', workspaceSeed.tab);
    if (workspaceSeed.sourceTarget) {
      url.searchParams.set('sourcePath', workspaceSeed.sourceTarget.path);
      if (workspaceSeed.sourceTarget.startLine) {
        url.searchParams.set('startLine', String(workspaceSeed.sourceTarget.startLine));
      }
      if (workspaceSeed.sourceTarget.endLine) {
        url.searchParams.set('endLine', String(workspaceSeed.sourceTarget.endLine));
      }
    }
    if (workspaceSeed.graphTarget) {
      url.searchParams.set('nodeId', workspaceSeed.graphTarget.nodeId);
      url.searchParams.set('nodeName', workspaceSeed.graphTarget.name);
      if (workspaceSeed.graphTarget.label)
        url.searchParams.set('nodeLabel', workspaceSeed.graphTarget.label);
      if (workspaceSeed.graphTarget.filePath)
        url.searchParams.set('nodeFile', workspaceSeed.graphTarget.filePath);
      if (workspaceSeed.graphTarget.startLine)
        url.searchParams.set('nodeStart', String(workspaceSeed.graphTarget.startLine));
      if (workspaceSeed.graphTarget.endLine)
        url.searchParams.set('nodeEnd', String(workspaceSeed.graphTarget.endLine));
    }
    window.open(url.toString(), 'code-explorer-workspace', 'popup,width=1440,height=920');
  }, [workspaceSeed]);

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === 'k') {
        event.preventDefault();
        setQuickOpen(true);
      }
    };
    window.addEventListener('keydown', onKeyDown);
    return () => window.removeEventListener('keydown', onKeyDown);
  }, []);

  return (
    <div className={`theme-${theme} app-shell flex h-full w-full`}>
      <ChatSidebar />
      <main className="relative flex min-w-0 flex-1 flex-col">
        <header className="app-header flex min-h-14 items-center gap-3 border-b px-4 text-sm">
          <div className="flex min-w-0 items-center gap-3">
            <div className="brand-tile flex h-8 w-8 shrink-0 items-center justify-center rounded-md border">
              <MessageSquareText className="h-4 w-4" aria-hidden />
            </div>
            <div className="min-w-0">
              <div className="truncate font-medium text-[var(--text-primary)]">{sessionTitle}</div>
              <div className="truncate text-xs text-[var(--text-muted)]">{sessionSubtitle}</div>
            </div>
          </div>
          <div className="ml-auto flex items-center gap-2">
            <ThemeToggle theme={theme} onToggle={toggleTheme} />
            <BackendStatus />
            <LlmStatus llm={llm} />
            <SystemDiagnostics />
            <ChatExports llm={llm} />
            <ProjectSelector />
            <button
              type="button"
              onClick={() => setQuickOpen(true)}
              className="control-button flex items-center gap-1.5 rounded-md border px-2 py-1 text-xs"
              aria-label="Ouvrir la recherche rapide"
              title="Recherche rapide (Ctrl+K)"
            >
              <Command className="h-3.5 w-3.5" aria-hidden />
              <span className="hidden xl:inline">Ctrl K</span>
            </button>
            <button
              type="button"
              onClick={() => setWorkspaceOpen(!isWorkspaceOpen)}
              className={`control-button flex items-center gap-1.5 rounded-md border px-2 py-1 text-xs ${
                isWorkspaceOpen ? 'control-button-active' : ''
              }`}
              aria-pressed={isWorkspaceOpen}
              aria-label={
                isWorkspaceOpen
                  ? "Fermer l'explorateur"
                  : verifiedRelatedSourceGroups.length > 0
                    ? `Ouvrir l'explorateur sources et graphe, ${relatedSourceCountLabel}`
                    : "Ouvrir l'explorateur sources et graphe"
              }
              title={
                verifiedRelatedSourceGroups.length > 0
                  ? `Sources et graphe - ${relatedSourceCountLabel}, ${sourceValidationLabel}`
                  : `Sources et graphe - ${sourceValidationLabel}`
              }
            >
              <Braces className="h-3.5 w-3.5" aria-hidden />
              <span className="hidden sm:inline">Explorer</span>
              {verifiedRelatedSourceGroups.length > 0 && (
                <span className="related-source-header-count rounded px-1 text-[10px] tabular-nums">
                  {verifiedRelatedSourceGroups.length}
                </span>
              )}
              {sourceValidationState === 'pending' && (
                <span className="related-source-header-count rounded px-1 text-[10px]">…</span>
              )}
              <span className="sr-only">{sourceValidationLabel}</span>
            </button>
            <button
              type="button"
              onClick={() => setSfdOpen(!isSfdOpen)}
              className={`control-button flex items-center gap-1.5 rounded-md border px-2 py-1 text-xs ${
                isSfdOpen ? 'control-button-active' : ''
              }`}
              aria-pressed={isSfdOpen}
              aria-label={isSfdOpen ? 'Fermer le panneau SFD' : 'Ouvrir le panneau SFD'}
            >
              <FileText className="h-3.5 w-3.5" aria-hidden />
              <span className="hidden sm:inline">SFD</span>
            </button>
            <button
              type="button"
              onClick={() => setWorkDocumentsOpen(!isWorkDocumentsOpen)}
              className={`control-button flex items-center gap-1.5 rounded-md border px-2 py-1 text-xs ${
                isWorkDocumentsOpen ? 'control-button-active' : ''
              }`}
              aria-pressed={isWorkDocumentsOpen}
              aria-label={
                isWorkDocumentsOpen
                  ? 'Fermer l’atelier Word DOCX'
                  : 'Ouvrir l’atelier Word DOCX'
              }
              title="Atelier Word DOCX: importer un document, extraire les questions, générer les réponses"
            >
              <FileQuestion className="h-3.5 w-3.5" aria-hidden />
              <span className="hidden sm:inline">Word DOCX</span>
              {workQuestionCount > 0 && (
                <span className="related-source-header-count rounded px-1 text-[10px] tabular-nums">
                  {workQuestionCount}
                </span>
              )}
            </button>
          </div>
        </header>
        <div
          className={`app-main flex min-h-0 flex-1 ${isWorkspaceOpen ? 'app-main-with-workspace' : ''}`}
        >
          <div className="min-w-0 flex-1">
            <ChatMessages
              llm={llm.config}
              wide={isWorkspaceOpen}
              onOpenSourceReference={openSourceReference}
              validSourcePaths={validSourcePaths}
              sourceValidationState={sourceValidationState}
            />
          </div>
          {isWorkspaceOpen && (
            <WorkspacePanel
              key={workspaceSeed.key}
              initialSourceTarget={workspaceSeed.sourceTarget}
              initialGraphTarget={workspaceSeed.graphTarget}
              initialTab={workspaceSeed.tab}
              highlightedSourcePaths={relatedSourcePaths}
              relatedSourceGroups={verifiedRelatedSourceGroups}
              onTabChange={changeWorkspaceTab}
              onDetach={detachWorkspace}
              onClose={() => setWorkspaceOpen(false)}
            />
          )}
        </div>
        <ChatInput llm={llm.config} wide={isWorkspaceOpen} />
        <SfdDraftsPanel />
        <WorkDocumentsPanel
          llm={llm.config}
          onOpenSourceReference={openSourceReference}
          validSourcePaths={validSourcePaths}
          sourceValidationState={sourceValidationState}
        />
        {isQuickOpen && (
          <QuickOpen
            repo={selectedRepo}
            repoName={selectedRepoName}
            onClose={() => setQuickOpen(false)}
            onOpenSource={openSourceTarget}
            onOpenGraph={openGraphTarget}
          />
        )}
      </main>
    </div>
  );
}

function normalizeSourcePath(path: string): string {
  return path.replace(/\\/g, '/').toLowerCase();
}

function collectFilePathSet(nodes: FileTreeNode[]): Set<string> {
  const paths = new Set<string>();
  const visit = (items: FileTreeNode[]) => {
    for (const item of items) {
      if (item.isDir) {
        visit(item.children);
      } else {
        paths.add(normalizeSourcePath(item.path));
      }
    }
  };
  visit(nodes);
  return paths;
}

function filterSourceGroupsByKnownPaths(
  groups: SourceReferenceGroup[],
  knownPaths: ReadonlySet<string> | null,
  validationState: SourceValidationState
): SourceReferenceGroup[] {
  if (validationState === 'pending') return [];
  if (validationState !== 'ready' || !knownPaths) return groups;
  return groups.filter((group) => knownPaths.has(normalizeSourcePath(group.path)));
}

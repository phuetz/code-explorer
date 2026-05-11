import { useCallback, useEffect, useMemo, useState } from 'react';
import { Braces, Command, FileText, MessageSquareText } from 'lucide-react';
import { ChatSidebar } from './ChatSidebar';
import { ChatMessages } from './ChatMessages';
import { ChatInput } from './ChatInput';
import { ProjectSelector } from './ProjectSelector';
import { BackendStatus } from './BackendStatus';
import { SfdDraftsPanel } from './SfdDraftsPanel';
import { ChatExports } from './ChatExports';
import { LlmStatus } from './LlmStatus';
import { SystemDiagnostics } from './SystemDiagnostics';
import { QuickOpen } from './QuickOpen';
import { useChatStore } from '../../stores/chat-store';
import { useLlmConfig } from '../../hooks/use-llm-config';
import { useTheme } from '../../hooks/use-theme';
import { formatMessageTimestamp } from '../../utils/dates';
import { WorkspacePanel, type GraphTarget, type SourceTarget } from '../explorer/WorkspacePanel';
import { currentSessionSourceGroups } from '../../utils/analysis-snapshots';
import type { SourceReference } from '../../utils/source-references';
import { ThemeToggle } from './ThemeToggle';

export function ChatPanel() {
  const session = useChatStore((s) => s.getCurrentSession());
  const isSfdOpen = useChatStore((s) => s.isSfdPanelOpen);
  const setSfdOpen = useChatStore((s) => s.setSfdPanelOpen);
  const selectedRepo = useChatStore((s) => s.selectedRepo);
  const selectedRepoName = useChatStore((s) => s.selectedRepoName);
  const [isWorkspaceOpen, setWorkspaceOpen] = useState(false);
  const [isQuickOpen, setQuickOpen] = useState(false);
  const [workspaceSeed, setWorkspaceSeed] = useState<{
    key: number;
    sourceTarget: SourceTarget | null;
    graphTarget: GraphTarget | null;
    tab: 'sources' | 'graph';
  }>({ key: 0, sourceTarget: null, graphTarget: null, tab: 'sources' });
  const llm = useLlmConfig();
  const { theme, toggleTheme } = useTheme();
  const sessionTitle = session?.title.trim() || 'GitNexus Chat';
  const sessionSubtitle = session
    ? `${session.messages.length} message${session.messages.length > 1 ? 's' : ''} - Dernière activité ${formatMessageTimestamp(session.updatedAt) || 'inconnue'}`
    : 'Analyse de code et recherche outillée';
  const relatedSourceGroups = useMemo(() => {
    return currentSessionSourceGroups(session);
  }, [session]);
  const relatedSourcePaths = useMemo(() => {
    return relatedSourceGroups.map((group) => group.path);
  }, [relatedSourceGroups]);
  const relatedSourceCountLabel =
    relatedSourceGroups.length > 0
      ? `${relatedSourceGroups.length} fichier${relatedSourceGroups.length > 1 ? 's' : ''} concerné${
          relatedSourceGroups.length > 1 ? 's' : ''
        }`
      : '';

  const openSourceReference = useCallback((reference: SourceReference) => {
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
  }, []);

  const openSourceTarget = useCallback((target: SourceTarget) => {
    setWorkspaceSeed((current) => ({
      key: current.key + 1,
      sourceTarget: target,
      graphTarget: null,
      tab: 'sources',
    }));
    setWorkspaceOpen(true);
    setQuickOpen(false);
  }, []);

  const openGraphTarget = useCallback((target: GraphTarget) => {
    setWorkspaceSeed((current) => ({
      key: current.key + 1,
      sourceTarget: null,
      graphTarget: target,
      tab: 'graph',
    }));
    setWorkspaceOpen(true);
    setQuickOpen(false);
  }, []);

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
              onClick={() => setWorkspaceOpen((open) => !open)}
              className={`control-button flex items-center gap-1.5 rounded-md border px-2 py-1 text-xs ${
                isWorkspaceOpen ? 'control-button-active' : ''
              }`}
              aria-pressed={isWorkspaceOpen}
              aria-label={
                isWorkspaceOpen
                  ? "Fermer l'explorateur"
                  : relatedSourceGroups.length > 0
                    ? `Ouvrir l'explorateur sources et graphe, ${relatedSourceCountLabel}`
                    : "Ouvrir l'explorateur sources et graphe"
              }
              title={
                relatedSourceGroups.length > 0
                  ? `Sources et graphe - ${relatedSourceCountLabel}`
                  : 'Sources et graphe'
              }
            >
              <Braces className="h-3.5 w-3.5" aria-hidden />
              <span className="hidden sm:inline">Explorer</span>
              {relatedSourceGroups.length > 0 && (
                <span className="related-source-header-count rounded px-1 text-[10px] tabular-nums">
                  {relatedSourceGroups.length}
                </span>
              )}
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
          </div>
        </header>
        <div className="app-main flex min-h-0 flex-1">
          <div className="min-w-0 flex-1">
            <ChatMessages llm={llm.config} onOpenSourceReference={openSourceReference} />
          </div>
          {isWorkspaceOpen && (
            <WorkspacePanel
              key={workspaceSeed.key}
              initialSourceTarget={workspaceSeed.sourceTarget}
              initialGraphTarget={workspaceSeed.graphTarget}
              initialTab={workspaceSeed.tab}
              highlightedSourcePaths={relatedSourcePaths}
              relatedSourceGroups={relatedSourceGroups}
              onClose={() => setWorkspaceOpen(false)}
            />
          )}
        </div>
        <ChatInput />
        <SfdDraftsPanel />
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

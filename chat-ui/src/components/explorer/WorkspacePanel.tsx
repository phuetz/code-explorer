import { lazy, Suspense, useCallback, useEffect, useMemo, useState, type ReactNode } from 'react';
import {
  Braces,
  Check,
  ChevronDown,
  ChevronRight,
  ChevronUp,
  Copy,
  Database,
  GitFork,
  Download,
  FileCode2,
  Folder,
  GitBranch,
  Loader2,
  ListFilter,
  MessageSquarePlus,
  Search,
  X,
} from 'lucide-react';
import {
  mcpClient,
  type FileTreeNode,
  type GraphEdge,
  type GraphNode,
  type GraphPayload,
  type SourceContent,
  type SymbolSearchResult,
} from '../../api/mcp-client';
import { useChatStore } from '../../stores/chat-store';
import { copyTextToClipboard } from '../../utils/clipboard';
import type { AnalysisSnapshot } from '../../types/chat';
import { buildAnalysisSnapshot, snapshotSourceGroups } from '../../utils/analysis-snapshots';
import type { SourceReference, SourceReferenceGroup } from '../../utils/source-references';
import {
  downloadTextFile,
  relatedSourcesFilename,
  relatedSourcesMarkdown,
  sourceReferenceSummary,
} from '../../utils/related-sources-export';
import type { SourceCodeHighlightState } from './SourceCodeHighlighter';

const SourceCodeHighlighter = lazy(() =>
  import('./SourceCodeHighlighter').then((m) => ({ default: m.SourceCodeHighlighter }))
);

type WorkspaceTab = 'sources' | 'graph';

export interface SourceTarget {
  path: string;
  startLine?: number;
  endLine?: number;
}

export interface GraphTarget {
  nodeId: string;
  name: string;
  label?: string;
  filePath?: string;
  startLine?: number;
  endLine?: number;
}

interface WorkspacePanelProps {
  onClose: () => void;
  initialSourceTarget?: SourceTarget | null;
  initialGraphTarget?: GraphTarget | null;
  initialTab?: WorkspaceTab;
  highlightedSourcePaths?: string[];
  relatedSourceGroups?: SourceReferenceGroup[];
}

export function WorkspacePanel({
  onClose,
  initialSourceTarget = null,
  initialGraphTarget = null,
  initialTab,
  highlightedSourcePaths = [],
  relatedSourceGroups = [],
}: WorkspacePanelProps) {
  const currentSession = useChatStore((s) => s.getCurrentSession());
  const saveAnalysisSnapshot = useChatStore((s) => s.saveAnalysisSnapshot);
  const deleteAnalysisSnapshot = useChatStore((s) => s.deleteAnalysisSnapshot);
  const selectedRepo = useChatStore((s) => s.selectedRepo);
  const selectedRepoName = useChatStore((s) => s.selectedRepoName);
  const [tab, setTab] = useState<WorkspaceTab>(initialTab ?? (initialGraphTarget ? 'graph' : 'sources'));
  const [sourceTarget, setSourceTarget] = useState<SourceTarget | null>(initialSourceTarget);
  const [graphTarget, setGraphTarget] = useState<GraphTarget | null>(initialGraphTarget);
  const [activeAnalysisSnapshot, setActiveAnalysisSnapshot] = useState<AnalysisSnapshot | null>(null);
  const effectiveRelatedSourceGroups = useMemo(
    () => activeAnalysisSnapshot ? snapshotSourceGroups(activeAnalysisSnapshot) : relatedSourceGroups,
    [activeAnalysisSnapshot, relatedSourceGroups]
  );
  const highlightedSourcePathSet = useMemo(
    () => new Set([...highlightedSourcePaths, ...effectiveRelatedSourceGroups.map((group) => group.path)].map((path) => normalizePath(path))),
    [effectiveRelatedSourceGroups, highlightedSourcePaths]
  );

  const openSource = useCallback((target: SourceTarget) => {
    setSourceTarget(target);
    setTab('sources');
  }, []);

  const openGraph = useCallback((target: GraphTarget) => {
    setGraphTarget(target);
    setTab('graph');
  }, []);

  const saveCurrentAnalysis = useCallback(() => {
    if (!currentSession || effectiveRelatedSourceGroups.length === 0) return;
    saveAnalysisSnapshot(
      currentSession.id,
      buildAnalysisSnapshot({
        session: currentSession,
        repo: selectedRepo,
        repoName: selectedRepoName,
        sourceGroups: effectiveRelatedSourceGroups,
      })
    );
  }, [currentSession, effectiveRelatedSourceGroups, saveAnalysisSnapshot, selectedRepo, selectedRepoName]);

  const openAnalysisSnapshot = useCallback((snapshot: AnalysisSnapshot) => {
    setActiveAnalysisSnapshot(snapshot);
    setTab('sources');
  }, []);

  const deleteAnalysis = useCallback(
    (snapshotId: string) => {
      if (!currentSession) return;
      deleteAnalysisSnapshot(currentSession.id, snapshotId);
      setActiveAnalysisSnapshot((snapshot) => snapshot?.id === snapshotId ? null : snapshot);
    },
    [currentSession, deleteAnalysisSnapshot]
  );

  return (
    <aside className="flex h-full w-[min(520px,42vw)] min-w-[360px] flex-col border-l border-neutral-900 bg-neutral-950">
      <header className="flex min-h-12 items-center gap-2 border-b border-neutral-900 px-3">
        <div className="flex min-w-0 flex-1 items-center gap-2">
          <Braces className="h-4 w-4 text-violet-300" aria-hidden />
          <div className="min-w-0">
            <div className="truncate text-sm font-medium text-neutral-100">Explorateur</div>
            <div className="truncate text-xs text-neutral-500">{selectedRepoName ?? selectedRepo ?? 'Aucun projet'}</div>
          </div>
        </div>
        <button
          type="button"
          onClick={onClose}
          className="rounded-md border border-neutral-800 p-1.5 text-neutral-400 hover:bg-neutral-900 hover:text-neutral-100"
          aria-label="Fermer l'explorateur"
          title="Fermer"
        >
          <X className="h-4 w-4" aria-hidden />
        </button>
      </header>

      <div className="flex border-b border-neutral-900 px-2 py-2 text-xs">
        <WorkspaceTabButton active={tab === 'sources'} onClick={() => setTab('sources')} icon={<FileCode2 className="h-3.5 w-3.5" />}>
          Sources
        </WorkspaceTabButton>
        <WorkspaceTabButton active={tab === 'graph'} onClick={() => setTab('graph')} icon={<GitBranch className="h-3.5 w-3.5" />}>
          Graphe
        </WorkspaceTabButton>
      </div>

      {!selectedRepo ? (
        <div className="flex flex-1 items-center justify-center p-6 text-center text-sm text-neutral-500">
          Selectionne un projet indexe pour naviguer dans ses sources et son graphe.
        </div>
      ) : tab === 'sources' ? (
        <SourceExplorer
          repo={selectedRepo}
          repoLabel={selectedRepoName ?? selectedRepo}
          target={sourceTarget}
          relatedSourceGroups={effectiveRelatedSourceGroups}
          analysisSnapshots={currentSession?.analyses ?? []}
          activeAnalysisSnapshot={activeAnalysisSnapshot}
          onSaveAnalysis={saveCurrentAnalysis}
          onOpenAnalysis={openAnalysisSnapshot}
          onClearAnalysis={() => setActiveAnalysisSnapshot(null)}
          onDeleteAnalysis={deleteAnalysis}
          highlightedSourcePaths={highlightedSourcePathSet}
          onOpenGraph={openGraph}
        />
      ) : (
        <GraphNavigator repo={selectedRepo} onOpenSource={openSource} initialTarget={graphTarget} />
      )}
    </aside>
  );
}

function WorkspaceTabButton({
  active,
  onClick,
  icon,
  children,
}: {
  active: boolean;
  onClick: () => void;
  icon: ReactNode;
  children: ReactNode;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      className={`mr-1 inline-flex items-center gap-1.5 rounded-md border px-2.5 py-1.5 ${
        active
          ? 'border-neutral-700 bg-neutral-900 text-neutral-100'
          : 'border-transparent text-neutral-500 hover:bg-neutral-900 hover:text-neutral-200'
      }`}
    >
      {icon}
      {children}
    </button>
  );
}

function SourceExplorer({
  repo,
  repoLabel,
  target,
  relatedSourceGroups,
  analysisSnapshots,
  activeAnalysisSnapshot,
  highlightedSourcePaths,
  onSaveAnalysis,
  onOpenAnalysis,
  onClearAnalysis,
  onDeleteAnalysis,
  onOpenGraph,
}: {
  repo: string;
  repoLabel: string;
  target: SourceTarget | null;
  relatedSourceGroups: SourceReferenceGroup[];
  analysisSnapshots: AnalysisSnapshot[];
  activeAnalysisSnapshot: AnalysisSnapshot | null;
  highlightedSourcePaths: ReadonlySet<string>;
  onSaveAnalysis: () => void;
  onOpenAnalysis: (snapshot: AnalysisSnapshot) => void;
  onClearAnalysis: () => void;
  onDeleteAnalysis: (snapshotId: string) => void;
  onOpenGraph: (target: GraphTarget) => void;
}) {
  const setInputDraft = useChatStore((s) => s.setInputDraft);
  const [treeState, setTreeState] = useState<{
    repo: string | null;
    files: FileTreeNode[];
    error: string | null;
  }>({ repo: null, files: [], error: null });
  const [filter, setFilter] = useState('');
  const [showRelatedOnly, setShowRelatedOnly] = useState(false);
  const [manuallyExpandedPaths, setManuallyExpandedPaths] = useState<Set<string>>(new Set());
  const [manuallyCollapsedPaths, setManuallyCollapsedPaths] = useState<Set<string>>(new Set());
  const [sourceQuery, setSourceQuery] = useState('');
  const [activeMatchIndex, setActiveMatchIndex] = useState(0);
  const [outlineFocus, setOutlineFocus] = useState<{ path: string; lineNumber: number } | null>(null);
  const [pathCopied, setPathCopied] = useState(false);
  const [graphLookup, setGraphLookup] = useState<{ loading: boolean; error: string | null }>({
    loading: false,
    error: null,
  });
  const [manualTarget, setManualTarget] = useState<SourceTarget | null>(null);
  const activeTarget = manualTarget ?? target;
  const activePath = activeTarget?.path ?? null;
  const activeStartLine = activeTarget?.startLine;
  const activeEndLine = activeTarget?.endLine;
  const activeTargetKey = activePath
    ? `${repo}:${activePath}:${activeStartLine ?? ''}:${activeEndLine ?? ''}`
    : null;
  const [sourceState, setSourceState] = useState<{
    key: string | null;
    source: SourceContent | null;
    error: string | null;
  }>({ key: null, source: null, error: null });

  useEffect(() => {
    let alive = true;
    void mcpClient
      .fileTree(repo)
      .then((files) => {
        if (alive) setTreeState({ repo, files, error: null });
      })
      .catch((error) => {
        if (alive) {
          setTreeState({
            repo,
            files: [],
            error: error instanceof Error ? error.message : String(error),
          });
        }
      });
    return () => {
      alive = false;
    };
  }, [repo]);

  useEffect(() => {
    if (!activePath || !activeTargetKey) return;
    let alive = true;
    void mcpClient
      .source(repo, activePath, {
        start: activeStartLine,
        end: activeEndLine,
      })
      .then((content) => {
        if (alive) setSourceState({ key: activeTargetKey, source: content, error: null });
      })
      .catch((error) => {
        if (alive) {
          setSourceState({
            key: activeTargetKey,
            source: null,
            error: error instanceof Error ? error.message : String(error),
          });
        }
      });
    return () => {
      alive = false;
    };
  }, [
    activeEndLine,
    activePath,
    activeStartLine,
    activeTargetKey,
    repo,
  ]);

  const openFile = useCallback((path: string, startLine?: number, endLine?: number) => {
    const parents = parentFolderSet(path);
    setManuallyCollapsedPaths((current) => removeSetValues(current, parents));
    setManuallyExpandedPaths((current) => mergeSets(current, parents));
    setManualTarget({ path, startLine, endLine });
  }, []);

  const treeLoading = treeState.repo !== repo;
  const treeError = treeLoading ? null : treeState.error;
  const relatedFolderCounts = useMemo(
    () => buildRelatedFolderCounts(relatedSourceGroups),
    [relatedSourceGroups]
  );
  const autoExpandedPaths = useMemo(
    () => buildAutoExpandedFolders(treeState.files, relatedSourceGroups, activePath, true),
    [activePath, relatedSourceGroups, treeState.files]
  );
  const expandedPaths = useMemo(
    () => expandedFolderSet(autoExpandedPaths, manuallyExpandedPaths, manuallyCollapsedPaths),
    [autoExpandedPaths, manuallyCollapsedPaths, manuallyExpandedPaths]
  );
  const toggleFolder = useCallback((path: string) => {
    const key = normalizePath(path);
    if (expandedPaths.has(key)) {
      setManuallyExpandedPaths((current) => removeSetValues(current, new Set([key])));
      setManuallyCollapsedPaths((current) => mergeSets(current, new Set([key])));
    } else {
      setManuallyCollapsedPaths((current) => removeSetValues(current, new Set([key])));
      setManuallyExpandedPaths((current) => mergeSets(current, new Set([key])));
    }
  }, [expandedPaths]);
  const filteredTree = useMemo(() => filterTree(treeState.files, filter), [treeState.files, filter]);
  const displayedTree = useMemo(
    () =>
      showRelatedOnly
        ? filterTreeToRelated(filteredTree, highlightedSourcePaths)
        : sortTreeByRelated(filteredTree, highlightedSourcePaths, relatedFolderCounts),
    [filteredTree, highlightedSourcePaths, relatedFolderCounts, showRelatedOnly]
  );
  const sourceLoading = !!activeTargetKey && sourceState.key !== activeTargetKey;
  const source = sourceLoading ? null : sourceState.source;
  const sourceError = sourceLoading ? null : sourceState.error;
  const sourceRelatedGroup = useMemo(
    () => relatedSourceGroups.find((group) => normalizePath(group.path) === normalizePath(source?.path)),
    [relatedSourceGroups, source?.path]
  );
  const sourceMatches = useMemo(() => findSourceMatches(source, sourceQuery), [source, sourceQuery]);
  const sourceOutline = useMemo(() => extractSourceOutline(source), [source]);
  const safeActiveMatchIndex =
    sourceMatches.length === 0 ? 0 : Math.min(activeMatchIndex, sourceMatches.length - 1);
  const activeMatchLine = sourceMatches[safeActiveMatchIndex]?.lineNumber ?? null;
  const activeOutlineLine = source && outlineFocus?.path === source.path ? outlineFocus.lineNumber : null;
  const highlightState = useMemo<SourceCodeHighlightState>(
    () => ({
      searchLines: new Set(sourceMatches.map((match) => match.lineNumber)),
      activeLine: activeMatchLine ?? activeOutlineLine,
      targetStartLine: activeStartLine,
      targetEndLine: activeEndLine,
    }),
    [activeEndLine, activeMatchLine, activeOutlineLine, activeStartLine, sourceMatches]
  );

  const askAboutSource = () => {
    if (!source) return;
    const range =
      source.startLine && source.endLine
        ? ` lignes ${source.startLine}-${source.endLine}`
        : '';
    setInputDraft(`Explique le fichier ${source.path}${range} et ses liens avec le graphe GitNexus.`);
  };

  const copySourcePath = async () => {
    if (!source) return;
    const ok = await copyTextToClipboard(source.path);
    if (!ok) return;
    setPathCopied(true);
    window.setTimeout(() => setPathCopied(false), 1200);
  };

  const openGraphForSource = async () => {
    if (!source || graphLookup.loading) return;
    setGraphLookup({ loading: true, error: null });
    try {
      const symbols = await mcpClient.symbols(repo, symbolQueryFromPath(source.path), 25);
      const symbol = bestSymbolForSource(symbols, source);
      if (!symbol) {
        setGraphLookup({ loading: false, error: 'Aucun symbole du graphe trouvé pour ce fichier.' });
        return;
      }
      setGraphLookup({ loading: false, error: null });
      onOpenGraph({
        nodeId: symbol.nodeId,
        name: symbol.name,
        label: symbol.label,
        filePath: symbol.filePath,
        startLine: symbol.startLine,
        endLine: symbol.endLine,
      });
    } catch (err) {
      setGraphLookup({
        loading: false,
        error: err instanceof Error ? err.message : String(err),
      });
    }
  };

  const goToPreviousMatch = () => {
    if (sourceMatches.length === 0) return;
    setActiveMatchIndex((index) => (index + sourceMatches.length - 1) % sourceMatches.length);
  };

  const goToNextMatch = () => {
    if (sourceMatches.length === 0) return;
    setActiveMatchIndex((index) => (index + 1) % sourceMatches.length);
  };

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <div className="border-b border-neutral-900 p-3">
        <label className="flex items-center gap-2 rounded-md border border-neutral-800 bg-neutral-900/70 px-2 py-1.5 text-xs text-neutral-400">
          <Search className="h-3.5 w-3.5" aria-hidden />
          <input
            value={filter}
            onChange={(event) => setFilter(event.target.value)}
            placeholder="Filtrer les fichiers..."
            className="min-w-0 flex-1 bg-transparent text-neutral-100 outline-none placeholder:text-neutral-600"
          />
        </label>
      </div>
      <div className="grid min-h-0 flex-1 grid-cols-[190px_minmax(0,1fr)]">
        <div className="min-h-0 overflow-auto border-r border-neutral-900 p-2 text-xs">
          <AnalysisSnapshotsPanel
            snapshots={analysisSnapshots}
            activeSnapshot={activeAnalysisSnapshot}
            canSave={relatedSourceGroups.length > 0}
            onSave={onSaveAnalysis}
            onOpen={onOpenAnalysis}
            onClear={onClearAnalysis}
            onDelete={onDeleteAnalysis}
          />
          {relatedSourceGroups.length > 0 && (
            <ExplorerRelatedSourcesPanel
              repo={repo}
              repoLabel={repoLabel}
              groups={relatedSourceGroups}
              relatedOnly={showRelatedOnly}
              onToggleRelatedOnly={() => setShowRelatedOnly((value) => !value)}
              onOpenReference={(reference) => openFile(reference.path, reference.startLine, reference.endLine)}
            />
          )}
          {treeLoading ? (
            <LoadingLine label="Chargement des sources..." />
          ) : treeError ? (
            <ErrorText message={treeError} />
          ) : displayedTree.length === 0 ? (
            <div className="p-3 text-neutral-600">Aucun fichier.</div>
          ) : (
            <FileTree
              nodes={displayedTree}
              activePath={activePath}
              expandedPaths={expandedPaths}
              relatedFolderCounts={relatedFolderCounts}
              highlightedSourcePaths={highlightedSourcePaths}
              onToggleFolder={toggleFolder}
              onOpenFile={(path) => void openFile(path)}
            />
          )}
        </div>
        <div className="min-h-0 overflow-hidden">
          {sourceLoading ? (
            <div className="flex h-full items-center justify-center">
              <LoadingLine label="Lecture du fichier..." />
            </div>
          ) : sourceError ? (
            <div className="p-4">
              <ErrorText message={sourceError} />
            </div>
          ) : source ? (
            <div className="flex h-full flex-col">
              <div className="border-b border-neutral-900 px-3 py-2 text-xs">
                <div className="flex min-h-8 items-center gap-2">
                  <FileCode2 className="h-3.5 w-3.5 shrink-0 text-violet-300" aria-hidden />
                  <span className="min-w-0 flex-1 truncate font-mono text-neutral-200">{source.path}</span>
                  {sourceRelatedGroup && (
                    <span className="source-related-badge shrink-0 rounded px-1.5 py-0.5 text-[10px] font-medium">
                      Fichier concerné
                    </span>
                  )}
                  <span className="shrink-0 text-neutral-600">
                    {source.totalLines} lignes{source.language ? ` - ${source.language}` : ''}
                  </span>
                  <button
                    type="button"
                    onClick={() => void copySourcePath()}
                    className="rounded-md border border-neutral-800 p-1.5 text-neutral-300 hover:bg-neutral-900"
                    aria-label="Copier le chemin du fichier"
                    title={pathCopied ? 'Chemin copié' : 'Copier le chemin'}
                  >
                    <Copy className="h-3.5 w-3.5" aria-hidden />
                  </button>
                  <button
                    type="button"
                    onClick={() => void openGraphForSource()}
                    disabled={graphLookup.loading}
                    className="rounded-md border border-neutral-800 p-1.5 text-neutral-300 hover:bg-neutral-900 disabled:cursor-wait disabled:opacity-60"
                    aria-label="Ouvrir le fichier dans le graphe"
                    title="Ouvrir dans le graphe"
                  >
                    {graphLookup.loading ? (
                      <Loader2 className="h-3.5 w-3.5 animate-spin" aria-hidden />
                    ) : (
                      <GitBranch className="h-3.5 w-3.5" aria-hidden />
                    )}
                  </button>
                  <button
                    type="button"
                    onClick={askAboutSource}
                    className="rounded-md border border-neutral-800 p-1.5 text-neutral-300 hover:bg-neutral-900"
                    aria-label="Envoyer ce contexte au chat"
                    title="Envoyer ce contexte au chat"
                  >
                    <MessageSquarePlus className="h-3.5 w-3.5" aria-hidden />
                  </button>
                </div>
                <div className="mt-2 flex items-center gap-2">
                  <label className="flex min-w-0 flex-1 items-center gap-2 rounded-md border border-neutral-800 bg-neutral-900/70 px-2 py-1.5 text-neutral-400">
                    <Search className="h-3.5 w-3.5" aria-hidden />
                    <input
                      value={sourceQuery}
                      onChange={(event) => {
                        setSourceQuery(event.target.value);
                        setActiveMatchIndex(0);
                      }}
                      placeholder="Rechercher dans ce fichier..."
                      className="min-w-0 flex-1 bg-transparent text-neutral-100 outline-none placeholder:text-neutral-600"
                    />
                  </label>
                  <span className="w-14 shrink-0 text-center tabular-nums text-neutral-500">
                    {sourceQuery.trim()
                      ? `${sourceMatches.length === 0 ? 0 : safeActiveMatchIndex + 1} / ${sourceMatches.length}`
                      : '-'}
                  </span>
                  <button
                    type="button"
                    onClick={goToPreviousMatch}
                    disabled={sourceMatches.length === 0}
                    className="rounded-md border border-neutral-800 p-1.5 text-neutral-300 hover:bg-neutral-900 disabled:cursor-not-allowed disabled:opacity-40"
                    aria-label="Occurrence précédente"
                    title="Occurrence précédente"
                  >
                    <ChevronUp className="h-3.5 w-3.5" aria-hidden />
                  </button>
                  <button
                    type="button"
                    onClick={goToNextMatch}
                    disabled={sourceMatches.length === 0}
                    className="rounded-md border border-neutral-800 p-1.5 text-neutral-300 hover:bg-neutral-900 disabled:cursor-not-allowed disabled:opacity-40"
                    aria-label="Occurrence suivante"
                    title="Occurrence suivante"
                  >
                    <ChevronDown className="h-3.5 w-3.5" aria-hidden />
                  </button>
                </div>
                {graphLookup.error && <div className="mt-2 text-[11px] text-red-300">{graphLookup.error}</div>}
                {sourceRelatedGroup && (
                  <SourceRelatedReferences
                    group={sourceRelatedGroup}
                    onOpenReference={(reference) => openFile(reference.path, reference.startLine, reference.endLine)}
                  />
                )}
                <SourceGraphInsights repo={repo} source={source} onOpenGraph={onOpenGraph} />
                {sourceOutline.length > 0 && (
                  <SourceOutline
                    items={sourceOutline}
                    activeLine={activeOutlineLine}
                    onSelect={(item) => {
                      setSourceQuery('');
                      setActiveMatchIndex(0);
                      setOutlineFocus({ path: source.path, lineNumber: item.lineNumber });
                    }}
                  />
                )}
              </div>
              <SourceCode source={source} highlight={highlightState} />
            </div>
          ) : (
            <div className="flex h-full items-center justify-center p-6 text-center text-sm text-neutral-600">
              Choisis un fichier pour l'afficher ici.
            </div>
          )}
        </div>
      </div>
    </div>
  );
}

function AnalysisSnapshotsPanel({
  snapshots,
  activeSnapshot,
  canSave,
  onSave,
  onOpen,
  onClear,
  onDelete,
}: {
  snapshots: AnalysisSnapshot[];
  activeSnapshot: AnalysisSnapshot | null;
  canSave: boolean;
  onSave: () => void;
  onOpen: (snapshot: AnalysisSnapshot) => void;
  onClear: () => void;
  onDelete: (snapshotId: string) => void;
}) {
  const visibleSnapshots = snapshots.slice(0, 4);
  return (
    <section className="analysis-snapshots mb-2 rounded-lg border p-2" aria-label="Analyses sauvegardées">
      <div className="mb-2 flex items-center justify-between gap-2">
        <div className="min-w-0">
          <div className="flex items-center gap-1.5 text-[11px] font-semibold text-[var(--text-primary)]">
            <Database className="h-3.5 w-3.5 text-[var(--accent)]" aria-hidden />
            <span className="truncate">Analyses</span>
          </div>
          <div className="text-[10px] text-[var(--text-muted)]">
            {snapshots.length} sauvegarde{snapshots.length > 1 ? 's' : ''}
          </div>
        </div>
        <button
          type="button"
          onClick={onSave}
          disabled={!canSave}
          className="icon-button rounded p-1 disabled:cursor-not-allowed disabled:opacity-45"
          aria-label="Sauvegarder l'analyse courante"
          title="Sauvegarder l'analyse courante"
        >
          <Check className="h-3.5 w-3.5" aria-hidden />
        </button>
      </div>
      {activeSnapshot && (
        <div className="analysis-active mb-2 rounded-md border px-2 py-1 text-[10px]">
          <div className="truncate font-medium">Analyse chargée</div>
          <button
            type="button"
            onClick={onClear}
            className="mt-1 text-[var(--accent-strong)] hover:underline"
          >
            Revenir à l'échange courant
          </button>
        </div>
      )}
      {visibleSnapshots.length === 0 ? (
        <div className="text-[10px] text-[var(--text-muted)]">
          Sauvegarde une analyse pour la rouvrir plus tard.
        </div>
      ) : (
        <div className="space-y-1">
          {visibleSnapshots.map((snapshot) => (
            <div
              key={snapshot.id}
              className={`analysis-snapshot-row rounded-md border px-2 py-1 ${
                activeSnapshot?.id === snapshot.id ? 'analysis-snapshot-active' : ''
              }`}
            >
              <button
                type="button"
                onClick={() => onOpen(snapshot)}
                className="w-full text-left"
                title={snapshot.title}
              >
                <span className="block truncate text-[11px] font-medium text-[var(--text-primary)]">
                  {snapshot.title}
                </span>
                <span className="block text-[10px] text-[var(--text-muted)]">
                  {snapshot.summary.fileCount} fichier{snapshot.summary.fileCount > 1 ? 's' : ''} ·{' '}
                  {snapshot.summary.diagramCount} diagramme{snapshot.summary.diagramCount > 1 ? 's' : ''}
                </span>
              </button>
              <button
                type="button"
                onClick={() => onDelete(snapshot.id)}
                className="mt-1 text-[10px] text-[var(--text-muted)] hover:text-[var(--danger)]"
              >
                Supprimer
              </button>
            </div>
          ))}
        </div>
      )}
    </section>
  );
}

function ExplorerRelatedSourcesPanel({
  repo,
  repoLabel,
  groups,
  relatedOnly,
  onToggleRelatedOnly,
  onOpenReference,
}: {
  repo: string;
  repoLabel: string;
  groups: SourceReferenceGroup[];
  relatedOnly: boolean;
  onToggleRelatedOnly: () => void;
  onOpenReference: (reference: SourceReference) => void;
}) {
  const setInputDraft = useChatStore((s) => s.setInputDraft);
  const [exportState, setExportState] = useState<'idle' | 'loading' | 'done' | 'error'>('idle');
  const [copyState, setCopyState] = useState<'idle' | 'done' | 'error'>('idle');
  const [exportError, setExportError] = useState<string | null>(null);
  const visibleGroups = groups.slice(0, 10);

  const exportPack = async () => {
    setExportState('loading');
    setExportError(null);
    try {
      const files = await Promise.all(
        groups.map(async (group) => {
          try {
            return {
              group,
              source: await mcpClient.source(repo, group.path),
              error: null,
            };
          } catch (error) {
            return {
              group,
              source: null,
              error: error instanceof Error ? error.message : String(error),
            };
          }
        })
      );
      downloadTextFile(
        relatedSourcesFilename(repoLabel, 'pack-analyse'),
        relatedSourcesMarkdown({
          files,
          repoLabel,
          createdAt: Date.now(),
          title: 'Pack d’analyse GitNexus',
          note: 'Ce pack regroupe les fichiers cités dans la conversation courante pour reprise ou analyse ultérieure.',
        })
      );
      setExportState('done');
      window.setTimeout(() => setExportState('idle'), 1400);
    } catch (error) {
      setExportState('error');
      setExportError(error instanceof Error ? error.message : String(error));
    }
  };

  const askAboutRelatedFiles = () => {
    const list = groups
      .slice(0, 16)
      .map((group) => `- ${sourceReferenceSummary(group)}`)
      .join('\n');
    setInputDraft(`Analyse les fichiers concernés par cet échange et explique les liens importants :\n${list}`);
  };

  const copyRelatedList = async () => {
    const ok = await copyTextToClipboard(groups.map(sourceReferenceSummary).join('\n'));
    setCopyState(ok ? 'done' : 'error');
    window.setTimeout(() => setCopyState('idle'), 1400);
  };

  return (
    <section className="related-explorer-panel mb-2 rounded-lg border p-2" aria-label="Fichiers concernés dans l'explorateur">
      <div className="mb-2 flex items-center justify-between gap-2">
        <div className="min-w-0">
          <div className="truncate text-[11px] font-semibold text-[var(--text-primary)]">
            Fichiers concernés
          </div>
          <div className="text-[10px] text-[var(--text-muted)]">
            {groups.length} fichier{groups.length > 1 ? 's' : ''} dans l'échange
          </div>
        </div>
        <div className="flex shrink-0 gap-1">
          <button
            type="button"
            onClick={() => void copyRelatedList()}
            className="icon-button rounded p-1"
            aria-label="Copier la liste des fichiers concernés"
            title={copyState === 'done' ? 'Liste copiée' : copyState === 'error' ? 'Copie impossible' : 'Copier la liste'}
          >
            {copyState === 'done' ? (
              <Check className="h-3.5 w-3.5" aria-hidden />
            ) : (
              <Copy className="h-3.5 w-3.5" aria-hidden />
            )}
          </button>
          <button
            type="button"
            onClick={onToggleRelatedOnly}
            className={`icon-button rounded p-1 ${relatedOnly ? 'control-button-active' : ''}`}
            aria-label={relatedOnly ? 'Afficher tous les fichiers' : 'Afficher seulement les fichiers concernés'}
            aria-pressed={relatedOnly}
            title={relatedOnly ? 'Afficher tous les fichiers' : 'Afficher seulement les fichiers concernés'}
          >
            <ListFilter className="h-3.5 w-3.5" aria-hidden />
          </button>
          <button
            type="button"
            onClick={askAboutRelatedFiles}
            className="icon-button rounded p-1"
            aria-label="Préparer une question sur les fichiers concernés"
            title="Question sur ces fichiers"
          >
            <MessageSquarePlus className="h-3.5 w-3.5" aria-hidden />
          </button>
          <button
            type="button"
            onClick={() => void exportPack()}
            disabled={exportState === 'loading'}
            className="icon-button rounded p-1 disabled:cursor-wait disabled:opacity-60"
            aria-label="Exporter le pack d'analyse des fichiers concernés"
            title={exportState === 'done' ? 'Pack exporté' : "Exporter le pack d'analyse"}
          >
            {exportState === 'loading' ? (
              <Loader2 className="h-3.5 w-3.5 animate-spin" aria-hidden />
            ) : (
              <Download className="h-3.5 w-3.5" aria-hidden />
            )}
          </button>
        </div>
      </div>
      {relatedOnly && (
        <div className="related-explorer-mode mb-2 rounded-md border px-2 py-1 text-[10px]">
          Arbre filtré sur les fichiers concernés.
        </div>
      )}
      <div className="space-y-1">
        {visibleGroups.map((group) => {
          const first = group.references[0] ?? { path: group.path };
          return (
            <button
              key={group.path}
              type="button"
              onClick={() => onOpenReference(first)}
              className="related-explorer-file w-full rounded-md border px-2 py-1 text-left font-mono"
              title={sourceReferenceSummary(group)}
            >
              <span className="block truncate">{group.path}</span>
              <span className="block text-[10px] text-[var(--text-muted)]">
                {group.references.length} réf.
                {first.startLine ? ` · ligne ${first.startLine}` : ''}
              </span>
            </button>
          );
        })}
      </div>
      {groups.length > visibleGroups.length && (
        <div className="mt-1 text-[10px] text-[var(--text-muted)]">
          +{groups.length - visibleGroups.length} autre{groups.length - visibleGroups.length > 1 ? 's' : ''}
        </div>
      )}
      {exportError && <div className="mt-2 text-[10px] text-red-300">{exportError}</div>}
    </section>
  );
}

function SourceRelatedReferences({
  group,
  onOpenReference,
}: {
  group: SourceReferenceGroup;
  onOpenReference: (reference: SourceReference) => void;
}) {
  const lineReferences = group.references.filter((reference) => reference.startLine).slice(0, 10);
  if (lineReferences.length === 0) return null;
  return (
    <div className="source-related-lines mt-2 flex flex-wrap items-center gap-1.5">
      <span className="text-[11px] text-[var(--text-muted)]">Références :</span>
      {lineReferences.map((reference) => (
        <button
          key={`${reference.path}:${reference.startLine ?? ''}:${reference.endLine ?? ''}`}
          type="button"
          onClick={() => onOpenReference(reference)}
          className="source-related-line rounded border px-1.5 py-0.5 font-mono text-[11px]"
          title={sourceReferenceSummary({ path: group.path, references: [reference] })}
        >
          L{reference.startLine}
          {reference.endLine && reference.endLine !== reference.startLine ? `-${reference.endLine}` : ''}
        </button>
      ))}
    </div>
  );
}

function SourceGraphInsights({
  repo,
  source,
  onOpenGraph,
}: {
  repo: string;
  source: SourceContent;
  onOpenGraph: (target: GraphTarget) => void;
}) {
  const [state, setState] = useState<{
    key: string;
    loading: boolean;
    symbols: SymbolSearchResult[];
    error: string | null;
  }>({ key: '', loading: false, symbols: [], error: null });
  const key = `${repo}:${source.path}`;
  const visibleSymbols = state.key === key ? state.symbols.slice(0, 6) : [];
  const loading = state.key !== key || state.loading;

  useEffect(() => {
    let alive = true;
    void mcpClient
      .symbols(repo, symbolQueryFromPath(source.path), 30)
      .then((symbols) => {
        if (!alive) return;
        const sourcePath = normalizePath(source.path);
        setState({
          key,
          loading: false,
          symbols: symbols.filter((symbol) => normalizePath(symbol.filePath) === sourcePath),
          error: null,
        });
      })
      .catch((error) => {
        if (!alive) return;
        setState({
          key,
          loading: false,
          symbols: [],
          error: error instanceof Error ? error.message : String(error),
        });
      });
    return () => {
      alive = false;
    };
  }, [key, repo, source.path]);

  if (!loading && visibleSymbols.length === 0 && !state.error) return null;

  return (
    <div className="source-graph-insights mt-2 rounded-md border p-2">
      <div className="mb-1 flex items-center justify-between gap-2">
        <div className="flex min-w-0 items-center gap-1.5 text-[11px] font-medium text-[var(--text-primary)]">
          <GitFork className="h-3.5 w-3.5 text-[var(--accent)]" aria-hidden />
          <span>Symboles graphe</span>
        </div>
        {loading && <Loader2 className="h-3.5 w-3.5 animate-spin text-[var(--text-muted)]" aria-hidden />}
      </div>
      {state.error ? (
        <div className="text-[10px] text-red-300">{state.error}</div>
      ) : visibleSymbols.length > 0 ? (
        <div className="flex flex-wrap gap-1">
          {visibleSymbols.map((symbol) => (
            <button
              key={symbol.nodeId}
              type="button"
              onClick={() =>
                onOpenGraph({
                  nodeId: symbol.nodeId,
                  name: symbol.name,
                  label: symbol.label,
                  filePath: symbol.filePath,
                  startLine: symbol.startLine,
                  endLine: symbol.endLine,
                })
              }
              className="source-graph-symbol rounded border px-1.5 py-0.5 text-[11px]"
              title={`Voir le voisinage graphe de ${symbol.name}`}
            >
              <span className="font-mono">{symbol.name}</span>
              <span className="ml-1 text-[var(--text-muted)]">{symbol.label}</span>
            </button>
          ))}
        </div>
      ) : (
        <div className="text-[10px] text-[var(--text-muted)]">Recherche des symboles...</div>
      )}
    </div>
  );
}

function FileTree({
  nodes,
  onOpenFile,
  onToggleFolder,
  highlightedSourcePaths,
  relatedFolderCounts,
  expandedPaths,
  activePath = null,
  depth = 0,
}: {
  nodes: FileTreeNode[];
  onOpenFile: (path: string) => void;
  onToggleFolder: (path: string) => void;
  highlightedSourcePaths: ReadonlySet<string>;
  relatedFolderCounts: ReadonlyMap<string, number>;
  expandedPaths: ReadonlySet<string>;
  activePath?: string | null;
  depth?: number;
}) {
  const activePathKey = normalizePath(activePath ?? undefined);
  return (
    <div className="space-y-0.5">
      {nodes.map((node) => {
        const pathKey = normalizePath(node.path);
        const isRelated = !node.isDir && highlightedSourcePaths.has(pathKey);
        const isActive = !node.isDir && activePathKey === pathKey;
        const folderRelatedCount = node.isDir ? relatedFolderCounts.get(pathKey) ?? 0 : 0;
        const isExpanded = !node.isDir || expandedPaths.has(pathKey);
        return (
          <div key={node.path}>
            <button
              type="button"
              onClick={() => {
                if (node.isDir) {
                  onToggleFolder(node.path);
                } else {
                  onOpenFile(node.path);
                }
              }}
              className={`file-tree-item flex w-full items-center gap-1.5 rounded border border-transparent px-1.5 py-1 text-left ${
                node.isDir ? 'text-neutral-500' : 'text-neutral-300 hover:bg-neutral-900 hover:text-neutral-100'
              } ${isRelated ? 'file-tree-related' : ''} ${folderRelatedCount > 0 ? 'file-tree-folder-related' : ''} ${isActive ? 'file-tree-active' : ''}`}
              style={{ paddingLeft: `${depth * 10 + 6}px` }}
              aria-expanded={node.isDir ? isExpanded : undefined}
              title={
                isRelated
                  ? `${node.path} - fichier concerné`
                  : folderRelatedCount > 0
                    ? `${node.path} - ${folderRelatedCount} fichier concerné`
                    : node.path
              }
              aria-label={
                isRelated
                  ? `${node.name} - fichier concerné`
                  : folderRelatedCount > 0
                    ? `${node.name} - ${folderRelatedCount} fichier concerné`
                    : node.name
              }
            >
              {node.isDir ? (
                <>
                  {isExpanded ? (
                    <ChevronDown className="h-3 w-3 shrink-0 text-[var(--text-muted)]" aria-hidden />
                  ) : (
                    <ChevronRight className="h-3 w-3 shrink-0 text-[var(--text-muted)]" aria-hidden />
                  )}
                  <Folder className="h-3.5 w-3.5 shrink-0 text-amber-300/70" aria-hidden />
                </>
              ) : (
                <FileCode2 className="h-3.5 w-3.5 shrink-0 text-neutral-500" aria-hidden />
              )}
              <span className="min-w-0 flex-1 truncate">{node.name}</span>
              {folderRelatedCount > 0 && (
                <span className="file-tree-related-count ml-auto shrink-0 rounded px-1 text-[10px] tabular-nums">
                  {folderRelatedCount}
                </span>
              )}
              {isRelated && <span className="file-tree-related-dot ml-auto h-1.5 w-1.5 shrink-0 rounded-full" aria-hidden />}
            </button>
            {node.isDir && isExpanded && node.children.length > 0 && (
              <FileTree
                nodes={node.children}
                activePath={activePath}
                expandedPaths={expandedPaths}
                highlightedSourcePaths={highlightedSourcePaths}
                relatedFolderCounts={relatedFolderCounts}
                onToggleFolder={onToggleFolder}
                onOpenFile={onOpenFile}
                depth={depth + 1}
              />
            )}
          </div>
        );
      })}
    </div>
  );
}

function SourceCode({
  source,
  highlight,
}: {
  source: SourceContent;
  highlight: SourceCodeHighlightState;
}) {
  return (
    <Suspense fallback={<PlainSourceCode source={source} highlight={highlight} />}>
      <SourceCodeHighlighter source={source} highlight={highlight} />
    </Suspense>
  );
}

function PlainSourceCode({
  source,
  highlight,
}: {
  source: SourceContent;
  highlight: SourceCodeHighlightState;
}) {
  const lines = source.content ? source.content.split('\n') : [];
  const start = source.startLine || 1;

  return (
    <pre
      className="source-code-view min-h-0 flex-1 overflow-auto p-0 text-[11px] leading-5"
      data-testid="source-code-view"
    >
      <code>
        {lines.map((line, index) => (
          <div
            key={`${source.path}-${start + index}`}
            className={`source-code-row flex ${sourceLineClass(start + index, highlight)}`}
            data-source-line={start + index}
          >
            <span className="source-line-number w-12 shrink-0 select-none border-r pr-3 text-right">
              {start + index}
            </span>
            <span className="source-code-text min-w-0 flex-1 whitespace-pre px-3 font-mono">{line || ' '}</span>
          </div>
        ))}
        {source.truncated && (
          <div className="source-truncated border-t px-3 py-2 text-xs">
            Extrait limite aux premieres lignes demandees.
          </div>
        )}
      </code>
    </pre>
  );
}

function SourceOutline({
  items,
  activeLine,
  onSelect,
}: {
  items: SourceOutlineItem[];
  activeLine: number | null;
  onSelect: (item: SourceOutlineItem) => void;
}) {
  return (
    <div className="source-outline mt-2 rounded-md border border-neutral-800 bg-neutral-900/35 p-2">
      <div className="mb-1 flex items-center justify-between gap-2">
        <span className="text-[11px] font-medium uppercase tracking-wide text-neutral-500">Plan</span>
        <span className="text-[11px] text-neutral-600">{items.length} symboles</span>
      </div>
      <div className="flex max-h-20 flex-wrap gap-1 overflow-auto">
        {items.map((item) => (
          <button
            key={`${item.kind}-${item.lineNumber}-${item.name}`}
            type="button"
            onClick={() => onSelect(item)}
            className={`source-outline-item rounded border px-1.5 py-0.5 text-[11px] ${
              activeLine === item.lineNumber ? 'source-outline-item-active' : ''
            }`}
            title={`Ligne ${item.lineNumber}`}
          >
            <span className="source-outline-kind">{sourceOutlineKindLabel(item.kind)}</span>
            <span className="font-mono">{item.name}</span>
            <span className="text-neutral-600">:{item.lineNumber}</span>
          </button>
        ))}
      </div>
    </div>
  );
}

function sourceLineClass(lineNumber: number, highlight: SourceCodeHighlightState): string {
  const classes = ['source-code-line'];
  if (isTargetLine(lineNumber, highlight)) classes.push('source-line-target');
  if (highlight.searchLines.has(lineNumber)) classes.push('source-line-match');
  if (highlight.activeLine === lineNumber) classes.push('source-line-active');
  return classes.join(' ');
}

function isTargetLine(lineNumber: number, highlight: SourceCodeHighlightState): boolean {
  if (!highlight.targetStartLine) return false;
  const end = highlight.targetEndLine ?? highlight.targetStartLine;
  return lineNumber >= highlight.targetStartLine && lineNumber <= end;
}

function GraphNavigator({
  repo,
  onOpenSource,
  initialTarget,
}: {
  repo: string;
  onOpenSource: (target: SourceTarget) => void;
  initialTarget?: GraphTarget | null;
}) {
  const setInputDraft = useChatStore((s) => s.setInputDraft);
  const [query, setQuery] = useState('');
  const [results, setResults] = useState<SymbolSearchResult[]>([]);
  const [selected, setSelected] = useState<GraphNode | null>(null);
  const [graph, setGraph] = useState<GraphPayload | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!initialTarget?.nodeId) return;
    let alive = true;
    void mcpClient
      .graphNeighborhood(repo, initialTarget.nodeId, 2)
      .then((nextGraph) => {
        if (!alive) return;
        setGraph(nextGraph);
        setSelected(
          nextGraph.nodes.find((node) => node.id === initialTarget.nodeId) ??
            graphTargetToNode(initialTarget)
        );
      })
      .catch((err) => {
        if (alive) setError(err instanceof Error ? err.message : String(err));
      })
      .finally(() => {
        if (alive) setLoading(false);
      });
    return () => {
      alive = false;
    };
  }, [initialTarget, repo]);

  const runSearch = async () => {
    if (!query.trim()) return;
    setLoading(true);
    setError(null);
    try {
      const symbols = await mcpClient.symbols(repo, query.trim(), 25);
      setResults(symbols);
      setGraph(null);
      setSelected(null);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setLoading(false);
    }
  };

  const openNeighborhood = async (symbol: SymbolSearchResult) => {
    setLoading(true);
    setError(null);
    try {
      const nextGraph = await mcpClient.graphNeighborhood(repo, symbol.nodeId, 2);
      setGraph(nextGraph);
      setSelected(nextGraph.nodes.find((node) => node.id === symbol.nodeId) ?? null);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setLoading(false);
    }
  };

  const askAboutNode = (node: GraphNode) => {
    setInputDraft(
      `Explique le noeud ${node.name} (${node.label}) dans ${node.filePath}, ses voisins dans le graphe et les risques de modification.`
    );
  };

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <div className="border-b border-neutral-900 p-3">
        <form
          className="flex gap-2"
          onSubmit={(event) => {
            event.preventDefault();
            void runSearch();
          }}
        >
          <label className="flex min-w-0 flex-1 items-center gap-2 rounded-md border border-neutral-800 bg-neutral-900/70 px-2 py-1.5 text-xs text-neutral-400">
            <Search className="h-3.5 w-3.5" aria-hidden />
            <input
              value={query}
              onChange={(event) => setQuery(event.target.value)}
              placeholder="Chercher une classe, methode, action..."
              className="min-w-0 flex-1 bg-transparent text-neutral-100 outline-none placeholder:text-neutral-600"
            />
          </label>
          <button
            type="submit"
            className="rounded-md border border-neutral-800 px-3 text-xs text-neutral-200 hover:bg-neutral-900"
          >
            Chercher
          </button>
        </form>
      </div>
      {error && (
        <div className="border-b border-red-950 bg-red-950/20 p-3">
          <ErrorText message={error} />
        </div>
      )}
      <div className="grid min-h-0 flex-1 grid-cols-[190px_minmax(0,1fr)]">
        <div className="min-h-0 overflow-auto border-r border-neutral-900 p-2 text-xs">
          {loading && results.length === 0 ? (
            <LoadingLine label="Recherche..." />
          ) : results.length === 0 ? (
            <div className="p-3 text-neutral-600">Lance une recherche pour ouvrir un voisinage graphe.</div>
          ) : (
            <div className="space-y-1">
              {results.map((symbol) => (
                <button
                  key={symbol.nodeId}
                  type="button"
                  onClick={() => void openNeighborhood(symbol)}
                  className="w-full rounded-md border border-neutral-900 bg-neutral-950 p-2 text-left hover:border-neutral-700 hover:bg-neutral-900"
                >
                  <div className="truncate font-medium text-neutral-100">{symbol.name}</div>
                  <div className="truncate text-[11px] text-neutral-500">
                    {symbol.label} - {symbol.filePath}
                  </div>
                </button>
              ))}
            </div>
          )}
        </div>
        <div className="min-h-0 overflow-auto p-3">
          {loading && results.length > 0 && <LoadingLine label="Chargement du voisinage..." />}
          {!graph ? (
            <div className="flex h-full items-center justify-center p-6 text-center text-sm text-neutral-600">
              Selectionne un symbole pour afficher son voisinage.
            </div>
          ) : (
            <div className="space-y-3">
              <div className="rounded-lg border border-neutral-800 bg-neutral-900/40 p-3">
                <div className="flex items-start justify-between gap-3">
                  <div className="min-w-0">
                    <div className="text-sm font-medium text-neutral-100">
                      {selected?.name ?? 'Voisinage graphe'}
                    </div>
                    <div className="mt-1 text-xs text-neutral-500">
                      {graph.stats.nodeCount} noeuds - {graph.stats.edgeCount} relations
                      {graph.stats.truncated ? ' - tronque' : ''}
                    </div>
                  </div>
                  {selected && (
                    <button
                      type="button"
                      onClick={() => askAboutNode(selected)}
                      className="rounded-md border border-neutral-800 p-1.5 text-neutral-300 hover:bg-neutral-800"
                      title="Demander au chat"
                    >
                      <MessageSquarePlus className="h-3.5 w-3.5" aria-hidden />
                    </button>
                  )}
                </div>
              </div>
              <GraphMap
                graph={graph}
                selectedId={selected?.id ?? null}
                onSelect={setSelected}
              />
              {selected && (
                <SelectedNodeDetails
                  node={selected}
                  graph={graph}
                  onAsk={askAboutNode}
                  onOpenSource={onOpenSource}
                />
              )}
              <div className="space-y-2">
                {graph.nodes
                  .slice()
                  .sort((a, b) => (a.depth ?? 0) - (b.depth ?? 0) || a.name.localeCompare(b.name))
                  .map((node) => (
                    <div
                      key={node.id}
                      className={`rounded-lg border bg-neutral-950 p-3 ${
                        selected?.id === node.id ? 'border-violet-500/60' : 'border-neutral-900'
                      }`}
                    >
                      <div className="flex items-start justify-between gap-3">
                        <button
                          type="button"
                          onClick={() => setSelected(node)}
                          className="min-w-0 flex-1 text-left"
                        >
                          <div className="truncate text-sm font-medium text-neutral-100">{node.name}</div>
                          <div className="mt-1 truncate text-xs text-neutral-500">
                            {node.label}
                            {typeof node.depth === 'number' ? ` - distance ${node.depth}` : ''} - {node.filePath}
                          </div>
                        </button>
                        {node.filePath && (
                          <button
                            type="button"
                            onClick={() =>
                              onOpenSource({
                                path: node.filePath,
                                startLine: node.startLine,
                                endLine: node.endLine,
                              })
                            }
                            className="rounded-md border border-neutral-800 px-2 py-1 text-xs text-neutral-300 hover:bg-neutral-900"
                          >
                            Source
                          </button>
                        )}
                      </div>
                    </div>
                  ))}
              </div>
            </div>
          )}
        </div>
      </div>
    </div>
  );
}

interface PositionedGraphNode extends GraphNode {
  x: number;
  y: number;
  radius: number;
}

function GraphMap({
  graph,
  selectedId,
  onSelect,
}: {
  graph: GraphPayload;
  selectedId: string | null;
  onSelect: (node: GraphNode) => void;
}) {
  const layout = useMemo(() => buildGraphLayout(graph.nodes, graph.edges), [graph.edges, graph.nodes]);

  return (
    <div className="overflow-hidden rounded-lg border border-neutral-800 bg-neutral-950">
      <div className="flex items-center justify-between border-b border-neutral-900 px-3 py-2">
        <div className="text-xs font-medium text-neutral-200">Voisinage visuel</div>
        <div className="text-[11px] text-neutral-600">
          {layout.nodes.length} noeuds - {layout.edges.length} liens
        </div>
      </div>
      <svg
        viewBox="0 0 720 300"
        role="img"
        aria-label="Carte du voisinage de graphe"
        className="h-72 w-full bg-[radial-gradient(circle_at_center,rgba(124,58,237,0.13),transparent_48%)]"
      >
        <g>
          {layout.edges.map((edge) => (
            <line
              key={edge.id}
              x1={edge.source.x}
              y1={edge.source.y}
              x2={edge.target.x}
              y2={edge.target.y}
              stroke="rgba(115,115,115,0.42)"
              strokeWidth={edge.relType === 'Calls' ? 1.8 : 1.1}
            >
              <title>{edge.relType}</title>
            </line>
          ))}
        </g>
        <g>
          {layout.nodes.map((node) => {
            const selected = node.id === selectedId;
            return (
              <g
                key={node.id}
                role="button"
                tabIndex={0}
                aria-label={`${node.name} ${node.label}`}
                transform={`translate(${node.x} ${node.y})`}
                className="cursor-pointer outline-none"
                onClick={() => onSelect(node)}
                onKeyDown={(event) => {
                  if (event.key === 'Enter' || event.key === ' ') {
                    event.preventDefault();
                    onSelect(node);
                  }
                }}
              >
                <circle
                  r={node.radius + (selected ? 5 : 0)}
                  fill={selected ? 'rgba(139,92,246,0.26)' : 'rgba(23,23,23,0.82)'}
                  stroke={selected ? 'rgb(167,139,250)' : nodeColor(node)}
                  strokeWidth={selected ? 2.5 : 1.6}
                />
                <circle r={Math.max(4, node.radius * 0.42)} fill={nodeColor(node)} />
                <text
                  x={0}
                  y={node.radius + 15}
                  textAnchor="middle"
                  className="pointer-events-none select-none fill-neutral-300 text-[10px]"
                >
                  {compactLabel(node.name, 18)}
                </text>
                <title>{`${node.name} (${node.label})`}</title>
              </g>
            );
          })}
        </g>
      </svg>
    </div>
  );
}

function SelectedNodeDetails({
  node,
  graph,
  onAsk,
  onOpenSource,
}: {
  node: GraphNode;
  graph: GraphPayload;
  onAsk: (node: GraphNode) => void;
  onOpenSource: (target: SourceTarget) => void;
}) {
  const relations = graph.edges
    .filter((edge) => edge.source === node.id || edge.target === node.id)
    .slice(0, 6);

  return (
    <div className="rounded-lg border border-neutral-800 bg-neutral-900/45 p-3">
      <div className="flex items-start justify-between gap-3">
        <div className="min-w-0">
          <div className="truncate text-sm font-medium text-neutral-100">{node.name}</div>
          <div className="mt-1 truncate text-xs text-neutral-500">
            {node.label} - {node.filePath || 'source inconnue'}
          </div>
        </div>
        <div className="flex shrink-0 gap-1.5">
          {node.filePath && (
            <button
              type="button"
              onClick={() =>
                onOpenSource({
                  path: node.filePath,
                  startLine: node.startLine,
                  endLine: node.endLine,
                })
              }
              className="rounded-md border border-neutral-800 px-2 py-1 text-xs text-neutral-300 hover:bg-neutral-800"
            >
              Source
            </button>
          )}
          <button
            type="button"
            onClick={() => onAsk(node)}
            className="rounded-md border border-neutral-800 p-1.5 text-neutral-300 hover:bg-neutral-800"
            title="Demander au chat"
          >
            <MessageSquarePlus className="h-3.5 w-3.5" aria-hidden />
          </button>
        </div>
      </div>
      {relations.length > 0 && (
        <div className="mt-3 flex flex-wrap gap-1.5">
          {relations.map((edge) => (
            <span
              key={edge.id}
              className="rounded border border-neutral-800 bg-neutral-950 px-1.5 py-0.5 text-[11px] text-neutral-400"
            >
              {edge.relType}
            </span>
          ))}
        </div>
      )}
    </div>
  );
}

function buildGraphLayout(nodes: GraphNode[], edges: GraphEdge[]) {
  const visibleNodes = nodes
    .slice()
    .sort((a, b) => (a.depth ?? 99) - (b.depth ?? 99) || a.name.localeCompare(b.name))
    .slice(0, 80);
  const byId = new Map<string, PositionedGraphNode>();
  const rings = new Map<number, GraphNode[]>();
  for (const node of visibleNodes) {
    const depth = Math.max(0, Math.min(4, node.depth ?? 1));
    const ring = rings.get(depth) ?? [];
    ring.push(node);
    rings.set(depth, ring);
  }

  for (const [depth, ringNodes] of rings) {
    const radius = depth === 0 ? 0 : 56 + depth * 43;
    const nodeRadius = depth === 0 ? 14 : Math.max(7, 12 - depth);
    ringNodes.forEach((node, index) => {
      const angle = depth === 0 ? 0 : (Math.PI * 2 * index) / ringNodes.length - Math.PI / 2;
      byId.set(node.id, {
        ...node,
        x: 360 + Math.cos(angle) * radius,
        y: 145 + Math.sin(angle) * Math.min(radius, 130),
        radius: nodeRadius,
      });
    });
  }

  const visibleEdges = edges
    .map((edge) => ({
      ...edge,
      source: byId.get(edge.source),
      target: byId.get(edge.target),
    }))
    .filter((edge): edge is GraphEdge & { source: PositionedGraphNode; target: PositionedGraphNode } =>
      Boolean(edge.source && edge.target)
    )
    .slice(0, 140);

  return {
    nodes: Array.from(byId.values()),
    edges: visibleEdges,
  };
}

function nodeColor(node: GraphNode): string {
  if (node.isDeadCandidate) return 'rgb(248,113,113)';
  if (node.isTraced) return 'rgb(52,211,153)';
  if (node.label.includes('Controller')) return 'rgb(96,165,250)';
  if (node.label.includes('Service')) return 'rgb(167,139,250)';
  if (node.label.includes('Repository')) return 'rgb(251,191,36)';
  return 'rgb(163,163,163)';
}

function compactLabel(value: string, max: number): string {
  if (value.length <= max) return value;
  return `${value.slice(0, Math.max(1, max - 3))}...`;
}

interface SourceSearchMatch {
  lineNumber: number;
  column: number;
}

type SourceOutlineKind = 'type' | 'method' | 'property';

interface SourceOutlineItem {
  kind: SourceOutlineKind;
  name: string;
  lineNumber: number;
  level: number;
}

function findSourceMatches(source: SourceContent | null, query: string): SourceSearchMatch[] {
  const needle = query.trim().toLowerCase();
  if (!source || !needle) return [];
  const start = source.startLine || 1;
  return source.content
    .split('\n')
    .map((line, index) => ({
      lineNumber: start + index,
      column: line.toLowerCase().indexOf(needle),
    }))
    .filter((match) => match.column >= 0);
}

function extractSourceOutline(source: SourceContent | null): SourceOutlineItem[] {
  if (!source?.content) return [];
  const start = source.startLine || 1;
  const items: SourceOutlineItem[] = [];
  const seen = new Set<string>();

  source.content.split('\n').forEach((line, index) => {
    const item = outlineItemFromLine(line, start + index);
    if (!item) return;
    const key = `${item.kind}:${item.name}:${item.lineNumber}`;
    if (seen.has(key)) return;
    seen.add(key);
    items.push(item);
  });

  return items.slice(0, 32);
}

function outlineItemFromLine(line: string, lineNumber: number): SourceOutlineItem | null {
  const trimmed = line.trim();
  if (!trimmed || trimmed.startsWith('//') || trimmed.startsWith('*')) return null;
  const level = Math.min(3, Math.floor((line.match(/^\s*/)?.[0].length ?? 0) / 2));
  const typeMatch = /^(?:(?:export|public|private|protected|internal|abstract|sealed|static|partial)\s+)*(class|interface|enum|struct|record)\s+([A-Za-z_][A-Za-z0-9_]*)/.exec(trimmed);
  if (typeMatch) {
    return { kind: 'type', name: typeMatch[2], lineNumber, level };
  }

  const functionMatch = /^(?:export\s+)?(?:async\s+)?function\s+([A-Za-z_][A-Za-z0-9_]*)\s*\(/.exec(trimmed);
  if (functionMatch) {
    return { kind: 'method', name: functionMatch[1], lineNumber, level };
  }

  const rustFunctionMatch = /^(?:pub(?:\([^)]*\))?\s+)?(?:async\s+)?fn\s+([A-Za-z_][A-Za-z0-9_]*)\s*\(/.exec(trimmed);
  if (rustFunctionMatch) {
    return { kind: 'method', name: rustFunctionMatch[1], lineNumber, level };
  }

  const propertyMatch = /^(?:(?:public|private|protected|internal|static|virtual|override|required|readonly)\s+)+[A-Za-z_][A-Za-z0-9_<>,.?[\]\s]*\s+([A-Za-z_][A-Za-z0-9_]*)\s*\{\s*(?:get|set|init)\b/.exec(trimmed);
  if (propertyMatch) {
    return { kind: 'property', name: propertyMatch[1], lineNumber, level };
  }

  const methodMatch = /^(?:(?:public|private|protected|internal|static|async|virtual|override|sealed|partial|extern|unsafe)\s+)+[A-Za-z_][A-Za-z0-9_<>,.?[\]\s]*\s+([A-Za-z_][A-Za-z0-9_]*)\s*\([^;]*\)\s*(?:\{|=>|$)/.exec(trimmed);
  if (methodMatch && !CONTROL_WORDS.has(methodMatch[1])) {
    return { kind: 'method', name: methodMatch[1], lineNumber, level };
  }

  const arrowFunctionMatch = /^(?:export\s+)?(?:const|let|var)\s+([A-Za-z_][A-Za-z0-9_]*)\s*=\s*(?:async\s*)?(?:\([^)]*\)|[A-Za-z_][A-Za-z0-9_]*)\s*=>/.exec(trimmed);
  if (arrowFunctionMatch) {
    return { kind: 'method', name: arrowFunctionMatch[1], lineNumber, level };
  }

  return null;
}

const CONTROL_WORDS = new Set(['catch', 'for', 'if', 'lock', 'switch', 'using', 'while']);

function sourceOutlineKindLabel(kind: SourceOutlineKind): string {
  if (kind === 'type') return 'T';
  if (kind === 'property') return 'P';
  return 'M';
}

function symbolQueryFromPath(path: string): string {
  const filename = path.split(/[\\/]/).pop() ?? path;
  return filename.replace(/\.[^.]+$/, '') || filename;
}

function bestSymbolForSource(symbols: SymbolSearchResult[], source: SourceContent): SymbolSearchResult | null {
  const sourcePath = normalizePath(source.path);
  const sourceName = symbolQueryFromPath(source.path).toLowerCase();
  return (
    symbols
      .slice()
      .sort((a, b) => symbolScore(b, sourcePath, sourceName, source) - symbolScore(a, sourcePath, sourceName, source))[0] ??
    null
  );
}

function symbolScore(
  symbol: SymbolSearchResult,
  sourcePath: string,
  sourceName: string,
  source: SourceContent
): number {
  let score = 0;
  if (normalizePath(symbol.filePath) === sourcePath) score += 100;
  if (symbol.name.toLowerCase() === sourceName) score += 30;
  if (symbol.name.toLowerCase().includes(sourceName)) score += 10;
  if (linesOverlap(symbol.startLine, symbol.endLine, source.startLine, source.endLine)) score += 20;
  return score;
}

function linesOverlap(
  aStart?: number,
  aEnd?: number,
  bStart?: number,
  bEnd?: number
): boolean {
  if (!aStart || !bStart) return false;
  const aLast = aEnd ?? aStart;
  const bLast = bEnd ?? bStart;
  return aStart <= bLast && bStart <= aLast;
}

function normalizePath(path: string | undefined): string {
  return (path ?? '').replace(/\\/g, '/').toLowerCase();
}

function buildAutoExpandedFolders(
  nodes: FileTreeNode[],
  groups: SourceReferenceGroup[],
  activePath: string | null,
  includeTopLevel: boolean
): Set<string> {
  const expanded = new Set<string>();
  if (includeTopLevel) {
    nodes.filter((node) => node.isDir).slice(0, 12).forEach((node) => expanded.add(normalizePath(node.path)));
  }

  for (const group of groups) {
    addParentFolders(expanded, group.path);
  }
  if (activePath) {
    addParentFolders(expanded, activePath);
  }
  return expanded;
}

function addParentFolders(target: Set<string>, path: string) {
  const parts = normalizePath(path).split('/').filter(Boolean);
  for (let index = 1; index < parts.length; index += 1) {
    target.add(parts.slice(0, index).join('/'));
  }
}

function parentFolderSet(path: string): Set<string> {
  const folders = new Set<string>();
  addParentFolders(folders, path);
  return folders;
}

function expandedFolderSet(
  autoExpanded: ReadonlySet<string>,
  manuallyExpanded: ReadonlySet<string>,
  manuallyCollapsed: ReadonlySet<string>
): Set<string> {
  const expanded = new Set([...autoExpanded, ...manuallyExpanded]);
  for (const path of manuallyCollapsed) {
    expanded.delete(path);
  }
  return expanded;
}

function mergeSets(current: Set<string>, additions: ReadonlySet<string>): Set<string> {
  let changed = false;
  const next = new Set(current);
  for (const value of additions) {
    if (!next.has(value)) {
      next.add(value);
      changed = true;
    }
  }
  return changed ? next : current;
}

function removeSetValues(current: Set<string>, removals: ReadonlySet<string>): Set<string> {
  let changed = false;
  const next = new Set(current);
  for (const value of removals) {
    if (next.delete(value)) changed = true;
  }
  return changed ? next : current;
}

function filterTreeToRelated(
  nodes: FileTreeNode[],
  highlightedSourcePaths: ReadonlySet<string>
): FileTreeNode[] {
  const result: FileTreeNode[] = [];
  for (const node of nodes) {
    const pathKey = normalizePath(node.path);
    if (!node.isDir) {
      if (highlightedSourcePaths.has(pathKey)) result.push(node);
      continue;
    }

    const children = filterTreeToRelated(node.children, highlightedSourcePaths);
    if (children.length > 0) {
      result.push({ ...node, children });
    }
  }
  return result;
}

function sortTreeByRelated(
  nodes: FileTreeNode[],
  highlightedSourcePaths: ReadonlySet<string>,
  relatedFolderCounts: ReadonlyMap<string, number>
): FileTreeNode[] {
  return nodes
    .map((node) => ({
      ...node,
      children: node.isDir
        ? sortTreeByRelated(node.children, highlightedSourcePaths, relatedFolderCounts)
        : node.children,
    }))
    .sort((a, b) => {
      const rankDelta =
        relatedTreeRank(a, highlightedSourcePaths, relatedFolderCounts) -
        relatedTreeRank(b, highlightedSourcePaths, relatedFolderCounts);
      if (rankDelta !== 0) return rankDelta;
      if (a.isDir !== b.isDir) return a.isDir ? -1 : 1;
      return a.name.localeCompare(b.name);
    });
}

function relatedTreeRank(
  node: FileTreeNode,
  highlightedSourcePaths: ReadonlySet<string>,
  relatedFolderCounts: ReadonlyMap<string, number>
): number {
  const pathKey = normalizePath(node.path);
  if (!node.isDir && highlightedSourcePaths.has(pathKey)) return 0;
  if (node.isDir && (relatedFolderCounts.get(pathKey) ?? 0) > 0) return 1;
  return 2;
}

function buildRelatedFolderCounts(groups: SourceReferenceGroup[]): Map<string, number> {
  const counts = new Map<string, number>();
  for (const group of groups) {
    const parts = normalizePath(group.path).split('/').filter(Boolean);
    for (let index = 1; index < parts.length; index += 1) {
      const folder = parts.slice(0, index).join('/');
      counts.set(folder, (counts.get(folder) ?? 0) + 1);
    }
  }
  return counts;
}

function graphTargetToNode(target: GraphTarget): GraphNode {
  return {
    id: target.nodeId,
    name: target.name,
    label: target.label ?? 'Symbol',
    filePath: target.filePath ?? '',
    startLine: target.startLine,
    endLine: target.endLine,
    depth: 0,
  };
}

function filterTree(nodes: FileTreeNode[], query: string): FileTreeNode[] {
  const q = query.trim().toLowerCase();
  if (!q) return nodes;
  const result: FileTreeNode[] = [];
  for (const node of nodes) {
    const childMatches = node.isDir ? filterTree(node.children, query) : [];
    const selfMatches = node.name.toLowerCase().includes(q) || node.path.toLowerCase().includes(q);
    if (selfMatches || childMatches.length > 0) {
      result.push({
        ...node,
        children: node.isDir && childMatches.length > 0 ? childMatches : node.children,
      });
    }
  }
  return result;
}

function LoadingLine({ label }: { label: string }) {
  return (
    <div className="flex items-center gap-2 p-3 text-xs text-neutral-500">
      <Loader2 className="h-3.5 w-3.5 animate-spin" aria-hidden />
      {label}
    </div>
  );
}

function ErrorText({ message }: { message: string }) {
  return <div className="whitespace-pre-wrap text-xs text-red-300">{message}</div>;
}

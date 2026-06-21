import { useEffect, useMemo, useRef, useState, type ReactNode } from 'react';
import { Braces, FileCode2, Loader2, Search, X } from 'lucide-react';
import {
  mcpClient,
  type FileTreeNode,
  type SymbolSearchResult,
} from '../../api/mcp-client';
import type { GraphTarget, SourceTarget } from '../explorer/WorkspacePanel';

interface QuickOpenProps {
  repo: string | null;
  repoName: string | null;
  onClose: () => void;
  onOpenSource: (target: SourceTarget) => void;
  onOpenGraph: (target: GraphTarget) => void;
}

interface FlatFile {
  name: string;
  path: string;
}

export function QuickOpen({
  repo,
  repoName,
  onClose,
  onOpenSource,
  onOpenGraph,
}: QuickOpenProps) {
  const inputRef = useRef<HTMLInputElement | null>(null);
  const [query, setQuery] = useState('');
  const [fileState, setFileState] = useState<{
    repo: string | null;
    files: FlatFile[];
    error: string | null;
  }>({ repo: null, files: [], error: null });
  const [symbolState, setSymbolState] = useState<{
    key: string | null;
    symbols: SymbolSearchResult[];
    error: string | null;
  }>({ key: null, symbols: [], error: null });

  useEffect(() => {
    inputRef.current?.focus();
  }, []);

  useEffect(() => {
    if (!repo) return undefined;
    let alive = true;
    void mcpClient
      .fileTree(repo)
      .then((tree) => {
        if (!alive) return;
        setFileState({ repo, files: flattenFileTree(tree), error: null });
      })
      .catch((err) => {
        if (!alive) return;
        setFileState({
          repo,
          files: [],
          error: err instanceof Error ? err.message : String(err),
        });
      });
    return () => {
      alive = false;
    };
  }, [repo]);

  useEffect(() => {
    const term = query.trim();
    if (!repo || term.length < 2) return undefined;

    let alive = true;
    const key = `${repo}:${term}`;
    const timer = window.setTimeout(() => {
      void mcpClient
        .symbols(repo, term, 8)
        .then((items) => {
          if (!alive) return;
          setSymbolState({ key, symbols: items, error: null });
        })
        .catch((err) => {
          if (!alive) return;
          setSymbolState({
            key,
            symbols: [],
            error: err instanceof Error ? err.message : String(err),
          });
        });
    }, 180);

    return () => {
      alive = false;
      window.clearTimeout(timer);
    };
  }, [query, repo]);

  const activeFiles = useMemo(
    () => (fileState.repo === repo ? fileState.files : []),
    [fileState.files, fileState.repo, repo]
  );
  const activeTerm = query.trim();
  const symbolKey = repo && activeTerm.length >= 2 ? `${repo}:${activeTerm}` : null;
  const symbols = symbolKey && symbolState.key === symbolKey ? symbolState.symbols : [];
  const fileLoading = !!repo && fileState.repo !== repo;
  const fileError = fileState.repo === repo ? fileState.error : null;
  const symbolLoading = !!symbolKey && symbolState.key !== symbolKey;
  const symbolError = symbolKey && symbolState.key === symbolKey ? symbolState.error : null;

  const fileMatches = useMemo(() => {
    const term = query.trim().toLowerCase();
    if (!term) return activeFiles.slice(0, 8);
    return activeFiles
      .filter((file) => file.path.toLowerCase().includes(term) || file.name.toLowerCase().includes(term))
      .slice(0, 8);
  }, [activeFiles, query]);

  const hasQuery = query.trim().length > 0;
  const hasResults = fileMatches.length > 0 || symbols.length > 0;

  return (
    <div
      className="fixed inset-0 z-50 flex items-start justify-center bg-black/70 px-4 pt-[12vh]"
      role="dialog"
      aria-modal="true"
      aria-label="Recherche rapide Code Explorer"
      onKeyDown={(event) => {
        if (event.key === 'Escape') onClose();
      }}
    >
      <div className="w-full max-w-2xl overflow-hidden rounded-lg border border-neutral-800 bg-neutral-950 shadow-2xl">
        <div className="flex items-center gap-3 border-b border-neutral-800 px-4 py-3">
          <Search className="h-4 w-4 text-violet-300" aria-hidden />
          <input
            ref={inputRef}
            value={query}
            onChange={(event) => setQuery(event.target.value)}
            placeholder={repo ? 'Chercher un fichier, une classe, une methode...' : 'Selectionne un projet indexe'}
            disabled={!repo}
            className="min-w-0 flex-1 bg-transparent text-sm text-neutral-100 outline-none placeholder:text-neutral-600 disabled:cursor-not-allowed"
          />
          <div className="hidden rounded border border-neutral-800 px-1.5 py-0.5 text-[10px] text-neutral-500 sm:block">
            Ctrl K
          </div>
          <button
            type="button"
            onClick={onClose}
            className="rounded-md border border-neutral-800 p-1.5 text-neutral-400 hover:bg-neutral-900 hover:text-neutral-100"
            aria-label="Fermer la recherche rapide"
            title="Fermer"
          >
            <X className="h-4 w-4" aria-hidden />
          </button>
        </div>

        <div className="border-b border-neutral-900 px-4 py-2 text-xs text-neutral-500">
          {repo ? `Projet actif: ${repoName ?? repo}` : 'Aucun projet actif'}
        </div>

        <div className="max-h-[58vh] overflow-auto p-2">
          {!repo ? (
            <EmptyState text="Selectionne un projet indexe avant de lancer une recherche rapide." />
          ) : fileError ? (
            <ErrorState text={fileError} />
          ) : !hasQuery && fileLoading ? (
            <LoadingState text="Chargement de l'index des fichiers..." />
          ) : !hasQuery ? (
            <EmptyState text="Tape quelques lettres pour chercher dans les fichiers et les symboles." />
          ) : !hasResults && symbolLoading ? (
            <LoadingState text="Recherche des symboles..." />
          ) : !hasResults ? (
            <EmptyState text="Aucun resultat pour cette recherche." />
          ) : (
            <div className="space-y-3">
              {fileMatches.length > 0 && (
                <ResultSection title="Fichiers">
                  {fileMatches.map((file) => (
                    <ResultButton
                      key={file.path}
                      icon={<FileCode2 className="h-4 w-4 text-neutral-500" aria-hidden />}
                      title={file.name}
                      subtitle={file.path}
                      ariaLabel={`Ouvrir le fichier ${file.path}`}
                      onClick={() => onOpenSource({ path: file.path })}
                    />
                  ))}
                </ResultSection>
              )}
              {symbols.length > 0 && (
                <ResultSection title="Symboles">
                  {symbols.map((symbol) => (
                    <ResultButton
                      key={symbol.nodeId}
                      icon={<Braces className="h-4 w-4 text-violet-300" aria-hidden />}
                      title={symbol.name}
                      subtitle={`${symbol.label} - ${symbol.filePath}`}
                      ariaLabel={`Ouvrir le symbole ${symbol.name}`}
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
                    />
                  ))}
                </ResultSection>
              )}
              {symbolError && <ErrorState text={symbolError} />}
            </div>
          )}
        </div>
      </div>
    </div>
  );
}

function ResultSection({ title, children }: { title: string; children: ReactNode }) {
  return (
    <section>
      <div className="px-2 pb-1 text-[11px] font-medium uppercase tracking-wide text-neutral-600">
        {title}
      </div>
      <div className="space-y-1">{children}</div>
    </section>
  );
}

function ResultButton({
  icon,
  title,
  subtitle,
  ariaLabel,
  onClick,
}: {
  icon: ReactNode;
  title: string;
  subtitle: string;
  ariaLabel: string;
  onClick: () => void;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      className="flex w-full items-center gap-3 rounded-md px-2 py-2 text-left hover:bg-neutral-900"
      aria-label={ariaLabel}
    >
      <span className="flex h-8 w-8 shrink-0 items-center justify-center rounded-md border border-neutral-800 bg-neutral-900">
        {icon}
      </span>
      <span className="min-w-0 flex-1">
        <span className="block truncate text-sm font-medium text-neutral-100">{title}</span>
        <span className="block truncate text-xs text-neutral-500">{subtitle}</span>
      </span>
    </button>
  );
}

function LoadingState({ text }: { text: string }) {
  return (
    <div className="flex items-center gap-2 rounded-md px-3 py-6 text-sm text-neutral-500">
      <Loader2 className="h-4 w-4 animate-spin" aria-hidden />
      {text}
    </div>
  );
}

function EmptyState({ text }: { text: string }) {
  return <div className="rounded-md px-3 py-6 text-center text-sm text-neutral-600">{text}</div>;
}

function ErrorState({ text }: { text: string }) {
  return <div className="whitespace-pre-wrap rounded-md px-3 py-3 text-xs text-red-300">{text}</div>;
}

function flattenFileTree(nodes: FileTreeNode[]): FlatFile[] {
  const out: FlatFile[] = [];
  for (const node of nodes) {
    if (node.isDir) {
      out.push(...flattenFileTree(node.children));
    } else {
      out.push({ name: node.name, path: node.path });
    }
  }
  return out.sort((a, b) => a.path.localeCompare(b.path));
}

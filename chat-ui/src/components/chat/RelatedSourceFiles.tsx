import { useMemo, useState } from 'react';
import { AlertTriangle, Download, FileCode2, Loader2 } from 'lucide-react';
import { mcpClient } from '../../api/mcp-client';
import {
  extractSourceReferences,
  groupSourceReferences,
  type SourceReference,
  type SourceReferenceGroup,
  type SourceValidationState,
} from '../../utils/source-references';
import {
  downloadTextFile,
  relatedSourcesFilename,
  relatedSourcesMarkdown,
  sourceReferenceSummary,
} from '../../utils/related-sources-export';

interface Props {
  content: string;
  messageCreatedAt: number;
  repo: string | null;
  repoName: string | null;
  onOpenSourceReference?: (reference: SourceReference) => void;
  validSourcePaths?: ReadonlySet<string> | null;
  sourceValidationState?: SourceValidationState;
}

export function RelatedSourceFiles({
  content,
  messageCreatedAt,
  repo,
  repoName,
  onOpenSourceReference,
  validSourcePaths = null,
  sourceValidationState,
}: Props) {
  const validationState = sourceValidationState ?? (validSourcePaths ? 'ready' : 'unavailable');
  const detectedGroups = useMemo(
    () => groupSourceReferences(extractSourceReferences(content)).slice(0, 24),
    [content]
  );
  const groups = useMemo(
    () => filterGroupsByKnownPaths(detectedGroups, validSourcePaths, validationState),
    [detectedGroups, validSourcePaths, validationState]
  );
  const rejectedGroups = useMemo(
    () => rejectGroupsByKnownPaths(detectedGroups, validSourcePaths, validationState),
    [detectedGroups, validSourcePaths, validationState]
  );
  const [exportState, setExportState] = useState<'idle' | 'loading' | 'done' | 'error'>('idle');
  const [exportError, setExportError] = useState<string | null>(null);

  if (detectedGroups.length === 0) return null;

  const exportFiles = async () => {
    if (!repo) {
      setExportState('error');
      setExportError('Sélectionne un projet pour exporter les fichiers.');
      return;
    }

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
        relatedSourcesFilename(repoName ?? repo),
        relatedSourcesMarkdown({
          files,
          repoLabel: repoName ?? repo,
          createdAt: messageCreatedAt,
        })
      );
      setExportState('done');
      window.setTimeout(() => setExportState('idle'), 1400);
    } catch (error) {
      setExportState('error');
      setExportError(error instanceof Error ? error.message : String(error));
    }
  };

  return (
    <section className="related-sources mt-4 rounded-lg border p-3 text-xs" aria-label="Fichiers concernés">
      <div className="mb-2 flex items-center justify-between gap-3">
        <div className="flex min-w-0 items-center gap-2">
          <FileCode2 className="h-4 w-4 shrink-0 text-violet-300" aria-hidden />
          <div className="min-w-0">
            <div className="font-medium text-[var(--text-primary)]">Fichiers concernés</div>
            <div className="text-[11px] text-[var(--text-muted)]">
              {validationState === 'pending'
                ? "Validation dans l'index..."
                : `${groups.length} fichier${groups.length > 1 ? 's' : ''} vérifié${groups.length > 1 ? 's' : ''}`}
            </div>
          </div>
        </div>
        <button
          type="button"
          onClick={() => void exportFiles()}
          disabled={exportState === 'loading' || groups.length === 0}
          className="control-button flex shrink-0 items-center gap-1.5 rounded-md border px-2 py-1 disabled:cursor-wait disabled:opacity-60"
          aria-label="Exporter les fichiers concernés"
          title="Exporter les fichiers concernés"
          data-export-skip
        >
          {exportState === 'loading' ? (
            <Loader2 className="h-3.5 w-3.5 animate-spin" aria-hidden />
          ) : (
            <Download className="h-3.5 w-3.5" aria-hidden />
          )}
          <span>{exportState === 'done' ? 'Exporté' : 'Exporter'}</span>
        </button>
      </div>
      {validationState === 'pending' && (
        <div className="mb-2 flex items-center gap-2 rounded-md border border-sky-300/50 bg-sky-50 px-2 py-1.5 text-[11px] text-sky-800 dark:border-sky-500/30 dark:bg-sky-500/10 dark:text-sky-200">
          <Loader2 className="h-3.5 w-3.5 shrink-0 animate-spin" aria-hidden />
          <span>Validation des chemins cités dans l'index du projet avant ouverture ou export.</span>
        </div>
      )}
      {rejectedGroups.length > 0 && (
        <div className="mb-2 rounded-md border border-amber-300/50 bg-amber-50 px-2 py-1.5 text-[11px] text-amber-800 dark:border-amber-500/30 dark:bg-amber-500/10 dark:text-amber-200">
          <div className="flex items-start gap-1.5">
            <AlertTriangle className="mt-0.5 h-3.5 w-3.5 shrink-0" aria-hidden />
            <span>
              {rejectedGroups.length} citation{rejectedGroups.length > 1 ? 's' : ''} ignorée{rejectedGroups.length > 1 ? 's' : ''} car absente{rejectedGroups.length > 1 ? 's' : ''} de l'index :{' '}
              <span className="font-mono">{rejectedGroups.slice(0, 2).map((group) => group.path).join(', ')}</span>
              {rejectedGroups.length > 2 ? '…' : ''}
            </span>
          </div>
        </div>
      )}
      {groups.length > 0 ? (
        <div className="flex flex-wrap gap-1.5">
          {groups.map((group) => (
            <RelatedSourceFileButton
              key={group.path}
              group={group}
              onOpenSourceReference={onOpenSourceReference}
            />
          ))}
        </div>
      ) : (
        <div className="rounded-md border border-[var(--border)] px-2 py-1.5 text-[11px] text-[var(--text-muted)]">
          Aucun chemin cité n'a été validé dans le dépôt sélectionné.
        </div>
      )}
      {exportError && <div className="mt-2 text-[11px] text-red-300">{exportError}</div>}
    </section>
  );
}

function normalizeSourcePath(path: string): string {
  return path.replace(/\\/g, '/').toLowerCase();
}

function filterGroupsByKnownPaths(
  groups: SourceReferenceGroup[],
  knownPaths: ReadonlySet<string> | null,
  validationState: SourceValidationState
): SourceReferenceGroup[] {
  if (validationState === 'pending') return [];
  if (validationState !== 'ready' || !knownPaths) return groups;
  return groups.filter((group) => knownPaths.has(normalizeSourcePath(group.path)));
}

function rejectGroupsByKnownPaths(
  groups: SourceReferenceGroup[],
  knownPaths: ReadonlySet<string> | null,
  validationState: SourceValidationState
): SourceReferenceGroup[] {
  if (validationState !== 'ready' || !knownPaths) return [];
  return groups.filter((group) => !knownPaths.has(normalizeSourcePath(group.path)));
}

function RelatedSourceFileButton({
  group,
  onOpenSourceReference,
}: {
  group: SourceReferenceGroup;
  onOpenSourceReference?: (reference: SourceReference) => void;
}) {
  const first = group.references[0] ?? { path: group.path };
  return (
    <button
      type="button"
      onClick={() => onOpenSourceReference?.(first)}
      className="related-source-chip min-w-0 rounded-md border px-2 py-1 text-left font-mono"
      title={sourceReferenceSummary(group)}
      disabled={!onOpenSourceReference}
    >
      <span className="block max-w-[18rem] truncate">{group.path}</span>
      <span className="block text-[10px] text-[var(--text-muted)]">
        {group.references.length} référence{group.references.length > 1 ? 's' : ''}
        {first.startLine ? ` · ligne ${first.startLine}` : ''}
      </span>
    </button>
  );
}

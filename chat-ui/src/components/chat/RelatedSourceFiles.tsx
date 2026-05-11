import { useMemo, useState } from 'react';
import { Download, FileCode2, Loader2 } from 'lucide-react';
import { mcpClient } from '../../api/mcp-client';
import {
  extractSourceReferences,
  groupSourceReferences,
  type SourceReference,
  type SourceReferenceGroup,
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
}

export function RelatedSourceFiles({
  content,
  messageCreatedAt,
  repo,
  repoName,
  onOpenSourceReference,
}: Props) {
  const groups = useMemo(
    () => groupSourceReferences(extractSourceReferences(content)).slice(0, 24),
    [content]
  );
  const [exportState, setExportState] = useState<'idle' | 'loading' | 'done' | 'error'>('idle');
  const [exportError, setExportError] = useState<string | null>(null);

  if (groups.length === 0) return null;

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
              {groups.length} fichier{groups.length > 1 ? 's' : ''} détecté{groups.length > 1 ? 's' : ''}
            </div>
          </div>
        </div>
        <button
          type="button"
          onClick={() => void exportFiles()}
          disabled={exportState === 'loading'}
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
      <div className="flex flex-wrap gap-1.5">
        {groups.map((group) => (
          <RelatedSourceFileButton
            key={group.path}
            group={group}
            onOpenSourceReference={onOpenSourceReference}
          />
        ))}
      </div>
      {exportError && <div className="mt-2 text-[11px] text-red-300">{exportError}</div>}
    </section>
  );
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

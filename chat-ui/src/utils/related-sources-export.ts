import type { SourceContent } from '../api/mcp-client';
import type { SourceReferenceGroup } from './source-references';

export interface RelatedSourceExportFile {
  group: SourceReferenceGroup;
  source: SourceContent | null;
  error: string | null;
}

export function relatedSourcesMarkdown({
  files,
  repoLabel,
  createdAt,
  title = 'Fichiers concernés',
  note,
}: {
  files: RelatedSourceExportFile[];
  repoLabel: string;
  createdAt: number;
  title?: string;
  note?: string;
}): string {
  const lines = [
    `# ${title}`,
    '',
    `- Projet: ${repoLabel}`,
    `- Export: ${new Date(createdAt).toLocaleString()}`,
    `- Fichiers: ${files.length}`,
    '',
  ];

  if (note) {
    lines.push(note, '');
  }

  for (const item of files) {
    lines.push(`## ${item.group.path}`, '');
    lines.push(`Références: ${sourceReferenceSummary(item.group)}`, '');
    if (item.error || !item.source) {
      lines.push(`> Export impossible: ${item.error ?? 'contenu indisponible'}`, '');
      continue;
    }
    lines.push(`Lignes: ${item.source.totalLines}${item.source.truncated ? ' (contenu tronqué)' : ''}`, '');
    lines.push(`\`\`\`${item.source.language ?? ''}`);
    lines.push(item.source.content.trimEnd());
    lines.push('```', '');
  }

  return lines.join('\n').trimEnd() + '\n';
}

export function sourceReferenceSummary(group: SourceReferenceGroup): string {
  return group.references
    .map((reference) => {
      if (!reference.startLine) return reference.path;
      if (reference.endLine && reference.endLine !== reference.startLine) {
        return `${reference.path}:${reference.startLine}-${reference.endLine}`;
      }
      return `${reference.path}:${reference.startLine}`;
    })
    .join(', ');
}

export function relatedSourcesFilename(repoLabel: string, kind = 'fichiers-concernes'): string {
  const stamp = new Date().toISOString().replace(/[-:]/g, '').replace(/\..+$/, '').replace('T', '-');
  const slug = repoLabel
    .toLowerCase()
    .normalize('NFD')
    .replace(/[\u0300-\u036f]/g, '')
    .replace(/[^a-z0-9]+/g, '-')
    .replace(/^-+|-+$/g, '')
    .slice(0, 60);
  return `code-explorer-${kind}-${slug || 'projet'}-${stamp}.md`;
}

export function downloadTextFile(
  filename: string,
  content: string,
  mimeType = 'text/markdown;charset=utf-8'
) {
  const blob = new Blob([content], { type: mimeType });
  downloadBlobFile(filename, blob);
}

export function downloadBlobFile(filename: string, blob: Blob, mimeType?: string) {
  const typedBlob = mimeType && blob.type !== mimeType ? new Blob([blob], { type: mimeType }) : blob;
  const url = URL.createObjectURL(typedBlob);
  const a = document.createElement('a');
  a.href = url;
  a.download = filename;
  document.body.appendChild(a);
  a.click();
  a.remove();
  URL.revokeObjectURL(url);
}

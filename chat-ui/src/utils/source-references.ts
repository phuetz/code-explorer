export interface SourceReference {
  path: string;
  startLine?: number;
  endLine?: number;
}

export interface SourceReferenceGroup {
  path: string;
  references: SourceReference[];
}

const SOURCE_REFERENCE_HASH = '#gitnexus-source';
const SOURCE_REFERENCE_RE =
  /(^|[\s([{"'`])((?:[A-Za-z0-9_.-]+[\\/])+[A-Za-z0-9_.-]+\.(?:cshtml|csproj|config|razor|tsx|jsx|toml|swift|html|java|json|yaml|bat|cmd|css|cs|go|hpp|js|kt|md|php|ps1|py|rb|rs|sln|sql|ts|xml|yml|h))(?::(\d+)(?:-(\d+))?)?/g;

export function linkifySourceReferences(markdown: string): string {
  return markdown
    .split(/(```[\s\S]*?```|~~~[\s\S]*?~~~)/g)
    .map((block) => {
      if (block.startsWith('```') || block.startsWith('~~~')) return block;
      return block
        .split(/(`[^`\n]+`)/g)
        .map((inline) => (inline.startsWith('`') ? inline : linkifySourceReferenceSegment(inline)))
        .join('');
    })
    .join('');
}

export function extractSourceReferences(markdown: string): SourceReference[] {
  const references: SourceReference[] = [];
  const seen = new Set<string>();
  for (const block of markdown.split(/(```[\s\S]*?```|~~~[\s\S]*?~~~)/g)) {
    if (block.startsWith('```') || block.startsWith('~~~')) continue;
    SOURCE_REFERENCE_RE.lastIndex = 0;
    let match: RegExpExecArray | null;
    while ((match = SOURCE_REFERENCE_RE.exec(block)) !== null) {
      const [, , path, start, end] = match;
      const reference: SourceReference = {
        path: path.replace(/\\/g, '/'),
        startLine: start ? Number(start) : undefined,
        endLine: end ? Number(end) : start ? Number(start) : undefined,
      };
      const key = sourceReferenceKey(reference);
      if (seen.has(key)) continue;
      seen.add(key);
      references.push(reference);
    }
  }
  return references;
}

export function groupSourceReferences(references: SourceReference[]): SourceReferenceGroup[] {
  const groups = new Map<string, SourceReference[]>();
  for (const reference of references) {
    const path = reference.path.replace(/\\/g, '/');
    const list = groups.get(path) ?? [];
    if (!list.some((item) => sourceReferenceKey(item) === sourceReferenceKey(reference))) {
      list.push({ ...reference, path });
    }
    groups.set(path, list);
  }
  return Array.from(groups.entries()).map(([path, list]) => ({
    path,
    references: list.sort((a, b) => (a.startLine ?? 0) - (b.startLine ?? 0)),
  }));
}

export function parseSourceReferenceHref(href: string | undefined): SourceReference | null {
  if (!href?.startsWith(SOURCE_REFERENCE_HASH)) return null;
  const queryIndex = href.indexOf('?');
  if (queryIndex === -1) return null;
  const params = new URLSearchParams(href.slice(queryIndex + 1));
  const path = params.get('path');
  if (!path) return null;
  return {
    path,
    startLine: parsePositiveInt(params.get('start')),
    endLine: parsePositiveInt(params.get('end')),
  };
}

function linkifySourceReferenceSegment(text: string): string {
  return text.replace(
    SOURCE_REFERENCE_RE,
    (full, prefix: string, path: string, start?: string, end?: string, offset?: number, input?: string) => {
      if (prefix === '(' && typeof offset === 'number' && input?.[offset - 1] === ']') {
        return full;
      }
      const normalizedPath = path.replace(/\\/g, '/');
      const label = `${normalizedPath}${start ? `:${start}${end ? `-${end}` : ''}` : ''}`;
      const href = buildSourceReferenceHref({
        path: normalizedPath,
        startLine: start ? Number(start) : undefined,
        endLine: end ? Number(end) : start ? Number(start) : undefined,
      });
      return `${prefix}[${label}](${href})`;
    }
  );
}

function buildSourceReferenceHref(reference: SourceReference): string {
  const query = new URLSearchParams({ path: reference.path });
  if (reference.startLine) query.set('start', String(reference.startLine));
  if (reference.endLine) query.set('end', String(reference.endLine));
  return `${SOURCE_REFERENCE_HASH}?${query.toString()}`;
}

function sourceReferenceKey(reference: SourceReference): string {
  return `${reference.path.replace(/\\/g, '/')}:${reference.startLine ?? ''}:${reference.endLine ?? ''}`;
}

function parsePositiveInt(value: string | null): number | undefined {
  if (!value) return undefined;
  const parsed = Number(value);
  return Number.isInteger(parsed) && parsed > 0 ? parsed : undefined;
}

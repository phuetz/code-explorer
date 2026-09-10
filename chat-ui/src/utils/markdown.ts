const MERMAID_GRAPH_TYPES = [
  'flowchart',
  'sequenceDiagram',
  'classDiagram',
  'classDiagram-v2',
  'erDiagram',
  'stateDiagram',
  'stateDiagram-v2',
  'gantt',
  'pie',
  'mindmap',
  'gitGraph',
  'journey',
  'graph',
];

const MERMAID_START_RE =
  /^\s*(flowchart\s+(?:TB|TD|BT|RL|LR)|graph\s+(?:TB|TD|BT|RL|LR)|sequenceDiagram|classDiagram(?:-v2)?|erDiagram|stateDiagram(?:-v2)?|gantt|pie\b|mindmap|gitGraph|journey)\b/i;

const CODE_LANGUAGE_ALIASES = new Map<string, string>([
  ['c#', 'csharp'],
  ['cs', 'csharp'],
  ['ps1', 'powershell'],
  ['pwsh', 'powershell'],
  ['shell', 'bash'],
  ['sh', 'bash'],
  ['zsh', 'bash'],
  ['js', 'javascript'],
  ['jsx', 'jsx'],
  ['ts', 'typescript'],
  ['tsx', 'tsx'],
  ['py', 'python'],
  ['yml', 'yaml'],
]);

const MERMAID_LINE_RE = new RegExp(
  String.raw`^\s*(?:\}|[+\-#~]\w|subgraph\b|end\b|participant\b|actor\b|autonumber\b|loop\b|alt\b|opt\b|else\b|par\b|and\b|rect\b|note\b|activate\b|deactivate\b|class\b|classDef\b|click\b|style\b|linkStyle\b|title\b|section\b|dateFormat\b|axisFormat\b|todayMarker\b|[A-Za-z0-9_.-]+(?:\s*(?:-->|---|-.->|==>|-\.-|--|:::|::|[-=]+>>[+\-]?|--x|-x|--\)|-\))|\s*[\[\(\{>]))`,
  'i'
);

export function normalizeCodeFenceLanguage(language: string | undefined): string | undefined {
  const normalized = language?.trim().toLowerCase();
  if (!normalized) return undefined;
  return CODE_LANGUAGE_ALIASES.get(normalized) ?? normalized;
}

export function looksLikeMermaid(text: string): boolean {
  const head = text.trimStart().split(/\s|\n/, 1)[0] ?? '';
  return MERMAID_GRAPH_TYPES.some((type) => type.toLowerCase() === head.toLowerCase());
}

function isBareMermaidContinuation(line: string): boolean {
  if (!line.trim()) return true;
  if (/^\s+/.test(line)) return true;
  return MERMAID_LINE_RE.test(line);
}

export function normalizeBareMermaid(markdown: string): string {
  const lines = markdown.split(/\r?\n/);
  const out: string[] = [];
  let inFence = false;

  for (let i = 0; i < lines.length; i += 1) {
    const line = lines[i];
    if (/^\s*```/.test(line)) {
      inFence = !inFence;
      out.push(line);
      continue;
    }

    if (!inFence && MERMAID_START_RE.test(line)) {
      out.push('```mermaid');
      out.push(line);
      i += 1;
      while (i < lines.length && isBareMermaidContinuation(lines[i])) {
        out.push(lines[i]);
        i += 1;
      }
      while (out[out.length - 1] === '') out.pop();
      out.push('```');
      i -= 1;
      continue;
    }

    out.push(line);
  }

  return out.join('\n');
}

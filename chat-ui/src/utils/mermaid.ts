const FLOWCHART_START_RE = /^\s*(?:flowchart|graph)\b/im;
const FLOWCHART_LABEL_NEEDS_QUOTES_RE = /[(){}<>:,]/;
const FLOWCHART_SHAPE_PREFIXES = new Set(['(', '[', '{', '/', '\\', '>']);

export function repairMermaidSource(text: string): string {
  const normalized = text.replace(/\r\n?/g, '\n');
  if (!FLOWCHART_START_RE.test(normalized)) return normalized;

  return normalized
    .split('\n')
    .map((line) => line.replace(/-->\s*\|\|\s*/g, '--> '))
    .map((line) => quoteProblemFlowchartLabels(line))
    .join('\n');
}

function quoteProblemFlowchartLabels(line: string): string {
  let out = '';
  let index = 0;

  while (index < line.length) {
    if (line[index] !== '[') {
      out += line[index];
      index += 1;
      continue;
    }

    const next = line[index + 1];
    if (!next || next === '[' || next === '"' || next === "'") {
      out += line[index];
      index += 1;
      continue;
    }

    const end = line.indexOf(']', index + 1);
    if (end === -1) {
      out += line.slice(index);
      break;
    }

    const label = line.slice(index + 1, end);
    if (line[end + 1] === ']' || shouldKeepRawFlowchartLabel(label)) {
      out += line.slice(index, end + 1);
    } else {
      out += `["${escapeMermaidQuotedLabel(label)}"]`;
    }
    index = end + 1;
  }

  return out;
}

function shouldKeepRawFlowchartLabel(label: string): boolean {
  const trimmed = label.trim();
  if (!trimmed || !FLOWCHART_LABEL_NEEDS_QUOTES_RE.test(trimmed)) return true;

  const first = trimmed[0];
  if (FLOWCHART_SHAPE_PREFIXES.has(first)) return true;

  return false;
}

function escapeMermaidQuotedLabel(label: string): string {
  return label.replace(/\\/g, '\\\\').replace(/"/g, '\\"');
}

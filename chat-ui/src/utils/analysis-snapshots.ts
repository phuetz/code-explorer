import type { AnalysisSnapshot, Message, Session } from '../types/chat';
import { extractSourceReferences, groupSourceReferences, type SourceReferenceGroup } from './source-references';

export function buildAnalysisSnapshot({
  session,
  repo,
  repoName,
  sourceGroups,
}: {
  session: Session;
  repo: string | null;
  repoName: string | null;
  sourceGroups: SourceReferenceGroup[];
}): AnalysisSnapshot {
  const now = Date.now();
  return {
    id: crypto.randomUUID(),
    title: analysisTitle(session.title, now),
    repo,
    repoName,
    createdAt: now,
    updatedAt: now,
    sourceReferences: sourceGroups.flatMap((group) => group.references),
    summary: {
      fileCount: sourceGroups.length,
      diagramCount: countMermaidDiagrams(session.messages),
      toolCallCount: session.messages.reduce((count, message) => count + (message.toolCalls?.length ?? 0), 0),
      decisionCount: countDecisionHints(session.messages),
    },
  };
}

export function snapshotSourceGroups(snapshot: AnalysisSnapshot): SourceReferenceGroup[] {
  return groupSourceReferences(snapshot.sourceReferences);
}

function analysisTitle(sessionTitle: string, createdAt: number): string {
  const stamp = new Date(createdAt).toLocaleString();
  return `${sessionTitle || 'Analyse'} - ${stamp}`;
}

function countMermaidDiagrams(messages: Message[]): number {
  return messages.reduce(
    (count, message) => count + (message.content.match(/```mermaid\b/gi)?.length ?? 0),
    0
  );
}

function countDecisionHints(messages: Message[]): number {
  const decisionRe = /^\s*(?:#{2,5}\s*)?(?:Décision|Decision|Choix|Risque|Action|À faire|A faire)\b/im;
  return messages.reduce(
    (count, message) =>
      count + message.content.split('\n').filter((line) => decisionRe.test(line)).length,
    0
  );
}

export function currentSessionSourceGroups(session: Session | null): SourceReferenceGroup[] {
  if (!session) return [];
  return groupSourceReferences(
    session.messages.flatMap((message) =>
      message.role === 'assistant' ? extractSourceReferences(message.content) : []
    )
  );
}

import { mcpClient } from '../api/mcp-client';
import type { WorkdocStateSummary } from '../api/mcp-client';
import type { WorkDocument } from '../types/chat';

export async function saveWorkDocumentState(document: WorkDocument): Promise<void> {
  await mcpClient.saveWorkDocumentState(document);
}

export async function loadWorkDocumentState(id: string): Promise<WorkDocument | null> {
  try {
    const response = await mcpClient.loadWorkDocumentState<WorkDocument>(id);
    return isWorkDocument(response.document) ? response.document : null;
  } catch (error) {
    if (error instanceof Error && /HTTP 404\b/.test(error.message)) {
      return null;
    }
    throw error;
  }
}

export async function listWorkDocumentStateSummaries(): Promise<WorkdocStateSummary[]> {
  const response = await mcpClient.listWorkDocumentStates();
  return response.documents;
}

export async function deleteWorkDocumentState(id: string): Promise<void> {
  await mcpClient.deleteWorkDocumentState(id);
}

function isWorkDocument(value: unknown): value is WorkDocument {
  if (!value || typeof value !== 'object') return false;
  const candidate = value as Partial<WorkDocument>;
  return (
    typeof candidate.id === 'string' &&
    typeof candidate.filename === 'string' &&
    Array.isArray(candidate.questions)
  );
}

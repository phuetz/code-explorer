import type { LlmConfigInfo } from '../api/mcp-client';

export type Role = 'user' | 'assistant' | 'system';
export type WorkspaceTabPreference = 'sources' | 'graph';

export interface Message {
  id: string;
  role: Role;
  content: string;
  createdAt: number;
  llm?: LlmConfigInfo | null;
  verified?: boolean;
  toolCalls?: ToolCall[];
}

export interface ToolCall {
  id: string;
  name: string;
  args: Record<string, unknown>;
  result?: unknown;
  status: 'pending' | 'running' | 'done' | 'error';
}

export interface AnalysisSourceReference {
  path: string;
  startLine?: number;
  endLine?: number;
}

export interface AnalysisSnapshot {
  id: string;
  title: string;
  repo: string | null;
  repoName: string | null;
  createdAt: number;
  updatedAt: number;
  sourceReferences: AnalysisSourceReference[];
  summary: {
    fileCount: number;
    diagramCount: number;
    toolCallCount: number;
    decisionCount: number;
  };
}

export type WorkDocumentQuestionStatus = 'pending' | 'answering' | 'answered' | 'error';

export interface WorkDocumentQuestion {
  id: string;
  order: number;
  label: string;
  text: string;
  context: string;
  status: WorkDocumentQuestionStatus;
  prompt?: string;
  answer?: string;
  error?: string;
  answeredAt?: number;
  messageIds?: {
    user?: string;
    assistant?: string;
  };
}

export interface WorkDocument {
  id: string;
  filename: string;
  importedAt: number;
  repo: string | null;
  repoName: string | null;
  sessionId?: string;
  sourceBytes: number;
  markdownChars: number;
  sourceMarkdown?: string;
  questions: WorkDocumentQuestion[];
}

export interface Session {
  id: string;
  title: string;
  createdAt: number;
  updatedAt: number;
  messages: Message[];
  analyses?: AnalysisSnapshot[];
}

export interface MCPTool {
  name: string;
  description: string;
  inputSchema: Record<string, unknown>;
}

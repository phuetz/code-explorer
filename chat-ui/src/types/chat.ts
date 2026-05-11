export type Role = 'user' | 'assistant' | 'system';

export interface Message {
  id: string;
  role: Role;
  content: string;
  createdAt: number;
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

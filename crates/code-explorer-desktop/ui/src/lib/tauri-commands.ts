/**
 * Type-safe wrappers for Tauri IPC commands.
 * These types mirror the Rust response types in src/types.rs.
 */
import { safeInvoke as invoke } from "./tauri-env";

// ─── Types ───────────────────────────────────────────────────────────────

export interface RepoInfo {
  name: string;
  path: string;
  indexedAt: string;
  lastCommit: string;
  files?: number;
  nodes?: number;
  edges?: number;
  communities?: number;
  indexDurationMs?: number;
}

export interface PhaseTiming {
  name: string;
  durationMs: number;
}

export interface IndexMetrics {
  schemaVersion: number;
  toolVersion: string;
  indexedAt: string;
  totalDurationMs: number;
  phases: PhaseTiming[];
  files: number;
  nodes: number;
  edges: number;
  communities: number;
  processes: number;
  filesPerSec: number;
  nodesPerSec: number;
}

export interface CytoNode {
  id: string;
  label: string;
  name: string;
  filePath: string;
  startLine?: number;
  endLine?: number;
  isExported?: boolean;
  community?: string;
  language?: string;
  description?: string;
  parameterCount?: number;
  returnType?: string;
  layerType?: string;
  entryPointScore?: number;
  entryPointReason?: string;
  isTraced?: boolean;
  traceCallCount?: number;
  isDeadCandidate?: boolean;
  complexity?: number;
  depth?: number;
}

export interface CytoEdge {
  id: string;
  source: string;
  target: string;
  relType: string;
  confidence: number;
}

export interface GraphStats {
  nodeCount: number;
  edgeCount: number;
  truncated: boolean;
}

export interface GraphPayload {
  nodes: CytoNode[];
  edges: CytoEdge[];
  stats: GraphStats;
}

export type ZoomLevel = "package" | "module" | "symbol";

export interface GraphFilter {
  zoomLevel: ZoomLevel;
  labels?: string[];
  filePaths?: string[];
  maxNodes?: number;
}

export interface SearchResult {
  nodeId: string;
  name: string;
  label: string;
  filePath: string;
  score: number;
  startLine?: number;
  endLine?: number;
}

export interface RelatedNode {
  id: string;
  name: string;
  label: string;
  filePath: string;
}

export interface CommunityInfo {
  id: string;
  name: string;
  description?: string;
  memberCount?: number;
  cohesion?: number;
}

export interface SymbolContext {
  node: CytoNode;
  callers: RelatedNode[];
  callees: RelatedNode[];
  imports: RelatedNode[];
  importedBy: RelatedNode[];
  inherits: RelatedNode[];
  inheritedBy: RelatedNode[];
  community?: CommunityInfo;
}

export interface ImpactNode {
  node: CytoNode;
  depth: number;
  path: string[];
}

export interface ImpactSummary {
  upstreamCount: number;
  downstreamCount: number;
  affectedFilesCount: number;
  maxDepth: number;
}

export interface ImpactResult {
  target: CytoNode;
  upstream: ImpactNode[];
  downstream: ImpactNode[];
  graph: GraphPayload;
  affectedFiles: string[];
  summary: ImpactSummary;
}

export interface FileTreeNode {
  name: string;
  path: string;
  isDir: boolean;
  children: FileTreeNode[];
}

export interface FileContent {
  path: string;
  content: string;
  language?: string;
  totalLines: number;
}

// ─── Documentation Types ────────────────────────────────────────────

export interface DocPage {
  id: string;
  title: string;
  path?: string;
  icon?: string;
  children?: DocPage[];
}

export interface DocStats {
  files: number;
  nodes: number;
  edges: number;
  modules: number;
}

export interface DocIndex {
  title: string;
  generatedAt: string;
  stats: DocStats;
  pages: DocPage[];
}

export interface DocContent {
  path: string;
  content: string;
  title: string;
}

// ─── Chat Types ─────────────────────────────────────────────────────

export interface ChatMessage {
  role: string;
  content: string;
}

export interface ChatRequest {
  question: string;
  history: ChatMessage[];
}

export interface ChatSource {
  nodeId: string;
  symbolName: string;
  symbolType: string;
  filePath: string;
  startLine?: number;
  endLine?: number;
  snippet?: string;
  callers?: string[];
  callees?: string[];
  community?: string;
  relevanceScore: number;
}

export interface ChatResponse {
  answer: string;
  sources: ChatSource[];
  model?: string | null;
}

export interface ChatConfig {
  provider: string;
  apiKey: string;
  baseUrl: string;
  model: string;
  maxTokens: number;
  reasoningEffort: string;
}

export interface ChatConnectionTestResult {
  ok: boolean;
  /** HTTP status code (0 for network errors). */
  status: number;
  model: string;
  /** Trimmed model reply (when ok) or short body preview (when error). */
  message: string;
  latencyMs: number;
}

// ─── Chat agent tools (Theme B) ──────────────────────────────────────

/** Whether the active repo's chat is running with embeddings or BM25-only. */
export interface ChatSearchCapabilities {
  embeddingsLoaded: boolean;
  modelName?: string;
  vectorCount?: number;
}

/** One MCP prompt template — surfaced in the UI as a one-click recipe. */
export interface McpPromptDefinition {
  name: string;
  description: string;
  arguments: Array<{
    name: string;
    description: string;
    required: boolean;
  }>;
}

/** Descriptor for an agent/MCP tool surfaced to the chat UI. */
export interface ChatToolDescriptor {
  name: string;
  description: string;
  category: string;
  /** JSON schema (draft-07) for the tool's args. */
  parameters: unknown;
}

export interface ChatToolRetryRequest {
  sessionId: string;
  messageId: string;
  toolCallId: string;
  name: string;
  /** New JSON-encoded args. When absent, the backend reuses `priorArgs`. */
  newArgs?: string;
  /** Prior JSON-encoded args, used when `newArgs` is not supplied. */
  priorArgs?: string;
}

export interface ChatToolRetryResult {
  toolCallId: string;
  name: string;
  args: string;
  result: string;
  durationMs: number;
  /** "success" | "error". */
  status: string;
}

// ─── Chat Intelligence Types ────────────────────────────────────────

export type QueryComplexity = "simple" | "medium" | "complex";

export interface ChatContextFilter {
  files: string[];
  symbols: string[];
  modules: string[];
  languages: string[];
  labels: string[];
}

export interface ChatSmartRequest {
  question: string;
  history: ChatMessage[];
  filters?: ChatContextFilter;
  deepResearch?: boolean;
}

export interface QueryAnalysis {
  complexity: QueryComplexity;
  suggestedTools: string[];
  estimatedSteps: number;
  reasoning: string;
  keywords: string[];
  needsCrossFile: boolean;
  needsImpact: boolean;
}

export type PlanStatus = "pending" | "running" | "completed" | "failed";
export type StepStatus = "pending" | "running" | "completed" | "failed" | "skipped";

export interface StepResult {
  summary: string;
  sources: ChatSource[];
  data?: unknown;
  durationMs: number;
}

export interface ResearchStep {
  id: string;
  order: number;
  tool: string;
  description: string;
  params: Record<string, unknown>;
  dependsOn: string[];
  status: StepStatus;
  result?: StepResult;
}

export interface ResearchPlan {
  id: string;
  query: string;
  analysis: QueryAnalysis;
  steps: ResearchStep[];
  status: PlanStatus;
}

export interface ChatSmartResponse {
  answer: string;
  sources: ChatSource[];
  model?: string | null;
  plan?: ResearchPlan;
  complexity: QueryComplexity;
}

// ─── Feature-Dev ─────────────────────────────────────────────────

export type FeatureDevPhase = "explorer" | "architect" | "reviewer";
export type PhaseStatus = "pending" | "running" | "completed" | "failed";

export interface SurfaceAnalysis {
  modules: string[];
  entryPoints: string[];
  layers: string[];
  keyFiles: string[];
}

export interface FilePlan {
  path: string;
  purpose: string;
}

export interface Blueprint {
  filesToCreate: FilePlan[];
  filesToModify: FilePlan[];
  buildSequence: string[];
  dataFlow?: string;
}

export interface ReviewIssue {
  severity: string;
  confidence: number;
  title: string;
  detail: string;
  file?: string;
}

export interface Review {
  issues: ReviewIssue[];
  predictedImpact?: string;
  verdict: string;
}

export interface FeatureDevSection {
  phase: FeatureDevPhase;
  title: string;
  markdown: string;
  surface?: SurfaceAnalysis;
  blueprint?: Blueprint;
  review?: Review;
  durationMs: number;
}

export interface FeatureDevArtifact {
  id: string;
  featureDescription: string;
  sections: FeatureDevSection[];
  status: PlanStatus;
  summary?: string;
}

export interface FeatureDevRequest {
  featureDescription: string;
  filters?: ChatContextFilter;
  explorerOnly?: boolean;
}

export interface FeatureDevPhaseEvent {
  artifactId: string;
  phase: FeatureDevPhase;
  status: PhaseStatus;
  message?: string;
}

export interface FeatureDevSectionEvent {
  artifactId: string;
  section: FeatureDevSection;
}

// ─── Code-Review ─────────────────────────────────────────────────

export interface CodeReviewSignals {
  changedFiles: string[];
  changedSymbols: string[];
  affectedCount: number;
  affectedProcesses: string[];
  hotspotFiles: string[];
  untracedSymbols: string[];
  deadCandidates: string[];
  riskLevel: string;
}

export interface CodeReviewArtifact {
  id: string;
  scopeSummary: string;
  status: PlanStatus;
  signals: CodeReviewSignals;
  review: Review;
  markdown: string;
  durationMs: number;
}

export interface CodeReviewRequest {
  targetSymbols?: string[];
  minConfidence?: number;
  includeAllSeverities?: boolean;
}

// ─── Simplify ────────────────────────────────────────────────────

export interface ComplexSymbol {
  name: string;
  filePath: string;
  complexity: number;
  label: string;
}

export interface DuplicateGroup {
  name: string;
  occurrences: number;
  files: string[];
}

export interface SimplifySignals {
  scope: string;
  complexSymbols: ComplexSymbol[];
  deadCandidates: string[];
  untracedSymbols: string[];
  llmSmells: string[];
  duplicateGroups: DuplicateGroup[];
  totalFiles: number;
  totalSymbols: number;
}

export interface SimplifyProposal {
  kind: string; // extract | delete | merge | inline | rename
  target: string;
  rationale: string;
  confidence: number;
}

export interface SimplifyArtifact {
  id: string;
  status: PlanStatus;
  signals: SimplifySignals;
  proposals: SimplifyProposal[];
  markdown: string;
  durationMs: number;
}

export interface SimplifyRequest {
  target?: string;
  minComplexity?: number;
}

// ─── Rename Refactor ─────────────────────────────────────────────

export interface RenameRequest {
  target: string;
  newName: string;
  dryRun?: boolean;
}

export interface RenameEdit {
  file: string;
  line: number;
  col: number;
  oldText: string;
  newText: string;
  snippet: string;
  confidence: number;
  reason: string;
}

export interface RenameResult {
  target: string;
  newName: string;
  dryRun: boolean;
  filesAffected: number;
  graphEdits: RenameEdit[];
  textSearchEdits: RenameEdit[];
  applied?: unknown;
}

// ─── Bookmarks ───────────────────────────────────────────────────

export interface Bookmark {
  nodeId: string;
  name: string;
  label: string;
  filePath?: string;
  note?: string;
  createdAt: number;
}

// ─── Comments (per-node threads) ─────────────────────────────────

export interface Comment {
  id: string;
  nodeId: string;
  author: string;
  body: string;
  createdAt: number;
}

// ─── Wiki generation ─────────────────────────────────────────────

export interface WikiGenerateRequest {
  outDir?: string;
  withIndex?: boolean;
  enrichWithLlm?: boolean;
}

export interface WikiPage {
  module: string;
  filename: string;
  path: string;
  memberCount: number;
  sizeBytes: number;
}

export interface WikiGenerateResult {
  outDir: string;
  pages: WikiPage[];
  totalFiles: number;
}

export interface WikiProgressEvent {
  current: number;
  total: number;
  module: string;
}

// ─── User data bundle ────────────────────────────────────────────

export interface BundleExportRequest {
  outPath?: string;
}

export interface BundleExportResult {
  path: string;
  sizeBytes: number;
  fileCount: number;
}

export interface BundleImportRequest {
  bundlePath: string;
  overwrite?: boolean;
}

export interface BundleImportResult {
  restored: number;
  skipped: number;
  entries: string[];
}

// ─── User slash commands (light plugin system) ───────────────────

export interface UserCommand {
  id: string;
  name: string;
  template: string;
  mode?: string;
  description?: string;
  updatedAt: number;
}

export interface ResolvedCommand {
  text: string;
  mode: string;
}

// ─── Workflow editor ─────────────────────────────────────────────

export interface WorkflowStep {
  id: string;
  kind: "search" | "cypher" | "impact" | "read_file" | "llm" | string;
  label: string;
  params: Record<string, unknown>;
}

export interface Workflow {
  id: string;
  name: string;
  description?: string;
  steps: WorkflowStep[];
  updatedAt: number;
}

export interface WorkflowSummary {
  id: string;
  name: string;
  stepCount: number;
  updatedAt: number;
}

export interface StepRun {
  stepId: string;
  label: string;
  kind: string;
  status: "ok" | "error" | "skipped" | string;
  durationMs: number;
  text: string;
  json?: unknown;
  error?: string;
}

export interface WorkflowRunResult {
  workflowId: string;
  steps: StepRun[];
  totalMs: number;
}

// ─── Custom dashboards ───────────────────────────────────────────

export interface DashboardWidget {
  id: string;
  title: string;
  kind: "metric" | "table" | "bar" | string;
  cypher: string;
  valueColumn?: string;
  labelColumn?: string;
  description?: string;
}

export interface Dashboard {
  id: string;
  name: string;
  description?: string;
  widgets: DashboardWidget[];
  updatedAt: number;
}

export interface DashboardSummary {
  id: string;
  name: string;
  widgetCount: number;
  updatedAt: number;
}

// ─── Snapshot history + diff ─────────────────────────────────────

export interface SnapshotMeta {
  id: string;
  label: string;
  createdAt: number;
  nodeCount: number;
  edgeCount: number;
  sizeBytes: number;
  /** Theme C — commit-aware snapshots */
  commitSha?: string;
  authoredAt?: number;
  subject?: string;
}

export interface CommitInfo {
  sha: string;
  shortSha: string;
  author: string;
  authoredAt: number;
  subject: string;
}

export interface DiffNode {
  id: string;
  name: string;
  label: string;
  filePath: string;
}

export interface ModifiedNode {
  id: string;
  name: string;
  label: string;
  filePath: string;
  changes: string[];
}

export interface LabelDelta {
  label: string;
  fromCount: number;
  toCount: number;
  added: number;
  removed: number;
}

export interface SnapshotDiff {
  fromId: string;
  toId: string;
  fromNodeCount: number;
  toNodeCount: number;
  fromEdgeCount: number;
  toEdgeCount: number;
  byLabel: LabelDelta[];
  addedSample: DiffNode[];
  removedSample: DiffNode[];
  modifiedSample: ModifiedNode[];
  totalAdded: number;
  totalRemoved: number;
  totalModified: number;
}

export interface SnapshotDiffRequest {
  from: string; // snapshot id, or "live"
  to: string;
}

// ─── Activity history ────────────────────────────────────────────

export interface ActivityEntry {
  timestamp: number;
  commit?: string | null;
  nodeCount: number;
  edgeCount: number;
  functionCount: number;
  fileCount: number;
  deadCount: number;
  tracedCount: number;
  communityCount: number;
  note?: string | null;
}

// ─── Multi-repo overview ─────────────────────────────────────────

export interface LanguageStat {
  language: string;
  fileCount: number;
}

export interface RepoOverview {
  name: string;
  path: string;
  indexedAt: string;
  lastCommit: string;
  nodeCount: number;
  edgeCount: number;
  fileCount: number;
  functionCount: number;
  classCount: number;
  communityCount: number;
  deadCount: number;
  tracedCount: number;
  tracingCoverage: number;
  indexDurationMs?: number | null;
  languageBreakdown: LanguageStat[];
  error?: string | null;
}

// ─── Cypher notebooks ────────────────────────────────────────────

export interface NotebookCell {
  id: string;
  kind: "markdown" | "cypher" | string;
  source: string;
  cachedOutput?: unknown;
  lastRunMs?: number;
}

export interface Notebook {
  id: string;
  name: string;
  description?: string;
  tags: string[];
  cells: NotebookCell[];
  updatedAt: number;
}

export interface NotebookSummary {
  id: string;
  name: string;
  cellCount: number;
  updatedAt: number;
}

// ─── HTML interactive export ─────────────────────────────────────

export interface HtmlExportRequest {
  outPath?: string;
  maxNodes?: number;
}

export interface HtmlExportResult {
  path: string;
  nodeCount: number;
  edgeCount: number;
  size: number;
}

// ─── Saved Cypher Queries ────────────────────────────────────────

export interface SavedQuery {
  id: string;
  name: string;
  query: string;
  description?: string;
  tags: string[];
  updatedAt: number;
}

// ─── Saved Graph Views (Theme C) ─────────────────────────────────

export interface CameraState {
  x: number;
  y: number;
  ratio: number;
  angle?: number;
}

export interface SavedView {
  id: string;
  repo?: string;
  name: string;
  lens?: string;
  /** Free-form filter object — front-end owns the schema. */
  filters?: unknown;
  cameraState?: CameraState;
  nodeSelection: string[];
  description?: string;
  createdAt: number;
  updatedAt: number;
}

// ─── Graph diff (Theme C) ────────────────────────────────────────

export interface EdgeKey {
  sourceId: string;
  targetId: string;
  relType: string;
}

export interface ModifiedNodeDiff {
  nodeId: string;
  changedProps: string[];
}

export interface GraphDiff {
  addedNodes: string[];
  removedNodes: string[];
  addedEdges: EdgeKey[];
  removedEdges: EdgeKey[];
  modified: ModifiedNodeDiff[];
}

// ─── Path finding (Theme C) ──────────────────────────────────────

export interface FindPathResult {
  from: string;
  to: string;
  depthUsed: number;
  path: string[];
  found: boolean;
}

export interface ProcessStep {
  nodeId: string;
  name: string;
  label: string;
  filePath: string;
}

export interface ProcessFlow {
  id: string;
  name: string;
  processType: string;
  stepCount: number;
  steps: ProcessStep[];
  mermaid: string;
}

export interface GitHotspot {
  path: string;
  commitCount: number;
  linesAdded: number;
  linesRemoved: number;
  churn: number;
  score: number;
  lastModified: string;
  authorCount: number;
}

export interface GitCoupling {
  fileA: string;
  fileB: string;
  sharedCommits: number;
  couplingStrength: number;
}

export interface GitAuthorContribution {
  name: string;
  email: string;
  commits: number;
  pct: number;
}

export interface GitOwnership {
  path: string;
  primaryAuthor: string;
  ownershipPct: number;
  authorCount: number;
  authors: GitAuthorContribution[];
}

export interface CodeHealth {
  overallScore: number;
  grade: string;
  hotspotScore: number;
  cohesionScore: number;
  tracingCoverage: number;
  ownershipScore: number;
  fileCount: number;
  nodeCount: number;
  edgeCount: number;
  avgComplexity: number;
  maxComplexity: number;
}

export interface FeatureInfo {
  id: string;
  name: string;
  description?: string;
  memberCount: number;
  cohesion?: number;
}

export interface FileQuickPick {
  path: string;
  name: string;
  language?: string;
  symbolCount: number;
}

export interface SymbolQuickPick {
  nodeId: string;
  name: string;
  kind: string;
  filePath: string;
  container?: string;
  startLine?: number;
}

export interface ModuleQuickPick {
  communityId: string;
  name: string;
  memberCount: number;
  description?: string;
}

// ─── Pipeline Progress Types ─────────────────────────────────────────────

export type PipelinePhase =
  | "idle"
  | "extracting"
  | "structure"
  | "parsing"
  | "imports"
  | "calls"
  | "heritage"
  | "communities"
  | "processes"
  | "enriching"
  | "complete"
  | "error";

export interface PipelineStats {
  filesProcessed: number;
  totalFiles: number;
  nodesCreated: number;
}

export interface PipelineProgress {
  phase: PipelinePhase;
  percent: number;
  message: string;
  detail?: string;
  stats?: PipelineStats;
}

/** Human-readable labels for each pipeline phase */
export const PHASE_LABELS: Record<PipelinePhase, string> = {
  idle: "Idle",
  extracting: "Extracting",
  structure: "Scanning files",
  parsing: "Parsing AST",
  imports: "Resolving imports",
  calls: "Analyzing calls",
  heritage: "Class hierarchy",
  communities: "Detecting communities",
  processes: "Tracing processes",
  enriching: "Enriching",
  complete: "Complete",
  error: "Error",
};

/** Overall weight of each phase for global progress bar (must sum to 100) */
export const PHASE_WEIGHTS: Partial<Record<PipelinePhase, number>> = {
  structure: 5,
  parsing: 45,
  imports: 15,
  calls: 15,
  heritage: 5,
  communities: 10,
  processes: 5,
};

// ─── Commands ────────────────────────────────────────────────────────────

export const commands = {
  // Repos
  listRepos: () => invoke<RepoInfo[]>("list_repos"),
  openRepo: (name: string) => invoke<RepoInfo>("open_repo", { name }),
  analyzeRepo: (path: string) => invoke<string>("analyze_repo", { path }),
  unregisterRepo: (path: string) => invoke<void>("unregister_repo", { path }),
  generateDocs: (what: string, path: string) =>
    invoke<string>("generate_docs", { what, path }),

  // Graph
  getGraphData: (filter: GraphFilter) =>
    invoke<GraphPayload>("get_graph_data", { filter }),
  getSubgraph: (centerNodeId: string, depth?: number) =>
    invoke<GraphPayload>("get_subgraph", { centerNodeId, depth }),

  // Search — `rerank` pulls BM25 top-20 through the configured LLM for a
  // precision bump on ambiguous queries. `hybrid` fuses BM25 with semantic
  // embeddings via Reciprocal Rank Fusion (requires prior `code-explorer embed`).
  // Both fall back silently to BM25 order if prerequisites are missing.
  searchSymbols: (
    query: string,
    limit?: number,
    rerank?: boolean,
    hybrid?: boolean,
  ) => invoke<SearchResult[]>("search_symbols", { query, limit, rerank, hybrid }),

  // Context
  getSymbolContext: (nodeId: string) =>
    invoke<SymbolContext>("get_symbol_context", { nodeId }),

  // Impact
  getImpactAnalysis: (
    targetId: string,
    direction?: string,
    maxDepth?: number
  ) =>
    invoke<ImpactResult>("get_impact_analysis", {
      targetId,
      direction,
      maxDepth,
    }),

  // Files
  getFileTree: () => invoke<FileTreeNode[]>("get_file_tree"),
  readFileContent: (
    filePath: string,
    startLine?: number,
    endLine?: number
  ) =>
    invoke<FileContent>("read_file_content", { filePath, startLine, endLine }),

  // Cypher
  executeCypher: (query: string) =>
    invoke<unknown[]>("execute_cypher", { query }),

  // Documentation
  getDocIndex: () =>
    invoke<DocIndex | null>("get_doc_index"),
  readDoc: (path: string) =>
    invoke<DocContent>("read_doc", { path }),

  // Chat Q&A
  chatAsk: (request: ChatRequest) =>
    invoke<ChatResponse>("chat_ask", { request }),
  chatGetConfig: () =>
    invoke<ChatConfig>("chat_get_config"),
  chatSetConfig: (config: ChatConfig) =>
    invoke<void>("chat_set_config", { config }),
  chatTestConnection: (config: ChatConfig) =>
    invoke<ChatConnectionTestResult>("chat_test_connection", { config }),

  // Chat agent tool introspection + retry (Theme B)
  listChatTools: () =>
    invoke<ChatToolDescriptor[]>("list_chat_tools"),
  chatRetryTool: (request: ChatToolRetryRequest) =>
    invoke<ChatToolRetryResult>("chat_retry_tool", { request }),
  chatCancel: () =>
    invoke<void>("chat_cancel"),
  chatSearchCapabilities: () =>
    invoke<ChatSearchCapabilities>("chat_search_capabilities"),
  /** List the 6 MCP prompt templates (`detect_impact`, `generate_map`, …). */
  listChatPrompts: () =>
    invoke<{ prompts: McpPromptDefinition[] }>("list_chat_prompts"),
  /** Render an MCP prompt to plain text — paste into the chat input. */
  getChatPrompt: (name: string, args: Record<string, string>) =>
    invoke<string>("get_chat_prompt", { name, args }),

  // Chat Intelligence (Executor)
  chatExecuteStep: (planId: string, stepId: string) =>
    invoke<StepResult>("chat_execute_step", { planId, stepId }),
  chatExecutePlan: (request: ChatSmartRequest) =>
    invoke<ChatSmartResponse>("chat_execute_plan", { request }),

  // Feature-Dev (3-phase artifact pipeline)
  featureDevRun: (request: FeatureDevRequest) =>
    invoke<FeatureDevArtifact>("feature_dev_run", { request }),

  // Code-Review (pre-commit review)
  codeReviewRun: (request: CodeReviewRequest) =>
    invoke<CodeReviewArtifact>("code_review_run", { request }),

  // Simplify (refactor proposals from graph signals)
  simplifyRun: (request: SimplifyRequest) =>
    invoke<SimplifyArtifact>("simplify_run", { request }),

  // Rename refactor (multi-file)
  renameRun: (request: RenameRequest) =>
    invoke<RenameResult>("rename_run", { request }),

  // Bookmarks (per-repo)
  bookmarksList: () => invoke<Bookmark[]>("bookmarks_list"),
  bookmarksAdd: (bookmark: Bookmark) =>
    invoke<Bookmark[]>("bookmarks_add", { bookmark }),
  bookmarksRemove: (nodeId: string) =>
    invoke<Bookmark[]>("bookmarks_remove", { nodeId }),
  bookmarksClear: () => invoke<void>("bookmarks_clear"),

  // Comments (per-node threads)
  commentsForNode: (nodeId: string) =>
    invoke<Comment[]>("comments_for_node", { nodeId }),
  commentsAdd: (nodeId: string, author: string, body: string) =>
    invoke<Comment[]>("comments_add", { nodeId, author, body }),
  commentsRemove: (nodeId: string, commentId: string) =>
    invoke<Comment[]>("comments_remove", { nodeId, commentId }),

  // Saved Cypher queries (per-repo)
  savedQueriesList: () => invoke<SavedQuery[]>("saved_queries_list"),
  savedQueriesSave: (query: SavedQuery) =>
    invoke<SavedQuery[]>("saved_queries_save", { query }),
  savedQueriesDelete: (id: string) =>
    invoke<SavedQuery[]>("saved_queries_delete", { id }),

  // Interactive HTML export (self-contained graph viewer)
  exportInteractiveHtml: (request: HtmlExportRequest) =>
    invoke<HtmlExportResult>("export_interactive_html", { request }),

  // Wiki generation (one .md per community)
  wikiGenerate: (request: WikiGenerateRequest) =>
    invoke<WikiGenerateResult>("wiki_generate", { request }),

  // Cypher notebooks
  notebookList: () => invoke<NotebookSummary[]>("notebook_list"),
  notebookLoad: (id: string) => invoke<Notebook>("notebook_load", { id }),
  notebookSave: (notebook: Notebook) =>
    invoke<NotebookSummary>("notebook_save", { notebook }),
  notebookDelete: (id: string) => invoke<void>("notebook_delete", { id }),

  // Multi-repo overview
  reposOverview: () => invoke<RepoOverview[]>("repos_overview"),
  repoMetrics: (name: string) =>
    invoke<IndexMetrics | null>("repo_metrics", { name }),

  // Activity history (timeline)
  activityRecord: (note?: string) =>
    invoke<ActivityEntry>("activity_record", { note: note ?? null }),
  activityList: () => invoke<ActivityEntry[]>("activity_list"),
  activityClear: () => invoke<void>("activity_clear"),

  // Snapshot history + diff
  snapshotCreate: (label?: string, commitSha?: string) =>
    invoke<SnapshotMeta>("snapshot_create", {
      label: label ?? null,
      commitSha: commitSha ?? null,
    }),
  snapshotList: () => invoke<SnapshotMeta[]>("snapshot_list"),
  snapshotDelete: (id: string) =>
    invoke<SnapshotMeta[]>("snapshot_delete", { id }),
  snapshotDiff: (request: SnapshotDiffRequest) =>
    invoke<SnapshotDiff>("snapshot_diff", { request }),
  /** Theme C — list recent commits on the active repo's current branch. */
  snapshotListCommits: (limit?: number) =>
    invoke<CommitInfo[]>("snapshot_list_commits", { limit: limit ?? null }),

  // Theme C — graph time-travel & saved views
  diffSnapshots: (from: string, to: string) =>
    invoke<GraphDiff>("diff_snapshots", { from, to }),
  findPath: (
    fromNodeId: string,
    toNodeId: string,
    edgeTypes?: string[],
    maxDepth?: number,
  ) =>
    invoke<FindPathResult>("find_path", {
      fromNodeId,
      toNodeId,
      edgeTypes: edgeTypes ?? null,
      maxDepth: maxDepth ?? null,
    }),
  savedViewsList: () => invoke<SavedView[]>("saved_views_list"),
  savedViewsSave: (view: SavedView) =>
    invoke<SavedView[]>("saved_views_save", { view }),
  savedViewsDelete: (id: string) =>
    invoke<SavedView[]>("saved_views_delete", { id }),

  // Custom dashboards
  dashboardList: () => invoke<DashboardSummary[]>("dashboard_list"),
  dashboardLoad: (id: string) => invoke<Dashboard>("dashboard_load", { id }),
  dashboardSave: (dashboard: Dashboard) =>
    invoke<DashboardSummary>("dashboard_save", { dashboard }),
  dashboardDelete: (id: string) =>
    invoke<void>("dashboard_delete", { id }),

  // Workflow editor
  workflowList: () => invoke<WorkflowSummary[]>("workflow_list"),
  workflowLoad: (id: string) => invoke<Workflow>("workflow_load", { id }),
  workflowSave: (workflow: Workflow) =>
    invoke<WorkflowSummary>("workflow_save", { workflow }),
  workflowDelete: (id: string) => invoke<void>("workflow_delete", { id }),
  workflowRun: (workflow: Workflow) =>
    invoke<WorkflowRunResult>("workflow_run", { workflow }),

  // User-defined slash commands (light plugin system)
  userCommandsList: () => invoke<UserCommand[]>("user_commands_list"),
  userCommandsSave: (command: UserCommand) =>
    invoke<UserCommand[]>("user_commands_save", { command }),
  userCommandsDelete: (id: string) =>
    invoke<UserCommand[]>("user_commands_delete", { id }),
  userCommandResolve: (name: string, args: string) =>
    invoke<ResolvedCommand | null>("user_command_resolve", { name, args }),

  // User data bundle export/import
  userBundleExport: (request: BundleExportRequest) =>
    invoke<BundleExportResult>("user_bundle_export", { request }),
  userBundleImport: (request: BundleImportRequest) =>
    invoke<BundleImportResult>("user_bundle_import", { request }),

  // Quick Picks (IDE-style search)
  chatPickFiles: (query: string, limit?: number) =>
    invoke<FileQuickPick[]>("chat_pick_files", { query, limit }),
  chatPickSymbols: (query: string, fileFilter?: string, limit?: number) =>
    invoke<SymbolQuickPick[]>("chat_pick_symbols", { query, fileFilter, limit }),
  chatPickModules: (query: string, limit?: number) =>
    invoke<ModuleQuickPick[]>("chat_pick_modules", { query, limit }),

  // Export
  exportDocsDocx: () =>
    invoke<string>("export_docs_docx"),
  exportObsidianVault: () =>
    invoke<string>("export_obsidian_vault"),
  getAspnetStats: () =>
    invoke<AspNetStats>("get_aspnet_stats"),

  // Process Flows
  getProcessFlows: () =>
    invoke<ProcessFlow[]>("get_process_flows"),

  // Code Health
  getCodeHealth: () =>
    invoke<CodeHealth>("get_code_health"),

  // Git Analytics
  getHotspots: (sinceDays?: number) =>
    invoke<GitHotspot[]>("get_hotspots", { sinceDays }),
  getCoupling: (minShared?: number) =>
    invoke<GitCoupling[]>("get_coupling", { minShared }),
  getOwnership: () =>
    invoke<GitOwnership[]>("get_ownership"),

  // Features (community-based)
  getFeatures: () =>
    invoke<FeatureInfo[]>("get_features"),

  // Coverage & Diagrams
  getCoverageStats: () =>
    invoke<CoverageStats>("get_coverage_stats"),
  getDiagram: (target: string, diagramType?: string) =>
    invoke<DiagramResult>("get_diagram", { target, diagramType }),

  // Code Quality Suite (Theme A)
  detectCycles: (scope?: "imports" | "calls") =>
    invoke<Cycle[]>("detect_cycles", { scope }),
  findClones: (minTokens?: number, threshold?: number, limit?: number) =>
    invoke<CloneCluster[]>("find_clones_cmd", { minTokens, threshold, limit }),
  listTodos: (severity?: string, limit?: number) =>
    invoke<TodoEntry[]>("list_todos_cmd", { severity, limit }),
  getComplexityReport: (threshold?: number, limit?: number) =>
    invoke<ComplexityReport>("get_complexity_report", { threshold, limit }),

  // Schema & API Inventory (Theme D)
  listEndpoints: (method?: string, pattern?: string) =>
    invoke<ApiEndpointSummary[]>("list_endpoints", { method, pattern }),
  listDbTables: () =>
    invoke<DbTableSummary[]>("list_db_tables"),
  listEnvVars: (unusedOnly?: boolean) =>
    invoke<EnvVarSummary[]>("list_env_vars", { unusedOnly }),
  getEndpointHandler: (route: string, method: string) =>
    invoke<EndpointHandlerDetails>("get_endpoint_handler", { route, method }),
};

// ─── Coverage ────────────────────────────────────────────────────────

export interface CoverageStats {
  totalMethods: number;
  tracedMethods: number;
  deadCodeCandidates: number;
  coveragePct: number;
  deadMethods: DeadMethod[];
}

export interface DeadMethod {
  name: string;
  filePath: string;
  className: string | null;
  nodeId: string;
}

// ─── Diagram ─────────────────────────────────────────────────────────

export interface DiagramResult {
  mermaid: string;
  targetName: string;
  targetLabel: string;
  diagramType: string;
}

// ─── ASP.NET Stats ────────────────────────────────────────────────────

export interface AspNetStats {
  controllers: number;
  actions: number;
  apiEndpoints: number;
  views: number;
  entities: number;
  dbContexts: number;
  areas: number;
}

// ─── Code Quality Suite (Theme A) ─────────────────────────────────────

export interface Cycle {
  nodes: string[];
  names: string[];
  filePaths: string[];
  length: number;
  severity: string;
}

export interface CloneMember {
  nodeId: string;
  name: string;
  filePath: string;
  startLine?: number;
  endLine?: number;
  tokenCount: number;
  snippet: string;
}

export interface CloneCluster {
  clusterId: string;
  members: CloneMember[];
  similarity: number;
  minTokens: number;
}

export interface TodoEntry {
  nodeId: string;
  kind: string;
  text?: string;
  filePath: string;
  line?: number;
  language?: string;
}

export interface ComplexSymbolReport {
  nodeId: string;
  name: string;
  filePath: string;
  label: string;
  complexity: number;
  startLine?: number;
  endLine?: number;
  severity: string;
}

export interface ComplexitySeverityCounts {
  low: number;
  medium: number;
  high: number;
  critical: number;
}

export interface ModuleComplexity {
  module: string;
  symbolCount: number;
  avgComplexity: number;
  maxComplexity: number;
}

export interface ComplexityReport {
  totalSymbols: number;
  measuredSymbols: number;
  avgComplexity: number;
  maxComplexity: number;
  p50: number;
  p90: number;
  p99: number;
  topSymbols: ComplexSymbolReport[];
  severityCounts: ComplexitySeverityCounts;
  byModule: ModuleComplexity[];
}

// ─── Schema & API Inventory (Theme D) ─────────────────────────────────

export interface ApiEndpointSummary {
  nodeId: string;
  httpMethod: string;
  route: string;
  framework?: string;
  filePath: string;
  startLine?: number;
  handlerId?: string;
  handlerName?: string;
}

export interface DbColumnSummary {
  nodeId: string;
  name: string;
  columnType?: string;
  isPrimaryKey: boolean;
  isNullable: boolean;
}

export interface DbTableSummary {
  nodeId: string;
  name: string;
  filePath: string;
  columnCount: number;
  fkCount: number;
  columns: DbColumnSummary[];
}

export interface EnvVarSummary {
  nodeId: string;
  name: string;
  declaredIn?: string;
  usedInCount: number;
  unused: boolean;
  undeclared: boolean;
}

export interface HandlerNeighbor {
  nodeId: string;
  name: string;
  label: string;
  relType: string;
}

export interface HandlerInfo {
  nodeId: string;
  name: string;
  label: string;
  filePath: string;
  startLine?: number;
  endLine?: number;
}

export interface EndpointHandlerDetails {
  endpoint: ApiEndpointSummary;
  handler?: HandlerInfo | null;
  callers: HandlerNeighbor[];
  callees: HandlerNeighbor[];
}

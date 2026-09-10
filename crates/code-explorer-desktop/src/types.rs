//! Response types for Tauri IPC commands.
//!
//! These types are Cytoscape.js-native and designed for direct consumption
//! by the React frontend. They avoid the MCP JSON envelope format.

use serde::{Deserialize, Serialize};

// ─── Graph Payload (Cytoscape-native) ────────────────────────────────────

/// A graph payload ready for Cytoscape.js rendering.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphPayload {
    pub nodes: Vec<CytoNode>,
    pub edges: Vec<CytoEdge>,
    pub stats: GraphStats,
}

/// A node in Cytoscape format.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CytoNode {
    pub id: String,
    pub label: String,
    pub name: String,
    pub file_path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_line: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_line: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_exported: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub community: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parameter_count: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub return_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub layer_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entry_point_score: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entry_point_reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_traced: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trace_call_count: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_dead_candidate: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub complexity: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub depth: Option<u32>,
}

/// An edge in Cytoscape format.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CytoEdge {
    pub id: String,
    pub source: String,
    pub target: String,
    pub rel_type: String,
    pub confidence: f64,
}

/// Graph statistics.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphStats {
    pub node_count: usize,
    pub edge_count: usize,
    pub truncated: bool,
}

// ─── Graph Filters ───────────────────────────────────────────────────────

/// Zoom level for progressive disclosure.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ZoomLevel {
    /// Show Folder/Package nodes with CONTAINS edges
    Package,
    /// Show File/Module nodes with IMPORTS edges
    Module,
    /// Show Function/Class/Method nodes with CALLS/USES edges
    Symbol,
}

/// Filter for graph data requests.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphFilter {
    pub zoom_level: ZoomLevel,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub labels: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_paths: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_nodes: Option<usize>,
}

impl Default for GraphFilter {
    fn default() -> Self {
        Self {
            zoom_level: ZoomLevel::Package,
            labels: None,
            file_paths: None,
            max_nodes: Some(500),
        }
    }
}

// ─── Repository Info ─────────────────────────────────────────────────────

/// Repository info for the frontend.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RepoInfo {
    pub name: String,
    pub path: String,
    pub indexed_at: String,
    pub last_commit: String,
    pub files: Option<usize>,
    pub nodes: Option<usize>,
    pub edges: Option<usize>,
    pub communities: Option<usize>,
    pub index_duration_ms: Option<u64>,
}

// ─── File Tree ───────────────────────────────────────────────────────────

/// A file tree node.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileTreeNode {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
    pub children: Vec<FileTreeNode>,
}

// ─── Search Results ──────────────────────────────────────────────────────

/// A search result.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchResult {
    pub node_id: String,
    pub name: String,
    pub label: String,
    pub file_path: String,
    pub score: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_line: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_line: Option<u32>,
}

// ─── Symbol Context ──────────────────────────────────────────────────────

/// 360-degree context for a symbol.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SymbolContext {
    pub node: CytoNode,
    pub callers: Vec<RelatedNode>,
    pub callees: Vec<RelatedNode>,
    pub imports: Vec<RelatedNode>,
    pub imported_by: Vec<RelatedNode>,
    pub inherits: Vec<RelatedNode>,
    pub inherited_by: Vec<RelatedNode>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub community: Option<CommunityInfo>,
}

/// A related node in context view.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RelatedNode {
    pub id: String,
    pub name: String,
    pub label: String,
    pub file_path: String,
}

/// Community info.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommunityInfo {
    pub id: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub member_count: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cohesion: Option<f64>,
}

// ─── Impact Analysis ─────────────────────────────────────────────────────

/// Blast radius analysis result.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImpactResult {
    pub target: CytoNode,
    pub upstream: Vec<ImpactNode>,
    pub downstream: Vec<ImpactNode>,
    pub graph: GraphPayload,
    pub affected_files: Vec<String>,
    pub summary: ImpactSummary,
}

/// A node in the impact graph with depth info.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImpactNode {
    pub node: CytoNode,
    pub depth: u32,
    pub path: Vec<String>,
}

/// Summary statistics for impact analysis.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImpactSummary {
    pub upstream_count: usize,
    pub downstream_count: usize,
    pub affected_files_count: usize,
    pub max_depth: u32,
}

// ─── File Content ────────────────────────────────────────────────────────

/// File content with metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileContent {
    pub path: String,
    pub content: String,
    pub language: Option<String>,
    pub total_lines: usize,
}

// ─── Documentation ──────────────────────────────────────────────────────────

/// A documentation page in the navigation tree.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DocPage {
    pub id: String,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub children: Option<Vec<DocPage>>,
}

/// Documentation index with navigation tree and metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DocIndex {
    pub title: String,
    pub generated_at: String,
    pub stats: DocStats,
    pub pages: Vec<DocPage>,
}

/// Documentation statistics.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DocStats {
    pub files: usize,
    pub nodes: usize,
    pub edges: usize,
    pub modules: usize,
}

/// Content of a documentation page.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DocContent {
    pub path: String,
    pub content: String,
    pub title: String,
}

// ─── Chat Q&A ───────────────────────────────────────────────────────────

/// Chat message (user or assistant).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

/// Chat request from the frontend.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatRequest {
    pub question: String,
    #[serde(default)]
    pub history: Vec<ChatMessage>,
}

/// Chat response returned to the frontend.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatResponse {
    pub answer: String,
    pub sources: Vec<ChatSource>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
}

/// A source citation in a chat response.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatSource {
    pub node_id: String,
    pub symbol_name: String,
    pub symbol_type: String,
    pub file_path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_line: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_line: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub snippet: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub callers: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub callees: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub community: Option<String>,
    pub relevance_score: f64,
}

/// LLM chat configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatConfig {
    pub provider: String,
    pub api_key: String,
    pub base_url: String,
    pub model: String,
    pub max_tokens: u32,
    /// Reasoning effort level for models that support thinking (e.g. Gemini).
    /// Values: "none", "low", "medium", "high". Empty or "none" means disabled.
    #[serde(default)]
    pub reasoning_effort: String,
}

impl Default for ChatConfig {
    fn default() -> Self {
        Self {
            provider: "ollama".to_string(),
            api_key: String::new(),
            base_url: "http://localhost:11434/v1".to_string(),
            model: "llama3.2".to_string(),
            max_tokens: 4096,
            reasoning_effort: String::new(),
        }
    }
}

/// Reports whether the active repo's chat path is running BM25-only or hybrid
/// BM25+semantic search. Surfaced in the UI so users can run `code-explorer embed`
/// when semantic ranking would help — without that hint, a degraded chat is
/// indistinguishable from a fast one.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatSearchCapabilities {
    /// True when `.codeexplorer/embeddings.bin` was found and loaded for the
    /// active repo. False means the chat falls back to BM25 + exact name.
    pub embeddings_loaded: bool,
    /// Embedding model name as recorded in `embeddings.meta.json`. Useful in
    /// the UI to distinguish "loaded from MiniLM" vs "loaded from BGE-M3".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_name: Option<String>,
    /// Number of indexed vectors. Helps spot stale embeddings vs a freshly
    /// re-indexed repo.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vector_count: Option<usize>,
}

// ─── Chat Intelligence (Planner & Executor) ─────────────────────────

/// Query complexity classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum QueryComplexity {
    /// Direct lookup — single search or symbol resolution
    Simple,
    /// 2-3 operations — search + context or search + impact
    Medium,
    /// Multi-step research — DAG of operations
    Complex,
}

/// Filters for scoping chat context to specific files/symbols/modules.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatContextFilter {
    /// Filter to specific file paths or glob patterns
    #[serde(default)]
    pub files: Vec<String>,
    /// Filter to specific symbol names
    #[serde(default)]
    pub symbols: Vec<String>,
    /// Filter to specific module/community names
    #[serde(default)]
    pub modules: Vec<String>,
    /// Filter to specific languages
    #[serde(default)]
    pub languages: Vec<String>,
    /// Filter to specific node labels (Function, Class, etc.)
    #[serde(default)]
    pub labels: Vec<String>,
}

/// Enhanced chat request with optional context filters.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatSmartRequest {
    pub question: String,
    #[serde(default)]
    pub history: Vec<ChatMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filters: Option<ChatContextFilter>,
    /// If true, execute a full research plan (complex mode)
    #[serde(default)]
    pub deep_research: bool,
}

/// Result of analyzing a query's complexity and required tools.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QueryAnalysis {
    pub complexity: QueryComplexity,
    pub suggested_tools: Vec<String>,
    pub estimated_steps: u32,
    pub reasoning: String,
    /// Keywords extracted from the query
    pub keywords: Vec<String>,
    /// Whether the query needs cross-file analysis
    pub needs_cross_file: bool,
    /// Whether the query needs impact/dependency analysis
    pub needs_impact: bool,
}

/// A research plan — a DAG of steps to answer a complex question.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResearchPlan {
    pub id: String,
    pub query: String,
    pub analysis: QueryAnalysis,
    pub steps: Vec<ResearchStep>,
    pub status: PlanStatus,
}

/// Status of the overall research plan.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PlanStatus {
    Pending,
    Running,
    Completed,
    Failed,
}

/// A single step in a research plan.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResearchStep {
    pub id: String,
    pub order: u32,
    /// Tool to use: search_symbols, get_symbol_context, get_impact_analysis, execute_cypher, read_file_content
    pub tool: String,
    /// Description of what this step does
    pub description: String,
    /// Parameters for the tool
    pub params: serde_json::Value,
    /// IDs of steps this depends on
    #[serde(default)]
    pub depends_on: Vec<String>,
    pub status: StepStatus,
    /// Result of executing this step
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<StepResult>,
}

/// Status of an individual research step.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum StepStatus {
    Pending,
    Running,
    Completed,
    Failed,
    Skipped,
}

/// Result of executing a research step.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StepResult {
    /// Summary of what was found
    pub summary: String,
    /// Sources discovered in this step
    #[serde(default)]
    pub sources: Vec<ChatSource>,
    /// Raw data (tool-specific JSON)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
    /// Duration in milliseconds
    pub duration_ms: u64,
}

/// Enhanced chat response with optional research plan.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatSmartResponse {
    pub answer: String,
    pub sources: Vec<ChatSource>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plan: Option<ResearchPlan>,
    pub complexity: QueryComplexity,
}

/// File quick-pick result (for Ctrl+P style file picker).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileQuickPick {
    pub path: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    pub symbol_count: u32,
}

/// Symbol quick-pick result (for Ctrl+Shift+O style symbol picker).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SymbolQuickPick {
    pub node_id: String,
    pub name: String,
    pub kind: String,
    pub file_path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub container: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_line: Option<u32>,
}

/// A detected feature/community in the knowledge graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FeatureInfo {
    pub id: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub member_count: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cohesion: Option<f64>,
}

/// Module quick-pick result.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModuleQuickPick {
    pub community_id: String,
    pub name: String,
    pub member_count: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

// ─── Feature-Dev Artifact ────────────────────────────────────────────────
//
// Mirrors the Claude `feature-dev` skill: a three-phase pipeline
// (explorer → architect → reviewer) that produces a structured artifact
// instead of a conversational answer. Each phase streams its section as
// soon as it's done so the UI can render incrementally.

/// One of the three phases of feature-dev.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FeatureDevPhase {
    Explorer,
    Architect,
    Reviewer,
}

/// Status of a phase (emitted to the frontend).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PhaseStatus {
    Pending,
    Running,
    Completed,
    Failed,
}

/// A single section of the feature-dev artifact, produced by one phase.
///
/// `kind` discriminates the shape: "surface_analysis" (explorer output),
/// "blueprint" (architect output), "review" (reviewer output). The
/// `markdown` field holds the pre-rendered Markdown the phase produced;
/// the structured fields are optional hints the UI can render in a
/// richer way (tables, chips, etc.).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FeatureDevSection {
    pub phase: FeatureDevPhase,
    pub title: String,
    /// Pre-rendered Markdown content for the section body.
    pub markdown: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub surface: Option<SurfaceAnalysis>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blueprint: Option<Blueprint>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub review: Option<Review>,
    /// Milliseconds spent on this phase (wall clock).
    pub duration_ms: u64,
}

/// Explorer-phase output: what this part of the codebase currently looks like.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SurfaceAnalysis {
    /// Modules / communities likely affected by the feature.
    pub modules: Vec<String>,
    /// Entry points that already handle similar concerns.
    pub entry_points: Vec<String>,
    /// Architecture layers detected in the affected area
    /// (e.g. `["Controller", "Service", "Repository"]`).
    pub layers: Vec<String>,
    /// File paths that are load-bearing for the affected area.
    pub key_files: Vec<String>,
}

/// Architect-phase output: the implementation blueprint.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Blueprint {
    pub files_to_create: Vec<FilePlan>,
    pub files_to_modify: Vec<FilePlan>,
    /// Ordered list of build steps (high-level plan).
    pub build_sequence: Vec<String>,
    /// Data-flow description (free text).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data_flow: Option<String>,
}

/// A planned change to a file.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FilePlan {
    pub path: String,
    pub purpose: String,
}

/// Reviewer-phase output: high-confidence issues surfaced before any code is written.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Review {
    pub issues: Vec<ReviewIssue>,
    /// Predicted blast radius of the blueprint against the current graph.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub predicted_impact: Option<String>,
    /// Overall readiness: "ready", "needs_revisions", "blocked".
    pub verdict: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewIssue {
    pub severity: String, // "high" | "medium" | "low"
    pub confidence: f64,
    pub title: String,
    pub detail: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
}

/// The complete artifact produced by a feature-dev run.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FeatureDevArtifact {
    pub id: String,
    pub feature_description: String,
    pub sections: Vec<FeatureDevSection>,
    pub status: PlanStatus,
    /// Optional final summary generated after the 3 phases complete.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
}

/// Request body for the feature_dev Tauri command.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FeatureDevRequest {
    pub feature_description: String,
    #[serde(default)]
    pub filters: Option<ChatContextFilter>,
    /// If true, returns a dry artifact with only the explorer phase
    /// (faster, good for previewing the surface before committing to a
    /// full architect+reviewer pass).
    #[serde(default)]
    pub explorer_only: bool,
}

/// Event payload for `feature-dev-phase` frontend events.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FeatureDevPhaseEvent {
    pub artifact_id: String,
    pub phase: FeatureDevPhase,
    pub status: PhaseStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

/// Event payload for `feature-dev-section` frontend events: one section
/// (produced by a completed phase) streamed to the UI.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FeatureDevSectionEvent {
    pub artifact_id: String,
    pub section: FeatureDevSection,
}

// ─── Code-Review Artifact ────────────────────────────────────────────────
//
// Absorbs Claude's `code-review` skill. Takes the current git diff (or an
// explicit symbol list), pre-computes objective signals from the graph
// (impact blast radius, hotspots intersect, coverage gaps, ownership
// fragmentation), then asks the LLM to produce a focused review with
// confidence-filtered issues.

/// Request body for the `code_review_run` Tauri command.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct CodeReviewRequest {
    /// Explicit list of changed symbol names/ids to review. If omitted,
    /// the command derives the scope from `git diff HEAD`.
    #[serde(default)]
    pub target_symbols: Vec<String>,
    /// Confidence floor for issue inclusion (default 0.8).
    #[serde(default)]
    pub min_confidence: Option<f64>,
    /// Whether to include low/medium severity issues (default: false).
    #[serde(default)]
    pub include_all_severities: bool,
}

/// The complete artifact produced by a code_review run.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CodeReviewArtifact {
    pub id: String,
    pub scope_summary: String,
    pub status: PlanStatus,
    pub signals: CodeReviewSignals,
    pub review: Review,
    /// Combined Markdown document suitable for export.
    pub markdown: String,
    pub duration_ms: u64,
}

/// Objective graph-derived signals fed to the reviewer LLM.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CodeReviewSignals {
    pub changed_files: Vec<String>,
    pub changed_symbols: Vec<String>,
    pub affected_count: u32,
    pub affected_processes: Vec<String>,
    /// Files in the change set that are also in the hotspots top-N.
    pub hotspot_files: Vec<String>,
    /// Symbols in the change set with no tracing coverage.
    pub untraced_symbols: Vec<String>,
    /// Symbols in the change set already flagged as dead candidates.
    pub dead_candidates: Vec<String>,
    pub risk_level: String,
}

// ─── Simplify Artifact ───────────────────────────────────────────────────
//
// Absorbs Claude's `simplify` skill: examine a target (file or module),
// surface dead code, code smells, complexity hotspots, and duplication
// candidates, then propose concrete refactor moves with rationale.

/// Request body for `simplify_run`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SimplifyRequest {
    /// File path, module/community name, or symbol to focus on.
    /// If omitted, picks the most-complex file in the active repo.
    #[serde(default)]
    pub target: Option<String>,
    /// Minimum complexity to consider a symbol simplifiable (default: 8).
    #[serde(default)]
    pub min_complexity: Option<u32>,
}

/// Aggregated simplify signals (deterministic, graph-derived).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SimplifySignals {
    pub scope: String,
    pub complex_symbols: Vec<ComplexSymbol>,
    pub dead_candidates: Vec<String>,
    pub untraced_symbols: Vec<String>,
    pub llm_smells: Vec<String>,
    /// Symbol-name groups with the same name (potential duplication).
    pub duplicate_groups: Vec<DuplicateGroup>,
    pub total_files: u32,
    pub total_symbols: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ComplexSymbol {
    pub name: String,
    pub file_path: String,
    pub complexity: u32,
    pub label: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DuplicateGroup {
    pub name: String,
    pub occurrences: u32,
    pub files: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SimplifyArtifact {
    pub id: String,
    pub status: PlanStatus,
    pub signals: SimplifySignals,
    /// LLM- (or graph-) generated refactoring proposals.
    pub proposals: Vec<SimplifyProposal>,
    pub markdown: String,
    pub duration_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SimplifyProposal {
    pub kind: String, // "extract" | "delete" | "merge" | "inline" | "rename"
    pub target: String,
    pub rationale: String,
    pub confidence: f64,
}

// ─── Rename Refactor ─────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RenameRequest {
    pub target: String,
    pub new_name: String,
    /// When true (default), no files are touched — only the patch list is returned.
    #[serde(default)]
    pub dry_run: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RenameEdit {
    pub file: String,
    pub line: u32,
    pub col: u32,
    pub old_text: String,
    pub new_text: String,
    pub snippet: String,
    pub confidence: f64,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RenameResult {
    pub target: String,
    pub new_name: String,
    pub dry_run: bool,
    pub files_affected: u32,
    pub graph_edits: Vec<RenameEdit>,
    pub text_search_edits: Vec<RenameEdit>,
    /// Only populated when dry_run = false — per-file applied edit counts.
    #[serde(default)]
    pub applied: Option<serde_json::Value>,
}

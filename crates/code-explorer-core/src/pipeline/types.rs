use serde::{Deserialize, Serialize};

/// Pipeline execution phase.
/// Matches the TypeScript `PipelinePhase` type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PipelinePhase {
    Idle,
    Extracting,
    Structure,
    Parsing,
    Imports,
    Calls,
    Heritage,
    Communities,
    Processes,
    /// ASP.NET MVC 5 / EF6 enrichment (controllers, actions, entities, views, .edmx)
    AspNetMvc,
    Enriching,
    Complete,
    Error,
}

impl PipelinePhase {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Extracting => "extracting",
            Self::Structure => "structure",
            Self::Parsing => "parsing",
            Self::Imports => "imports",
            Self::Calls => "calls",
            Self::Heritage => "heritage",
            Self::Communities => "communities",
            Self::Processes => "processes",
            Self::AspNetMvc => "aspnet_mvc",
            Self::Enriching => "enriching",
            Self::Complete => "complete",
            Self::Error => "error",
        }
    }
}

impl std::fmt::Display for PipelinePhase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Progress report from the ingestion pipeline.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineProgress {
    pub phase: PipelinePhase,
    /// Completion percentage (0-100)
    pub percent: f64,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stats: Option<PipelineStats>,
}

/// Statistics reported during pipeline execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PipelineStats {
    pub files_processed: usize,
    pub total_files: usize,
    pub nodes_created: usize,
}

/// Wall-clock duration of a single ingestion phase.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PhaseTiming {
    /// Phase identifier (matches the `phase=` label used in tracing, e.g. "parsing").
    pub name: String,
    pub duration_ms: u64,
}

/// Detailed performance metrics for one indexing run, persisted to
/// `.codeexplorer/metrics.json`. The headline `total_duration_ms` is also mirrored
/// into `RepoStats::index_duration_ms` so summary views need not read this file.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexMetrics {
    /// Schema version of this metrics file; lets future readers migrate.
    pub schema_version: u32,
    /// Code Explorer version that produced the index (`CARGO_PKG_VERSION`).
    pub tool_version: String,
    /// RFC3339 timestamp of the index run.
    pub indexed_at: String,
    pub total_duration_ms: u64,
    /// Per-phase wall-clock breakdown. Empty for incremental runs that skip the full pipeline.
    #[serde(default)]
    pub phases: Vec<PhaseTiming>,
    pub files: usize,
    pub nodes: usize,
    pub edges: usize,
    pub communities: usize,
    pub processes: usize,
    pub files_per_sec: f64,
    pub nodes_per_sec: f64,
}

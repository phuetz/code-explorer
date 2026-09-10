//! Multi-repo overview — aggregate stats across every indexed repo.
//!
//! Reads the global registry, opens each snapshot lazily, and returns a
//! summary for each. Used by the Manage-mode multi-repo dashboard.
//!
//! For each repo we surface signals that matter for picking what to work on
//! next: total nodes/edges, file count, language mix, dead-code count,
//! tracing coverage, and the timestamp of the last index.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use tauri::State;

use code_explorer_core::graph::types::*;
use code_explorer_core::storage::repo_manager;

use crate::state::AppState;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RepoOverview {
    pub name: String,
    pub path: String,
    pub indexed_at: String,
    pub last_commit: String,
    pub node_count: u32,
    pub edge_count: u32,
    pub file_count: u32,
    pub function_count: u32,
    pub class_count: u32,
    pub community_count: u32,
    pub dead_count: u32,
    pub traced_count: u32,
    /// Tracing coverage 0..1 (traced / total instrumentable).
    pub tracing_coverage: f32,
    /// Total wall-clock time of the last index, in milliseconds (from the registry stats).
    pub index_duration_ms: Option<u64>,
    pub language_breakdown: Vec<LanguageStat>,
    /// True when graph.bin is missing or unreadable.
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LanguageStat {
    pub language: String,
    pub file_count: u32,
}

#[tauri::command]
pub async fn repos_overview(_state: State<'_, AppState>) -> Result<Vec<RepoOverview>, String> {
    let registry = repo_manager::read_registry().map_err(|e| e.to_string())?;
    let mut out: Vec<RepoOverview> = Vec::new();

    for entry in registry {
        let index_duration_ms = entry.stats.as_ref().and_then(|s| s.index_duration_ms);
        let storage = entry
            .storage_path
            .strip_prefix(r"\\?\")
            .unwrap_or(&entry.storage_path);
        let snap = code_explorer_db::snapshot::snapshot_path(std::path::Path::new(storage));
        match code_explorer_db::snapshot::load_snapshot(&snap) {
            Ok(graph) => {
                let mut function_count = 0u32;
                let mut class_count = 0u32;
                let mut file_count = 0u32;
                let mut community_count = 0u32;
                let mut dead_count = 0u32;
                let mut traced_count = 0u32;
                let mut traceable = 0u32;
                let mut langs: HashMap<String, u32> = HashMap::new();

                for n in graph.iter_nodes() {
                    match n.label {
                        NodeLabel::File => {
                            file_count += 1;
                            if let Some(ext) = std::path::Path::new(&n.properties.file_path)
                                .extension()
                                .and_then(|e| e.to_str())
                            {
                                let lang = match ext {
                                    "ts" | "tsx" => "TypeScript",
                                    "js" | "jsx" => "JavaScript",
                                    "py" => "Python",
                                    "java" => "Java",
                                    "kt" => "Kotlin",
                                    "rs" => "Rust",
                                    "go" => "Go",
                                    "cs" => "C#",
                                    "cpp" | "cc" | "cxx" | "hpp" | "h" => "C++",
                                    "c" => "C",
                                    "php" => "PHP",
                                    "rb" => "Ruby",
                                    "swift" => "Swift",
                                    "cshtml" | "razor" => "Razor",
                                    other => {
                                        // Skip files we don't track in language stats.
                                        let _ = other;
                                        continue;
                                    }
                                };
                                *langs.entry(lang.to_string()).or_insert(0) += 1;
                            }
                        }
                        NodeLabel::Function | NodeLabel::Method | NodeLabel::Constructor => {
                            function_count += 1;
                            traceable += 1;
                            if n.properties.is_traced.unwrap_or(false) {
                                traced_count += 1;
                            }
                            if n.properties.is_dead_candidate.unwrap_or(false) {
                                dead_count += 1;
                            }
                        }
                        NodeLabel::Class | NodeLabel::Interface | NodeLabel::Struct => {
                            class_count += 1;
                        }
                        NodeLabel::Community => {
                            community_count += 1;
                        }
                        _ => {}
                    }
                }

                let mut language_breakdown: Vec<LanguageStat> = langs
                    .into_iter()
                    .map(|(language, file_count)| LanguageStat {
                        language,
                        file_count,
                    })
                    .collect();
                language_breakdown.sort_by(|a, b| b.file_count.cmp(&a.file_count));

                out.push(RepoOverview {
                    name: entry.name,
                    path: entry.path,
                    indexed_at: entry.indexed_at,
                    last_commit: entry.last_commit,
                    node_count: graph.iter_nodes().count() as u32,
                    edge_count: graph.iter_relationships().count() as u32,
                    file_count,
                    function_count,
                    class_count,
                    community_count,
                    dead_count,
                    traced_count,
                    tracing_coverage: if traceable > 0 {
                        traced_count as f32 / traceable as f32
                    } else {
                        0.0
                    },
                    index_duration_ms,
                    language_breakdown,
                    error: None,
                });
            }
            Err(e) => out.push(RepoOverview {
                name: entry.name,
                path: entry.path,
                indexed_at: entry.indexed_at,
                last_commit: entry.last_commit,
                node_count: 0,
                edge_count: 0,
                file_count: 0,
                function_count: 0,
                class_count: 0,
                community_count: 0,
                dead_count: 0,
                traced_count: 0,
                tracing_coverage: 0.0,
                index_duration_ms,
                language_breakdown: Vec::new(),
                error: Some(format!("Snapshot unreadable: {e}")),
            }),
        }
    }

    out.sort_by(|a, b| b.node_count.cmp(&a.node_count));
    Ok(out)
}

/// Detailed indexing metrics (per-phase breakdown + throughput) for one repo,
/// read from its `.codeexplorer/metrics.json`. Returns `None` if the repo has
/// not been indexed with a metrics-aware build yet.
#[tauri::command]
pub async fn repo_metrics(
    _state: State<'_, AppState>,
    name: String,
) -> Result<Option<code_explorer_core::pipeline::types::IndexMetrics>, String> {
    let registry = repo_manager::read_registry().map_err(|e| e.to_string())?;
    let entry = match registry.into_iter().find(|e| e.name == name) {
        Some(e) => e,
        None => return Ok(None),
    };
    let storage = entry
        .storage_path
        .strip_prefix(r"\\?\")
        .unwrap_or(&entry.storage_path);
    repo_manager::load_metrics(std::path::Path::new(storage)).map_err(|e| e.to_string())
}

//! Code Quality Suite commands (Theme A).
//!
//! Surfaces the four `code-explorer-db::analytics` capabilities through Tauri:
//! - `detect_cycles`      — circular dependencies via Tarjan SCC
//! - `find_clones`        — Rabin-Karp based duplicate detection
//! - `list_todos`         — TODO/FIXME inventory
//! - `get_complexity`     — cyclomatic complexity report

use std::path::PathBuf;

use serde::Serialize;
use tauri::State;

use code_explorer_core::graph::types::NodeLabel;
use code_explorer_db::analytics::clones::{find_clones, CloneCluster, CloneOptions};
use code_explorer_db::analytics::complexity::{get_complexity, ComplexityOptions, ComplexityReport};
use code_explorer_db::analytics::cycles::{find_cycles, Cycle, CycleScope};

use crate::state::AppState;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TodoEntry {
    pub node_id: String,
    pub kind: String,
    pub text: Option<String>,
    pub file_path: String,
    pub line: Option<u32>,
    pub language: Option<String>,
}

// ─── Commands ────────────────────────────────────────────────────────────

#[tauri::command]
pub async fn detect_cycles(
    state: State<'_, AppState>,
    scope: Option<String>,
) -> Result<Vec<Cycle>, String> {
    let (graph, _, _, _) = state.get_repo(None).await?;
    let scope_str = scope.unwrap_or_else(|| "imports".to_string());
    let scope = CycleScope::parse(&scope_str)
        .ok_or_else(|| format!("Invalid scope '{scope_str}' (expected 'imports' or 'calls')"))?;
    Ok(find_cycles(&graph, scope))
}

#[tauri::command]
pub async fn find_clones_cmd(
    state: State<'_, AppState>,
    min_tokens: Option<u32>,
    threshold: Option<f64>,
    limit: Option<u32>,
) -> Result<Vec<CloneCluster>, String> {
    let (graph, _, _, repo_path) = state.get_repo(None).await?;
    let opts = CloneOptions {
        min_tokens: min_tokens.unwrap_or(30).max(5) as usize,
        threshold: threshold.unwrap_or(0.9).clamp(0.0, 1.0),
        max_clusters: limit.unwrap_or(100).min(500) as usize,
    };
    let repo_path = PathBuf::from(repo_path);
    Ok(find_clones(&graph, &repo_path, opts))
}

#[tauri::command]
pub async fn list_todos_cmd(
    state: State<'_, AppState>,
    severity: Option<String>,
    limit: Option<u32>,
) -> Result<Vec<TodoEntry>, String> {
    let (graph, _, _, _) = state.get_repo(None).await?;

    let want = severity.map(|s| s.to_ascii_uppercase());
    let mut out: Vec<TodoEntry> = Vec::new();
    for node in graph.iter_nodes() {
        if node.label != NodeLabel::TodoMarker {
            continue;
        }
        let kind = node.properties.todo_kind.clone().unwrap_or_default();
        if let Some(ref w) = want {
            if !kind.eq_ignore_ascii_case(w) {
                continue;
            }
        }
        out.push(TodoEntry {
            node_id: node.id.clone(),
            kind,
            text: node.properties.todo_text.clone(),
            file_path: node.properties.file_path.clone(),
            line: node.properties.start_line,
            language: node.properties.language.map(|l| l.as_str().to_string()),
        });
    }

    // Sort: FIXME > HACK > TODO > XXX, then by file path.
    out.sort_by(|a, b| {
        rank(&a.kind)
            .cmp(&rank(&b.kind))
            .then_with(|| a.file_path.cmp(&b.file_path))
    });

    if let Some(cap) = limit {
        out.truncate(cap as usize);
    }
    Ok(out)
}

#[tauri::command]
pub async fn get_complexity_report(
    state: State<'_, AppState>,
    threshold: Option<u32>,
    limit: Option<u32>,
) -> Result<ComplexityReport, String> {
    let (graph, _, _, _) = state.get_repo(None).await?;
    let opts = ComplexityOptions {
        threshold: threshold.unwrap_or(0),
        top_n: limit.unwrap_or(50).min(500) as usize,
    };
    Ok(get_complexity(&graph, opts))
}

fn rank(kind: &str) -> u8 {
    match kind {
        "FIXME" => 0,
        "HACK" => 1,
        "TODO" => 2,
        "XXX" => 3,
        _ => 4,
    }
}

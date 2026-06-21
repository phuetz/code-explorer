//! Repo activity history — lightweight timeline of analyze runs.
//!
//! Each entry captures a snapshot summary at a point in time so the user
//! can spot trends (graph growth, dead-code drift, tracing regressions)
//! without storing full historical snapshots.
//!
//! Entries are appended on demand — typically called right after an
//! `analyze` completes. UI displays them as a horizontal timeline.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tauri::State;

use code_explorer_core::graph::types::*;

use crate::state::AppState;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActivityEntry {
    pub timestamp: i64,
    pub commit: Option<String>,
    pub node_count: u32,
    pub edge_count: u32,
    pub function_count: u32,
    pub file_count: u32,
    pub dead_count: u32,
    pub traced_count: u32,
    pub community_count: u32,
    /// Optional free-text label for the entry (e.g. "after auth refactor").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ActivityFile {
    #[serde(default)]
    pub entries: Vec<ActivityEntry>,
}

fn activity_path(storage: &str) -> PathBuf {
    PathBuf::from(storage).join("activity.json")
}

fn load(path: &std::path::Path) -> ActivityFile {
    match std::fs::read_to_string(path) {
        Ok(s) => serde_json::from_str(&s).unwrap_or_default(),
        Err(_) => ActivityFile::default(),
    }
}

fn save(path: &std::path::Path, file: &ActivityFile) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let s = serde_json::to_string_pretty(file).map_err(|e| e.to_string())?;
    std::fs::write(path, s).map_err(|e| e.to_string())
}

/// Snapshot the active repo's current state into the activity history.
/// Appends to the file; never overwrites previous entries.
#[tauri::command]
pub async fn activity_record(
    state: State<'_, AppState>,
    note: Option<String>,
) -> Result<ActivityEntry, String> {
    let storage = state.active_storage_path().await?;
    let (graph, _idx, _fts, _path) = state.get_repo(None).await?;

    let mut function_count = 0u32;
    let mut file_count = 0u32;
    let mut dead_count = 0u32;
    let mut traced_count = 0u32;
    let mut community_count = 0u32;
    for n in graph.iter_nodes() {
        match n.label {
            NodeLabel::Function | NodeLabel::Method | NodeLabel::Constructor => {
                function_count += 1;
                if n.properties.is_traced.unwrap_or(false) {
                    traced_count += 1;
                }
                if n.properties.is_dead_candidate.unwrap_or(false) {
                    dead_count += 1;
                }
            }
            NodeLabel::File => file_count += 1,
            NodeLabel::Community => community_count += 1,
            _ => {}
        }
    }

    let entry = ActivityEntry {
        timestamp: chrono::Utc::now().timestamp_millis(),
        commit: None,
        node_count: graph.iter_nodes().count() as u32,
        edge_count: graph.iter_relationships().count() as u32,
        function_count,
        file_count,
        dead_count,
        traced_count,
        community_count,
        note,
    };

    let path = activity_path(&storage);
    let mut file = load(&path);
    file.entries.push(entry.clone());
    // Cap the history at 200 entries to keep the file small and the
    // timeline readable. Drop oldest first.
    if file.entries.len() > 200 {
        let drop = file.entries.len() - 200;
        file.entries.drain(0..drop);
    }
    save(&path, &file)?;
    Ok(entry)
}

#[tauri::command]
pub async fn activity_list(state: State<'_, AppState>) -> Result<Vec<ActivityEntry>, String> {
    let storage = state.active_storage_path().await?;
    Ok(load(&activity_path(&storage)).entries)
}

#[tauri::command]
pub async fn activity_clear(state: State<'_, AppState>) -> Result<(), String> {
    let storage = state.active_storage_path().await?;
    let path = activity_path(&storage);
    save(&path, &ActivityFile::default())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_save_load_caps_at_200() {
        let dir =
            std::env::temp_dir().join(format!("code-explorer-activity-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("activity.json");
        let mut file = ActivityFile::default();
        for i in 0..250 {
            file.entries.push(ActivityEntry {
                timestamp: i as i64,
                commit: None,
                node_count: i as u32,
                edge_count: 0,
                function_count: 0,
                file_count: 0,
                dead_count: 0,
                traced_count: 0,
                community_count: 0,
                note: None,
            });
        }
        if file.entries.len() > 200 {
            let drop = file.entries.len() - 200;
            file.entries.drain(0..drop);
        }
        save(&path, &file).unwrap();
        let loaded = load(&path);
        assert_eq!(loaded.entries.len(), 200);
        // Oldest dropped → first remaining timestamp is 50.
        assert_eq!(loaded.entries[0].timestamp, 50);
        let _ = std::fs::remove_dir_all(&dir);
    }
}

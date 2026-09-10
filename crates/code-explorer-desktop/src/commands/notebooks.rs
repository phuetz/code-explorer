//! Cypher notebooks — full version of saved queries.
//!
//! A notebook is an ordered list of cells; each cell is either Markdown
//! prose or a Cypher query. Persisted as JSON in `<.codeexplorer>/notebooks/`,
//! one file per notebook so users can git-version individual notebooks.
//!
//! The runtime stores cell *outputs* alongside cells when the user opts in
//! (so the notebook can be reopened without re-running queries).

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tauri::State;
use uuid::Uuid;

use crate::state::AppState;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NotebookCell {
    pub id: String,
    /// "markdown" or "cypher"
    pub kind: String,
    /// Cell source content.
    pub source: String,
    /// Cached output for cypher cells (rendered as JSON).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cached_output: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_run_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Notebook {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    pub cells: Vec<NotebookCell>,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NotebookSummary {
    pub id: String,
    pub name: String,
    pub cell_count: u32,
    pub updated_at: i64,
}

fn notebooks_dir(storage: &str) -> PathBuf {
    PathBuf::from(storage).join("notebooks")
}

fn notebook_path(storage: &str, id: &str) -> Result<PathBuf, String> {
    // Defense against path traversal — only allow id matching [A-Za-z0-9_-]+
    let safe: String = id
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-')
        .collect();
    if safe.is_empty() {
        return Err("Invalid id: must contain at least one alphanumeric character".into());
    }
    Ok(notebooks_dir(storage).join(format!("{safe}.json")))
}

#[tauri::command]
pub async fn notebook_list(state: State<'_, AppState>) -> Result<Vec<NotebookSummary>, String> {
    let storage = state.active_storage_path().await?;
    let dir = notebooks_dir(&storage);
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    for entry in std::fs::read_dir(&dir).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let s = match std::fs::read_to_string(&path) {
            Ok(s) => s,
            Err(_) => continue,
        };
        let nb: Notebook = match serde_json::from_str(&s) {
            Ok(n) => n,
            Err(_) => continue,
        };
        out.push(NotebookSummary {
            id: nb.id,
            name: nb.name,
            cell_count: nb.cells.len() as u32,
            updated_at: nb.updated_at,
        });
    }
    out.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    Ok(out)
}

#[tauri::command]
pub async fn notebook_load(state: State<'_, AppState>, id: String) -> Result<Notebook, String> {
    let storage = state.active_storage_path().await?;
    let path = notebook_path(&storage, &id)?;
    let s =
        std::fs::read_to_string(&path).map_err(|e| format!("Notebook '{id}' not found: {e}"))?;
    serde_json::from_str(&s).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn notebook_save(
    state: State<'_, AppState>,
    notebook: Notebook,
) -> Result<NotebookSummary, String> {
    let storage = state.active_storage_path().await?;
    std::fs::create_dir_all(notebooks_dir(&storage)).map_err(|e| e.to_string())?;
    let mut nb = notebook;
    if nb.id.is_empty() {
        nb.id = format!("nb_{}", Uuid::new_v4().simple());
    }
    nb.updated_at = chrono::Utc::now().timestamp_millis();
    let path = notebook_path(&storage, &nb.id)?;
    let s = serde_json::to_string_pretty(&nb).map_err(|e| e.to_string())?;
    std::fs::write(&path, s).map_err(|e| e.to_string())?;
    Ok(NotebookSummary {
        id: nb.id,
        name: nb.name,
        cell_count: nb.cells.len() as u32,
        updated_at: nb.updated_at,
    })
}

#[tauri::command]
pub async fn notebook_delete(state: State<'_, AppState>, id: String) -> Result<(), String> {
    let storage = state.active_storage_path().await?;
    let path = notebook_path(&storage, &id)?;
    if path.exists() {
        std::fs::remove_file(&path).map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_notebook_path_filters_unsafe_chars() {
        let p = notebook_path("/tmp/store", "../../etc/passwd").unwrap();
        let last = p.file_name().unwrap().to_string_lossy().to_string();
        assert_eq!(last, "etcpasswd.json");
    }

    #[test]
    fn test_notebook_path_rejects_all_unsafe() {
        assert!(notebook_path("/tmp/store", "@@@@").is_err());
    }

    #[test]
    fn test_notebook_path_keeps_dashes_underscores() {
        let p = notebook_path("/tmp/store", "auth-flow_v2").unwrap();
        assert!(p.to_string_lossy().ends_with("auth-flow_v2.json"));
    }
}

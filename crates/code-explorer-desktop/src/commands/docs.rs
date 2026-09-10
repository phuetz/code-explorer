//! Commands for reading generated documentation.

use std::path::PathBuf;

use tauri::State;

use crate::state::AppState;
use crate::types::{DocContent, DocIndex};

/// Get the documentation index (navigation tree) for the active repository.
/// Reads from .codeexplorer/docs/_index.json
#[tauri::command]
pub async fn get_doc_index(state: State<'_, AppState>) -> Result<Option<DocIndex>, String> {
    let repo_path = get_active_repo_path(&state).await?;
    let index_path = repo_path.join(".codeexplorer").join("docs").join("_index.json");

    if !index_path.exists() {
        return Ok(None);
    }

    let content = std::fs::read_to_string(&index_path)
        .map_err(|e| format!("Failed to read doc index: {}", e))?;

    let index: DocIndex =
        serde_json::from_str(&content).map_err(|e| format!("Failed to parse doc index: {}", e))?;

    Ok(Some(index))
}

/// Read a specific documentation page by its relative path.
/// Path is relative to .codeexplorer/docs/
#[tauri::command]
pub async fn read_doc(state: State<'_, AppState>, path: String) -> Result<DocContent, String> {
    let repo_path = get_active_repo_path(&state).await?;
    let doc_path = repo_path.join(".codeexplorer").join("docs").join(&path);

    // Security: ensure the path doesn't escape the docs directory
    let canonical = doc_path
        .canonicalize()
        .map_err(|e| format!("Doc not found '{}': {}", path, e))?;
    let docs_dir = repo_path
        .join(".codeexplorer")
        .join("docs")
        .canonicalize()
        .map_err(|e| format!("Docs directory not found: {}", e))?;
    if !canonical.starts_with(&docs_dir) {
        return Err("Invalid doc path".to_string());
    }

    let content = std::fs::read_to_string(&canonical)
        .map_err(|e| format!("Failed to read doc '{}': {}", path, e))?;

    // Extract title from first markdown heading
    let title = content
        .lines()
        .find(|line| line.starts_with("# "))
        .map(|line| line.trim_start_matches("# ").to_string())
        .unwrap_or_else(|| path.clone());

    Ok(DocContent {
        path,
        content,
        title,
    })
}

/// Helper: get the active repository's filesystem path.
async fn get_active_repo_path(state: &State<'_, AppState>) -> Result<PathBuf, String> {
    let name = state
        .active_repo_name()
        .await
        .ok_or_else(|| "No active repository".to_string())?;

    let registry = state.registry().await;
    let entry = registry
        .iter()
        .find(|e| e.name == name)
        .ok_or_else(|| format!("Repository '{}' not found in registry", name))?;

    Ok(PathBuf::from(&entry.path))
}

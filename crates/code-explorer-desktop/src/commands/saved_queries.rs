//! Saved Cypher queries — small library per repo.
//!
//! Stored alongside bookmarks at `<storage_path>/saved_queries.json`.
//! Mini-version of "Cypher notebooks" from Axe E: lets the user name and
//! recall their most-used queries without leaving the Cypher panel.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tauri::State;

use crate::state::AppState;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SavedQuery {
    /// Stable id (uuid or slugged name).
    pub id: String,
    pub name: String,
    pub query: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Tags help filtering in the UI library.
    #[serde(default)]
    pub tags: Vec<String>,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SavedQueriesFile {
    #[serde(default)]
    pub queries: Vec<SavedQuery>,
}

fn queries_path(storage: &str) -> PathBuf {
    PathBuf::from(storage).join("saved_queries.json")
}

fn load(path: &std::path::Path) -> SavedQueriesFile {
    match std::fs::read_to_string(path) {
        Ok(s) => serde_json::from_str(&s).unwrap_or_default(),
        Err(_) => SavedQueriesFile::default(),
    }
}

fn save(path: &std::path::Path, file: &SavedQueriesFile) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let s = serde_json::to_string_pretty(file).map_err(|e| e.to_string())?;
    std::fs::write(path, s).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn saved_queries_list(state: State<'_, AppState>) -> Result<Vec<SavedQuery>, String> {
    let storage = state.active_storage_path().await?;
    Ok(load(&queries_path(&storage)).queries)
}

#[tauri::command]
pub async fn saved_queries_save(
    state: State<'_, AppState>,
    query: SavedQuery,
) -> Result<Vec<SavedQuery>, String> {
    let storage = state.active_storage_path().await?;
    let path = queries_path(&storage);
    let mut file = load(&path);
    // Upsert by id.
    file.queries.retain(|q| q.id != query.id);
    file.queries.push(query);
    file.queries.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    save(&path, &file)?;
    Ok(file.queries)
}

#[tauri::command]
pub async fn saved_queries_delete(
    state: State<'_, AppState>,
    id: String,
) -> Result<Vec<SavedQuery>, String> {
    let storage = state.active_storage_path().await?;
    let path = queries_path(&storage);
    let mut file = load(&path);
    file.queries.retain(|q| q.id != id);
    save(&path, &file)?;
    Ok(file.queries)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_save_load_upsert() {
        let dir =
            std::env::temp_dir().join(format!("code-explorer-queries-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("queries.json");
        let q = SavedQuery {
            id: "q1".into(),
            name: "All functions".into(),
            query: "MATCH (n:Function) RETURN n LIMIT 50".into(),
            description: None,
            tags: vec!["overview".into()],
            updated_at: 1,
        };
        let file = SavedQueriesFile {
            queries: vec![q.clone()],
        };
        save(&path, &file).unwrap();
        let loaded = load(&path);
        assert_eq!(loaded.queries.len(), 1);
        assert_eq!(loaded.queries[0].id, "q1");
        let _ = std::fs::remove_dir_all(&dir);
    }
}

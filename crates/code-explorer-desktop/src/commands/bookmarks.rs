//! User bookmarks for graph nodes — persisted per repo.
//!
//! Stored as `<storage_path>/bookmarks.json` so they travel with the
//! repo's `.codeexplorer/` folder (gitignored by convention) and survive
//! re-indexing.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tauri::State;

use crate::state::AppState;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Bookmark {
    /// Stable graph node id ("Function:src/foo.ts:bar").
    pub node_id: String,
    pub name: String,
    pub label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_path: Option<String>,
    /// Optional user-provided note (one short sentence).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    /// Unix epoch milliseconds.
    pub created_at: i64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BookmarksFile {
    #[serde(default)]
    pub bookmarks: Vec<Bookmark>,
}

fn bookmarks_path(storage: &str) -> PathBuf {
    PathBuf::from(storage).join("bookmarks.json")
}

fn load(path: &std::path::Path) -> BookmarksFile {
    match std::fs::read_to_string(path) {
        Ok(s) => serde_json::from_str(&s).unwrap_or_default(),
        Err(_) => BookmarksFile::default(),
    }
}

fn save(path: &std::path::Path, file: &BookmarksFile) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let s = serde_json::to_string_pretty(file).map_err(|e| e.to_string())?;
    std::fs::write(path, s).map_err(|e| e.to_string())
}

async fn resolve_storage(state: &State<'_, AppState>) -> Result<String, String> {
    state.active_storage_path().await
}

#[tauri::command]
pub async fn bookmarks_list(state: State<'_, AppState>) -> Result<Vec<Bookmark>, String> {
    let storage = resolve_storage(&state).await?;
    Ok(load(&bookmarks_path(&storage)).bookmarks)
}

#[tauri::command]
pub async fn bookmarks_add(
    state: State<'_, AppState>,
    bookmark: Bookmark,
) -> Result<Vec<Bookmark>, String> {
    let storage = resolve_storage(&state).await?;
    let path = bookmarks_path(&storage);
    let mut file = load(&path);
    // Idempotent: same node_id replaces the previous entry.
    file.bookmarks.retain(|b| b.node_id != bookmark.node_id);
    file.bookmarks.push(bookmark);
    save(&path, &file)?;
    Ok(file.bookmarks)
}

#[tauri::command]
pub async fn bookmarks_remove(
    state: State<'_, AppState>,
    node_id: String,
) -> Result<Vec<Bookmark>, String> {
    let storage = resolve_storage(&state).await?;
    let path = bookmarks_path(&storage);
    let mut file = load(&path);
    file.bookmarks.retain(|b| b.node_id != node_id);
    save(&path, &file)?;
    Ok(file.bookmarks)
}

#[tauri::command]
pub async fn bookmarks_clear(state: State<'_, AppState>) -> Result<(), String> {
    let storage = resolve_storage(&state).await?;
    let path = bookmarks_path(&storage);
    save(&path, &BookmarksFile::default())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_save_and_load_roundtrip() {
        let dir =
            std::env::temp_dir().join(format!("code-explorer-bookmarks-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("bookmarks.json");
        let file = BookmarksFile {
            bookmarks: vec![Bookmark {
                node_id: "Function:src/a.rs:foo".into(),
                name: "foo".into(),
                label: "Function".into(),
                file_path: Some("src/a.rs".into()),
                note: Some("entry point".into()),
                created_at: 1700000000000,
            }],
        };
        save(&path, &file).unwrap();
        let loaded = load(&path);
        assert_eq!(loaded.bookmarks.len(), 1);
        assert_eq!(loaded.bookmarks[0].name, "foo");
        assert_eq!(loaded.bookmarks[0].note.as_deref(), Some("entry point"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_load_missing_returns_empty() {
        let f = load(std::path::Path::new("/nonexistent/path/bookmarks.json"));
        assert!(f.bookmarks.is_empty());
    }
}

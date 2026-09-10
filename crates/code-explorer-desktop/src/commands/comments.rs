//! Per-node comment threads — persisted per repo.
//!
//! Stored as `<storage_path>/comments.json`. Each node id maps to a list
//! of comments; comments are append-only with a delete-by-id endpoint.

use std::collections::HashMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tauri::State;
use uuid::Uuid;

use crate::state::AppState;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Comment {
    pub id: String,
    pub node_id: String,
    pub author: String,
    pub body: String,
    pub created_at: i64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CommentsFile {
    /// node_id → comments
    #[serde(default)]
    pub by_node: HashMap<String, Vec<Comment>>,
}

fn comments_path(storage: &str) -> PathBuf {
    PathBuf::from(storage).join("comments.json")
}

fn load(path: &std::path::Path) -> CommentsFile {
    match std::fs::read_to_string(path) {
        Ok(s) => serde_json::from_str(&s).unwrap_or_default(),
        Err(_) => CommentsFile::default(),
    }
}

fn save(path: &std::path::Path, file: &CommentsFile) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let s = serde_json::to_string_pretty(file).map_err(|e| e.to_string())?;
    std::fs::write(path, s).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn comments_for_node(
    state: State<'_, AppState>,
    node_id: String,
) -> Result<Vec<Comment>, String> {
    let storage = state.active_storage_path().await?;
    let file = load(&comments_path(&storage));
    Ok(file.by_node.get(&node_id).cloned().unwrap_or_default())
}

#[tauri::command]
pub async fn comments_add(
    state: State<'_, AppState>,
    node_id: String,
    author: String,
    body: String,
) -> Result<Vec<Comment>, String> {
    let body = body.trim();
    if body.is_empty() {
        return Err("Comment body cannot be empty".into());
    }
    let storage = state.active_storage_path().await?;
    let path = comments_path(&storage);
    let mut file = load(&path);
    let comment = Comment {
        id: format!("c_{}", Uuid::new_v4()),
        node_id: node_id.clone(),
        author: if author.trim().is_empty() {
            "anonymous".into()
        } else {
            author
        },
        body: body.to_string(),
        created_at: chrono::Utc::now().timestamp_millis(),
    };
    file.by_node
        .entry(node_id.clone())
        .or_default()
        .push(comment);
    save(&path, &file)?;
    Ok(file.by_node.get(&node_id).cloned().unwrap_or_default())
}

#[tauri::command]
pub async fn comments_remove(
    state: State<'_, AppState>,
    node_id: String,
    comment_id: String,
) -> Result<Vec<Comment>, String> {
    let storage = state.active_storage_path().await?;
    let path = comments_path(&storage);
    let mut file = load(&path);
    if let Some(list) = file.by_node.get_mut(&node_id) {
        list.retain(|c| c.id != comment_id);
    }
    save(&path, &file)?;
    Ok(file.by_node.get(&node_id).cloned().unwrap_or_default())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_save_load_roundtrip() {
        let dir =
            std::env::temp_dir().join(format!("code-explorer-comments-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("comments.json");
        let mut file = CommentsFile::default();
        file.by_node.insert(
            "n1".into(),
            vec![Comment {
                id: "c1".into(),
                node_id: "n1".into(),
                author: "alice".into(),
                body: "Look here".into(),
                created_at: 1700000000000,
            }],
        );
        save(&path, &file).unwrap();
        let loaded = load(&path);
        assert_eq!(loaded.by_node.get("n1").unwrap().len(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }
}

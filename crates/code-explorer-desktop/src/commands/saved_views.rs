//! Saved graph views — name + recall a curated graph configuration.
//!
//! Theme C — a `SavedView` captures the set of toggles the user assembled to
//! reach an interesting picture of the graph: lens, filters, camera position,
//! manual node selection. Persisted per-repo at `<storage>/saved_views.json`.
//!
//! Mirror of `saved_queries.rs` (same load/save/upsert pattern). The Rust
//! side is intentionally schema-loose — the front-end Zustand store owns the
//! semantics; we just persist the JSON blob.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tauri::State;

use crate::state::AppState;

/// A camera position (Sigma.js convention: viewport centered on `(x, y)` with
/// `ratio` meaning 1.0 = default zoom).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CameraState {
    pub x: f64,
    pub y: f64,
    pub ratio: f64,
    #[serde(default)]
    pub angle: f64,
}

/// Saved graph view — a "bookmark" for a complete view configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SavedView {
    /// Stable id (timestamp-derived or UUID).
    pub id: String,
    /// Optional repo scope. Most clients save per-repo, so we recommend
    /// passing the active repo name; left as `Option` so the same store
    /// can host repo-agnostic favorites.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repo: Option<String>,
    pub name: String,
    /// Active lens identifier (`"all"`, `"calls"`, `"hotspots"`, etc.).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lens: Option<String>,
    /// Free-form filter blob — the front end persists whatever filter object
    /// it owns (zoom level, hidden edge types, complexity threshold, ...).
    #[serde(default)]
    pub filters: serde_json::Value,
    /// Camera position so re-applying the view restores zoom/pan.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub camera_state: Option<CameraState>,
    /// Manually-selected node IDs preserved across re-renders.
    #[serde(default)]
    pub node_selection: Vec<String>,
    /// Optional human description.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Creation/last-update timestamp (ms epoch).
    pub created_at: i64,
    /// Updated timestamp (ms epoch). Defaults to `created_at` when absent.
    #[serde(default)]
    pub updated_at: i64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SavedViewsFile {
    #[serde(default)]
    pub views: Vec<SavedView>,
}

fn views_path(storage: &str) -> PathBuf {
    PathBuf::from(storage).join("saved_views.json")
}

fn load(path: &std::path::Path) -> SavedViewsFile {
    match std::fs::read_to_string(path) {
        Ok(s) => serde_json::from_str(&s).unwrap_or_default(),
        Err(_) => SavedViewsFile::default(),
    }
}

fn save(path: &std::path::Path, file: &SavedViewsFile) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let s = serde_json::to_string_pretty(file).map_err(|e| e.to_string())?;
    std::fs::write(path, s).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn saved_views_list(state: State<'_, AppState>) -> Result<Vec<SavedView>, String> {
    let storage = state.active_storage_path().await?;
    Ok(load(&views_path(&storage)).views)
}

#[tauri::command]
pub async fn saved_views_save(
    state: State<'_, AppState>,
    view: SavedView,
) -> Result<Vec<SavedView>, String> {
    let storage = state.active_storage_path().await?;
    let path = views_path(&storage);
    let mut file = load(&path);
    let mut next = view;
    if next.updated_at == 0 {
        next.updated_at = next.created_at.max(chrono::Utc::now().timestamp_millis());
    }
    // Upsert by id.
    file.views.retain(|v| v.id != next.id);
    file.views.push(next);
    file.views.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    save(&path, &file)?;
    Ok(file.views)
}

#[tauri::command]
pub async fn saved_views_delete(
    state: State<'_, AppState>,
    id: String,
) -> Result<Vec<SavedView>, String> {
    let storage = state.active_storage_path().await?;
    let path = views_path(&storage);
    let mut file = load(&path);
    file.views.retain(|v| v.id != id);
    save(&path, &file)?;
    Ok(file.views)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_save_load_upsert() {
        let dir =
            std::env::temp_dir().join(format!("code-explorer-saved-views-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("saved_views.json");
        let v = SavedView {
            id: "v1".into(),
            repo: Some("demo".into()),
            name: "Auth flow".into(),
            lens: Some("calls".into()),
            filters: serde_json::json!({ "zoomLevel": "symbol" }),
            camera_state: Some(CameraState {
                x: 0.5,
                y: 0.5,
                ratio: 1.2,
                angle: 0.0,
            }),
            node_selection: vec!["Function:src/auth.ts:login".into()],
            description: None,
            created_at: 1,
            updated_at: 1,
        };
        let file = SavedViewsFile { views: vec![v] };
        save(&path, &file).unwrap();
        let loaded = load(&path);
        assert_eq!(loaded.views.len(), 1);
        assert_eq!(loaded.views[0].id, "v1");
        assert_eq!(loaded.views[0].lens.as_deref(), Some("calls"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}

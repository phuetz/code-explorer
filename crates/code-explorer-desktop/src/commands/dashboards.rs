//! Custom dashboards (Axe E).
//!
//! A dashboard is an ordered list of widgets. Each widget runs a Cypher
//! query and renders the result as a metric / table / bar chart. Stored
//! per-repo in `<.codeexplorer>/dashboards/`, one file per dashboard so
//! individual dashboards are easy to share or git-version.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tauri::State;
use uuid::Uuid;

use crate::state::AppState;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DashboardWidget {
    pub id: String,
    pub title: String,
    /// "metric" | "table" | "bar"
    pub kind: String,
    pub cypher: String,
    /// Optional column to use as the value for "metric" / "bar" widgets.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value_column: Option<String>,
    /// Optional column to use as the label for "bar" widgets.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label_column: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Dashboard {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    pub widgets: Vec<DashboardWidget>,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DashboardSummary {
    pub id: String,
    pub name: String,
    pub widget_count: u32,
    pub updated_at: i64,
}

fn dashboards_dir(storage: &str) -> PathBuf {
    PathBuf::from(storage).join("dashboards")
}

fn dashboard_path(storage: &str, id: &str) -> Result<PathBuf, String> {
    let safe: String = id
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-')
        .collect();
    if safe.is_empty() {
        return Err("Invalid id: must contain at least one alphanumeric character".into());
    }
    Ok(dashboards_dir(storage).join(format!("{safe}.json")))
}

#[tauri::command]
pub async fn dashboard_list(state: State<'_, AppState>) -> Result<Vec<DashboardSummary>, String> {
    let storage = state.active_storage_path().await?;
    let dir = dashboards_dir(&storage);
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
        let d: Dashboard = match serde_json::from_str(&s) {
            Ok(d) => d,
            Err(_) => continue,
        };
        out.push(DashboardSummary {
            id: d.id,
            name: d.name,
            widget_count: d.widgets.len() as u32,
            updated_at: d.updated_at,
        });
    }
    out.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    Ok(out)
}

#[tauri::command]
pub async fn dashboard_load(state: State<'_, AppState>, id: String) -> Result<Dashboard, String> {
    let storage = state.active_storage_path().await?;
    let path = dashboard_path(&storage, &id)?;
    let s =
        std::fs::read_to_string(&path).map_err(|e| format!("Dashboard '{id}' not found: {e}"))?;
    serde_json::from_str(&s).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn dashboard_save(
    state: State<'_, AppState>,
    dashboard: Dashboard,
) -> Result<DashboardSummary, String> {
    let storage = state.active_storage_path().await?;
    std::fs::create_dir_all(dashboards_dir(&storage)).map_err(|e| e.to_string())?;
    let mut d = dashboard;
    if d.id.is_empty() {
        d.id = format!("dash_{}", Uuid::new_v4().simple());
    }
    d.updated_at = chrono::Utc::now().timestamp_millis();
    let path = dashboard_path(&storage, &d.id)?;
    let s = serde_json::to_string_pretty(&d).map_err(|e| e.to_string())?;
    std::fs::write(&path, s).map_err(|e| e.to_string())?;
    Ok(DashboardSummary {
        id: d.id,
        name: d.name,
        widget_count: d.widgets.len() as u32,
        updated_at: d.updated_at,
    })
}

#[tauri::command]
pub async fn dashboard_delete(state: State<'_, AppState>, id: String) -> Result<(), String> {
    let storage = state.active_storage_path().await?;
    let path = dashboard_path(&storage, &id)?;
    if path.exists() {
        std::fs::remove_file(&path).map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dashboard_path_strips_unsafe_chars() {
        let p = dashboard_path("/tmp/x", "../../etc/passwd").unwrap();
        assert!(p.to_string_lossy().ends_with("etcpasswd.json"));
    }

    #[test]
    fn test_dashboard_path_rejects_all_unsafe() {
        assert!(dashboard_path("/tmp/x", "@@@@").is_err());
    }
}

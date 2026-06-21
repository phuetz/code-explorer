use serde::Serialize;
use tauri::State;

use crate::state::AppState;

// ─── Hotspots ────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileHotspot {
    pub path: String,
    pub commit_count: u32,
    pub lines_added: u32,
    pub lines_removed: u32,
    pub churn: u32,
    pub score: f64,
    pub last_modified: String,
    pub author_count: u32,
}

#[tauri::command]
pub async fn get_hotspots(
    state: State<'_, AppState>,
    since_days: Option<u32>,
) -> Result<Vec<FileHotspot>, String> {
    let (_, _, _, repo_path_str) = state.get_repo(None).await?;
    let days = since_days.unwrap_or(90);
    let repo_path = std::path::PathBuf::from(repo_path_str);

    let result = tokio::task::spawn_blocking(move || {
        code_explorer_git::hotspots::analyze_hotspots(&repo_path, days)
    })
    .await
    .map_err(|e| format!("Task error: {}", e))?
    .map_err(|e| format!("Hotspot analysis failed: {}", e))?;

    Ok(result
        .into_iter()
        .map(|h| FileHotspot {
            path: h.path,
            commit_count: h.commit_count,
            lines_added: h.lines_added,
            lines_removed: h.lines_removed,
            churn: h.churn,
            score: h.score,
            last_modified: h.last_modified,
            author_count: h.author_count,
        })
        .collect())
}

// ─── Coupling ────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangeCoupling {
    pub file_a: String,
    pub file_b: String,
    pub shared_commits: u32,
    pub coupling_strength: f64,
}

#[tauri::command]
pub async fn get_coupling(
    state: State<'_, AppState>,
    min_shared: Option<u32>,
) -> Result<Vec<ChangeCoupling>, String> {
    let (_, _, _, repo_path_str) = state.get_repo(None).await?;
    let min = min_shared.unwrap_or(3);
    let repo_path = std::path::PathBuf::from(repo_path_str);

    let result = tokio::task::spawn_blocking(move || {
        code_explorer_git::coupling::analyze_coupling(&repo_path, min, Some(180))
    })
    .await
    .map_err(|e| format!("Task error: {}", e))?
    .map_err(|e| format!("Coupling analysis failed: {}", e))?;

    Ok(result
        .into_iter()
        .map(|c| ChangeCoupling {
            file_a: c.file_a,
            file_b: c.file_b,
            shared_commits: c.shared_commits,
            coupling_strength: c.coupling_strength,
        })
        .collect())
}

// ─── Ownership ───────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthorContribution {
    pub name: String,
    pub email: String,
    pub commits: u32,
    pub pct: f64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileOwnership {
    pub path: String,
    pub primary_author: String,
    pub ownership_pct: f64,
    pub author_count: u32,
    pub authors: Vec<AuthorContribution>,
}

#[tauri::command]
pub async fn get_ownership(state: State<'_, AppState>) -> Result<Vec<FileOwnership>, String> {
    let (_, _, _, repo_path_str) = state.get_repo(None).await?;
    let repo_path = std::path::PathBuf::from(repo_path_str);

    let result =
        tokio::task::spawn_blocking(move || code_explorer_git::ownership::analyze_ownership(&repo_path))
            .await
            .map_err(|e| format!("Task error: {}", e))?
            .map_err(|e| format!("Ownership analysis failed: {}", e))?;

    Ok(result
        .into_iter()
        .map(|o| FileOwnership {
            path: o.path,
            primary_author: o.primary_author,
            ownership_pct: o.ownership_pct,
            author_count: o.author_count,
            authors: o
                .authors
                .into_iter()
                .map(|a| AuthorContribution {
                    name: a.name,
                    email: a.email,
                    commits: a.commits,
                    pct: a.pct,
                })
                .collect(),
        })
        .collect())
}

use std::path::Path;

use tauri::{AppHandle, Emitter, State};
use tokio::sync::mpsc;

use code_explorer_core::pipeline::types::PipelineProgress;
use code_explorer_core::storage::{git, repo_manager};
use code_explorer_db::csv_generator;
use code_explorer_db::snapshot;
use code_explorer_ingest::pipeline::{run_pipeline, PipelineOptions};

use crate::state::AppState;
use crate::types::RepoInfo;

#[tauri::command]
pub async fn list_repos(state: State<'_, AppState>) -> Result<Vec<RepoInfo>, String> {
    let entries = state.load_registry().await?;
    let repos = entries
        .into_iter()
        .map(|e| RepoInfo {
            name: e.name,
            path: e.path.strip_prefix(r"\\?\").unwrap_or(&e.path).to_string(),
            indexed_at: e.indexed_at,
            last_commit: e.last_commit,
            files: e.stats.as_ref().and_then(|s| s.files),
            nodes: e.stats.as_ref().and_then(|s| s.nodes),
            edges: e.stats.as_ref().and_then(|s| s.edges),
            communities: e.stats.as_ref().and_then(|s| s.communities),
            index_duration_ms: e.stats.as_ref().and_then(|s| s.index_duration_ms),
        })
        .collect();
    Ok(repos)
}

#[tauri::command]
pub async fn open_repo(state: State<'_, AppState>, name: String) -> Result<RepoInfo, String> {
    state.open_repo(&name).await?;

    let registry = state.registry().await;
    let entry = registry
        .iter()
        .find(|e| e.name == name)
        .ok_or_else(|| format!("Repository '{}' not found", name))?;

    Ok(RepoInfo {
        name: entry.name.clone(),
        path: entry.path.clone(),
        indexed_at: entry.indexed_at.clone(),
        last_commit: entry.last_commit.clone(),
        files: entry.stats.as_ref().and_then(|s| s.files),
        nodes: entry.stats.as_ref().and_then(|s| s.nodes),
        edges: entry.stats.as_ref().and_then(|s| s.edges),
        communities: entry.stats.as_ref().and_then(|s| s.communities),
        index_duration_ms: entry.stats.as_ref().and_then(|s| s.index_duration_ms),
    })
}

/// Index a repository using the Rust pipeline directly (no subprocess).
/// Emits "pipeline-progress" events to the frontend for real-time progress tracking.
#[tauri::command]
pub async fn analyze_repo(
    app: AppHandle,
    state: State<'_, AppState>,
    path: String,
) -> Result<String, String> {
    let repo_path = Path::new(&path)
        .canonicalize()
        .map_err(|e| format!("Invalid path '{}': {}", path, e))?;

    let options = PipelineOptions {
        force: true,
        embeddings: false,
        verbose: false,
        skip_git: false,
        ..Default::default()
    };

    // Create a progress channel and forward events to the frontend via Tauri events
    let (progress_tx, mut progress_rx) = mpsc::unbounded_channel::<PipelineProgress>();

    let app_handle = app.clone();
    tokio::spawn(async move {
        while let Some(progress) = progress_rx.recv().await {
            let _ = app_handle.emit("pipeline-progress", &progress);
        }
    });

    // Run the ingestion pipeline with progress reporting
    let result = run_pipeline(&repo_path, Some(progress_tx), options)
        .await
        .map_err(|e| format!("Pipeline failed: {}", e))?;

    let file_count = result.total_file_count;
    let node_count = result.graph.node_count();
    let edge_count = result.graph.relationship_count();
    let community_count = result.community_count;
    let process_count = result.process_count;

    // Save metadata
    let commit = git::current_commit(&repo_path).unwrap_or_else(|| "unknown".to_string());
    let meta = repo_manager::RepoMeta {
        repo_path: repo_path.display().to_string(),
        last_commit: commit,
        indexed_at: chrono_now(),
        stats: Some(repo_manager::RepoStats {
            files: Some(file_count),
            nodes: Some(node_count),
            edges: Some(edge_count),
            communities: Some(community_count),
            processes: Some(process_count),
            embeddings: None,
            index_duration_ms: Some(result.total_duration_ms),
        }),
    };

    let storage_paths = repo_manager::get_storage_paths(&repo_path);
    repo_manager::save_meta(&storage_paths.storage_path, &meta)
        .map_err(|e| format!("Failed to save metadata: {}", e))?;
    repo_manager::register_repo(&repo_path, &meta)
        .map_err(|e| format!("Failed to register repo: {}", e))?;

    // Persist the detailed performance metrics (per-phase breakdown + throughput).
    {
        let secs = result.total_duration_ms as f64 / 1000.0;
        let metrics = code_explorer_core::pipeline::types::IndexMetrics {
            schema_version: 1,
            tool_version: env!("CARGO_PKG_VERSION").to_string(),
            indexed_at: meta.indexed_at.clone(),
            total_duration_ms: result.total_duration_ms,
            phases: result.phase_timings.clone(),
            files: file_count,
            nodes: node_count,
            edges: edge_count,
            communities: community_count,
            processes: process_count,
            files_per_sec: if secs > 0.0 { file_count as f64 / secs } else { 0.0 },
            nodes_per_sec: if secs > 0.0 { node_count as f64 / secs } else { 0.0 },
        };
        repo_manager::save_metrics(&storage_paths.storage_path, &metrics)
            .map_err(|e| format!("Failed to save metrics: {}", e))?;
    }

    // Save graph snapshot
    let snap_path = snapshot::snapshot_path(&storage_paths.storage_path);
    snapshot::save_snapshot(&result.graph, &snap_path)
        .map_err(|e| format!("Failed to save snapshot: {}", e))?;

    // Save file manifest so subsequent `code-explorer watch` / incremental runs
    // can correctly diff against the indexed state. Without this, a desktop
    // analyze followed by a CLI watch would treat every source file as
    // newly added on the first incremental pass and re-parse the whole
    // repository. Mirrors the CLI `analyze` command.
    {
        let file_entries = code_explorer_ingest::phases::structure::walk_repository(&repo_path)
            .map_err(|e| format!("Failed to walk repo for manifest: {}", e))?;
        let manifest = code_explorer_ingest::manifest::build_manifest_from_entries(&file_entries);
        let manifest_file = code_explorer_ingest::manifest::manifest_path(&storage_paths.storage_path);
        code_explorer_ingest::manifest::save_manifest(&manifest, &manifest_file)
            .map_err(|e| format!("Failed to save manifest: {}", e))?;
    }

    // Generate CSVs
    let csv_dir = storage_paths.storage_path.join("csv");
    std::fs::create_dir_all(&csv_dir).map_err(|e| format!("Failed to create CSV dir: {}", e))?;
    csv_generator::generate_all_csvs(&result.graph, &repo_path, &csv_dir)
        .map_err(|e| format!("Failed to generate CSVs: {}", e))?;

    // Reload the repo in AppState so the UI picks up new data.
    //
    // Look up the canonical name from the registry by path comparison instead
    // of re-deriving via `file_name()`, which returns None for paths with a
    // trailing separator and would silently fall back to "unknown".
    state.load_registry().await?;
    let registry = state.registry().await;
    let canonical_str = repo_path.display().to_string();
    let resolved_name = registry
        .iter()
        .find(|e| {
            // Compare canonicalized strings; fall back to ends_with as a last resort
            e.path == canonical_str || e.path.ends_with(canonical_str.as_str())
        })
        .map(|e| e.name.clone())
        .unwrap_or_else(|| {
            repo_path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("unknown")
                .to_string()
        });
    let _ = state.reload_repo(&resolved_name).await;

    // Emit completion event
    let _ = app.emit(
        "pipeline-progress",
        &PipelineProgress {
            phase: code_explorer_core::pipeline::types::PipelinePhase::Complete,
            percent: 100.0,
            message: format!(
                "Indexed successfully: {} files, {} nodes, {} edges, {} communities",
                file_count, node_count, edge_count, community_count
            ),
            detail: None,
            stats: Some(code_explorer_core::pipeline::types::PipelineStats {
                files_processed: file_count,
                total_files: file_count,
                nodes_created: node_count,
            }),
        },
    );

    Ok(format!(
        "Indexed successfully: {} files, {} nodes, {} edges, {} communities",
        file_count, node_count, edge_count, community_count
    ))
}

/// Remove a repo from the global registry without touching its files on disk.
/// Used when the `.codeexplorer/graph.bin` is missing/corrupted and the user
/// wants to drop the orphan entry from the UI.
#[tauri::command]
pub async fn unregister_repo(path: String) -> Result<(), String> {
    let repo_path = std::path::Path::new(&path);
    repo_manager::unregister_repo(repo_path).map_err(|e| format!("Failed to unregister: {e}"))
}

/// Generate docs (wiki, context, skills) using the Rust CLI binary.
/// Finds the code-explorer binary next to the desktop binary, then falls back to PATH.
#[tauri::command]
pub async fn generate_docs(what: String, path: String) -> Result<String, String> {
    let valid = ["context", "wiki", "skills", "docs", "all"];
    if !valid.contains(&what.as_str()) {
        return Err(format!(
            "Invalid target '{}'. Must be one of: {}",
            what,
            valid.join(", ")
        ));
    }

    // Validate path: must exist and be a directory
    let repo_path = std::path::Path::new(&path);
    if !repo_path.exists() || !repo_path.is_dir() {
        return Err(format!(
            "Invalid path: '{}' does not exist or is not a directory",
            path
        ));
    }
    // Canonicalize to prevent path traversal via ..
    let canonical_path = repo_path
        .canonicalize()
        .map_err(|e| format!("Invalid path: {}", e))?;
    let safe_path = canonical_path.to_string_lossy().to_string();

    let code_explorer_bin = find_code_explorer_binary()?;

    // Use tokio::process::Command so the subprocess runs asynchronously and
    // does NOT block the tokio runtime thread. Previously this was
    // std::process::Command::output(), which is a blocking call that stalls
    // every other Tauri IPC command for the duration of the subprocess —
    // easy to notice because `generate all` on a large repo takes tens of
    // seconds during which the UI becomes unresponsive.
    let output = tokio::process::Command::new(&code_explorer_bin)
        .args(["generate", &what, "--path", &safe_path])
        .output()
        .await
        .map_err(|e| format!("Failed to run '{}'. Error: {}", code_explorer_bin, e))?;

    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    } else {
        Err(format!(
            "generate {} failed: {}",
            what,
            String::from_utf8_lossy(&output.stderr)
        ))
    }
}

/// Find the code-explorer CLI binary.
/// 1. Look next to the current executable (same build output dir)
/// 2. Fall back to "code-explorer" in PATH
fn find_code_explorer_binary() -> Result<String, String> {
    if let Ok(explicit) = std::env::var("CODE_EXPLORER_CLI_PATH") {
        let path = Path::new(&explicit);
        if path.exists() {
            return Ok(explicit);
        }
        return Err(format!(
            "CODE_EXPLORER_CLI_PATH points to a missing file: {}",
            explicit
        ));
    }

    // In dev/debug, the desktop binary is at target/debug/code-explorer-desktop.exe
    // and the CLI binary is at target/debug/code-explorer.exe (same directory).
    // Prefer the current "code-explorer" name, fall back to the legacy "code-explorer"
    // name so older builds keep working.
    if let Ok(current_exe) = std::env::current_exe() {
        if let Some(dir) = current_exe.parent() {
            let names: &[&str] = if cfg!(windows) {
                &["code-explorer.exe", "code-explorer.exe"]
            } else {
                &["code-explorer", "code-explorer"]
            };
            for name in names {
                let sibling = dir.join(name);
                if sibling.exists() {
                    return Ok(sibling.display().to_string());
                }
            }
        }
    }

    Err(
        "Code Explorer CLI binary not found next to the desktop binary. Build the CLI or set CODE_EXPLORER_CLI_PATH."
            .to_string(),
    )
}

fn chrono_now() -> String {
    // Produce a proper RFC 3339 / ISO 8601 timestamp like "2026-04-06T08:30:00Z".
    // The previous implementation emitted "1712486400Z" — a Unix epoch with a
    // bare `Z` suffix — which is not a valid date string and breaks the
    // frontend's `new Date(...)` parsing on the repo registry display.
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

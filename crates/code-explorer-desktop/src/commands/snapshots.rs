//! Snapshot history + diff (Axes B3 full + B4).
//!
//! Persists named copies of `graph.bin` into `<.codeexplorer>/snapshots/` so the
//! user can compare the architecture state between two points in time and
//! navigate back to a previous structure without re-indexing.
//!
//! The store is capped (default 10) to keep disk usage bounded — oldest
//! snapshots get evicted FIFO when the limit is hit.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde::{Deserialize, Serialize};
use tauri::State;

use crate::state::AppState;

const MAX_SNAPSHOTS: usize = 10;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotMeta {
    /// Snapshot id (timestamp-derived, safe filename).
    pub id: String,
    pub label: String,
    pub created_at: i64,
    pub node_count: u32,
    pub edge_count: u32,
    pub size_bytes: u64,
    /// Commit SHA captured at snapshot time (Theme C — commit-aware snapshots).
    /// `None` for legacy/manual snapshots taken from the live graph.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub commit_sha: Option<String>,
    /// Author/committer timestamp of `commit_sha` (ms epoch). Optional.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub authored_at: Option<i64>,
    /// First line of the commit message.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subject: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotIndex {
    #[serde(default)]
    pub snapshots: Vec<SnapshotMeta>,
}

fn snapshots_dir(storage: &str) -> PathBuf {
    PathBuf::from(storage).join("snapshots")
}

fn index_path(storage: &str) -> PathBuf {
    snapshots_dir(storage).join("index.json")
}

fn snapshot_file_path(storage: &str, id: &str) -> PathBuf {
    let safe: String = id
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-')
        .collect();
    snapshots_dir(storage).join(format!("{safe}.bin"))
}

fn snapshot_meta_path(storage: &str, id: &str) -> PathBuf {
    let safe: String = id
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-')
        .collect();
    snapshots_dir(storage).join(format!("{safe}.meta.json"))
}

/// Persist per-snapshot metadata sidecar. Best-effort — if the write fails we
/// still return the SnapshotMeta so the caller's flow isn't aborted; the next
/// `snapshot_list` call will simply not have the sidecar to enrich from.
fn write_snapshot_metadata(storage: &str, meta: &SnapshotMeta) -> Result<(), String> {
    let path = snapshot_meta_path(storage, &meta.id);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let s = serde_json::to_string_pretty(meta).map_err(|e| e.to_string())?;
    std::fs::write(path, s).map_err(|e| e.to_string())
}

fn load_index(storage: &str) -> SnapshotIndex {
    match std::fs::read_to_string(index_path(storage)) {
        Ok(s) => serde_json::from_str(&s).unwrap_or_default(),
        Err(_) => SnapshotIndex::default(),
    }
}

fn save_index(storage: &str, idx: &SnapshotIndex) -> Result<(), String> {
    let dir = snapshots_dir(storage);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let s = serde_json::to_string_pretty(idx).map_err(|e| e.to_string())?;
    std::fs::write(index_path(storage), s).map_err(|e| e.to_string())
}

// ─── Tauri commands ────────────────────────────────────────────────

/// Create a snapshot of the current graph, or — when `commit_sha` is set —
/// ingest the repo at that commit via a temporary `git worktree` and snapshot
/// the result. The user's working tree is never touched.
#[tauri::command]
pub async fn snapshot_create(
    state: State<'_, AppState>,
    label: Option<String>,
    commit_sha: Option<String>,
) -> Result<SnapshotMeta, String> {
    let storage = state.active_storage_path().await?;

    // Branch on whether this is a commit-aware snapshot or a copy of the
    // live `graph.bin` (legacy behaviour).
    if let Some(sha) = commit_sha.as_ref().filter(|s| !s.trim().is_empty()) {
        let repo_path = state.active_repo_path().await?;
        return create_snapshot_at_commit(&storage, &repo_path, sha, label).await;
    }

    let live_path = PathBuf::from(&storage).join("graph.bin");
    if !live_path.exists() {
        return Err("No graph.bin in storage — analyze the repo first".into());
    }

    let now_ms = chrono::Utc::now().timestamp_millis();
    let id = format!("snap_{}", now_ms);
    let dest = snapshot_file_path(&storage, &id);
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    std::fs::copy(&live_path, &dest).map_err(|e| e.to_string())?;

    let size = std::fs::metadata(&dest).map(|m| m.len()).unwrap_or(0);

    // Quickly read counts from the snapshot for the meta entry.
    let (node_count, edge_count) = match code_explorer_db::snapshot::load_snapshot(&dest) {
        Ok(g) => (
            g.iter_nodes().count() as u32,
            g.iter_relationships().count() as u32,
        ),
        Err(_) => (0, 0),
    };

    let meta = SnapshotMeta {
        id: id.clone(),
        label: label.unwrap_or_else(|| "Manual snapshot".into()),
        created_at: now_ms,
        node_count,
        edge_count,
        size_bytes: size,
        commit_sha: None,
        authored_at: None,
        subject: None,
    };

    write_snapshot_metadata(&storage, &meta)?;

    let mut idx = load_index(&storage);
    idx.snapshots.push(meta.clone());
    enforce_cap_and_save(&storage, &mut idx)?;
    Ok(meta)
}

/// Helper: enforce MAX_SNAPSHOTS, sort, persist.
fn enforce_cap_and_save(storage: &str, idx: &mut SnapshotIndex) -> Result<(), String> {
    if idx.snapshots.len() > MAX_SNAPSHOTS {
        idx.snapshots
            .sort_by(|a, b| a.created_at.cmp(&b.created_at));
        let drop = idx.snapshots.len() - MAX_SNAPSHOTS;
        for evicted in idx.snapshots.drain(0..drop) {
            let _ = std::fs::remove_file(snapshot_file_path(storage, &evicted.id));
            let _ = std::fs::remove_file(snapshot_meta_path(storage, &evicted.id));
        }
    }
    idx.snapshots
        .sort_by(|a, b| b.created_at.cmp(&a.created_at));
    save_index(storage, idx)
}

/// Theme C — re-ingest the repository at `commit_sha` via a temporary worktree
/// and persist the resulting graph as a snapshot tagged with commit metadata.
async fn create_snapshot_at_commit(
    storage: &str,
    repo_path: &str,
    commit_sha: &str,
    label: Option<String>,
) -> Result<SnapshotMeta, String> {
    let repo_path = std::fs::canonicalize(repo_path)
        .map_err(|e| format!("repo path '{repo_path}' not accessible: {e}"))?;

    // Resolve the SHA to a full hash + grab metadata before touching the worktree.
    let resolved_sha = git_resolve_sha(&repo_path, commit_sha)?;
    let (authored_at, subject) = git_commit_metadata(&repo_path, &resolved_sha);

    // Stable, collision-resistant temp dir under the user's storage dir so
    // `git worktree add` doesn't write into a system location the user can't
    // see and we can clean up cleanly even if the process is killed.
    let now_ms = chrono::Utc::now().timestamp_millis();
    let short_sha = &resolved_sha[..resolved_sha.len().min(12)];
    let id = format!("snap_{now_ms}_{short_sha}");
    let worktree_dir = PathBuf::from(storage)
        .join("snapshots")
        .join(format!(".worktree_{id}"));
    if let Some(parent) = worktree_dir.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }

    // `git worktree add --detach <dir> <sha>` keeps HEAD detached so we don't
    // create a stray local branch. `--force` lets us reuse a stale dir if a
    // previous run was interrupted.
    let add_status = Command::new("git")
        .current_dir(&repo_path)
        .args([
            "worktree",
            "add",
            "--detach",
            "--force",
            worktree_dir.to_string_lossy().as_ref(),
            &resolved_sha,
        ])
        .output()
        .map_err(|e| format!("Failed to spawn `git worktree add`: {e}"))?;

    if !add_status.status.success() {
        let stderr = String::from_utf8_lossy(&add_status.stderr);
        return Err(format!(
            "git worktree add failed for {resolved_sha}: {}",
            stderr.trim()
        ));
    }

    // Run the ingestion pipeline against the temporary worktree.
    let pipeline_result = {
        use code_explorer_ingest::pipeline::{run_pipeline, PipelineOptions};
        let options = PipelineOptions {
            force: true,
            embeddings: false,
            verbose: false,
            skip_git: true,
            ..Default::default()
        };
        // Pass `None` progress channel — UI doesn't need streaming events for
        // the snapshot path; the indexing of a single commit is short and we
        // don't want to spam the existing pipeline-progress channel.
        run_pipeline(&worktree_dir, None, options).await
    };

    // Best-effort worktree cleanup, regardless of pipeline outcome. Two-step:
    // `worktree remove` for the registration; `remove_dir_all` as a belt-and-
    // braces guarantee on Windows where stale `.git/worktrees/<id>/` entries
    // can otherwise linger.
    let _ = Command::new("git")
        .current_dir(&repo_path)
        .args([
            "worktree",
            "remove",
            "--force",
            worktree_dir.to_string_lossy().as_ref(),
        ])
        .output();
    let _ = std::fs::remove_dir_all(&worktree_dir);

    let result = pipeline_result.map_err(|e| format!("Pipeline failed at {resolved_sha}: {e}"))?;
    let graph = result.graph;
    let node_count = graph.node_count() as u32;
    let edge_count = graph.relationship_count() as u32;

    // Persist the snapshot directly via code-explorer-db so we don't need to round-
    // trip through `live` graph.bin.
    let dest = snapshot_file_path(storage, &id);
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    code_explorer_db::snapshot::save_snapshot(&graph, &dest)
        .map_err(|e| format!("Failed to write snapshot file: {e}"))?;
    let size = std::fs::metadata(&dest).map(|m| m.len()).unwrap_or(0);

    let meta = SnapshotMeta {
        id: id.clone(),
        label: label.unwrap_or_else(|| format!("Commit {short_sha}")),
        created_at: now_ms,
        node_count,
        edge_count,
        size_bytes: size,
        commit_sha: Some(resolved_sha),
        authored_at,
        subject,
    };
    write_snapshot_metadata(storage, &meta)?;

    let mut idx = load_index(storage);
    idx.snapshots.push(meta.clone());
    enforce_cap_and_save(storage, &mut idx)?;
    Ok(meta)
}

#[tauri::command]
pub async fn snapshot_list(state: State<'_, AppState>) -> Result<Vec<SnapshotMeta>, String> {
    let storage = state.active_storage_path().await?;
    let mut snapshots = load_index(&storage).snapshots;
    // Rehydrate commit-aware fields from the per-snapshot sidecar files for
    // any entries that were captured before the index started persisting them
    // inline. The index is the source of truth for the list of snapshot IDs;
    // sidecars only enrich missing fields.
    for snap in &mut snapshots {
        if snap.commit_sha.is_some() && snap.subject.is_some() {
            continue;
        }
        let meta_path = snapshot_meta_path(&storage, &snap.id);
        if let Ok(s) = std::fs::read_to_string(&meta_path) {
            if let Ok(parsed) = serde_json::from_str::<SnapshotMeta>(&s) {
                if snap.commit_sha.is_none() {
                    snap.commit_sha = parsed.commit_sha;
                }
                if snap.authored_at.is_none() {
                    snap.authored_at = parsed.authored_at;
                }
                if snap.subject.is_none() {
                    snap.subject = parsed.subject;
                }
            }
        }
    }
    Ok(snapshots)
}

#[tauri::command]
pub async fn snapshot_delete(
    state: State<'_, AppState>,
    id: String,
) -> Result<Vec<SnapshotMeta>, String> {
    let storage = state.active_storage_path().await?;
    let mut idx = load_index(&storage);
    idx.snapshots.retain(|s| s.id != id);
    save_index(&storage, &idx)?;
    let _ = std::fs::remove_file(snapshot_file_path(&storage, &id));
    let _ = std::fs::remove_file(snapshot_meta_path(&storage, &id));
    Ok(idx.snapshots)
}

// ─── Diff ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotDiffRequest {
    /// "live" means the current graph.bin; otherwise a snapshot id.
    pub from: String,
    pub to: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotDiff {
    pub from_id: String,
    pub to_id: String,
    pub from_node_count: u32,
    pub to_node_count: u32,
    pub from_edge_count: u32,
    pub to_edge_count: u32,
    /// Per-label node deltas (added, removed).
    pub by_label: Vec<LabelDelta>,
    pub added_sample: Vec<DiffNode>,
    pub removed_sample: Vec<DiffNode>,
    pub modified_sample: Vec<ModifiedNode>,
    pub total_added: u32,
    pub total_removed: u32,
    pub total_modified: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LabelDelta {
    pub label: String,
    pub from_count: u32,
    pub to_count: u32,
    pub added: u32,
    pub removed: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffNode {
    pub id: String,
    pub name: String,
    pub label: String,
    pub file_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModifiedNode {
    pub id: String,
    pub name: String,
    pub label: String,
    pub file_path: String,
    /// Notable property changes (e.g. is_dead_candidate flipped).
    pub changes: Vec<String>,
}

#[tauri::command]
pub async fn snapshot_diff(
    state: State<'_, AppState>,
    request: SnapshotDiffRequest,
) -> Result<SnapshotDiff, String> {
    let storage = state.active_storage_path().await?;
    let from_path = resolve_snapshot_path(&storage, &request.from)?;
    let to_path = resolve_snapshot_path(&storage, &request.to)?;

    let from = code_explorer_db::snapshot::load_snapshot(&from_path)
        .map_err(|e| format!("Failed to load 'from' snapshot: {e}"))?;
    let to = code_explorer_db::snapshot::load_snapshot(&to_path)
        .map_err(|e| format!("Failed to load 'to' snapshot: {e}"))?;

    let from_ids: HashSet<String> = from.iter_nodes().map(|n| n.id.clone()).collect();
    let to_ids: HashSet<String> = to.iter_nodes().map(|n| n.id.clone()).collect();

    let mut added: Vec<DiffNode> = Vec::new();
    let mut removed: Vec<DiffNode> = Vec::new();
    let mut modified: Vec<ModifiedNode> = Vec::new();
    let mut by_label_from: std::collections::HashMap<String, u32> =
        std::collections::HashMap::new();
    let mut by_label_to: std::collections::HashMap<String, u32> = std::collections::HashMap::new();
    let mut by_label_added: std::collections::HashMap<String, u32> =
        std::collections::HashMap::new();
    let mut by_label_removed: std::collections::HashMap<String, u32> =
        std::collections::HashMap::new();

    for n in to.iter_nodes() {
        let label = n.label.as_str().to_string();
        *by_label_to.entry(label.clone()).or_insert(0) += 1;
        if !from_ids.contains(&n.id) {
            *by_label_added.entry(label.clone()).or_insert(0) += 1;
            added.push(DiffNode {
                id: n.id.clone(),
                name: n.properties.name.clone(),
                label,
                file_path: n.properties.file_path.clone(),
            });
        }
    }
    for n in from.iter_nodes() {
        let label = n.label.as_str().to_string();
        *by_label_from.entry(label.clone()).or_insert(0) += 1;
        if !to_ids.contains(&n.id) {
            *by_label_removed.entry(label.clone()).or_insert(0) += 1;
            removed.push(DiffNode {
                id: n.id.clone(),
                name: n.properties.name.clone(),
                label,
                file_path: n.properties.file_path.clone(),
            });
        }
    }

    // Detect "modified" nodes: same id, but key boolean flags flipped.
    for n_from in from.iter_nodes() {
        let Some(n_to) = to.get_node(&n_from.id) else {
            continue;
        };
        let mut changes: Vec<String> = Vec::new();
        let pf = &n_from.properties;
        let pt = &n_to.properties;
        if pf.is_dead_candidate.unwrap_or(false) != pt.is_dead_candidate.unwrap_or(false) {
            changes.push(format!(
                "isDeadCandidate: {} → {}",
                pf.is_dead_candidate.unwrap_or(false),
                pt.is_dead_candidate.unwrap_or(false)
            ));
        }
        if pf.is_traced.unwrap_or(false) != pt.is_traced.unwrap_or(false) {
            changes.push(format!(
                "isTraced: {} → {}",
                pf.is_traced.unwrap_or(false),
                pt.is_traced.unwrap_or(false)
            ));
        }
        if pf.complexity != pt.complexity {
            changes.push(format!(
                "complexity: {:?} → {:?}",
                pf.complexity, pt.complexity
            ));
        }
        if pf.entry_point_score != pt.entry_point_score {
            changes.push(format!(
                "entryPointScore: {:?} → {:?}",
                pf.entry_point_score, pt.entry_point_score
            ));
        }
        if !changes.is_empty() {
            modified.push(ModifiedNode {
                id: n_from.id.clone(),
                name: n_to.properties.name.clone(),
                label: n_to.label.as_str().to_string(),
                file_path: n_to.properties.file_path.clone(),
                changes,
            });
        }
    }

    let total_added = added.len() as u32;
    let total_removed = removed.len() as u32;
    let total_modified = modified.len() as u32;

    // Sample top-25 by category to keep payload bounded.
    added.truncate(25);
    removed.truncate(25);
    modified.truncate(25);

    // Build by_label deltas (only labels that appear anywhere).
    let mut all_labels: HashSet<String> = HashSet::new();
    all_labels.extend(by_label_from.keys().cloned());
    all_labels.extend(by_label_to.keys().cloned());
    let mut by_label: Vec<LabelDelta> = all_labels
        .into_iter()
        .map(|label| LabelDelta {
            from_count: *by_label_from.get(&label).unwrap_or(&0),
            to_count: *by_label_to.get(&label).unwrap_or(&0),
            added: *by_label_added.get(&label).unwrap_or(&0),
            removed: *by_label_removed.get(&label).unwrap_or(&0),
            label,
        })
        .filter(|d| d.from_count > 0 || d.to_count > 0)
        .collect();
    by_label.sort_by(|a, b| (b.added + b.removed).cmp(&(a.added + a.removed)));

    Ok(SnapshotDiff {
        from_id: request.from.clone(),
        to_id: request.to.clone(),
        from_node_count: from.iter_nodes().count() as u32,
        to_node_count: to.iter_nodes().count() as u32,
        from_edge_count: from.iter_relationships().count() as u32,
        to_edge_count: to.iter_relationships().count() as u32,
        by_label,
        added_sample: added,
        removed_sample: removed,
        modified_sample: modified,
        total_added,
        total_removed,
        total_modified,
    })
}

// ─── Theme C — Commit timeline ─────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommitInfo {
    pub sha: String,
    pub short_sha: String,
    pub author: String,
    pub authored_at: i64,
    pub subject: String,
}

/// List recent commits on the current branch of the active repo. Used by the
/// SnapshotsPanel to populate the "Snapshot at commit" timeline.
#[tauri::command]
pub async fn snapshot_list_commits(
    state: State<'_, AppState>,
    limit: Option<u32>,
) -> Result<Vec<CommitInfo>, String> {
    let repo_path = state.active_repo_path().await?;
    let repo_path = std::fs::canonicalize(&repo_path)
        .map_err(|e| format!("repo path '{repo_path}' not accessible: {e}"))?;
    let n = limit.unwrap_or(50).min(500);

    // Use a record separator that's unlikely to appear in commit messages.
    // %x1f = ASCII unit-separator, %x1e = record-separator.
    let format = "--pretty=format:%H%x1f%h%x1f%an%x1f%at%x1f%s%x1e";
    let output = Command::new("git")
        .current_dir(&repo_path)
        .args(["log", &format!("-n{n}"), format])
        .output()
        .map_err(|e| format!("Failed to run `git log`: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("git log failed: {}", stderr.trim()));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut commits = Vec::new();
    for raw in stdout.split('\u{1e}') {
        let raw = raw.trim_matches(|c| c == '\n' || c == '\r');
        if raw.is_empty() {
            continue;
        }
        let parts: Vec<&str> = raw.splitn(5, '\u{1f}').collect();
        if parts.len() < 5 {
            continue;
        }
        let authored_at = parts[3].parse::<i64>().unwrap_or(0).saturating_mul(1000);
        commits.push(CommitInfo {
            sha: parts[0].to_string(),
            short_sha: parts[1].to_string(),
            author: parts[2].to_string(),
            authored_at,
            subject: parts[4].to_string(),
        });
    }
    Ok(commits)
}

fn git_resolve_sha(repo_path: &Path, sha: &str) -> Result<String, String> {
    let output = Command::new("git")
        .current_dir(repo_path)
        .args(["rev-parse", "--verify", &format!("{sha}^{{commit}}")])
        .output()
        .map_err(|e| format!("Failed to spawn `git rev-parse`: {e}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("Unknown commit '{sha}': {}", stderr.trim()));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// Returns `(authored_at_ms, subject)` for the given commit, or `(None, None)`
/// if any field cannot be retrieved.
fn git_commit_metadata(repo_path: &Path, sha: &str) -> (Option<i64>, Option<String>) {
    let output = Command::new("git")
        .current_dir(repo_path)
        .args(["log", "-n1", "--pretty=format:%at\u{1f}%s", sha])
        .output();
    let Ok(out) = output else {
        return (None, None);
    };
    if !out.status.success() {
        return (None, None);
    }
    let s = String::from_utf8_lossy(&out.stdout);
    let parts: Vec<&str> = s.splitn(2, '\u{1f}').collect();
    let authored_at = parts
        .first()
        .and_then(|t| t.trim().parse::<i64>().ok())
        .map(|secs| secs.saturating_mul(1000));
    let subject = parts.get(1).map(|s| s.trim().to_string());
    (authored_at, subject)
}

fn resolve_snapshot_path(storage: &str, id: &str) -> Result<PathBuf, String> {
    if id == "live" || id == "current" {
        let p = PathBuf::from(storage).join("graph.bin");
        if !p.exists() {
            return Err("Live graph.bin not found".into());
        }
        return Ok(p);
    }
    let p = snapshot_file_path(storage, id);
    if !p.exists() {
        return Err(format!("Snapshot '{id}' not found"));
    }
    Ok(p)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_snapshot_file_path_strips_unsafe_chars() {
        let p = snapshot_file_path("/tmp/store", "../../etc/passwd");
        let last = p.file_name().unwrap().to_string_lossy().to_string();
        assert_eq!(last, "etcpasswd.bin");
    }

    #[test]
    fn test_index_roundtrip() {
        let dir =
            std::env::temp_dir().join(format!("code-explorer-snapshots-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::create_dir_all(dir.join("snapshots")).unwrap();
        let storage = dir.to_string_lossy().to_string();
        let mut idx = SnapshotIndex::default();
        idx.snapshots.push(SnapshotMeta {
            id: "snap_1".into(),
            label: "test".into(),
            created_at: 1,
            node_count: 10,
            edge_count: 20,
            size_bytes: 1234,
            commit_sha: None,
            authored_at: None,
            subject: None,
        });
        save_index(&storage, &idx).unwrap();
        let loaded = load_index(&storage);
        assert_eq!(loaded.snapshots.len(), 1);
        assert_eq!(loaded.snapshots[0].id, "snap_1");
        let _ = std::fs::remove_dir_all(&dir);
    }
}

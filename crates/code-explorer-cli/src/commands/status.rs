//! The `status` command: check Code Explorer index status for the current directory.

use code_explorer_core::storage::{git, repo_manager};
use code_explorer_ingest::manifest::FileChange;

/// Files that differ between the working tree and the last indexed manifest
/// (default exclusions + `.gitignore`). `None` when it cannot be judged.
pub fn working_tree_drift(repo_path: &std::path::Path) -> Option<Vec<FileChange>> {
    let storage = repo_manager::get_storage_paths(repo_path);
    // Replay the walk the last `analyze` did (its --exclude/--include flags,
    // prose or not); an index without the record used the defaults.
    let saved = code_explorer_ingest::manifest::load_settings(&storage.storage_path)
        .unwrap_or_default();
    let rules = super::analyze::WalkOptions {
        exclude: saved.exclude.clone(),
        include: saved.include.clone(),
        no_default_excludes: saved.no_default_excludes,
        max_files: 0,
    }
    .resolve(repo_path);
    code_explorer_ingest::incremental::working_tree_changes(
        repo_path,
        &storage.storage_path,
        &rules,
        saved.documents,
    )
    .ok()
    .flatten()
}

/// `modified: path` lines, at most `max`, with an "... and N more" tail.
pub fn drift_file_lines(changes: &[FileChange], max: usize) -> Vec<String> {
    let mut lines: Vec<String> = changes
        .iter()
        .take(max)
        .map(|c| match c {
            FileChange::Added(p) => format!("added:    {p}"),
            FileChange::Modified(p) => format!("modified: {p}"),
            FileChange::Removed(p) => format!("removed:  {p}"),
        })
        .collect();
    if changes.len() > max {
        lines.push(format!("... and {} more", changes.len() - max));
    }
    lines
}

pub fn run() -> anyhow::Result<()> {
    let cwd = std::env::current_dir()?;
    let storage_paths = repo_manager::get_storage_paths(&cwd);

    println!("Code Explorer Status");
    println!("  Directory: {}", cwd.display());

    // Check if indexed
    if !repo_manager::has_index(&cwd) {
        println!("  Status: NOT INDEXED");
        println!();
        println!("Run `code-explorer analyze` to index this repository.");
        return Ok(());
    }

    // Load and display metadata
    match repo_manager::load_meta(&storage_paths.storage_path)? {
        Some(meta) => {
            println!("  Status: INDEXED");
            println!("  Indexed at: {}", meta.indexed_at);
            println!("  Commit: {}", meta.last_commit);
            println!("  Storage: {}", storage_paths.storage_path.display());

            // Check if index is stale: HEAD moved, or the working tree moved
            // (uncommitted edits are what a developer actually queries).
            let current_commit = git::current_commit(&cwd);
            let mut fresh = true;
            match current_commit {
                Some(ref commit) if commit != &meta.last_commit => {
                    fresh = false;
                    println!();
                    println!("  WARNING: Index is stale!");
                    println!("    Indexed commit: {}", meta.last_commit);
                    println!("    Current commit: {commit}");
                    println!("    Run `code-explorer analyze` to update.");
                }
                None => {
                    println!("  Git: not available or not a git repo");
                }
                _ => {}
            }
            match working_tree_drift(&cwd) {
                Some(changes) if !changes.is_empty() => {
                    fresh = false;
                    println!();
                    println!("  WARNING: Index is stale (uncommitted work not indexed)!");
                    println!("    {}", code_explorer_ingest::incremental::describe_changes(&changes));
                    for line in drift_file_lines(&changes, 10) {
                        println!("      {line}");
                    }
                    println!("    Run `code-explorer analyze` to update (incremental).");
                }
                None => {
                    fresh = false;
                    println!("  Working tree: freshness unknown (no file manifest, re-run `code-explorer analyze --force`)");
                }
                _ => {}
            }
            if fresh {
                println!("  Index is up-to-date.");
            }

            if let Some(stats) = &meta.stats {
                println!();
                println!("  Statistics:");
                if let Some(n) = stats.files {
                    println!("    Files:       {n}");
                }
                if let Some(n) = stats.nodes {
                    println!("    Nodes:       {n}");
                }
                if let Some(n) = stats.edges {
                    println!("    Edges:       {n}");
                }
                if let Some(n) = stats.documents {
                    if n > 0 {
                        println!("    Documents:   {n}");
                    }
                }
                if let Some(n) = stats.communities {
                    println!("    Communities: {n}");
                }
                if let Some(n) = stats.processes {
                    println!("    Processes:   {n}");
                }
                if let Some(n) = stats.embeddings {
                    println!("    Embeddings:  {n}");
                }
                if let Some(ms) = stats.index_duration_ms {
                    println!("    Duration:    {:.2}s", ms as f64 / 1000.0);
                }
            }

            // Detailed per-phase timing breakdown, if metrics.json is present.
            if let Ok(Some(metrics)) = repo_manager::load_metrics(&storage_paths.storage_path) {
                if !metrics.phases.is_empty() {
                    println!();
                    println!("  Phase breakdown:");
                    for pt in &metrics.phases {
                        println!("    {:<18} {:>7} ms", pt.name, pt.duration_ms);
                    }
                    println!(
                        "  Throughput: {:.0} files/s, {:.0} nodes/s",
                        metrics.files_per_sec, metrics.nodes_per_sec
                    );
                }
            }
        }
        None => {
            println!("  Status: INDEX CORRUPTED (meta.json missing)");
            println!("  Run `code-explorer analyze --force` to re-index.");
        }
    }

    Ok(())
}

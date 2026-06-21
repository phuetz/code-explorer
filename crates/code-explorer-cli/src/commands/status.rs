//! The `status` command: check Code Explorer index status for the current directory.

use code_explorer_core::storage::{git, repo_manager};

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

            // Check if index is stale
            let current_commit = git::current_commit(&cwd);
            match current_commit {
                Some(ref commit) if commit != &meta.last_commit => {
                    println!();
                    println!("  WARNING: Index is stale!");
                    println!("    Indexed commit: {}", meta.last_commit);
                    println!("    Current commit: {commit}");
                    println!("    Run `code-explorer analyze` to update.");
                }
                None => {
                    println!("  Git: not available or not a git repo");
                }
                _ => {
                    println!("  Index is up-to-date.");
                }
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

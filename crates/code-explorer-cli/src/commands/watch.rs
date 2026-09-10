//! The `watch` command: monitors a repository for changes and incrementally updates the graph.
//!
//! Uses the incremental engine from [`code_explorer_ingest::incremental`] to
//! efficiently update the knowledge graph when files change, rather than
//! re-indexing the entire repository.

use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::Result;
use colored::Colorize;
use notify_debouncer_mini::{new_debouncer, DebouncedEventKind};

use code_explorer_core::config::languages::SupportedLanguage;
use code_explorer_core::storage::repo_manager;
use code_explorer_db::snapshot;

pub async fn run(path: Option<&str>) -> Result<()> {
    let repo_path = path.map(Path::new).unwrap_or(Path::new("."));
    let repo_path = repo_path.canonicalize()?;

    println!("{}", "Code Explorer Watch Mode".green().bold());
    println!("Watching: {}", repo_path.display());
    println!("Press Ctrl+C to stop\n");

    // Load existing graph or analyze
    let storage = repo_manager::get_storage_paths(&repo_path);
    let snap_path = snapshot::snapshot_path(&storage.storage_path);

    let graph = if snapshot::snapshot_exists(&snap_path) {
        println!("{}", "Loading existing graph...".dimmed());
        Arc::new(Mutex::new(snapshot::load_snapshot(&snap_path)?))
    } else {
        println!(
            "{}",
            "No existing index. Running initial analysis...".yellow()
        );
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        let result =
            code_explorer_ingest::pipeline::run_pipeline(&repo_path, Some(tx), Default::default())
                .await?;

        // Save initial manifest alongside snapshot
        let file_entries = code_explorer_ingest::phases::structure::walk_repository(&repo_path)?;
        let manifest = code_explorer_ingest::manifest::build_manifest_from_entries(&file_entries);
        let manifest_file = code_explorer_ingest::manifest::manifest_path(&storage.storage_path);
        code_explorer_ingest::manifest::save_manifest(&manifest, &manifest_file)?;

        snapshot::save_snapshot(&result.graph, &snap_path)?;
        Arc::new(Mutex::new(result.graph))
    };

    let graph_count = {
        let g = graph.lock().expect("graph mutex poisoned");
        (g.node_count(), g.relationship_count())
    };
    println!(
        "Graph loaded: {} nodes, {} edges",
        graph_count.0, graph_count.1
    );
    println!("{}\n", "Watching for changes...".green());

    // Setup file watcher with 500ms debounce
    let (tx, rx) = std::sync::mpsc::channel();
    let mut debouncer = new_debouncer(Duration::from_millis(500), tx)?;

    debouncer
        .watcher()
        .watch(repo_path.as_ref(), notify::RecursiveMode::Recursive)?;

    // Watch loop
    loop {
        match rx.recv() {
            Ok(Ok(events)) => {
                // Check if any source files changed.
                // `DebouncedEventKind::Any` is emitted for a normal coalesced
                // change. `AnyContinuous` is emitted when the file is under
                // continuous rapid writes (e.g., editor format-on-save loops)
                // and the debounce timeout elapses anyway — we still need to
                // reindex in that case, so match both variants.
                let mut has_source_changes = false;
                for event in &events {
                    if matches!(
                        event.kind,
                        DebouncedEventKind::Any | DebouncedEventKind::AnyContinuous
                    ) {
                        let path = &event.path;
                        if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                            if SupportedLanguage::from_extension(&format!(".{ext}")).is_some() {
                                let rel = path
                                    .strip_prefix(&repo_path)
                                    .unwrap_or(path)
                                    .to_string_lossy()
                                    .replace('\\', "/");
                                // Filter out build artifacts and metadata. The
                                // contains("/target/") check missed `target/` at
                                // the repo root, causing rebuild loops on Rust
                                // projects.
                                let in_target =
                                    rel.starts_with("target/") || rel.contains("/target/");
                                let in_node_modules = rel.starts_with("node_modules/")
                                    || rel.contains("/node_modules/");
                                if !rel.starts_with(".codeexplorer")
                                    && !rel.starts_with(".git/")
                                    && !in_target
                                    && !in_node_modules
                                {
                                    has_source_changes = true;
                                    break;
                                }
                            }
                        }
                    }
                }

                if has_source_changes {
                    println!(
                        "\n{} Change detected, running incremental update...",
                        ">>>".yellow().bold(),
                    );

                    // Use the incremental engine
                    let update_result = {
                        let mut g = match graph.lock() {
                            Ok(g) => g,
                            Err(e) => {
                                eprintln!("    Graph mutex poisoned, recovering");
                                e.into_inner()
                            }
                        };
                        code_explorer_ingest::incremental::incremental_update(
                            &repo_path,
                            &storage.storage_path,
                            &mut g,
                        )
                    };

                    match update_result {
                        Ok(result) => {
                            if result.total_changed() > 0 {
                                println!(
                                    "    {} +{} added, ~{} modified, -{} removed",
                                    "delta".cyan(),
                                    result.added,
                                    result.modified,
                                    result.removed,
                                );
                                println!(
                                    "    {} Removed {} nodes, added {} nodes",
                                    "graph".cyan(),
                                    result.nodes_removed,
                                    result.nodes_added,
                                );

                                // Save updated snapshot FIRST, then the manifest.
                                // `incremental_update` hands the new manifest back
                                // to the caller instead of persisting it itself —
                                // saving the manifest before the snapshot is durable
                                // can silently strand the on-disk graph in a stale
                                // state if the snapshot write fails (the next run
                                // sees "no changes" and loads the old snapshot).
                                let g = match graph.lock() {
                                    Ok(g) => g,
                                    Err(e) => {
                                        eprintln!("    Graph mutex poisoned, recovering");
                                        e.into_inner()
                                    }
                                };
                                let snapshot_saved = match snapshot::save_snapshot(&g, &snap_path) {
                                    Ok(_) => {
                                        println!("    {} Snapshot saved", "OK".green());
                                        true
                                    }
                                    Err(e) => {
                                        eprintln!(
                                            "    {} Failed to save snapshot: {}",
                                            "!".red(),
                                            e
                                        );
                                        false
                                    }
                                };
                                // Only update the manifest once the snapshot is
                                // durably on disk, otherwise the manifest would
                                // claim files are current while the graph still
                                // reflects the pre-update state.
                                if snapshot_saved {
                                    let manifest_file = code_explorer_ingest::manifest::manifest_path(
                                        &storage.storage_path,
                                    );
                                    if let Err(e) = code_explorer_ingest::manifest::save_manifest(
                                        &result.new_manifest,
                                        &manifest_file,
                                    ) {
                                        eprintln!(
                                            "    {} Failed to save manifest: {}",
                                            "!".red(),
                                            e
                                        );
                                    }
                                }
                                println!(
                                    "    {} Graph: {} nodes, {} edges total",
                                    "=".cyan(),
                                    g.node_count(),
                                    g.relationship_count()
                                );
                            } else {
                                println!("    {} No effective changes detected", "=".dimmed(),);
                            }
                        }
                        Err(e) => {
                            eprintln!("    {} Incremental update failed: {}", "!".red(), e);
                        }
                    }

                    println!("{}", "\nWatching for changes...".dimmed());
                }
            }
            Ok(Err(e)) => {
                eprintln!("{} Watch error: {}", "!".red(), e);
            }
            Err(e) => {
                eprintln!("{} Channel error: {}", "!".red(), e);
                break;
            }
        }
    }

    Ok(())
}

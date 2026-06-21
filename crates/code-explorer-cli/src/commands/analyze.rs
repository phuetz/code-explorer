//! The `analyze` command: indexes a repository into a knowledge graph.

use std::path::Path;

use indicatif::{ProgressBar, ProgressStyle};

use code_explorer_core::storage::{git, repo_manager};

#[allow(clippy::too_many_arguments)]
pub async fn run(
    path: &str,
    force: bool,
    embeddings: bool,
    verbose: bool,
    skip_git: bool,
    incremental: bool,
    llm_enrich: bool,
    llm_token_budget: Option<u64>,
    llm_max_symbols: Option<usize>,
) -> anyhow::Result<()> {
    let repo_path = Path::new(path)
        .canonicalize()
        .unwrap_or_else(|_| Path::new(path).to_path_buf());

    println!("Indexing repository: {}", repo_path.display());

    // Check if already indexed
    if !force && repo_manager::has_index(&repo_path) {
        println!("Repository already indexed. Use --force to re-index.");
        return Ok(());
    }

    // Create progress bar
    let style = ProgressStyle::default_bar()
        .template("{spinner:.green} [{elapsed_precise}] {msg}")
        .unwrap();
    let pb = ProgressBar::new_spinner();
    pb.set_style(style);
    pb.enable_steady_tick(std::time::Duration::from_millis(100));

    // Create progress channel
    let (tx, mut rx) =
        tokio::sync::mpsc::unbounded_channel::<code_explorer_core::pipeline::PipelineProgress>();

    // Spawn progress handler
    let pb_clone = pb.clone();
    let progress_handle = tokio::spawn(async move {
        while let Some(progress) = rx.recv().await {
            pb_clone.set_message(format!(
                "[{:>12}] {:.0}% {}",
                progress.phase.as_str(),
                progress.percent,
                progress.message
            ));
        }
    });

    // Build LLM enrichment config if requested
    let llm_config = if llm_enrich {
        match super::generate::load_llm_config() {
            Some(cfg) => {
                let mut enrich_cfg = code_explorer_ingest::phases::llm_enrichment::LlmEnrichmentConfig {
                    base_url: cfg.base_url,
                    api_key: cfg.api_key,
                    model: cfg.model,
                    max_tokens: cfg.max_tokens,
                    reasoning_effort: cfg.reasoning_effort,
                    ..Default::default()
                };
                if let Some(budget) = llm_token_budget {
                    enrich_cfg.token_budget = budget;
                }
                if let Some(max) = llm_max_symbols {
                    enrich_cfg.max_symbols = max;
                }
                Some(enrich_cfg)
            }
            None => {
                println!("Warning: --llm-enrich requires ~/.codeexplorer/chat-config.json");
                println!("  Skipping LLM enrichment phase.");
                None
            }
        }
    } else {
        None
    };

    // Run pipeline
    let options = code_explorer_ingest::pipeline::PipelineOptions {
        force,
        embeddings,
        verbose,
        skip_git,
        incremental,
        llm_enrich: llm_config,
    };

    let result = code_explorer_ingest::pipeline::run_pipeline(&repo_path, Some(tx), options).await;

    // Wait for progress handler to finish
    let _ = progress_handle.await;
    pb.finish_and_clear();

    match result {
        Ok(result) => {
            println!("\nIndexing complete!");
            println!("  Files:       {}", result.total_file_count);
            println!("  Nodes:       {}", result.graph.node_count());
            println!("  Edges:       {}", result.graph.relationship_count());
            println!("  Communities: {}", result.community_count);
            println!("  Processes:   {}", result.process_count);
            println!(
                "  Duration:    {:.2}s ({} ms)",
                result.total_duration_ms as f64 / 1000.0,
                result.total_duration_ms
            );
            if !result.phase_timings.is_empty() {
                println!("  Phase breakdown:");
                for pt in &result.phase_timings {
                    println!("    {:<18} {:>7} ms", pt.name, pt.duration_ms);
                }
            }

            // Save metadata
            let commit = git::current_commit(&repo_path).unwrap_or_else(|| "unknown".to_string());
            let meta = repo_manager::RepoMeta {
                repo_path: repo_path.display().to_string(),
                last_commit: commit,
                indexed_at: chrono_now(),
                stats: Some(repo_manager::RepoStats {
                    files: Some(result.total_file_count),
                    nodes: Some(result.graph.node_count()),
                    edges: Some(result.graph.relationship_count()),
                    communities: Some(result.community_count),
                    processes: Some(result.process_count),
                    embeddings: None,
                    index_duration_ms: Some(result.total_duration_ms),
                }),
            };

            let storage_paths = repo_manager::get_storage_paths(&repo_path);
            repo_manager::save_meta(&storage_paths.storage_path, &meta)?;
            repo_manager::register_repo(&repo_path, &meta)?;

            // Persist the detailed performance metrics (per-phase breakdown + throughput).
            {
                let secs = result.total_duration_ms as f64 / 1000.0;
                let nodes = result.graph.node_count();
                let edges = result.graph.relationship_count();
                let metrics = code_explorer_core::pipeline::types::IndexMetrics {
                    schema_version: 1,
                    tool_version: env!("CARGO_PKG_VERSION").to_string(),
                    indexed_at: meta.indexed_at.clone(),
                    total_duration_ms: result.total_duration_ms,
                    phases: result.phase_timings.clone(),
                    files: result.total_file_count,
                    nodes,
                    edges,
                    communities: result.community_count,
                    processes: result.process_count,
                    files_per_sec: if secs > 0.0 {
                        result.total_file_count as f64 / secs
                    } else {
                        0.0
                    },
                    nodes_per_sec: if secs > 0.0 { nodes as f64 / secs } else { 0.0 },
                };
                repo_manager::save_metrics(&storage_paths.storage_path, &metrics)?;
            }

            // Save binary snapshot for fast reload (REPL, MCP, CLI queries)
            let snap_path = code_explorer_db::snapshot::snapshot_path(&storage_paths.storage_path);
            code_explorer_db::snapshot::save_snapshot(&result.graph, &snap_path)?;
            println!(
                "  Graph snapshot saved ({} bytes)",
                std::fs::metadata(&snap_path).map(|m| m.len()).unwrap_or(0)
            );

            // Save file manifest for incremental indexing
            {
                let file_entries = code_explorer_ingest::phases::structure::walk_repository(&repo_path)?;
                let manifest =
                    code_explorer_ingest::manifest::build_manifest_from_entries(&file_entries);
                let manifest_file =
                    code_explorer_ingest::manifest::manifest_path(&storage_paths.storage_path);
                code_explorer_ingest::manifest::save_manifest(&manifest, &manifest_file)?;
                println!("  File manifest saved ({} files)", manifest.files.len());
            }

            // Generate CSV and save
            println!("  Saving CSVs...");
            let csv_dir = storage_paths.storage_path.join("csv");
            std::fs::create_dir_all(&csv_dir)?;
            code_explorer_db::csv_generator::generate_all_csvs(&result.graph, &repo_path, &csv_dir)?;

            // Load CSVs into KuzuDB (when the kuzu-backend feature is enabled)
            #[cfg(feature = "kuzu-backend")]
            {
                println!("  Loading into KuzuDB...");
                let mut db = code_explorer_db::adapter::DbAdapter::new_kuzu();
                db.open(&storage_paths.lbug_path)?;
                db.create_schema()?;
                db.bulk_load_csv(&csv_dir)?;
                db.close()?;
                println!("  KuzuDB loaded successfully.");
            }

            println!("  Done! Run 'code-explorer mcp' to start the MCP server.");
            Ok(())
        }
        Err(e) => {
            eprintln!("Pipeline failed: {e}");
            Err(e.into())
        }
    }
}

fn chrono_now() -> String {
    // Produce a proper RFC 3339 / ISO 8601 timestamp like "2026-04-06T08:30:00Z".
    // The previous "Unix epoch + Z" output is not a valid date string and
    // breaks consumers (desktop registry display, JSON-driven tooling).
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

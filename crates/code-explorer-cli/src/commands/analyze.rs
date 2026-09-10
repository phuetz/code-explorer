//! The `analyze` command: indexes a repository into a knowledge graph.

use std::path::Path;

use indicatif::{ProgressBar, ProgressStyle};

use code_explorer_core::config::exclusions::ExclusionRules;
use code_explorer_core::storage::{git, repo_manager};

/// What the caller asked to keep out of (or back into) the walk.
#[derive(Debug, Default, Clone)]
pub struct WalkOptions {
    /// `--exclude` patterns.
    pub exclude: Vec<String>,
    /// `--include` patterns; they win over every exclusion.
    pub include: Vec<String>,
    /// `--no-default-excludes`: drop the built-in list.
    pub no_default_excludes: bool,
    /// `--max-files`: refuse a repository bigger than this. 0 disables.
    pub max_files: usize,
}

impl WalkOptions {
    /// Merge the flags with what the repository itself declares.
    pub fn resolve(&self, repo_path: &Path) -> ExclusionRules {
        let mut rules = if self.no_default_excludes {
            ExclusionRules::none()
        } else {
            ExclusionRules::for_repo(repo_path)
        };
        rules.extend(&self.exclude, &self.include);
        rules
    }
}

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
    include_docs: Option<bool>,
    walk: WalkOptions,
) -> anyhow::Result<()> {
    let repo_path = Path::new(path)
        .canonicalize()
        .unwrap_or_else(|_| Path::new(path).to_path_buf());

    println!("Indexing repository: {}", repo_path.display());

    let exclusions = walk.resolve(&repo_path);
    if exclusions.is_empty() {
        println!("Exclusions:  none (--no-default-excludes)");
    } else {
        println!("Exclusions:  {}", describe(&exclusions));
    }

    // Say how big the job is *before* starting it, and refuse a job that has
    // no business running: an unbounded `analyze` that never returns teaches
    // the user nothing, and costs them an afternoon.
    let scan_start = std::time::Instant::now();
    let scan = code_explorer_ingest::phases::structure::scan_candidates(&repo_path, &exclusions)?;
    println!(
        "Candidates:  {} parseable of {} files walked ({:.2}s)",
        scan.candidates,
        scan.walked,
        scan_start.elapsed().as_secs_f64()
    );
    if walk.max_files > 0 && scan.candidates > walk.max_files {
        eprint!("{}", over_budget_message(&repo_path, &scan, walk.max_files));
        anyhow::bail!(
            "{} candidate files exceeds --max-files {}",
            scan.candidates,
            walk.max_files
        );
    }

    // Check if already indexed
    if !force && !incremental && repo_manager::has_index(&repo_path) {
        println!("Repository already indexed. Use --force to re-index.");
        return Ok(());
    }

    if incremental && !force {
        println!("Incremental parsing; repository-wide resolution and enrichment.");
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
        include_docs,
        exclusions: Some(exclusions.clone()),
    };

    let result = code_explorer_ingest::pipeline::run_pipeline(&repo_path, Some(tx), options).await;

    // Wait for progress handler to finish
    let _ = progress_handle.await;
    pb.finish_and_clear();

    match result {
        Ok(result) => {
            println!("\nIndexing complete!");
            println!("  Files:       {}", result.total_file_count);
            println!("  Parsed:      {}", result.parsed_files);
            println!("  Nodes:       {}", result.graph.node_count());
            println!("  Edges:       {}", result.graph.relationship_count());
            if result.doc_stats.documents > 0 {
                println!(
                    "  Documents:   {} ({} headings, {} internal links)",
                    result.doc_stats.documents,
                    result.doc_stats.headings,
                    result.doc_stats.links_resolved
                );
            }
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
                schema_version: None,
                stats: Some(repo_manager::RepoStats {
                    files: Some(result.total_file_count),
                    nodes: Some(result.graph.node_count()),
                    edges: Some(result.graph.relationship_count()),
                    communities: Some(result.community_count),
                    processes: Some(result.process_count),
                    embeddings: None,
                    documents: Some(result.doc_stats.documents),
                    index_duration_ms: Some(result.total_duration_ms),
                }),
            };

            let storage_paths = repo_manager::get_storage_paths(&repo_path);
            repo_manager::save_meta(&storage_paths.storage_path, &meta)?;
            std::fs::write(storage_paths.storage_path.join("analyze.json"),
                serde_json::to_vec_pretty(&serde_json::json!({
                    "parsed_files": result.parsed_files,
                    "total_files": result.total_file_count,
                    "duration_ms": result.total_duration_ms,
                    "fallback_reason": result.incremental_fallback,
                    "resolution_scope": "repository",
                    "local_enrichments": result.local_enrichments
                }))?)?;
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
                let file_entries = code_explorer_ingest::phases::structure::walk_repository_with(
                    &repo_path,
                    &exclusions,
                )?;
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

/// The refusal: what was counted, which directories carry it, and the exact
/// command that makes the job small enough. Pure, so a test can read it.
fn over_budget_message(
    repo_path: &Path,
    scan: &code_explorer_ingest::phases::structure::CandidateScan,
    max_files: usize,
) -> String {
    use std::fmt::Write;

    let mut out = String::new();
    let _ = writeln!(
        out,
        "\nRefusing to index {} candidate files: --max-files is {}.",
        scan.candidates, max_files
    );
    let largest = scan.largest(5);
    if !largest.is_empty() {
        let _ = writeln!(out, "\nLargest directories after exclusions:");
        for dir in largest {
            let _ = writeln!(
                out,
                "  {:<28} {:>7} parseable  {:>7} files  {:>9}",
                dir.path,
                dir.candidates,
                dir.walked,
                dir.human_bytes()
            );
        }
    }
    // A directory holding most of the repository is the repository. Telling
    // someone to exclude their own `src/` is worse than saying nothing: the
    // honest answer there is that the ceiling, not the tree, is what is wrong.
    let dominant = largest
        .first()
        .filter(|d| d.path != "." && d.candidates * 2 > scan.candidates);
    if let Some(dir) = dominant {
        let _ = writeln!(
            out,
            "\n'{}' alone holds {} of the {} candidates: that is this repository's own\nsource, not something to exclude.",
            dir.path, dir.candidates, scan.candidates
        );
    }

    let suggestion: Vec<String> = largest
        .iter()
        .filter(|d| {
            d.path != "."
                && d.candidates > 0
                && dominant.map_or(true, |dom| dom.path != d.path)
        })
        .take(3)
        .map(|d| format!("--exclude {}", d.path))
        .collect();
    if !suggestion.is_empty() {
        let _ = writeln!(out, "\nDrop what you do not need indexed:");
        let _ = writeln!(
            out,
            "  code-explorer analyze {} {}",
            repo_path.display(),
            suggestion.join(" ")
        );
    } else if dominant.is_none() {
        let _ = writeln!(out, "\nDrop what you do not need indexed:");
        let _ = writeln!(
            out,
            "  code-explorer analyze {} --exclude <directory>",
            repo_path.display()
        );
    }
    let _ = writeln!(
        out,
        "\n{}:\n  code-explorer analyze {} --max-files {}",
        if dominant.is_some() {
            "Raise the ceiling — this repository really is that big"
        } else {
            "Or raise the ceiling deliberately"
        },
        repo_path.display(),
        scan.candidates.next_multiple_of(1000)
    );
    let _ = writeln!(out, "  (--max-files 0 removes the guard entirely.)");
    out
}

/// One line naming the rules in effect, truncated so it stays readable.
fn describe(rules: &ExclusionRules) -> String {
    const SHOWN: usize = 8;
    let mut parts: Vec<String> = rules.patterns().iter().take(SHOWN).cloned().collect();
    if rules.patterns().len() > SHOWN {
        parts.push(format!("+{} more", rules.patterns().len() - SHOWN));
    }
    let mut line = parts.join(", ");
    if !rules.includes().is_empty() {
        line.push_str(&format!(" (kept: {})", rules.includes().join(", ")));
    }
    line
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_on_without_flags() {
        let rules = WalkOptions::default().resolve(Path::new("/nonexistent-repo"));
        assert!(rules.is_excluded("node_modules/react/index.js"));
        assert!(rules.is_excluded("_archive/old.ts"));
        assert!(!rules.is_excluded("src/app.ts"));
    }

    #[test]
    fn no_default_excludes_keeps_everything_but_explicit_flags() {
        let opts = WalkOptions {
            exclude: vec!["vendor".into()],
            no_default_excludes: true,
            ..Default::default()
        };
        let rules = opts.resolve(Path::new("/nonexistent-repo"));
        assert!(!rules.is_excluded("node_modules/react/index.js"));
        assert!(rules.is_excluded("vendor/lib.php"));
    }

    #[test]
    fn extra_excludes_stack_on_the_defaults() {
        let opts = WalkOptions {
            exclude: vec!["vendor".into(), "fixtures".into()],
            ..Default::default()
        };
        let rules = opts.resolve(Path::new("/nonexistent-repo"));
        assert!(rules.is_excluded("node_modules/react/index.js"));
        assert!(rules.is_excluded("vendor/lib.php"));
        assert!(rules.is_excluded("tests/fixtures/big.ts"));
    }

    #[test]
    fn include_reopens_a_default_exclusion() {
        let opts = WalkOptions {
            include: vec!["build".into()],
            ..Default::default()
        };
        let rules = opts.resolve(Path::new("/nonexistent-repo"));
        assert!(!rules.is_excluded("build/generated/app.ts"));
        assert!(rules.is_excluded("node_modules/react/index.js"));
    }

    #[test]
    fn over_budget_message_names_the_cost_and_the_cure() {
        use code_explorer_ingest::phases::structure::{CandidateScan, DirTally};
        let scan = CandidateScan {
            candidates: 61_234,
            walked: 70_000,
            dirs: vec![
                DirTally { path: "node_modules".into(), candidates: 41_200, walked: 48_000, bytes: 2_200_000_000 },
                DirTally { path: "_archive".into(), candidates: 15_000, walked: 15_500, bytes: 14_000_000 },
                DirTally { path: "src".into(), candidates: 5_000, walked: 6_000, bytes: 30_000_000 },
                DirTally { path: ".".into(), candidates: 34, walked: 500, bytes: 900_000 },
            ],
        };
        let msg = over_budget_message(Path::new("/repo"), &scan, 50_000);

        assert!(msg.contains("Refusing to index 61234 candidate files"));
        assert!(msg.contains("--max-files is 50000"));
        // The five biggest directories, with their weight.
        assert!(msg.contains("node_modules"));
        assert!(msg.contains("2.0 GB"));
        assert!(msg.contains("_archive"));
        // A command that can be pasted, not advice.
        assert!(msg.contains("code-explorer analyze /repo --exclude _archive --exclude src"));
        assert!(msg.contains("--max-files 62000"));
        assert!(msg.contains("--max-files 0"));
    }

    #[test]
    fn a_dominant_directory_is_not_offered_as_a_thing_to_exclude() {
        // The real WorkflowBuilder shape: the weight is the repository's own
        // `src/`. Suggesting `--exclude src` would be worse than useless.
        use code_explorer_ingest::phases::structure::{CandidateScan, DirTally};
        let scan = CandidateScan {
            candidates: 5_525,
            walked: 7_003,
            dirs: vec![
                DirTally { path: "src".into(), candidates: 5_342, walked: 5_394, bytes: 64_000_000 },
                DirTally { path: "e2e".into(), candidates: 82, walked: 106, bytes: 664_000 },
                DirTally { path: "scripts".into(), candidates: 38, walked: 103, bytes: 621_000 },
            ],
        };
        let msg = over_budget_message(Path::new("/repo"), &scan, 1);

        assert!(
            msg.contains("'src' alone holds 5342 of the 5525 candidates"),
            "the message must name the dominant directory:\n{msg}"
        );
        assert!(!msg.contains("--exclude src"), "never suggest excluding it:\n{msg}");
        assert!(msg.contains("--exclude e2e --exclude scripts"));
        assert!(
            msg.contains("Raise the ceiling — this repository really is that big"),
            "the honest cure here is the ceiling:\n{msg}"
        );
        assert!(msg.contains("--max-files 6000"));
    }

    #[test]
    fn over_budget_message_survives_a_repository_with_no_subdirectory() {
        use code_explorer_ingest::phases::structure::CandidateScan;
        let scan = CandidateScan { candidates: 10, walked: 10, dirs: Vec::new() };
        let msg = over_budget_message(Path::new("/repo"), &scan, 5);
        assert!(msg.contains("--exclude <directory>"));
        assert!(!msg.contains("Largest directories"));
    }

    #[test]
    fn describe_truncates_the_long_default_list() {
        let line = describe(&ExclusionRules::with_defaults());
        assert!(line.contains("node_modules"));
        assert!(line.contains("more"));
    }
}

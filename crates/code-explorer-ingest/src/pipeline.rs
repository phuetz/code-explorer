use code_explorer_core::graph::KnowledgeGraph;
use code_explorer_core::pipeline::types::{PhaseTiming, PipelinePhase, PipelineProgress};
use code_explorer_core::symbol::SymbolTable;
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::time::Instant;
use tokio::sync::mpsc;

use crate::incremental;
use crate::phases;

pub type ProgressSender = mpsc::UnboundedSender<PipelineProgress>;

/// Pipeline configuration constants
pub const CHUNK_BYTE_BUDGET: usize = 20 * 1024 * 1024; // 20MB per chunk
pub const AST_CACHE_CAP: usize = 50;
pub const MAX_SYNTHETIC_BINDINGS_PER_FILE: usize = 1000;
pub const MIN_FILES_FOR_WORKERS: usize = 15;
pub const MIN_BYTES_FOR_WORKERS: usize = 512 * 1024;

/// Result of running the full pipeline.
pub struct PipelineResult {
    pub graph: KnowledgeGraph,
    pub repo_path: String,
    pub total_file_count: usize,
    pub community_count: usize,
    pub process_count: usize,
    /// Total wall-clock time of the run, in milliseconds.
    pub total_duration_ms: u64,
    /// Per-phase wall-clock breakdown (empty for incremental runs that skip the full pipeline).
    pub phase_timings: Vec<PhaseTiming>,
}

/// Options for pipeline execution.
#[derive(Debug, Default)]
pub struct PipelineOptions {
    pub force: bool,
    pub embeddings: bool,
    pub verbose: bool,
    pub skip_git: bool,
    /// If true and a manifest exists, use incremental indexing instead of full.
    pub incremental: bool,
    /// If Some, run Phase 8 LLM enrichment with the given config.
    pub llm_enrich: Option<phases::llm_enrichment::LlmEnrichmentConfig>,
}

/// Run the full ingestion pipeline on a repository.
pub async fn run_pipeline(
    repo_path: &Path,
    progress_tx: Option<ProgressSender>,
    options: PipelineOptions,
) -> Result<PipelineResult, crate::IngestError> {
    let pipeline_start = Instant::now();
    let repo_path_str = repo_path.display().to_string();
    let mut phase_timings: Vec<PhaseTiming> = Vec::new();

    // Helper to send progress
    let send_progress = |phase, percent, message: &str| {
        if let Some(tx) = &progress_tx {
            let _ = tx.send(PipelineProgress {
                phase,
                percent,
                message: message.to_string(),
                detail: None,
                stats: None,
            });
        }
    };

    // Incremental mode: if a manifest and graph snapshot exist, only re-parse changed files
    let storage_path = repo_path.join(".codeexplorer");
    let snap_path = storage_path.join("graph.bin");
    let manifest_path = storage_path.join("manifest.json");
    if options.incremental && !options.force && snap_path.exists() && manifest_path.exists() {
        send_progress(PipelinePhase::Structure, 0.0, "Incremental update...");

        let mut graph = code_explorer_db::snapshot::load_snapshot(&snap_path).map_err(|e| {
            crate::IngestError::PhaseError {
                phase: "incremental".into(),
                message: format!("Failed to load snapshot: {e}"),
            }
        })?;

        let inc_result = incremental::incremental_update(repo_path, &storage_path, &mut graph)?;

        if inc_result.total_changed() > 0 {
            // Capture the new manifest now; we will only persist it AFTER
            // the snapshot is durable, so a crash between the two writes
            // cannot leave the manifest ahead of the snapshot.
            let new_manifest_to_save = inc_result.new_manifest.clone();
            // Purge stale Community/Process nodes (and their `MemberOf` /
            // `StepInProcess` edges) from the previous run before re-running
            // detection. Louvain is deterministic in structure only up to a
            // renumbering, so `Community:community_0` from run N and run
            // N+1 may represent different clusters — without this cleanup,
            // the re-run's `add_node` would overwrite the node while
            // leaving stale membership edges pointing at it. The same
            // applies to `Process:process_*` nodes and `StepInProcess`
            // edges. `remove_nodes_by_label` cascades incident edges.
            use code_explorer_core::graph::types::NodeLabel;
            graph.remove_nodes_by_label(NodeLabel::Community);
            graph.remove_nodes_by_label(NodeLabel::Process);

            // Re-run community + process detection on the updated graph
            let community_count = phases::community::detect_communities(&mut graph)?;
            let process_count = phases::process::detect_processes(&mut graph)?;
            phases::dead_code::mark_dead_code(&mut graph);

            // Save updated snapshot to disk FIRST, then the manifest. This
            // ordering matters: if the snapshot save fails, we leave both the
            // old snapshot and old manifest on disk so the next run can simply
            // re-detect the same changes and try again. Persisting the
            // manifest before the snapshot would silently bake in a stale
            // graph (manifest claims everything is current, but the on-disk
            // snapshot doesn't reflect the changes we just applied).
            code_explorer_db::snapshot::save_snapshot(&graph, &snap_path).map_err(|e| {
                crate::IngestError::PhaseError {
                    phase: "incremental".into(),
                    message: format!("Failed to save snapshot: {e}"),
                }
            })?;

            crate::manifest::save_manifest(&new_manifest_to_save, &manifest_path).map_err(|e| {
                crate::IngestError::PhaseError {
                    phase: "incremental".into(),
                    message: format!("Failed to save manifest: {e}"),
                }
            })?;

            tracing::info!(
                added = inc_result.added,
                modified = inc_result.modified,
                removed = inc_result.removed,
                total_duration_ms = pipeline_start.elapsed().as_millis() as u64,
                "Incremental pipeline complete"
            );

            send_progress(
                PipelinePhase::Complete,
                100.0,
                &format!(
                    "Incremental: +{} ~{} -{} files",
                    inc_result.added, inc_result.modified, inc_result.removed,
                ),
            );

            return Ok(PipelineResult {
                graph,
                repo_path: repo_path_str,
                total_file_count: inc_result.unchanged + inc_result.added + inc_result.modified,
                community_count,
                process_count,
                total_duration_ms: pipeline_start.elapsed().as_millis() as u64,
                phase_timings: Vec::new(),
            });
        } else {
            tracing::info!("No changes detected, graph is up to date");
            send_progress(PipelinePhase::Complete, 100.0, "No changes detected");

            let community_count = graph
                .iter_nodes()
                .filter(|n| n.label == code_explorer_core::graph::types::NodeLabel::Community)
                .count();
            let process_count = graph
                .iter_nodes()
                .filter(|n| n.label == code_explorer_core::graph::types::NodeLabel::Process)
                .count();

            return Ok(PipelineResult {
                graph,
                repo_path: repo_path_str,
                total_file_count: inc_result.unchanged,
                community_count,
                process_count,
                total_duration_ms: pipeline_start.elapsed().as_millis() as u64,
                phase_timings: Vec::new(),
            });
        }
    }

    send_progress(PipelinePhase::Structure, 0.0, "Scanning repository...");

    // Phase 1: Structure - walk filesystem
    let phase_start = Instant::now();
    let file_entries = phases::structure::walk_repository(repo_path)?;
    let total_files = file_entries.len();

    let mut graph = KnowledgeGraph::with_capacity(total_files * 5, total_files * 10);

    // Phase 1b: Create File/Folder nodes
    phases::structure::create_structure_nodes(&mut graph, &file_entries);
    let duration = phase_start.elapsed();
    tracing::info!(
        phase = "structure",
        duration_ms = duration.as_millis() as u64,
        files = total_files,
        "Phase complete"
    );
    phase_timings.push(PhaseTiming {
        name: "structure".into(),
        duration_ms: duration.as_millis() as u64,
    });
    send_progress(
        PipelinePhase::Structure,
        100.0,
        &format!("Found {total_files} files"),
    );

    // Phase 2: Parsing - extract symbols from AST
    send_progress(PipelinePhase::Parsing, 0.0, "Parsing files...");
    let phase_start = Instant::now();
    let extracted = phases::parsing::parse_files(&mut graph, &file_entries, progress_tx.as_ref())?;

    // Phase 2b: Detect component libraries from .csproj project files.
    // This runs after parsing to enrich the graph with NuGet package-level detections,
    // which have higher confidence than source-level pattern matching.
    let has_razor_files = file_entries
        .iter()
        .any(|f| f.path.ends_with(".cshtml") || f.path.ends_with(".razor"));
    if has_razor_files {
        phases::parsing::detect_csproj_components(&mut graph, repo_path);
    }

    // Phase 2d: Cross-file Go method nesting. The per-file pass links methods to
    // same-file receiver types; this links the rest to a same-directory (same-package)
    // type, which Go's rules make unambiguous. Cheap regex scan; only floating methods.
    if file_entries.iter().any(|f| f.path.ends_with(".go")) {
        let linked = phases::parsing::reconcile_cross_file_methods(&mut graph, &file_entries);
        tracing::debug!(phase = "go_reconcile", linked, "Cross-file Go methods linked");
    }

    // Phase 2e: C++ out-of-class method nesting (`void User::save()` in a .cpp links to
    // the User class declared in a header). Regex scan; only floating methods, endpoints
    // verified, so no dangling/duplicate edges.
    if file_entries
        .iter()
        .any(|f| matches!(f.path.rsplit_once('.').map(|(_, e)| e), Some("cpp" | "cc" | "cxx" | "hpp" | "hh")))
    {
        let linked = phases::parsing::reconcile_out_of_class_methods(&mut graph, &file_entries);
        tracing::debug!(phase = "cpp_reconcile", linked, "C++ out-of-class methods linked");
    }

    // Build symbol table from graph
    let mut symbol_table = SymbolTable::new();
    phases::parsing::build_symbol_table(&graph, &mut symbol_table);
    let duration = phase_start.elapsed();
    tracing::info!(
        phase = "parsing",
        duration_ms = duration.as_millis() as u64,
        files = total_files,
        symbols = symbol_table.len(),
        "Phase complete"
    );
    phase_timings.push(PhaseTiming {
        name: "parsing".into(),
        duration_ms: duration.as_millis() as u64,
    });
    send_progress(
        PipelinePhase::Parsing,
        100.0,
        &format!("Parsed {total_files} files"),
    );

    // Phase 2c: TODO/FIXME inventory — language-agnostic comment scan.
    // Runs here because we now have the File nodes (from Phase 1b) and the
    // full FileEntry list, but we don't need the symbol graph to be built
    // yet. Fast (parallel via rayon) so the cost stays hidden behind the
    // much slower parsing phase.
    let phase_start = Instant::now();
    let todo_stats = phases::todos::scan_todos(&mut graph, &file_entries);
    let duration = phase_start.elapsed();
    tracing::info!(
        phase = "todos",
        duration_ms = duration.as_millis() as u64,
        markers = todo_stats.markers,
        "Phase complete"
    );
    phase_timings.push(PhaseTiming {
        name: "todos".into(),
        duration_ms: duration.as_millis() as u64,
    });

    // Phase 3: Import resolution
    send_progress(PipelinePhase::Imports, 0.0, "Resolving imports...");
    let phase_start = Instant::now();
    let (import_map, named_import_map, re_export_map, package_map, module_alias_map) =
        phases::imports::resolve_imports(
            &mut graph,
            repo_path,
            &file_entries,
            &extracted,
            &symbol_table,
        )?;
    let duration = phase_start.elapsed();
    tracing::info!(
        phase = "imports",
        duration_ms = duration.as_millis() as u64,
        import_edges = import_map.len(),
        "Phase complete"
    );
    phase_timings.push(PhaseTiming {
        name: "imports".into(),
        duration_ms: duration.as_millis() as u64,
    });
    send_progress(PipelinePhase::Imports, 100.0, "Imports resolved");

    // Phase 4: Call resolution
    send_progress(PipelinePhase::Calls, 0.0, "Resolving calls...");
    let phase_start = Instant::now();
    phases::calls::resolve_calls(
        &mut graph,
        &extracted,
        &symbol_table,
        &import_map,
        &named_import_map,
        &re_export_map,
        &package_map,
        &module_alias_map,
        &file_entries,
    )?;
    // Safety net: re-point any CALLS edge whose source node doesn't exist to its File
    // node (keeps best-effort source attribution, e.g. C/C++, from leaving orphan edges).
    let repointed = phases::calls::repoint_orphan_call_sources(&mut graph);
    let duration = phase_start.elapsed();
    tracing::info!(
        phase = "calls",
        duration_ms = duration.as_millis() as u64,
        total_edges = graph.relationship_count(),
        repointed_orphan_sources = repointed,
        "Phase complete"
    );
    phase_timings.push(PhaseTiming {
        name: "calls".into(),
        duration_ms: duration.as_millis() as u64,
    });
    send_progress(PipelinePhase::Calls, 100.0, "Calls resolved");

    // Phase 5: Heritage
    send_progress(PipelinePhase::Heritage, 0.0, "Processing inheritance...");
    let phase_start = Instant::now();
    phases::heritage::process_heritage(
        &mut graph,
        &extracted,
        &symbol_table,
        &import_map,
        &named_import_map,
        &re_export_map,
    )?;
    let duration = phase_start.elapsed();
    tracing::info!(
        phase = "heritage",
        duration_ms = duration.as_millis() as u64,
        total_edges = graph.relationship_count(),
        "Phase complete"
    );
    phase_timings.push(PhaseTiming {
        name: "heritage".into(),
        duration_ms: duration.as_millis() as u64,
    });
    send_progress(PipelinePhase::Heritage, 100.0, "Heritage processed");

    // Phase 5b: ASP.NET MVC 5 / EF6 enrichment
    // Runs after heritage (needs class hierarchy) and before communities
    send_progress(
        PipelinePhase::AspNetMvc,
        0.0,
        "Detecting ASP.NET MVC patterns...",
    );
    let phase_start = Instant::now();
    let aspnet_stats = phases::aspnet_mvc::enrich_aspnet_mvc(&mut graph, &file_entries)?;
    let duration = phase_start.elapsed();
    tracing::info!(
        phase = "aspnet_mvc",
        duration_ms = duration.as_millis() as u64,
        controllers = aspnet_stats.controllers,
        actions = aspnet_stats.actions,
        entities = aspnet_stats.db_entities,
        views = aspnet_stats.views,
        "Phase complete"
    );
    phase_timings.push(PhaseTiming {
        name: "aspnet_mvc".into(),
        duration_ms: duration.as_millis() as u64,
    });
    if aspnet_stats.controllers > 0 || aspnet_stats.db_entities > 0 {
        send_progress(
            PipelinePhase::AspNetMvc,
            100.0,
            &format!(
                "ASP.NET: {} controllers, {} actions, {} entities, {} views",
                aspnet_stats.controllers,
                aspnet_stats.actions + aspnet_stats.api_endpoints,
                aspnet_stats.db_entities,
                aspnet_stats.views,
            ),
        );
    } else {
        send_progress(
            PipelinePhase::AspNetMvc,
            100.0,
            "No ASP.NET MVC patterns detected",
        );
    }

    // Phase 5c: API surface extraction (Theme D).
    // Scans for REST endpoints across Express/Next.js/FastAPI/Spring and
    // creates ApiEndpoint nodes linked to their handler Methods. Skips C#
    // files (owned by aspnet_mvc).
    let phase_start = Instant::now();
    let api_stats = phases::api_surface::extract_api_surface(&mut graph, &file_entries);
    let duration = phase_start.elapsed();
    tracing::info!(
        phase = "api_surface",
        duration_ms = duration.as_millis() as u64,
        endpoints = api_stats.endpoints,
        express = api_stats.express_next,
        fastapi = api_stats.fastapi_flask,
        spring = api_stats.spring,
        next = api_stats.next_app_router,
        "Phase complete"
    );
    phase_timings.push(PhaseTiming {
        name: "api_surface".into(),
        duration_ms: duration.as_millis() as u64,
    });

    // Phase 5d: DB schema extraction (Theme D).
    // Parses SQL migrations, Prisma schemas, and ORM classes to produce
    // DbEntity + DbColumn nodes with HasColumn / ReferencesTable /
    // RepresentedBy edges. EF6 / .edmx is owned by aspnet_mvc.
    let phase_start = Instant::now();
    let db_stats = phases::db_schema::extract_db_schema(&mut graph, &file_entries);
    let duration = phase_start.elapsed();
    tracing::info!(
        phase = "db_schema",
        duration_ms = duration.as_millis() as u64,
        tables = db_stats.tables,
        columns = db_stats.columns,
        fks = db_stats.foreign_keys,
        orm = db_stats.orm_mappings,
        "Phase complete"
    );
    phase_timings.push(PhaseTiming {
        name: "db_schema".into(),
        duration_ms: duration.as_millis() as u64,
    });

    // Phase 5e: Config / env-var inventory (Theme D).
    let phase_start = Instant::now();
    let env_stats = phases::config_inventory::extract_env_vars(&mut graph, &file_entries);
    let duration = phase_start.elapsed();
    tracing::info!(
        phase = "config_inventory",
        duration_ms = duration.as_millis() as u64,
        declared = env_stats.declared,
        referenced = env_stats.referenced,
        unused = env_stats.unused,
        undeclared = env_stats.undeclared,
        "Phase complete"
    );
    phase_timings.push(PhaseTiming {
        name: "config_inventory".into(),
        duration_ms: duration.as_millis() as u64,
    });

    // Phase 6a: Community detection
    send_progress(PipelinePhase::Communities, 0.0, "Detecting communities...");
    let phase_start = Instant::now();
    let community_count = phases::community::detect_communities(&mut graph)?;
    let duration = phase_start.elapsed();
    tracing::info!(
        phase = "communities",
        duration_ms = duration.as_millis() as u64,
        communities = community_count,
        "Phase complete"
    );
    phase_timings.push(PhaseTiming {
        name: "communities".into(),
        duration_ms: duration.as_millis() as u64,
    });
    send_progress(
        PipelinePhase::Communities,
        100.0,
        &format!("Found {community_count} communities"),
    );

    // Phase 6b: Process detection
    send_progress(PipelinePhase::Processes, 0.0, "Tracing execution flows...");
    let phase_start = Instant::now();
    let process_count = phases::process::detect_processes(&mut graph)?;
    let duration = phase_start.elapsed();
    tracing::info!(
        phase = "processes",
        duration_ms = duration.as_millis() as u64,
        processes = process_count,
        "Phase complete"
    );
    phase_timings.push(PhaseTiming {
        name: "processes".into(),
        duration_ms: duration.as_millis() as u64,
    });
    send_progress(
        PipelinePhase::Processes,
        100.0,
        &format!("Found {process_count} processes"),
    );

    // Phase 7: Dead code detection
    let phase_start = Instant::now();
    phases::dead_code::mark_dead_code(&mut graph);
    let duration = phase_start.elapsed();
    tracing::info!(
        phase = "dead_code",
        duration_ms = duration.as_millis() as u64,
        "Phase complete"
    );
    phase_timings.push(PhaseTiming {
        name: "dead_code".into(),
        duration_ms: duration.as_millis() as u64,
    });

    // Phase 8: LLM Enrichment (optional, requires API key)
    if let Some(ref llm_config) = options.llm_enrich {
        send_progress(PipelinePhase::Enriching, 0.0, "Enriching with LLM...");
        let phase_start = Instant::now();
        let llm_stats = phases::llm_enrichment::enrich_with_llm(
            &mut graph,
            repo_path,
            llm_config,
            progress_tx.as_ref(),
        )
        .await?;
        let duration = phase_start.elapsed();
        tracing::info!(
            phase = "llm_enrichment",
            duration_ms = duration.as_millis() as u64,
            enriched = llm_stats.symbols_enriched,
            cached = llm_stats.symbols_skipped_cached,
            batches = llm_stats.batches_sent,
            tokens = llm_stats.tokens_used_estimate,
            errors = llm_stats.errors,
            "Phase complete"
        );
        phase_timings.push(PhaseTiming {
            name: "llm_enrichment".into(),
            duration_ms: duration.as_millis() as u64,
        });
        send_progress(
            PipelinePhase::Enriching,
            100.0,
            &format!("Enriched {} symbols", llm_stats.symbols_enriched),
        );
    }

    send_progress(PipelinePhase::Complete, 100.0, "Pipeline complete");

    let total_duration_ms = pipeline_start.elapsed().as_millis() as u64;
    tracing::info!(
        total_duration_ms = total_duration_ms,
        total_files = total_files,
        total_nodes = graph.node_count(),
        total_edges = graph.relationship_count(),
        total_communities = community_count,
        total_processes = process_count,
        "Pipeline complete"
    );

    Ok(PipelineResult {
        graph,
        repo_path: repo_path_str,
        total_file_count: total_files,
        community_count,
        process_count,
        total_duration_ms,
        phase_timings,
    })
}

/// Topological level sort using Kahn's algorithm.
/// Groups files by dependency level for parallel processing.
pub fn topological_level_sort(import_map: &HashMap<String, HashSet<String>>) -> TopologicalResult {
    let mut in_degree: HashMap<&str, usize> = HashMap::new();
    let mut reverse_deps: HashMap<&str, Vec<&str>> = HashMap::new();
    let mut all_files: HashSet<&str> = HashSet::new();

    // Build in-degree and reverse dependency maps
    for (file, imports) in import_map {
        all_files.insert(file);
        in_degree.entry(file).or_insert(0);
        for imported in imports {
            if imported == file {
                continue;
            } // skip self-imports
            all_files.insert(imported);
            *in_degree.entry(file.as_str()).or_insert(0) += 1;
            reverse_deps
                .entry(imported.as_str())
                .or_default()
                .push(file);
        }
    }

    // Initialize with zero-degree nodes
    for file in &all_files {
        in_degree.entry(file).or_insert(0);
    }

    let mut queue: Vec<&str> = in_degree
        .iter()
        .filter(|(_, deg)| **deg == 0)
        .map(|(file, _)| *file)
        .collect();
    queue.sort(); // Deterministic ordering

    let mut levels: Vec<Vec<String>> = Vec::new();
    let mut processed = 0;

    while !queue.is_empty() {
        let current_level: Vec<String> = queue.iter().map(|s| s.to_string()).collect();
        let mut next_queue = Vec::new();

        for file in &queue {
            if let Some(dependents) = reverse_deps.get(file) {
                for dep in dependents {
                    if let Some(deg) = in_degree.get_mut(dep) {
                        *deg -= 1;
                        if *deg == 0 {
                            next_queue.push(*dep);
                        }
                    }
                }
            }
        }

        processed += current_level.len();
        levels.push(current_level);
        next_queue.sort();
        queue = next_queue;
    }

    // Remaining nodes are in cycles
    let cycle_count = all_files.len() - processed;
    if cycle_count > 0 {
        let cycle_files: Vec<String> = in_degree
            .iter()
            .filter(|(_, deg)| **deg > 0)
            .map(|(file, _)| file.to_string())
            .collect();
        levels.push(cycle_files);
    }

    TopologicalResult {
        levels,
        cycle_count,
    }
}

pub struct TopologicalResult {
    pub levels: Vec<Vec<String>>,
    pub cycle_count: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_topological_sort_linear_chain() {
        // a.ts -> b.ts -> c.ts (a imports b, b imports c)
        let mut import_map: HashMap<String, HashSet<String>> = HashMap::new();
        import_map.insert("a.ts".to_string(), HashSet::from(["b.ts".to_string()]));
        import_map.insert("b.ts".to_string(), HashSet::from(["c.ts".to_string()]));

        let result = topological_level_sort(&import_map);
        assert_eq!(result.cycle_count, 0);
        assert_eq!(result.levels.len(), 3);
        // c.ts has no imports, so it should be in level 0
        assert!(result.levels[0].contains(&"c.ts".to_string()));
        // b.ts depends on c.ts, so level 1
        assert!(result.levels[1].contains(&"b.ts".to_string()));
        // a.ts depends on b.ts, so level 2
        assert!(result.levels[2].contains(&"a.ts".to_string()));
    }

    #[test]
    fn test_topological_sort_parallel_deps() {
        // a.ts -> c.ts, b.ts -> c.ts (both a and b import c)
        let mut import_map: HashMap<String, HashSet<String>> = HashMap::new();
        import_map.insert("a.ts".to_string(), HashSet::from(["c.ts".to_string()]));
        import_map.insert("b.ts".to_string(), HashSet::from(["c.ts".to_string()]));

        let result = topological_level_sort(&import_map);
        assert_eq!(result.cycle_count, 0);
        assert_eq!(result.levels.len(), 2);
        // c.ts in level 0
        assert!(result.levels[0].contains(&"c.ts".to_string()));
        // a.ts and b.ts in level 1
        assert!(result.levels[1].contains(&"a.ts".to_string()));
        assert!(result.levels[1].contains(&"b.ts".to_string()));
    }

    #[test]
    fn test_topological_sort_cycle() {
        // a.ts -> b.ts -> a.ts (cycle)
        let mut import_map: HashMap<String, HashSet<String>> = HashMap::new();
        import_map.insert("a.ts".to_string(), HashSet::from(["b.ts".to_string()]));
        import_map.insert("b.ts".to_string(), HashSet::from(["a.ts".to_string()]));

        let result = topological_level_sort(&import_map);
        assert_eq!(result.cycle_count, 2);
        // The cycle files should still appear in the levels (as the last level)
        let all_files: HashSet<String> = result
            .levels
            .iter()
            .flat_map(|level| level.iter().cloned())
            .collect();
        assert!(all_files.contains("a.ts"));
        assert!(all_files.contains("b.ts"));
    }

    #[test]
    fn test_topological_sort_empty() {
        let import_map: HashMap<String, HashSet<String>> = HashMap::new();
        let result = topological_level_sort(&import_map);
        assert_eq!(result.cycle_count, 0);
        assert!(result.levels.is_empty());
    }

    #[test]
    fn test_topological_sort_no_deps() {
        // All files independent
        let mut import_map: HashMap<String, HashSet<String>> = HashMap::new();
        import_map.insert("a.ts".to_string(), HashSet::new());
        import_map.insert("b.ts".to_string(), HashSet::new());
        import_map.insert("c.ts".to_string(), HashSet::new());

        let result = topological_level_sort(&import_map);
        assert_eq!(result.cycle_count, 0);
        assert_eq!(result.levels.len(), 1);
        assert_eq!(result.levels[0].len(), 3);
    }

    #[test]
    fn test_topological_sort_diamond() {
        // Diamond: a -> b, a -> c, b -> d, c -> d
        let mut import_map: HashMap<String, HashSet<String>> = HashMap::new();
        import_map.insert(
            "a.ts".to_string(),
            HashSet::from(["b.ts".to_string(), "c.ts".to_string()]),
        );
        import_map.insert("b.ts".to_string(), HashSet::from(["d.ts".to_string()]));
        import_map.insert("c.ts".to_string(), HashSet::from(["d.ts".to_string()]));

        let result = topological_level_sort(&import_map);
        assert_eq!(result.cycle_count, 0);
        assert_eq!(result.levels.len(), 3);
        assert!(result.levels[0].contains(&"d.ts".to_string()));
        assert!(result.levels[1].contains(&"b.ts".to_string()));
        assert!(result.levels[1].contains(&"c.ts".to_string()));
        assert!(result.levels[2].contains(&"a.ts".to_string()));
    }
}

#[cfg(test)]
mod integration_tests {
    use super::*;
    use code_explorer_core::graph::{NodeLabel, RelationshipType};
    use std::fs;
    use std::path::PathBuf;

    fn create_test_dir() -> PathBuf {
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let dir = std::env::temp_dir().join(format!(
            "code_explorer_test_{}_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            COUNTER.fetch_add(1, Ordering::Relaxed),
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn cleanup(dir: &PathBuf) {
        let _ = fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn test_pipeline_csharp_controller() {
        let dir = create_test_dir();
        let cs_file = dir.join("HomeController.cs");
        fs::write(
            &cs_file,
            r#"
using System.Web.Mvc;

public class HomeController : Controller
{
    public ActionResult Index()
    {
        return View();
    }

    [HttpPost]
    public ActionResult Login(string username, string password)
    {
        return RedirectToAction("Index");
    }
}
"#,
        )
        .unwrap();

        let result = run_pipeline(&dir, None, PipelineOptions::default()).await;
        assert!(
            result.is_ok(),
            "Pipeline should succeed: {:?}",
            result.err()
        );

        let result = result.unwrap();
        let graph = &result.graph;

        // Verify nodes exist
        assert!(graph.node_count() > 0, "Graph should have nodes");

        // Check for Class or Controller nodes named HomeController
        let has_class = graph.iter_nodes().any(|n| {
            n.properties.name == "HomeController"
                && (n.label == NodeLabel::Class || n.label == NodeLabel::Controller)
        });
        assert!(has_class, "Should detect HomeController class");

        cleanup(&dir);
    }

    #[tokio::test]
    async fn test_pipeline_javascript_functions() {
        let dir = create_test_dir();
        let js_file = dir.join("app.js");
        fs::write(
            &js_file,
            r#"
function greet(name) {
    return "Hello, " + name;
}

function processData(items) {
    return items.map(item => greet(item.name));
}

module.exports = { greet, processData };
"#,
        )
        .unwrap();

        let result = run_pipeline(&dir, None, PipelineOptions::default()).await;
        assert!(result.is_ok(), "Pipeline failed: {:?}", result.err());

        let result = result.unwrap();
        let graph = &result.graph;

        // Should have Function nodes
        let functions: Vec<&str> = graph
            .iter_nodes()
            .filter(|n| n.label == NodeLabel::Function)
            .map(|n| n.properties.name.as_str())
            .collect();

        assert!(
            functions.contains(&"greet"),
            "Should detect greet function, found: {:?}",
            functions
        );
        assert!(
            functions.contains(&"processData"),
            "Should detect processData function, found: {:?}",
            functions
        );

        cleanup(&dir);
    }

    #[tokio::test]
    async fn test_pipeline_typescript_named_import_disambiguates_calls() {
        let dir = create_test_dir();
        fs::write(dir.join("a.ts"), "export function foo() { return 1; }\n").unwrap();
        fs::write(dir.join("c.ts"), "export function foo() { return 2; }\n").unwrap();
        fs::write(
            dir.join("b.ts"),
            "import { foo } from \"./a.js\";\nexport function load() { return foo(); }\n",
        )
        .unwrap();

        let result = run_pipeline(
            &dir,
            None,
            PipelineOptions {
                skip_git: true,
                ..Default::default()
            },
        )
        .await
        .unwrap();

        let call = result
            .graph
            .iter_relationships()
            .find(|rel| {
                if !matches!(rel.rel_type, RelationshipType::Calls) {
                    return false;
                }
                let Some(source) = result.graph.get_node(&rel.source_id) else {
                    return false;
                };
                let Some(target) = result.graph.get_node(&rel.target_id) else {
                    return false;
                };
                source.properties.name == "load" && target.properties.name == "foo"
            })
            .expect("load should call imported foo");

        let target = result.graph.get_node(&call.target_id).unwrap();
        assert!(
            target.properties.file_path.ends_with("a.ts"),
            "expected imported a.ts foo, got {} via {}",
            target.properties.file_path,
            call.reason
        );
        assert_eq!(call.reason, "named-import");
        assert!(call.confidence >= 0.8);

        cleanup(&dir);
    }

    #[tokio::test]
    async fn test_pipeline_typescript_type_only_imports_are_not_runtime_call_targets() {
        let dir = create_test_dir();
        fs::write(
            dir.join("a.ts"),
            "export function Foo() { return 1; }\nexport function run() { return 2; }\n",
        )
        .unwrap();
        fs::write(
            dir.join("index.ts"),
            "export type { Foo } from \"./a.js\";\n",
        )
        .unwrap();
        fs::write(
            dir.join("b.ts"),
            r#"import type { Foo as TypeFoo } from "./a.js";
import { type Foo, run } from "./a.js";
import { Foo as ReExportedTypeFoo } from "./index.js";

export function load() {
  TypeFoo();
  Foo();
  ReExportedTypeFoo();
  return run();
}
"#,
        )
        .unwrap();

        let result = run_pipeline(
            &dir,
            None,
            PipelineOptions {
                skip_git: true,
                ..Default::default()
            },
        )
        .await
        .unwrap();

        let bad_type_only_call = result.graph.iter_relationships().find(|rel| {
            if !matches!(rel.rel_type, RelationshipType::Calls) {
                return false;
            }
            let Some(source) = result.graph.get_node(&rel.source_id) else {
                return false;
            };
            let Some(target) = result.graph.get_node(&rel.target_id) else {
                return false;
            };
            source.properties.name == "load" && target.properties.name == "Foo"
        });
        assert!(
            bad_type_only_call.is_none(),
            "type-only imports and re-exports should not become runtime CALLS targets"
        );

        let runtime_call = result
            .graph
            .iter_relationships()
            .find(|rel| {
                if !matches!(rel.rel_type, RelationshipType::Calls) {
                    return false;
                }
                let Some(source) = result.graph.get_node(&rel.source_id) else {
                    return false;
                };
                let Some(target) = result.graph.get_node(&rel.target_id) else {
                    return false;
                };
                source.properties.name == "load" && target.properties.name == "run"
            })
            .expect("runtime named imports should still resolve");

        assert_eq!(runtime_call.reason, "named-import");

        cleanup(&dir);
    }

    #[tokio::test]
    async fn test_pipeline_typescript_type_only_namespace_imports_are_not_runtime_call_targets() {
        let dir = create_test_dir();
        fs::write(
            dir.join("a.ts"),
            "export function Foo() { return 1; }\nexport function run() { return 2; }\n",
        )
        .unwrap();
        fs::write(
            dir.join("index.ts"),
            r#"export type * as Types from "./a.js";
export { run } from "./a.js";
"#,
        )
        .unwrap();
        fs::write(
            dir.join("b.ts"),
            r#"import type * as DirectTypes from "./a.js";
import { Types, run } from "./index.js";

export function load() {
  DirectTypes.Foo();
  Types.Foo();
  return run();
}
"#,
        )
        .unwrap();

        let result = run_pipeline(
            &dir,
            None,
            PipelineOptions {
                skip_git: true,
                ..Default::default()
            },
        )
        .await
        .unwrap();

        let bad_type_only_namespace_call = result.graph.iter_relationships().find(|rel| {
            if !matches!(rel.rel_type, RelationshipType::Calls) {
                return false;
            }
            let Some(source) = result.graph.get_node(&rel.source_id) else {
                return false;
            };
            let Some(target) = result.graph.get_node(&rel.target_id) else {
                return false;
            };
            source.properties.name == "load" && target.properties.name == "Foo"
        });
        assert!(
            bad_type_only_namespace_call.is_none(),
            "type-only namespace imports and re-exports should not become runtime CALLS targets"
        );

        let runtime_call = result
            .graph
            .iter_relationships()
            .find(|rel| {
                if !matches!(rel.rel_type, RelationshipType::Calls) {
                    return false;
                }
                let Some(source) = result.graph.get_node(&rel.source_id) else {
                    return false;
                };
                let Some(target) = result.graph.get_node(&rel.target_id) else {
                    return false;
                };
                source.properties.name == "load" && target.properties.name == "run"
            })
            .expect("runtime re-exported imports should still resolve");

        assert_eq!(runtime_call.reason, "named-import");

        cleanup(&dir);
    }

    #[tokio::test]
    async fn test_pipeline_typescript_external_imported_free_call_ignores_property_collision() {
        let dir = create_test_dir();
        fs::write(
            dir.join("service-installer.ts"),
            r#"import { platform } from "os";

export interface InstallResult {
  platform: string;
}

export class ServiceInstaller {
  install(): InstallResult {
    const os = platform();
    return { platform: os };
  }
}
"#,
        )
        .unwrap();

        let result = run_pipeline(
            &dir,
            None,
            PipelineOptions {
                skip_git: true,
                ..Default::default()
            },
        )
        .await
        .unwrap();

        let bad_same_file_property_call = result.graph.iter_relationships().find(|rel| {
            if !matches!(rel.rel_type, RelationshipType::Calls) || rel.reason != "same-file" {
                return false;
            }
            let Some(source) = result.graph.get_node(&rel.source_id) else {
                return false;
            };
            let Some(target) = result.graph.get_node(&rel.target_id) else {
                return false;
            };
            source.properties.name == "install"
                && target.label == NodeLabel::Property
                && target.properties.name == "platform"
        });

        assert!(
            bad_same_file_property_call.is_none(),
            "free calls to imported TS bindings should not resolve to same-file property declarations"
        );

        cleanup(&dir);
    }

    #[tokio::test]
    async fn test_pipeline_typescript_external_named_import_shadows_import_scoped_candidate() {
        let dir = create_test_dir();
        fs::write(
            dir.join("writer.ts"),
            "export function writeFile() { return undefined; }\nexport function helper() { return undefined; }\n",
        )
        .unwrap();
        fs::write(
            dir.join("main.ts"),
            r#"import { writeFile } from "fs/promises";
import { helper } from "./writer.js";

export async function save() {
  helper();
  await writeFile("out.txt", "data");
}
"#,
        )
        .unwrap();

        let result = run_pipeline(
            &dir,
            None,
            PipelineOptions {
                skip_git: true,
                ..Default::default()
            },
        )
        .await
        .unwrap();

        let helper_call = result
            .graph
            .iter_relationships()
            .find(|rel| {
                if !matches!(rel.rel_type, RelationshipType::Calls) {
                    return false;
                }
                let Some(source) = result.graph.get_node(&rel.source_id) else {
                    return false;
                };
                let Some(target) = result.graph.get_node(&rel.target_id) else {
                    return false;
                };
                source.properties.name == "save" && target.properties.name == "helper"
            })
            .expect("relative helper import should still resolve");

        let helper_target = result.graph.get_node(&helper_call.target_id).unwrap();
        assert!(
            helper_target.properties.file_path.ends_with("writer.ts"),
            "expected helper target in writer.ts, got {} via {}",
            helper_target.properties.file_path,
            helper_call.reason
        );
        assert_eq!(helper_call.reason, "named-import");

        let bad_import_scoped_call = result.graph.iter_relationships().find(|rel| {
            if !matches!(rel.rel_type, RelationshipType::Calls) || rel.reason != "import-scoped" {
                return false;
            }
            let Some(source) = result.graph.get_node(&rel.source_id) else {
                return false;
            };
            let Some(target) = result.graph.get_node(&rel.target_id) else {
                return false;
            };
            source.properties.name == "save"
                && target.properties.name == "writeFile"
                && target.properties.file_path.ends_with("writer.ts")
        });

        assert!(
            bad_import_scoped_call.is_none(),
            "external named imports like fs/promises.writeFile should not resolve to unrelated local import-scoped symbols"
        );

        cleanup(&dir);
    }

    #[tokio::test]
    async fn test_pipeline_typescript_external_typed_receiver_ignores_property_collision() {
        let dir = create_test_dir();
        fs::write(
            dir.join("cli.ts"),
            r#"import type { Command } from "commander";

export interface CliOptions {
  command?: string;
  description?: string;
  memory?: string;
}

export function register(program: Command): void {
  const hermes = program
    .command("hermes")
    .description("Inspect Hermes");

  hermes
    .command("run")
    .description("Run task");

  const memory = hermes
    .command("memory")
    .description("Manage memory");

  memory
    .command("status")
    .description("Show memory status");
}

export function createPipelineCommand(): Command {
  const pipelineCommand = new Command("pipeline");
  pipelineCommand.description("Manage pipelines");
  return pipelineCommand;
}
"#,
        )
        .unwrap();

        let result = run_pipeline(
            &dir,
            None,
            PipelineOptions {
                skip_git: true,
                ..Default::default()
            },
        )
        .await
        .unwrap();

        let bad_same_file_property_call = result.graph.iter_relationships().find(|rel| {
            if !matches!(rel.rel_type, RelationshipType::Calls) || rel.reason != "same-file" {
                return false;
            }
            let Some(source) = result.graph.get_node(&rel.source_id) else {
                return false;
            };
            let Some(target) = result.graph.get_node(&rel.target_id) else {
                return false;
            };
            matches!(
                source.properties.name.as_str(),
                "register" | "createPipelineCommand"
            ) && target.label == NodeLabel::Property
                && matches!(target.properties.name.as_str(), "command" | "description")
        });

        assert!(
            bad_same_file_property_call.is_none(),
            "member calls on receivers typed from external imports should not resolve to same-file property declarations"
        );

        cleanup(&dir);
    }

    #[tokio::test]
    async fn test_pipeline_typescript_local_function_alias_ignores_property_collision() {
        let dir = create_test_dir();
        fs::write(
            dir.join("whatsapp.ts"),
            r#"interface BaileysModule {
  useMultiFileAuthState: (path: string) => Promise<{ state: unknown }>;
}

export class WhatsAppChannel {
  loadAuthState(baileys: BaileysModule, sessionPath: string) {
    const useMultiFileAuthState = baileys.useMultiFileAuthState;
    return useMultiFileAuthState(sessionPath);
  }
}
"#,
        )
        .unwrap();

        let result = run_pipeline(
            &dir,
            None,
            PipelineOptions {
                skip_git: true,
                ..Default::default()
            },
        )
        .await
        .unwrap();

        let bad_same_file_property_call = result.graph.iter_relationships().find(|rel| {
            if !matches!(rel.rel_type, RelationshipType::Calls) || rel.reason != "same-file" {
                return false;
            }
            let Some(source) = result.graph.get_node(&rel.source_id) else {
                return false;
            };
            let Some(target) = result.graph.get_node(&rel.target_id) else {
                return false;
            };
            source.properties.name == "loadAuthState"
                && target.label == NodeLabel::Property
                && target.properties.name == "useMultiFileAuthState"
        });

        assert!(
            bad_same_file_property_call.is_none(),
            "free calls to local function aliases should not resolve to same-file property declarations"
        );

        cleanup(&dir);
    }

    #[tokio::test]
    async fn test_pipeline_typescript_named_re_export_disambiguates_calls() {
        let dir = create_test_dir();
        fs::write(dir.join("a.ts"), "export function foo() { return 1; }\n").unwrap();
        fs::write(dir.join("c.ts"), "export function foo() { return 2; }\n").unwrap();
        fs::write(dir.join("index.ts"), "export { foo } from \"./a\";\n").unwrap();
        fs::write(
            dir.join("b.ts"),
            "import { foo } from \"./index\";\nexport function load() { return foo(); }\n",
        )
        .unwrap();

        let result = run_pipeline(
            &dir,
            None,
            PipelineOptions {
                skip_git: true,
                ..Default::default()
            },
        )
        .await
        .unwrap();

        let call = result
            .graph
            .iter_relationships()
            .find(|rel| {
                if !matches!(rel.rel_type, RelationshipType::Calls) {
                    return false;
                }
                let Some(source) = result.graph.get_node(&rel.source_id) else {
                    return false;
                };
                let Some(target) = result.graph.get_node(&rel.target_id) else {
                    return false;
                };
                source.properties.name == "load" && target.properties.name == "foo"
            })
            .expect("load should call re-exported foo");

        let target = result.graph.get_node(&call.target_id).unwrap();
        assert!(
            target.properties.file_path.ends_with("a.ts"),
            "expected re-exported a.ts foo, got {} via {}",
            target.properties.file_path,
            call.reason
        );
        assert_eq!(call.reason, "named-import");

        cleanup(&dir);
    }

    #[tokio::test]
    async fn test_pipeline_typescript_local_import_re_export_disambiguates_calls() {
        let dir = create_test_dir();
        fs::write(dir.join("a.ts"), "export function foo() { return 1; }\n").unwrap();
        fs::write(dir.join("c.ts"), "export function foo() { return 2; }\n").unwrap();
        fs::write(
            dir.join("index.ts"),
            "import { foo } from \"./a.js\";\nexport { foo };\n",
        )
        .unwrap();
        fs::write(
            dir.join("b.ts"),
            "import { foo } from \"./index.js\";\nexport function load() { return foo(); }\n",
        )
        .unwrap();

        let result = run_pipeline(
            &dir,
            None,
            PipelineOptions {
                skip_git: true,
                ..Default::default()
            },
        )
        .await
        .unwrap();

        let call = result
            .graph
            .iter_relationships()
            .find(|rel| {
                if !matches!(rel.rel_type, RelationshipType::Calls) {
                    return false;
                }
                let Some(source) = result.graph.get_node(&rel.source_id) else {
                    return false;
                };
                let Some(target) = result.graph.get_node(&rel.target_id) else {
                    return false;
                };
                source.properties.name == "load" && target.properties.name == "foo"
            })
            .expect("load should call locally re-exported foo");

        let target = result.graph.get_node(&call.target_id).unwrap();
        assert!(
            target.properties.file_path.ends_with("a.ts"),
            "expected locally re-exported a.ts foo, got {} via {}",
            target.properties.file_path,
            call.reason
        );
        assert_eq!(call.reason, "named-import");

        cleanup(&dir);
    }

    #[tokio::test]
    async fn test_pipeline_typescript_commented_re_export_disambiguates_calls() {
        let dir = create_test_dir();
        fs::write(dir.join("a.ts"), "export function foo() { return 1; }\n").unwrap();
        fs::write(dir.join("c.ts"), "export function foo() { return 2; }\n").unwrap();
        fs::write(
            dir.join("index.ts"),
            r#"export {
  // Preferred implementation
  foo,
} from "./a.js";
"#,
        )
        .unwrap();
        fs::write(
            dir.join("b.ts"),
            "import { foo } from \"./index.js\";\nexport function load() { return foo(); }\n",
        )
        .unwrap();

        let result = run_pipeline(
            &dir,
            None,
            PipelineOptions {
                skip_git: true,
                ..Default::default()
            },
        )
        .await
        .unwrap();

        let call = result
            .graph
            .iter_relationships()
            .find(|rel| {
                if !matches!(rel.rel_type, RelationshipType::Calls) {
                    return false;
                }
                let Some(source) = result.graph.get_node(&rel.source_id) else {
                    return false;
                };
                let Some(target) = result.graph.get_node(&rel.target_id) else {
                    return false;
                };
                source.properties.name == "load" && target.properties.name == "foo"
            })
            .expect("load should call commented re-exported foo");

        let target = result.graph.get_node(&call.target_id).unwrap();
        assert!(
            target.properties.file_path.ends_with("a.ts"),
            "expected commented re-exported a.ts foo, got {} via {}",
            target.properties.file_path,
            call.reason
        );
        assert_eq!(call.reason, "named-import");

        cleanup(&dir);
    }

    #[tokio::test]
    async fn test_pipeline_typescript_export_namespace_barrel_resolves_member_call() {
        let dir = create_test_dir();
        fs::write(dir.join("a.ts"), "export function foo() { return 1; }\n").unwrap();
        fs::write(dir.join("c.ts"), "export function foo() { return 2; }\n").unwrap();
        fs::write(dir.join("index.ts"), "export * as api from \"./a\";\n").unwrap();
        fs::write(dir.join("barrel.ts"), "export { api } from \"./index\";\n").unwrap();
        fs::write(
            dir.join("b.ts"),
            "import { api } from \"./barrel\";\nfunction foo() { return 99; }\nexport function load() { return api.foo(); }\n",
        )
        .unwrap();

        let result = run_pipeline(
            &dir,
            None,
            PipelineOptions {
                skip_git: true,
                ..Default::default()
            },
        )
        .await
        .unwrap();

        let call = result
            .graph
            .iter_relationships()
            .find(|rel| {
                if !matches!(rel.rel_type, RelationshipType::Calls) {
                    return false;
                }
                let Some(source) = result.graph.get_node(&rel.source_id) else {
                    return false;
                };
                let Some(target) = result.graph.get_node(&rel.target_id) else {
                    return false;
                };
                source.properties.name == "load" && target.properties.name == "foo"
            })
            .expect("load should call foo through an export namespace barrel");

        let target = result.graph.get_node(&call.target_id).unwrap();
        assert!(
            target.properties.file_path.ends_with("a.ts"),
            "expected export namespace barrel to resolve a.ts foo, got {} via {}",
            target.properties.file_path,
            call.reason
        );
        assert_eq!(call.reason, "module-alias:api:foo");

        cleanup(&dir);
    }

    #[tokio::test]
    async fn test_pipeline_typescript_default_import_resolves_exported_symbol() {
        let dir = create_test_dir();
        fs::write(
            dir.join("a.ts"),
            "export default function runTask() { return 1; }\n",
        )
        .unwrap();
        fs::write(dir.join("c.ts"), "export function task() { return 2; }\n").unwrap();
        fs::write(
            dir.join("b.ts"),
            "import task from \"./a\";\nexport function load() { return task(); }\n",
        )
        .unwrap();

        let result = run_pipeline(
            &dir,
            None,
            PipelineOptions {
                skip_git: true,
                ..Default::default()
            },
        )
        .await
        .unwrap();

        let call = result
            .graph
            .iter_relationships()
            .find(|rel| {
                if !matches!(rel.rel_type, RelationshipType::Calls) {
                    return false;
                }
                let Some(source) = result.graph.get_node(&rel.source_id) else {
                    return false;
                };
                let Some(target) = result.graph.get_node(&rel.target_id) else {
                    return false;
                };
                source.properties.name == "load" && target.properties.name == "runTask"
            })
            .expect("load should call the default-exported runTask symbol");

        let target = result.graph.get_node(&call.target_id).unwrap();
        assert!(
            target.properties.file_path.ends_with("a.ts"),
            "expected default export in a.ts, got {} via {}",
            target.properties.file_path,
            call.reason
        );
        assert_eq!(call.reason, "named-import");

        cleanup(&dir);
    }

    #[tokio::test]
    async fn test_pipeline_typescript_default_anonymous_exports_are_call_targets() {
        let dir = create_test_dir();
        fs::write(
            dir.join("fn.ts"),
            "export default function() { return 1; }\n",
        )
        .unwrap();
        fs::write(
            dir.join("cls.ts"),
            r#"export default class {
  static create() { return 2; }
  run() { return 3; }
}
"#,
        )
        .unwrap();
        fs::write(
            dir.join("app.ts"),
            r#"import doWork from "./fn.js";
import Service from "./cls.js";

export function load() {
  doWork();
  Service.create();
  const svc = new Service();
  return svc.run();
}
"#,
        )
        .unwrap();

        let result = run_pipeline(
            &dir,
            None,
            PipelineOptions {
                skip_git: true,
                ..Default::default()
            },
        )
        .await
        .unwrap();

        let fn_default = result
            .graph
            .iter_nodes()
            .find(|node| {
                node.label == NodeLabel::Function
                    && node.properties.file_path == "fn.ts"
                    && node.properties.name == "default"
            })
            .expect("anonymous default function should create a default Function node");
        assert_eq!(fn_default.properties.is_exported, Some(true));

        let class_default = result
            .graph
            .iter_nodes()
            .find(|node| {
                node.label == NodeLabel::Class
                    && node.properties.file_path == "cls.ts"
                    && node.properties.name == "default"
            })
            .expect("anonymous default class should create a default Class node");
        assert_eq!(class_default.properties.is_exported, Some(true));

        let has_create = result.graph.iter_relationships().any(|rel| {
            if !matches!(rel.rel_type, RelationshipType::HasMethod)
                || rel.source_id != class_default.id
            {
                return false;
            }
            result
                .graph
                .get_node(&rel.target_id)
                .is_some_and(|target| target.properties.name == "create")
        });
        assert!(
            has_create,
            "default anonymous class methods should be nested under Class:default"
        );

        let call_to_default_function = result.graph.iter_relationships().find(|rel| {
            if !matches!(rel.rel_type, RelationshipType::Calls) {
                return false;
            }
            let Some(source) = result.graph.get_node(&rel.source_id) else {
                return false;
            };
            let Some(target) = result.graph.get_node(&rel.target_id) else {
                return false;
            };
            source.properties.name == "load"
                && target.properties.file_path == "fn.ts"
                && target.properties.name == "default"
        });
        assert!(
            call_to_default_function.is_some_and(|rel| rel.reason == "named-import"),
            "default function import should resolve via named-import"
        );

        let static_call = result
            .graph
            .iter_relationships()
            .find(|rel| {
                if !matches!(rel.rel_type, RelationshipType::Calls) {
                    return false;
                }
                let Some(source) = result.graph.get_node(&rel.source_id) else {
                    return false;
                };
                let Some(target) = result.graph.get_node(&rel.target_id) else {
                    return false;
                };
                source.properties.name == "load"
                    && target.properties.file_path == "cls.ts"
                    && target.properties.name == "create"
            })
            .expect("default class static method should resolve");
        assert!(static_call
            .reason
            .starts_with("static-call-ts:Service::create"));

        let instance_call = result
            .graph
            .iter_relationships()
            .find(|rel| {
                if !matches!(rel.rel_type, RelationshipType::Calls) {
                    return false;
                }
                let Some(source) = result.graph.get_node(&rel.source_id) else {
                    return false;
                };
                let Some(target) = result.graph.get_node(&rel.target_id) else {
                    return false;
                };
                source.properties.name == "load"
                    && target.properties.file_path == "cls.ts"
                    && target.properties.name == "run"
            })
            .expect("default class instance method should resolve");
        assert!(instance_call
            .reason
            .starts_with("receiver-type:svc:Service:run"));

        cleanup(&dir);
    }

    #[tokio::test]
    async fn test_pipeline_typescript_default_object_export_resolves_members() {
        let dir = create_test_dir();
        fs::write(
            dir.join("api.ts"),
            r#"function hiddenTask() { return 1; }
export function directTask() { return 2; }
class Service {
  static create() { return 3; }
}

export default {
  runTask: hiddenTask,
  directTask,
  Service,
};
"#,
        )
        .unwrap();
        fs::write(
            dir.join("other.ts"),
            "export function hiddenTask() { return 99; }\nexport function runTask() { return 98; }\n",
        )
        .unwrap();
        fs::write(
            dir.join("index.ts"),
            "export { default as api } from \"./api\";\n",
        )
        .unwrap();
        fs::write(
            dir.join("app.ts"),
            r#"import apiDirect from "./api";
import { api } from "./index";

export function load() {
  apiDirect.runTask();
  api.directTask();
  return api.Service.create();
}
"#,
        )
        .unwrap();

        let result = run_pipeline(
            &dir,
            None,
            PipelineOptions {
                skip_git: true,
                ..Default::default()
            },
        )
        .await
        .unwrap();

        let run_task_call = result
            .graph
            .iter_relationships()
            .find(|rel| {
                if !matches!(rel.rel_type, RelationshipType::Calls) {
                    return false;
                }
                let Some(source) = result.graph.get_node(&rel.source_id) else {
                    return false;
                };
                let Some(target) = result.graph.get_node(&rel.target_id) else {
                    return false;
                };
                source.properties.name == "load"
                    && target.properties.file_path == "api.ts"
                    && target.properties.name == "hiddenTask"
            })
            .expect("default object member should resolve to aliased local function");
        assert_eq!(run_task_call.reason, "default-object:apiDirect:runTask");

        let direct_call = result
            .graph
            .iter_relationships()
            .find(|rel| {
                if !matches!(rel.rel_type, RelationshipType::Calls) {
                    return false;
                }
                let Some(source) = result.graph.get_node(&rel.source_id) else {
                    return false;
                };
                let Some(target) = result.graph.get_node(&rel.target_id) else {
                    return false;
                };
                source.properties.name == "load"
                    && target.properties.file_path == "api.ts"
                    && target.properties.name == "directTask"
            })
            .expect("barrel-exported default object should resolve shorthand member");
        assert_eq!(direct_call.reason, "default-object:api:directTask");

        let static_call = result
            .graph
            .iter_relationships()
            .find(|rel| {
                if !matches!(rel.rel_type, RelationshipType::Calls) {
                    return false;
                }
                let Some(source) = result.graph.get_node(&rel.source_id) else {
                    return false;
                };
                let Some(target) = result.graph.get_node(&rel.target_id) else {
                    return false;
                };
                source.properties.name == "load"
                    && target.properties.file_path == "api.ts"
                    && target.properties.name == "create"
            })
            .expect("default object class member should resolve static method");
        assert_eq!(
            static_call.reason,
            "default-object-static:api:Service::create"
        );

        cleanup(&dir);
    }

    #[tokio::test]
    async fn test_pipeline_typescript_named_object_export_resolves_members() {
        let dir = create_test_dir();
        fs::write(
            dir.join("api.ts"),
            r#"function hiddenTask() { return 1; }
export function directTask() { return 2; }
export function runTask() { return 98; }
class Service {
  static create() { return 3; }
}

export const api: {
  runTask(): number;
  directTask(): number;
  inline(): number;
  Service: typeof Service;
} = {
  runTask: hiddenTask,
  directTask,
  inline: () => directTask(),
  Service,
};
"#,
        )
        .unwrap();
        fs::write(
            dir.join("other.ts"),
            "export function runTask() { return 99; }\n",
        )
        .unwrap();
        fs::write(
            dir.join("app.ts"),
            r#"import { api } from "./api";

export function load() {
  api.runTask();
  api.directTask();
  api.inline();
  return api.Service.create();
}
"#,
        )
        .unwrap();

        let result = run_pipeline(
            &dir,
            None,
            PipelineOptions {
                skip_git: true,
                ..Default::default()
            },
        )
        .await
        .unwrap();

        let run_task_call = result
            .graph
            .iter_relationships()
            .find(|rel| {
                if !matches!(rel.rel_type, RelationshipType::Calls) {
                    return false;
                }
                let Some(source) = result.graph.get_node(&rel.source_id) else {
                    return false;
                };
                let Some(target) = result.graph.get_node(&rel.target_id) else {
                    return false;
                };
                source.properties.name == "load"
                    && target.properties.file_path == "api.ts"
                    && target.properties.name == "hiddenTask"
            })
            .expect("named object alias member should resolve to aliased local function");
        assert_eq!(run_task_call.reason, "named-object:api:runTask");

        let direct_call = result
            .graph
            .iter_relationships()
            .find(|rel| {
                if !matches!(rel.rel_type, RelationshipType::Calls) {
                    return false;
                }
                let Some(source) = result.graph.get_node(&rel.source_id) else {
                    return false;
                };
                let Some(target) = result.graph.get_node(&rel.target_id) else {
                    return false;
                };
                source.properties.name == "load"
                    && target.properties.file_path == "api.ts"
                    && target.properties.name == "directTask"
            })
            .expect("named object shorthand member should resolve");
        assert_eq!(direct_call.reason, "named-object:api:directTask");

        let inline_call = result
            .graph
            .iter_relationships()
            .find(|rel| {
                if !matches!(rel.rel_type, RelationshipType::Calls) {
                    return false;
                }
                let Some(source) = result.graph.get_node(&rel.source_id) else {
                    return false;
                };
                let Some(target) = result.graph.get_node(&rel.target_id) else {
                    return false;
                };
                source.properties.name == "load"
                    && target.properties.file_path == "api.ts"
                    && target.properties.name == "inline"
            })
            .expect("named object inline function member should resolve");
        assert_eq!(inline_call.reason, "named-object:api:inline");

        let static_call = result
            .graph
            .iter_relationships()
            .find(|rel| {
                if !matches!(rel.rel_type, RelationshipType::Calls) {
                    return false;
                }
                let Some(source) = result.graph.get_node(&rel.source_id) else {
                    return false;
                };
                let Some(target) = result.graph.get_node(&rel.target_id) else {
                    return false;
                };
                source.properties.name == "load"
                    && target.properties.file_path == "api.ts"
                    && target.properties.name == "create"
            })
            .expect("named object class member should resolve static method");
        assert_eq!(
            static_call.reason,
            "named-object-static:api:Service::create"
        );

        cleanup(&dir);
    }

    #[tokio::test]
    async fn test_pipeline_typescript_named_object_export_resolves_through_wildcard_js_shim() {
        let dir = create_test_dir();
        fs::write(
            dir.join("api.ts"),
            r#"function hiddenTask() { return 1; }
export function directTask() { return 2; }

export const api = {
  runTask: hiddenTask,
  directTask,
};
"#,
        )
        .unwrap();
        fs::write(dir.join("api.js"), "export * from \"./api.ts\";\n").unwrap();
        fs::write(
            dir.join("app.ts"),
            r#"import { api } from "./api.js";

export function load() {
  api.runTask();
  return api.directTask();
}
"#,
        )
        .unwrap();

        let result = run_pipeline(
            &dir,
            None,
            PipelineOptions {
                skip_git: true,
                ..Default::default()
            },
        )
        .await
        .unwrap();

        let run_task_call = result
            .graph
            .iter_relationships()
            .find(|rel| {
                if !matches!(rel.rel_type, RelationshipType::Calls) {
                    return false;
                }
                let Some(source) = result.graph.get_node(&rel.source_id) else {
                    return false;
                };
                let Some(target) = result.graph.get_node(&rel.target_id) else {
                    return false;
                };
                source.properties.name == "load"
                    && target.properties.file_path == "api.ts"
                    && target.properties.name == "hiddenTask"
            })
            .expect("wildcard js shim should expose named object aliased member");
        assert_eq!(run_task_call.reason, "named-object:api:runTask");

        let direct_call = result
            .graph
            .iter_relationships()
            .find(|rel| {
                if !matches!(rel.rel_type, RelationshipType::Calls) {
                    return false;
                }
                let Some(source) = result.graph.get_node(&rel.source_id) else {
                    return false;
                };
                let Some(target) = result.graph.get_node(&rel.target_id) else {
                    return false;
                };
                source.properties.name == "load"
                    && target.properties.file_path == "api.ts"
                    && target.properties.name == "directTask"
            })
            .expect("wildcard js shim should expose named object shorthand member");
        assert_eq!(direct_call.reason, "named-object:api:directTask");

        cleanup(&dir);
    }

    #[tokio::test]
    async fn test_pipeline_typescript_separate_named_object_export_resolves_members() {
        let dir = create_test_dir();
        fs::write(
            dir.join("api.ts"),
            r#"function hiddenTask() { return 1; }
export function directTask() { return 2; }
class Service {
  static create() { return 3; }
}

const api: {
  runTask(): number;
  directTask(): number;
  Service: typeof Service;
} = {
  runTask: hiddenTask,
  directTask,
  Service,
};

export { api as publicApi };
export default api;
"#,
        )
        .unwrap();
        fs::write(
            dir.join("app.ts"),
            r#"import defaultApi, { publicApi } from "./api";

export function load() {
  publicApi.runTask();
  defaultApi.directTask();
  return publicApi.Service.create();
}
"#,
        )
        .unwrap();

        let result = run_pipeline(
            &dir,
            None,
            PipelineOptions {
                skip_git: true,
                ..Default::default()
            },
        )
        .await
        .unwrap();

        let run_task_call = result
            .graph
            .iter_relationships()
            .find(|rel| {
                if !matches!(rel.rel_type, RelationshipType::Calls) {
                    return false;
                }
                let Some(source) = result.graph.get_node(&rel.source_id) else {
                    return false;
                };
                let Some(target) = result.graph.get_node(&rel.target_id) else {
                    return false;
                };
                source.properties.name == "load"
                    && target.properties.file_path == "api.ts"
                    && target.properties.name == "hiddenTask"
            })
            .expect("separate named object export should expose aliased member");
        assert_eq!(run_task_call.reason, "named-object:publicApi:runTask");

        let default_call = result
            .graph
            .iter_relationships()
            .find(|rel| {
                if !matches!(rel.rel_type, RelationshipType::Calls) {
                    return false;
                }
                let Some(source) = result.graph.get_node(&rel.source_id) else {
                    return false;
                };
                let Some(target) = result.graph.get_node(&rel.target_id) else {
                    return false;
                };
                source.properties.name == "load"
                    && target.properties.file_path == "api.ts"
                    && target.properties.name == "directTask"
            })
            .expect("separate default object export should expose shorthand member");
        assert_eq!(default_call.reason, "default-object:defaultApi:directTask");

        let static_call = result
            .graph
            .iter_relationships()
            .find(|rel| {
                if !matches!(rel.rel_type, RelationshipType::Calls) {
                    return false;
                }
                let Some(source) = result.graph.get_node(&rel.source_id) else {
                    return false;
                };
                let Some(target) = result.graph.get_node(&rel.target_id) else {
                    return false;
                };
                source.properties.name == "load"
                    && target.properties.file_path == "api.ts"
                    && target.properties.name == "create"
            })
            .expect("separate named object export should expose static class member");
        assert_eq!(
            static_call.reason,
            "named-object-static:publicApi:Service::create"
        );

        cleanup(&dir);
    }

    #[tokio::test]
    async fn test_pipeline_typescript_default_re_export_disambiguates_calls() {
        let dir = create_test_dir();
        fs::write(
            dir.join("a.ts"),
            "export default function runTask() { return 1; }\n",
        )
        .unwrap();
        fs::write(dir.join("c.ts"), "export function task() { return 2; }\n").unwrap();
        fs::write(
            dir.join("index.ts"),
            "export { default as task } from \"./a\";\n",
        )
        .unwrap();
        fs::write(
            dir.join("b.ts"),
            "import { task } from \"./index\";\nexport function load() { return task(); }\n",
        )
        .unwrap();

        let result = run_pipeline(
            &dir,
            None,
            PipelineOptions {
                skip_git: true,
                ..Default::default()
            },
        )
        .await
        .unwrap();

        let call = result
            .graph
            .iter_relationships()
            .find(|rel| {
                if !matches!(rel.rel_type, RelationshipType::Calls) {
                    return false;
                }
                let Some(source) = result.graph.get_node(&rel.source_id) else {
                    return false;
                };
                let Some(target) = result.graph.get_node(&rel.target_id) else {
                    return false;
                };
                source.properties.name == "load" && target.properties.name == "runTask"
            })
            .expect("load should call the re-exported default runTask symbol");

        let target = result.graph.get_node(&call.target_id).unwrap();
        assert!(
            target.properties.file_path.ends_with("a.ts"),
            "expected default re-export in a.ts, got {} via {}",
            target.properties.file_path,
            call.reason
        );
        assert_eq!(call.reason, "named-import");

        cleanup(&dir);
    }

    #[tokio::test]
    async fn test_pipeline_typescript_wildcard_re_export_resolves_receiver_type() {
        let dir = create_test_dir();
        fs::write(
            dir.join("service.ts"),
            "export class Service { run() { return 1; } }\n",
        )
        .unwrap();
        fs::write(
            dir.join("other.ts"),
            "export class Other { run() { return 2; } }\n",
        )
        .unwrap();
        fs::write(dir.join("index.ts"), "export * from \"./service\";\n").unwrap();
        fs::write(
            dir.join("main.ts"),
            "import { Service } from \"./index\";\nexport function load() {\n  const svc = new Service();\n  return svc.run();\n}\n",
        )
        .unwrap();

        let result = run_pipeline(
            &dir,
            None,
            PipelineOptions {
                skip_git: true,
                ..Default::default()
            },
        )
        .await
        .unwrap();

        let call = result
            .graph
            .iter_relationships()
            .find(|rel| {
                if !matches!(rel.rel_type, RelationshipType::Calls) {
                    return false;
                }
                let Some(source) = result.graph.get_node(&rel.source_id) else {
                    return false;
                };
                let Some(target) = result.graph.get_node(&rel.target_id) else {
                    return false;
                };
                source.properties.name == "load" && target.properties.name == "run"
            })
            .expect("load should call wildcard re-exported Service.run");

        let target = result.graph.get_node(&call.target_id).unwrap();
        assert!(
            target.properties.file_path.ends_with("service.ts"),
            "expected wildcard re-exported Service.run, got {} via {}",
            target.properties.file_path,
            call.reason
        );
        assert!(call.reason.starts_with("receiver-type:svc:Service:run"));

        cleanup(&dir);
    }

    #[tokio::test]
    async fn test_pipeline_typescript_namespace_destructuring_disambiguates_alias_call() {
        let dir = create_test_dir();
        fs::write(dir.join("api.ts"), "export function run() { return 1; }\n").unwrap();
        fs::write(
            dir.join("other.ts"),
            "export function execute() { return 2; }\n",
        )
        .unwrap();
        fs::write(
            dir.join("main.ts"),
            "import * as api from \"./api.js\";\nimport * as other from \"./other.js\";\nexport function load() {\n  const { run: execute } = api;\n  return execute();\n}\n",
        )
        .unwrap();

        let result = run_pipeline(
            &dir,
            None,
            PipelineOptions {
                skip_git: true,
                ..Default::default()
            },
        )
        .await
        .unwrap();

        let call = result
            .graph
            .iter_relationships()
            .find(|rel| {
                if !matches!(rel.rel_type, RelationshipType::Calls) {
                    return false;
                }
                let Some(source) = result.graph.get_node(&rel.source_id) else {
                    return false;
                };
                let Some(target) = result.graph.get_node(&rel.target_id) else {
                    return false;
                };
                source.properties.name == "load" && target.properties.name == "run"
            })
            .expect("load should call the destructured namespace member");

        let target = result.graph.get_node(&call.target_id).unwrap();
        assert!(
            target.properties.file_path.ends_with("api.ts"),
            "expected namespace destructuring to resolve api.ts run, got {} via {}",
            target.properties.file_path,
            call.reason
        );
        assert_eq!(call.reason, "named-import");

        let bad_alias_call = result.graph.iter_relationships().find(|rel| {
            if !matches!(rel.rel_type, RelationshipType::Calls) {
                return false;
            }
            let Some(source) = result.graph.get_node(&rel.source_id) else {
                return false;
            };
            let Some(target) = result.graph.get_node(&rel.target_id) else {
                return false;
            };
            source.properties.name == "load" && target.properties.name == "execute"
        });
        assert!(
            bad_alias_call.is_none(),
            "destructured namespace alias should not resolve to a same-named import"
        );

        cleanup(&dir);
    }

    #[tokio::test]
    async fn test_pipeline_typescript_namespace_member_alias_disambiguates_call() {
        let dir = create_test_dir();
        fs::write(dir.join("api.ts"), "export function run() { return 1; }\n").unwrap();
        fs::write(
            dir.join("other.ts"),
            "export function execute() { return 2; }\n",
        )
        .unwrap();
        fs::write(
            dir.join("main.ts"),
            "import * as api from \"./api.js\";\nimport * as other from \"./other.js\";\nconst execute = api.run;\nexport function load() { return execute(); }\n",
        )
        .unwrap();

        let result = run_pipeline(
            &dir,
            None,
            PipelineOptions {
                skip_git: true,
                ..Default::default()
            },
        )
        .await
        .unwrap();

        let call = result
            .graph
            .iter_relationships()
            .find(|rel| {
                if !matches!(rel.rel_type, RelationshipType::Calls) {
                    return false;
                }
                let Some(source) = result.graph.get_node(&rel.source_id) else {
                    return false;
                };
                let Some(target) = result.graph.get_node(&rel.target_id) else {
                    return false;
                };
                source.properties.name == "load" && target.properties.name == "run"
            })
            .expect("load should call the namespace member alias target");

        let target = result.graph.get_node(&call.target_id).unwrap();
        assert!(
            target.properties.file_path.ends_with("api.ts"),
            "expected namespace member alias to resolve api.ts run, got {} via {}",
            target.properties.file_path,
            call.reason
        );
        assert_eq!(call.reason, "named-import");

        let bad_alias_call = result.graph.iter_relationships().find(|rel| {
            if !matches!(rel.rel_type, RelationshipType::Calls) {
                return false;
            }
            let Some(source) = result.graph.get_node(&rel.source_id) else {
                return false;
            };
            let Some(target) = result.graph.get_node(&rel.target_id) else {
                return false;
            };
            source.properties.name == "load" && target.properties.name == "execute"
        });
        assert!(
            bad_alias_call.is_none(),
            "namespace member alias should not resolve to a same-named import"
        );

        cleanup(&dir);
    }

    #[tokio::test]
    async fn test_pipeline_typescript_dynamic_import_disambiguates_calls() {
        let dir = create_test_dir();
        fs::write(dir.join("a.ts"), "export function foo() { return 1; }\n").unwrap();
        fs::write(dir.join("c.ts"), "export function foo() { return 2; }\n").unwrap();
        fs::write(
            dir.join("b.ts"),
            "export async function load() {\n  const { foo } = await import(\"./a.js\");\n  return foo();\n}\n",
        )
        .unwrap();

        let result = run_pipeline(
            &dir,
            None,
            PipelineOptions {
                skip_git: true,
                ..Default::default()
            },
        )
        .await
        .unwrap();

        let call = result
            .graph
            .iter_relationships()
            .find(|rel| {
                if !matches!(rel.rel_type, RelationshipType::Calls) {
                    return false;
                }
                let Some(source) = result.graph.get_node(&rel.source_id) else {
                    return false;
                };
                let Some(target) = result.graph.get_node(&rel.target_id) else {
                    return false;
                };
                source.properties.name == "load" && target.properties.name == "foo"
            })
            .expect("load should call dynamically imported foo");

        let target = result.graph.get_node(&call.target_id).unwrap();
        assert!(
            target.properties.file_path.ends_with("a.ts"),
            "expected dynamically imported a.ts foo, got {} via {}",
            target.properties.file_path,
            call.reason
        );
        assert_eq!(call.reason, "named-import");
        assert!(call.confidence >= 0.8);

        cleanup(&dir);
    }

    #[tokio::test]
    async fn test_pipeline_typescript_dynamic_import_then_disambiguates_calls() {
        let dir = create_test_dir();
        fs::write(dir.join("a.ts"), "export function foo() { return 1; }\n").unwrap();
        fs::write(dir.join("c.ts"), "export function foo() { return 2; }\n").unwrap();
        fs::write(
            dir.join("b.ts"),
            r#"export function load() {
  import("./a.js").then(({ foo }) => {
    return foo();
  });
}
"#,
        )
        .unwrap();

        let result = run_pipeline(
            &dir,
            None,
            PipelineOptions {
                skip_git: true,
                ..Default::default()
            },
        )
        .await
        .unwrap();

        let call = result
            .graph
            .iter_relationships()
            .find(|rel| {
                if !matches!(rel.rel_type, RelationshipType::Calls) {
                    return false;
                }
                let Some(source) = result.graph.get_node(&rel.source_id) else {
                    return false;
                };
                let Some(target) = result.graph.get_node(&rel.target_id) else {
                    return false;
                };
                source.properties.name == "load" && target.properties.name == "foo"
            })
            .expect("load should call dynamically imported foo from .then callback");

        let target = result.graph.get_node(&call.target_id).unwrap();
        assert!(
            target.properties.file_path.ends_with("a.ts"),
            "expected dynamically imported a.ts foo, got {} via {}",
            target.properties.file_path,
            call.reason
        );
        assert_eq!(call.reason, "named-import");

        cleanup(&dir);
    }

    #[tokio::test]
    async fn test_pipeline_typescript_promise_all_dynamic_import_disambiguates_calls() {
        let dir = create_test_dir();
        fs::write(
            dir.join("settings.ts"),
            "export function getSettingsManager() { return 1; }\n",
        )
        .unwrap();
        fs::write(
            dir.join("learning.ts"),
            "export function buildLearningRetrospective(_runId: string) { return 2; }\n",
        )
        .unwrap();
        fs::write(
            dir.join("other.ts"),
            "export function getSettingsManager() { return 3; }\nexport function buildLearningRetrospective() { return 4; }\n",
        )
        .unwrap();
        fs::write(
            dir.join("main.ts"),
            r#"export async function load() {
  const [{ getSettingsManager }, { buildLearningRetrospective: buildRetro }] =
    await Promise.all([
      import("./settings.js"),
      import("./learning.js"),
    ]);
  return getSettingsManager() + buildRetro("run");
}
"#,
        )
        .unwrap();

        let result = run_pipeline(
            &dir,
            None,
            PipelineOptions {
                skip_git: true,
                ..Default::default()
            },
        )
        .await
        .unwrap();

        for (target_name, target_file) in [
            ("getSettingsManager", "settings.ts"),
            ("buildLearningRetrospective", "learning.ts"),
        ] {
            let call = result
                .graph
                .iter_relationships()
                .find(|rel| {
                    if !matches!(rel.rel_type, RelationshipType::Calls) {
                        return false;
                    }
                    let Some(source) = result.graph.get_node(&rel.source_id) else {
                        return false;
                    };
                    let Some(target) = result.graph.get_node(&rel.target_id) else {
                        return false;
                    };
                    source.properties.name == "load" && target.properties.name == target_name
                })
                .unwrap_or_else(|| panic!("load should call Promise.all import {target_name}"));

            let target = result.graph.get_node(&call.target_id).unwrap();
            assert!(
                target.properties.file_path.ends_with(target_file),
                "expected Promise.all import to resolve {target_file} {target_name}, got {} via {}",
                target.properties.file_path,
                call.reason
            );
            assert_eq!(call.reason, "named-import");
        }

        cleanup(&dir);
    }

    #[tokio::test]
    async fn test_pipeline_typescript_dynamic_import_return_factory_disambiguates_calls() {
        let dir = create_test_dir();
        fs::write(
            dir.join("channels.ts"),
            "export function enqueueMessage(_session: string) { return 1; }\n",
        )
        .unwrap();
        fs::write(
            dir.join("other.ts"),
            "export function enqueueMessage(_session: string) { return 2; }\n",
        )
        .unwrap();
        fs::write(
            dir.join("main.ts"),
            r#"let _enqueueMessage: typeof import("./channels.js").enqueueMessage;

async function getEnqueueMessage() {
  if (!_enqueueMessage) {
    const mod = await import("./channels.js");
    _enqueueMessage = mod.enqueueMessage;
  }
  return _enqueueMessage;
}

export async function load() {
  const enqueueMessage = await getEnqueueMessage();
  return enqueueMessage("session");
}
"#,
        )
        .unwrap();

        let result = run_pipeline(
            &dir,
            None,
            PipelineOptions {
                skip_git: true,
                ..Default::default()
            },
        )
        .await
        .unwrap();

        let call = result
            .graph
            .iter_relationships()
            .find(|rel| {
                if !matches!(rel.rel_type, RelationshipType::Calls) {
                    return false;
                }
                let Some(source) = result.graph.get_node(&rel.source_id) else {
                    return false;
                };
                let Some(target) = result.graph.get_node(&rel.target_id) else {
                    return false;
                };
                source.properties.name == "load" && target.properties.name == "enqueueMessage"
            })
            .expect("load should call dynamic import return factory result");

        let target = result.graph.get_node(&call.target_id).unwrap();
        assert!(
            target.properties.file_path.ends_with("channels.ts"),
            "expected dynamic import return factory to resolve channels.ts enqueueMessage, got {} via {}",
            target.properties.file_path,
            call.reason
        );
        assert_eq!(call.reason, "named-import");

        cleanup(&dir);
    }

    #[tokio::test]
    async fn test_pipeline_typescript_for_of_function_variable_does_not_use_import_scoped_fallback()
    {
        let dir = create_test_dir();
        fs::write(
            dir.join("watchers.ts"),
            "export function resetSkillRegistry() { return 1; }\nexport function cleanup() { return 2; }\n",
        )
        .unwrap();
        fs::write(
            dir.join("main.ts"),
            r#"import { resetSkillRegistry } from "./watchers.js";

export function dispose() {
  for (const cleanup of [
    resetSkillRegistry,
  ]) {
    cleanup();
  }
}
"#,
        )
        .unwrap();

        let result = run_pipeline(
            &dir,
            None,
            PipelineOptions {
                skip_git: true,
                ..Default::default()
            },
        )
        .await
        .unwrap();

        let bad_import_scoped_call = result.graph.iter_relationships().find(|rel| {
            if !matches!(rel.rel_type, RelationshipType::Calls) || rel.reason != "import-scoped" {
                return false;
            }
            let Some(source) = result.graph.get_node(&rel.source_id) else {
                return false;
            };
            let Some(target) = result.graph.get_node(&rel.target_id) else {
                return false;
            };
            source.properties.name == "dispose"
                && target.properties.name == "cleanup"
                && target.properties.file_path.ends_with("watchers.ts")
        });

        assert!(
            bad_import_scoped_call.is_none(),
            "for-of function variables should not resolve to unrelated imported cleanup symbols"
        );

        cleanup(&dir);
    }

    #[tokio::test]
    async fn test_pipeline_typescript_bound_method_alias_disambiguates_calls() {
        let dir = create_test_dir();
        fs::write(
            dir.join("listener.ts"),
            r#"export class FleetListener {
  async invokeTool(_name: string) { return 1; }
  async invokeToolStream(_name: string) { return 2; }
}
"#,
        )
        .unwrap();
        fs::write(
            dir.join("other.ts"),
            "export function invokeTool(_name: string) { return 3; }\nexport function invokeToolStream(_name: string) { return 4; }\n",
        )
        .unwrap();
        fs::write(
            dir.join("main.ts"),
            r#"import { FleetListener } from "./listener.js";

export async function handleTool(target: { listener: FleetListener }, stream: boolean) {
  const callTool = target.listener.invokeTool?.bind(target.listener);
  const streamTool = target.listener.invokeToolStream?.bind(target.listener);
  if (stream) {
    return streamTool("search");
  }
  return callTool("search");
}
"#,
        )
        .unwrap();

        let result = run_pipeline(
            &dir,
            None,
            PipelineOptions {
                skip_git: true,
                ..Default::default()
            },
        )
        .await
        .unwrap();

        for target_name in ["invokeTool", "invokeToolStream"] {
            let call = result
                .graph
                .iter_relationships()
                .find(|rel| {
                    if !matches!(rel.rel_type, RelationshipType::Calls) {
                        return false;
                    }
                    let Some(source) = result.graph.get_node(&rel.source_id) else {
                        return false;
                    };
                    let Some(target) = result.graph.get_node(&rel.target_id) else {
                        return false;
                    };
                    source.properties.name == "handleTool" && target.properties.name == target_name
                })
                .unwrap_or_else(|| panic!("handleTool should call bound {target_name}"));

            let target = result.graph.get_node(&call.target_id).unwrap();
            assert!(
                target.properties.file_path.ends_with("listener.ts"),
                "expected bound method alias to resolve listener.ts {target_name}, got {} via {}",
                target.properties.file_path,
                call.reason
            );
            assert!(
                call.reason.starts_with("bound-method:target.listener:"),
                "expected bound-method reason, got {}",
                call.reason
            );
        }

        let bad_import_scoped_call = result.graph.iter_relationships().find(|rel| {
            if !matches!(rel.rel_type, RelationshipType::Calls) || rel.reason != "import-scoped" {
                return false;
            }
            let Some(source) = result.graph.get_node(&rel.source_id) else {
                return false;
            };
            let Some(target) = result.graph.get_node(&rel.target_id) else {
                return false;
            };
            source.properties.name == "handleTool"
                && matches!(
                    target.properties.name.as_str(),
                    "invokeTool" | "invokeToolStream"
                )
        });

        assert!(
            bad_import_scoped_call.is_none(),
            "bound method aliases should not fall back to import-scoped calls"
        );

        cleanup(&dir);
    }

    #[tokio::test]
    async fn test_pipeline_typescript_non_null_assertion_calls_are_extracted() {
        let dir = create_test_dir();
        fs::write(
            dir.join("helper.ts"),
            r#"export class Helper {
  run(_value: string) { return "ok"; }
}
"#,
        )
        .unwrap();
        fs::write(
            dir.join("main.ts"),
            r#"import { Helper } from "./helper.js";

function cb() { return "wrong"; }

export function load(helper: Helper, cbParam?: () => void) {
  cbParam!();
  return helper.run!("value");
}
"#,
        )
        .unwrap();

        let result = run_pipeline(
            &dir,
            None,
            PipelineOptions {
                skip_git: true,
                ..Default::default()
            },
        )
        .await
        .unwrap();

        let run_call = result
            .graph
            .iter_relationships()
            .find(|rel| {
                if !matches!(rel.rel_type, RelationshipType::Calls) {
                    return false;
                }
                let Some(source) = result.graph.get_node(&rel.source_id) else {
                    return false;
                };
                let Some(target) = result.graph.get_node(&rel.target_id) else {
                    return false;
                };
                source.properties.name == "load" && target.properties.name == "run"
            })
            .expect("load should call helper.run through a non-null assertion");

        let run_target = result.graph.get_node(&run_call.target_id).unwrap();
        assert!(
            run_target.properties.file_path.ends_with("helper.ts"),
            "expected helper.run!() to resolve helper.ts run, got {} via {}",
            run_target.properties.file_path,
            run_call.reason
        );
        assert!(
            run_call
                .reason
                .starts_with("receiver-type:helper:Helper:run"),
            "expected receiver-type reason, got {}",
            run_call.reason
        );

        let bad_callback_call = result.graph.iter_relationships().find(|rel| {
            if !matches!(rel.rel_type, RelationshipType::Calls) {
                return false;
            }
            let Some(source) = result.graph.get_node(&rel.source_id) else {
                return false;
            };
            let Some(target) = result.graph.get_node(&rel.target_id) else {
                return false;
            };
            source.properties.name == "load" && target.properties.name == "cb"
        });

        assert!(
            bad_callback_call.is_none(),
            "callback parameters called via non-null assertion should not resolve to same-file homonyms"
        );

        cleanup(&dir);
    }

    #[tokio::test]
    async fn test_pipeline_typescript_imported_call_result_disambiguates_calls() {
        let dir = create_test_dir();
        fs::write(
            dir.join("tools.ts"),
            r#"export function run() { return 1; }
export function close() { return 2; }
export function useTools() {
  return { run, close };
}
"#,
        )
        .unwrap();
        fs::write(
            dir.join("other.ts"),
            "export function run() { return 3; }\nexport function close() { return 4; }\n",
        )
        .unwrap();
        fs::write(
            dir.join("main.ts"),
            r#"import { useTools } from "./tools.js";

export function load() {
  const { run, close: stop } = useTools();
  return run() + stop();
}
"#,
        )
        .unwrap();

        let result = run_pipeline(
            &dir,
            None,
            PipelineOptions {
                skip_git: true,
                ..Default::default()
            },
        )
        .await
        .unwrap();

        for target_name in ["run", "close"] {
            let call = result
                .graph
                .iter_relationships()
                .find(|rel| {
                    if !matches!(rel.rel_type, RelationshipType::Calls) {
                        return false;
                    }
                    let Some(source) = result.graph.get_node(&rel.source_id) else {
                        return false;
                    };
                    let Some(target) = result.graph.get_node(&rel.target_id) else {
                        return false;
                    };
                    source.properties.name == "load" && target.properties.name == target_name
                })
                .unwrap_or_else(|| panic!("load should call imported call result {target_name}"));

            let target = result.graph.get_node(&call.target_id).unwrap();
            assert!(
                target.properties.file_path.ends_with("tools.ts"),
                "expected imported call result to resolve tools.ts {target_name}, got {} via {}",
                target.properties.file_path,
                call.reason
            );
            assert_eq!(call.reason, "named-import");
        }

        cleanup(&dir);
    }

    #[tokio::test]
    async fn test_pipeline_typescript_lazy_dynamic_import_factory_disambiguates_calls() {
        let dir = create_test_dir();
        fs::create_dir_all(dir.join("renderers")).unwrap();
        fs::write(
            dir.join("renderers").join("index.ts"),
            "export function initializeRenderers() { return 1; }\n",
        )
        .unwrap();
        fs::create_dir_all(dir.join("utils")).unwrap();
        fs::write(
            dir.join("utils").join("settings-manager.ts"),
            "export function getSettingsManager() { return 1; }\n",
        )
        .unwrap();
        fs::write(
            dir.join("other.ts"),
            "export function initializeRenderers() { return 2; }\nexport function getSettingsManager() { return 2; }\n",
        )
        .unwrap();
        fs::write(
            dir.join("main.ts"),
            r#"async function lazyLoad(_key: string, loader: () => Promise<unknown>) {
  return loader();
}

export async function load() {
  const lazyImport = {
    renderers: () => lazyLoad("renderers", () => import("./renderers/index.js")),
    settingsManager: () => lazyLoad("settingsManager", () => import("./utils/settings-manager.js").then(m => m.getSettingsManager)),
  };
  const { initializeRenderers } = await lazyImport.renderers();
  const getSettingsManager = await lazyImport.settingsManager();
  return initializeRenderers() + getSettingsManager();
}
"#,
        )
        .unwrap();

        let result = run_pipeline(
            &dir,
            None,
            PipelineOptions {
                skip_git: true,
                ..Default::default()
            },
        )
        .await
        .unwrap();

        let call = result
            .graph
            .iter_relationships()
            .find(|rel| {
                if !matches!(rel.rel_type, RelationshipType::Calls) {
                    return false;
                }
                let Some(source) = result.graph.get_node(&rel.source_id) else {
                    return false;
                };
                let Some(target) = result.graph.get_node(&rel.target_id) else {
                    return false;
                };
                source.properties.name == "load" && target.properties.name == "initializeRenderers"
            })
            .expect("load should call lazy dynamically imported initializeRenderers");

        let target = result.graph.get_node(&call.target_id).unwrap();
        assert!(
            target.properties.file_path.ends_with("renderers/index.ts"),
            "expected lazy dynamic import to resolve renderers/index.ts, got {} via {}",
            target.properties.file_path,
            call.reason
        );
        assert_eq!(call.reason, "named-import");

        let settings_call = result
            .graph
            .iter_relationships()
            .find(|rel| {
                if !matches!(rel.rel_type, RelationshipType::Calls) {
                    return false;
                }
                let Some(source) = result.graph.get_node(&rel.source_id) else {
                    return false;
                };
                let Some(target) = result.graph.get_node(&rel.target_id) else {
                    return false;
                };
                source.properties.name == "load" && target.properties.name == "getSettingsManager"
            })
            .expect("load should call named export returned by lazy dynamic import factory");

        let settings_target = result.graph.get_node(&settings_call.target_id).unwrap();
        assert!(
            settings_target
                .properties
                .file_path
                .ends_with("utils/settings-manager.ts"),
            "expected lazy dynamic import function factory to resolve utils/settings-manager.ts, got {} via {}",
            settings_target.properties.file_path,
            settings_call.reason
        );
        assert_eq!(settings_call.reason, "named-import");

        cleanup(&dir);
    }

    #[tokio::test]
    async fn test_pipeline_typescript_tsconfig_paths_disambiguates_calls() {
        let dir = create_test_dir();
        fs::create_dir_all(dir.join("src/lib")).unwrap();
        fs::write(
            dir.join("tsconfig.json"),
            r#"{
  "compilerOptions": {
    "baseUrl": ".",
    "paths": {
      "@lib/*": ["src/lib/*"]
    }
  }
}"#,
        )
        .unwrap();
        fs::write(
            dir.join("src/lib/a.ts"),
            "export function foo() { return 1; }\n",
        )
        .unwrap();
        fs::write(
            dir.join("src/lib/c.ts"),
            "export function foo() { return 2; }\n",
        )
        .unwrap();
        fs::write(
            dir.join("src/b.ts"),
            "import { foo } from \"@lib/a\";\nexport function load() { return foo(); }\n",
        )
        .unwrap();

        let result = run_pipeline(
            &dir,
            None,
            PipelineOptions {
                skip_git: true,
                ..Default::default()
            },
        )
        .await
        .unwrap();

        let call = result
            .graph
            .iter_relationships()
            .find(|rel| {
                if !matches!(rel.rel_type, RelationshipType::Calls) {
                    return false;
                }
                let Some(source) = result.graph.get_node(&rel.source_id) else {
                    return false;
                };
                let Some(target) = result.graph.get_node(&rel.target_id) else {
                    return false;
                };
                source.properties.name == "load" && target.properties.name == "foo"
            })
            .expect("load should call aliased foo");

        let target = result.graph.get_node(&call.target_id).unwrap();
        assert!(
            target.properties.file_path.ends_with("src/lib/a.ts"),
            "expected tsconfig alias to resolve src/lib/a.ts, got {} via {}",
            target.properties.file_path,
            call.reason
        );
        assert_eq!(call.reason, "named-import");

        cleanup(&dir);
    }

    #[tokio::test]
    async fn test_pipeline_typescript_parent_tsconfig_paths_when_indexing_src() {
        let dir = create_test_dir();
        let src = dir.join("src");
        fs::create_dir_all(src.join("lib")).unwrap();
        fs::write(
            dir.join("tsconfig.json"),
            r#"{
  "compilerOptions": {
    "baseUrl": ".",
    "paths": {
      "@lib/*": ["src/lib/*"]
    }
  }
}"#,
        )
        .unwrap();
        fs::write(
            src.join("lib/a.ts"),
            "export function foo() { return 1; }\n",
        )
        .unwrap();
        fs::write(
            src.join("lib/c.ts"),
            "export function foo() { return 2; }\n",
        )
        .unwrap();
        fs::write(
            src.join("b.ts"),
            "import { foo } from \"@lib/a\";\nexport function load() { return foo(); }\n",
        )
        .unwrap();

        let result = run_pipeline(
            &src,
            None,
            PipelineOptions {
                skip_git: true,
                ..Default::default()
            },
        )
        .await
        .unwrap();

        let call = result
            .graph
            .iter_relationships()
            .find(|rel| {
                if !matches!(rel.rel_type, RelationshipType::Calls) {
                    return false;
                }
                let Some(source) = result.graph.get_node(&rel.source_id) else {
                    return false;
                };
                let Some(target) = result.graph.get_node(&rel.target_id) else {
                    return false;
                };
                source.properties.name == "load" && target.properties.name == "foo"
            })
            .expect("load should call aliased foo from parent tsconfig");

        let target = result.graph.get_node(&call.target_id).unwrap();
        assert!(
            target.properties.file_path.ends_with("lib/a.ts"),
            "expected parent tsconfig alias to resolve lib/a.ts, got {} via {}",
            target.properties.file_path,
            call.reason
        );
        assert_eq!(call.reason, "named-import");

        cleanup(&dir);
    }

    #[tokio::test]
    async fn test_pipeline_typescript_namespace_import_resolves_member_call() {
        let dir = create_test_dir();
        fs::write(dir.join("api.ts"), "export function foo() { return 1; }\n").unwrap();
        fs::write(
            dir.join("other.ts"),
            "export function foo() { return 2; }\n",
        )
        .unwrap();
        fs::write(
            dir.join("b.ts"),
            "import * as api from \"./api\";\nexport function load() { return api.foo(); }\n",
        )
        .unwrap();

        let result = run_pipeline(
            &dir,
            None,
            PipelineOptions {
                skip_git: true,
                ..Default::default()
            },
        )
        .await
        .unwrap();

        let call = result
            .graph
            .iter_relationships()
            .find(|rel| {
                if !matches!(rel.rel_type, RelationshipType::Calls) {
                    return false;
                }
                let Some(source) = result.graph.get_node(&rel.source_id) else {
                    return false;
                };
                let Some(target) = result.graph.get_node(&rel.target_id) else {
                    return false;
                };
                source.properties.name == "load" && target.properties.name == "foo"
            })
            .expect("load should call namespace-imported foo");

        let target = result.graph.get_node(&call.target_id).unwrap();
        assert!(
            target.properties.file_path.ends_with("api.ts"),
            "expected namespace import to resolve api.ts, got {} via {}",
            target.properties.file_path,
            call.reason
        );
        assert!(call.reason.starts_with("module-alias:api:foo"));

        cleanup(&dir);
    }

    #[tokio::test]
    async fn test_pipeline_typescript_tsx_component_parses_jsx() {
        let dir = create_test_dir();
        fs::write(
            dir.join("App.tsx"),
            r#"import { Remote } from "./Remote.js";

function Local() { return <span />; }
function main() { return null; }

export function App() {
  return (
    <>
      <Local />
      <Remote />
      <main />
    </>
  );
}
"#,
        )
        .unwrap();
        fs::write(
            dir.join("Remote.tsx"),
            "export function Remote() { return <span />; }\n",
        )
        .unwrap();

        let result = run_pipeline(
            &dir,
            None,
            PipelineOptions {
                skip_git: true,
                ..Default::default()
            },
        )
        .await
        .unwrap();

        let graph = &result.graph;
        assert!(
            graph
                .iter_nodes()
                .any(|node| { node.label == NodeLabel::Function && node.properties.name == "App" }),
            "TSX parser should extract App function from a component with JSX"
        );

        for (target_name, target_file) in [("Local", "App.tsx"), ("Remote", "Remote.tsx")] {
            let call = graph
                .iter_relationships()
                .find(|rel| {
                    if !matches!(rel.rel_type, RelationshipType::Calls) {
                        return false;
                    }
                    let Some(source) = graph.get_node(&rel.source_id) else {
                        return false;
                    };
                    let Some(target) = graph.get_node(&rel.target_id) else {
                        return false;
                    };
                    source.properties.name == "App" && target.properties.name == target_name
                })
                .unwrap_or_else(|| panic!("App should render JSX component {target_name}"));

            let target = graph.get_node(&call.target_id).unwrap();
            assert!(
                target.properties.file_path.ends_with(target_file),
                "expected JSX component {target_name} to resolve {target_file}, got {} via {}",
                target.properties.file_path,
                call.reason
            );
        }

        let bad_intrinsic_call = graph.iter_relationships().find(|rel| {
            if !matches!(rel.rel_type, RelationshipType::Calls) {
                return false;
            }
            let Some(source) = graph.get_node(&rel.source_id) else {
                return false;
            };
            let Some(target) = graph.get_node(&rel.target_id) else {
                return false;
            };
            source.properties.name == "App" && target.properties.name == "main"
        });

        assert!(
            bad_intrinsic_call.is_none(),
            "lowercase JSX intrinsic tags should not resolve to same-file functions"
        );

        cleanup(&dir);
    }

    #[tokio::test]
    async fn test_pipeline_typescript_react_memo_component_has_node_for_calls() {
        let dir = create_test_dir();
        fs::write(
            dir.join("Panel.tsx"),
            r#"import React from "react";

function helper() { return null; }

export const Panel = React.memo(function Panel() {
  return helper();
});
"#,
        )
        .unwrap();

        let result = run_pipeline(
            &dir,
            None,
            PipelineOptions {
                skip_git: true,
                ..Default::default()
            },
        )
        .await
        .unwrap();

        let panel = result
            .graph
            .iter_nodes()
            .find(|node| node.label == NodeLabel::Function && node.properties.name == "Panel")
            .expect("React.memo component should have a Function node");

        let call = result
            .graph
            .iter_relationships()
            .find(|rel| {
                if !matches!(rel.rel_type, RelationshipType::Calls) {
                    return false;
                }
                let Some(source) = result.graph.get_node(&rel.source_id) else {
                    return false;
                };
                let Some(target) = result.graph.get_node(&rel.target_id) else {
                    return false;
                };
                source.properties.name == "Panel" && target.properties.name == "helper"
            })
            .expect("Panel should call helper");

        assert_eq!(
            call.source_id, panel.id,
            "CALLS source should point at the real Panel node"
        );
        assert_eq!(call.reason, "same-file");

        cleanup(&dir);
    }

    #[tokio::test]
    async fn test_pipeline_typescript_react_memo_identifier_alias_has_public_node() {
        let dir = create_test_dir();
        fs::write(
            dir.join("Dialog.tsx"),
            r#"import React from "react";

function DialogInner() { return null; }

const Dialog = React.memo(DialogInner);
export default Dialog;
"#,
        )
        .unwrap();

        let result = run_pipeline(
            &dir,
            None,
            PipelineOptions {
                skip_git: true,
                ..Default::default()
            },
        )
        .await
        .unwrap();

        let dialog = result
            .graph
            .iter_nodes()
            .find(|node| node.label == NodeLabel::Function && node.properties.name == "Dialog")
            .expect("React.memo identifier alias should have a Function node");

        assert_eq!(dialog.properties.file_path, "Dialog.tsx");

        cleanup(&dir);
    }

    #[tokio::test]
    async fn test_pipeline_typescript_wrapped_callback_uses_callback_source() {
        let dir = create_test_dir();
        fs::write(
            dir.join("Component.tsx"),
            r#"import { useCallback } from "react";

function helper() { return null; }

export function Component() {
  const handleSubmit = useCallback(() => {
    return helper();
  }, []);

  return handleSubmit;
}
"#,
        )
        .unwrap();

        let result = run_pipeline(
            &dir,
            None,
            PipelineOptions {
                skip_git: true,
                ..Default::default()
            },
        )
        .await
        .unwrap();

        let handle_submit = result
            .graph
            .iter_nodes()
            .find(|node| {
                node.label == NodeLabel::Function && node.properties.name == "handleSubmit"
            })
            .expect("wrapped callback should have a Function node");

        let call = result
            .graph
            .iter_relationships()
            .find(|rel| {
                if !matches!(rel.rel_type, RelationshipType::Calls) {
                    return false;
                }
                let Some(source) = result.graph.get_node(&rel.source_id) else {
                    return false;
                };
                let Some(target) = result.graph.get_node(&rel.target_id) else {
                    return false;
                };
                source.properties.name == "handleSubmit" && target.properties.name == "helper"
            })
            .expect("handleSubmit should call helper");

        assert_eq!(
            call.source_id, handle_submit.id,
            "CALLS source should point at the wrapped callback, not Component"
        );

        cleanup(&dir);
    }

    #[tokio::test]
    async fn test_pipeline_typescript_object_property_functions_have_nodes_for_calls() {
        let dir = create_test_dir();
        fs::write(
            dir.join("tools.ts"),
            r#"function helper() { return 1; }

export const tools = {
  run: () => helper(),
  stop: function () {
    return helper();
  },
};
"#,
        )
        .unwrap();

        let result = run_pipeline(
            &dir,
            None,
            PipelineOptions {
                skip_git: true,
                ..Default::default()
            },
        )
        .await
        .unwrap();

        for name in ["run", "stop"] {
            let source_node = result
                .graph
                .iter_nodes()
                .find(|node| node.label == NodeLabel::Function && node.properties.name == name)
                .unwrap_or_else(|| panic!("object property function {name} should have a node"));

            let call = result
                .graph
                .iter_relationships()
                .find(|rel| {
                    if !matches!(rel.rel_type, RelationshipType::Calls) {
                        return false;
                    }
                    let Some(source) = result.graph.get_node(&rel.source_id) else {
                        return false;
                    };
                    let Some(target) = result.graph.get_node(&rel.target_id) else {
                        return false;
                    };
                    source.properties.name == name && target.properties.name == "helper"
                })
                .unwrap_or_else(|| panic!("{name} should call helper"));

            assert_eq!(
                call.source_id, source_node.id,
                "CALLS source should point at the object property function"
            );
        }

        cleanup(&dir);
    }

    #[tokio::test]
    async fn test_pipeline_typescript_class_field_function_is_callable_method() {
        let dir = create_test_dir();
        fs::write(
            dir.join("worker.ts"),
            "export class Worker {\n  run = () => 1;\n  start() { return this.run(); }\n}\n",
        )
        .unwrap();

        let result = run_pipeline(
            &dir,
            None,
            PipelineOptions {
                skip_git: true,
                ..Default::default()
            },
        )
        .await
        .unwrap();

        let run_method = result
            .graph
            .iter_nodes()
            .find(|node| node.label == NodeLabel::Method && node.properties.name == "run");
        assert!(
            run_method.is_some(),
            "class field arrow functions should be represented as callable methods"
        );

        let call = result
            .graph
            .iter_relationships()
            .find(|rel| {
                if !matches!(rel.rel_type, RelationshipType::Calls) {
                    return false;
                }
                let Some(source) = result.graph.get_node(&rel.source_id) else {
                    return false;
                };
                let Some(target) = result.graph.get_node(&rel.target_id) else {
                    return false;
                };
                source.properties.name == "start" && target.properties.name == "run"
            })
            .expect("start should call class field method run");

        let target = result.graph.get_node(&call.target_id).unwrap();
        assert_eq!(
            target.label,
            NodeLabel::Method,
            "expected start to call Method run, got {:?} via {}",
            target.label,
            call.reason
        );

        cleanup(&dir);
    }

    #[tokio::test]
    async fn test_pipeline_typescript_string_key_object_functions_have_nodes_for_calls() {
        let dir = create_test_dir();
        fs::write(
            dir.join("tools.ts"),
            r#"function helper() { return 1; }

export const tools = {
  "run-task": () => helper(),
  "stop-task": function () {
    return helper();
  },
};
"#,
        )
        .unwrap();

        let result = run_pipeline(
            &dir,
            None,
            PipelineOptions {
                skip_git: true,
                ..Default::default()
            },
        )
        .await
        .unwrap();

        for name in ["run-task", "stop-task"] {
            let source_node = result
                .graph
                .iter_nodes()
                .find(|node| node.label == NodeLabel::Function && node.properties.name == name)
                .unwrap_or_else(|| panic!("string-key object function {name} should have a node"));

            assert!(
                !source_node.id.contains('"'),
                "string-key function node id should not include quotes"
            );

            let call = result
                .graph
                .iter_relationships()
                .find(|rel| {
                    if !matches!(rel.rel_type, RelationshipType::Calls) {
                        return false;
                    }
                    let Some(source) = result.graph.get_node(&rel.source_id) else {
                        return false;
                    };
                    let Some(target) = result.graph.get_node(&rel.target_id) else {
                        return false;
                    };
                    source.properties.name == name && target.properties.name == "helper"
                })
                .unwrap_or_else(|| panic!("{name} should call helper"));

            assert_eq!(
                call.source_id, source_node.id,
                "CALLS source should point at the string-key object function"
            );
        }

        cleanup(&dir);
    }

    #[tokio::test]
    async fn test_pipeline_typescript_member_calls_do_not_fall_back_to_global_functions() {
        let dir = create_test_dir();
        fs::write(
            dir.join("helpers.ts"),
            "export function map(value: unknown) { return value; }\n\
             export function parse(value: string) { return value; }\n",
        )
        .unwrap();
        fs::write(
            dir.join("main.ts"),
            r#"export function load(items: string[], raw: string) {
  const mapped = items.map((item) => item);
  return JSON.parse(raw) ?? mapped;
}
"#,
        )
        .unwrap();

        let result = run_pipeline(
            &dir,
            None,
            PipelineOptions {
                skip_git: true,
                ..Default::default()
            },
        )
        .await
        .unwrap();

        let bad_global_call = result.graph.iter_relationships().find(|rel| {
            if !matches!(rel.rel_type, RelationshipType::Calls) || rel.reason != "global" {
                return false;
            }
            let Some(source) = result.graph.get_node(&rel.source_id) else {
                return false;
            };
            let Some(target) = result.graph.get_node(&rel.target_id) else {
                return false;
            };
            source.properties.name == "load"
                && matches!(target.properties.name.as_str(), "map" | "parse")
        });

        assert!(
            bad_global_call.is_none(),
            "TS member calls like items.map() and JSON.parse() should not resolve via global fallback"
        );

        cleanup(&dir);
    }

    #[tokio::test]
    async fn test_pipeline_typescript_import_scoped_properties_are_not_call_targets() {
        let dir = create_test_dir();
        fs::write(
            dir.join("types.ts"),
            "export interface Relationship { from: string; }\n\
             export interface SemanticMap { relationships: Map<string, Relationship>; }\n",
        )
        .unwrap();
        fs::write(
            dir.join("formatter.ts"),
            r#"import type { SemanticMap } from "./types.js";

export function formatMap(map: SemanticMap) {
  return Array.from(map.relationships.values()).length;
}
"#,
        )
        .unwrap();

        let result = run_pipeline(
            &dir,
            None,
            PipelineOptions {
                skip_git: true,
                ..Default::default()
            },
        )
        .await
        .unwrap();

        let bad_import_scoped_call = result.graph.iter_relationships().find(|rel| {
            if !matches!(rel.rel_type, RelationshipType::Calls) || rel.reason != "import-scoped" {
                return false;
            }
            let Some(source) = result.graph.get_node(&rel.source_id) else {
                return false;
            };
            let Some(target) = result.graph.get_node(&rel.target_id) else {
                return false;
            };
            source.properties.name == "formatMap"
                && target.label == NodeLabel::Property
                && target.properties.name == "from"
        });

        assert!(
            bad_import_scoped_call.is_none(),
            "TS fuzzy import-scoped fallback should not treat imported type properties as call targets"
        );

        cleanup(&dir);
    }

    #[tokio::test]
    async fn test_pipeline_typescript_member_calls_do_not_use_import_scoped_method_fallback() {
        let dir = create_test_dir();
        fs::write(
            dir.join("logger.ts"),
            r#"export class Logger {
  close() {
    return undefined;
  }
}

export const logger = {
  warn(message: string) {
    return message;
  },
};
"#,
        )
        .unwrap();
        fs::write(
            dir.join("main.ts"),
            r#"import { logger } from "./logger.js";

export function stop(server: { close(): void }) {
  logger.warn("stopping");
  server.close();
}
"#,
        )
        .unwrap();

        let result = run_pipeline(
            &dir,
            None,
            PipelineOptions {
                skip_git: true,
                ..Default::default()
            },
        )
        .await
        .unwrap();

        let logger_warn_call = result
            .graph
            .iter_relationships()
            .find(|rel| {
                if !matches!(rel.rel_type, RelationshipType::Calls) {
                    return false;
                }
                let Some(source) = result.graph.get_node(&rel.source_id) else {
                    return false;
                };
                let Some(target) = result.graph.get_node(&rel.target_id) else {
                    return false;
                };
                source.properties.name == "stop"
                    && target.properties.file_path == "logger.ts"
                    && target.properties.name == "warn"
            })
            .expect("named object member should still resolve before fuzzy fallback");
        assert_eq!(logger_warn_call.reason, "named-object:logger:warn");

        let bad_import_scoped_call = result.graph.iter_relationships().find(|rel| {
            if !matches!(rel.rel_type, RelationshipType::Calls) || rel.reason != "import-scoped" {
                return false;
            }
            let Some(source) = result.graph.get_node(&rel.source_id) else {
                return false;
            };
            let Some(target) = result.graph.get_node(&rel.target_id) else {
                return false;
            };
            source.properties.name == "stop"
                && target.properties.file_path == "logger.ts"
                && target.properties.name == "close"
        });

        assert!(
            bad_import_scoped_call.is_none(),
            "TS member calls on unrelated receivers should not resolve by fuzzy import-scoped method name"
        );

        cleanup(&dir);
    }

    #[tokio::test]
    async fn test_pipeline_typescript_same_file_builtin_member_calls_ignore_properties() {
        let dir = create_test_dir();
        fs::write(
            dir.join("main.ts"),
            r#"interface RuntimeOptions {
  cwd: string;
  now: Date;
  from: string;
}

export function resolveCwd() {
  return process.cwd();
}

export function timestamp() {
  return Date.now();
}

export function collect(values: string[]) {
  return Array.from(values);
}
"#,
        )
        .unwrap();

        let result = run_pipeline(
            &dir,
            None,
            PipelineOptions {
                skip_git: true,
                ..Default::default()
            },
        )
        .await
        .unwrap();

        let bad_same_file_property_call = result.graph.iter_relationships().find(|rel| {
            if !matches!(rel.rel_type, RelationshipType::Calls) || rel.reason != "same-file" {
                return false;
            }
            let Some(target) = result.graph.get_node(&rel.target_id) else {
                return false;
            };
            target.label == NodeLabel::Property
                && matches!(target.properties.name.as_str(), "cwd" | "now" | "from")
        });

        assert!(
            bad_same_file_property_call.is_none(),
            "TS built-in member calls should not resolve to same-file property declarations"
        );

        cleanup(&dir);
    }

    #[tokio::test]
    async fn test_pipeline_typescript_same_file_call_prefers_function_over_property_name_collision()
    {
        let dir = create_test_dir();
        fs::write(
            dir.join("main.ts"),
            r#"interface Ok<T> {
  readonly ok: true;
  readonly value: T;
}

interface Err {
  readonly ok: false;
  readonly error: Error;
}

type Result<T> = Ok<T> | Err;

export function ok<T>(value: T): Ok<T> {
  return { ok: true, value };
}

export function validate(value: string): Result<string> {
  return ok(value.trim());
}
"#,
        )
        .unwrap();

        let result = run_pipeline(
            &dir,
            None,
            PipelineOptions {
                skip_git: true,
                ..Default::default()
            },
        )
        .await
        .unwrap();

        let call_to_ok_function = result.graph.iter_relationships().find(|rel| {
            if !matches!(rel.rel_type, RelationshipType::Calls) {
                return false;
            }
            let Some(source) = result.graph.get_node(&rel.source_id) else {
                return false;
            };
            let Some(target) = result.graph.get_node(&rel.target_id) else {
                return false;
            };
            source.properties.name == "validate"
                && target.label == NodeLabel::Function
                && target.properties.name == "ok"
        });

        assert!(
            call_to_ok_function.is_some(),
            "TS same-file calls should prefer a Function target over an interface Property with the same name"
        );

        let bad_same_file_property_call = result.graph.iter_relationships().find(|rel| {
            if !matches!(rel.rel_type, RelationshipType::Calls) || rel.reason != "same-file" {
                return false;
            }
            let Some(source) = result.graph.get_node(&rel.source_id) else {
                return false;
            };
            let Some(target) = result.graph.get_node(&rel.target_id) else {
                return false;
            };
            source.properties.name == "validate"
                && target.label == NodeLabel::Property
                && target.properties.name == "ok"
        });

        assert!(
            bad_same_file_property_call.is_none(),
            "TS same-file calls should not resolve ok(...) to Ok.ok"
        );

        cleanup(&dir);
    }

    #[tokio::test]
    async fn test_pipeline_typescript_imported_member_call_ignores_same_file_property_collision() {
        let dir = create_test_dir();
        fs::write(
            dir.join("logger.ts"),
            r#"export const logger = {
  error(message: string) {
    return message;
  },
};
"#,
        )
        .unwrap();
        fs::write(
            dir.join("main.ts"),
            r#"import { logger } from "./logger.js";

interface JSONRPCResponse {
  error?: {
    code: number;
    message: string;
  };
}

export function handleData(message: string) {
  logger.error(message);
}
"#,
        )
        .unwrap();

        let result = run_pipeline(
            &dir,
            None,
            PipelineOptions {
                skip_git: true,
                ..Default::default()
            },
        )
        .await
        .unwrap();

        let bad_same_file_property_call = result.graph.iter_relationships().find(|rel| {
            if !matches!(rel.rel_type, RelationshipType::Calls) || rel.reason != "same-file" {
                return false;
            }
            let Some(source) = result.graph.get_node(&rel.source_id) else {
                return false;
            };
            let Some(target) = result.graph.get_node(&rel.target_id) else {
                return false;
            };
            source.properties.name == "handleData"
                && target.label == NodeLabel::Property
                && target.properties.name == "error"
        });

        assert!(
            bad_same_file_property_call.is_none(),
            "TS member calls on imported receivers should not resolve to same-file property declarations"
        );

        cleanup(&dir);
    }

    #[tokio::test]
    async fn test_pipeline_typescript_builtin_member_call_ignores_same_file_property_collision() {
        let dir = create_test_dir();
        fs::write(
            dir.join("main.ts"),
            r#"interface SemanticMap {
  map: string;
  trim: string;
}

export function collect(elements: string[]) {
  return elements.map((element) => element.trim());
}
"#,
        )
        .unwrap();

        let result = run_pipeline(
            &dir,
            None,
            PipelineOptions {
                skip_git: true,
                ..Default::default()
            },
        )
        .await
        .unwrap();

        let bad_same_file_property_call = result.graph.iter_relationships().find(|rel| {
            if !matches!(rel.rel_type, RelationshipType::Calls) || rel.reason != "same-file" {
                return false;
            }
            let Some(source) = result.graph.get_node(&rel.source_id) else {
                return false;
            };
            let Some(target) = result.graph.get_node(&rel.target_id) else {
                return false;
            };
            source.properties.name == "collect"
                && target.label == NodeLabel::Property
                && matches!(target.properties.name.as_str(), "map" | "trim")
        });

        assert!(
            bad_same_file_property_call.is_none(),
            "TS built-in member calls like elements.map() and element.trim() should not resolve to same-file property declarations"
        );

        cleanup(&dir);
    }

    #[tokio::test]
    async fn test_pipeline_typescript_nested_this_builtin_member_calls_ignore_properties() {
        let dir = create_test_dir();
        fs::write(
            dir.join("main.ts"),
            r#"interface LocalShape {
  entries: string;
  values: string;
}

export class Store {
  private readonly entries = new Map<string, string>();

  list(): string[] {
    return Array.from(this.entries.entries()).map(([key, value]) => `${key}:${value}`);
  }

  allValues(): string[] {
    return [...this.entries.values()];
  }
}
"#,
        )
        .unwrap();

        let result = run_pipeline(
            &dir,
            None,
            PipelineOptions {
                skip_git: true,
                ..Default::default()
            },
        )
        .await
        .unwrap();

        let bad_same_file_property_call = result.graph.iter_relationships().find(|rel| {
            if !matches!(rel.rel_type, RelationshipType::Calls) || rel.reason != "same-file" {
                return false;
            }
            let Some(source) = result.graph.get_node(&rel.source_id) else {
                return false;
            };
            let Some(target) = result.graph.get_node(&rel.target_id) else {
                return false;
            };
            matches!(source.properties.name.as_str(), "list" | "allValues")
                && target.label == NodeLabel::Property
                && matches!(target.properties.name.as_str(), "entries" | "values")
        });

        assert!(
            bad_same_file_property_call.is_none(),
            "TS built-in member calls on nested this receivers should not resolve to same-file property declarations"
        );

        cleanup(&dir);
    }

    #[tokio::test]
    async fn test_pipeline_typescript_fs_stats_member_calls_ignore_same_file_properties() {
        let dir = create_test_dir();
        fs::write(
            dir.join("main.ts"),
            r#"import * as fs from "fs";

interface FileInfo {
  isDirectory: boolean;
  isFile: boolean;
}

export async function info(filePath: string) {
  const stats = await fs.promises.stat(filePath);
  return stats.isDirectory() || stats.isFile();
}
"#,
        )
        .unwrap();

        let result = run_pipeline(
            &dir,
            None,
            PipelineOptions {
                skip_git: true,
                ..Default::default()
            },
        )
        .await
        .unwrap();

        let bad_same_file_property_call = result.graph.iter_relationships().find(|rel| {
            if !matches!(rel.rel_type, RelationshipType::Calls) || rel.reason != "same-file" {
                return false;
            }
            let Some(source) = result.graph.get_node(&rel.source_id) else {
                return false;
            };
            let Some(target) = result.graph.get_node(&rel.target_id) else {
                return false;
            };
            source.properties.name == "info"
                && target.label == NodeLabel::Property
                && matches!(target.properties.name.as_str(), "isDirectory" | "isFile")
        });

        assert!(
            bad_same_file_property_call.is_none(),
            "TS fs.Stats member calls should not resolve to same-file FileInfo properties"
        );

        cleanup(&dir);
    }

    #[tokio::test]
    async fn test_pipeline_typescript_fs_dirent_member_calls_ignore_same_file_properties() {
        let dir = create_test_dir();
        fs::write(
            dir.join("main.ts"),
            r#"import * as fs from "fs";

interface FileInfo {
  isDirectory: boolean;
  isFile: boolean;
}

export async function list(dir: string) {
  const entries = await fs.promises.readdir(dir, { withFileTypes: true });
  const sortedEntries = entries.sort((a, b) => {
    if (a.isDirectory() && !b.isDirectory()) return -1;
    return 0;
  });

  for (const entry of sortedEntries) {
    if (entry.isFile()) return true;
  }

  const first = sortedEntries[0];
  if (first.isDirectory()) return true;

  return false;
}
"#,
        )
        .unwrap();

        let result = run_pipeline(
            &dir,
            None,
            PipelineOptions {
                skip_git: true,
                ..Default::default()
            },
        )
        .await
        .unwrap();

        let bad_same_file_property_call = result.graph.iter_relationships().find(|rel| {
            if !matches!(rel.rel_type, RelationshipType::Calls) || rel.reason != "same-file" {
                return false;
            }
            let Some(target) = result.graph.get_node(&rel.target_id) else {
                return false;
            };
            target.label == NodeLabel::Property
                && matches!(target.properties.name.as_str(), "isDirectory" | "isFile")
        });

        assert!(
            bad_same_file_property_call.is_none(),
            "TS fs.Dirent member calls should not resolve to same-file FileInfo properties"
        );

        cleanup(&dir);
    }

    #[tokio::test]
    async fn test_pipeline_typescript_external_runtime_member_calls_ignore_same_file_properties() {
        let dir = create_test_dir();
        fs::write(
            dir.join("main.ts"),
            r#"import { Router, type Response } from "express";
import { spawn } from "child_process";

interface LocalShape {
  text: string;
  status: string;
  json: string;
  on: string;
}

export async function loadRemote() {
  const response = await fetch("https://example.test/data");
  const text = await response.text();
  return response.json();
}

export function createRoutes() {
  const router = Router();
  router.post("/items", (_req, res) => {
    res.status(201).json({ ok: true });
  });
  return router;
}

export function sendError(res: Response) {
  res.status(500).json({ ok: false });
}

export function runChild() {
  const child = spawn("node", ["--version"]);
  child.stdout.on("data", () => undefined);
}

export const provider = {
  run: async () => {
    const response = await fetch("https://example.test/audio");
    if (!response.ok) {
      return response.text();
    }
    return "";
  },
};
"#,
        )
        .unwrap();

        let result = run_pipeline(
            &dir,
            None,
            PipelineOptions {
                skip_git: true,
                ..Default::default()
            },
        )
        .await
        .unwrap();

        let bad_same_file_property_call = result.graph.iter_relationships().find(|rel| {
            if !matches!(rel.rel_type, RelationshipType::Calls) || rel.reason != "same-file" {
                return false;
            }
            let Some(source) = result.graph.get_node(&rel.source_id) else {
                return false;
            };
            let Some(target) = result.graph.get_node(&rel.target_id) else {
                return false;
            };
            matches!(
                source.properties.name.as_str(),
                "loadRemote" | "createRoutes" | "sendError" | "runChild" | "run"
            ) && target.label == NodeLabel::Property
                && matches!(
                    target.properties.name.as_str(),
                    "text" | "status" | "json" | "on"
                )
        });

        assert!(
            bad_same_file_property_call.is_none(),
            "TS external runtime member calls should not resolve to same-file property declarations"
        );

        cleanup(&dir);
    }

    #[tokio::test]
    async fn test_pipeline_typescript_external_imports_do_not_fall_back_to_global_symbols() {
        let dir = create_test_dir();
        fs::write(
            dir.join("ide-protocol.ts"),
            "export interface Command { title: string; command: string; }\n",
        )
        .unwrap();
        fs::write(
            dir.join("main.ts"),
            r#"import { Command } from "commander";

export function createUpdateCommand(): Command {
  return new Command("update");
}
"#,
        )
        .unwrap();

        let result = run_pipeline(
            &dir,
            None,
            PipelineOptions {
                skip_git: true,
                ..Default::default()
            },
        )
        .await
        .unwrap();

        let bad_global_call = result.graph.iter_relationships().find(|rel| {
            if !matches!(rel.rel_type, RelationshipType::Calls) || rel.reason != "global" {
                return false;
            }
            let Some(source) = result.graph.get_node(&rel.source_id) else {
                return false;
            };
            let Some(target) = result.graph.get_node(&rel.target_id) else {
                return false;
            };
            source.properties.name == "createUpdateCommand" && target.properties.name == "Command"
        });

        assert!(
            bad_global_call.is_none(),
            "external imports like commander.Command should not resolve to unrelated local global symbols"
        );

        cleanup(&dir);
    }

    #[tokio::test]
    async fn test_pipeline_typescript_external_dynamic_namespace_does_not_fall_back_to_global_symbols(
    ) {
        let dir = create_test_dir();
        fs::write(
            dir.join("optional-deps.d.ts"),
            "export class RemoteModel {}\nexport class RemoteSession {}\n",
        )
        .unwrap();
        fs::write(
            dir.join("main.ts"),
            r#"export async function initialize() {
  const remote = await import("external-llm").catch(() => null);
  if (!remote) return null;
  const { RemoteModel } = remote;
  return new RemoteModel();
}

export async function complete() {
  const remote = await import("external-llm").catch(() => null);
  if (!remote) return null;
  const { RemoteSession } = remote;
  return new RemoteSession();
}
"#,
        )
        .unwrap();

        let result = run_pipeline(
            &dir,
            None,
            PipelineOptions {
                skip_git: true,
                ..Default::default()
            },
        )
        .await
        .unwrap();

        let bad_global_call = result.graph.iter_relationships().find(|rel| {
            if !matches!(rel.rel_type, RelationshipType::Calls) || rel.reason != "global" {
                return false;
            }
            let Some(source) = result.graph.get_node(&rel.source_id) else {
                return false;
            };
            let Some(target) = result.graph.get_node(&rel.target_id) else {
                return false;
            };
            matches!(
                (
                    source.properties.name.as_str(),
                    target.properties.name.as_str()
                ),
                ("initialize", "RemoteModel") | ("complete", "RemoteSession")
            )
        });

        assert!(
            bad_global_call.is_none(),
            "external dynamic namespace names should not resolve to unrelated global symbols"
        );

        cleanup(&dir);
    }

    #[tokio::test]
    async fn test_pipeline_typescript_type_only_globals_are_not_call_targets() {
        let dir = create_test_dir();
        fs::write(
            dir.join("types.ts"),
            "export interface Point { x: number; y: number; }\n",
        )
        .unwrap();
        fs::write(
            dir.join("main.ts"),
            "export function performClick() { return new Point(1, 2); }\n",
        )
        .unwrap();

        let result = run_pipeline(
            &dir,
            None,
            PipelineOptions {
                skip_git: true,
                ..Default::default()
            },
        )
        .await
        .unwrap();

        let bad_global_call = result.graph.iter_relationships().find(|rel| {
            if !matches!(rel.rel_type, RelationshipType::Calls) || rel.reason != "global" {
                return false;
            }
            let Some(source) = result.graph.get_node(&rel.source_id) else {
                return false;
            };
            let Some(target) = result.graph.get_node(&rel.target_id) else {
                return false;
            };
            source.properties.name == "performClick" && target.properties.name == "Point"
        });

        assert!(
            bad_global_call.is_none(),
            "TypeScript interfaces and type aliases should not be call targets"
        );

        cleanup(&dir);
    }

    #[tokio::test]
    async fn test_pipeline_csharp_calls_do_not_fall_back_to_typescript_type_only_targets() {
        let dir = create_test_dir();
        fs::write(
            dir.join("types.ts"),
            "export interface Size { width: number; height: number; }\n",
        )
        .unwrap();
        fs::write(
            dir.join("Bridge.cs"),
            r#"public class Bridge
{
    public void HandleRequest()
    {
        var bitmapSize = new Size(1, 1);
    }
}
"#,
        )
        .unwrap();

        let result = run_pipeline(
            &dir,
            None,
            PipelineOptions {
                skip_git: true,
                ..Default::default()
            },
        )
        .await
        .unwrap();

        let bad_global_call = result.graph.iter_relationships().find(|rel| {
            if !matches!(rel.rel_type, RelationshipType::Calls) || rel.reason != "global" {
                return false;
            }
            let Some(source) = result.graph.get_node(&rel.source_id) else {
                return false;
            };
            let Some(target) = result.graph.get_node(&rel.target_id) else {
                return false;
            };
            source.properties.name == "HandleRequest" && target.properties.name == "Size"
        });

        assert!(
            bad_global_call.is_none(),
            "C# calls should not resolve to TypeScript interfaces or type aliases"
        );

        cleanup(&dir);
    }

    #[tokio::test]
    async fn test_pipeline_typescript_relative_import_prefers_exact_path_over_suffix_collision() {
        let dir = create_test_dir();
        fs::create_dir_all(dir.join("agent/middleware")).unwrap();
        fs::create_dir_all(dir.join("middleware")).unwrap();
        fs::write(
            dir.join("agent/middleware/types.ts"),
            "export function continueResult() { return { wrong: true }; }\n",
        )
        .unwrap();
        fs::write(
            dir.join("middleware/types.ts"),
            "export function continueResult() { return { ok: true }; }\n",
        )
        .unwrap();
        fs::write(
            dir.join("middleware/pipeline.ts"),
            r#"import { continueResult } from "./types.js";

export function runBefore() {
  return continueResult();
}
"#,
        )
        .unwrap();

        let result = run_pipeline(
            &dir,
            None,
            PipelineOptions {
                skip_git: true,
                ..Default::default()
            },
        )
        .await
        .unwrap();

        let call = result
            .graph
            .iter_relationships()
            .find(|rel| {
                if !matches!(rel.rel_type, RelationshipType::Calls) {
                    return false;
                }
                let Some(source) = result.graph.get_node(&rel.source_id) else {
                    return false;
                };
                let Some(target) = result.graph.get_node(&rel.target_id) else {
                    return false;
                };
                source.properties.name == "runBefore" && target.properties.name == "continueResult"
            })
            .expect("runBefore should call imported continueResult");

        let target = result.graph.get_node(&call.target_id).unwrap();
        assert_eq!(target.properties.file_path, "middleware/types.ts");
        assert_eq!(call.reason, "named-import");

        cleanup(&dir);
    }

    #[tokio::test]
    async fn test_pipeline_typescript_parameter_calls_do_not_fall_back_to_global_symbols() {
        let dir = create_test_dir();
        fs::write(
            dir.join("helpers.ts"),
            "export function run() { return 'wrong'; }\n\
             export function estimateTokens(value: string) { return value.length; }\n",
        )
        .unwrap();
        fs::write(
            dir.join("main.ts"),
            r#"export async function withRequestModel(run: () => Promise<string>) {
  return await run();
}

export function compressContext(estimateTokens: (text: string) => number) {
  return estimateTokens("hello");
}
"#,
        )
        .unwrap();

        let result = run_pipeline(
            &dir,
            None,
            PipelineOptions {
                skip_git: true,
                ..Default::default()
            },
        )
        .await
        .unwrap();

        let bad_global_call = result.graph.iter_relationships().find(|rel| {
            if !matches!(rel.rel_type, RelationshipType::Calls) || rel.reason != "global" {
                return false;
            }
            let Some(source) = result.graph.get_node(&rel.source_id) else {
                return false;
            };
            let Some(target) = result.graph.get_node(&rel.target_id) else {
                return false;
            };
            matches!(
                (
                    source.properties.name.as_str(),
                    target.properties.name.as_str()
                ),
                ("withRequestModel", "run") | ("compressContext", "estimateTokens")
            )
        });

        assert!(
            bad_global_call.is_none(),
            "calls to function parameters should not resolve to unrelated global symbols"
        );

        cleanup(&dir);
    }

    #[tokio::test]
    async fn test_pipeline_typescript_promise_executor_parameters_ignore_property_collision() {
        let dir = create_test_dir();
        fs::write(
            dir.join("main.ts"),
            r#"interface Pending<T> {
  resolve: (value: T) => void;
  reject: (error: Error) => void;
}

export class Queue {
  private pending: Pending<string>[] = [];

  enqueue(): Promise<string> {
    return new Promise((resolve, reject) => {
      if (this.pending.length > 10) {
        reject(new Error("full"));
        return;
      }

      this.pending.push({ resolve, reject });
    });
  }

  flush(value: string): void {
    const task = this.pending.shift();
    if (task) {
      task.resolve(value);
    }
  }
}
"#,
        )
        .unwrap();

        let result = run_pipeline(
            &dir,
            None,
            PipelineOptions {
                skip_git: true,
                ..Default::default()
            },
        )
        .await
        .unwrap();

        let bad_executor_property_call = result.graph.iter_relationships().find(|rel| {
            if !matches!(rel.rel_type, RelationshipType::Calls) || rel.reason != "same-file" {
                return false;
            }
            let Some(source) = result.graph.get_node(&rel.source_id) else {
                return false;
            };
            let Some(target) = result.graph.get_node(&rel.target_id) else {
                return false;
            };
            source.properties.name == "enqueue"
                && target.label == NodeLabel::Property
                && matches!(target.properties.name.as_str(), "resolve" | "reject")
        });

        assert!(
            bad_executor_property_call.is_none(),
            "TS Promise executor parameters should not resolve to same-file properties with the same names"
        );

        let member_property_call = result.graph.iter_relationships().find(|rel| {
            if !matches!(rel.rel_type, RelationshipType::Calls) || rel.reason != "same-file" {
                return false;
            }
            let Some(source) = result.graph.get_node(&rel.source_id) else {
                return false;
            };
            let Some(target) = result.graph.get_node(&rel.target_id) else {
                return false;
            };
            source.properties.name == "flush"
                && target.label == NodeLabel::Property
                && target.properties.name == "resolve"
        });

        assert!(
            member_property_call.is_some(),
            "TS member calls to callable properties should still be preserved"
        );

        cleanup(&dir);
    }

    #[tokio::test]
    async fn test_pipeline_typescript_require_destructuring_disambiguates_calls() {
        let dir = create_test_dir();
        fs::write(dir.join("a.ts"), "export function foo() { return 1; }\n").unwrap();
        fs::write(dir.join("c.ts"), "export function foo() { return 2; }\n").unwrap();
        fs::write(
            dir.join("b.ts"),
            "const { foo } = require(\"./a.js\");\nexport function load() { return foo(); }\n",
        )
        .unwrap();

        let result = run_pipeline(
            &dir,
            None,
            PipelineOptions {
                skip_git: true,
                ..Default::default()
            },
        )
        .await
        .unwrap();

        let call = result
            .graph
            .iter_relationships()
            .find(|rel| {
                if !matches!(rel.rel_type, RelationshipType::Calls) {
                    return false;
                }
                let Some(source) = result.graph.get_node(&rel.source_id) else {
                    return false;
                };
                let Some(target) = result.graph.get_node(&rel.target_id) else {
                    return false;
                };
                source.properties.name == "load" && target.properties.name == "foo"
            })
            .expect("load should call require-destructured foo");

        let target = result.graph.get_node(&call.target_id).unwrap();
        assert!(
            target.properties.file_path.ends_with("a.ts"),
            "expected require destructuring to resolve a.ts, got {} via {}",
            target.properties.file_path,
            call.reason
        );
        assert_eq!(call.reason, "named-import");

        cleanup(&dir);
    }

    #[tokio::test]
    async fn test_pipeline_typescript_require_member_alias_disambiguates_call() {
        let dir = create_test_dir();
        fs::write(dir.join("api.ts"), "export function run() { return 1; }\n").unwrap();
        fs::write(
            dir.join("other.ts"),
            "export function execute() { return 2; }\n",
        )
        .unwrap();
        fs::write(
            dir.join("main.ts"),
            "const execute = require(\"./api.js\").run;\nexport function load() { return execute(); }\n",
        )
        .unwrap();

        let result = run_pipeline(
            &dir,
            None,
            PipelineOptions {
                skip_git: true,
                ..Default::default()
            },
        )
        .await
        .unwrap();

        let call = result
            .graph
            .iter_relationships()
            .find(|rel| {
                if !matches!(rel.rel_type, RelationshipType::Calls) {
                    return false;
                }
                let Some(source) = result.graph.get_node(&rel.source_id) else {
                    return false;
                };
                let Some(target) = result.graph.get_node(&rel.target_id) else {
                    return false;
                };
                source.properties.name == "load" && target.properties.name == "run"
            })
            .expect("load should call the require member alias target");

        let target = result.graph.get_node(&call.target_id).unwrap();
        assert!(
            target.properties.file_path.ends_with("api.ts"),
            "expected require member alias to resolve api.ts run, got {} via {}",
            target.properties.file_path,
            call.reason
        );
        assert_eq!(call.reason, "named-import");

        let bad_alias_call = result.graph.iter_relationships().find(|rel| {
            if !matches!(rel.rel_type, RelationshipType::Calls) {
                return false;
            }
            let Some(source) = result.graph.get_node(&rel.source_id) else {
                return false;
            };
            let Some(target) = result.graph.get_node(&rel.target_id) else {
                return false;
            };
            source.properties.name == "load" && target.properties.name == "execute"
        });
        assert!(
            bad_alias_call.is_none(),
            "require member alias should not resolve to a same-named import"
        );

        cleanup(&dir);
    }

    #[tokio::test]
    async fn test_pipeline_typescript_unresolved_require_binding_does_not_fall_back_to_global_symbol(
    ) {
        let dir = create_test_dir();
        fs::write(dir.join("fetch-tool.ts"), "export class FetchTool {}\n").unwrap();
        fs::write(
            dir.join("tool-manager.ts"),
            r#"export function factory() {
  const { FetchTool } = require("./fetch.js");
  return new FetchTool();
}
"#,
        )
        .unwrap();

        let result = run_pipeline(
            &dir,
            None,
            PipelineOptions {
                skip_git: true,
                ..Default::default()
            },
        )
        .await
        .unwrap();

        let bad_global_call = result.graph.iter_relationships().find(|rel| {
            if !matches!(rel.rel_type, RelationshipType::Calls) || rel.reason != "global" {
                return false;
            }
            let Some(source) = result.graph.get_node(&rel.source_id) else {
                return false;
            };
            let Some(target) = result.graph.get_node(&rel.target_id) else {
                return false;
            };
            source.properties.name == "factory" && target.properties.name == "FetchTool"
        });

        assert!(
            bad_global_call.is_none(),
            "unresolved local require bindings should shadow globals instead of falling back globally"
        );

        cleanup(&dir);
    }

    #[tokio::test]
    async fn test_pipeline_typescript_missing_local_require_export_does_not_fall_back_to_global_symbol(
    ) {
        let dir = create_test_dir();
        fs::write(dir.join("search.ts"), "export class SearchTool {}\n").unwrap();
        fs::write(
            dir.join("enhanced-search.ts"),
            "export class EnhancedSearch {}\n",
        )
        .unwrap();
        fs::write(
            dir.join("tool-manager.ts"),
            r#"export function factory() {
  const { SearchTool } = require("./enhanced-search.js");
  return new SearchTool();
}
"#,
        )
        .unwrap();

        let result = run_pipeline(
            &dir,
            None,
            PipelineOptions {
                skip_git: true,
                ..Default::default()
            },
        )
        .await
        .unwrap();

        let bad_global_call = result.graph.iter_relationships().find(|rel| {
            if !matches!(rel.rel_type, RelationshipType::Calls) || rel.reason != "global" {
                return false;
            }
            let Some(source) = result.graph.get_node(&rel.source_id) else {
                return false;
            };
            let Some(target) = result.graph.get_node(&rel.target_id) else {
                return false;
            };
            source.properties.name == "factory" && target.properties.name == "SearchTool"
        });

        assert!(
            bad_global_call.is_none(),
            "missing exports from resolved local require bindings should not fall back globally"
        );

        cleanup(&dir);
    }

    #[tokio::test]
    async fn test_pipeline_typescript_require_namespace_resolves_member_call() {
        let dir = create_test_dir();
        fs::write(dir.join("api.ts"), "export function foo() { return 1; }\n").unwrap();
        fs::write(
            dir.join("other.ts"),
            "export function foo() { return 2; }\n",
        )
        .unwrap();
        fs::write(
            dir.join("b.ts"),
            "const api = require(\"./api.js\");\nexport function load() { return api.foo(); }\n",
        )
        .unwrap();

        let result = run_pipeline(
            &dir,
            None,
            PipelineOptions {
                skip_git: true,
                ..Default::default()
            },
        )
        .await
        .unwrap();

        let call = result
            .graph
            .iter_relationships()
            .find(|rel| {
                if !matches!(rel.rel_type, RelationshipType::Calls) {
                    return false;
                }
                let Some(source) = result.graph.get_node(&rel.source_id) else {
                    return false;
                };
                let Some(target) = result.graph.get_node(&rel.target_id) else {
                    return false;
                };
                source.properties.name == "load" && target.properties.name == "foo"
            })
            .expect("load should call require namespace foo");

        let target = result.graph.get_node(&call.target_id).unwrap();
        assert!(
            target.properties.file_path.ends_with("api.ts"),
            "expected require namespace to resolve api.ts, got {} via {}",
            target.properties.file_path,
            call.reason
        );
        assert!(call.reason.starts_with("module-alias:api:foo"));

        cleanup(&dir);
    }

    #[tokio::test]
    async fn test_pipeline_typescript_constructor_receiver_disambiguates_method_call() {
        let dir = create_test_dir();
        fs::write(
            dir.join("service.ts"),
            "export class Service { run() { return 1; } }\n",
        )
        .unwrap();
        fs::write(
            dir.join("other.ts"),
            "export class Other { run() { return 2; } }\n",
        )
        .unwrap();
        fs::write(
            dir.join("main.ts"),
            "import { Service } from \"./service\";\nexport function load() {\n  const svc = new Service();\n  return svc.run();\n}\n",
        )
        .unwrap();

        let result = run_pipeline(
            &dir,
            None,
            PipelineOptions {
                skip_git: true,
                ..Default::default()
            },
        )
        .await
        .unwrap();

        let call = result
            .graph
            .iter_relationships()
            .find(|rel| {
                if !matches!(rel.rel_type, RelationshipType::Calls) {
                    return false;
                }
                let Some(source) = result.graph.get_node(&rel.source_id) else {
                    return false;
                };
                let Some(target) = result.graph.get_node(&rel.target_id) else {
                    return false;
                };
                source.properties.name == "load" && target.properties.name == "run"
            })
            .expect("load should call Service.run");

        let target = result.graph.get_node(&call.target_id).unwrap();
        assert!(
            target.properties.file_path.ends_with("service.ts"),
            "expected constructor receiver to resolve service.ts run, got {} via {}",
            target.properties.file_path,
            call.reason
        );
        assert!(call.reason.starts_with("receiver-type:svc:Service:run"));

        cleanup(&dir);
    }

    #[tokio::test]
    async fn test_pipeline_typescript_receiver_inference_is_function_scoped() {
        let dir = create_test_dir();
        fs::write(
            dir.join("service.ts"),
            "export class Service { run() { return 1; } }\n",
        )
        .unwrap();
        fs::write(
            dir.join("other.ts"),
            "export class Other { run() { return 2; } }\n",
        )
        .unwrap();
        fs::write(
            dir.join("main.ts"),
            r#"import { Service } from "./service";
import { Other } from "./other";

export function first() {
  const svc = new Service();
  return svc.run();
}

export function second() {
  const svc = new Other();
  return svc.run();
}
"#,
        )
        .unwrap();

        let result = run_pipeline(
            &dir,
            None,
            PipelineOptions {
                skip_git: true,
                ..Default::default()
            },
        )
        .await
        .unwrap();

        let first_call = result
            .graph
            .iter_relationships()
            .find(|rel| {
                if !matches!(rel.rel_type, RelationshipType::Calls) {
                    return false;
                }
                let Some(source) = result.graph.get_node(&rel.source_id) else {
                    return false;
                };
                let Some(target) = result.graph.get_node(&rel.target_id) else {
                    return false;
                };
                source.properties.name == "first" && target.properties.name == "run"
            })
            .expect("first should call Service.run");
        let first_target = result.graph.get_node(&first_call.target_id).unwrap();
        assert!(
            first_target.properties.file_path.ends_with("service.ts"),
            "expected first to resolve service.ts run, got {} via {}",
            first_target.properties.file_path,
            first_call.reason
        );
        assert!(first_call
            .reason
            .starts_with("receiver-type:svc:Service:run"));

        let second_call = result
            .graph
            .iter_relationships()
            .find(|rel| {
                if !matches!(rel.rel_type, RelationshipType::Calls) {
                    return false;
                }
                let Some(source) = result.graph.get_node(&rel.source_id) else {
                    return false;
                };
                let Some(target) = result.graph.get_node(&rel.target_id) else {
                    return false;
                };
                source.properties.name == "second" && target.properties.name == "run"
            })
            .expect("second should call Other.run");
        let second_target = result.graph.get_node(&second_call.target_id).unwrap();
        assert!(
            second_target.properties.file_path.ends_with("other.ts"),
            "expected second to resolve other.ts run, got {} via {}",
            second_target.properties.file_path,
            second_call.reason
        );
        assert!(second_call
            .reason
            .starts_with("receiver-type:svc:Other:run"));

        cleanup(&dir);
    }

    #[tokio::test]
    async fn test_pipeline_typescript_this_field_receiver_disambiguates_method_call() {
        let dir = create_test_dir();
        fs::write(
            dir.join("service.ts"),
            "export class Service { run() { return 1; } }\n",
        )
        .unwrap();
        fs::write(
            dir.join("other.ts"),
            "export class Other { run() { return 2; } }\n",
        )
        .unwrap();
        fs::write(
            dir.join("main.ts"),
            r#"import { Service } from "./service";

export class Runner {
  private svc: Service;

  constructor(svc: Service) {
    this.svc = svc;
  }

  load() {
    return this.svc.run();
  }
}
"#,
        )
        .unwrap();

        let result = run_pipeline(
            &dir,
            None,
            PipelineOptions {
                skip_git: true,
                ..Default::default()
            },
        )
        .await
        .unwrap();

        let call = result
            .graph
            .iter_relationships()
            .find(|rel| {
                if !matches!(rel.rel_type, RelationshipType::Calls) {
                    return false;
                }
                let Some(source) = result.graph.get_node(&rel.source_id) else {
                    return false;
                };
                let Some(target) = result.graph.get_node(&rel.target_id) else {
                    return false;
                };
                source.properties.name == "load" && target.properties.name == "run"
            })
            .expect("load should call Service.run via this.svc");

        let target = result.graph.get_node(&call.target_id).unwrap();
        assert!(
            target.properties.file_path.ends_with("service.ts"),
            "expected this.svc to resolve service.ts run, got {} via {}",
            target.properties.file_path,
            call.reason
        );
        assert!(call.reason.starts_with("receiver-type:svc:Service:run"));

        cleanup(&dir);
    }

    #[tokio::test]
    async fn test_pipeline_typescript_annotated_receiver_disambiguates_method_call() {
        let dir = create_test_dir();
        fs::write(
            dir.join("service.ts"),
            "export class Service { run() { return 1; } }\nexport function createService(): Service { return new Service(); }\n",
        )
        .unwrap();
        fs::write(
            dir.join("other.ts"),
            "export class Other { run() { return 2; } }\n",
        )
        .unwrap();
        fs::write(
            dir.join("main.ts"),
            "import { Service, createService } from \"./service\";\nexport function load() {\n  const svc: Service = createService();\n  return svc.run();\n}\n",
        )
        .unwrap();

        let result = run_pipeline(
            &dir,
            None,
            PipelineOptions {
                skip_git: true,
                ..Default::default()
            },
        )
        .await
        .unwrap();

        let call = result
            .graph
            .iter_relationships()
            .find(|rel| {
                if !matches!(rel.rel_type, RelationshipType::Calls) {
                    return false;
                }
                let Some(source) = result.graph.get_node(&rel.source_id) else {
                    return false;
                };
                let Some(target) = result.graph.get_node(&rel.target_id) else {
                    return false;
                };
                source.properties.name == "load" && target.properties.name == "run"
            })
            .expect("load should call annotated Service.run");

        let target = result.graph.get_node(&call.target_id).unwrap();
        assert!(
            target.properties.file_path.ends_with("service.ts"),
            "expected annotated receiver to resolve service.ts run, got {} via {}",
            target.properties.file_path,
            call.reason
        );
        assert!(call.reason.starts_with("receiver-type:svc:Service:run"));

        cleanup(&dir);
    }

    #[tokio::test]
    async fn test_pipeline_typescript_static_class_call_disambiguates_method_call() {
        let dir = create_test_dir();
        fs::write(
            dir.join("service.ts"),
            "export class Service { static create() { return new Service(); } }\n",
        )
        .unwrap();
        fs::write(
            dir.join("other.ts"),
            "export class Other { static create() { return new Other(); } }\n",
        )
        .unwrap();
        fs::write(
            dir.join("main.ts"),
            "import { Service } from \"./service\";\nexport function load() { return Service.create(); }\n",
        )
        .unwrap();

        let result = run_pipeline(
            &dir,
            None,
            PipelineOptions {
                skip_git: true,
                ..Default::default()
            },
        )
        .await
        .unwrap();

        let call = result
            .graph
            .iter_relationships()
            .find(|rel| {
                if !matches!(rel.rel_type, RelationshipType::Calls) {
                    return false;
                }
                let Some(source) = result.graph.get_node(&rel.source_id) else {
                    return false;
                };
                let Some(target) = result.graph.get_node(&rel.target_id) else {
                    return false;
                };
                source.properties.name == "load" && target.properties.name == "create"
            })
            .expect("load should call Service.create");

        let target = result.graph.get_node(&call.target_id).unwrap();
        assert!(
            target.properties.file_path.ends_with("service.ts"),
            "expected static class call to resolve service.ts create, got {} via {}",
            target.properties.file_path,
            call.reason
        );
        assert!(call.reason.starts_with("static-call-ts:Service::create"));

        cleanup(&dir);
    }

    #[tokio::test]
    async fn test_pipeline_empty_project() {
        let dir = create_test_dir();
        // Empty directory -- should not crash
        let result = run_pipeline(&dir, None, PipelineOptions::default()).await;
        assert!(result.is_ok(), "Empty project should not crash");

        let result = result.unwrap();
        // Empty project may have 0 nodes (no source files to parse)
        // The key assertion is that it didn't error out
        assert_eq!(
            result.total_file_count, 0,
            "Empty project should report 0 files"
        );

        cleanup(&dir);
    }

    #[tokio::test]
    async fn test_pipeline_error_recovery() {
        let dir = create_test_dir();

        // One valid file
        fs::write(dir.join("valid.js"), "function hello() { return 42; }").unwrap();

        // One malformed file (binary content)
        fs::write(dir.join("corrupt.js"), [0xFF, 0xFE, 0x00, 0x01]).unwrap();

        let result = run_pipeline(&dir, None, PipelineOptions::default()).await;
        assert!(result.is_ok(), "Pipeline should recover from bad files");

        let result = result.unwrap();
        // Valid file should still be processed
        assert!(
            result.graph.node_count() > 0,
            "Valid file nodes should exist despite corrupt file"
        );

        cleanup(&dir);
    }

    #[tokio::test]
    async fn test_pipeline_multiple_languages() {
        let dir = create_test_dir();

        fs::write(dir.join("app.js"), "function jsFunc() {}").unwrap();
        fs::write(dir.join("main.py"), "def py_func():\n    pass").unwrap();
        fs::write(dir.join("lib.rs"), "pub fn rust_func() {}").unwrap();

        let result = run_pipeline(&dir, None, PipelineOptions::default()).await;
        assert!(
            result.is_ok(),
            "Multi-language pipeline failed: {:?}",
            result.err()
        );

        let result = result.unwrap();
        let graph = &result.graph;

        // Should have detected multiple languages
        let languages: std::collections::HashSet<_> = graph
            .iter_nodes()
            .filter_map(|n| n.properties.language)
            .collect();

        assert!(
            languages.len() >= 2,
            "Should detect at least 2 languages, found: {:?}",
            languages
        );

        cleanup(&dir);
    }

    #[tokio::test]
    async fn test_pipeline_python_classes() {
        let dir = create_test_dir();
        fs::write(
            dir.join("models.py"),
            r#"
class Animal:
    def __init__(self, name):
        self.name = name

    def speak(self):
        pass

class Dog(Animal):
    def speak(self):
        return "Woof!"
"#,
        )
        .unwrap();

        let result = run_pipeline(&dir, None, PipelineOptions::default()).await;
        assert!(result.is_ok(), "Pipeline failed: {:?}", result.err());

        let result = result.unwrap();
        let graph = &result.graph;

        let classes: Vec<&str> = graph
            .iter_nodes()
            .filter(|n| n.label == NodeLabel::Class)
            .map(|n| n.properties.name.as_str())
            .collect();

        assert!(
            classes.contains(&"Animal"),
            "Should detect Animal class, found: {:?}",
            classes
        );
        assert!(
            classes.contains(&"Dog"),
            "Should detect Dog class, found: {:?}",
            classes
        );

        cleanup(&dir);
    }

    #[tokio::test]
    async fn test_pipeline_file_count_matches() {
        let dir = create_test_dir();
        fs::write(dir.join("a.js"), "var x = 1;").unwrap();
        fs::write(dir.join("b.js"), "var y = 2;").unwrap();
        fs::write(dir.join("c.py"), "z = 3").unwrap();

        let result = run_pipeline(&dir, None, PipelineOptions::default()).await;
        assert!(result.is_ok());

        let result = result.unwrap();
        assert_eq!(result.total_file_count, 3, "Should report exactly 3 files");

        cleanup(&dir);
    }
}

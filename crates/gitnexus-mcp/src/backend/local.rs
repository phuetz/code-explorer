//! Local backend: resolves repos from the registry and dispatches tool calls
//! to the appropriate handler.

use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;
use tracing::info;

use gitnexus_core::graph::types::{NodeLabel, RelationshipType};
use gitnexus_core::graph::KnowledgeGraph;
use gitnexus_core::storage::repo_manager::{self, registry_entry_id, RegistryEntry};
use gitnexus_db::inmemory::cypher::GraphIndexes;
use gitnexus_db::inmemory::fts::{fts_result_to_json, FtsIndex, FtsResult};
use gitnexus_db::pool::ConnectionPool;
use gitnexus_db::query;
use gitnexus_git::coupling::CouplingError;
use gitnexus_git::hotspots::HotspotError;
use gitnexus_git::ownership::OwnershipError;
use gitnexus_search::bm25::BM25SearchResult;
use gitnexus_search::embeddings::{
    generate_embeddings, load_embeddings, search_semantic, EmbeddingConfig,
};
use gitnexus_search::hybrid;
use gitnexus_search::reranker::{Candidate, LlmReranker, Reranker};

use crate::error::{McpError, Result};
use crate::hints;

/// Local MCP backend: manages connections and dispatches tool calls.
pub struct LocalBackend {
    pool: ConnectionPool,
    registry: Vec<RegistryEntry>,
    /// Cached snapshots keyed by absolute snapshot path, shared across tool calls.
    snapshot_cache: HashMap<PathBuf, Arc<KnowledgeGraph>>,
    /// Cached graph indexes (adjacency lists + label index), built lazily.
    indexes_cache: HashMap<PathBuf, Arc<GraphIndexes>>,
    /// Cached full-text search index, built lazily.
    fts_cache: HashMap<PathBuf, Arc<FtsIndex>>,
}

const MAX_QUERY_LIMIT: usize = 100;
const MAX_ANALYTICS_LIMIT: usize = 100;
const MAX_IMPACT_DEPTH: usize = 10;

fn git_history_required_error(tool: &str) -> McpError {
    McpError::InvalidArguments {
        tool: tool.into(),
        reason: "Git analytics require an indexed path inside a Git working tree. Choose a Git-backed project or re-run `gitnexus analyze` from the repository checkout.".into(),
    }
}

fn clamp_limit(raw: Option<u64>, default: usize, max: usize) -> usize {
    raw.map(|v| v as usize).unwrap_or(default).clamp(1, max)
}

fn normalize_repo_lookup_key(raw: &str) -> String {
    let mut key = raw.trim().replace('\\', "/").to_lowercase();
    if let Some(rest) = key.strip_prefix("//?/unc/") {
        key = format!("//{}", rest);
    } else if let Some(rest) = key.strip_prefix("//?/") {
        key = rest.to_string();
    }
    while key.ends_with('/') {
        key.pop();
    }
    key
}

impl LocalBackend {
    /// Create a new LocalBackend.
    pub fn new() -> Self {
        Self {
            pool: ConnectionPool::new(),
            registry: Vec::new(),
            snapshot_cache: HashMap::new(),
            indexes_cache: HashMap::new(),
            fts_cache: HashMap::new(),
        }
    }

    /// Load a snapshot from disk, returning a cached `Arc<KnowledgeGraph>` if already loaded.
    fn load_cached_snapshot(&mut self, snap_path: &std::path::Path) -> Result<Arc<KnowledgeGraph>> {
        let key = snap_path.to_path_buf();
        if let Some(cached) = self.snapshot_cache.get(&key) {
            return Ok(Arc::clone(cached));
        }
        let graph = gitnexus_db::snapshot::load_snapshot(snap_path)
            .map_err(|e| McpError::Internal(format!("Failed to load graph: {e}")))?;
        let arc = Arc::new(graph);
        self.snapshot_cache.insert(key, Arc::clone(&arc));
        Ok(arc)
    }

    /// Load snapshot + lazily build GraphIndexes and FtsIndex, returning cached copies.
    fn load_cached_indexes(
        &mut self,
        snap_path: &std::path::Path,
    ) -> Result<(Arc<KnowledgeGraph>, Arc<GraphIndexes>, Arc<FtsIndex>)> {
        let graph = self.load_cached_snapshot(snap_path)?;
        let key = snap_path.to_path_buf();

        let indexes = if let Some(cached) = self.indexes_cache.get(&key) {
            Arc::clone(cached)
        } else {
            let idx = Arc::new(GraphIndexes::build(&graph));
            self.indexes_cache.insert(key.clone(), Arc::clone(&idx));
            idx
        };

        let fts = if let Some(cached) = self.fts_cache.get(&key) {
            Arc::clone(cached)
        } else {
            let fts_idx = Arc::new(FtsIndex::build(&graph));
            self.fts_cache.insert(key, Arc::clone(&fts_idx));
            fts_idx
        };

        Ok((graph, indexes, fts))
    }

    /// Resolve an indexed repository and return cached graph/search indexes for it.
    ///
    /// This is used by the HTTP REST transport for browser-native read-only
    /// exploration endpoints. The returned registry entry is cloned so callers
    /// can safely keep repository metadata while the mutable backend borrow is
    /// used to populate caches.
    pub fn load_repo_indexes(
        &mut self,
        repo: &str,
    ) -> Result<(
        RegistryEntry,
        Arc<KnowledgeGraph>,
        Arc<GraphIndexes>,
        Arc<FtsIndex>,
    )> {
        let entry = self.resolve_repo(Some(repo))?.clone();
        let snap_path = PathBuf::from(&entry.storage_path).join("graph.bin");
        let (graph, indexes, fts) = self.load_cached_indexes(&snap_path)?;
        Ok((entry, graph, indexes, fts))
    }

    /// Collect enrichment metadata from a node's properties into a JSON value.
    fn collect_enrichment_for_node(graph: &KnowledgeGraph, node_id: &str) -> Value {
        let node = match graph.get_node(node_id) {
            Some(n) => n,
            None => return json!(null),
        };
        let p = &node.properties;
        json!({
            "description": p.description,
            "enrichedBy": p.enriched_by,
            "complexity": p.complexity,
            "isDeadCandidate": p.is_dead_candidate,
            "isTraced": p.is_traced,
            "traceCallCount": p.trace_call_count,
            "entryPointScore": p.entry_point_score,
            "entryPointReason": p.entry_point_reason,
            "frameworkHint": p.ast_framework_reason,
            "layerType": p.layer_type,
            "httpMethod": p.http_method,
            "routeTemplate": p.route_template,
            "llmSmells": p.llm_smells,
            "llmPatterns": p.llm_patterns,
            "llmRiskScore": p.llm_risk_score,
            "llmRefactoring": p.llm_refactoring,
        })
    }

    /// Read a source file with path-traversal protection.
    fn read_code_snippet(
        repo_path: &std::path::Path,
        file_path: &str,
        start: Option<u32>,
        end: Option<u32>,
    ) -> Option<String> {
        let full_path = repo_path.join(file_path);
        let canonical_repo = repo_path.canonicalize().ok()?;
        let canonical_file = full_path.canonicalize().ok()?;
        if !canonical_file.starts_with(&canonical_repo) {
            return None;
        }
        let content = std::fs::read_to_string(&canonical_file).ok()?;
        let lines: Vec<&str> = content.lines().collect();
        let s = start.unwrap_or(1).saturating_sub(1) as usize;
        let e = end.map(|v| v as usize).unwrap_or(s + 100).max(s);
        Some(lines[s..e.min(lines.len())].join("\n"))
    }

    /// Initialize the backend: load the global registry and discover repos.
    pub fn init(&mut self) -> Result<()> {
        self.registry = repo_manager::read_registry().map_err(McpError::Core)?;
        info!(
            "LocalBackend: loaded {} repos from registry",
            self.registry.len()
        );
        Ok(())
    }

    /// Get the current registry.
    pub fn registry(&self) -> &[RegistryEntry] {
        &self.registry
    }

    /// Resolve a repository by name or path.
    ///
    /// If `repo` is None and there's exactly one registered repo, uses that.
    /// Otherwise searches by name or path (case-insensitive).
    pub fn resolve_repo(&self, repo: Option<&str>) -> Result<&RegistryEntry> {
        match repo {
            None => {
                if self.registry.len() == 1 {
                    Ok(&self.registry[0])
                } else if self.registry.is_empty() {
                    Err(McpError::RepoNotFound(
                        "No repositories indexed. Run `gitnexus analyze` first.".into(),
                    ))
                } else {
                    Err(McpError::InvalidArguments {
                        tool: "resolve_repo".into(),
                        reason: format!(
                            "Multiple repos indexed ({}). Specify 'repo' parameter.",
                            self.registry.len()
                        ),
                    })
                }
            }
            Some(name_or_path) => {
                let lookup_key = normalize_repo_lookup_key(name_or_path);
                self.registry
                    .iter()
                    .find(|e| {
                        if registry_entry_id(e) == name_or_path {
                            return true;
                        }
                        // Exact name / exact path / suffix on a path-segment
                        // boundary. Without the segment guard, "foo" would
                        // also match "/repos/myfoo", silently selecting the
                        // wrong repo for any name that's a tail substring.
                        if e.name.to_lowercase() == lookup_key {
                            return true;
                        }
                        let path_key = normalize_repo_lookup_key(&e.path);
                        if path_key == lookup_key {
                            return true;
                        }
                        !lookup_key.contains('/') && path_key.ends_with(&format!("/{}", lookup_key))
                    })
                    .ok_or_else(|| McpError::RepoNotFound(name_or_path.to_string()))
            }
        }
    }

    /// Dispatch a tool call by name with the given arguments.
    pub async fn call_tool(&mut self, name: &str, args: &Value) -> Result<Value> {
        match name {
            "list_repos" => self.tool_list_repos(),
            "query" => self.tool_query(args).await,
            "context" => self.tool_context(args).await,
            "impact" => self.tool_impact(args).await,
            "detect_changes" => self.tool_detect_changes(args).await,
            "rename" => self.tool_rename(args).await,
            "cypher" => self.tool_cypher(args).await,
            "hotspots" => self.tool_hotspots(args).await,
            "coupling" => self.tool_coupling(args).await,
            "ownership" => self.tool_ownership(args).await,
            "coverage" => self.tool_coverage(args).await,
            "diagram" => self.tool_diagram(args).await,
            "report" => self.tool_report(args).await,
            "business" => self.tool_business(args).await,
            "search_processes" => self.tool_search_processes(args).await,
            "analyze_execution_trace" => self.tool_analyze_execution_trace(args).await,
            "search_code" => self.tool_search_code(args).await,
            "read_file" => self.tool_read_file(args).await,
            "get_insights" => self.tool_get_insights(args).await,
            "save_memory" => self.tool_save_memory(args).await,
            // Code Quality Suite (Theme A)
            "find_cycles" => self.tool_find_cycles(args).await,
            "find_similar_code" => self.tool_find_similar_code(args).await,
            "list_todos" => self.tool_list_todos(args).await,
            "get_complexity" => self.tool_get_complexity(args).await,
            // Schema & API Inventory (Theme D)
            "list_endpoints" => self.tool_list_endpoints(args).await,
            "list_db_tables" => self.tool_list_db_tables(args).await,
            "list_env_vars" => self.tool_list_env_vars(args).await,
            "get_endpoint_handler" => self.tool_get_endpoint_handler(args).await,
            // SFD doc-authoring workflow (P1.1)
            "list_sfd_pages" => self.tool_list_sfd_pages(args).await,
            "write_sfd_draft" => self.tool_write_sfd_draft(args).await,
            "validate_sfd" => self.tool_validate_sfd(args).await,
            _ => Err(McpError::UnknownTool(name.to_string())),
        }
    }

    // ─── Tool Implementations ───────────────────────────────────────

    fn tool_list_repos(&self) -> Result<Value> {
        let repos: Vec<Value> = self
            .registry
            .iter()
            .map(|e| {
                let mut obj = json!({
                    "id": registry_entry_id(e),
                    "name": e.name,
                    "path": e.path,
                    "indexedAt": e.indexed_at,
                    "lastCommit": e.last_commit,
                });
                if let Some(stats) = &e.stats {
                    obj["stats"] = serde_json::to_value(stats).unwrap_or_default();
                }
                obj
            })
            .collect();

        Ok(json!({
            "content": [{
                "type": "text",
                "text": serde_json::to_string_pretty(&repos).unwrap_or_else(|_| "[]".to_string())
            }],
            "_meta": {
                "hint": hints::hint_for("list_repos")
            }
        }))
    }

    async fn tool_query(&mut self, args: &Value) -> Result<Value> {
        let query_text = args.get("query").and_then(|v| v.as_str()).ok_or_else(|| {
            McpError::InvalidArguments {
                tool: "query".into(),
                reason: "Missing required 'query' parameter".into(),
            }
        })?;

        let repo_name = args.get("repo").and_then(|v| v.as_str());
        let limit = clamp_limit(
            args.get("limit").and_then(|v| v.as_u64()),
            10,
            MAX_QUERY_LIMIT,
        );
        // Clients pinned to the old flat-list shape can opt out of grouping.
        let group_by_process = args
            .get("group_by_process")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);

        let snap_path = {
            let entry = self.resolve_repo(repo_name)?;
            std::path::Path::new(&entry.storage_path).join("graph.bin")
        };

        let (graph, _indexes, fts) = self.load_cached_indexes(&snap_path)?;
        let results = fts.search(&graph, query_text, None, limit);

        // Opt-out path: flat results, using the same snapshot FTS data as CLI search.
        if !group_by_process {
            let results_json: Vec<Value> = results.iter().map(fts_result_to_json).collect();
            return Ok(json!({
                "content": [{
                    "type": "text",
                    "text": serde_json::to_string_pretty(&results_json).unwrap_or_else(|_| "[]".to_string())
                }],
                "_meta": {
                    "hint": hints::hint_for("query"),
                    "resultCount": results.len()
                }
            }));
        }

        // Group by Process using the snapshot's STEP_IN_PROCESS edges.
        // Symbols with no process parent go to `definitions` (fallback bucket).
        let mut processes_map: HashMap<String, Value> = HashMap::new();
        let mut process_symbols: Vec<Value> = Vec::new();
        let mut definitions: Vec<Value> = Vec::new();

        // Pre-index: node_id → Vec<(process_id, step_index)>
        let mut symbol_to_processes: HashMap<String, Vec<(String, Option<u32>)>> = HashMap::new();
        let mut process_step_counts: HashMap<String, u32> = HashMap::new();
        for rel in graph.iter_relationships() {
            if rel.rel_type == RelationshipType::StepInProcess {
                symbol_to_processes
                    .entry(rel.source_id.clone())
                    .or_default()
                    .push((rel.target_id.clone(), rel.step));
                *process_step_counts
                    .entry(rel.target_id.clone())
                    .or_insert(0) += 1;
            }
        }

        for r in &results {
            let base = fts_result_to_json(r);
            if let Some(parents) = symbol_to_processes.get(&r.node_id) {
                for (proc_id, step_idx) in parents {
                    let mut enriched = base.clone();
                    enriched["process_id"] = json!(proc_id);
                    enriched["step_index"] = json!(step_idx);
                    process_symbols.push(enriched);

                    processes_map.entry(proc_id.clone()).or_insert_with(|| {
                        if let Some(pn) = graph.get_node(proc_id) {
                            let proc_type = pn
                                .properties
                                .process_type
                                .as_ref()
                                .map(|t| format!("{t:?}"))
                                .unwrap_or_else(|| "unknown".into());
                            let name = pn.properties.name.clone();
                            let description = pn.properties.description.clone();
                            json!({
                                "process_id": proc_id,
                                "name": name,
                                "description": description,
                                "process_type": proc_type,
                                "step_count": process_step_counts.get(proc_id).copied().unwrap_or(0),
                            })
                        } else {
                            json!({"process_id": proc_id})
                        }
                    });
                }
            } else {
                definitions.push(base);
            }
        }

        // Sort processes by descending step_count so the most substantial
        // flows show first in the agent's view.
        let mut processes_vec: Vec<Value> = processes_map.into_values().collect();
        processes_vec.sort_by(|a, b| {
            let sa = a.get("step_count").and_then(|v| v.as_u64()).unwrap_or(0);
            let sb = b.get("step_count").and_then(|v| v.as_u64()).unwrap_or(0);
            sb.cmp(&sa)
        });

        Ok(json!({
            "content": [{
                "type": "text",
                "text": serde_json::to_string_pretty(&json!({
                    "processes": processes_vec,
                    "process_symbols": process_symbols,
                    "definitions": definitions,
                })).unwrap_or_else(|_| "{}".to_string())
            }],
            "_meta": {
                "hint": hints::hint_for("query"),
                "resultCount": results.len(),
                "processCount": processes_vec.len(),
            }
        }))
    }

    async fn tool_context(&mut self, args: &Value) -> Result<Value> {
        let name = args.get("name").and_then(|v| v.as_str()).ok_or_else(|| {
            McpError::InvalidArguments {
                tool: "context".into(),
                reason: "Missing required 'name' parameter".into(),
            }
        })?;

        let repo_name = args.get("repo").and_then(|v| v.as_str());
        let uid = args.get("uid").and_then(|v| v.as_str());
        let file_filter = args.get("file").and_then(|v| v.as_str());

        // Previously this tool issued a Cypher query using map literals
        // (`collect(DISTINCT {rel: ..., target: ...})`). The default
        // in-memory Cypher executor does not support map literal syntax,
        // so the query always failed to parse on real data. Implement the
        // 360° lookup directly against the loaded snapshot — same pattern
        // as `tool_impact` below.
        let snap_path = {
            let entry = self.resolve_repo(repo_name)?;
            std::path::Path::new(&entry.storage_path).join("graph.bin")
        };
        let query_start = Instant::now();
        let graph = self.load_cached_snapshot(&snap_path)?;

        // Resolve the target node id. Priority: explicit uid → exact name
        // (+ optional file filter) → case-insensitive exact name.
        let node_id: Option<String> = if let Some(uid_val) = uid {
            graph.get_node(uid_val).map(|n| n.id.clone())
        } else {
            let name_lower = name.to_lowercase();
            graph
                .iter_nodes()
                .find(|n| {
                    let name_match =
                        n.properties.name == name || n.properties.name.to_lowercase() == name_lower;
                    let file_match = match file_filter {
                        Some(f) => n.properties.file_path == f,
                        None => true,
                    };
                    name_match && file_match
                })
                .map(|n| n.id.clone())
        };

        let Some(node_id) = node_id else {
            let duration = query_start.elapsed();
            return Ok(json!({
                "content": [{
                    "type": "text",
                    "text": format!("Symbol '{}' not found.", name)
                }],
                "_meta": {
                    "hint": hints::hint_for("context"),
                    "durationMs": duration.as_millis() as u64,
                    "enrichment": json!(null)
                }
            }));
        };

        // Build outgoing / incoming relationship lists by iterating the graph.
        // Deduplicate on (rel_type, other_id) to match the old
        // `collect(DISTINCT ...)` semantics.
        let mut outgoing_seen = std::collections::HashSet::new();
        let mut incoming_seen = std::collections::HashSet::new();
        let mut outgoing: Vec<Value> = Vec::new();
        let mut incoming: Vec<Value> = Vec::new();
        for rel in graph.iter_relationships() {
            let rel_type = rel.rel_type.as_str();
            if rel.source_id == node_id && outgoing_seen.insert((rel_type, rel.target_id.clone())) {
                let target_name = graph
                    .get_node(&rel.target_id)
                    .map(|n| n.properties.name.clone())
                    .unwrap_or_default();
                outgoing.push(json!({
                    "rel": rel_type,
                    "target": target_name,
                    "targetId": rel.target_id,
                }));
            }
            if rel.target_id == node_id && incoming_seen.insert((rel_type, rel.source_id.clone())) {
                let source_name = graph
                    .get_node(&rel.source_id)
                    .map(|n| n.properties.name.clone())
                    .unwrap_or_default();
                incoming.push(json!({
                    "rel": rel_type,
                    "source": source_name,
                    "sourceId": rel.source_id,
                }));
            }
        }

        // Cap the payload so we don't flood the LLM context with hub nodes.
        const MAX_EDGES_PER_DIRECTION: usize = 1000;
        if outgoing.len() > MAX_EDGES_PER_DIRECTION {
            tracing::warn!(
                total = outgoing.len(),
                "context: truncating outgoing edges to {}",
                MAX_EDGES_PER_DIRECTION
            );
            outgoing.truncate(MAX_EDGES_PER_DIRECTION);
        }
        if incoming.len() > MAX_EDGES_PER_DIRECTION {
            tracing::warn!(
                total = incoming.len(),
                "context: truncating incoming edges to {}",
                MAX_EDGES_PER_DIRECTION
            );
            incoming.truncate(MAX_EDGES_PER_DIRECTION);
        }

        let node = graph.get_node(&node_id).expect("node_id was just resolved");
        let p = &node.properties;
        let node_json = json!({
            "id": node.id,
            "label": node.label.as_str(),
            "name": p.name,
            "filePath": p.file_path,
            "startLine": p.start_line,
            "endLine": p.end_line,
        });

        let enrichment = Self::collect_enrichment_for_node(&graph, &node_id);
        let duration = query_start.elapsed();
        if duration.as_secs() > 5 {
            tracing::warn!(
                node_id = %node_id,
                duration_ms = duration.as_millis() as u64,
                "Slow context lookup detected"
            );
        }

        let results = vec![json!({
            "n": node_json,
            "outgoing": outgoing,
            "incoming": incoming,
        })];

        Ok(json!({
            "content": [{
                "type": "text",
                "text": query::format_query_result(&results)
            }],
            "_meta": {
                "hint": hints::hint_for("context"),
                "durationMs": duration.as_millis() as u64,
                "enrichment": enrichment
            }
        }))
    }

    async fn tool_impact(&mut self, args: &Value) -> Result<Value> {
        let target = args.get("target").and_then(|v| v.as_str()).ok_or_else(|| {
            McpError::InvalidArguments {
                tool: "impact".into(),
                reason: "Missing required 'target' parameter".into(),
            }
        })?;

        let repo_name = args.get("repo").and_then(|v| v.as_str());
        let direction = args
            .get("direction")
            .and_then(|v| v.as_str())
            .unwrap_or("both");
        let max_depth = args
            .get("max_depth")
            .and_then(|v| v.as_u64())
            .unwrap_or(5)
            .clamp(1, MAX_IMPACT_DEPTH as u64) as usize;

        // Previously this tool issued Cypher queries with variable-length
        // path syntax (`[r:CALLS|USES|IMPORTS*1..N]`). The default in-memory
        // backend's Cypher executor only supports single relationship types
        // and no variable-length paths, so the queries always failed to
        // parse — and the `if let Ok(rows) = ...` pattern silently swallowed
        // the parser error, leaving every caller with an empty result and no
        // diagnostic. Implement the BFS directly against the loaded snapshot
        // so the tool actually returns the blast radius.
        let snap_path = {
            let entry = self.resolve_repo(repo_name)?;
            std::path::Path::new(&entry.storage_path).join("graph.bin")
        };
        let graph = self.load_cached_snapshot(&snap_path)?;

        // Find target node(s) by exact id, then by exact name (case-insensitive).
        let target_lower = target.to_lowercase();
        let target_ids: std::collections::HashSet<String> = graph
            .iter_nodes()
            .filter(|n| n.id == target || n.properties.name.to_lowercase() == target_lower)
            .map(|n| n.id.clone())
            .collect();

        if target_ids.is_empty() {
            return Err(McpError::Internal(format!(
                "Symbol '{}' not found in graph",
                target
            )));
        }

        // Build forward (source -> targets) and reverse (target -> sources)
        // adjacency only over the impact-bearing relationship types so the
        // BFS doesn't drift into structural edges like HasMethod / Defines.
        //
        // Keep this set in sync with the canonical causal-edge filter used
        // by `impact.rs::bfs_impact`, `impact_cmd.rs`, `shell.rs::cmd_impact`
        // and `chat_executor.rs::execute_impact`. Previously this tool only
        // walked Calls/Uses/Imports/CallsAction/CallsService/DependsOn,
        // missing every impact that flows through inheritance (changing a
        // base class affects subclasses), interface implementation, view
        // rendering, routing, HTTP fetch, or entity mapping. Same root
        // cause as #39, #52, #55 in the other impact code paths.
        let want_rel = |rt: RelationshipType| {
            matches!(
                rt,
                RelationshipType::Calls
                    | RelationshipType::CallsAction
                    | RelationshipType::CallsService
                    | RelationshipType::Imports
                    | RelationshipType::Uses
                    | RelationshipType::DependsOn
                    | RelationshipType::Inherits
                    | RelationshipType::Implements
                    | RelationshipType::Extends
                    | RelationshipType::Overrides
                    | RelationshipType::RendersView
                    | RelationshipType::HandlesRoute
                    | RelationshipType::Fetches
                    | RelationshipType::MapsToEntity
            )
        };

        let mut forward: HashMap<String, Vec<String>> = HashMap::new();
        let mut reverse: HashMap<String, Vec<String>> = HashMap::new();
        for rel in graph.iter_relationships() {
            if !want_rel(rel.rel_type) {
                continue;
            }
            forward
                .entry(rel.source_id.clone())
                .or_default()
                .push(rel.target_id.clone());
            reverse
                .entry(rel.target_id.clone())
                .or_default()
                .push(rel.source_id.clone());
        }

        // BFS up to `max_depth`. Returns Vec<(node_id, depth)> in BFS order
        // so the caller sees nearest neighbours first. The seed targets
        // themselves are excluded from the result so callers don't show up
        // as their own impact.
        let bfs = |adjacency: &HashMap<String, Vec<String>>| -> Vec<(String, usize)> {
            let mut visited: std::collections::HashSet<String> =
                target_ids.iter().cloned().collect();
            let mut queue: std::collections::VecDeque<(String, usize)> =
                target_ids.iter().map(|id| (id.clone(), 0usize)).collect();
            let mut out: Vec<(String, usize)> = Vec::new();
            while let Some((node, depth)) = queue.pop_front() {
                if depth >= max_depth {
                    continue;
                }
                if let Some(neighbors) = adjacency.get(&node) {
                    for n in neighbors {
                        if visited.insert(n.clone()) {
                            out.push((n.clone(), depth + 1));
                            queue.push_back((n.clone(), depth + 1));
                        }
                    }
                }
            }
            out
        };

        let format_rows = |rows: Vec<(String, usize)>| -> Vec<Value> {
            rows.into_iter()
                .filter_map(|(id, depth)| {
                    let node = graph.get_node(&id)?;
                    Some(json!({
                        "name": node.properties.name,
                        "id": node.id,
                        "filePath": node.properties.file_path,
                        "label": node.label.as_str(),
                        "depth": depth,
                    }))
                })
                .collect()
        };

        let mut all_results: Vec<Value> = Vec::new();

        if direction == "upstream" || direction == "both" {
            let callers = format_rows(bfs(&reverse));
            all_results.push(json!({
                "direction": "upstream",
                "callers": callers,
            }));
        }

        if direction == "downstream" || direction == "both" {
            let callees = format_rows(bfs(&forward));
            all_results.push(json!({
                "direction": "downstream",
                "callees": callees,
            }));
        }

        // Collect enrichment for the target node(s)
        let enrichment = target_ids
            .iter()
            .next()
            .map(|nid| Self::collect_enrichment_for_node(&graph, nid))
            .unwrap_or(json!(null));

        Ok(json!({
            "content": [{
                "type": "text",
                "text": serde_json::to_string_pretty(&all_results).unwrap_or_else(|_| "[]".to_string())
            }],
            "_meta": {
                "hint": hints::hint_for("impact"),
                "target": target,
                "direction": direction,
                "maxDepth": max_depth,
                "enrichment": enrichment
            }
        }))
    }

    async fn tool_detect_changes(&mut self, args: &Value) -> Result<Value> {
        let repo_name = args.get("repo").and_then(|v| v.as_str());
        let max_upstream_depth = args
            .get("max_upstream_depth")
            .and_then(|v| v.as_u64())
            .unwrap_or(3)
            .clamp(1, MAX_IMPACT_DEPTH as u64) as usize;

        let (repo_path, snap_path) = {
            let entry = self.resolve_repo(repo_name)?;
            (
                std::path::PathBuf::from(&entry.path),
                std::path::Path::new(&entry.storage_path).join("graph.bin"),
            )
        };

        // Parse `git diff --unified=0 HEAD` to get file + line-range hunks.
        let diff_hunks = collect_git_diff_hunks(&repo_path);
        let untracked_files = collect_git_untracked(&repo_path);
        let changed_files: Vec<String> = diff_hunks.iter().map(|(p, _)| p.clone()).collect();

        // Load graph; if absent, degrade gracefully to a file-list response.
        let graph = match self.load_cached_snapshot(&snap_path) {
            Ok(g) => g,
            Err(_) => {
                return Ok(json!({
                    "content": [{
                        "type": "text",
                        "text": serde_json::to_string_pretty(&json!({
                            "summary": {
                                "changed_files": changed_files.len(),
                                "changed_count": 0,
                                "affected_count": 0,
                                "risk_level": classify_risk(0, 0, changed_files.len()),
                            },
                            "changed_files": changed_files,
                            "untracked_files": untracked_files,
                            "note": "No graph snapshot available — run `gitnexus analyze` to enable symbol/process mapping.",
                            "modified": changed_files,
                            "untracked": untracked_files,
                            "totalChanges": changed_files.len() + untracked_files.len(),
                        })).unwrap_or_default()
                    }],
                    "_meta": {"hint": hints::hint_for("detect_changes")}
                }));
            }
        };

        // Identify directly-changed symbols: nodes whose line range overlaps a diff hunk.
        let mut changed_symbols: Vec<Value> = Vec::new();
        let mut changed_node_ids: std::collections::HashSet<String> =
            std::collections::HashSet::new();

        for (file_path, ranges) in &diff_hunks {
            let Some(node_ids) = graph.nodes_by_file(file_path) else {
                continue;
            };
            for nid in node_ids {
                let Some(node) = graph.get_node(nid) else {
                    continue;
                };
                let ns = node.properties.start_line.unwrap_or(0);
                let ne = node.properties.end_line.unwrap_or(u32::MAX);
                if ranges
                    .iter()
                    .any(|(rs, re)| ranges_overlap(ns, ne, *rs, *re))
                {
                    changed_symbols.push(json!({
                        "id": nid,
                        "name": node.properties.name,
                        "label": node.label.as_str(),
                        "filePath": file_path,
                        "startLine": ns,
                        "endLine": ne,
                    }));
                    changed_node_ids.insert(nid.clone());
                }
            }
        }

        // Build reverse causal adjacency (same filter as tool_impact).
        let want_rel = |rt: RelationshipType| {
            matches!(
                rt,
                RelationshipType::Calls
                    | RelationshipType::CallsAction
                    | RelationshipType::CallsService
                    | RelationshipType::Imports
                    | RelationshipType::Uses
                    | RelationshipType::DependsOn
                    | RelationshipType::Inherits
                    | RelationshipType::Implements
                    | RelationshipType::Extends
                    | RelationshipType::Overrides
                    | RelationshipType::RendersView
                    | RelationshipType::HandlesRoute
                    | RelationshipType::Fetches
                    | RelationshipType::MapsToEntity
            )
        };
        let mut reverse: HashMap<String, Vec<String>> = HashMap::new();
        let mut step_in_process: HashMap<String, Vec<(String, Option<u32>)>> = HashMap::new();
        for rel in graph.iter_relationships() {
            if want_rel(rel.rel_type) {
                reverse
                    .entry(rel.target_id.clone())
                    .or_default()
                    .push(rel.source_id.clone());
            }
            if rel.rel_type == RelationshipType::StepInProcess {
                step_in_process
                    .entry(rel.source_id.clone())
                    .or_default()
                    .push((rel.target_id.clone(), rel.step));
            }
        }

        // BFS upstream from changed nodes to collect transitively-affected nodes.
        let mut affected: std::collections::HashSet<String> = changed_node_ids.clone();
        let mut queue: std::collections::VecDeque<(String, usize)> = changed_node_ids
            .iter()
            .map(|id| (id.clone(), 0usize))
            .collect();
        while let Some((node, depth)) = queue.pop_front() {
            if depth >= max_upstream_depth {
                continue;
            }
            if let Some(neighbors) = reverse.get(&node) {
                for n in neighbors {
                    if affected.insert(n.clone()) {
                        queue.push_back((n.clone(), depth + 1));
                    }
                }
            }
        }

        // Collect affected processes: any Process that has a step node in `affected`.
        let mut affected_process_ids: std::collections::HashSet<String> =
            std::collections::HashSet::new();
        for nid in &affected {
            if let Some(procs) = step_in_process.get(nid) {
                for (pid, _) in procs {
                    affected_process_ids.insert(pid.clone());
                }
            }
        }
        let affected_processes: Vec<Value> = affected_process_ids
            .iter()
            .filter_map(|pid| {
                let pn = graph.get_node(pid)?;
                Some(json!({
                    "id": pid,
                    "name": pn.properties.name,
                    "description": pn.properties.description,
                    "stepCount": pn.properties.step_count,
                }))
            })
            .collect();

        // Sample the top-N non-direct affected nodes so agents see concrete
        // examples of the blast radius without exploding response size.
        let mut affected_sample: Vec<Value> = Vec::new();
        for nid in affected
            .iter()
            .filter(|id| !changed_node_ids.contains(*id))
            .take(25)
        {
            if let Some(n) = graph.get_node(nid) {
                affected_sample.push(json!({
                    "id": nid,
                    "name": n.properties.name,
                    "label": n.label.as_str(),
                    "filePath": n.properties.file_path,
                }));
            }
        }

        let risk_level = classify_risk(
            changed_symbols.len(),
            affected.len().saturating_sub(changed_symbols.len()),
            affected_processes.len(),
        );

        Ok(json!({
            "content": [{
                "type": "text",
                "text": serde_json::to_string_pretty(&json!({
                    "summary": {
                        "changed_files": changed_files.len(),
                        "changed_count": changed_symbols.len(),
                        "affected_count": affected.len().saturating_sub(changed_symbols.len()),
                        "affected_processes": affected_processes.len(),
                        "risk_level": risk_level,
                    },
                    "changed_files": changed_files,
                    "untracked_files": untracked_files,
                    "changed_symbols": changed_symbols,
                    "affected_sample": affected_sample,
                    "affected_processes": affected_processes,
                    // Backward-compat fields for older MCP clients:
                    "modified": changed_files,
                    "untracked": untracked_files,
                    "totalChanges": changed_files.len() + untracked_files.len(),
                })).unwrap_or_default()
            }],
            "_meta": {
                "hint": hints::hint_for("detect_changes"),
                "riskLevel": risk_level,
            }
        }))
    }

    async fn tool_rename(&mut self, args: &Value) -> Result<Value> {
        let target = args.get("target").and_then(|v| v.as_str()).ok_or_else(|| {
            McpError::InvalidArguments {
                tool: "rename".into(),
                reason: "Missing required 'target' parameter".into(),
            }
        })?;

        let new_name = args
            .get("new_name")
            .and_then(|v| v.as_str())
            .ok_or_else(|| McpError::InvalidArguments {
                tool: "rename".into(),
                reason: "Missing required 'new_name' parameter".into(),
            })?;

        // dry_run defaults to true — the tool never mutates files unless the
        // caller explicitly opts in. Apply path is intentionally deferred;
        // agents should feed the returned patches to their editor.
        let dry_run = args
            .get("dry_run")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);

        let repo_name = args.get("repo").and_then(|v| v.as_str());
        let (repo_path, snap_path) = {
            let entry = self.resolve_repo(repo_name)?;
            (
                std::path::PathBuf::from(&entry.path),
                std::path::Path::new(&entry.storage_path).join("graph.bin"),
            )
        };

        let graph = self.load_cached_snapshot(&snap_path)?;

        // Resolve target nodes by id, then by exact name (case-insensitive).
        let target_lower = target.to_lowercase();
        let target_ids: std::collections::HashSet<String> = graph
            .iter_nodes()
            .filter(|n| n.id == target || n.properties.name.to_lowercase() == target_lower)
            .map(|n| n.id.clone())
            .collect();

        if target_ids.is_empty() {
            return Err(McpError::Internal(format!(
                "Symbol '{target}' not found in graph"
            )));
        }

        // Collect source node ids of every edge pointing AT the target.
        // We restrict to edges that semantically imply a textual name use.
        let ref_edges: Vec<(String, RelationshipType)> = graph
            .iter_relationships()
            .filter(|r| target_ids.contains(&r.target_id))
            .filter(|r| {
                matches!(
                    r.rel_type,
                    RelationshipType::Calls
                        | RelationshipType::Uses
                        | RelationshipType::Imports
                        | RelationshipType::Inherits
                        | RelationshipType::Implements
                        | RelationshipType::Extends
                        | RelationshipType::Overrides
                        | RelationshipType::CallsAction
                        | RelationshipType::CallsService
                )
            })
            .map(|r| (r.source_id.clone(), r.rel_type))
            .collect();

        // `graph_edits` = token occurrences inside a node whose relationship
        // to the target is graph-confirmed. Confidence 0.9–1.0.
        let mut graph_edits: Vec<Value> = Vec::new();
        let mut covered: std::collections::HashSet<(String, u32)> =
            std::collections::HashSet::new();

        let word_re = regex::Regex::new(&format!(r"\b{}\b", regex::escape(target))).ok();

        // 1) Definition sites.
        for tid in &target_ids {
            if let Some(tn) = graph.get_node(tid) {
                if let Some(re) = word_re.as_ref() {
                    for edit in scan_node_occurrences(&repo_path, tn, re, new_name) {
                        let key = (
                            edit.get("file")
                                .and_then(|v| v.as_str())
                                .unwrap_or("")
                                .to_string(),
                            edit.get("line").and_then(|v| v.as_u64()).unwrap_or(0) as u32,
                        );
                        if covered.insert(key) {
                            let mut ed = edit;
                            ed["confidence"] = json!(1.0);
                            ed["reason"] = json!("definition");
                            graph_edits.push(ed);
                        }
                    }
                }
            }
        }

        // 2) Reference sites (scoped by source node's line range).
        let mut seen_sources = std::collections::HashSet::new();
        for (src_id, rel_type) in &ref_edges {
            if !seen_sources.insert(src_id.clone()) {
                continue;
            }
            let Some(src) = graph.get_node(src_id) else {
                continue;
            };
            if let Some(re) = word_re.as_ref() {
                for edit in scan_node_occurrences(&repo_path, src, re, new_name) {
                    let key = (
                        edit.get("file")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string(),
                        edit.get("line").and_then(|v| v.as_u64()).unwrap_or(0) as u32,
                    );
                    if covered.insert(key) {
                        let mut ed = edit;
                        ed["confidence"] = json!(0.9);
                        ed["reason"] = json!(format!("reference via {}", rel_type.as_str()));
                        graph_edits.push(ed);
                    }
                }
            }
        }

        // `text_search_edits` = everything else ripgrep finds for \btarget\b
        // that isn't already in graph_edits. Lower confidence — the agent
        // should review. Respects .gitignore via the `ignore` crate.
        let text_search_edits: Vec<Value> = if let Some(re) = word_re.as_ref() {
            scan_repo_for_identifier(&repo_path, re, new_name, &covered)
        } else {
            Vec::new()
        };

        let files_affected: std::collections::HashSet<String> = graph_edits
            .iter()
            .chain(text_search_edits.iter())
            .filter_map(|e| e.get("file").and_then(|v| v.as_str()).map(String::from))
            .collect();

        // Apply phase: only when dry_run=false AND every edit is high-confidence.
        // Never auto-apply text_search_edits — they require human review.
        let applied = if !dry_run && !graph_edits.is_empty() {
            apply_edits_to_disk(&repo_path, &graph_edits).ok()
        } else {
            None
        };

        let target_nodes_json: Vec<Value> = target_ids
            .iter()
            .filter_map(|id| graph.get_node(id))
            .map(|n| {
                json!({
                    "id": n.id,
                    "name": n.properties.name,
                    "label": n.label.as_str(),
                    "filePath": n.properties.file_path,
                    "startLine": n.properties.start_line,
                    "endLine": n.properties.end_line,
                })
            })
            .collect();

        Ok(json!({
            "content": [{
                "type": "text",
                "text": serde_json::to_string_pretty(&json!({
                    "status": "success",
                    "target": target,
                    "new_name": new_name,
                    "dry_run": dry_run,
                    "files_affected": files_affected.len(),
                    "total_edits": graph_edits.len() + text_search_edits.len(),
                    "graph_edits_count": graph_edits.len(),
                    "text_search_edits_count": text_search_edits.len(),
                    "target_nodes": target_nodes_json,
                    "graph_edits": graph_edits,
                    "text_search_edits": text_search_edits,
                    "applied": applied,
                })).unwrap_or_default()
            }],
            "_meta": {
                "hint": hints::hint_for("rename"),
                "dryRun": dry_run,
            }
        }))
    }

    async fn tool_cypher(&self, args: &Value) -> Result<Value> {
        let cypher = args.get("query").and_then(|v| v.as_str()).ok_or_else(|| {
            McpError::InvalidArguments {
                tool: "cypher".into(),
                reason: "Missing required 'query' parameter".into(),
            }
        })?;

        // Reject write queries
        if query::is_write_query(cypher) {
            return Err(McpError::WriteQueryRejected);
        }

        let repo_name = args.get("repo").and_then(|v| v.as_str());
        let entry = self.resolve_repo(repo_name)?;
        let db_path = std::path::Path::new(&entry.storage_path).join("db");
        let adapter = self.pool.get_or_open(&db_path).map_err(McpError::Db)?;

        let query_start = Instant::now();
        let mut results = adapter.execute_query(cypher).map_err(McpError::Db)?;
        let duration = query_start.elapsed();

        if duration.as_secs() > 5 {
            tracing::warn!(
                query = %cypher,
                duration_ms = duration.as_millis() as u64,
                "Slow Cypher query detected"
            );
        }

        let total_rows = results.len();
        let truncated = total_rows > 1000;
        if truncated {
            tracing::warn!(
                total_rows = total_rows,
                "Query returned {} results, truncating to 1000",
                total_rows
            );
            results.truncate(1000);
        }

        Ok(json!({
            "content": [{
                "type": "text",
                "text": query::format_query_result(&results)
            }],
            "_meta": {
                "hint": hints::hint_for("cypher"),
                "rowCount": results.len(),
                "totalRows": total_rows,
                "truncated": truncated,
                "durationMs": duration.as_millis() as u64
            }
        }))
    }

    async fn tool_hotspots(&self, args: &Value) -> Result<Value> {
        let repo_name = args.get("repo").and_then(|v| v.as_str());
        let since_days = args
            .get("since_days")
            .and_then(|v| v.as_u64())
            .unwrap_or(90) as u32;
        let limit = args.get("limit").and_then(|v| v.as_u64());
        let limit = clamp_limit(limit, 20, MAX_ANALYTICS_LIMIT);

        let entry = self.resolve_repo(repo_name)?;
        let repo_path = std::path::Path::new(&entry.path);

        let mut hotspots = match gitnexus_git::hotspots::analyze_hotspots(repo_path, since_days) {
            Ok(hotspots) => hotspots,
            Err(HotspotError::NotGitRepo(_)) => return Err(git_history_required_error("hotspots")),
            Err(e) => return Err(McpError::Internal(e.to_string())),
        };

        hotspots.truncate(limit);

        let results: Vec<Value> = hotspots
            .iter()
            .map(|h| serde_json::to_value(h).unwrap_or_default())
            .collect();

        Ok(json!({
            "content": [{
                "type": "text",
                "text": serde_json::to_string_pretty(&results).unwrap_or_else(|_| "[]".to_string())
            }],
            "_meta": {
                "hint": hints::hint_for("hotspots"),
                "resultCount": results.len(),
                "sinceDays": since_days
            }
        }))
    }

    async fn tool_coupling(&self, args: &Value) -> Result<Value> {
        let repo_name = args.get("repo").and_then(|v| v.as_str());
        let min_shared = args.get("min_shared").and_then(|v| v.as_u64()).unwrap_or(3) as u32;
        let limit = args.get("limit").and_then(|v| v.as_u64());
        let limit = clamp_limit(limit, 20, MAX_ANALYTICS_LIMIT);

        let entry = self.resolve_repo(repo_name)?;
        let repo_path = std::path::Path::new(&entry.path);

        let mut couplings =
            match gitnexus_git::coupling::analyze_coupling(repo_path, min_shared, Some(180)) {
                Ok(couplings) => couplings,
                Err(CouplingError::NotGitRepo(_)) => {
                    return Err(git_history_required_error("coupling"))
                }
                Err(e) => return Err(McpError::Internal(e.to_string())),
            };

        couplings.truncate(limit);

        let results: Vec<Value> = couplings
            .iter()
            .map(|c| serde_json::to_value(c).unwrap_or_default())
            .collect();

        Ok(json!({
            "content": [{
                "type": "text",
                "text": serde_json::to_string_pretty(&results).unwrap_or_else(|_| "[]".to_string())
            }],
            "_meta": {
                "hint": hints::hint_for("coupling"),
                "resultCount": results.len(),
                "minShared": min_shared
            }
        }))
    }

    async fn tool_ownership(&self, args: &Value) -> Result<Value> {
        let repo_name = args.get("repo").and_then(|v| v.as_str());
        let limit = args.get("limit").and_then(|v| v.as_u64());
        let limit = clamp_limit(limit, 20, MAX_ANALYTICS_LIMIT);

        let entry = self.resolve_repo(repo_name)?;
        let repo_path = std::path::Path::new(&entry.path);

        let mut ownerships = match gitnexus_git::ownership::analyze_ownership(repo_path) {
            Ok(ownerships) => ownerships,
            Err(OwnershipError::NotGitRepo(_)) => {
                return Err(git_history_required_error("ownership"));
            }
            Err(e) => return Err(McpError::Internal(e.to_string())),
        };

        ownerships.truncate(limit);

        let results: Vec<Value> = ownerships
            .iter()
            .map(|o| serde_json::to_value(o).unwrap_or_default())
            .collect();

        Ok(json!({
            "content": [{
                "type": "text",
                "text": serde_json::to_string_pretty(&results).unwrap_or_else(|_| "[]".to_string())
            }],
            "_meta": {
                "hint": hints::hint_for("ownership"),
                "resultCount": results.len()
            }
        }))
    }

    async fn tool_coverage(&mut self, args: &Value) -> Result<Value> {
        let repo_name = args.get("repo").and_then(|v| v.as_str());
        let target = args.get("target").and_then(|v| v.as_str());

        let snap_path = {
            let entry = self.resolve_repo(repo_name)?;
            std::path::Path::new(&entry.storage_path).join("graph.bin")
        };

        let graph = self.load_cached_snapshot(&snap_path)?;

        // Build class -> methods index. Dead-code status comes from the
        // `is_dead_candidate` property already computed by the dead-code phase,
        // so we no longer need to recompute incoming-call sets here.
        let mut class_methods: HashMap<String, Vec<String>> = HashMap::new();
        for rel in graph.iter_relationships() {
            if rel.rel_type == RelationshipType::HasMethod {
                class_methods
                    .entry(rel.source_id.clone())
                    .or_default()
                    .push(rel.target_id.clone());
            }
        }

        let result = if let Some(target_name) = target {
            // Single class coverage
            let target_lower = target_name.to_lowercase();
            let class_node = graph.iter_nodes().find(|n| {
                n.properties.name.to_lowercase() == target_lower
                    && matches!(
                        n.label,
                        NodeLabel::Class | NodeLabel::Service | NodeLabel::Controller
                    )
            });

            if let Some(cn) = class_node {
                let methods = class_methods.get(&cn.id).cloned().unwrap_or_default();
                let total = methods.len();
                let traced = methods
                    .iter()
                    .filter(|mid| {
                        graph
                            .get_node(mid)
                            .is_some_and(|n| n.properties.is_traced == Some(true))
                    })
                    .count();
                // Use the `is_dead_candidate` property computed by the dead-code phase,
                // which correctly excludes entry points, tests, interface methods,
                // view scripts, and ControllerAction-paired methods. Recomputing
                // dead-status here from incoming_calls alone overcounts dead methods.
                let dead = methods
                    .iter()
                    .filter(|mid| {
                        graph
                            .get_node(mid)
                            .and_then(|n| n.properties.is_dead_candidate)
                            .unwrap_or(false)
                    })
                    .count();

                json!({
                    "class": cn.properties.name,
                    "totalMethods": total,
                    "tracedMethods": traced,
                    "deadCodeCandidates": dead,
                    "coveragePct": if total > 0 { (traced as f64 / total as f64 * 100.0).round() } else { 0.0 }
                })
            } else {
                json!({"error": format!("Class '{}' not found", target_name)})
            }
        } else {
            // Global coverage
            let all_methods: Vec<_> = graph
                .iter_nodes()
                .filter(|n| n.label == NodeLabel::Method || n.label == NodeLabel::Function)
                .collect();
            let total = all_methods.len();
            let traced = all_methods
                .iter()
                .filter(|n| n.properties.is_traced == Some(true))
                .count();
            // Same fix as the per-class branch: use is_dead_candidate from the
            // dead-code phase, which respects entry-point/test/interface exclusions.
            let dead = all_methods
                .iter()
                .filter(|n| n.properties.is_dead_candidate == Some(true))
                .count();

            json!({
                "totalMethods": total,
                "tracedMethods": traced,
                "deadCodeCandidates": dead,
                "coveragePct": if total > 0 { (traced as f64 / total as f64 * 100.0).round() } else { 0.0 }
            })
        };

        Ok(json!({
            "content": [{
                "type": "text",
                "text": serde_json::to_string_pretty(&result).unwrap_or_default()
            }],
            "_meta": {
                "hint": hints::hint_for("coverage")
            }
        }))
    }

    async fn tool_diagram(&mut self, args: &Value) -> Result<Value> {
        let target = args.get("target").and_then(|v| v.as_str()).ok_or_else(|| {
            McpError::InvalidArguments {
                tool: "diagram".into(),
                reason: "Missing required 'target' parameter".into(),
            }
        })?;

        let repo_name = args.get("repo").and_then(|v| v.as_str());
        let diagram_type = args
            .get("type")
            .and_then(|v| v.as_str())
            .unwrap_or("flowchart");

        // The MCP diagram tool currently only emits Mermaid `graph TD` flowcharts.
        // Reject other types up front instead of silently ignoring them — the
        // previous behaviour echoed `diagramType` in `_meta` while always
        // returning a flowchart, misleading callers.
        if diagram_type != "flowchart" {
            return Err(McpError::InvalidArguments {
                tool: "diagram".into(),
                reason: format!(
                    "Unsupported diagram type '{}'; only 'flowchart' is supported",
                    diagram_type
                ),
            });
        }

        let snap_path = {
            let entry = self.resolve_repo(repo_name)?;
            std::path::Path::new(&entry.storage_path).join("graph.bin")
        };

        let graph = self.load_cached_snapshot(&snap_path)?;

        // Find the target symbol
        let target_lower = target.to_lowercase();
        let mut candidates: Vec<_> = graph
            .iter_nodes()
            .filter(|n| n.properties.name.to_lowercase() == target_lower)
            .collect();
        candidates.sort_by_key(|n| match n.label {
            NodeLabel::Controller => 0,
            NodeLabel::Class => 1,
            NodeLabel::Service => 2,
            NodeLabel::Interface => 3,
            _ => 10,
        });

        let start_node = candidates
            .first()
            .ok_or_else(|| McpError::Internal(format!("Symbol '{}' not found", target)))?;

        // Generate simple flowchart showing calls from this symbol's methods
        let mut lines = Vec::new();
        lines.push("graph TD".to_string());

        let node_id = &start_node.id;
        let node_name = &start_node.properties.name;
        lines.push(format!(
            "    {}[\"{}\"]",
            sanitize_mermaid_id(node_id),
            escape_mermaid_label(node_name)
        ));

        // Pre-build set of child IDs to avoid O(E^2) inner loop
        let child_ids: std::collections::HashSet<&str> = graph
            .iter_relationships()
            .filter(|r| {
                r.source_id == *node_id
                    && matches!(
                        r.rel_type,
                        RelationshipType::HasMethod | RelationshipType::HasAction
                    )
            })
            .map(|r| r.target_id.as_str())
            .collect();

        // Find methods and their calls
        for rel in graph.iter_relationships() {
            if (&rel.source_id == node_id || child_ids.contains(rel.source_id.as_str()))
                && matches!(
                    rel.rel_type,
                    RelationshipType::Calls
                        | RelationshipType::CallsAction
                        | RelationshipType::CallsService
                )
            {
                if let Some(target_node) = graph.get_node(&rel.target_id) {
                    let src_name = graph
                        .get_node(&rel.source_id)
                        .map(|n| n.properties.name.as_str())
                        .unwrap_or("?");
                    lines.push(format!(
                        "    {}[\"{}\"] --> {}[\"{}\"]",
                        sanitize_mermaid_id(&rel.source_id),
                        escape_mermaid_label(src_name),
                        sanitize_mermaid_id(&rel.target_id),
                        escape_mermaid_label(&target_node.properties.name),
                    ));
                }
            }
        }

        if lines.len() == 2 {
            // No calls found, show class members instead
            for rel in graph.iter_relationships() {
                if &rel.source_id == node_id && matches!(rel.rel_type, RelationshipType::HasMethod)
                {
                    if let Some(member) = graph.get_node(&rel.target_id) {
                        lines.push(format!(
                            "    {} --> {}[\"{}\"]",
                            sanitize_mermaid_id(node_id),
                            sanitize_mermaid_id(&rel.target_id),
                            escape_mermaid_label(&member.properties.name),
                        ));
                    }
                }
            }
        }

        let mermaid = lines.join("\n");

        Ok(json!({
            "content": [{
                "type": "text",
                "text": format!("```mermaid\n{}\n```", mermaid)
            }],
            "_meta": {
                "hint": hints::hint_for("diagram"),
                "diagramType": diagram_type,
                "target": target
            }
        }))
    }

    async fn tool_report(&mut self, args: &Value) -> Result<Value> {
        let repo_name = args.get("repo").and_then(|v| v.as_str());
        let (repo_path, snap_path) = {
            let entry = self.resolve_repo(repo_name)?;
            (
                std::path::PathBuf::from(&entry.path),
                std::path::Path::new(&entry.storage_path).join("graph.bin"),
            )
        };
        let repo_path = repo_path.as_path();

        let graph = self.load_cached_snapshot(&snap_path)?;

        let node_count = graph.iter_nodes().count();
        let edge_count = graph.iter_relationships().count();
        let file_count = graph
            .iter_nodes()
            .filter(|n| n.label == NodeLabel::File)
            .count();
        let density = if node_count > 0 {
            edge_count as f64 / node_count as f64
        } else {
            0.0
        };

        // Git analytics
        let hotspots = gitnexus_git::hotspots::analyze_hotspots(repo_path, 90).unwrap_or_default();
        let couplings =
            gitnexus_git::coupling::analyze_coupling(repo_path, 3, Some(180)).unwrap_or_default();
        let ownerships = gitnexus_git::ownership::analyze_ownership(repo_path).unwrap_or_default();

        // Compute health score (0-100); healthy projects score ~85-95
        let mut score: f64 = 100.0;
        let hot_files = hotspots.iter().filter(|h| h.score > 0.7).count();
        score -= (hot_files as f64) * 3.0;
        let strong_couples = couplings
            .iter()
            .filter(|c| c.coupling_strength > 0.7)
            .count();
        score -= (strong_couples as f64) * 2.0;
        let orphan_files = ownerships.iter().filter(|o| o.ownership_pct < 50.0).count();
        score -= (orphan_files as f64) * 0.5;
        score = score.clamp(0.0, 100.0);

        // Derive the grade from the same rounded value surfaced to the caller
        // so the displayed score and letter grade can't disagree. Previously
        // the grade came from `score as u32` (truncation) while the displayed
        // score rounded to one decimal, so a raw score of 89.95 reported
        // `"90.0" / "B"` — the score crossed the A boundary after rounding
        // but the grade did not. Same bug pattern as health.rs / report.rs.
        let rounded_score = (score * 10.0).round() / 10.0;
        let grade = match rounded_score as u32 {
            90..=100 => "A",
            75..=89 => "B",
            60..=74 => "C",
            40..=59 => "D",
            _ => "E",
        };

        let report = json!({
            "grade": grade,
            "score": rounded_score,
            "graph": {
                "nodes": node_count,
                "edges": edge_count,
                "files": file_count,
                "density": (density * 100.0).round() / 100.0,
            },
            "hotspots": {
                "total": hotspots.len(),
                "critical": hot_files,
                "top3": hotspots.iter().take(3).map(|h| json!({
                    "path": h.path,
                    "score": (h.score * 100.0).round() / 100.0,
                    "commits": h.commit_count,
                })).collect::<Vec<_>>(),
            },
            "coupling": {
                "total": couplings.len(),
                "strong": strong_couples,
            },
            "ownership": {
                "totalFiles": ownerships.len(),
                "orphanedFiles": orphan_files,
            }
        });

        Ok(json!({
            "content": [{
                "type": "text",
                "text": serde_json::to_string_pretty(&report).unwrap_or_default()
            }],
            "_meta": {
                "hint": hints::hint_for("report")
            }
        }))
    }

    async fn tool_analyze_execution_trace(&mut self, args: &Value) -> Result<Value> {
        let trace_file = args
            .get("trace_file")
            .and_then(|v| v.as_str())
            .ok_or_else(|| McpError::InvalidArguments {
                tool: "analyze_execution_trace".into(),
                reason: "Missing required 'trace_file' parameter".into(),
            })?;

        let repo_name = args.get("repo").and_then(|v| v.as_str());

        let (repo_path, snap_path) = {
            let entry = self.resolve_repo(repo_name)?;
            (
                std::path::PathBuf::from(&entry.path),
                std::path::Path::new(&entry.storage_path).join("graph.bin"),
            )
        };

        let graph = self.load_cached_snapshot(&snap_path)?;

        // Resolve `trace_file` against the repo root and require the canonical
        // path to stay inside the repo. Without this, an MCP client could
        // supply an arbitrary absolute path (or `..`-laden relative path) and
        // exfiltrate any file the server can read.
        let candidate = if std::path::Path::new(trace_file).is_absolute() {
            std::path::PathBuf::from(trace_file)
        } else {
            repo_path.join(trace_file)
        };
        let canonical_trace = candidate
            .canonicalize()
            .map_err(|_| McpError::Internal(format!("Trace file not found: {}", trace_file)))?;
        let canonical_repo = repo_path
            .canonicalize()
            .map_err(|e| McpError::Internal(format!("Failed to canonicalize repo path: {}", e)))?;
        if !canonical_trace.starts_with(&canonical_repo) {
            return Err(McpError::Internal(
                "trace_file must be a path inside the repository".to_string(),
            ));
        }

        let trace_content = std::fs::read_to_string(&canonical_trace)
            .map_err(|e| McpError::Internal(format!("Failed to read trace file: {}", e)))?;

        let steps = gitnexus_core::trace::parse_trace(&trace_content)
            .map_err(|e| McpError::Internal(format!("Failed to parse trace: {}", e)))?;

        let name_to_ids = gitnexus_core::trace::build_name_index(&graph);

        let mut enriched_steps = Vec::new();
        let mut matched = 0;

        for step in steps {
            let mut enriched_step = step.clone();
            let method_name_opt = step
                .get("method")
                .or(step.get("name"))
                .and_then(|v| v.as_str());

            if let Some(full_method_name) = method_name_opt {
                if let Some(node_id) = gitnexus_core::trace::resolve_method_node(
                    &graph,
                    &name_to_ids,
                    full_method_name,
                ) {
                    if let Some(node) = graph.get_node(&node_id) {
                        matched += 1;
                        if let Some(obj) = enriched_step.as_object_mut() {
                            obj.insert("nodeId".to_string(), json!(node_id));
                            obj.insert(
                                "filePath".to_string(),
                                json!(node.properties.file_path.clone()),
                            );
                            obj.insert("startLine".to_string(), json!(node.properties.start_line));
                            obj.insert("endLine".to_string(), json!(node.properties.end_line));

                            // Path traversal guard: the graph node's file_path
                            // comes from a snapshot that may contain `..`
                            // segments (corrupted or hand-crafted), and we
                            // mustn't let them escape the repo root and
                            // exfiltrate arbitrary files into MCP responses.
                            let full_path = repo_path.join(&node.properties.file_path);
                            let source_safe = match (
                                full_path.canonicalize().ok(),
                                repo_path.canonicalize().ok(),
                            ) {
                                (Some(canon), Some(root)) => canon.starts_with(&root),
                                _ => false,
                            };
                            if source_safe {
                                if let (Some(start), Some(end)) =
                                    (node.properties.start_line, node.properties.end_line)
                                {
                                    if let Some(source) = gitnexus_core::trace::extract_source_lines(
                                        &full_path, start, end,
                                    ) {
                                        obj.insert("sourceCode".to_string(), json!(source));
                                    }
                                }
                            }
                        }
                    }
                }
            }
            enriched_steps.push(enriched_step);
        }

        Ok(json!({
            "content": [{
                "type": "text",
                "text": serde_json::to_string_pretty(&json!({
                    "totalSteps": enriched_steps.len(),
                    "matchedSteps": matched,
                    "trace": enriched_steps
                })).unwrap_or_default()
            }],
            "_meta": {
                "hint": hints::hint_for("analyze_execution_trace")
            }
        }))
    }

    async fn tool_search_processes(&mut self, args: &Value) -> Result<Value> {
        struct ProcessSearchRow {
            value: Value,
            step_count: u32,
            name: String,
        }

        let query_text = args
            .get("query")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim();
        let query_lower = query_text.to_lowercase();
        let repo_name = args.get("repo").and_then(|v| v.as_str());
        let limit = clamp_limit(
            args.get("limit").and_then(|v| v.as_u64()),
            10,
            MAX_QUERY_LIMIT,
        );

        let snap_path = {
            let entry = self.resolve_repo(repo_name)?;
            std::path::Path::new(&entry.storage_path).join("graph.bin")
        };
        let graph = self.load_cached_snapshot(&snap_path)?;

        let mut process_step_counts: HashMap<String, u32> = HashMap::new();
        for rel in graph.iter_relationships() {
            if rel.rel_type == RelationshipType::StepInProcess {
                *process_step_counts
                    .entry(rel.target_id.clone())
                    .or_insert(0) += 1;
            }
        }

        let mut rows: Vec<ProcessSearchRow> = graph
            .iter_nodes()
            .filter(|node| matches!(node.label, NodeLabel::Process))
            .filter(|node| {
                if query_lower.is_empty() {
                    return true;
                }
                node.properties.name.to_lowercase().contains(&query_lower)
                    || node
                        .properties
                        .description
                        .as_deref()
                        .unwrap_or("")
                        .to_lowercase()
                        .contains(&query_lower)
            })
            .map(|node| {
                let step_count = process_step_counts.get(&node.id).copied().unwrap_or(0);
                let process_type = node
                    .properties
                    .process_type
                    .as_ref()
                    .map(|t| format!("{t:?}"));
                let process_id = node.id.clone();
                let name = node.properties.name.clone();
                let description = node.properties.description.clone();
                let entry_point_id = node.properties.entry_point_id.clone();
                let terminal_id = node.properties.terminal_id.clone();
                let communities = node.properties.communities.clone();
                ProcessSearchRow {
                    step_count,
                    name: name.clone(),
                    value: json!({
                        "processId": process_id,
                        "name": name,
                        "description": description,
                        "processType": process_type,
                        "stepCount": step_count,
                        "entryPointId": entry_point_id,
                        "terminalId": terminal_id,
                        "communities": communities,
                    }),
                }
            })
            .collect();

        rows.sort_by(|a, b| {
            b.step_count
                .cmp(&a.step_count)
                .then_with(|| a.name.cmp(&b.name))
        });
        rows.truncate(limit);
        let result_count = rows.len();
        let processes: Vec<Value> = rows.into_iter().map(|row| row.value).collect();

        Ok(json!({
            "content": [{
                "type": "text",
                "text": serde_json::to_string_pretty(&processes).unwrap_or_else(|_| "[]".to_string())
            }],
            "_meta": {
                "hint": hints::hint_for("search_processes"),
                "resultCount": result_count
            }
        }))
    }

    async fn tool_business(&mut self, args: &Value) -> Result<Value> {
        let repo_name = args.get("repo").and_then(|v| v.as_str());
        let process_filter = args.get("process").and_then(|v| v.as_str());

        let snap_path = {
            let entry = self.resolve_repo(repo_name)?;
            std::path::Path::new(&entry.storage_path).join("graph.bin")
        };

        let graph = self.load_cached_snapshot(&snap_path)?;

        let mut processes = Vec::new();

        // Check for specific Alise business patterns in the graph
        let has_courriers = graph
            .iter_nodes()
            .any(|n| n.properties.name.contains("Courrier"));
        let has_paiements = graph.iter_nodes().any(|n| {
            n.properties.name.contains("Reglement") || n.properties.name.contains("Facture")
        });
        let has_baremes = graph
            .iter_nodes()
            .any(|n| n.properties.name.contains("Bareme"));
        let has_fournisseurs = graph
            .iter_nodes()
            .any(|n| n.properties.name.contains("Fournisseur"));

        if has_courriers {
            processes.push(json!({
                "id": "courriers",
                "name": "Système de Courriers",
                "description": "Génération de courriers officiels (Accord, Refus, etc.) via Aspose.Words."
            }));
        }
        if has_paiements {
            processes.push(json!({
                "id": "paiements",
                "name": "Cycle de Paiement",
                "description": "Flux financier complet : de la facture à l'export ELODIE."
            }));
        }
        if has_baremes {
            processes.push(json!({
                "id": "baremes",
                "name": "Calcul des Barèmes",
                "description": "Moteur de calcul des droits et plafonds d'aide sociale."
            }));
        }
        if has_fournisseurs {
            processes.push(json!({
                "id": "fournisseurs",
                "name": "Gestion des Fournisseurs",
                "description": "Référentiel des prestataires de services et coordonnées bancaires."
            }));
        }

        let result = if let Some(filter) = process_filter {
            let filter_lower = filter.to_lowercase();
            match filter_lower.as_str() {
                "courriers" if has_courriers => json!({
                    "id": "courriers",
                    "title": "Système de Courriers",
                    "details": "L'application gère 11 types de courriers. Processus : Sélection destinataires -> Mail Merge (ELODIE variables) -> Génération PDF -> Archivage.",
                    "key_entities": ["CourrierController", "RegleCourrierMasse", "CourrierGenerer"]
                }),
                "paiements" | "paiement" if has_paiements => json!({
                    "id": "paiements",
                    "title": "Cycle de Paiement",
                    "details": "États : DemPaiemVal (Création) -> DemGrPrVal (Groupé) -> DemTransmiseELODIE (Export). Export vers ELODIE via Flux3 Excel.",
                    "key_entities": ["FacturesController", "FactureService", "ElodieService"]
                }),
                "baremes" | "bareme" if has_baremes => json!({
                    "id": "baremes",
                    "title": "Calcul des Barèmes",
                    "details": "Calcul basé sur le quotient familial (Ressources / Parts). Détermine le TauxFASS et les plafonds de prise en charge.",
                    "key_entities": ["Bareme", "Tranche", "CalculService"]
                }),
                "fournisseurs" if has_fournisseurs => json!({
                    "id": "fournisseurs",
                    "title": "Gestion des Fournisseurs",
                    "details": "Gestion des tiers payés par la CMCAS. Inclut la recherche, l'historique de paiement et les RIB.",
                    "key_entities": ["Fournisseur", "IBAN", "FournisseurController"]
                }),
                _ => json!({
                    "error": format!("Processus '{}' non trouvé ou non applicable à ce projet.", filter),
                    "available_processes": processes
                }),
            }
        } else {
            json!({
                "available_processes": processes,
                "total_found": processes.len()
            })
        };

        Ok(json!({
            "content": [{
                "type": "text",
                "text": serde_json::to_string_pretty(&result).unwrap_or_default()
            }],
            "_meta": {
                "hint": hints::hint_for("business")
            }
        }))
    }

    // ─── New Tools ─────────────────────────────────────────────────

    async fn tool_search_code(&mut self, args: &Value) -> Result<Value> {
        let start = Instant::now();
        let query_str = args["query"]
            .as_str()
            .ok_or_else(|| McpError::InvalidArguments {
                tool: "search_code".into(),
                reason: "Missing required 'query' parameter".into(),
            })?;
        let repo_name = args["repo"].as_str();
        let limit = clamp_limit(args["limit"].as_u64(), 8, 20);
        // Post-retrieval LLM reranking (opt-in). Requires ~/.gitnexus/chat-config.json.
        // On config miss or reranker error we silently fall back to BM25 order.
        let rerank = args
            .get("rerank")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        // Hybrid BM25 + semantic RRF fusion (opt-in). Requires `gitnexus embed`
        // to have populated .gitnexus/embeddings.bin + embeddings.meta.json.
        let hybrid_mode = args
            .get("hybrid")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let entry = self.resolve_repo(repo_name)?;
        let repo_path = std::path::PathBuf::from(&entry.path);
        let storage_path = std::path::PathBuf::from(&entry.storage_path);
        let snap_path = gitnexus_db::snapshot::snapshot_path(&storage_path);
        let (graph, indexes, fts) = self.load_cached_indexes(&snap_path)?;

        let pool_size = if rerank || hybrid_mode {
            limit.max(20)
        } else {
            limit * 2
        };
        let fts_results = fts.search(&graph, query_str, None, pool_size);
        // Order matters: hybrid fusion first (so the LLM reranker sees a
        // pool enriched with semantic matches), then LLM rerank.
        let fts_results =
            Self::maybe_hybrid_fuse(hybrid_mode, query_str, &graph, &storage_path, fts_results)
                .await;
        let fts_results = Self::maybe_rerank_fts(rerank, query_str, fts_results).await;

        // Also try name-contains fallback
        let mut seen = std::collections::HashSet::new();
        let mut results: Vec<Value> = Vec::new();
        let query_lower = query_str.to_lowercase();

        // FTS results first
        for hit in &fts_results {
            if results.len() >= limit {
                break;
            }
            if !seen.insert(hit.node_id.clone()) {
                continue;
            }
            if let Some(node) = graph.get_node(&hit.node_id) {
                let snippet = Self::read_code_snippet(
                    &repo_path,
                    &node.properties.file_path,
                    node.properties.start_line,
                    node.properties
                        .end_line
                        .or(node.properties.start_line.map(|s| s + 50)),
                );

                let callers = Self::collect_neighbors(&graph, &indexes.incoming, &hit.node_id, 5);
                let callees = Self::collect_neighbors(&graph, &indexes.outgoing, &hit.node_id, 5);

                results.push(json!({
                    "nodeId": hit.node_id,
                    "name": node.properties.name,
                    "label": format!("{:?}", node.label),
                    "filePath": node.properties.file_path,
                    "startLine": node.properties.start_line,
                    "endLine": node.properties.end_line,
                    "score": hit.score,
                    "snippet": snippet,
                    "callers": callers,
                    "callees": callees,
                    "enrichment": Self::collect_enrichment_for_node(&graph, &hit.node_id),
                }));
            }
        }

        // Name-contains fallback if FTS gave few results
        if results.len() < limit {
            for node in graph.iter_nodes() {
                if results.len() >= limit {
                    break;
                }
                if seen.contains(&node.id) {
                    continue;
                }
                if node.properties.name.to_lowercase().contains(&query_lower) {
                    seen.insert(node.id.clone());
                    let snippet = Self::read_code_snippet(
                        &repo_path,
                        &node.properties.file_path,
                        node.properties.start_line,
                        node.properties
                            .end_line
                            .or(node.properties.start_line.map(|s| s + 50)),
                    );
                    results.push(json!({
                        "nodeId": node.id,
                        "name": node.properties.name,
                        "label": format!("{:?}", node.label),
                        "filePath": node.properties.file_path,
                        "startLine": node.properties.start_line,
                        "endLine": node.properties.end_line,
                        "score": 0.0,
                        "snippet": snippet,
                        "enrichment": Self::collect_enrichment_for_node(&graph, &node.id),
                    }));
                }
            }
        }

        let duration = start.elapsed();
        Ok(json!({
            "content": [{
                "type": "text",
                "text": serde_json::to_string_pretty(&results).unwrap_or_default()
            }],
            "_meta": {
                "hint": hints::hint_for("search_code"),
                "resultCount": results.len(),
                "durationMs": duration.as_millis() as u64,
                "reranked": rerank,
                "hybrid": hybrid_mode,
            }
        }))
    }

    /// Fuse BM25 results with semantic (embedding) top-K via Reciprocal Rank
    /// Fusion. Silently falls back to the original BM25 order on any failure
    /// (missing embeddings file, missing meta, inference error). Never drops
    /// results — the `fused` output is a reordered superset of the input.
    async fn maybe_hybrid_fuse(
        hybrid_mode: bool,
        query_str: &str,
        graph: &std::sync::Arc<gitnexus_core::graph::KnowledgeGraph>,
        storage_path: &std::path::Path,
        fts_results: Vec<FtsResult>,
    ) -> Vec<FtsResult> {
        if !hybrid_mode || fts_results.len() < 2 {
            return fts_results;
        }
        let emb_path = storage_path.join("embeddings.bin");
        let meta_path = storage_path.join("embeddings.meta.json");
        if !emb_path.exists() || !meta_path.exists() {
            tracing::warn!(
                "hybrid=true but embeddings files missing at {} — run 'gitnexus embed' first; using BM25 order",
                storage_path.display()
            );
            return fts_results;
        }

        let query_str = query_str.to_string();
        let graph = graph.clone();
        let emb_path = emb_path.clone();
        let meta_path = meta_path.clone();
        let fts_clone = fts_results.clone();

        // ONNX inference is blocking; run on the blocking pool.
        let outcome = tokio::task::spawn_blocking(move || -> anyhow::Result<Vec<FtsResult>> {
            let cfg: EmbeddingConfig = serde_json::from_str(&std::fs::read_to_string(&meta_path)?)?;
            let store = load_embeddings(&emb_path)?;
            if store.header.dimension != cfg.dimension {
                anyhow::bail!(
                    "embeddings.bin dim {} differs from meta dim {}",
                    store.header.dimension,
                    cfg.dimension
                );
            }
            let q_vecs = generate_embeddings(std::slice::from_ref(&query_str), &cfg);
            let q_vec = q_vecs
                .into_iter()
                .next()
                .ok_or_else(|| anyhow::anyhow!("generate_embeddings returned no output"))?;
            if q_vec.iter().all(|&v| v == 0.0) {
                anyhow::bail!("query embedding is all zeros (model/tokenizer missing?)");
            }
            let stored: Vec<(String, Vec<f32>)> = store.entries;
            let top_k = fts_clone.len();
            let mut semantic_results = search_semantic(&q_vec, &stored, top_k);
            for s in &mut semantic_results {
                if let Some(n) = graph.get_node(&s.node_id) {
                    s.file_path = n.properties.file_path.clone();
                    s.name = n.properties.name.clone();
                    s.label = format!("{:?}", n.label);
                    s.start_line = n.properties.start_line;
                    s.end_line = n.properties.end_line;
                }
            }
            let bm25_wrapped: Vec<BM25SearchResult> = fts_clone
                .iter()
                .enumerate()
                .map(|(i, r)| BM25SearchResult {
                    file_path: r.file_path.clone(),
                    score: r.score,
                    rank: i + 1,
                    node_id: r.node_id.clone(),
                    name: r.name.clone(),
                    label: r.label.clone(),
                    start_line: r.start_line,
                    end_line: r.end_line,
                })
                .collect();
            let fused = hybrid::merge_with_rrf(&bm25_wrapped, &semantic_results, top_k);
            Ok(fused
                .into_iter()
                .map(|h| FtsResult {
                    node_id: h.node_id,
                    score: h.score,
                    name: h.name,
                    file_path: h.file_path,
                    label: h.label,
                    start_line: h.start_line,
                    end_line: h.end_line,
                })
                .collect())
        })
        .await;

        match outcome {
            Ok(Ok(v)) => v,
            Ok(Err(e)) => {
                tracing::warn!(error = %e, "hybrid fuse failed; using BM25 order");
                fts_results
            }
            Err(e) => {
                tracing::warn!(error = %e, "hybrid fuse task panicked; using BM25 order");
                fts_results
            }
        }
    }

    /// Reorder FTS results via an LLM reranker when opt-in. Silently falls
    /// back to the original BM25 order on any failure (missing config, HTTP
    /// error, parse error, join panic). Never drops results.
    async fn maybe_rerank_fts(
        rerank: bool,
        query_str: &str,
        fts_results: Vec<FtsResult>,
    ) -> Vec<FtsResult> {
        if !rerank || fts_results.len() < 2 {
            return fts_results;
        }
        let config = match crate::llm_config::load_llm_config() {
            Some(c) => c,
            None => {
                tracing::warn!(
                    "rerank=true but ~/.gitnexus/chat-config.json missing; using BM25 order"
                );
                return fts_results;
            }
        };
        let candidates: Vec<Candidate> = fts_results
            .iter()
            .enumerate()
            .map(|(i, r)| Candidate {
                node_id: r.node_id.clone(),
                name: r.name.clone(),
                label: r.label.clone(),
                file_path: r.file_path.clone(),
                start_line: r.start_line,
                end_line: r.end_line,
                score: r.score,
                rank: i + 1,
                snippet: None,
            })
            .collect();

        let api_key = (!config.api_key.is_empty()).then_some(config.api_key);
        let reranker = LlmReranker::new(config.base_url, config.model, api_key);
        let q = query_str.to_string();
        let reranked =
            match tokio::task::spawn_blocking(move || reranker.rerank(&q, candidates)).await {
                Ok(Ok(r)) => r,
                Ok(Err(e)) => {
                    tracing::warn!(error = %e, "reranker failed; using BM25 order");
                    return fts_results;
                }
                Err(e) => {
                    tracing::warn!(error = %e, "reranker task join failed; using BM25 order");
                    return fts_results;
                }
            };

        // Rebuild the FtsResult list in reranker order; any result the LLM
        // omitted gets appended to the tail so nothing is silently lost.
        let mut by_id: HashMap<String, FtsResult> = fts_results
            .into_iter()
            .map(|r| (r.node_id.clone(), r))
            .collect();
        let mut out = Vec::with_capacity(by_id.len());
        for c in reranked {
            if let Some(r) = by_id.remove(&c.node_id) {
                out.push(r);
            }
        }
        out.extend(by_id.into_values());
        out
    }

    async fn tool_read_file(&mut self, args: &Value) -> Result<Value> {
        let start = Instant::now();
        let file_path = args["path"]
            .as_str()
            .ok_or_else(|| McpError::InvalidArguments {
                tool: "read_file".into(),
                reason: "Missing required 'path' parameter".into(),
            })?;
        let repo_name = args["repo"].as_str();
        let start_line = args["start_line"].as_u64().map(|v| v as u32);
        let end_line = args["end_line"].as_u64().map(|v| v as u32);

        let entry = self.resolve_repo(repo_name)?;
        let repo_path = std::path::PathBuf::from(&entry.path);

        // Read file content
        let content = Self::read_code_snippet(&repo_path, file_path, start_line, end_line)
            .ok_or_else(|| McpError::Internal(format!("Cannot read file: {file_path}")))?;

        // Load graph for symbol annotations
        let snap_path =
            gitnexus_db::snapshot::snapshot_path(&std::path::PathBuf::from(&entry.storage_path));
        let graph = self.load_cached_snapshot(&snap_path)?;

        // Find symbols defined in this file
        let mut symbols: Vec<Value> = Vec::new();
        // Normalize the path for comparison
        let norm_path = file_path.replace('\\', "/");
        if let Some(node_ids) = graph.nodes_by_file(&norm_path) {
            for nid in node_ids {
                if let Some(node) = graph.get_node(nid) {
                    symbols.push(json!({
                        "nodeId": node.id,
                        "name": node.properties.name,
                        "label": format!("{:?}", node.label),
                        "startLine": node.properties.start_line,
                        "endLine": node.properties.end_line,
                        "enrichment": Self::collect_enrichment_for_node(&graph, nid),
                    }));
                }
            }
        }

        // Also try with backslash path on Windows
        if symbols.is_empty() {
            let win_path = file_path.replace('/', "\\");
            if let Some(node_ids) = graph.nodes_by_file(&win_path) {
                for nid in node_ids {
                    if let Some(node) = graph.get_node(nid) {
                        symbols.push(json!({
                            "nodeId": node.id,
                            "name": node.properties.name,
                            "label": format!("{:?}", node.label),
                            "startLine": node.properties.start_line,
                            "endLine": node.properties.end_line,
                            "enrichment": Self::collect_enrichment_for_node(&graph, nid),
                        }));
                    }
                }
            }
        }

        let duration = start.elapsed();
        Ok(json!({
            "content": [{
                "type": "text",
                "text": content
            }],
            "_meta": {
                "hint": hints::hint_for("read_file"),
                "filePath": file_path,
                "startLine": start_line.unwrap_or(1),
                "symbols": symbols,
                "durationMs": duration.as_millis() as u64,
            }
        }))
    }

    async fn tool_get_insights(&mut self, args: &Value) -> Result<Value> {
        let start = Instant::now();
        let symbol = args["symbol"]
            .as_str()
            .ok_or_else(|| McpError::InvalidArguments {
                tool: "get_insights".into(),
                reason: "Missing required 'symbol' parameter".into(),
            })?;
        let repo_name = args["repo"].as_str();

        let entry = self.resolve_repo(repo_name)?;
        let snap_path =
            gitnexus_db::snapshot::snapshot_path(&std::path::PathBuf::from(&entry.storage_path));
        let (graph, indexes, _fts) = self.load_cached_indexes(&snap_path)?;

        // Try exact node ID first, then name match
        let node_id = if graph.get_node(symbol).is_some() {
            symbol.to_string()
        } else {
            // Search by name
            let sym_lower = symbol.to_lowercase();
            graph
                .iter_nodes()
                .find(|n| n.properties.name.to_lowercase() == sym_lower)
                .or_else(|| {
                    graph
                        .iter_nodes()
                        .find(|n| n.properties.name.to_lowercase().contains(&sym_lower))
                })
                .map(|n| n.id.clone())
                .ok_or_else(|| McpError::Internal(format!("Symbol not found: {symbol}")))?
        };

        let node = graph
            .get_node(&node_id)
            .ok_or_else(|| McpError::Internal(format!("Node not found: {node_id}")))?;

        let p = &node.properties;
        let enrichment = Self::collect_enrichment_for_node(&graph, &node_id);

        // Get callers/callees
        let callers = Self::collect_neighbors(&graph, &indexes.incoming, &node_id, 10);
        let callees = Self::collect_neighbors(&graph, &indexes.outgoing, &node_id, 10);

        // Find community membership
        let community_info = indexes
            .outgoing
            .get(&node_id)
            .and_then(|edges| {
                edges
                    .iter()
                    .find(|(_, rt)| matches!(rt, RelationshipType::MemberOf))
            })
            .and_then(|(comm_id, _)| graph.get_node(comm_id))
            .map(|comm| {
                json!({
                    "communityId": comm.id,
                    "label": comm.properties.heuristic_label,
                    "description": comm.properties.description,
                    "keywords": comm.properties.keywords,
                })
            });

        let duration = start.elapsed();
        Ok(json!({
            "content": [{
                "type": "text",
                "text": serde_json::to_string_pretty(&json!({
                    "nodeId": node_id,
                    "name": p.name,
                    "label": format!("{:?}", node.label),
                    "filePath": p.file_path,
                    "startLine": p.start_line,
                    "endLine": p.end_line,
                    "enrichment": enrichment,
                    "callers": callers,
                    "callees": callees,
                    "community": community_info,
                })).unwrap_or_default()
            }],
            "_meta": {
                "hint": hints::hint_for("get_insights"),
                "durationMs": duration.as_millis() as u64,
            }
        }))
    }

    async fn tool_save_memory(&mut self, args: &Value) -> Result<Value> {
        let fact = args["fact"]
            .as_str()
            .ok_or_else(|| McpError::InvalidArguments {
                tool: "save_memory".into(),
                reason: "Missing required 'fact' parameter".into(),
            })?;
        const MAX_FACT_BYTES: usize = 64 * 1024;
        if fact.len() > MAX_FACT_BYTES {
            return Err(McpError::InvalidArguments {
                tool: "save_memory".into(),
                reason: format!("fact exceeds {} byte limit", MAX_FACT_BYTES),
            });
        }
        let scope = args["scope"].as_str().unwrap_or("project");
        let repo_name = args["repo"].as_str();

        // Determine storage path
        let memory_dir = if scope == "global" {
            // Global memory: ~/.gitnexus/memory/
            let home = std::env::var("USERPROFILE")
                .or_else(|_| std::env::var("HOME"))
                .map_err(|_| McpError::Internal("Cannot determine home directory".into()))?;
            std::path::PathBuf::from(home)
                .join(".gitnexus")
                .join("memory")
        } else {
            // Project memory: .gitnexus/memory/
            let entry = self.resolve_repo(repo_name)?;
            std::path::PathBuf::from(&entry.storage_path).join("memory")
        };

        std::fs::create_dir_all(&memory_dir)
            .map_err(|e| McpError::Internal(format!("Cannot create memory dir: {e}")))?;

        // Generate filename from timestamp
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let memory_file = memory_dir.join(format!("fact_{ts}.md"));

        std::fs::write(&memory_file, fact)
            .map_err(|e| McpError::Internal(format!("Cannot write memory: {e}")))?;

        Ok(json!({
            "content": [{
                "type": "text",
                "text": format!("Fact saved to {} scope: {}", scope, memory_file.display())
            }],
            "_meta": {
                "hint": hints::hint_for("save_memory")
            }
        }))
    }

    // ─── Code Quality Suite (Theme A) ────────────────────────────────

    async fn tool_find_cycles(&mut self, args: &Value) -> Result<Value> {
        use gitnexus_db::analytics::cycles::{find_cycles, CycleScope};
        let repo_name = args.get("repo").and_then(|v| v.as_str());
        let scope_str = args
            .get("scope")
            .and_then(|v| v.as_str())
            .unwrap_or("imports");
        let scope = CycleScope::parse(scope_str).ok_or_else(|| McpError::InvalidArguments {
            tool: "find_cycles".into(),
            reason: format!("Invalid scope '{scope_str}' (expected 'imports' or 'calls')"),
        })?;
        let limit = clamp_limit(
            args.get("limit").and_then(|v| v.as_u64()),
            50,
            MAX_ANALYTICS_LIMIT,
        );

        let snap_path = {
            let entry = self.resolve_repo(repo_name)?;
            std::path::Path::new(&entry.storage_path).join("graph.bin")
        };
        let graph = self.load_cached_snapshot(&snap_path)?;

        let mut cycles = find_cycles(&graph, scope);
        cycles.truncate(limit);
        let results: Vec<Value> = cycles
            .iter()
            .map(|c| serde_json::to_value(c).unwrap_or_default())
            .collect();

        Ok(json!({
            "content": [{
                "type": "text",
                "text": serde_json::to_string_pretty(&results).unwrap_or_else(|_| "[]".to_string())
            }],
            "_meta": {
                "hint": hints::hint_for("find_cycles"),
                "scope": scope_str,
                "resultCount": results.len(),
            }
        }))
    }

    async fn tool_find_similar_code(&mut self, args: &Value) -> Result<Value> {
        use gitnexus_db::analytics::clones::{find_clones, CloneOptions};
        let repo_name = args.get("repo").and_then(|v| v.as_str());
        let min_tokens = args
            .get("min_tokens")
            .and_then(|v| v.as_u64())
            .unwrap_or(30) as usize;
        let threshold = args
            .get("threshold")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.9);
        let limit = clamp_limit(
            args.get("limit").and_then(|v| v.as_u64()),
            50,
            MAX_ANALYTICS_LIMIT,
        );

        let (repo_path, snap_path) = {
            let entry = self.resolve_repo(repo_name)?;
            (
                std::path::PathBuf::from(&entry.path),
                std::path::Path::new(&entry.storage_path).join("graph.bin"),
            )
        };
        let graph = self.load_cached_snapshot(&snap_path)?;

        let opts = CloneOptions {
            min_tokens: min_tokens.max(5),
            threshold: threshold.clamp(0.0, 1.0),
            max_clusters: limit,
        };
        let clusters = find_clones(&graph, &repo_path, opts);
        let results: Vec<Value> = clusters
            .iter()
            .map(|c| serde_json::to_value(c).unwrap_or_default())
            .collect();

        Ok(json!({
            "content": [{
                "type": "text",
                "text": serde_json::to_string_pretty(&results).unwrap_or_else(|_| "[]".to_string())
            }],
            "_meta": {
                "hint": hints::hint_for("find_similar_code"),
                "minTokens": min_tokens,
                "threshold": threshold,
                "resultCount": results.len(),
            }
        }))
    }

    async fn tool_list_todos(&mut self, args: &Value) -> Result<Value> {
        let repo_name = args.get("repo").and_then(|v| v.as_str());
        let severity_filter = args.get("severity").and_then(|v| v.as_str());
        let limit = clamp_limit(args.get("limit").and_then(|v| v.as_u64()), 200, 500);

        let snap_path = {
            let entry = self.resolve_repo(repo_name)?;
            std::path::Path::new(&entry.storage_path).join("graph.bin")
        };
        let graph = self.load_cached_snapshot(&snap_path)?;

        let mut todos: Vec<Value> = Vec::new();
        for node in graph.iter_nodes() {
            if node.label != NodeLabel::TodoMarker {
                continue;
            }
            let kind = node.properties.todo_kind.clone().unwrap_or_default();
            if let Some(want) = severity_filter {
                if !kind.eq_ignore_ascii_case(want) {
                    continue;
                }
            }
            todos.push(json!({
                "nodeId": node.id,
                "kind": kind,
                "text": node.properties.todo_text,
                "filePath": node.properties.file_path,
                "line": node.properties.start_line,
                "language": node.properties.language,
            }));
        }
        // Sort: FIXME > HACK > TODO > XXX, then by file path.
        todos.sort_by(|a, b| {
            let ka = todo_rank(a.get("kind").and_then(|v| v.as_str()).unwrap_or(""));
            let kb = todo_rank(b.get("kind").and_then(|v| v.as_str()).unwrap_or(""));
            ka.cmp(&kb).then_with(|| {
                a.get("filePath")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .cmp(b.get("filePath").and_then(|v| v.as_str()).unwrap_or(""))
            })
        });
        let total = todos.len();
        todos.truncate(limit);

        Ok(json!({
            "content": [{
                "type": "text",
                "text": serde_json::to_string_pretty(&todos).unwrap_or_else(|_| "[]".to_string())
            }],
            "_meta": {
                "hint": hints::hint_for("list_todos"),
                "resultCount": todos.len(),
                "totalCount": total,
            }
        }))
    }

    async fn tool_get_complexity(&mut self, args: &Value) -> Result<Value> {
        use gitnexus_db::analytics::complexity::{get_complexity, ComplexityOptions};
        let repo_name = args.get("repo").and_then(|v| v.as_str());
        let threshold = args.get("threshold").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
        let top_n = clamp_limit(args.get("limit").and_then(|v| v.as_u64()), 50, 200);

        let snap_path = {
            let entry = self.resolve_repo(repo_name)?;
            std::path::Path::new(&entry.storage_path).join("graph.bin")
        };
        let graph = self.load_cached_snapshot(&snap_path)?;

        let report = get_complexity(&graph, ComplexityOptions { threshold, top_n });
        let body = serde_json::to_value(&report).unwrap_or_default();

        Ok(json!({
            "content": [{
                "type": "text",
                "text": serde_json::to_string_pretty(&body).unwrap_or_else(|_| "{}".to_string())
            }],
            "_meta": {
                "hint": hints::hint_for("get_complexity"),
                "threshold": threshold,
                "topN": top_n,
            }
        }))
    }

    // ─── Schema & API Inventory (Theme D) ─────────────────────────────

    async fn tool_list_endpoints(&mut self, args: &Value) -> Result<Value> {
        let repo_name = args.get("repo").and_then(|v| v.as_str());
        let want_method = args
            .get("method")
            .and_then(|v| v.as_str())
            .map(|s| s.to_ascii_uppercase());
        let pattern = args
            .get("pattern")
            .and_then(|v| v.as_str())
            .map(|s| s.to_ascii_lowercase());
        let limit = clamp_limit(args.get("limit").and_then(|v| v.as_u64()), 200, 500);

        let snap_path = {
            let entry = self.resolve_repo(repo_name)?;
            std::path::Path::new(&entry.storage_path).join("graph.bin")
        };
        let graph = self.load_cached_snapshot(&snap_path)?;

        let mut results: Vec<Value> = Vec::new();
        for node in graph.iter_nodes() {
            if node.label != NodeLabel::ApiEndpoint {
                continue;
            }
            let http = node.properties.http_method.clone().unwrap_or_default();
            let route = node
                .properties
                .route
                .clone()
                .or_else(|| node.properties.route_template.clone())
                .unwrap_or_default();
            if let Some(w) = &want_method {
                if !http.eq_ignore_ascii_case(w) {
                    continue;
                }
            }
            if let Some(pat) = &pattern {
                if !route.to_ascii_lowercase().contains(pat) {
                    continue;
                }
            }
            let handler_name = node
                .properties
                .handler_id
                .as_deref()
                .and_then(|hid| graph.get_node(hid).map(|n| n.properties.name.clone()));
            results.push(json!({
                "nodeId": node.id,
                "httpMethod": http,
                "route": route,
                "framework": node.properties.framework,
                "filePath": node.properties.file_path,
                "startLine": node.properties.start_line,
                "handlerId": node.properties.handler_id,
                "handlerName": handler_name,
            }));
        }
        let total = results.len();
        results.truncate(limit);

        Ok(json!({
            "content": [{
                "type": "text",
                "text": serde_json::to_string_pretty(&results).unwrap_or_else(|_| "[]".to_string())
            }],
            "_meta": {
                "hint": hints::hint_for("list_endpoints"),
                "resultCount": results.len(),
                "totalCount": total,
            }
        }))
    }

    async fn tool_list_db_tables(&mut self, args: &Value) -> Result<Value> {
        let repo_name = args.get("repo").and_then(|v| v.as_str());
        let limit = clamp_limit(args.get("limit").and_then(|v| v.as_u64()), 200, 500);

        let snap_path = {
            let entry = self.resolve_repo(repo_name)?;
            std::path::Path::new(&entry.storage_path).join("graph.bin")
        };
        let graph = self.load_cached_snapshot(&snap_path)?;

        // Aggregate column + FK counts.
        let mut col_count: HashMap<String, u32> = HashMap::new();
        let mut fk_count: HashMap<String, u32> = HashMap::new();
        for rel in graph.iter_relationships() {
            match rel.rel_type {
                RelationshipType::HasColumn => {
                    *col_count.entry(rel.source_id.clone()).or_insert(0) += 1;
                }
                RelationshipType::ReferencesTable => {
                    *fk_count.entry(rel.source_id.clone()).or_insert(0) += 1;
                }
                _ => {}
            }
        }

        let mut results: Vec<Value> = Vec::new();
        for node in graph.iter_nodes() {
            if node.label != NodeLabel::DbEntity {
                continue;
            }
            results.push(json!({
                "nodeId": node.id,
                "name": node.properties.name,
                "filePath": node.properties.file_path,
                "columnCount": col_count.get(&node.id).copied().unwrap_or(0),
                "fkCount": fk_count.get(&node.id).copied().unwrap_or(0),
            }));
        }
        let total = results.len();
        results.truncate(limit);

        Ok(json!({
            "content": [{
                "type": "text",
                "text": serde_json::to_string_pretty(&results).unwrap_or_else(|_| "[]".to_string())
            }],
            "_meta": {
                "hint": hints::hint_for("list_db_tables"),
                "resultCount": results.len(),
                "totalCount": total,
            }
        }))
    }

    async fn tool_list_env_vars(&mut self, args: &Value) -> Result<Value> {
        let repo_name = args.get("repo").and_then(|v| v.as_str());
        let unused_only = args
            .get("unused_only")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let limit = clamp_limit(args.get("limit").and_then(|v| v.as_u64()), 200, 1000);

        let snap_path = {
            let entry = self.resolve_repo(repo_name)?;
            std::path::Path::new(&entry.storage_path).join("graph.bin")
        };
        let graph = self.load_cached_snapshot(&snap_path)?;

        let mut results: Vec<Value> = Vec::new();
        for node in graph.iter_nodes() {
            if node.label != NodeLabel::EnvVar {
                continue;
            }
            let unused = node.properties.unused.unwrap_or(false);
            if unused_only && !unused {
                continue;
            }
            results.push(json!({
                "nodeId": node.id,
                "name": node.properties.name,
                "declaredIn": node.properties.declared_in,
                "usedInCount": node.properties.used_in_count.unwrap_or(0),
                "unused": unused,
                "undeclared": node.properties.undeclared.unwrap_or(false),
            }));
        }
        let total = results.len();
        results.truncate(limit);

        Ok(json!({
            "content": [{
                "type": "text",
                "text": serde_json::to_string_pretty(&results).unwrap_or_else(|_| "[]".to_string())
            }],
            "_meta": {
                "hint": hints::hint_for("list_env_vars"),
                "resultCount": results.len(),
                "totalCount": total,
            }
        }))
    }

    async fn tool_get_endpoint_handler(&mut self, args: &Value) -> Result<Value> {
        let repo_name = args.get("repo").and_then(|v| v.as_str());
        let route = args
            .get("route")
            .and_then(|v| v.as_str())
            .ok_or_else(|| McpError::InvalidArguments {
                tool: "get_endpoint_handler".into(),
                reason: "Missing required 'route' parameter".into(),
            })?
            .to_string();
        let method = args
            .get("method")
            .and_then(|v| v.as_str())
            .ok_or_else(|| McpError::InvalidArguments {
                tool: "get_endpoint_handler".into(),
                reason: "Missing required 'method' parameter".into(),
            })?
            .to_ascii_uppercase();

        let snap_path = {
            let entry = self.resolve_repo(repo_name)?;
            std::path::Path::new(&entry.storage_path).join("graph.bin")
        };
        let graph = self.load_cached_snapshot(&snap_path)?;

        // Find the endpoint.
        let endpoint = graph.iter_nodes().find(|n| {
            n.label == NodeLabel::ApiEndpoint
                && n.properties
                    .http_method
                    .as_deref()
                    .map(|m| m.eq_ignore_ascii_case(&method))
                    .unwrap_or(false)
                && (n.properties.route.as_deref() == Some(route.as_str())
                    || n.properties.route_template.as_deref() == Some(route.as_str()))
        });

        let Some(endpoint) = endpoint else {
            return Ok(json!({
                "content": [{
                    "type": "text",
                    "text": format!("No endpoint found for {} {}", method, route)
                }],
                "_meta": {
                    "hint": hints::hint_for("get_endpoint_handler"),
                    "found": false,
                }
            }));
        };

        // Resolve handler via HandledBy edge.
        let handler_id = graph
            .iter_relationships()
            .find(|r| {
                matches!(r.rel_type, RelationshipType::HandledBy) && r.source_id == endpoint.id
            })
            .map(|r| r.target_id.clone())
            .or_else(|| endpoint.properties.handler_id.clone());

        let mut callers: Vec<Value> = Vec::new();
        let mut callees: Vec<Value> = Vec::new();
        let mut handler_json = Value::Null;

        if let Some(hid) = &handler_id {
            if let Some(handler) = graph.get_node(hid) {
                handler_json = json!({
                    "nodeId": handler.id,
                    "name": handler.properties.name,
                    "label": handler.label.as_str(),
                    "filePath": handler.properties.file_path,
                    "startLine": handler.properties.start_line,
                    "endLine": handler.properties.end_line,
                });
                for rel in graph.iter_relationships() {
                    if !matches!(rel.rel_type, RelationshipType::Calls) {
                        continue;
                    }
                    if rel.target_id == *hid {
                        if let Some(src) = graph.get_node(&rel.source_id) {
                            callers.push(json!({
                                "nodeId": src.id,
                                "name": src.properties.name,
                                "label": src.label.as_str(),
                            }));
                        }
                    }
                    if rel.source_id == *hid {
                        if let Some(tgt) = graph.get_node(&rel.target_id) {
                            callees.push(json!({
                                "nodeId": tgt.id,
                                "name": tgt.properties.name,
                                "label": tgt.label.as_str(),
                            }));
                        }
                    }
                }
            }
        }

        let payload = json!({
            "endpoint": {
                "nodeId": endpoint.id,
                "httpMethod": endpoint.properties.http_method,
                "route": endpoint.properties.route.clone().or(endpoint.properties.route_template.clone()),
                "framework": endpoint.properties.framework,
                "filePath": endpoint.properties.file_path,
                "startLine": endpoint.properties.start_line,
            },
            "handler": handler_json,
            "callers": callers,
            "callees": callees,
        });

        Ok(json!({
            "content": [{
                "type": "text",
                "text": serde_json::to_string_pretty(&payload).unwrap_or_else(|_| "{}".to_string())
            }],
            "_meta": {
                "hint": hints::hint_for("get_endpoint_handler"),
                "found": true,
                "hasHandler": handler_id.is_some(),
            }
        }))
    }

    /// Collect neighbor names from an adjacency list (incoming or outgoing).
    fn collect_neighbors(
        graph: &KnowledgeGraph,
        adjacency: &HashMap<String, Vec<(String, RelationshipType)>>,
        node_id: &str,
        limit: usize,
    ) -> Vec<Value> {
        adjacency
            .get(node_id)
            .map(|edges| {
                edges
                    .iter()
                    .filter(|(_, rt)| {
                        matches!(
                            rt,
                            RelationshipType::Calls
                                | RelationshipType::Uses
                                | RelationshipType::Imports
                                | RelationshipType::DependsOn
                                | RelationshipType::HasMethod
                                | RelationshipType::HasAction
                                | RelationshipType::CallsService
                        )
                    })
                    .take(limit)
                    .filter_map(|(nid, rt)| {
                        graph.get_node(nid).map(|n| {
                            json!({
                                "nodeId": nid,
                                "name": n.properties.name,
                                "label": format!("{:?}", n.label),
                                "relType": format!("{:?}", rt),
                            })
                        })
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    // ─── SFD doc-authoring workflow (P1.1) ────────────────────────────────
    //
    // Three tools delegating to `gitnexus_rag::sfd`. The lib enforces path
    // sandboxing (no `..`, no absolute paths, no traversal outside the docs
    // tree) so we can pass LLM-supplied parameters straight through.

    async fn tool_list_sfd_pages(&self, args: &Value) -> Result<Value> {
        let repo_name = args.get("repo").and_then(|v| v.as_str());
        let entry = self.resolve_repo(repo_name)?;
        let repo_path = std::path::Path::new(&entry.path);
        let listing = gitnexus_rag::sfd::list_pages(repo_path);
        let summary = if listing.missing {
            format!(
                "No docs directory at {} — run `gitnexus generate docs` first.",
                listing.docs_dir.display()
            )
        } else {
            let mut out = format!("Found {} module page(s):\n", listing.pages.len());
            for p in &listing.pages {
                out.push_str(&format!("- {}\n", p));
            }
            if !listing.drafts.is_empty() {
                out.push_str(&format!(
                    "\n{} draft(s) in _drafts/:\n",
                    listing.drafts.len()
                ));
                for d in &listing.drafts {
                    out.push_str(&format!("- {} (draft)\n", d));
                }
            }
            out
        };
        Ok(json!({
            "content": [{ "type": "text", "text": summary }],
            "_meta": {
                "pages": listing.pages,
                "drafts": listing.drafts,
                "docsDir": listing.docs_dir.display().to_string(),
                "missing": listing.missing,
            }
        }))
    }

    async fn tool_write_sfd_draft(&self, args: &Value) -> Result<Value> {
        let repo_name = args.get("repo").and_then(|v| v.as_str());
        let page = args.get("page").and_then(|v| v.as_str()).ok_or_else(|| {
            McpError::InvalidArguments {
                tool: "write_sfd_draft".into(),
                reason: "missing required parameter 'page'".into(),
            }
        })?;
        let content = args
            .get("content")
            .and_then(|v| v.as_str())
            .ok_or_else(|| McpError::InvalidArguments {
                tool: "write_sfd_draft".into(),
                reason: "missing required parameter 'content'".into(),
            })?;
        let entry = self.resolve_repo(repo_name)?;
        let repo_path = std::path::Path::new(&entry.path);
        let written = gitnexus_rag::sfd::write_draft(repo_path, page, content).map_err(|e| {
            McpError::InvalidArguments {
                tool: "write_sfd_draft".into(),
                reason: e.to_string(),
            }
        })?;
        let summary = format!(
            "Draft written: {} ({} bytes). Call `validate_sfd` with `path: \"_drafts\"` \
             to lint it before promotion.",
            written.path.display(),
            written.bytes
        );
        Ok(json!({
            "content": [{ "type": "text", "text": summary }],
            "_meta": {
                "path": written.path.display().to_string(),
                "bytes": written.bytes,
            }
        }))
    }

    async fn tool_validate_sfd(&self, args: &Value) -> Result<Value> {
        let repo_name = args.get("repo").and_then(|v| v.as_str());
        let sub_path = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
        let entry = self.resolve_repo(repo_name)?;
        let repo_path = std::path::Path::new(&entry.path);
        let report = gitnexus_rag::sfd::validate_draft(repo_path, sub_path).map_err(|e| {
            McpError::InvalidArguments {
                tool: "validate_sfd".into(),
                reason: e.to_string(),
            }
        })?;
        let status = if report.red_count == 0 && report.yellow_count == 0 {
            "GREEN — ready to ship"
        } else if report.red_count == 0 {
            "YELLOW — ships but has style issues"
        } else {
            "RED — blocked, must fix"
        };
        let mut summary = format!(
            "**Validation: {}**\n\n\
             - {} pages scanned, {} with issues\n\
             - {} RED issues, {} YELLOW issues\n",
            status,
            report.total_pages,
            report.pages_with_issues,
            report.red_count,
            report.yellow_count
        );
        if !report.by_kind.is_empty() {
            summary.push_str("\n**Issue kinds:**\n");
            for (kind, count) in &report.by_kind {
                summary.push_str(&format!("- {}: {}\n", kind, count));
            }
        }
        if !report.pages.is_empty() {
            summary.push_str("\n**Top pages with issues:**\n");
            let mut sorted = report.pages.clone();
            sorted.sort_by(|a, b| b.issues.len().cmp(&a.issues.len()));
            for page in sorted.iter().take(5) {
                summary.push_str(&format!(
                    "\n*{}* — {} issue(s):\n",
                    page.path,
                    page.issues.len()
                ));
                for iss in page.issues.iter().take(3) {
                    let sev = match iss.severity {
                        gitnexus_rag::validator::Severity::Red => "RED",
                        gitnexus_rag::validator::Severity::Yellow => "YELLOW",
                    };
                    let line = iss
                        .line
                        .map(|n| format!("L{}", n))
                        .unwrap_or_else(|| "-".to_string());
                    summary.push_str(&format!(
                        "  - [{}] {} {}: {}\n",
                        sev, line, iss.kind, iss.detail
                    ));
                }
                if page.issues.len() > 3 {
                    summary.push_str(&format!("  - ... +{} more\n", page.issues.len() - 3));
                }
            }
        }
        let report_json = serde_json::to_value(&report).unwrap_or(json!({}));
        Ok(json!({
            "content": [{ "type": "text", "text": summary }],
            "_meta": {
                "report": report_json,
                "status": match (report.red_count, report.yellow_count) {
                    (0, 0) => "green",
                    (0, _) => "yellow",
                    _ => "red",
                },
            }
        }))
    }
}

fn sanitize_mermaid_id(id: &str) -> String {
    id.replace([':', '/', '.', ' ', '<', '>', '(', ')', '{', '}'], "_")
}

/// Ranking for TODO-kind severity sort (lower = more urgent).
fn todo_rank(kind: &str) -> u8 {
    match kind {
        "FIXME" => 0,
        "HACK" => 1,
        "TODO" => 2,
        "XXX" => 3,
        _ => 4,
    }
}

/// Escape characters that would break a Mermaid `["..."]` label literal.
/// `sanitize_mermaid_id` is for *identifiers*; this is for the *display label*
/// shown to the user, which must preserve the original characters where
/// possible while not breaking Mermaid's parser.
///
/// Previously this only replaced `"` with `#quot;`, which left every other
/// problem character (`&`, `<`, `>`, `[`, `]`) to corrupt the label and broke
/// C# generics like `List<string>`, indexers like `Foo[int]`, and any symbol
/// name containing ampersands. We now use the standard HTML entity form
/// (`&amp;`, `&quot;`, `&lt;`, `&gt;`, `&#91;`, `&#93;`) to match the rest of
/// the codebase (`process_doc.rs`, `process.rs`, `diagram.rs`, `export.rs`,
/// `generate_aspnet.rs`, `generate/utils.rs`).
fn escape_mermaid_label(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('"', "&quot;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('[', "&#91;")
        .replace(']', "&#93;")
}

impl Default for LocalBackend {
    fn default() -> Self {
        Self::new()
    }
}

// ─── Helpers: git diff / range overlap / risk ───────────────────────────

/// Parse `git diff --unified=0 HEAD` into `Vec<(file, Vec<(start_line, end_line)>)>`.
/// Each range is inclusive and uses the NEW line numbering.
fn collect_git_diff_hunks(repo_path: &std::path::Path) -> Vec<(String, Vec<(u32, u32)>)> {
    let output = std::process::Command::new("git")
        .args(["diff", "--unified=0", "HEAD"])
        .current_dir(repo_path)
        .output();
    let stdout = match output {
        Ok(out) if out.status.success() => out.stdout,
        _ => return Vec::new(),
    };
    let text = String::from_utf8_lossy(&stdout);
    parse_diff_hunks(&text)
}

/// Pure parser over a unified-diff text (exposed for unit tests).
fn parse_diff_hunks(text: &str) -> Vec<(String, Vec<(u32, u32)>)> {
    let mut out: Vec<(String, Vec<(u32, u32)>)> = Vec::new();
    let mut current_file: Option<String> = None;
    for line in text.lines() {
        if let Some(rest) = line.strip_prefix("+++ ") {
            // Strip "b/" prefix git uses. "/dev/null" means a delete.
            let path = rest.trim();
            if path == "/dev/null" {
                current_file = None;
            } else {
                let p = path.strip_prefix("b/").unwrap_or(path);
                current_file = Some(p.to_string());
                out.push((p.to_string(), Vec::new()));
            }
        } else if let Some(rest) = line.strip_prefix("@@ ") {
            // "@@ -20,3 +25,4 @@ …"  →  parse the +25,4 part.
            let Some(file) = current_file.as_ref() else {
                continue;
            };
            if let Some(plus) = rest.split_whitespace().find(|t| t.starts_with('+')) {
                let spec = &plus[1..]; // strip leading '+'
                let (start_str, count_str) = spec.split_once(',').unwrap_or((spec, "1"));
                let start: u32 = start_str.parse().unwrap_or(0);
                let count: u32 = count_str.parse().unwrap_or(1);
                if count == 0 {
                    continue; // pure-deletion hunk, nothing to attribute on the new side
                }
                let end = start + count - 1;
                if let Some((_, ranges)) = out.iter_mut().find(|(f, _)| f == file) {
                    ranges.push((start, end));
                }
            }
        }
    }
    // Drop entries with no ranges (e.g. renames with no content diff).
    out.retain(|(_, ranges)| !ranges.is_empty());
    out
}

fn collect_git_untracked(repo_path: &std::path::Path) -> Vec<String> {
    let out = std::process::Command::new("git")
        .args(["ls-files", "--others", "--exclude-standard"])
        .current_dir(repo_path)
        .output();
    match out {
        Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout)
            .lines()
            .map(|l| l.to_string())
            .filter(|l| !l.is_empty())
            .collect(),
        _ => Vec::new(),
    }
}

fn ranges_overlap(a_start: u32, a_end: u32, b_start: u32, b_end: u32) -> bool {
    a_start <= b_end && b_start <= a_end
}

/// Scan a node's file within `[start_line..end_line]` for occurrences of a
/// regex and return one edit descriptor per match. Returns an empty vec when
/// the file cannot be read or the node has no file path.
fn scan_node_occurrences(
    repo_path: &std::path::Path,
    node: &gitnexus_core::graph::types::GraphNode,
    re: &regex::Regex,
    new_name: &str,
) -> Vec<Value> {
    let file_path = &node.properties.file_path;
    if file_path.is_empty() {
        return Vec::new();
    }
    let full = repo_path.join(file_path);
    let Ok(canonical_repo) = repo_path.canonicalize() else {
        return Vec::new();
    };
    let Ok(canonical_file) = full.canonicalize() else {
        return Vec::new();
    };
    if !canonical_file.starts_with(&canonical_repo) {
        return Vec::new();
    }
    let Ok(content) = std::fs::read_to_string(&canonical_file) else {
        return Vec::new();
    };

    let start = node.properties.start_line.unwrap_or(1).saturating_sub(1) as usize;
    let end = match node.properties.end_line {
        Some(e) => e as usize,
        None => return Vec::new(), // no source range — skip
    };

    let mut edits: Vec<Value> = Vec::new();
    for (idx, line) in content.lines().enumerate() {
        if idx < start || idx > end.saturating_sub(1) {
            continue;
        }
        for m in re.find_iter(line) {
            edits.push(json!({
                "file": file_path,
                "line": (idx + 1) as u32,
                "col": (m.start() + 1) as u32,
                "old_text": m.as_str(),
                "new_text": new_name,
                "snippet": truncate_snippet(line, 160),
            }));
        }
    }
    edits
}

/// Walk the repo (respecting .gitignore) and collect every `\btarget\b`
/// match NOT already present in `covered`.
fn scan_repo_for_identifier(
    repo_path: &std::path::Path,
    re: &regex::Regex,
    new_name: &str,
    covered: &std::collections::HashSet<(String, u32)>,
) -> Vec<Value> {
    let mut out: Vec<Value> = Vec::new();
    let walker = ignore::WalkBuilder::new(repo_path)
        .hidden(false)
        .git_ignore(true)
        .build();
    for entry in walker.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        // Skip binary-ish extensions cheaply.
        if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            if matches!(
                ext,
                "png"
                    | "jpg"
                    | "jpeg"
                    | "gif"
                    | "ico"
                    | "pdf"
                    | "zip"
                    | "tar"
                    | "gz"
                    | "bin"
                    | "exe"
                    | "dll"
                    | "so"
                    | "dylib"
                    | "jar"
                    | "class"
                    | "o"
                    | "a"
                    | "lib"
                    | "wasm"
                    | "mp4"
                    | "mp3"
                    | "woff"
                    | "woff2"
                    | "ttf"
                    | "eot"
            ) {
                continue;
            }
        }
        let Ok(content) = std::fs::read_to_string(path) else {
            continue;
        };
        let rel = match path.strip_prefix(repo_path) {
            Ok(p) => p.to_string_lossy().replace('\\', "/"),
            Err(_) => continue,
        };
        for (idx, line) in content.lines().enumerate() {
            let line_no = (idx + 1) as u32;
            if covered.contains(&(rel.clone(), line_no)) {
                continue;
            }
            if let Some(m) = re.find_iter(line).next() {
                out.push(json!({
                    "file": rel,
                    "line": line_no,
                    "col": (m.start() + 1) as u32,
                    "old_text": m.as_str(),
                    "new_text": new_name,
                    "snippet": truncate_snippet(line, 160),
                }));
            }
        }
        if out.len() > 500 {
            break; // cap response size for huge repos
        }
    }
    out
}

fn truncate_snippet(s: &str, max: usize) -> String {
    let t = s.trim();
    if t.len() <= max {
        t.to_string()
    } else {
        // Find the largest char boundary <= max to avoid panicking on multi-byte UTF-8
        let end = t
            .char_indices()
            .take_while(|(i, _)| *i < max)
            .last()
            .map(|(i, c)| i + c.len_utf8())
            .unwrap_or(0);
        format!("{}…", &t[..end])
    }
}

/// Apply graph_edits (high-confidence) to disk. Reads each file once,
/// patches all edits for it, then writes back. Returns per-file counts.
fn apply_edits_to_disk(repo_path: &std::path::Path, edits: &[Value]) -> std::io::Result<Value> {
    use std::collections::BTreeMap;
    // Group by file; sort edits bottom-up so offsets stay valid as we patch.
    let mut by_file: BTreeMap<String, Vec<(u32, u32, String, String)>> = BTreeMap::new();
    for e in edits {
        let Some(file) = e.get("file").and_then(|v| v.as_str()) else {
            continue;
        };
        let Some(line) = e.get("line").and_then(|v| v.as_u64()) else {
            continue;
        };
        let Some(col) = e.get("col").and_then(|v| v.as_u64()) else {
            continue;
        };
        let Some(old) = e.get("old_text").and_then(|v| v.as_str()) else {
            continue;
        };
        let Some(new) = e.get("new_text").and_then(|v| v.as_str()) else {
            continue;
        };
        by_file.entry(file.to_string()).or_default().push((
            line as u32,
            col as u32,
            old.to_string(),
            new.to_string(),
        ));
    }

    let mut applied_per_file: Vec<Value> = Vec::new();
    let canonical_repo = repo_path.canonicalize()?;
    for (file, mut edits) in by_file {
        edits.sort_by(|a, b| b.0.cmp(&a.0).then(b.1.cmp(&a.1)));
        let full = repo_path.join(&file);
        // Path traversal guard — skip if canonicalize fails or path escapes repo
        let canonical = match full.canonicalize() {
            Ok(c) => c,
            Err(_) => continue,
        };
        if !canonical.starts_with(&canonical_repo) {
            continue;
        }
        let content = std::fs::read_to_string(&canonical)?;
        let mut lines: Vec<String> = content.lines().map(String::from).collect();
        let mut applied = 0u32;
        for (line_no, col, old, new) in &edits {
            let idx = (*line_no as usize).saturating_sub(1);
            if idx >= lines.len() {
                continue;
            }
            let col_idx = (*col as usize).saturating_sub(1);
            let line = &lines[idx];
            let end = col_idx + old.len();
            if end <= line.len()
                && line.is_char_boundary(col_idx)
                && line.is_char_boundary(end)
                && &line[col_idx..end] == old
            {
                let mut patched = String::with_capacity(line.len() + new.len());
                patched.push_str(&line[..col_idx]);
                patched.push_str(new);
                patched.push_str(&line[end..]);
                lines[idx] = patched;
                applied += 1;
            }
        }
        // Preserve trailing newline if it was present.
        let mut new_content = lines.join("\n");
        if content.ends_with('\n') {
            new_content.push('\n');
        }
        std::fs::write(&canonical, new_content)?;
        applied_per_file.push(json!({"file": file, "applied": applied}));
    }
    Ok(json!({"files": applied_per_file}))
}

fn classify_risk(direct: usize, transitive: usize, processes: usize) -> &'static str {
    // Heuristic: a change is "high" if it hits a process or >10 transitive
    // dependents; "medium" if it touches 3+ direct symbols or any dependents;
    // "low" otherwise. Mirrors the upstream GitNexus risk buckets.
    if processes >= 2 || transitive >= 20 {
        "high"
    } else if processes >= 1 || transitive >= 5 || direct >= 3 {
        "medium"
    } else if direct > 0 {
        "low"
    } else {
        "none"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_local_backend_new() {
        let backend = LocalBackend::new();
        assert!(backend.registry().is_empty());
    }

    #[test]
    fn test_resolve_repo_empty() {
        let backend = LocalBackend::new();
        let result = backend.resolve_repo(None);
        assert!(result.is_err());
    }

    #[test]
    fn test_git_history_required_error_is_user_facing() {
        let message = git_history_required_error("hotspots").to_string();

        assert!(message.contains("Git analytics require"));
        assert!(!message.contains("fatal:"));
    }

    #[test]
    fn test_resolve_repo_not_found() {
        let backend = LocalBackend::new();
        let result = backend.resolve_repo(Some("nonexistent"));
        assert!(result.is_err());
    }

    #[test]
    fn test_resolve_repo_by_public_id() {
        let mut backend = LocalBackend::new();
        let entry = RegistryEntry {
            name: "gitnexus-rs".to_string(),
            path: "D:/Repos/gitnexus-rs".to_string(),
            storage_path: "D:/Repos/gitnexus-rs/.gitnexus".to_string(),
            indexed_at: "2026-05-06T05:00:00Z".to_string(),
            last_commit: "unknown".to_string(),
            stats: None,
        };
        let id = registry_entry_id(&entry);
        backend.registry.push(entry);

        let resolved = backend.resolve_repo(Some(&id)).unwrap();

        assert_eq!(resolved.name, "gitnexus-rs");
        assert_eq!(resolved.path, "D:/Repos/gitnexus-rs");
    }

    #[test]
    fn test_resolve_repo_accepts_windows_verbatim_path_equivalent() {
        let mut backend = LocalBackend::new();
        backend.registry.push(RegistryEntry {
            name: "sample-repo".to_string(),
            path: r"\\?\C:\repos\sample-repo".to_string(),
            storage_path: r"\\?\C:\repos\sample-repo\.gitnexus".to_string(),
            indexed_at: "2024-01-01".to_string(),
            last_commit: "abc123".to_string(),
            stats: None,
        });

        let resolved = backend.resolve_repo(Some(r"C:\repos\sample-repo")).unwrap();

        assert_eq!(resolved.name, "sample-repo");
    }

    #[tokio::test]
    async fn test_unknown_tool() {
        let mut backend = LocalBackend::new();
        let result = backend.call_tool("nonexistent", &json!({})).await;
        assert!(matches!(result, Err(McpError::UnknownTool(_))));
    }

    #[tokio::test]
    async fn test_cypher_write_rejected() {
        let mut backend = LocalBackend::new();
        // Initialize with a dummy entry so resolve_repo works
        backend.registry.push(RegistryEntry {
            name: "test".to_string(),
            path: "/tmp/test".to_string(),
            storage_path: "/tmp/test/.gitnexus".to_string(),
            indexed_at: "2024-01-01".to_string(),
            last_commit: "abc123".to_string(),
            stats: None,
        });

        let result = backend
            .call_tool("cypher", &json!({"query": "CREATE (n:File {id: '1'})"}))
            .await;
        assert!(matches!(result, Err(McpError::WriteQueryRejected)));
    }

    #[test]
    fn test_parse_diff_hunks_single_file() {
        let diff = "\
diff --git a/src/foo.ts b/src/foo.ts
index 1111111..2222222 100644
--- a/src/foo.ts
+++ b/src/foo.ts
@@ -10,3 +10,5 @@
@@ -40,0 +42,1 @@
";
        let parsed = parse_diff_hunks(diff);
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].0, "src/foo.ts");
        assert_eq!(parsed[0].1, vec![(10, 14), (42, 42)]);
    }

    #[test]
    fn test_parse_diff_hunks_pure_deletion_skipped() {
        // A hunk with +count=0 is a pure deletion and has no new-side lines
        // to attribute. Parser must skip it instead of emitting (line, line-1).
        let diff = "\
+++ b/a.rs
@@ -5,3 +5,0 @@
";
        let parsed = parse_diff_hunks(diff);
        assert!(
            parsed.is_empty(),
            "pure-deletion hunk should not produce a range"
        );
    }

    #[test]
    fn test_parse_diff_hunks_multiple_files() {
        let diff = "\
+++ b/a.ts
@@ -1 +1,2 @@
+++ b/b.ts
@@ -5,2 +5,3 @@
@@ -20 +21 @@
";
        let parsed = parse_diff_hunks(diff);
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].0, "a.ts");
        assert_eq!(parsed[0].1, vec![(1, 2)]);
        assert_eq!(parsed[1].0, "b.ts");
        assert_eq!(parsed[1].1, vec![(5, 7), (21, 21)]);
    }

    #[test]
    fn test_classify_risk_levels() {
        assert_eq!(classify_risk(0, 0, 0), "none");
        assert_eq!(classify_risk(1, 0, 0), "low");
        assert_eq!(classify_risk(3, 0, 0), "medium");
        assert_eq!(classify_risk(1, 6, 0), "medium");
        assert_eq!(classify_risk(0, 0, 1), "medium");
        assert_eq!(classify_risk(0, 0, 2), "high");
        assert_eq!(classify_risk(0, 20, 0), "high");
    }

    #[test]
    fn test_ranges_overlap() {
        assert!(ranges_overlap(10, 20, 15, 25));
        assert!(ranges_overlap(10, 20, 5, 15));
        assert!(ranges_overlap(10, 20, 10, 10));
        assert!(ranges_overlap(10, 20, 20, 20));
        assert!(!ranges_overlap(10, 20, 21, 30));
        assert!(!ranges_overlap(10, 20, 0, 9));
    }

    #[test]
    fn test_truncate_snippet() {
        assert_eq!(truncate_snippet("  hello  ", 10), "hello");
        assert_eq!(truncate_snippet("abcdef", 3), "abc…");
    }
}

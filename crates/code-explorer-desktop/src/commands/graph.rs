use std::collections::{HashSet, VecDeque};
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tauri::State;

use code_explorer_core::graph::types::{NodeLabel, RelationshipType};

use crate::commands::shared::{node_to_cyto, node_to_cyto_with_depth};
use crate::state::AppState;
use crate::types::*;

/// Get graph data filtered by zoom level.
#[tauri::command]
pub async fn get_graph_data(
    state: State<'_, AppState>,
    filter: GraphFilter,
) -> Result<GraphPayload, String> {
    let (graph, indexes, _fts, _repo_path) = state.get_repo(None).await?;

    let max_nodes = filter.max_nodes.unwrap_or(200);

    // Determine which node labels to include based on zoom level
    let allowed_labels = match filter.zoom_level {
        ZoomLevel::Package => vec![NodeLabel::Folder, NodeLabel::Package, NodeLabel::Project],
        ZoomLevel::Module => vec![NodeLabel::File, NodeLabel::Module, NodeLabel::Folder],
        ZoomLevel::Symbol => vec![
            NodeLabel::Function,
            NodeLabel::Class,
            NodeLabel::Method,
            NodeLabel::Interface,
            NodeLabel::Struct,
            NodeLabel::Trait,
            NodeLabel::Enum,
            NodeLabel::Variable,
            NodeLabel::Type,
            NodeLabel::Const,
            NodeLabel::Constructor,
            NodeLabel::Property,
            NodeLabel::Namespace,
            NodeLabel::Route,
            NodeLabel::Tool,
        ],
    };

    let allowed_rel_types = match filter.zoom_level {
        ZoomLevel::Package => vec![RelationshipType::Contains],
        ZoomLevel::Module => vec![RelationshipType::Contains, RelationshipType::Imports],
        ZoomLevel::Symbol => vec![
            RelationshipType::Calls,
            RelationshipType::Uses,
            RelationshipType::Imports,
            RelationshipType::Inherits,
            RelationshipType::Implements,
            RelationshipType::Extends,
        ],
    };

    // Custom label filter
    let custom_labels: Option<Vec<NodeLabel>> = filter.labels.as_ref().map(|labels| {
        labels
            .iter()
            .filter_map(|l| NodeLabel::from_str_label(l))
            .collect()
    });

    // Collect filtered nodes
    let filtered_nodes: Vec<_> = graph
        .iter_nodes()
        .filter(|node| {
            let label_ok = if let Some(ref custom) = custom_labels {
                custom.contains(&node.label)
            } else {
                allowed_labels.contains(&node.label)
            };
            if !label_ok {
                return false;
            }
            // File path filter. Each entry can be either an exact file path
            // or a directory prefix; for the directory case we must compare
            // against `{p}/` so a filter of `src/foo` doesn't accidentally
            // include nodes from `src/foobar/...`. Same substring-vs-segment
            // pattern as the dashboard.rs / cross_ref.rs / functional.rs
            // fixes earlier in this audit. Currently the frontend never
            // populates `filePaths`, but the field is part of the public IPC
            // contract and any future caller would hit the bug.
            if let Some(ref paths) = filter.file_paths {
                let fp = &node.properties.file_path;
                let matches = paths.iter().any(|p| {
                    if fp == p {
                        return true;
                    }
                    let dir_prefix = if p.ends_with('/') {
                        p.clone()
                    } else {
                        format!("{}/", p)
                    };
                    fp.starts_with(&dir_prefix)
                });
                if !matches {
                    return false;
                }
            }
            true
        })
        .collect();

    // Compute importance score for each node and sort descending
    let mut scored_nodes: Vec<(f64, &code_explorer_core::graph::types::GraphNode)> = filtered_nodes
        .into_iter()
        .map(|node| {
            let mut score: f64 = 0.0;

            // Connectivity (from indexes)
            let indegree = indexes
                .incoming
                .get(node.id.as_str())
                .map_or(0, |v| v.len());
            let outdegree = indexes
                .outgoing
                .get(node.id.as_str())
                .map_or(0, |v| v.len());
            score += (indegree + outdegree) as f64 * 2.0;

            // Entry point bonus
            if let Some(eps) = node.properties.entry_point_score {
                score += eps * 10.0;
            }

            // Exported symbols
            if node.properties.is_exported == Some(true) {
                score += 5.0;
            }

            // Traced symbols
            if node.properties.is_traced == Some(true) {
                score += 3.0;
            }

            // High-level types get priority
            match node.label {
                NodeLabel::Controller | NodeLabel::Service => score += 20.0,
                NodeLabel::Class | NodeLabel::Interface => score += 10.0,
                NodeLabel::Module | NodeLabel::Package => score += 15.0,
                _ => {}
            }

            (score, node)
        })
        .collect();

    // Use total_cmp for a total ordering on f64 (handles NaN consistently
    // and is symmetric, so the sort never panics in debug builds).
    scored_nodes.sort_by(|a, b| b.0.total_cmp(&a.0));

    // Capture how many candidates were available before truncating.
    let total_candidates = scored_nodes.len();

    // Take top N nodes
    let nodes: Vec<CytoNode> = scored_nodes
        .iter()
        .take(max_nodes)
        .map(|(_, node)| node_to_cyto(node))
        .collect();

    // Collect node ID set for edge filtering — only edges between selected nodes
    let node_ids: std::collections::HashSet<&str> = nodes.iter().map(|n| n.id.as_str()).collect();

    // Collect edges between selected nodes.
    // NOTE (M13): The outgoing index stores (target_id, Vec<RelationshipType>) but not
    // relationship IDs or confidence scores, so CytoEdge cannot be reconstructed from the
    // index alone. We keep the O(total_edges) scan here. To enable an O(selected * degree)
    // approach the index would need to store full relationship metadata (id, confidence).
    let mut edges = Vec::new();
    for rel in graph.iter_relationships() {
        if !allowed_rel_types.contains(&rel.rel_type) {
            continue;
        }
        if node_ids.contains(rel.source_id.as_str()) && node_ids.contains(rel.target_id.as_str()) {
            edges.push(rel_to_cyto(rel));
        }
    }

    // truncated only when we actually dropped candidates from the top-N cut.
    let truncated = total_candidates > nodes.len();
    let stats = GraphStats {
        node_count: nodes.len(),
        edge_count: edges.len(),
        truncated,
    };

    Ok(GraphPayload {
        nodes,
        edges,
        stats,
    })
}

/// Get a subgraph centered on a node, expanding to a given depth.
#[tauri::command]
pub async fn get_subgraph(
    state: State<'_, AppState>,
    center_node_id: String,
    depth: Option<u32>,
) -> Result<GraphPayload, String> {
    let (graph, indexes, _fts, _repo_path) = state.get_repo(None).await?;
    let max_depth = depth.unwrap_or(2).min(5); // Cap depth to prevent explosion
    const MAX_NODES: usize = 500; // Cap total nodes

    // BFS to collect neighborhood, tracking depth per node
    let mut visited = std::collections::HashSet::new();
    let mut depth_map = std::collections::HashMap::new();
    let mut queue = std::collections::VecDeque::new();

    visited.insert(center_node_id.clone());
    depth_map.insert(center_node_id.clone(), 0u32);
    queue.push_back((center_node_id, 0u32));

    // Track whether the BFS actually hit the MAX_NODES cap so the `truncated`
    // flag reported to the UI is meaningful. Previously the BFS inserted
    // every discovered neighbor into `visited` unconditionally and only
    // gated the *queue* expansion on MAX_NODES, so `visited.len()` could
    // grow far past the cap in a single frontier iteration and the final
    // payload returned more nodes than advertised.
    let mut hit_cap = false;

    'bfs: while let Some((node_id, d)) = queue.pop_front() {
        if d >= max_depth {
            continue;
        }

        // Outgoing neighbors
        if let Some(outs) = indexes.outgoing.get(&node_id) {
            for (target, _) in outs {
                if visited.len() >= MAX_NODES {
                    hit_cap = true;
                    break 'bfs;
                }
                if visited.insert(target.clone()) {
                    depth_map.insert(target.clone(), d + 1);
                    queue.push_back((target.clone(), d + 1));
                }
            }
        }

        // Incoming neighbors
        if let Some(ins) = indexes.incoming.get(&node_id) {
            for (source, _) in ins {
                if visited.len() >= MAX_NODES {
                    hit_cap = true;
                    break 'bfs;
                }
                if visited.insert(source.clone()) {
                    depth_map.insert(source.clone(), d + 1);
                    queue.push_back((source.clone(), d + 1));
                }
            }
        }
    }

    // Build payload
    let mut nodes = Vec::new();
    for id in &visited {
        if let Some(node) = graph.get_node(id) {
            let d = depth_map.get(id).copied();
            nodes.push(node_to_cyto_with_depth(node, d));
        }
    }

    let mut edges = Vec::new();
    for rel in graph.iter_relationships() {
        if visited.contains(&rel.source_id) && visited.contains(&rel.target_id) {
            edges.push(rel_to_cyto(rel));
        }
    }

    let stats = GraphStats {
        node_count: nodes.len(),
        edge_count: edges.len(),
        truncated: hit_cap,
    };

    Ok(GraphPayload {
        nodes,
        edges,
        stats,
    })
}

/// Get all detected features/communities with stats.
#[tauri::command]
pub async fn get_features(
    state: State<'_, AppState>,
) -> Result<Vec<crate::types::FeatureInfo>, String> {
    let (graph, _, _, _) = state.get_repo(None).await?;

    let mut features: Vec<crate::types::FeatureInfo> = Vec::new();

    for node in graph.iter_nodes() {
        if node.label == NodeLabel::Community {
            let community_name = &node.properties.name;

            // Use symbol_count from the Community node if available,
            // otherwise count nodes whose heuristic_label matches this community name.
            let member_count = node.properties.symbol_count.unwrap_or_else(|| {
                graph
                    .iter_nodes()
                    .filter(|n| n.properties.heuristic_label.as_deref() == Some(community_name))
                    .count() as u32
            });

            features.push(crate::types::FeatureInfo {
                id: node.id.clone(),
                name: node.properties.name.clone(),
                description: node.properties.description.clone(),
                member_count,
                cohesion: node.properties.cohesion,
            });
        }
    }

    // Sort by member count descending
    features.sort_by(|a, b| b.member_count.cmp(&a.member_count));

    // Cap at 50 communities to keep the sidebar panel performant and avoid
    // overwhelming the user with low-member groups.
    features.truncate(50);

    Ok(features)
}

// ─── Theme C — Call-path / shortest-path BFS ─────────────────────────────

/// Result of [`find_path`]: the shortest path from `from` to `to`, including
/// both endpoints. Returns `None` (Tauri-side serialized as `null`) when no
/// path exists within `max_depth` hops via the requested edge types.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FindPathResult {
    pub from: String,
    pub to: String,
    pub depth_used: u32,
    /// Node IDs along the path (length ≥ 2 when `found` is true).
    pub path: Vec<String>,
    pub found: bool,
}

/// BFS over the outgoing edge index, restricted to `edge_types`. Returns the
/// shortest path from `from_node_id` to `to_node_id`.
///
/// `edge_types` strings match `RelationshipType::as_str()` (SCREAMING_SNAKE_CASE,
/// e.g. `"CALLS"`, `"IMPORTS"`). Empty list = any edge type.
/// `max_depth` defaults to 10 and is capped at 50 to avoid pathological
/// traversals — call paths in practice are well under that ceiling.
#[tauri::command]
pub async fn find_path(
    state: State<'_, AppState>,
    from_node_id: String,
    to_node_id: String,
    edge_types: Option<Vec<String>>,
    max_depth: Option<u32>,
) -> Result<FindPathResult, String> {
    let (graph, indexes, _fts, _repo_path) = state.get_repo(None).await?;

    if graph.get_node(&from_node_id).is_none() {
        return Err(format!("source node '{from_node_id}' not found"));
    }
    if graph.get_node(&to_node_id).is_none() {
        return Err(format!("target node '{to_node_id}' not found"));
    }

    let depth_cap = max_depth.unwrap_or(10).min(50);

    if from_node_id == to_node_id {
        return Ok(FindPathResult {
            from: from_node_id.clone(),
            to: to_node_id,
            depth_used: 0,
            path: vec![from_node_id],
            found: true,
        });
    }

    // Normalise allowed edge types into a HashSet for O(1) membership check.
    // Comparison is done against the SCREAMING_SNAKE_CASE form.
    let edge_filter: Option<HashSet<String>> = edge_types
        .filter(|v| !v.is_empty())
        .map(|v| v.into_iter().map(|s| s.to_uppercase()).collect());

    // Standard BFS with parent tracking.
    let mut visited: HashSet<String> = HashSet::new();
    let mut parent: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    let mut depth: std::collections::HashMap<String, u32> = std::collections::HashMap::new();
    let mut queue: VecDeque<String> = VecDeque::new();

    visited.insert(from_node_id.clone());
    depth.insert(from_node_id.clone(), 0);
    queue.push_back(from_node_id.clone());

    while let Some(node) = queue.pop_front() {
        let d = *depth.get(&node).unwrap_or(&0);
        if d >= depth_cap {
            continue;
        }
        let Some(outs) = indexes.outgoing.get(&node) else {
            continue;
        };
        for (target, rel_type) in outs {
            if let Some(filter) = &edge_filter {
                if !filter.contains(rel_type.as_str()) {
                    continue;
                }
            }
            if !visited.insert(target.clone()) {
                continue;
            }
            parent.insert(target.clone(), node.clone());
            depth.insert(target.clone(), d + 1);
            if target == &to_node_id {
                // Reconstruct path from `to_node_id` walking parents back to `from_node_id`.
                let mut path = vec![to_node_id.clone()];
                let mut cur = to_node_id.clone();
                while let Some(p) = parent.get(&cur) {
                    path.push(p.clone());
                    if p == &from_node_id {
                        break;
                    }
                    cur = p.clone();
                }
                path.reverse();
                let depth_used = (path.len() as u32).saturating_sub(1);
                return Ok(FindPathResult {
                    from: from_node_id,
                    to: to_node_id,
                    depth_used,
                    path,
                    found: true,
                });
            }
            queue.push_back(target.clone());
        }
    }

    Ok(FindPathResult {
        from: from_node_id,
        to: to_node_id,
        depth_used: depth_cap,
        path: Vec::new(),
        found: false,
    })
}

// ─── Theme C — Snapshot diff (graph-aware) ───────────────────────────────

/// Resolve a snapshot id (or `"live"` / `"current"`) to a path on disk.
fn resolve_snapshot_path(storage: &str, id: &str) -> Result<PathBuf, String> {
    if id == "live" || id == "current" {
        let p = PathBuf::from(storage).join("graph.bin");
        if !p.exists() {
            return Err("Live graph.bin not found".into());
        }
        return Ok(p);
    }
    let safe: String = id
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-')
        .collect();
    let p = PathBuf::from(storage)
        .join("snapshots")
        .join(format!("{safe}.bin"));
    if !p.exists() {
        return Err(format!("Snapshot '{id}' not found"));
    }
    Ok(p)
}

/// Compute a structural diff between two snapshots (or one snapshot vs the
/// live graph). Wraps [`code_explorer_db::analytics::graph_diff::diff_graphs`].
///
/// Frontend-friendly fields: added/removed node IDs, added/removed edges as
/// `{source, target, relType}` triples, modified nodes as `{nodeId, changedProps}`.
#[tauri::command]
pub async fn diff_snapshots(
    state: State<'_, AppState>,
    from: String,
    to: String,
) -> Result<code_explorer_db::analytics::graph_diff::GraphDiff, String> {
    let storage = state.active_storage_path().await?;
    let from_path = resolve_snapshot_path(&storage, &from)?;
    let to_path = resolve_snapshot_path(&storage, &to)?;

    let a = code_explorer_db::snapshot::load_snapshot(&from_path)
        .map_err(|e| format!("Failed to load 'from' snapshot: {e}"))?;
    let b = code_explorer_db::snapshot::load_snapshot(&to_path)
        .map_err(|e| format!("Failed to load 'to' snapshot: {e}"))?;

    Ok(code_explorer_db::analytics::graph_diff::diff_graphs(&a, &b))
}

// ─── Helpers ─────────────────────────────────────────────────────────────

fn rel_to_cyto(rel: &code_explorer_core::graph::types::GraphRelationship) -> CytoEdge {
    CytoEdge {
        id: rel.id.clone(),
        source: rel.source_id.clone(),
        target: rel.target_id.clone(),
        rel_type: rel.rel_type.as_str().to_string(),
        confidence: rel.confidence,
    }
}

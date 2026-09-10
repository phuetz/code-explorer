use std::collections::{HashMap, HashSet, VecDeque};

use tauri::State;

use crate::commands::shared::node_to_cyto;
use crate::state::AppState;
use crate::types::*;

/// Blast radius analysis via BFS traversal.
#[tauri::command]
pub async fn get_impact_analysis(
    state: State<'_, AppState>,
    target_id: String,
    direction: Option<String>,
    max_depth: Option<u32>,
) -> Result<ImpactResult, String> {
    let (graph, indexes, _fts, _repo_path) = state.get_repo(None).await?;
    let depth_limit = max_depth.unwrap_or(5);
    let dir = direction.as_deref().unwrap_or("both");

    let target_node = graph
        .get_node(&target_id)
        .ok_or_else(|| format!("Node '{}' not found", target_id))?;

    let target = node_to_cyto(target_node);

    // BFS upstream (incoming edges)
    let upstream = if dir == "upstream" || dir == "both" {
        bfs_impact(&graph, &indexes, &target_id, depth_limit, true)
    } else {
        Vec::new()
    };

    // BFS downstream (outgoing edges)
    let downstream = if dir == "downstream" || dir == "both" {
        bfs_impact(&graph, &indexes, &target_id, depth_limit, false)
    } else {
        Vec::new()
    };

    // Collect all affected files
    let mut affected_files: HashSet<String> = HashSet::new();
    for impact in upstream.iter().chain(downstream.iter()) {
        affected_files.insert(impact.node.file_path.clone());
    }
    let mut affected_files: Vec<String> = affected_files.into_iter().collect();
    affected_files.sort();

    // Build subgraph for visualization
    let mut all_node_ids: HashSet<String> = HashSet::new();
    all_node_ids.insert(target_id.clone());
    for n in upstream.iter().chain(downstream.iter()) {
        all_node_ids.insert(n.node.id.clone());
    }

    let mut graph_nodes = Vec::new();
    for id in &all_node_ids {
        if let Some(n) = graph.get_node(id) {
            graph_nodes.push(node_to_cyto(n));
        }
    }

    // NOTE (M13): Same O(total_edges) scan as in graph.rs — kept because the outgoing index
    // does not store relationship IDs or confidence scores needed to build CytoEdge.
    let mut graph_edges = Vec::new();
    for rel in graph.iter_relationships() {
        if all_node_ids.contains(&rel.source_id) && all_node_ids.contains(&rel.target_id) {
            graph_edges.push(CytoEdge {
                id: rel.id.clone(),
                source: rel.source_id.clone(),
                target: rel.target_id.clone(),
                rel_type: rel.rel_type.as_str().to_string(),
                confidence: rel.confidence,
            });
        }
    }

    let max_depth_reached = upstream
        .iter()
        .chain(downstream.iter())
        .map(|n| n.depth)
        .max()
        .unwrap_or(0);

    let summary = ImpactSummary {
        upstream_count: upstream.len(),
        downstream_count: downstream.len(),
        affected_files_count: affected_files.len(),
        max_depth: max_depth_reached,
    };

    let edge_count = graph_edges.len();

    Ok(ImpactResult {
        target,
        upstream,
        downstream,
        graph: GraphPayload {
            nodes: graph_nodes,
            edges: graph_edges,
            stats: GraphStats {
                node_count: all_node_ids.len(),
                edge_count,
                truncated: false,
            },
        },
        affected_files,
        summary,
    })
}

/// Public version of bfs_impact for use by the agent tool loop.
pub fn bfs_impact_pub(
    graph: &code_explorer_core::graph::KnowledgeGraph,
    indexes: &code_explorer_db::inmemory::cypher::GraphIndexes,
    start_id: &str,
    max_depth: u32,
    upstream: bool,
) -> Vec<ImpactNode> {
    bfs_impact(graph, indexes, start_id, max_depth, upstream)
}

fn bfs_impact(
    graph: &code_explorer_core::graph::KnowledgeGraph,
    indexes: &code_explorer_db::inmemory::cypher::GraphIndexes,
    start_id: &str,
    max_depth: u32,
    upstream: bool,
) -> Vec<ImpactNode> {
    use code_explorer_core::graph::types::RelationshipType as R;

    // Only traverse edge types that actually represent "change X and Y may
    // break" semantics. Previously the BFS discarded the rel_type entirely
    // and followed every relationship, including purely structural edges
    // (`Contains`, `HasMethod`, `HasProperty`, `MemberOf`, `Defines`). A
    // method living in a large community would then list every other
    // member as "affected" via the MemberOf edge, producing wildly inflated
    // impact counts. Match the filter already used by
    // `chat_executor::execute_impact`, extended with the other causal edge
    // types the graph stores.
    let is_impact_edge = |rel_type: &R| -> bool {
        matches!(
            rel_type,
            R::Calls
                | R::CallsAction
                | R::CallsService
                | R::Imports
                | R::Inherits
                | R::Implements
                | R::Extends
                | R::Uses
                | R::Overrides
                | R::DependsOn
                | R::RendersView
                | R::HandlesRoute
                | R::Fetches
                | R::MapsToEntity
        )
    };

    let mut visited: HashSet<String> = HashSet::new();
    let mut parent_map: HashMap<String, String> = HashMap::new();
    let mut queue: VecDeque<(String, u32)> = VecDeque::new();
    let mut results = Vec::new();

    visited.insert(start_id.to_string());
    queue.push_back((start_id.to_string(), 0));

    while let Some((node_id, depth)) = queue.pop_front() {
        if depth >= max_depth {
            continue;
        }

        let neighbors = if upstream {
            indexes.incoming.get(&node_id)
        } else {
            indexes.outgoing.get(&node_id)
        };

        if let Some(neighbors) = neighbors {
            for (neighbor_id, rel_type) in neighbors {
                if !is_impact_edge(rel_type) {
                    continue;
                }
                if visited.insert(neighbor_id.clone()) {
                    parent_map.insert(neighbor_id.clone(), node_id.clone());
                    queue.push_back((neighbor_id.clone(), depth + 1));

                    if let Some(node) = graph.get_node(neighbor_id) {
                        // Build path from start to this node
                        let mut path = Vec::new();
                        let mut cursor = neighbor_id.clone();
                        while let Some(parent) = parent_map.get(&cursor) {
                            path.push(cursor.clone());
                            cursor = parent.clone();
                        }
                        path.push(start_id.to_string());
                        path.reverse();

                        results.push(ImpactNode {
                            node: node_to_cyto(node),
                            depth: depth + 1,
                            path,
                        });
                    }
                }
            }
        }
    }

    results
}

use std::collections::{HashMap, HashSet};

use code_explorer_core::graph::types::*;
use code_explorer_core::graph::KnowledgeGraph;
use code_explorer_core::id::generate_id;

/// Timeout for community detection in milliseconds.
#[allow(dead_code)]
const LEIDEN_TIMEOUT_MS: u64 = 60_000;

/// Minimum confidence for edges in large graphs.
const MIN_CONFIDENCE_LARGE: f64 = 0.5;

/// Node labels eligible for community membership.
const COMMUNITY_ELIGIBLE_LABELS: &[NodeLabel] = &[
    NodeLabel::Function,
    NodeLabel::Method,
    NodeLabel::Class,
    NodeLabel::Interface,
    NodeLabel::Struct,
    NodeLabel::Trait,
    NodeLabel::Enum,
    NodeLabel::Constructor,
];

/// Edge types that form community connections.
const COMMUNITY_EDGE_TYPES: &[RelationshipType] = &[
    RelationshipType::Calls,
    RelationshipType::Extends,
    RelationshipType::Implements,
    RelationshipType::Imports,
];

/// Detect communities using a simplified Louvain/Leiden algorithm.
///
/// Steps:
/// 1. Build an undirected graph from CALLS/EXTENDS/IMPLEMENTS edges
/// 2. Run iterative modularity optimization (Louvain-style)
/// 3. Generate heuristic labels from folder paths
/// 4. Calculate cohesion scores
/// 5. Create Community nodes and MEMBER_OF edges
pub fn detect_communities(graph: &mut KnowledgeGraph) -> Result<usize, crate::IngestError> {
    // Build adjacency structure from eligible nodes and edges
    let (node_ids, adjacency, edge_count) = build_community_graph(graph);

    if node_ids.is_empty() {
        return Ok(0);
    }

    let is_large = node_ids.len() > 10_000;
    let resolution = if is_large { 2.0 } else { 1.0 };
    let max_iterations = if is_large { 3 } else { 10 };

    // Run Louvain-style community detection
    let assignments = louvain_communities(
        &node_ids,
        &adjacency,
        edge_count,
        resolution,
        max_iterations,
    );

    // Group nodes by community
    let mut communities: HashMap<usize, Vec<&str>> = HashMap::new();
    for (i, &community_id) in assignments.iter().enumerate() {
        communities
            .entry(community_id)
            .or_default()
            .push(&node_ids[i]);
    }

    // Filter out singleton communities (need at least 2 members)
    let communities: HashMap<usize, Vec<&str>> = communities
        .into_iter()
        .filter(|(_, members)| members.len() >= 2)
        .collect();

    let community_count = communities.len();

    // Create Community nodes and MEMBER_OF edges
    let mut sorted_communities: Vec<_> = communities.into_iter().collect();
    sorted_communities.sort_by(|(a, _), (b, _)| a.cmp(b));
    for (idx, (_, members)) in sorted_communities.iter().enumerate() {
        let community_name = generate_heuristic_label(graph, members);
        let community_id = generate_id("Community", &format!("community_{idx}"));

        // Calculate cohesion
        let cohesion = calculate_cohesion(members, &adjacency, &node_ids);

        let community_node = GraphNode {
            id: community_id.clone(),
            label: NodeLabel::Community,
            properties: NodeProperties {
                name: community_name,
                file_path: String::new(),
                heuristic_label: None,
                cohesion: Some(cohesion),
                symbol_count: Some(members.len() as u32),
                enriched_by: Some(EnrichedBy::Heuristic),
                ..Default::default()
            },
        };
        graph.add_node(community_node);

        // Create MEMBER_OF edges
        for member_id in members {
            let edge_id = format!("member_of_{}_{}", member_id, community_id);
            graph.add_relationship(GraphRelationship {
                id: edge_id,
                source_id: member_id.to_string(),
                target_id: community_id.clone(),
                rel_type: RelationshipType::MemberOf,
                confidence: 1.0,
                reason: "community-detection".to_string(),
                step: None,
            });
        }
    }

    Ok(community_count)
}

/// Build an undirected adjacency list from the knowledge graph.
/// Returns (node_ids, adjacency_map, total_edge_count).
fn build_community_graph(
    graph: &KnowledgeGraph,
) -> (Vec<String>, HashMap<String, HashSet<String>>, usize) {
    let eligible_labels: HashSet<NodeLabel> = COMMUNITY_ELIGIBLE_LABELS.iter().copied().collect();
    let eligible_edge_types: HashSet<RelationshipType> =
        COMMUNITY_EDGE_TYPES.iter().copied().collect();

    // Collect eligible node IDs
    let mut node_ids: Vec<String> = Vec::new();
    let eligible_node_set: HashSet<String> = {
        let mut set = HashSet::new();
        graph.for_each_node(|node| {
            if eligible_labels.contains(&node.label) {
                set.insert(node.id.clone());
                node_ids.push(node.id.clone());
            }
        });
        set
    };

    let is_large = eligible_node_set.len() > 10_000;

    // Build adjacency
    let mut adjacency: HashMap<String, HashSet<String>> = HashMap::new();
    let mut edge_count = 0usize;

    graph.for_each_relationship(|rel| {
        if !eligible_edge_types.contains(&rel.rel_type) {
            return;
        }
        if !eligible_node_set.contains(&rel.source_id)
            || !eligible_node_set.contains(&rel.target_id)
        {
            return;
        }
        // Skip low-confidence edges in large graphs
        if is_large && rel.confidence < MIN_CONFIDENCE_LARGE {
            return;
        }

        // Undirected: add both directions
        adjacency
            .entry(rel.source_id.clone())
            .or_default()
            .insert(rel.target_id.clone());
        adjacency
            .entry(rel.target_id.clone())
            .or_default()
            .insert(rel.source_id.clone());
        edge_count += 1;
    });

    node_ids.sort();
    (node_ids, adjacency, edge_count)
}

/// Simplified Louvain modularity optimization.
///
/// Phase 1: Each node starts in its own community.
/// Phase 2: Greedily move nodes to neighbor's community if modularity improves.
/// Repeat until convergence or max_iterations.
fn louvain_communities(
    node_ids: &[String],
    adjacency: &HashMap<String, HashSet<String>>,
    total_edges: usize,
    resolution: f64,
    max_iterations: usize,
) -> Vec<usize> {
    let n = node_ids.len();
    if n == 0 || total_edges == 0 {
        return (0..n).collect();
    }

    let m2 = (2 * total_edges) as f64; // 2m for modularity formula

    // Node index lookup
    let node_index: HashMap<&str, usize> = node_ids
        .iter()
        .enumerate()
        .map(|(i, id)| (id.as_str(), i))
        .collect();

    // Degree of each node
    let degrees: Vec<f64> = node_ids
        .iter()
        .map(|id| adjacency.get(id).map_or(0, |adj| adj.len()) as f64)
        .collect();

    // community[i] = community ID for node i
    let mut community: Vec<usize> = (0..n).collect();

    // Sum of degrees within each community
    let mut community_degree_sum: Vec<f64> = degrees.clone();

    for _iteration in 0..max_iterations {
        let mut improved = false;

        for i in 0..n {
            let current_community = community[i];
            let ki = degrees[i];

            if ki == 0.0 {
                continue;
            }

            // Count edges to each neighboring community
            let mut neighbor_community_edges: HashMap<usize, f64> = HashMap::new();
            if let Some(neighbors) = adjacency.get(&node_ids[i]) {
                for neighbor in neighbors {
                    if let Some(&j) = node_index.get(neighbor.as_str()) {
                        *neighbor_community_edges.entry(community[j]).or_default() += 1.0;
                    }
                }
            }

            // Try removing node from current community
            let ki_in = neighbor_community_edges
                .get(&current_community)
                .copied()
                .unwrap_or(0.0);
            let sigma_tot_current = community_degree_sum[current_community];

            // Delta Q for removing from current community
            let remove_cost = ki_in / m2 - resolution * ki * (sigma_tot_current - ki) / (m2 * m2);

            // Find best community to move to
            let mut best_community = current_community;
            let mut best_gain = 0.0;

            for (&target_community, &edges_to_target) in &neighbor_community_edges {
                if target_community == current_community {
                    continue;
                }

                let sigma_tot_target = community_degree_sum[target_community];

                // Delta Q for adding to target community
                let add_gain =
                    edges_to_target / m2 - resolution * ki * sigma_tot_target / (m2 * m2);

                let total_gain = add_gain - remove_cost;
                if total_gain > best_gain {
                    best_gain = total_gain;
                    best_community = target_community;
                }
            }

            // Move node if there's improvement
            if best_community != current_community && best_gain > 1e-10 {
                community_degree_sum[current_community] -= ki;
                community_degree_sum[best_community] += ki;
                community[i] = best_community;
                improved = true;
            }
        }

        if !improved {
            break;
        }
    }

    // Renumber communities to be contiguous from 0
    let mut seen: HashMap<usize, usize> = HashMap::new();
    let mut next_id = 0;
    for c in &mut community {
        let new_id = *seen.entry(*c).or_insert_with(|| {
            let id = next_id;
            next_id += 1;
            id
        });
        *c = new_id;
    }

    community
}

/// Generate a heuristic label for a community based on common folder paths.
fn generate_heuristic_label(graph: &KnowledgeGraph, members: &[&str]) -> String {
    let mut folder_counts: HashMap<&str, usize> = HashMap::new();

    for member_id in members {
        if let Some(node) = graph.get_node(member_id) {
            let file_path = &node.properties.file_path;
            // Extract the deepest meaningful folder
            if let Some(last_slash) = file_path.rfind('/') {
                let folder = &file_path[..last_slash];
                // Use the last folder component
                let label = folder.rsplit('/').next().unwrap_or(folder);
                if !label.is_empty() {
                    *folder_counts.entry(label).or_default() += 1;
                }
            }
        }
    }

    // Pick the most common folder name
    folder_counts
        .into_iter()
        .max_by_key(|(_, count)| *count)
        .map(|(name, _)| name.to_string())
        .unwrap_or_else(|| "unnamed".to_string())
}

/// Calculate cohesion: internal edges / total edges for community members.
fn calculate_cohesion(
    members: &[&str],
    adjacency: &HashMap<String, HashSet<String>>,
    _all_node_ids: &[String],
) -> f64 {
    let member_set: HashSet<&str> = members.iter().copied().collect();
    let mut internal_edges = 0usize;
    let mut total_edges = 0usize;

    for member in members {
        if let Some(neighbors) = adjacency.get(*member) {
            for neighbor in neighbors {
                total_edges += 1;
                if member_set.contains(neighbor.as_str()) {
                    internal_edges += 1;
                }
            }
        }
    }

    if total_edges == 0 {
        return 0.0;
    }

    internal_edges as f64 / total_edges as f64
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_node(id: &str, label: NodeLabel, file_path: &str) -> GraphNode {
        GraphNode {
            id: id.to_string(),
            label,
            properties: NodeProperties {
                name: id.to_string(),
                file_path: file_path.to_string(),
                ..Default::default()
            },
        }
    }

    fn make_calls_edge(src: &str, tgt: &str) -> GraphRelationship {
        GraphRelationship {
            id: format!("calls_{}_{}", src, tgt),
            source_id: src.to_string(),
            target_id: tgt.to_string(),
            rel_type: RelationshipType::Calls,
            confidence: 1.0,
            reason: "test".to_string(),
            step: None,
        }
    }

    #[test]
    fn test_detect_communities_empty_graph() {
        let mut graph = KnowledgeGraph::new();
        let count = detect_communities(&mut graph).unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn test_detect_communities_two_clusters() {
        let mut graph = KnowledgeGraph::new();

        // Cluster 1: a1 <-> a2 <-> a3
        graph.add_node(make_node("a1", NodeLabel::Function, "src/auth/login.ts"));
        graph.add_node(make_node("a2", NodeLabel::Function, "src/auth/logout.ts"));
        graph.add_node(make_node("a3", NodeLabel::Function, "src/auth/session.ts"));
        graph.add_relationship(make_calls_edge("a1", "a2"));
        graph.add_relationship(make_calls_edge("a2", "a3"));
        graph.add_relationship(make_calls_edge("a3", "a1"));

        // Cluster 2: b1 <-> b2 <-> b3
        graph.add_node(make_node("b1", NodeLabel::Function, "src/db/query.ts"));
        graph.add_node(make_node("b2", NodeLabel::Function, "src/db/connect.ts"));
        graph.add_node(make_node("b3", NodeLabel::Function, "src/db/migrate.ts"));
        graph.add_relationship(make_calls_edge("b1", "b2"));
        graph.add_relationship(make_calls_edge("b2", "b3"));
        graph.add_relationship(make_calls_edge("b3", "b1"));

        let count = detect_communities(&mut graph).unwrap();
        // Should detect at least 1 community (possibly 2 distinct clusters)
        assert!(count >= 1);

        // Verify Community nodes were created
        let community_nodes: Vec<_> = graph
            .nodes()
            .into_iter()
            .filter(|n| n.label == NodeLabel::Community)
            .collect();
        assert!(!community_nodes.is_empty());

        // Verify MEMBER_OF edges were created
        let member_of_edges: Vec<_> = graph
            .relationships()
            .into_iter()
            .filter(|r| r.rel_type == RelationshipType::MemberOf)
            .collect();
        assert!(!member_of_edges.is_empty());
    }

    #[test]
    fn test_detect_communities_no_edges() {
        let mut graph = KnowledgeGraph::new();
        graph.add_node(make_node("f1", NodeLabel::Function, "a.ts"));
        graph.add_node(make_node("f2", NodeLabel::Function, "b.ts"));

        let count = detect_communities(&mut graph).unwrap();
        // No edges means no communities (all singletons get filtered)
        assert_eq!(count, 0);
    }

    #[test]
    fn test_louvain_basic() {
        let node_ids: Vec<String> = vec!["a".into(), "b".into(), "c".into(), "d".into()];
        let mut adjacency: HashMap<String, HashSet<String>> = HashMap::new();

        // a-b connected, c-d connected, no cross-cluster edges
        adjacency.entry("a".into()).or_default().insert("b".into());
        adjacency.entry("b".into()).or_default().insert("a".into());
        adjacency.entry("c".into()).or_default().insert("d".into());
        adjacency.entry("d".into()).or_default().insert("c".into());

        let result = louvain_communities(&node_ids, &adjacency, 2, 1.0, 10);

        // a and b should be in the same community
        assert_eq!(result[0], result[1]);
        // c and d should be in the same community
        assert_eq!(result[2], result[3]);
        // The two clusters should be different
        assert_ne!(result[0], result[2]);
    }

    #[test]
    fn test_calculate_cohesion_full() {
        let mut adjacency: HashMap<String, HashSet<String>> = HashMap::new();
        // Fully connected: a-b, b-a (all edges internal)
        adjacency.entry("a".into()).or_default().insert("b".into());
        adjacency.entry("b".into()).or_default().insert("a".into());

        let members = vec!["a", "b"];
        let all_node_ids: Vec<String> = vec!["a".into(), "b".into()];
        let cohesion = calculate_cohesion(&members, &adjacency, &all_node_ids);
        assert!((cohesion - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_calculate_cohesion_none() {
        let mut adjacency: HashMap<String, HashSet<String>> = HashMap::new();
        // a connects to c (external), not to b
        adjacency.entry("a".into()).or_default().insert("c".into());

        let members = vec!["a", "b"];
        let all_node_ids: Vec<String> = vec!["a".into(), "b".into(), "c".into()];
        let cohesion = calculate_cohesion(&members, &adjacency, &all_node_ids);
        assert!((cohesion - 0.0).abs() < f64::EPSILON);
    }
}

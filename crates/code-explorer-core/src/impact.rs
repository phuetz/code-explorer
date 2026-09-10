//! Shared impact analysis engine for CLI, MCP, and UI callers.

use std::collections::{HashMap, HashSet, VecDeque};

use crate::graph::types::{NodeLabel, RelationshipType};
use crate::graph::KnowledgeGraph;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImpactTarget {
    pub id: String,
    pub name: String,
    pub label: NodeLabel,
    pub file_path: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImpactNode {
    pub id: String,
    pub depth: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImpactDirectionResult {
    pub nodes: Vec<ImpactNode>,
    pub depth_counts: Vec<usize>,
}

impl ImpactDirectionResult {
    pub fn total(&self) -> usize {
        self.nodes.len()
    }

    pub fn node_ids_at_depth(&self, depth: usize) -> impl Iterator<Item = &str> {
        self.nodes
            .iter()
            .filter(move |node| node.depth == depth)
            .map(|node| node.id.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImpactAnalysis {
    pub target: ImpactTarget,
    pub seed_ids: Vec<String>,
    pub downstream: ImpactDirectionResult,
    pub upstream: ImpactDirectionResult,
}

pub fn analyze_impact(
    graph: &KnowledgeGraph,
    target: &str,
    max_depth: usize,
) -> Option<ImpactAnalysis> {
    let target_node = resolve_impact_target(graph, target)?;
    let seed_ids = collect_impact_seed_ids(graph, &target_node);
    let (forward, reverse) = build_impact_adjacency(graph);

    Some(ImpactAnalysis {
        downstream: bfs_impact(graph, &seed_ids, &forward, max_depth),
        upstream: bfs_impact(graph, &seed_ids, &reverse, max_depth),
        seed_ids,
        target: target_node,
    })
}

pub fn impact_node_priority(label: NodeLabel) -> usize {
    match label {
        NodeLabel::Controller => 0,
        NodeLabel::Class => 1,
        NodeLabel::Service => 2,
        NodeLabel::Method => 5,
        NodeLabel::File => 8,
        _ => 10,
    }
}

pub fn is_impact_traversal_edge(rel_type: RelationshipType) -> bool {
    // Keep membership edges traversable: impact often flows from a method to a
    // called class, then into that class's members, or upstream from a method
    // to its owning class. Skip only structural/noise relationships that make
    // impact walk duplicate or non-causal paths.
    !matches!(
        rel_type,
        RelationshipType::Contains
            | RelationshipType::StepInProcess
            | RelationshipType::Defines
            | RelationshipType::MemberOf
            | RelationshipType::BelongsToArea
    )
}

fn resolve_impact_target(graph: &KnowledgeGraph, target: &str) -> Option<ImpactTarget> {
    let target_lower = target.to_lowercase();
    let mut matches: Vec<_> = graph
        .iter_nodes()
        .filter(|node| node.id == target || node.properties.name.to_lowercase() == target_lower)
        .collect();

    if matches.is_empty() {
        matches = graph
            .iter_nodes()
            .filter(|node| node.properties.name.to_lowercase().contains(&target_lower))
            .collect();
    }

    matches.sort_by_key(|node| impact_node_priority(node.label));
    let node = matches.first()?;
    Some(ImpactTarget {
        id: node.id.clone(),
        name: node.properties.name.clone(),
        label: node.label,
        file_path: node.properties.file_path.clone(),
    })
}

fn collect_impact_seed_ids(graph: &KnowledgeGraph, target: &ImpactTarget) -> Vec<String> {
    let mut seed_ids: Vec<String> = vec![target.id.clone()];
    if !matches!(
        target.label,
        NodeLabel::Class | NodeLabel::Service | NodeLabel::Interface | NodeLabel::Controller
    ) {
        return seed_ids;
    }

    let mut source_ids = vec![target.id.clone()];
    if target.label == NodeLabel::Controller {
        for node in graph.iter_nodes() {
            if node.label == NodeLabel::Class
                && node.properties.name == target.name
                && node.properties.file_path == target.file_path
            {
                source_ids.push(node.id.clone());
            }
        }
    }

    for rel in graph.iter_relationships() {
        if source_ids.contains(&rel.source_id)
            && matches!(
                rel.rel_type,
                RelationshipType::HasMethod
                    | RelationshipType::HasProperty
                    | RelationshipType::HasAction
            )
        {
            seed_ids.push(rel.target_id.clone());
        }
    }

    seed_ids
}

fn build_impact_adjacency(
    graph: &KnowledgeGraph,
) -> (HashMap<String, Vec<String>>, HashMap<String, Vec<String>>) {
    let mut forward: HashMap<String, Vec<String>> = HashMap::new();
    let mut reverse: HashMap<String, Vec<String>> = HashMap::new();

    for rel in graph.iter_relationships() {
        if !is_impact_traversal_edge(rel.rel_type) {
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

    (forward, reverse)
}

fn bfs_impact(
    graph: &KnowledgeGraph,
    seed_ids: &[String],
    adjacency: &HashMap<String, Vec<String>>,
    max_depth: usize,
) -> ImpactDirectionResult {
    let mut visited: HashSet<String> = HashSet::new();
    let mut queue: VecDeque<(String, usize)> = VecDeque::new();
    let mut nodes = Vec::new();
    let mut depth_counts = vec![0; max_depth];

    for seed in seed_ids {
        if visited.insert(seed.clone()) {
            queue.push_back((seed.clone(), 0));
        }
    }

    while let Some((node_id, depth)) = queue.pop_front() {
        if depth >= max_depth {
            continue;
        }
        let Some(neighbors) = adjacency.get(&node_id) else {
            continue;
        };
        for neighbor in neighbors {
            if visited.contains(neighbor) {
                continue;
            }
            if let Some(node) = graph.get_node(neighbor) {
                if node.label == NodeLabel::Community
                    || node.properties.file_path.contains("/obj/")
                    || node.properties.file_path.contains("\\obj\\")
                {
                    continue;
                }
            }

            visited.insert(neighbor.clone());
            let result_depth = depth + 1;
            nodes.push(ImpactNode {
                id: neighbor.clone(),
                depth: result_depth,
            });
            if let Some(count) = depth_counts.get_mut(result_depth - 1) {
                *count += 1;
            }
            queue.push_back((neighbor.clone(), result_depth));
        }
    }

    ImpactDirectionResult {
        nodes,
        depth_counts,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::types::{GraphNode, GraphRelationship, NodeProperties};

    fn node(id: &str, label: NodeLabel, name: &str, file_path: &str) -> GraphNode {
        GraphNode {
            id: id.to_string(),
            label,
            properties: NodeProperties {
                name: name.to_string(),
                file_path: file_path.to_string(),
                ..Default::default()
            },
        }
    }

    fn rel(
        id: &str,
        source_id: &str,
        target_id: &str,
        rel_type: RelationshipType,
    ) -> GraphRelationship {
        GraphRelationship {
            id: id.to_string(),
            source_id: source_id.to_string(),
            target_id: target_id.to_string(),
            rel_type,
            confidence: 1.0,
            reason: "test".to_string(),
            step: None,
        }
    }

    #[test]
    fn impact_walks_membership_edges_in_both_directions() {
        let mut graph = KnowledgeGraph::new();
        graph.add_node(node(
            "Function:src/caller.ts:start",
            NodeLabel::Function,
            "start",
            "src/caller.ts",
        ));
        graph.add_node(node(
            "Class:src/agent.ts:Agent",
            NodeLabel::Class,
            "Agent",
            "src/agent.ts",
        ));
        graph.add_node(node(
            "Method:src/agent.ts:executePlan",
            NodeLabel::Method,
            "executePlan",
            "src/agent.ts",
        ));
        graph.add_node(node(
            "Class:src/planner.ts:Planner",
            NodeLabel::Class,
            "Planner",
            "src/planner.ts",
        ));
        graph.add_node(node(
            "Method:src/planner.ts:createPlan",
            NodeLabel::Method,
            "createPlan",
            "src/planner.ts",
        ));
        graph.add_relationship(rel(
            "r_agent_method",
            "Class:src/agent.ts:Agent",
            "Method:src/agent.ts:executePlan",
            RelationshipType::HasMethod,
        ));
        graph.add_relationship(rel(
            "r_caller_agent",
            "Function:src/caller.ts:start",
            "Class:src/agent.ts:Agent",
            RelationshipType::Calls,
        ));
        graph.add_relationship(rel(
            "r_method_planner",
            "Method:src/agent.ts:executePlan",
            "Class:src/planner.ts:Planner",
            RelationshipType::Calls,
        ));
        graph.add_relationship(rel(
            "r_planner_method",
            "Class:src/planner.ts:Planner",
            "Method:src/planner.ts:createPlan",
            RelationshipType::HasMethod,
        ));

        let impact = analyze_impact(&graph, "executePlan", 5).unwrap();
        let upstream_ids: Vec<_> = impact
            .upstream
            .nodes
            .iter()
            .map(|node| node.id.as_str())
            .collect();
        let downstream_ids: Vec<_> = impact
            .downstream
            .nodes
            .iter()
            .map(|node| node.id.as_str())
            .collect();

        assert!(upstream_ids.contains(&"Class:src/agent.ts:Agent"));
        assert!(upstream_ids.contains(&"Function:src/caller.ts:start"));
        assert!(downstream_ids.contains(&"Class:src/planner.ts:Planner"));
        assert!(downstream_ids.contains(&"Method:src/planner.ts:createPlan"));
        assert_eq!(impact.upstream.total(), 2);
        assert_eq!(impact.downstream.total(), 2);
    }

    #[test]
    fn class_targets_seed_member_methods() {
        let mut graph = KnowledgeGraph::new();
        graph.add_node(node(
            "Class:src/service.ts:Service",
            NodeLabel::Class,
            "Service",
            "src/service.ts",
        ));
        graph.add_node(node(
            "Method:src/service.ts:run",
            NodeLabel::Method,
            "run",
            "src/service.ts",
        ));
        graph.add_node(node(
            "Function:src/helper.ts:helper",
            NodeLabel::Function,
            "helper",
            "src/helper.ts",
        ));
        graph.add_relationship(rel(
            "r_has_method",
            "Class:src/service.ts:Service",
            "Method:src/service.ts:run",
            RelationshipType::HasMethod,
        ));
        graph.add_relationship(rel(
            "r_run_helper",
            "Method:src/service.ts:run",
            "Function:src/helper.ts:helper",
            RelationshipType::Calls,
        ));

        let impact = analyze_impact(&graph, "Service", 5).unwrap();
        assert_eq!(impact.seed_ids.len(), 2);
        assert_eq!(impact.downstream.total(), 1);
        assert_eq!(
            impact.downstream.nodes[0].id,
            "Function:src/helper.ts:helper"
        );
    }
}

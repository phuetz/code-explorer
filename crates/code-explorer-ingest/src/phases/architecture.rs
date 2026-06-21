//! Architecture analysis phase: circular dependency detection and layer violation checking.

use std::collections::{HashMap, HashSet};

use code_explorer_core::graph::types::{NodeLabel, RelationshipType};
use code_explorer_core::graph::KnowledgeGraph;

/// A circular dependency cycle between files or modules.
#[derive(Debug, Clone)]
pub struct CircularDependency {
    /// The files/modules forming the cycle, in order.
    pub cycle: Vec<String>,
}

/// A layer violation: a component in a higher layer directly calls a lower layer
/// without going through the intermediate layer.
#[derive(Debug, Clone)]
pub struct LayerViolation {
    /// Source node (e.g., Controller method)
    pub source_name: String,
    pub source_file: String,
    pub source_layer: &'static str,
    /// Target node (e.g., DAL method)
    pub target_name: String,
    pub target_file: String,
    pub target_layer: &'static str,
}

/// Result of architecture analysis.
#[derive(Debug, Default)]
pub struct ArchitectureResult {
    pub circular_deps: Vec<CircularDependency>,
    pub layer_violations: Vec<LayerViolation>,
}

/// Detect circular dependencies at file level using DFS.
///
/// Looks at Imports/DependsOn edges between File nodes and reports cycles.
pub fn detect_circular_dependencies(graph: &KnowledgeGraph) -> Vec<CircularDependency> {
    // Build adjacency list: file_path -> set of imported file_paths
    let mut adj: HashMap<String, HashSet<String>> = HashMap::new();

    for rel in graph.iter_relationships() {
        if matches!(
            rel.rel_type,
            RelationshipType::Imports | RelationshipType::DependsOn
        ) {
            if let (Some(src), Some(dst)) = (
                graph.get_node(&rel.source_id),
                graph.get_node(&rel.target_id),
            ) {
                if src.label == NodeLabel::File && dst.label == NodeLabel::File {
                    adj.entry(src.properties.file_path.clone())
                        .or_default()
                        .insert(dst.properties.file_path.clone());
                }
            }
        }
    }

    let mut cycles = Vec::new();
    let mut visited = HashSet::new();
    let mut rec_stack = HashSet::new();
    let mut path = Vec::new();

    for file in adj.keys() {
        if !visited.contains(file) {
            dfs_find_cycles(
                file,
                &adj,
                &mut visited,
                &mut rec_stack,
                &mut path,
                &mut cycles,
                0,
            );
        }
    }

    // Deduplicate cycles (same cycle can be found from different start nodes)
    let mut seen_cycle_keys: HashSet<String> = HashSet::new();
    cycles.retain(|c| {
        let mut sorted = c.cycle.clone();
        sorted.sort();
        let key = sorted.join("|");
        seen_cycle_keys.insert(key)
    });

    // Limit to first 20 cycles
    cycles.truncate(20);
    cycles
}

const MAX_DFS_DEPTH: usize = 100;

fn dfs_find_cycles(
    node: &str,
    adj: &HashMap<String, HashSet<String>>,
    visited: &mut HashSet<String>,
    rec_stack: &mut HashSet<String>,
    path: &mut Vec<String>,
    cycles: &mut Vec<CircularDependency>,
    depth: usize,
) {
    if depth > MAX_DFS_DEPTH {
        return;
    }

    visited.insert(node.to_string());
    rec_stack.insert(node.to_string());
    path.push(node.to_string());

    if let Some(neighbors) = adj.get(node) {
        for neighbor in neighbors {
            if !visited.contains(neighbor.as_str()) {
                dfs_find_cycles(neighbor, adj, visited, rec_stack, path, cycles, depth + 1);
            } else if rec_stack.contains(neighbor.as_str()) {
                // Found a cycle — extract it from path
                if let Some(start_idx) = path.iter().position(|p| p == neighbor) {
                    let cycle_path: Vec<String> = path[start_idx..].to_vec();
                    if cycle_path.len() >= 2 {
                        cycles.push(CircularDependency { cycle: cycle_path });
                    }
                }
            }
        }
    }

    path.pop();
    rec_stack.remove(node);
}

/// Classify a node into an architectural layer based on its label and path.
fn classify_layer(label: NodeLabel, file_path: &str) -> Option<&'static str> {
    let lower = file_path.to_lowercase();

    match label {
        NodeLabel::Controller | NodeLabel::ControllerAction | NodeLabel::ApiEndpoint => {
            Some("Presentation")
        }
        NodeLabel::View => Some("Presentation"),
        NodeLabel::Service => Some("Business"),
        NodeLabel::Repository => Some("Data"),
        NodeLabel::DbContext | NodeLabel::DbEntity => Some("Data"),
        _ => {
            // Heuristic: classify by path
            if lower.contains("controller") || lower.contains("/views/") {
                Some("Presentation")
            } else if lower.contains("service")
                || lower.contains("/bal/")
                || lower.contains("/bll/")
            {
                Some("Business")
            } else if lower.contains("repository")
                || lower.contains("/dal/")
                || lower.contains("entities")
                || lower.contains("dbcontext")
            {
                Some("Data")
            } else {
                None
            }
        }
    }
}

/// Layer ordering: lower number = higher layer (should not call higher-numbered layers directly).
fn layer_order(layer: &str) -> u8 {
    match layer {
        "Presentation" => 0,
        "Business" => 1,
        "Data" => 2,
        _ => 99,
    }
}

/// Detect layer violations: Presentation calling Data directly (skipping Business).
pub fn detect_layer_violations(graph: &KnowledgeGraph) -> Vec<LayerViolation> {
    let mut violations = Vec::new();

    // Build node layer classification
    let mut node_layers: HashMap<String, &'static str> = HashMap::new();
    for node in graph.iter_nodes() {
        if let Some(layer) = classify_layer(node.label, &node.properties.file_path) {
            node_layers.insert(node.id.clone(), layer);
        }
    }

    // Check Calls edges for layer skipping
    for rel in graph.iter_relationships() {
        if !matches!(
            rel.rel_type,
            RelationshipType::Calls | RelationshipType::CallsService
        ) {
            continue;
        }

        let src_layer = match node_layers.get(&rel.source_id) {
            Some(l) => l,
            None => continue,
        };
        let dst_layer = match node_layers.get(&rel.target_id) {
            Some(l) => l,
            None => continue,
        };

        let src_order = layer_order(src_layer);
        let dst_order = layer_order(dst_layer);

        // Violation: skipping a layer (Presentation→Data = order difference > 1)
        if dst_order > src_order + 1 {
            if let (Some(src_node), Some(dst_node)) = (
                graph.get_node(&rel.source_id),
                graph.get_node(&rel.target_id),
            ) {
                violations.push(LayerViolation {
                    source_name: src_node.properties.name.clone(),
                    source_file: src_node.properties.file_path.clone(),
                    source_layer: src_layer,
                    target_name: dst_node.properties.name.clone(),
                    target_file: dst_node.properties.file_path.clone(),
                    target_layer: dst_layer,
                });
            }
        }
    }

    // Limit to first 50 violations
    violations.truncate(50);
    violations
}

/// Run full architecture analysis.
pub fn analyze_architecture(graph: &KnowledgeGraph) -> ArchitectureResult {
    ArchitectureResult {
        circular_deps: detect_circular_dependencies(graph),
        layer_violations: detect_layer_violations(graph),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_layer_classification() {
        assert_eq!(
            classify_layer(NodeLabel::Controller, "Controllers/HomeController.cs"),
            Some("Presentation")
        );
        assert_eq!(
            classify_layer(NodeLabel::Service, "Services/UserService.cs"),
            Some("Business")
        );
        assert_eq!(
            classify_layer(NodeLabel::DbContext, "Data/AppDbContext.cs"),
            Some("Data")
        );
        assert_eq!(classify_layer(NodeLabel::Function, "Utils/Helper.cs"), None);
    }

    #[test]
    fn test_layer_order() {
        assert!(layer_order("Presentation") < layer_order("Business"));
        assert!(layer_order("Business") < layer_order("Data"));
    }
}

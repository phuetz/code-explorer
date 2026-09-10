//! Circular dependency detection via Tarjan's strongly connected components.
//!
//! Scope:
//! - **Imports** — SCC on the File→File subgraph induced by `IMPORTS` edges.
//! - **Calls**   — SCC on the Method/Function→Method/Function subgraph induced
//!   by `CALLS` edges.
//!
//! A cycle is any SCC of size ≥ 2, plus any self-loop (SCC of size 1 that
//! contains a self-edge — rare but technically a cycle).

use std::collections::HashMap;

use code_explorer_core::graph::types::{NodeLabel, RelationshipType};
use code_explorer_core::graph::KnowledgeGraph;
use serde::{Deserialize, Serialize};

/// Scope of cycle detection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CycleScope {
    /// File→File via IMPORTS edges.
    Imports,
    /// Method/Function→Method/Function via CALLS edges.
    Calls,
}

impl CycleScope {
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "imports" | "import" => Some(Self::Imports),
            "calls" | "call" => Some(Self::Calls),
            _ => None,
        }
    }
}

/// A single cycle: an ordered list of node IDs forming the loop.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Cycle {
    /// Node IDs in the cycle (order is one valid traversal; not canonical).
    pub nodes: Vec<String>,
    /// Human-readable names for the nodes (parallel to `nodes`).
    pub names: Vec<String>,
    /// File paths for the nodes (parallel to `nodes`).
    pub file_paths: Vec<String>,
    /// Number of nodes in the cycle.
    pub length: usize,
    /// Coarse severity: "low" (≤3), "medium" (≤6), "high" (>6).
    pub severity: String,
}

fn severity_for(len: usize) -> &'static str {
    match len {
        0..=3 => "low",
        4..=6 => "medium",
        _ => "high",
    }
}

/// Detect cycles in the requested scope.
pub fn find_cycles(graph: &KnowledgeGraph, scope: CycleScope) -> Vec<Cycle> {
    // 1. Build an adjacency list restricted to the scope.
    let (adj, node_info) = build_scoped_adjacency(graph, scope);

    if adj.is_empty() {
        return Vec::new();
    }

    // 2. Run Tarjan's SCC.
    let sccs = tarjan_scc(&adj);

    // 3. Keep only SCCs that are actual cycles (size ≥ 2, or size 1 with self-loop).
    let mut cycles: Vec<Cycle> = Vec::new();
    for scc in sccs {
        let is_cycle = scc.len() >= 2
            || (scc.len() == 1
                && adj
                    .get(&scc[0])
                    .map(|out| out.iter().any(|t| t == &scc[0]))
                    .unwrap_or(false));
        if !is_cycle {
            continue;
        }

        let len = scc.len();
        let (names, file_paths): (Vec<String>, Vec<String>) = scc
            .iter()
            .map(|id| {
                let info = node_info.get(id);
                (
                    info.map(|i| i.0.clone()).unwrap_or_default(),
                    info.map(|i| i.1.clone()).unwrap_or_default(),
                )
            })
            .unzip();

        cycles.push(Cycle {
            nodes: scc,
            names,
            file_paths,
            length: len,
            severity: severity_for(len).to_string(),
        });
    }

    // Sort: longest cycles first, then by first node ID for determinism.
    cycles.sort_by(|a, b| b.length.cmp(&a.length).then_with(|| a.nodes.cmp(&b.nodes)));
    cycles
}

/// Adjacency list keyed by node id.
type Adjacency = HashMap<String, Vec<String>>;
/// Per-node `(name, file_path)` table keyed by node id.
type NodeInfo = HashMap<String, (String, String)>;

/// Build an adjacency list and a per-node (name, file_path) table.
fn build_scoped_adjacency(graph: &KnowledgeGraph, scope: CycleScope) -> (Adjacency, NodeInfo) {
    let mut adj: HashMap<String, Vec<String>> = HashMap::new();
    let mut info: HashMap<String, (String, String)> = HashMap::new();

    let keep_node = |n: &code_explorer_core::graph::types::GraphNode| match scope {
        CycleScope::Imports => n.label == NodeLabel::File,
        CycleScope::Calls => matches!(
            n.label,
            NodeLabel::Method | NodeLabel::Function | NodeLabel::Constructor
        ),
    };

    let keep_rel = |rt: RelationshipType| match scope {
        CycleScope::Imports => rt == RelationshipType::Imports,
        CycleScope::Calls => rt == RelationshipType::Calls,
    };

    for node in graph.iter_nodes() {
        if keep_node(node) {
            info.insert(
                node.id.clone(),
                (
                    node.properties.name.clone(),
                    node.properties.file_path.clone(),
                ),
            );
            adj.entry(node.id.clone()).or_default();
        }
    }

    for rel in graph.iter_relationships() {
        if !keep_rel(rel.rel_type) {
            continue;
        }
        if !info.contains_key(&rel.source_id) || !info.contains_key(&rel.target_id) {
            continue;
        }
        adj.entry(rel.source_id.clone())
            .or_default()
            .push(rel.target_id.clone());
    }

    (adj, info)
}

/// Iterative Tarjan's strongly connected components.
///
/// The standard textbook formulation is recursive, but Rust's default stack
/// blows up on codebases with deep call/import chains (observed OOM on
/// ~30k node graphs). Keep an explicit work stack instead.
fn tarjan_scc(adj: &HashMap<String, Vec<String>>) -> Vec<Vec<String>> {
    let mut state = TarjanState {
        index_counter: 0,
        stack: Vec::new(),
        on_stack: HashMap::new(),
        indices: HashMap::new(),
        lowlinks: HashMap::new(),
        result: Vec::new(),
    };

    // Deterministic traversal order by sorting node keys up front.
    let mut nodes: Vec<&String> = adj.keys().collect();
    nodes.sort();

    for v in nodes {
        if state.indices.contains_key(v) {
            continue;
        }
        strongconnect(v, adj, &mut state);
    }

    state.result
}

fn strongconnect(root: &str, adj: &HashMap<String, Vec<String>>, state: &mut TarjanState) {
    // Iterative DFS with explicit work items.
    enum Work {
        Enter(String),
        Resume(String),
    }

    let mut work: Vec<Work> = vec![Work::Enter(root.to_string())];

    while let Some(item) = work.pop() {
        match item {
            Work::Enter(v) => {
                state.indices.insert(v.clone(), state.index_counter);
                state.lowlinks.insert(v.clone(), state.index_counter);
                state.index_counter += 1;
                state.stack.push(v.clone());
                state.on_stack.insert(v.clone(), true);

                let neighbors = adj.get(&v).cloned().unwrap_or_default();
                // Sort neighbors for determinism.
                let mut neighbors = neighbors;
                neighbors.sort();

                // Schedule Resume after all children are processed.
                work.push(Work::Resume(v.clone()));
                for neighbor in neighbors.into_iter().rev() {
                    if !state.indices.contains_key(&neighbor) {
                        work.push(Work::Enter(neighbor));
                    } else if *state.on_stack.get(&neighbor).unwrap_or(&false) {
                        // Back-edge: update lowlink immediately.
                        let w_index = *state.indices.get(&neighbor).unwrap();
                        let v_low = *state.lowlinks.get(&v).unwrap();
                        state.lowlinks.insert(v.clone(), v_low.min(w_index));
                    }
                }
            }
            Work::Resume(v) => {
                // Propagate lowlinks from children that finished.
                let neighbors = adj.get(&v).cloned().unwrap_or_default();
                for w in &neighbors {
                    if state.on_stack.get(w).copied().unwrap_or(false) {
                        let w_low = *state.lowlinks.get(w).unwrap_or(&usize::MAX);
                        let v_low = *state.lowlinks.get(&v).unwrap();
                        state.lowlinks.insert(v.clone(), v_low.min(w_low));
                    }
                }

                // If v is a root of an SCC, pop the stack.
                if state.lowlinks.get(&v) == state.indices.get(&v) {
                    let mut scc = Vec::new();
                    while let Some(w) = state.stack.pop() {
                        state.on_stack.insert(w.clone(), false);
                        let is_root = w == v;
                        scc.push(w);
                        if is_root {
                            break;
                        }
                    }
                    state.result.push(scc);
                }
            }
        }
    }
}

struct TarjanState {
    index_counter: usize,
    stack: Vec<String>,
    on_stack: HashMap<String, bool>,
    indices: HashMap<String, usize>,
    lowlinks: HashMap<String, usize>,
    result: Vec<Vec<String>>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use code_explorer_core::graph::types::{GraphNode, GraphRelationship, NodeProperties};

    fn make_file(path: &str) -> GraphNode {
        GraphNode {
            id: format!("File:{path}"),
            label: NodeLabel::File,
            properties: NodeProperties {
                name: path.to_string(),
                file_path: path.to_string(),
                ..Default::default()
            },
        }
    }

    fn make_imports(src: &str, tgt: &str) -> GraphRelationship {
        GraphRelationship {
            id: format!("imp_{src}_{tgt}"),
            source_id: format!("File:{src}"),
            target_id: format!("File:{tgt}"),
            rel_type: RelationshipType::Imports,
            confidence: 1.0,
            reason: "test".to_string(),
            step: None,
        }
    }

    #[test]
    fn test_find_cycles_no_cycle() {
        let mut g = KnowledgeGraph::new();
        g.add_node(make_file("a.ts"));
        g.add_node(make_file("b.ts"));
        g.add_relationship(make_imports("a.ts", "b.ts"));
        let cycles = find_cycles(&g, CycleScope::Imports);
        assert_eq!(cycles.len(), 0);
    }

    #[test]
    fn test_find_cycles_simple_loop() {
        let mut g = KnowledgeGraph::new();
        g.add_node(make_file("a.ts"));
        g.add_node(make_file("b.ts"));
        g.add_relationship(make_imports("a.ts", "b.ts"));
        g.add_relationship(make_imports("b.ts", "a.ts"));
        let cycles = find_cycles(&g, CycleScope::Imports);
        assert_eq!(cycles.len(), 1);
        assert_eq!(cycles[0].length, 2);
        assert_eq!(cycles[0].severity, "low");
    }

    #[test]
    fn test_find_cycles_three_node() {
        let mut g = KnowledgeGraph::new();
        g.add_node(make_file("a.ts"));
        g.add_node(make_file("b.ts"));
        g.add_node(make_file("c.ts"));
        g.add_relationship(make_imports("a.ts", "b.ts"));
        g.add_relationship(make_imports("b.ts", "c.ts"));
        g.add_relationship(make_imports("c.ts", "a.ts"));
        let cycles = find_cycles(&g, CycleScope::Imports);
        assert_eq!(cycles.len(), 1);
        assert_eq!(cycles[0].length, 3);
    }

    #[test]
    fn test_cycle_scope_parse() {
        assert_eq!(CycleScope::parse("imports"), Some(CycleScope::Imports));
        assert_eq!(CycleScope::parse("Imports"), Some(CycleScope::Imports));
        assert_eq!(CycleScope::parse("calls"), Some(CycleScope::Calls));
        assert_eq!(CycleScope::parse("junk"), None);
    }

    #[test]
    fn test_empty_graph() {
        let g = KnowledgeGraph::new();
        assert!(find_cycles(&g, CycleScope::Imports).is_empty());
        assert!(find_cycles(&g, CycleScope::Calls).is_empty());
    }
}

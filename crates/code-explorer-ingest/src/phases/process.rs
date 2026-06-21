use std::collections::{HashMap, HashSet, VecDeque};

use code_explorer_core::graph::types::*;
use code_explorer_core::graph::KnowledgeGraph;
use code_explorer_core::id::generate_id;

/// Maximum depth for BFS trace from entry points.
const MAX_DEPTH: usize = 10;
/// Maximum branching factor per node during BFS.
const MAX_BRANCHING: usize = 4;
/// Minimum steps required for a valid process.
const MIN_STEPS: usize = 3;
/// Maximum number of processes to keep.
const MAX_PROCESSES: usize = 75;

/// Entry point name patterns that boost scoring.
const ENTRY_POINT_PATTERNS: &[&str] = &[
    "handle",
    "process",
    "execute",
    "run",
    "start",
    "init",
    "main",
    "serve",
    "listen",
    "route",
    "dispatch",
    "on_",
    "controller",
    "endpoint",
    "api",
];

/// Detect execution flow processes by tracing from entry points.
///
/// Steps:
/// 1. Build caller/callee adjacency from CALLS edges
/// 2. Score nodes to find entry points (exported, few callers, many callees)
/// 3. BFS trace from each entry point (maxDepth=10, maxBranching=4)
/// 4. Deduplicate and keep top 75 processes
/// 5. Create Process nodes and STEP_IN_PROCESS edges
pub fn detect_processes(graph: &mut KnowledgeGraph) -> Result<usize, crate::IngestError> {
    // Build raw adjacency lists from CALLS edges
    let (raw_callees, _raw_callers) = build_call_adjacency(graph);

    if raw_callees.is_empty() {
        tracing::info!("No CALLS edges found, skipping process detection");
        return Ok(0);
    }

    // Build a Function→Function call graph.
    // Raw CALLS often go File→Function (since source_id = file_node_id in extraction).
    // We need to infer Function→Function by: if File A calls Function X,
    // and File A defines Function Y, then Y→X within that file.
    let (func_callees, func_callers) = build_function_call_graph(graph, &raw_callees);

    tracing::debug!(
        "Function call graph: {} sources with callees, {} targets with callers",
        func_callees.len(),
        func_callers.len()
    );

    if func_callees.is_empty() {
        return Ok(0);
    }

    // Find and score entry points
    let mut entry_points = find_entry_points(graph, &func_callees, &func_callers);
    tracing::debug!("Found {} entry point candidates", entry_points.len());
    entry_points.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    entry_points.truncate(200);

    // Trace processes from each entry point using the function-level graph
    let mut traces: Vec<ProcessTrace> = Vec::new();
    for ep in &entry_points {
        let new_traces = bfs_trace(&ep.node_id, &func_callees);
        traces.extend(new_traces);
    }
    tracing::debug!("BFS produced {} raw traces", traces.len());

    // Deduplicate: remove subset traces, keep longest per entry->terminal pair
    traces = deduplicate_traces(traces);

    // Limit to top MAX_PROCESSES
    traces.truncate(MAX_PROCESSES);

    // Build community membership lookup for classification
    let community_membership = build_community_membership(graph);

    let process_count = traces.len();

    // Create Process nodes and STEP_IN_PROCESS edges
    for (idx, trace) in traces.iter().enumerate() {
        let process_name = generate_process_name(graph, trace);
        let process_id = generate_id("Process", &format!("process_{idx}"));

        // Classify as intra_community or cross_community
        let process_type = classify_process(trace, &community_membership);

        // Collect communities involved
        let communities: Vec<String> = {
            let mut community_ids: HashSet<String> = HashSet::new();
            for step_id in &trace.steps {
                if let Some(cids) = community_membership.get(step_id.as_str()) {
                    community_ids.extend(cids.iter().cloned());
                }
            }
            let mut sorted: Vec<String> = community_ids.into_iter().collect();
            sorted.sort();
            sorted
        };

        let entry_point_id = trace.steps.first().cloned();
        let terminal_id = trace.steps.last().cloned();

        let process_node = GraphNode {
            id: process_id.clone(),
            label: NodeLabel::Process,
            properties: NodeProperties {
                name: process_name,
                file_path: String::new(),
                process_type: Some(process_type),
                step_count: Some(trace.steps.len() as u32),
                communities: if communities.is_empty() {
                    None
                } else {
                    Some(communities)
                },
                entry_point_id,
                terminal_id,
                enriched_by: Some(EnrichedBy::Heuristic),
                ..Default::default()
            },
        };
        graph.add_node(process_node);

        // Create STEP_IN_PROCESS edges
        for (step_idx, step_id) in trace.steps.iter().enumerate() {
            let edge_id = format!("step_{}_{}", process_id, step_idx);
            graph.add_relationship(GraphRelationship {
                id: edge_id,
                source_id: step_id.clone(),
                target_id: process_id.clone(),
                rel_type: RelationshipType::StepInProcess,
                confidence: 1.0,
                reason: "process-detection".to_string(),
                step: Some((step_idx + 1) as u32), // 1-indexed
            });
        }
    }

    Ok(process_count)
}

/// Adjacency lists for CALLS edges.
fn build_call_adjacency(
    graph: &KnowledgeGraph,
) -> (HashMap<String, Vec<String>>, HashMap<String, Vec<String>>) {
    let mut callees_of: HashMap<String, Vec<String>> = HashMap::new();
    let mut callers_of: HashMap<String, Vec<String>> = HashMap::new();

    graph.for_each_relationship(|rel| {
        if rel.rel_type == RelationshipType::Calls {
            callees_of
                .entry(rel.source_id.clone())
                .or_default()
                .push(rel.target_id.clone());
            callers_of
                .entry(rel.target_id.clone())
                .or_default()
                .push(rel.source_id.clone());
        }
    });

    (callees_of, callers_of)
}

/// An entry point candidate with a score.
struct EntryPointCandidate {
    node_id: String,
    score: f64,
}

/// Build a Function→Function call graph from raw (possibly File→Function) CALLS edges.
fn build_function_call_graph(
    graph: &KnowledgeGraph,
    raw_callees: &HashMap<String, Vec<String>>,
) -> (HashMap<String, Vec<String>>, HashMap<String, Vec<String>>) {
    let mut func_callees: HashMap<String, Vec<String>> = HashMap::new();
    let mut func_callers: HashMap<String, Vec<String>> = HashMap::new();

    for (source_id, targets) in raw_callees {
        let source_node = graph.get_node(source_id);
        let is_file = source_node.is_some_and(|n| n.label == NodeLabel::File);

        if is_file {
            // Find functions defined in this file (via DEFINES edges)
            let file_functions: Vec<String> = graph
                .iter_relationships()
                .filter(|r| r.rel_type == RelationshipType::Defines && r.source_id == *source_id)
                .filter(|r| {
                    graph.get_node(&r.target_id).is_some_and(|n| {
                        matches!(
                            n.label,
                            NodeLabel::Function | NodeLabel::Method | NodeLabel::Constructor
                        )
                    })
                })
                .map(|r| r.target_id.clone())
                .collect();

            for func_id in &file_functions {
                for target in targets {
                    let is_func_target = graph.get_node(target).is_some_and(|n| {
                        matches!(
                            n.label,
                            NodeLabel::Function | NodeLabel::Method | NodeLabel::Constructor
                        )
                    });
                    if is_func_target && func_id != target {
                        func_callees
                            .entry(func_id.clone())
                            .or_default()
                            .push(target.clone());
                        func_callers
                            .entry(target.clone())
                            .or_default()
                            .push(func_id.clone());
                    }
                }
            }
        } else {
            // Direct Function→Function
            for target in targets {
                if source_id != target {
                    func_callees
                        .entry(source_id.clone())
                        .or_default()
                        .push(target.clone());
                    func_callers
                        .entry(target.clone())
                        .or_default()
                        .push(source_id.clone());
                }
            }
        }
    }

    // Deduplicate to avoid inflated scores from File→Function edge expansion
    for callees in func_callees.values_mut() {
        callees.sort_unstable();
        callees.dedup();
    }
    for callers in func_callers.values_mut() {
        callers.sort_unstable();
        callers.dedup();
    }

    (func_callees, func_callers)
}

/// Find entry points: function/method nodes with callees but few callers.
fn find_entry_points(
    graph: &KnowledgeGraph,
    func_callees: &HashMap<String, Vec<String>>,
    func_callers: &HashMap<String, Vec<String>>,
) -> Vec<EntryPointCandidate> {
    let mut candidates = Vec::new();

    for (node_id, callees) in func_callees {
        if callees.is_empty() {
            continue;
        }

        let node = match graph.get_node(node_id) {
            Some(n) => n,
            None => continue,
        };

        match node.label {
            NodeLabel::Function | NodeLabel::Method | NodeLabel::Constructor => {}
            _ => continue,
        }

        let callee_count = callees.len();
        let caller_count = func_callers.get(node_id).map_or(0, |c| c.len());

        let mut score = 0.0;

        // Boost for being exported
        if node.properties.is_exported == Some(true) {
            score += 3.0;
        }

        // Boost for zero callers (true entry point)
        if caller_count == 0 {
            score += 5.0;
        } else {
            score += 2.0 / (caller_count as f64);
        }

        // Boost for many callees (orchestrator)
        score += (callee_count as f64).min(5.0);

        // Boost for name patterns
        let name_lower = node.properties.name.to_lowercase();
        for pattern in ENTRY_POINT_PATTERNS {
            if name_lower.contains(pattern) {
                score += 2.0;
                break;
            }
        }

        // Boost from entry_point_score if available
        if let Some(ep_score) = node.properties.entry_point_score {
            score += ep_score;
        }

        candidates.push(EntryPointCandidate {
            node_id: node_id.to_string(),
            score,
        });
    }

    candidates
}

/// A traced execution flow.
#[derive(Clone)]
struct ProcessTrace {
    steps: Vec<String>,
}

/// BFS trace from an entry point, respecting depth and branching limits.
fn bfs_trace(start_id: &str, callees_of: &HashMap<String, Vec<String>>) -> Vec<ProcessTrace> {
    /// Hard cap on total traces per BFS to bound exponential blowup. Replaces
    /// the previous edge-dedup which silently dropped valid paths whenever a
    /// directed edge was reused in a different prefix within the same BFS.
    const MAX_TRACES: usize = 4096;
    /// Hard cap on the queue size to bound peak memory.
    const MAX_QUEUE: usize = 16384;

    let mut traces: Vec<ProcessTrace> = Vec::new();
    let mut queue: VecDeque<(Vec<String>, HashSet<String>, usize)> = VecDeque::new();

    let mut initial_visited = HashSet::new();
    initial_visited.insert(start_id.to_string());
    queue.push_back((vec![start_id.to_string()], initial_visited, 0));

    while let Some((path, visited, depth)) = queue.pop_front() {
        if traces.len() >= MAX_TRACES {
            break;
        }

        if depth >= MAX_DEPTH {
            if path.len() >= MIN_STEPS {
                traces.push(ProcessTrace { steps: path });
            }
            continue;
        }

        let Some(current) = path.last() else {
            continue;
        };
        let callees = match callees_of.get(current.as_str()) {
            Some(c) => c,
            None => {
                // Terminal node
                if path.len() >= MIN_STEPS {
                    traces.push(ProcessTrace { steps: path });
                }
                continue;
            }
        };

        if callees.is_empty() {
            if path.len() >= MIN_STEPS {
                traces.push(ProcessTrace { steps: path });
            }
            continue;
        }

        // Limit branching
        let branch_limit = callees.len().min(MAX_BRANCHING);
        let mut extended = false;

        for callee in callees.iter().take(branch_limit) {
            // Avoid cycles in the path (O(1) lookup)
            if visited.contains(callee) {
                continue;
            }

            if queue.len() >= MAX_QUEUE {
                break;
            }

            let mut new_path = path.clone();
            new_path.push(callee.clone());
            let mut new_visited = visited.clone();
            new_visited.insert(callee.clone());
            queue.push_back((new_path, new_visited, depth + 1));
            extended = true;
        }

        // If we couldn't extend (all callees in path already), this is a terminal
        if !extended && path.len() >= MIN_STEPS {
            traces.push(ProcessTrace { steps: path });
        }
    }

    traces
}

/// Deduplicate traces: remove subsets, keep longest per entry->terminal pair.
fn deduplicate_traces(mut traces: Vec<ProcessTrace>) -> Vec<ProcessTrace> {
    // Sort by length descending so longer traces take priority
    traces.sort_by(|a, b| b.steps.len().cmp(&a.steps.len()));

    let mut kept: Vec<ProcessTrace> = Vec::new();
    let mut seen_pairs: HashSet<(String, String)> = HashSet::new();

    for trace in traces {
        if trace.steps.is_empty() {
            continue;
        }

        // Safety: empty traces are filtered by the check on line 390
        let entry = trace.steps.first().expect("non-empty after filter").clone();
        let terminal = trace.steps.last().expect("non-empty after filter").clone();
        let pair = (entry, terminal);

        // Keep only the longest trace per entry->terminal pair
        if seen_pairs.contains(&pair) {
            continue;
        }

        // Check if this trace is a subset of an already-kept trace
        let is_subset = kept.iter().any(|existing| {
            trace.steps.len() < existing.steps.len()
                && trace.steps.iter().all(|step| existing.steps.contains(step))
        });

        // Only mark the pair as "seen" when we actually kept a trace for it.
        // Marking on subset-discard would block a later non-subset trace with
        // the same (entry, terminal) but a distinct intermediate path —
        // which is sort-order dependent and was discarding valid call graphs.
        if !is_subset {
            seen_pairs.insert(pair);
            kept.push(trace);
        }
    }

    kept
}

/// Build a map of node_id -> community IDs it belongs to.
fn build_community_membership(graph: &KnowledgeGraph) -> HashMap<String, Vec<String>> {
    let mut membership: HashMap<String, Vec<String>> = HashMap::new();

    graph.for_each_relationship(|rel| {
        if rel.rel_type == RelationshipType::MemberOf {
            membership
                .entry(rel.source_id.clone())
                .or_default()
                .push(rel.target_id.clone());
        }
    });

    membership
}

/// Classify a process as intra_community or cross_community.
fn classify_process(
    trace: &ProcessTrace,
    community_membership: &HashMap<String, Vec<String>>,
) -> ProcessType {
    let mut communities: HashSet<&str> = HashSet::new();

    for step_id in &trace.steps {
        if let Some(cids) = community_membership.get(step_id.as_str()) {
            for cid in cids {
                communities.insert(cid);
            }
        }
    }

    if communities.len() <= 1 {
        ProcessType::IntraCommunity
    } else {
        ProcessType::CrossCommunity
    }
}

/// Generate a descriptive name for a process.
fn generate_process_name(graph: &KnowledgeGraph, trace: &ProcessTrace) -> String {
    let entry_name = trace
        .steps
        .first()
        .and_then(|id| graph.get_node(id))
        .map(|n| n.properties.name.clone())
        .unwrap_or_else(|| "unknown".to_string());

    let terminal_name = trace
        .steps
        .last()
        .and_then(|id| graph.get_node(id))
        .map(|n| n.properties.name.clone())
        .unwrap_or_else(|| "unknown".to_string());

    if entry_name == terminal_name {
        entry_name
    } else {
        format!("{} -> {}", entry_name, terminal_name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_fn_node(id: &str, name: &str, file_path: &str, exported: bool) -> GraphNode {
        GraphNode {
            id: id.to_string(),
            label: NodeLabel::Function,
            properties: NodeProperties {
                name: name.to_string(),
                file_path: file_path.to_string(),
                is_exported: Some(exported),
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
    fn test_detect_processes_empty_graph() {
        let mut graph = KnowledgeGraph::new();
        let count = detect_processes(&mut graph).unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn test_detect_processes_linear_chain() {
        let mut graph = KnowledgeGraph::new();

        // handleRequest -> validateInput -> processData -> saveResult
        graph.add_node(make_fn_node("f1", "handleRequest", "api.ts", true));
        graph.add_node(make_fn_node("f2", "validateInput", "validate.ts", false));
        graph.add_node(make_fn_node("f3", "processData", "process.ts", false));
        graph.add_node(make_fn_node("f4", "saveResult", "db.ts", false));

        graph.add_relationship(make_calls_edge("f1", "f2"));
        graph.add_relationship(make_calls_edge("f2", "f3"));
        graph.add_relationship(make_calls_edge("f3", "f4"));

        let count = detect_processes(&mut graph).unwrap();

        // Should find at least one process
        assert!(count >= 1);

        // Verify Process node was created
        let process_nodes: Vec<_> = graph
            .nodes()
            .into_iter()
            .filter(|n| n.label == NodeLabel::Process)
            .collect();
        assert!(!process_nodes.is_empty());

        // Verify STEP_IN_PROCESS edges
        let step_edges: Vec<_> = graph
            .relationships()
            .into_iter()
            .filter(|r| r.rel_type == RelationshipType::StepInProcess)
            .collect();
        assert!(!step_edges.is_empty());

        // Steps should be 1-indexed
        for edge in &step_edges {
            assert!(edge.step.is_some());
            assert!(edge.step.unwrap() >= 1);
        }
    }

    #[test]
    fn test_detect_processes_too_short() {
        let mut graph = KnowledgeGraph::new();

        // Only 2-step chain (below MIN_STEPS=3)
        graph.add_node(make_fn_node("f1", "handleRequest", "api.ts", true));
        graph.add_node(make_fn_node("f2", "processData", "process.ts", false));

        graph.add_relationship(make_calls_edge("f1", "f2"));

        let count = detect_processes(&mut graph).unwrap();
        // Should not create any processes (too short)
        assert_eq!(count, 0);
    }

    #[test]
    fn test_classify_process_intra_community() {
        let trace = ProcessTrace {
            steps: vec!["a".into(), "b".into(), "c".into()],
        };

        // All in same community
        let mut membership: HashMap<String, Vec<String>> = HashMap::new();
        membership.insert("a".into(), vec!["comm1".into()]);
        membership.insert("b".into(), vec!["comm1".into()]);
        membership.insert("c".into(), vec!["comm1".into()]);

        assert_eq!(
            classify_process(&trace, &membership),
            ProcessType::IntraCommunity
        );
    }

    #[test]
    fn test_classify_process_cross_community() {
        let trace = ProcessTrace {
            steps: vec!["a".into(), "b".into(), "c".into()],
        };

        let mut membership: HashMap<String, Vec<String>> = HashMap::new();
        membership.insert("a".into(), vec!["comm1".into()]);
        membership.insert("b".into(), vec!["comm1".into()]);
        membership.insert("c".into(), vec!["comm2".into()]);

        assert_eq!(
            classify_process(&trace, &membership),
            ProcessType::CrossCommunity
        );
    }

    #[test]
    fn test_deduplicate_traces() {
        // Longer trace and its subset
        let traces = vec![
            ProcessTrace {
                steps: vec!["a".into(), "b".into(), "c".into(), "d".into()],
            },
            ProcessTrace {
                steps: vec!["a".into(), "b".into(), "c".into()],
            },
        ];

        let deduped = deduplicate_traces(traces);
        // Should keep only the longer trace (a->d), since a->c is a subset
        assert_eq!(deduped.len(), 1);
        assert_eq!(deduped[0].steps.len(), 4);
    }

    #[test]
    fn test_bfs_trace_cycle_protection() {
        let mut callees_of: HashMap<String, Vec<String>> = HashMap::new();
        // a -> b -> c -> a (cycle)
        callees_of.insert("a".into(), vec!["b".into()]);
        callees_of.insert("b".into(), vec!["c".into()]);
        callees_of.insert("c".into(), vec!["a".into()]);

        let traces = bfs_trace("a", &callees_of);
        // Should produce a trace without repeating nodes
        for trace in &traces {
            let unique: HashSet<&String> = trace.steps.iter().collect();
            assert_eq!(
                unique.len(),
                trace.steps.len(),
                "Trace should not contain cycles"
            );
        }
    }
}

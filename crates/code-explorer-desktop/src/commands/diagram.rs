use std::collections::{HashMap, HashSet, VecDeque};

use serde::Serialize;
use tauri::State;

use code_explorer_core::graph::types::{GraphNode, NodeLabel, RelationshipType};
use code_explorer_core::graph::KnowledgeGraph;
use code_explorer_db::inmemory::cypher::GraphIndexes;

use crate::state::AppState;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagramResult {
    pub mermaid: String,
    pub target_name: String,
    pub target_label: String,
    pub diagram_type: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DiagramKind {
    Flowchart,
    Sequence,
    Class,
}

#[tauri::command]
pub async fn get_diagram(
    state: State<'_, AppState>,
    target: String,
    diagram_type: Option<String>,
) -> Result<DiagramResult, String> {
    let (graph, indexes, _, _) = state.get_repo(None).await?;
    build_diagram(&graph, &indexes, &target, diagram_type.as_deref())
}

pub(crate) fn build_diagram(
    graph: &KnowledgeGraph,
    indexes: &GraphIndexes,
    target: &str,
    diagram_type: Option<&str>,
) -> Result<DiagramResult, String> {
    let kind = DiagramKind::parse(diagram_type);

    let target_lower = target.to_lowercase();
    let mut candidates: Vec<_> = graph
        .iter_nodes()
        .filter(|n| n.properties.name.to_lowercase() == target_lower)
        .collect();
    candidates.sort_by_key(|n| candidate_priority(n.label, kind));

    let start_node = candidates
        .first()
        .ok_or_else(|| format!("Symbol '{}' not found", target))?;
    let diagram_root_id = if kind == DiagramKind::Class {
        hierarchy_root_id(indexes, &start_node.id)
    } else {
        start_node.id.clone()
    };

    let mermaid = match kind {
        DiagramKind::Flowchart => build_flowchart(graph, indexes, &diagram_root_id),
        DiagramKind::Sequence => build_sequence_diagram(graph, indexes, &diagram_root_id),
        DiagramKind::Class => build_class_diagram(graph, indexes, &diagram_root_id),
    };

    let root_node = graph
        .get_node(&diagram_root_id)
        .ok_or_else(|| format!("Symbol '{}' not found", target))?;

    Ok(DiagramResult {
        mermaid,
        target_name: root_node.properties.name.clone(),
        target_label: root_node.label.as_str().to_string(),
        diagram_type: kind.as_str().to_string(),
    })
}

fn build_flowchart(graph: &KnowledgeGraph, indexes: &GraphIndexes, node_id: &str) -> String {
    let Some(start_node) = graph.get_node(node_id) else {
        return "graph TD".to_string();
    };

    let mut lines = vec!["graph TD".to_string()];
    lines.push(format!(
        "    {}[\"{}\"]",
        sanitize(node_id),
        escape_label(&start_node.properties.name)
    ));

    let outgoing = outgoing_edges(indexes, node_id);
    let methods = collect_methods(graph, indexes, node_id);

    for method_id in &methods {
        if let Some(method) = graph.get_node(method_id) {
            lines.push(format!(
                "    {} --> {}[\"{}\"]",
                sanitize(node_id),
                sanitize(method_id),
                escape_label(&method.properties.name),
            ));

            for (callee_id, rel_type) in outgoing_edges(indexes, method_id) {
                if !matches!(
                    rel_type,
                    RelationshipType::Calls
                        | RelationshipType::CallsAction
                        | RelationshipType::CallsService
                ) {
                    continue;
                }
                if let Some(callee) = graph.get_node(callee_id) {
                    lines.push(format!(
                        "    {} --> {}[\"{}\"]",
                        sanitize(method_id),
                        sanitize(callee_id),
                        escape_label(&callee.properties.name),
                    ));
                }
            }
        }
    }

    if methods.is_empty() {
        for (target_id, rel_type) in outgoing {
            if !matches!(
                rel_type,
                RelationshipType::Calls | RelationshipType::Imports | RelationshipType::DependsOn
            ) {
                continue;
            }
            if let Some(target_node) = graph.get_node(target_id) {
                lines.push(format!(
                    "    {} -->|{}| {}[\"{}\"]",
                    sanitize(node_id),
                    rel_type.as_str(),
                    sanitize(target_id),
                    escape_label(&target_node.properties.name),
                ));
            }
        }
    }

    lines.join("\n")
}

fn build_sequence_diagram(graph: &KnowledgeGraph, indexes: &GraphIndexes, node_id: &str) -> String {
    const MAX_DEPTH: usize = 3;

    let Some(start_node) = graph.get_node(node_id) else {
        return "sequenceDiagram".to_string();
    };

    let root_methods = collect_methods(graph, indexes, node_id);
    let mut participant_lines = Vec::new();
    let mut seen_participants = HashSet::new();
    let mut message_lines = Vec::new();
    let mut seen_messages = HashSet::new();
    let mut queue = VecDeque::new();
    let mut visited_depths = HashMap::new();

    for method_id in &root_methods {
        if let Some(method) = graph.get_node(method_id) {
            let (caller_id, caller_label) = participant_for_method(graph, indexes, method_id)
                .unwrap_or_else(|| (sanitize(node_id), start_node.properties.name.clone()));
            push_participant(
                &mut participant_lines,
                &mut seen_participants,
                &caller_id,
                &caller_label,
            );
            push_message(
                &mut message_lines,
                &mut seen_messages,
                &caller_id,
                &caller_id,
                &format!("{}()", method.properties.name),
            );
            visited_depths.insert(method_id.clone(), 0usize);
            queue.push_back((method_id.clone(), 0usize));
        }
    }

    while let Some((method_id, depth)) = queue.pop_front() {
        if depth >= MAX_DEPTH {
            continue;
        }

        let Some((source_participant_id, source_participant_label)) =
            participant_for_method(graph, indexes, &method_id)
        else {
            continue;
        };
        push_participant(
            &mut participant_lines,
            &mut seen_participants,
            &source_participant_id,
            &source_participant_label,
        );

        let mut callees: Vec<_> = outgoing_edges(indexes, &method_id)
            .iter()
            .filter(|(_, rel_type)| {
                matches!(
                    rel_type,
                    RelationshipType::Calls
                        | RelationshipType::CallsAction
                        | RelationshipType::CallsService
                )
            })
            .cloned()
            .collect();
        callees.sort_by_key(|(callee_id, _)| sortable_name(graph, callee_id));

        for (callee_id, _) in callees {
            let Some(callee) = graph.get_node(&callee_id) else {
                continue;
            };

            let (target_participant_id, target_participant_label) =
                participant_for_method(graph, indexes, &callee_id)
                    .unwrap_or_else(|| (sanitize(&callee_id), callee.properties.name.clone()));
            push_participant(
                &mut participant_lines,
                &mut seen_participants,
                &target_participant_id,
                &target_participant_label,
            );
            push_message(
                &mut message_lines,
                &mut seen_messages,
                &source_participant_id,
                &target_participant_id,
                &format!("{}()", callee.properties.name),
            );

            if is_method_like(callee.label) {
                let next_depth = depth + 1;
                let should_visit = visited_depths
                    .get(&callee_id)
                    .map(|current| next_depth < *current)
                    .unwrap_or(true);
                if should_visit {
                    visited_depths.insert(callee_id.clone(), next_depth);
                    queue.push_back((callee_id, next_depth));
                }
            }
        }
    }

    let mut lines = vec!["sequenceDiagram".to_string()];
    if participant_lines.is_empty() {
        let start_id = sanitize(node_id);
        lines.push(format!(
            "    participant {} as {}",
            start_id,
            escape_sequence_text(&start_node.properties.name)
        ));
        lines.push(format!(
            "    Note over {}: No method call chain found",
            start_id
        ));
        return lines.join("\n");
    }

    lines.extend(participant_lines);
    if message_lines.is_empty() {
        lines.push(format!(
            "    Note over {}: No method call chain found",
            sanitize(node_id)
        ));
    } else {
        lines.extend(message_lines);
    }
    lines.join("\n")
}

fn build_class_diagram(graph: &KnowledgeGraph, indexes: &GraphIndexes, node_id: &str) -> String {
    let Some(start_node) = graph.get_node(node_id) else {
        return "classDiagram".to_string();
    };

    let mut node_ids = HashSet::new();
    node_ids.insert(node_id.to_string());
    let mut hierarchy_edges = Vec::new();

    for (target_id, rel_type) in outgoing_edges(indexes, node_id) {
        if is_hierarchy_rel(*rel_type) {
            node_ids.insert(target_id.clone());
            hierarchy_edges.push((node_id.to_string(), target_id.clone(), *rel_type));
        }
    }

    for (source_id, rel_type) in incoming_edges(indexes, node_id) {
        if is_hierarchy_rel(*rel_type) {
            node_ids.insert(source_id.clone());
            hierarchy_edges.push((source_id.clone(), node_id.to_string(), *rel_type));
        }
    }

    let mut ordered_ids: Vec<_> = node_ids.into_iter().collect();
    ordered_ids.sort_by_key(|id| sortable_name(graph, id));

    let mut lines = vec!["classDiagram".to_string()];
    for current_id in ordered_ids {
        if let Some(node) = graph.get_node(&current_id) {
            lines.push(render_class_block(graph, node));
        }
    }

    if hierarchy_edges.is_empty() {
        lines.push(format!(
            "    %% No direct class hierarchy found for {}",
            escape_label(&start_node.properties.name)
        ));
        return lines.join("\n");
    }

    hierarchy_edges.sort_by_key(|(source_id, target_id, rel_type)| {
        (
            sortable_name(graph, source_id),
            sortable_name(graph, target_id),
            rel_type.as_str().to_string(),
        )
    });

    for (source_id, target_id, rel_type) in hierarchy_edges {
        let arrow = match rel_type {
            RelationshipType::Implements => "..|>",
            RelationshipType::Inherits | RelationshipType::Extends => "--|>",
            _ => "-->",
        };
        lines.push(format!(
            "    {} {} {}",
            class_id(graph, &source_id),
            arrow,
            class_id(graph, &target_id),
        ));
    }

    lines.join("\n")
}

fn hierarchy_root_id(indexes: &GraphIndexes, node_id: &str) -> String {
    incoming_edges(indexes, node_id)
        .iter()
        .find(|(_, rel_type)| {
            matches!(
                rel_type,
                RelationshipType::HasMethod | RelationshipType::HasAction
            )
        })
        .map(|(owner_id, _)| owner_id.clone())
        .unwrap_or_else(|| node_id.to_string())
}

fn sanitize(id: &str) -> String {
    let mut out = id.replace(
        [':', '/', '.', ' ', '<', '>', '(', ')', '{', '}', '-', ','],
        "_",
    );
    if out.is_empty()
        || !out
            .chars()
            .next()
            .map(|c| c.is_ascii_alphabetic() || c == '_')
            .unwrap_or(false)
    {
        out.insert_str(0, "n_");
    }
    out
}

/// Escape a string for inclusion inside a mermaid `["..."]` label.
/// Mermaid does not understand `\"`, so we replace problematic characters with
/// HTML entities (which mermaid renders correctly inside quoted labels).
fn escape_label(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('"', "&quot;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('[', "&#91;")
        .replace(']', "&#93;")
        .replace('(', "&#40;")
        .replace(')', "&#41;")
        .replace('`', "&#96;")
        .replace('{', "&#123;")
        .replace('}', "&#125;")
}

fn escape_sequence_text(s: &str) -> String {
    escape_label(&s.replace('\n', " "))
}

fn candidate_priority(label: NodeLabel, kind: DiagramKind) -> u8 {
    match kind {
        DiagramKind::Sequence => match label {
            NodeLabel::Method | NodeLabel::ControllerAction | NodeLabel::Function => 0,
            NodeLabel::Controller => 1,
            NodeLabel::Class => 2,
            NodeLabel::Service => 3,
            NodeLabel::Repository => 4,
            NodeLabel::Interface => 5,
            _ => 10,
        },
        DiagramKind::Class => match label {
            NodeLabel::Controller => 0,
            NodeLabel::Class => 1,
            NodeLabel::Service => 2,
            NodeLabel::Repository => 3,
            NodeLabel::Interface => 4,
            NodeLabel::Struct => 5,
            _ => 10,
        },
        DiagramKind::Flowchart => match label {
            NodeLabel::Controller => 0,
            NodeLabel::Class => 1,
            NodeLabel::Service => 2,
            NodeLabel::Interface => 3,
            _ => 10,
        },
    }
}

fn outgoing_edges<'a>(
    indexes: &'a GraphIndexes,
    node_id: &str,
) -> &'a [(String, RelationshipType)] {
    indexes
        .outgoing
        .get(node_id)
        .map(Vec::as_slice)
        .unwrap_or(&[])
}

fn incoming_edges<'a>(
    indexes: &'a GraphIndexes,
    node_id: &str,
) -> &'a [(String, RelationshipType)] {
    indexes
        .incoming
        .get(node_id)
        .map(Vec::as_slice)
        .unwrap_or(&[])
}

fn collect_methods(graph: &KnowledgeGraph, indexes: &GraphIndexes, node_id: &str) -> Vec<String> {
    let Some(start_node) = graph.get_node(node_id) else {
        return Vec::new();
    };

    if is_method_like(start_node.label) {
        return vec![node_id.to_string()];
    }

    let mut methods: Vec<_> = outgoing_edges(indexes, node_id)
        .iter()
        .filter(|(_, rel_type)| {
            matches!(
                rel_type,
                RelationshipType::HasMethod | RelationshipType::HasAction
            )
        })
        .map(|(target_id, _)| target_id.clone())
        .collect();
    methods.sort_by_key(|id| {
        graph
            .get_node(id)
            .map(|node| {
                (
                    node.properties.start_line.unwrap_or(u32::MAX),
                    node.properties.name.clone(),
                )
            })
            .unwrap_or((u32::MAX, id.clone()))
    });
    methods
}

fn is_method_like(label: NodeLabel) -> bool {
    matches!(
        label,
        NodeLabel::Method
            | NodeLabel::Function
            | NodeLabel::ControllerAction
            | NodeLabel::Constructor
    )
}

/// Build a skeleton flowchart (topological only, no if/else conditions).
/// Returns a Mermaid `flowchart TD` string with the call chain from `start_id`.
/// Intended as a starting point for the LLM to fill in conditions from source code.
pub(crate) fn build_skeleton_flowchart(
    graph: &KnowledgeGraph,
    indexes: &GraphIndexes,
    start_id: &str,
) -> String {
    let Some(start_node) = graph.get_node(start_id) else {
        return "flowchart TD".to_string();
    };

    let mut lines: Vec<String> = vec!["flowchart TD".to_string()];
    let mut seen = HashSet::new();
    let start_san = sanitize(start_id);
    let file_ref = if !start_node.properties.file_path.is_empty() {
        if let Some(fname) = start_node.properties.file_path.split('/').next_back() {
            format!(
                "\\n{}:{}",
                fname,
                start_node.properties.start_line.unwrap_or(0)
            )
        } else {
            String::new()
        }
    } else {
        String::new()
    };

    lines.push(format!(
        "    {}([\"START: {}{}\"])",
        start_san,
        escape_label(&start_node.properties.name),
        file_ref
    ));
    // BUG FIX: only insert start_id into `seen` when it is NOT already method-like.
    // When start_id IS a method, collect_methods returns [start_id], which would be
    // immediately skipped as already-seen → one-node diagram with no chain.
    if !is_method_like(start_node.label) {
        seen.insert(start_id.to_string());
    }

    // Collect methods and their callees (2 hops max, max 20 nodes)
    let methods = collect_methods(graph, indexes, start_id);
    let mut node_count = 1usize;

    for method_id in methods.iter().take(8) {
        if node_count >= 20 {
            break;
        }
        let Some(method_node) = graph.get_node(method_id) else {
            continue;
        };
        if seen.contains(method_id) {
            continue;
        }
        seen.insert(method_id.clone());

        let method_san = sanitize(method_id);
        let file_ref = if let Some(fname) = method_node.properties.file_path.split('/').next_back()
        {
            format!(
                "\\n{}:{}",
                fname,
                method_node.properties.start_line.unwrap_or(0)
            )
        } else {
            String::new()
        };

        // Decision diamond for query/check methods
        let name = &method_node.properties.name;
        let is_decision = name.to_lowercase().starts_with("get")
            || name.to_lowercase().starts_with("check")
            || name.to_lowercase().starts_with("is")
            || name.to_lowercase().starts_with("has")
            || name.to_lowercase().starts_with("verif");

        if is_decision {
            lines.push(format!(
                "    {}{{\"{}{}?\"}}",
                method_san,
                escape_label(name),
                file_ref
            ));
        } else {
            // Check if it's a DB write
            let is_db = name.to_lowercase().contains("save")
                || name.to_lowercase().contains("update")
                || name.to_lowercase().contains("delete")
                || name.to_lowercase().contains("insert");
            if is_db {
                lines.push(format!(
                    "    {}[(\"{}{}\")] ",
                    method_san,
                    escape_label(name),
                    file_ref
                ));
            } else {
                lines.push(format!(
                    "    {}[\"{}{}\" ]",
                    method_san,
                    escape_label(name),
                    file_ref
                ));
            }
        }
        lines.push(format!("    {} --> {}", sanitize(start_id), method_san));
        node_count += 1;

        // Callees of this method
        if let Some(callees) = indexes.outgoing.get(method_id) {
            for (callee_id, rel) in callees.iter().take(3) {
                if node_count >= 20 {
                    break;
                }
                if !matches!(
                    rel,
                    RelationshipType::Calls
                        | RelationshipType::CallsAction
                        | RelationshipType::CallsService
                ) {
                    continue;
                }
                if seen.contains(callee_id) {
                    continue;
                }
                let Some(callee_node) = graph.get_node(callee_id) else {
                    continue;
                };
                seen.insert(callee_id.clone());
                let callee_san = sanitize(callee_id);
                let file_ref =
                    if let Some(fname) = callee_node.properties.file_path.split('/').next_back() {
                        format!(
                            "\\n{}:{}",
                            fname,
                            callee_node.properties.start_line.unwrap_or(0)
                        )
                    } else {
                        String::new()
                    };
                lines.push(format!(
                    "    {}[\"{}{}\" ]",
                    callee_san,
                    escape_label(&callee_node.properties.name),
                    file_ref
                ));
                lines.push(format!("    {} --> {}", method_san, callee_san));
                node_count += 1;
            }
        }
    }

    lines.join("\n")
}

fn participant_for_method(
    graph: &KnowledgeGraph,
    indexes: &GraphIndexes,
    method_id: &str,
) -> Option<(String, String)> {
    for (owner_id, rel_type) in incoming_edges(indexes, method_id) {
        if matches!(
            rel_type,
            RelationshipType::HasMethod | RelationshipType::HasAction
        ) {
            if let Some(owner) = graph.get_node(owner_id) {
                return Some((sanitize(owner_id), owner.properties.name.clone()));
            }
        }
    }

    graph
        .get_node(method_id)
        .map(|method| (sanitize(method_id), method.properties.name.clone()))
}

fn push_participant(
    lines: &mut Vec<String>,
    seen: &mut HashSet<String>,
    participant_id: &str,
    label: &str,
) {
    if seen.insert(participant_id.to_string()) {
        lines.push(format!(
            "    participant {} as {}",
            participant_id,
            escape_sequence_text(label)
        ));
    }
}

fn push_message(
    lines: &mut Vec<String>,
    seen: &mut HashSet<(String, String, String)>,
    source_id: &str,
    target_id: &str,
    label: &str,
) {
    let key = (
        source_id.to_string(),
        target_id.to_string(),
        label.to_string(),
    );
    if seen.insert(key) {
        lines.push(format!(
            "    {}->>{}: {}",
            source_id,
            target_id,
            escape_sequence_text(label)
        ));
    }
}

fn is_hierarchy_rel(rel_type: RelationshipType) -> bool {
    matches!(
        rel_type,
        RelationshipType::Inherits | RelationshipType::Extends | RelationshipType::Implements
    )
}

fn class_id(graph: &KnowledgeGraph, node_id: &str) -> String {
    graph
        .get_node(node_id)
        .map(|node| sanitize(&node.properties.name))
        .unwrap_or_else(|| sanitize(node_id))
}

fn render_class_block(graph: &KnowledgeGraph, node: &GraphNode) -> String {
    [
        format!("    class {} {{", class_id(graph, &node.id)),
        format!("        <<{}>>", node.label.as_str()),
        "    }".to_string(),
    ]
    .join("\n")
}

fn sortable_name(graph: &KnowledgeGraph, node_id: &str) -> String {
    graph
        .get_node(node_id)
        .map(|node| node.properties.name.clone())
        .unwrap_or_else(|| node_id.to_string())
}

impl DiagramKind {
    fn parse(input: Option<&str>) -> Self {
        match input
            .unwrap_or("flowchart")
            .trim()
            .to_ascii_lowercase()
            .as_str()
        {
            "sequence" | "sequencediagram" | "sequence_diagram" => Self::Sequence,
            "class" | "classdiagram" | "class_diagram" => Self::Class,
            _ => Self::Flowchart,
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Flowchart => "flowchart",
            Self::Sequence => "sequence",
            Self::Class => "class",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use code_explorer_core::graph::types::{GraphRelationship, NodeProperties};

    fn node(id: &str, label: NodeLabel, name: &str, file_path: &str, start_line: u32) -> GraphNode {
        GraphNode {
            id: id.to_string(),
            label,
            properties: NodeProperties {
                name: name.to_string(),
                file_path: file_path.to_string(),
                start_line: Some(start_line),
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
            reason: String::new(),
            step: None,
        }
    }

    #[test]
    fn build_skeleton_flowchart_returns_header_for_empty_graph() {
        let graph = KnowledgeGraph::new();
        let indexes = GraphIndexes::build(&graph);

        let mermaid = build_skeleton_flowchart(&graph, &indexes, "missing");

        assert_eq!(mermaid, "flowchart TD");
    }

    #[test]
    fn diagram_kind_parse_sequence() {
        assert_eq!(DiagramKind::parse(Some("sequence")), DiagramKind::Sequence);
    }

    #[test]
    fn diagram_kind_parse_unknown_defaults_to_flowchart() {
        assert_eq!(DiagramKind::parse(Some("unknown")), DiagramKind::Flowchart);
    }

    #[test]
    fn build_skeleton_flowchart_generates_valid_mermaid_nodes() {
        let class_id = "Class:src/Courrier.cs:Courrier";
        let method_id = "Method:src/Courrier.cs:BuildMessage";
        let callee_id = "Method:src/Courrier.cs:SaveMessage";

        let mut graph = KnowledgeGraph::new();
        graph.add_node(node(
            class_id,
            NodeLabel::Class,
            "Courrier",
            "src/Courrier.cs",
            1,
        ));
        graph.add_node(node(
            method_id,
            NodeLabel::Method,
            "BuildMessage",
            "src/Courrier.cs",
            12,
        ));
        graph.add_node(node(
            callee_id,
            NodeLabel::Method,
            "SaveMessage",
            "src/Courrier.cs",
            42,
        ));
        graph.add_relationship(rel(
            "rel:has-method",
            class_id,
            method_id,
            RelationshipType::HasMethod,
        ));
        graph.add_relationship(rel(
            "rel:calls",
            method_id,
            callee_id,
            RelationshipType::Calls,
        ));

        let indexes = GraphIndexes::build(&graph);
        let mermaid = build_skeleton_flowchart(&graph, &indexes, class_id);

        assert!(mermaid.starts_with("flowchart TD"));
        assert!(mermaid
            .lines()
            .any(|line| line.contains("[\"") || line.contains("([\"")));
        assert!(mermaid.contains("Courrier"));
        assert!(mermaid.contains("BuildMessage"));
        assert!(mermaid.contains("SaveMessage"));
        assert!(mermaid.contains("-->"));
    }
}

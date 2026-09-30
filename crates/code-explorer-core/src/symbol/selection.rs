//! Stable symbol lookup shared by commands that accept a symbol argument.

use crate::graph::types::{GraphNode, NodeLabel, RelationshipType};
use crate::graph::KnowledgeGraph;
use std::collections::HashMap;

/// Resolve a node ID, a `Type.member`, a `path/file.ext:member`, or a bare name.
/// Exact matches always precede substring matches; ties have a stable order.
pub fn find_symbols<'a>(graph: &'a KnowledgeGraph, input: &str) -> Vec<&'a GraphNode> {
    if let Some(node) = graph.get_node(input) {
        return vec![node];
    }

    let normalized = input.replace('\\', "/");
    let (file, qualified) = normalized
        .rsplit_once(':')
        .filter(|(path, _)| path.contains('/') || path.contains('.'))
        .map(|(path, name)| (Some(path), name))
        .unwrap_or((None, normalized.as_str()));
    let (owner, name) = qualified
        .rsplit_once('.')
        .filter(|(owner, name)| is_identifier(owner) && is_identifier(name))
        .map(|(owner, name)| (Some(owner), name))
        .unwrap_or((None, qualified));

    let mut matches: Vec<_> = graph
        .iter_nodes()
        .filter(|node| {
            if let Some(path) = file {
                let node_path = node.properties.file_path.replace('\\', "/");
                if node_path != path && !node_path.ends_with(&format!("/{path}")) {
                    return false;
                }
            }
            if !node.properties.name.eq_ignore_ascii_case(name) {
                return false;
            }
            owner.map_or(true, |owner| {
                graph.iter_relationships().any(|rel| {
                    rel.target_id == node.id
                        && matches!(
                            rel.rel_type,
                            RelationshipType::HasMethod | RelationshipType::Defines
                        )
                        && graph.get_node(&rel.source_id).is_some_and(|parent| {
                            parent.properties.name.eq_ignore_ascii_case(owner)
                                && matches!(
                                    parent.label,
                                    NodeLabel::Class
                                        | NodeLabel::Struct
                                        | NodeLabel::Interface
                                        | NodeLabel::Impl
                                        | NodeLabel::Controller
                                )
                        })
                })
            })
        })
        .collect();

    if matches.is_empty() && owner.is_none() && file.is_none() {
        matches = graph
            .iter_nodes()
            .filter(|node| {
                node.properties
                    .name
                    .to_lowercase()
                    .contains(&name.to_lowercase())
            })
            .collect();
    }

    let owners: HashMap<&str, (u8, &str)> = graph
        .iter_relationships()
        .filter(|rel| rel.rel_type == RelationshipType::HasMethod)
        .filter_map(|rel| {
            graph.get_node(&rel.source_id).map(|parent| {
                (
                    rel.target_id.as_str(),
                    (
                        if matches!(
                            parent.label,
                            NodeLabel::Class | NodeLabel::Struct | NodeLabel::Controller
                        ) {
                            0
                        } else {
                            1
                        },
                        parent.properties.name.as_str(),
                    ),
                )
            })
        })
        .collect();
    matches.sort_by(|a, b| {
        priority(a.label)
            .cmp(&priority(b.label))
            .then_with(|| {
                is_test_path(&a.properties.file_path).cmp(&is_test_path(&b.properties.file_path))
            })
            .then_with(|| {
                (!owners.contains_key(a.id.as_str())).cmp(&!owners.contains_key(b.id.as_str()))
            })
            .then_with(|| owners.get(a.id.as_str()).cmp(&owners.get(b.id.as_str())))
            .then_with(|| a.properties.file_path.cmp(&b.properties.file_path))
            .then_with(|| a.properties.start_line.cmp(&b.properties.start_line))
            .then_with(|| a.id.cmp(&b.id))
    });
    matches
}

fn is_test_path(path: &str) -> bool {
    path.starts_with("tests/")
        || path.contains("/__tests__/")
        || path.contains(".test.")
        || path.contains(".spec.")
}

fn is_identifier(value: &str) -> bool {
    let mut chars = value.chars();
    matches!(chars.next(), Some(c) if c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

fn priority(label: NodeLabel) -> u8 {
    match label {
        NodeLabel::Controller => 0,
        NodeLabel::Class | NodeLabel::Struct => 1,
        NodeLabel::Service | NodeLabel::Interface => 2,
        NodeLabel::Method | NodeLabel::Function => 3,
        NodeLabel::ApiEndpoint | NodeLabel::Route => 4,
        NodeLabel::File => 8,
        _ => 10,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::types::{GraphRelationship, NodeProperties};

    #[test]
    fn qualified_name_file_and_id_select_the_same_homonym() {
        let mut graph = KnowledgeGraph::new();
        for (id, label, name, path) in [
            (
                "class:a",
                NodeLabel::Class,
                "AgentExecutor",
                "src/agent-executor.ts",
            ),
            (
                "method:a",
                NodeLabel::Method,
                "processUserMessage",
                "src/agent-executor.ts",
            ),
            (
                "class:b",
                NodeLabel::Class,
                "CodeBuddyAgent",
                "src/agent/codebuddy-agent.ts",
            ),
            (
                "method:b",
                NodeLabel::Method,
                "processUserMessage",
                "src/agent/codebuddy-agent.ts",
            ),
            (
                "interface:c",
                NodeLabel::Interface,
                "Agent",
                "src/types/agent.ts",
            ),
            (
                "method:c",
                NodeLabel::Method,
                "processUserMessage",
                "src/types/agent.ts",
            ),
        ] {
            graph.add_node(GraphNode {
                id: id.into(),
                label,
                properties: NodeProperties {
                    name: name.into(),
                    file_path: path.into(),
                    ..Default::default()
                },
            });
        }
        for (source, target) in [
            ("class:a", "method:a"),
            ("class:b", "method:b"),
            ("interface:c", "method:c"),
        ] {
            graph.add_relationship(GraphRelationship {
                id: format!("{source}:{target}"),
                source_id: source.into(),
                target_id: target.into(),
                rel_type: RelationshipType::HasMethod,
                confidence: 1.0,
                reason: String::new(),
                step: None,
            });
        }
        assert_eq!(find_symbols(&graph, "processUserMessage")[0].id, "method:a");
        for input in [
            "AgentExecutor.processUserMessage",
            "src/agent-executor.ts:processUserMessage",
            "method:a",
        ] {
            assert_eq!(find_symbols(&graph, input)[0].id, "method:a");
        }
        assert_eq!(
            find_symbols(&graph, "CodeBuddyAgent.processUserMessage")[0].id,
            "method:b"
        );
    }

    #[test]
    fn dotted_markdown_file_name_remains_a_plain_name() {
        let mut graph = KnowledgeGraph::new();
        graph.add_node(GraphNode {
            id: "file:chapter".into(),
            label: NodeLabel::File,
            properties: NodeProperties {
                name: "chapter-000.md".into(),
                file_path: "docs/chapter-000.md".into(),
                ..Default::default()
            },
        });
        assert_eq!(find_symbols(&graph, "chapter-000.md")[0].id, "file:chapter");
    }
}

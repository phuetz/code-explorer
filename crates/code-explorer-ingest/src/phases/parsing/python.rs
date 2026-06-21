use std::collections::{HashMap, HashSet};

use code_explorer_core::config::languages::SupportedLanguage;
use code_explorer_core::graph::types::*;
use code_explorer_core::id::generate_id;

use crate::phases::structure::FileEntry;

/// Python-specific post-pass. Two jobs, both fixing nesting the generic path misses:
///  1. `link_class_methods` — nest methods under their `Class` (HasMethod). Python
///     methods are captured as `@definition.function` → labeled `Function`, so the
///     generic nesting guard (`Method | Property | Constructor`) skips them.
///  2. `link_instance_attributes` — nest `self.x = …` instance attributes under their
///     class (HasProperty). Python stores most state on `self` inside `__init__`/methods,
///     but only *annotated class-level* attributes are captured by the query; these
///     instance attributes were invisible. We synthesize the `Property` nodes and the
///     `HasProperty` edges here.
pub(super) fn post_parse(
    root: tree_sitter::Node,
    file: &FileEntry,
    nodes: &mut Vec<GraphNode>,
    relationships: &mut Vec<GraphRelationship>,
) {
    link_class_methods(root, file, nodes, relationships);
    link_instance_attributes(root, file, nodes, relationships);
}

/// Nest each `function_definition` directly in a class body under its `Class` node —
/// unwrapping `decorated_definition` (`@property`/`@staticmethod`/`@classmethod`/route
/// decorators) and handling `async def` (still a `function_definition`). Iterating only
/// DIRECT body children avoids attaching nested `def`s declared inside a method.
fn link_class_methods(
    root: tree_sitter::Node,
    file: &FileEntry,
    nodes: &[GraphNode],
    relationships: &mut Vec<GraphRelationship>,
) {
    let content = file.content.as_bytes();

    // name -> Class node id, for Class nodes defined in this file.
    let mut class_node_ids: HashMap<&str, &str> = HashMap::new();
    for n in nodes {
        if n.label == NodeLabel::Class {
            class_node_ids
                .entry(n.properties.name.as_str())
                .or_insert(n.id.as_str());
        }
    }
    if class_node_ids.is_empty() {
        return;
    }

    let mut emitted: HashSet<String> = HashSet::new();
    let mut stack = vec![root];
    while let Some(node) = stack.pop() {
        if node.kind() == "class_definition" {
            if let Some(class_name) = node
                .child_by_field_name("name")
                .and_then(|n| n.utf8_text(content).ok())
            {
                if let Some(&owner_id) = class_node_ids.get(class_name) {
                    if let Some(body) = python_class_body(&node) {
                        let mut bcur = body.walk();
                        for child in body.children(&mut bcur) {
                            let Some(name_node) = python_method_name_node(&child) else {
                                continue;
                            };
                            let Ok(method_name) = name_node.utf8_text(content) else {
                                continue;
                            };
                            let method_id =
                                generate_id("Function", &format!("{}:{}", file.path, method_name));
                            // Key the edge on the owning class too, so two classes in
                            // one file that both define e.g. `__init__` each get their
                            // own HasMethod edge. (The target Function node is still
                            // shared across same-named methods in a file — a pre-existing
                            // id-model limitation, not addressed here.)
                            let edge_id = format!(
                                "{}_{}_{}",
                                RelationshipType::HasMethod.as_str().to_lowercase(),
                                owner_id,
                                method_id
                            );
                            if !emitted.insert(edge_id.clone()) {
                                continue;
                            }
                            relationships.push(GraphRelationship {
                                id: edge_id,
                                source_id: owner_id.to_string(),
                                target_id: method_id,
                                rel_type: RelationshipType::HasMethod,
                                confidence: 1.0,
                                reason: "python_class_nesting".to_string(),
                                step: None,
                            });
                        }
                    }
                }
            }
        }

        let mut cur = node.walk();
        for child in node.children(&mut cur) {
            stack.push(child);
        }
    }
}

/// Synthesize `Property` nodes + `HasProperty` edges for `self.<attr> = …` instance
/// attributes assigned anywhere inside a class's methods. The `Property` node id matches
/// the deterministic scheme (`Property:<file>:<attr>`), so an attribute that is *also*
/// an annotated class var collapses onto the same node. New nodes get a `File→Property`
/// `Defines` edge to match query-extracted properties.
fn link_instance_attributes(
    root: tree_sitter::Node,
    file: &FileEntry,
    nodes: &mut Vec<GraphNode>,
    relationships: &mut Vec<GraphRelationship>,
) {
    let content = file.content.as_bytes();

    // Owned snapshots taken before mutating `nodes`.
    let class_ids: HashMap<String, String> = nodes
        .iter()
        .filter(|n| n.label == NodeLabel::Class)
        .map(|n| (n.properties.name.clone(), n.id.clone()))
        .collect();
    if class_ids.is_empty() {
        return;
    }
    let mut existing: HashSet<String> = nodes.iter().map(|n| n.id.clone()).collect();
    let file_node_id = generate_id("File", &file.path);

    let mut new_nodes: Vec<GraphNode> = Vec::new();
    let mut emitted_edges: HashSet<String> = HashSet::new();

    let mut stack = vec![root];
    while let Some(node) = stack.pop() {
        if node.kind() == "class_definition" {
            if let Some(cname) = node
                .child_by_field_name("name")
                .and_then(|n| n.utf8_text(content).ok())
            {
                if let Some(owner_id) = class_ids.get(cname) {
                    let mut attrs: Vec<(&str, u32)> = Vec::new();
                    collect_self_attrs(&node, content, &mut attrs);
                    for (attr, line) in attrs {
                        let pid = generate_id("Property", &format!("{}:{}", file.path, attr));
                        if existing.insert(pid.clone()) {
                            new_nodes.push(GraphNode {
                                id: pid.clone(),
                                label: NodeLabel::Property,
                                properties: NodeProperties {
                                    name: attr.to_string(),
                                    file_path: file.path.clone(),
                                    start_line: Some(line),
                                    end_line: Some(line),
                                    language: Some(SupportedLanguage::Python),
                                    is_exported: Some(!attr.starts_with('_')),
                                    ..Default::default()
                                },
                            });
                            relationships.push(GraphRelationship {
                                id: format!("defines_{}_{}", file_node_id, pid),
                                source_id: file_node_id.clone(),
                                target_id: pid.clone(),
                                rel_type: RelationshipType::Defines,
                                confidence: 1.0,
                                reason: "python_self_attr".to_string(),
                                step: None,
                            });
                        }
                        let edge_id = format!(
                            "{}_{}_{}",
                            RelationshipType::HasProperty.as_str().to_lowercase(),
                            owner_id,
                            pid
                        );
                        if emitted_edges.insert(edge_id.clone()) {
                            relationships.push(GraphRelationship {
                                id: edge_id,
                                source_id: owner_id.clone(),
                                target_id: pid,
                                rel_type: RelationshipType::HasProperty,
                                confidence: 1.0,
                                reason: "python_self_attr".to_string(),
                                step: None,
                            });
                        }
                    }
                }
            }
        }

        let mut cur = node.walk();
        for child in node.children(&mut cur) {
            stack.push(child);
        }
    }

    nodes.extend(new_nodes);
}

/// Collect `self.<attr>` assignment targets inside a class's methods, as `(attr, line)`.
/// Skips nested `class_definition` subtrees (an inner class's `self` is its own).
fn collect_self_attrs<'a>(
    class_node: &tree_sitter::Node,
    content: &'a [u8],
    out: &mut Vec<(&'a str, u32)>,
) {
    let Some(body) = python_class_body(class_node) else {
        return;
    };
    let mut stack = vec![body];
    while let Some(n) = stack.pop() {
        match n.kind() {
            // A nested class owns its own `self`; handled when the outer walk reaches it.
            "class_definition" => continue,
            "assignment" | "augmented_assignment" => {
                if let Some(left) = n.child_by_field_name("left") {
                    collect_self_attr_targets(&left, content, out);
                }
            }
            _ => {}
        }
        let mut cur = n.walk();
        for child in n.children(&mut cur) {
            stack.push(child);
        }
    }
}

/// Recurse into an assignment target, collecting each `self.<attr>` (handles single
/// `self.x` and tuple/list targets `self.x, self.y = …`).
fn collect_self_attr_targets<'a>(
    node: &tree_sitter::Node,
    content: &'a [u8],
    out: &mut Vec<(&'a str, u32)>,
) {
    if node.kind() == "attribute" {
        if let Some(name) = self_attr_name(node, content) {
            out.push((name, node.start_position().row as u32 + 1));
        }
        return;
    }
    let mut cur = node.walk();
    for child in node.children(&mut cur) {
        collect_self_attr_targets(&child, content, out);
    }
}

/// `self.<attr>` → `Some("attr")`, else `None`.
fn self_attr_name<'a>(attribute: &tree_sitter::Node, content: &'a [u8]) -> Option<&'a str> {
    let obj = attribute.child_by_field_name("object")?;
    if obj.kind() != "identifier" || obj.utf8_text(content).ok()? != "self" {
        return None;
    }
    attribute
        .child_by_field_name("attribute")
        .and_then(|a| a.utf8_text(content).ok())
}

/// The `block` body of a Python `class_definition` (via the `body` field, with a
/// defensive fallback to the first `block` child).
fn python_class_body<'t>(class_node: &tree_sitter::Node<'t>) -> Option<tree_sitter::Node<'t>> {
    class_node.child_by_field_name("body").or_else(|| {
        let mut cur = class_node.walk();
        let found = class_node
            .children(&mut cur)
            .find(|ch| ch.kind() == "block");
        found
    })
}

/// If a class-body child is a method, return its name node. Handles a bare
/// `function_definition` (including `async def`) and a `decorated_definition`
/// wrapping a function (`@property`/`@staticmethod`/`@classmethod`/route decorators).
/// Returns `None` for non-method members (assignments, nested classes, `pass`, …).
fn python_method_name_node<'t>(child: &tree_sitter::Node<'t>) -> Option<tree_sitter::Node<'t>> {
    let func = match child.kind() {
        "function_definition" => *child,
        "decorated_definition" => {
            let def = child.child_by_field_name("definition")?;
            if def.kind() != "function_definition" {
                return None;
            }
            def
        }
        _ => return None,
    };
    func.child_by_field_name("name")
}

#[cfg(test)]
mod tests {
    use crate::phases::parsing::parse_files;
    use crate::phases::structure::FileEntry;
    use code_explorer_core::config::languages::SupportedLanguage;
    use code_explorer_core::graph::types::*;
    use code_explorer_core::graph::KnowledgeGraph;

    #[test]
    fn test_python_self_attributes_nest_as_properties() {
        let content = r#"
class Account:
    rate: float = 0.0

    def __init__(self, owner):
        self.owner = owner
        self._balance = 0
        self.x, self.y = 1, 2

    def deposit(self, amount):
        self.history = []

class Other:
    def __init__(self):
        self.owner = "n/a"
"#;
        let file = FileEntry {
            path: "acct.py".to_string(),
            content: content.to_string(),
            size: content.len(),
            language: Some(SupportedLanguage::Python),
        };
        let mut graph = KnowledgeGraph::new();
        graph.add_node(GraphNode {
            id: "File:acct.py".to_string(),
            label: NodeLabel::File,
            properties: NodeProperties {
                name: "acct.py".to_string(),
                file_path: "acct.py".to_string(),
                ..Default::default()
            },
        });
        let _ = parse_files(&mut graph, &[file], None).unwrap();

        let has_prop = |src: &str, attr: &str| {
            graph.iter_relationships().any(|r| {
                r.rel_type == RelationshipType::HasProperty
                    && r.source_id == src
                    && r.target_id == format!("Property:acct.py:{attr}")
            })
        };

        // Instance attributes assigned on self nest under their class.
        assert!(has_prop("Class:acct.py:Account", "owner"), "self.owner nests");
        assert!(has_prop("Class:acct.py:Account", "_balance"), "self._balance nests");
        assert!(has_prop("Class:acct.py:Account", "history"), "self.history (in another method) nests");
        // Tuple-target instance attributes nest.
        assert!(has_prop("Class:acct.py:Account", "x"), "tuple self.x nests");
        assert!(has_prop("Class:acct.py:Account", "y"), "tuple self.y nests");
        // The Property nodes were synthesized.
        assert!(graph.get_node("Property:acct.py:history").is_some(), "history Property node created");
        // A different class with the same attr name also gets its own edge.
        assert!(has_prop("Class:acct.py:Other", "owner"), "Other.owner nests (shared node, own edge)");
    }
}

use std::collections::{HashMap, HashSet};

use code_explorer_core::graph::types::*;
use code_explorer_core::graph::KnowledgeGraph;
use code_explorer_core::id::generate_id;
use once_cell::sync::Lazy;
use regex::Regex;

use crate::phases::structure::FileEntry;

static RE_GO_METHOD: Lazy<Regex> = Lazy::new(|| {
    // `func (r *Type[T]) Method(`  /  `func (r Type) Method(` at file scope.
    Regex::new(r#"(?m)^func\s*\(\s*\w+\s+\*?\s*(\w+)(?:\[[^\]]*\])?\s*\)\s*(\w+)\s*\("#)
        .expect("go method regex compiles")
});

/// Go-specific post-pass: nest methods and struct fields under their owning type.
///
/// Go breaks the generic lexical-nesting model twice (both are 0 in a fresh index):
///  1. A method is a *top-level* `method_declaration` whose owning type is named by its
///     **receiver** (`func (u *User) Save()`), not by lexical containment — so
///     `find_enclosing_class_id` never reaches `User` and no `HasMethod` is emitted.
///  2. Struct fields live inside `type_spec → struct_type`; that `type_spec` container is
///     not in `CONTAINER_KINDS` (and its label couldn't be derived there), so the generic
///     `HasProperty` nesting also fails.
///
/// Here we resolve each method's receiver type and each struct's fields to the same-file
/// `Struct`/`Interface` node and emit the `HasMethod` / `HasProperty` edges (keying the
/// edge id on the owner so same-named members on different types don't collide). Same-file
/// only: a Go type and its methods can be split across files of a package — cross-file
/// resolution would need a package-wide pass and is a follow-up.
pub(super) fn post_parse(
    root: tree_sitter::Node,
    file: &FileEntry,
    nodes: &[GraphNode],
    relationships: &mut Vec<GraphRelationship>,
) {
    let content = file.content.as_bytes();

    // name -> owning type node id, for Struct/Interface nodes defined in this file.
    let mut type_ids: HashMap<&str, &str> = HashMap::new();
    for n in nodes {
        if matches!(n.label, NodeLabel::Struct | NodeLabel::Interface) {
            type_ids
                .entry(n.properties.name.as_str())
                .or_insert(n.id.as_str());
        }
    }
    if type_ids.is_empty() {
        return;
    }

    let mut emitted: HashSet<String> = HashSet::new();
    let mut stack = vec![root];
    while let Some(node) = stack.pop() {
        match node.kind() {
            // `func (r *Recv) Method()` — owning type is the receiver, resolved by name.
            "method_declaration" => {
                if let Some(recv) = go_receiver_type_name(&node, content) {
                    if let Some(&owner_id) = type_ids.get(recv) {
                        if let Some(mname) = node
                            .child_by_field_name("name")
                            .and_then(|n| n.utf8_text(content).ok())
                        {
                            let mid = generate_id("Method", &format!("{}:{}", file.path, mname));
                            push_edge(
                                &mut emitted,
                                relationships,
                                RelationshipType::HasMethod,
                                owner_id,
                                mid,
                                "go_method_nesting",
                            );
                        }
                    }
                }
            }
            // `type T struct { Field A }` — fields nest under the Struct node.
            "type_spec" => {
                let tname = node
                    .child_by_field_name("name")
                    .and_then(|n| n.utf8_text(content).ok());
                let sty = node.child_by_field_name("type");
                if let (Some(tname), Some(sty)) = (tname, sty) {
                    if sty.kind() == "struct_type" {
                        if let Some(&owner_id) = type_ids.get(tname) {
                            for fname in go_struct_field_names(&sty, content) {
                                let pid =
                                    generate_id("Property", &format!("{}:{}", file.path, fname));
                                push_edge(
                                    &mut emitted,
                                    relationships,
                                    RelationshipType::HasProperty,
                                    owner_id,
                                    pid,
                                    "go_field_nesting",
                                );
                            }
                        }
                    }
                }
            }
            _ => {}
        }

        let mut cur = node.walk();
        for child in node.children(&mut cur) {
            stack.push(child);
        }
    }
}

fn push_edge(
    emitted: &mut HashSet<String>,
    relationships: &mut Vec<GraphRelationship>,
    rel_type: RelationshipType,
    owner_id: &str,
    target_id: String,
    reason: &str,
) {
    let edge_id = format!(
        "{}_{}_{}",
        rel_type.as_str().to_lowercase(),
        owner_id,
        target_id
    );
    if !emitted.insert(edge_id.clone()) {
        return;
    }
    relationships.push(GraphRelationship {
        id: edge_id,
        source_id: owner_id.to_string(),
        target_id,
        rel_type,
        confidence: 1.0,
        reason: reason.to_string(),
        step: None,
    });
}

/// Base receiver type name of a `method_declaration`, unwrapping `*T` and `T[U]` to `T`.
fn go_receiver_type_name<'a>(method: &tree_sitter::Node, content: &'a [u8]) -> Option<&'a str> {
    let recv = method.child_by_field_name("receiver")?; // parameter_list
    let mut cur = recv.walk();
    let param = recv
        .children(&mut cur)
        .find(|c| c.kind() == "parameter_declaration")?;
    let mut t = param.child_by_field_name("type")?;
    loop {
        match t.kind() {
            "type_identifier" => return t.utf8_text(content).ok(),
            "pointer_type" => t = t.named_child(0)?,
            "generic_type" => t = t.child_by_field_name("type")?,
            _ => return None,
        }
    }
}

/// Named field identifiers declared directly in a `struct_type` body (handles
/// multi-name declarations like `X, Y int`; skips embedded/anonymous fields).
fn go_struct_field_names<'a>(struct_type: &tree_sitter::Node, content: &'a [u8]) -> Vec<&'a str> {
    let mut out = Vec::new();
    let mut c = struct_type.walk();
    for child in struct_type.children(&mut c) {
        if child.kind() != "field_declaration_list" {
            continue;
        }
        let mut c2 = child.walk();
        for fd in child.children(&mut c2) {
            if fd.kind() != "field_declaration" {
                continue;
            }
            let mut c3 = fd.walk();
            for part in fd.children(&mut c3) {
                if part.kind() == "field_identifier" {
                    if let Ok(name) = part.utf8_text(content) {
                        out.push(name);
                    }
                }
            }
        }
    }
    out
}

/// Cross-file Go method reconciliation, run once after parsing.
///
/// Per-file `post_parse` only links a method to a receiver type defined in the SAME
/// file, but Go packages span files (a type in `user.go`, its methods in
/// `user_methods.go`). A Go method can only be declared on a type in its OWN package,
/// and a package is exactly one directory — so we link any still-floating Go `Method`
/// to a `Struct`/`Interface` of its receiver type in the SAME DIRECTORY (unambiguous).
/// Only methods without an existing `HasMethod` edge are touched, so this never
/// duplicates the same-file links. Returns the number of edges added.
pub(crate) fn reconcile_cross_file_methods(graph: &mut KnowledgeGraph, files: &[FileEntry]) -> usize {
    fn dir_of(path: &str) -> &str {
        path.rsplit_once('/').map(|(d, _)| d).unwrap_or("")
    }

    // Compute new edges while only borrowing the graph immutably; the maps holding
    // `&str` into the graph are dropped at the end of this block so we can mutate after.
    let new_edges: Vec<(String, String)> = {
        let mut already_nested: HashSet<&str> = HashSet::new();
        for r in graph.iter_relationships() {
            if r.rel_type == RelationshipType::HasMethod {
                already_nested.insert(r.target_id.as_str());
            }
        }
        let mut type_ids: HashMap<(&str, &str), &str> = HashMap::new();
        // Set of existing Method node ids, so we never link to a phantom target.
        let mut method_ids: HashSet<&str> = HashSet::new();
        for n in graph.iter_nodes() {
            match n.label {
                NodeLabel::Struct | NodeLabel::Interface => {
                    type_ids
                        .entry((dir_of(&n.properties.file_path), n.properties.name.as_str()))
                        .or_insert(n.id.as_str());
                }
                NodeLabel::Method => {
                    method_ids.insert(n.id.as_str());
                }
                _ => {}
            }
        }
        if type_ids.is_empty() {
            return 0;
        }

        let mut edges = Vec::new();
        let mut emitted: HashSet<String> = HashSet::new();
        for f in files {
            if !f.path.ends_with(".go") {
                continue;
            }
            let dir = dir_of(&f.path);
            for cap in RE_GO_METHOD.captures_iter(&f.content) {
                let recv = cap.get(1).unwrap().as_str();
                let method = cap.get(2).unwrap().as_str();
                let method_id = generate_id("Method", &format!("{}:{}", f.path, method));
                // Only link to a Method node that exists and isn't already nested.
                if !method_ids.contains(method_id.as_str())
                    || already_nested.contains(method_id.as_str())
                {
                    continue;
                }
                if let Some(&owner) = type_ids.get(&(dir, recv)) {
                    let edge_id = format!("has_method_{}_{}", owner, method_id);
                    if emitted.insert(edge_id) {
                        edges.push((owner.to_string(), method_id));
                    }
                }
            }
        }
        edges
    };

    let count = new_edges.len();
    for (owner, method_id) in new_edges {
        let edge_id = format!("has_method_{}_{}", owner, method_id);
        graph.add_relationship(GraphRelationship {
            id: edge_id,
            source_id: owner,
            target_id: method_id,
            rel_type: RelationshipType::HasMethod,
            confidence: 1.0,
            reason: "go_cross_file_method".to_string(),
            step: None,
        });
    }
    count
}

#[cfg(test)]
mod tests {
    use super::reconcile_cross_file_methods;
    use crate::phases::parsing::parse_files;
    use crate::phases::structure::FileEntry;
    use code_explorer_core::config::languages::SupportedLanguage;
    use code_explorer_core::graph::types::*;
    use code_explorer_core::graph::KnowledgeGraph;

    #[test]
    fn test_go_methods_and_fields_nest_under_type() {
        let content = r#"
package main

type User struct {
    Name string
    age  int
}

func (u *User) Save() {}
func (u User) Greet() string { return u.Name }

func freeFunc() {}
"#;
        let file = FileEntry {
            path: "m.go".to_string(),
            content: content.to_string(),
            size: content.len(),
            language: Some(SupportedLanguage::Go),
        };
        let mut graph = KnowledgeGraph::new();
        graph.add_node(GraphNode {
            id: "File:m.go".to_string(),
            label: NodeLabel::File,
            properties: NodeProperties {
                name: "m.go".to_string(),
                file_path: "m.go".to_string(),
                ..Default::default()
            },
        });
        let _ = parse_files(&mut graph, &[file], None).unwrap();

        let edge = |rt: RelationshipType, src: &str, tgt: &str| {
            graph
                .iter_relationships()
                .any(|r| r.rel_type == rt && r.source_id == src && r.target_id == tgt)
        };

        // Pointer- and value-receiver methods both nest under the receiver type.
        assert!(
            edge(RelationshipType::HasMethod, "Struct:m.go:User", "Method:m.go:Save"),
            "pointer-receiver method nests"
        );
        assert!(
            edge(RelationshipType::HasMethod, "Struct:m.go:User", "Method:m.go:Greet"),
            "value-receiver method nests"
        );
        // Struct fields nest under the struct.
        assert!(
            edge(RelationshipType::HasProperty, "Struct:m.go:User", "Property:m.go:Name"),
            "exported field nests"
        );
        assert!(
            edge(RelationshipType::HasProperty, "Struct:m.go:User", "Property:m.go:age"),
            "unexported field nests"
        );
        // A free function is not a method and must not nest.
        assert!(
            !graph.iter_relationships().any(|r| r.rel_type == RelationshipType::HasMethod
                && r.target_id.ends_with(":freeFunc")),
            "free function must not nest"
        );
    }

    #[test]
    fn test_go_cross_file_method_reconciliation() {
        // Type in pkg/user.go, its method in pkg/user_methods.go (same dir = same package).
        let mk = |path: &str, content: &str| FileEntry {
            path: path.to_string(),
            content: content.to_string(),
            size: content.len(),
            language: Some(SupportedLanguage::Go),
        };
        let files = [
            mk("pkg/user.go", "package pkg\n\ntype User struct {\n  Name string\n}\n"),
            mk("pkg/user_methods.go", "package pkg\n\nfunc (u *User) Save() {}\n"),
        ];
        let mut graph = KnowledgeGraph::new();
        for p in ["pkg/user.go", "pkg/user_methods.go"] {
            graph.add_node(GraphNode {
                id: format!("File:{p}"),
                label: NodeLabel::File,
                properties: NodeProperties {
                    name: p.to_string(),
                    file_path: p.to_string(),
                    ..Default::default()
                },
            });
        }
        let _ = parse_files(&mut graph, &files, None).unwrap();

        fn has(g: &KnowledgeGraph, src: &str, tgt: &str) -> bool {
            g.iter_relationships().any(|r| r.rel_type == RelationshipType::HasMethod
                && r.source_id == src
                && r.target_id == tgt)
        }
        // Same-file post-pass can't link Save (User is in a different file).
        assert!(
            !has(&graph, "Struct:pkg/user.go:User", "Method:pkg/user_methods.go:Save"),
            "cross-file method should not be linked by the per-file pass"
        );
        // The reconciler links it via same-directory (same-package) resolution.
        let n = reconcile_cross_file_methods(&mut graph, &files);
        assert!(n >= 1, "reconciler links at least one cross-file method");
        assert!(
            has(&graph, "Struct:pkg/user.go:User", "Method:pkg/user_methods.go:Save"),
            "cross-file method linked after reconcile"
        );
    }
}

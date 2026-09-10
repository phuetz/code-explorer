use std::collections::{HashMap, HashSet};

use code_explorer_core::graph::types::*;
use code_explorer_core::graph::KnowledgeGraph;
use code_explorer_core::id::generate_id;
use once_cell::sync::Lazy;
use regex::Regex;

use crate::phases::structure::FileEntry;

static RE_CPP_METHOD_DEF: Lazy<Regex> = Lazy::new(|| {
    // Out-of-class method definition: `<return type> Class::method(` at statement scope.
    // The leading class `[\w:<>,\*&\s~]*?` allows a (possibly qualified) return type but
    // excludes `=`, `(`, `.`, `->`, so calls like `x = Factory::make(` aren't matched.
    // Captures the immediate class qualifier and the method name (incl. `~Dtor`).
    Regex::new(r#"(?m)^\s*[\w:<>,\*&\s~]*?\b(\w+)\s*::\s*(~?\w+)\s*\("#)
        .expect("cpp method-def regex compiles")
});

/// For a C/C++ `function_definition` whose name lives in a declarator (it has no `name`
/// field, unlike Python/Rust), return `(name, label)` matching the node id the C++/C
/// tree-sitter query created — or `None` for uncertain forms. A `None`, and any residual
/// mismatch, is made safe by `repoint_orphan_call_sources` (the call then stays
/// File-sourced). Used by `find_enclosing_method_id` to attribute calls in C/C++ funcs.
pub(super) fn enclosing_cpp_fn<'a>(
    func_def: &tree_sitter::Node,
    content: &'a [u8],
) -> Option<(&'a str, &'static str)> {
    // Free template functions are labeled `Template` (not Function) by the query, and
    // template member functions aren't captured the same way — bail before the label
    // would mismatch.
    {
        let mut a = func_def.parent();
        while let Some(p) = a {
            match p.kind() {
                "template_declaration" => return None,
                "translation_unit" | "namespace_definition" | "field_declaration_list" => break,
                _ => {}
            }
            a = p.parent();
        }
    }

    // Unwrap pointer/reference return-type declarators to the function_declarator.
    let mut decl = func_def.child_by_field_name("declarator")?;
    while matches!(decl.kind(), "pointer_declarator" | "reference_declarator") {
        decl = decl
            .child_by_field_name("declarator")
            .or_else(|| decl.named_child(0))?;
    }
    if decl.kind() != "function_declarator" {
        return None;
    }
    let name_decl = decl.child_by_field_name("declarator")?;
    let (name_node, qualified) = match name_decl.kind() {
        "identifier" | "field_identifier" | "destructor_name" => (name_decl, false),
        "qualified_identifier" => {
            let n = name_decl.child_by_field_name("name")?;
            match n.kind() {
                "identifier" | "destructor_name" => (n, true),
                _ => return None,
            }
        }
        _ => return None,
    };
    let name = name_node.utf8_text(content).ok()?;

    let in_class = {
        let mut a = func_def.parent();
        let mut found = false;
        while let Some(p) = a {
            match p.kind() {
                "field_declaration_list" => {
                    found = true;
                    break;
                }
                "translation_unit" | "namespace_definition" => break,
                _ => {}
            }
            a = p.parent();
        }
        found
    };
    let label = if qualified || in_class { "Method" } else { "Function" };
    Some((name, label))
}

fn is_cpp_file(path: &str) -> bool {
    // Out-of-class method definitions live in C++ translation units / C++ headers.
    // `.h` is parsed as C (no C++ classes), so it's excluded.
    matches!(
        path.rsplit_once('.').map(|(_, e)| e),
        Some("cpp" | "cc" | "cxx" | "hpp" | "hh")
    )
}

/// Nest C++ out-of-class method definitions (`void User::save() {…}`, common in `.cpp`
/// files) under their class. The tree-sitter query records such a method as a `Method`
/// node named after the bare method (`Method:<file>:save`) with no enclosing class, so it
/// floats. Inline methods (defined in the class body) already nest via the generic path.
///
/// Resolution: a `Class::method` def links to a `Class`/`Struct` node named `Class` —
/// preferring one in the SAME DIRECTORY, else falling back to a repo-wide UNIQUE match
/// (handles the common `include/` vs `src/` split); ambiguous names are skipped. Only
/// still-floating methods are touched and both endpoints are verified to exist, so this
/// never creates dangling or duplicate edges. Returns the number of edges added.
pub(crate) fn reconcile_out_of_class_methods(
    graph: &mut KnowledgeGraph,
    files: &[FileEntry],
) -> usize {
    fn dir_of(path: &str) -> &str {
        path.rsplit_once('/').map(|(d, _)| d).unwrap_or("")
    }

    let new_edges: Vec<(String, String)> = {
        let mut already_nested: HashSet<&str> = HashSet::new();
        for r in graph.iter_relationships() {
            if r.rel_type == RelationshipType::HasMethod {
                already_nested.insert(r.target_id.as_str());
            }
        }
        // class name -> list of (dir, node id) for Class/Struct nodes; and the set of
        // Method node ids that actually exist (so we never link to a phantom target —
        // the regex can match defs the tree-sitter query named differently, e.g.
        // operators/templates).
        let mut by_name: HashMap<&str, Vec<(&str, &str)>> = HashMap::new();
        let mut method_ids: HashSet<&str> = HashSet::new();
        for n in graph.iter_nodes() {
            match n.label {
                NodeLabel::Class | NodeLabel::Struct => by_name
                    .entry(n.properties.name.as_str())
                    .or_default()
                    .push((dir_of(&n.properties.file_path), n.id.as_str())),
                NodeLabel::Method => {
                    method_ids.insert(n.id.as_str());
                }
                _ => {}
            }
        }
        if by_name.is_empty() {
            return 0;
        }

        // Resolve a class name from a file's directory: same-dir wins; else unique global.
        let resolve = |class: &str, dir: &str| -> Option<&str> {
            let cands = by_name.get(class)?;
            if let Some((_, id)) = cands.iter().find(|(d, _)| *d == dir) {
                return Some(id);
            }
            if cands.len() == 1 {
                return Some(cands[0].1);
            }
            None
        };

        let mut edges = Vec::new();
        let mut emitted: HashSet<String> = HashSet::new();
        for f in files {
            if !is_cpp_file(&f.path) {
                continue;
            }
            let dir = dir_of(&f.path);
            for cap in RE_CPP_METHOD_DEF.captures_iter(&f.content) {
                let class = cap.get(1).unwrap().as_str();
                let method = cap.get(2).unwrap().as_str();
                let method_id = generate_id("Method", &format!("{}:{}", f.path, method));
                // Only link to a Method node that actually exists and isn't yet nested.
                if !method_ids.contains(method_id.as_str())
                    || already_nested.contains(method_id.as_str())
                {
                    continue;
                }
                if let Some(owner) = resolve(class, dir) {
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
            confidence: 0.9,
            reason: "cpp_out_of_class_method".to_string(),
            step: None,
        });
    }
    count
}

#[cfg(test)]
mod tests {
    use super::reconcile_out_of_class_methods;
    use crate::phases::parsing::parse_files;
    use crate::phases::structure::FileEntry;
    use code_explorer_core::config::languages::SupportedLanguage;
    use code_explorer_core::graph::types::*;
    use code_explorer_core::graph::KnowledgeGraph;

    #[test]
    fn test_cpp_out_of_class_methods_reconcile() {
        let mk = |path: &str, content: &str| FileEntry {
            path: path.to_string(),
            content: content.to_string(),
            size: content.len(),
            language: Some(SupportedLanguage::CPlusPlus),
        };
        // Class declared in lib/user.hpp; methods defined out-of-class in lib/user.cpp.
        let files = [
            mk("lib/user.hpp", "struct User {\n  void save();\n  int count() const;\n};\n"),
            mk(
                "lib/user.cpp",
                "#include \"user.hpp\"\nvoid User::save() {}\nint User::count() const { return 0; }\n",
            ),
        ];
        let mut graph = KnowledgeGraph::new();
        for p in ["lib/user.hpp", "lib/user.cpp"] {
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
        // The out-of-class definition floats until reconciliation.
        assert!(
            !has(&graph, "Struct:lib/user.hpp:User", "Method:lib/user.cpp:save"),
            "out-of-class def not linked by per-file pass"
        );
        let n = reconcile_out_of_class_methods(&mut graph, &files);
        assert!(n >= 2, "links both out-of-class methods");
        assert!(
            has(&graph, "Struct:lib/user.hpp:User", "Method:lib/user.cpp:save"),
            "save() def linked to User after reconcile"
        );
        assert!(
            has(&graph, "Struct:lib/user.hpp:User", "Method:lib/user.cpp:count"),
            "count() def linked to User after reconcile"
        );
    }
}

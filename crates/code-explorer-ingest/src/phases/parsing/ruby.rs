use std::collections::{HashMap, HashSet};

use code_explorer_core::graph::types::*;
use code_explorer_core::id::generate_id;

use crate::phases::structure::FileEntry;

/// Ruby-specific post-pass: nest methods defined directly in a `module` block under
/// their `Module` node (HasMethod).
///
/// Methods inside a `class` already nest via the generic path (tree-sitter-ruby's
/// `class` node is a recognized container). `module` is not — and it cannot be added to
/// the shared `CONTAINER_KINDS`, because tree-sitter-**python**'s root node is also
/// `module`, which would mis-nest every Python top-level def. So Ruby module nesting is
/// handled here, gated to Ruby. Only DIRECT module-body methods are linked; a method in
/// a `class` nested inside the module is left to the generic path.
pub(super) fn post_parse(
    root: tree_sitter::Node,
    file: &FileEntry,
    nodes: &[GraphNode],
    relationships: &mut Vec<GraphRelationship>,
) {
    let content = file.content.as_bytes();

    let mut module_ids: HashMap<&str, &str> = HashMap::new();
    for n in nodes {
        if n.label == NodeLabel::Module {
            module_ids
                .entry(n.properties.name.as_str())
                .or_insert(n.id.as_str());
        }
    }
    if module_ids.is_empty() {
        return;
    }

    let mut emitted: HashSet<String> = HashSet::new();
    let mut stack = vec![root];
    while let Some(node) = stack.pop() {
        if node.kind() == "module" {
            if let Some(mname) = node
                .child_by_field_name("name")
                .and_then(|n| n.utf8_text(content).ok())
            {
                if let Some(&owner_id) = module_ids.get(mname) {
                    for method in module_direct_methods(&node) {
                        if let Some(name) = method
                            .child_by_field_name("name")
                            .and_then(|n| n.utf8_text(content).ok())
                        {
                            let mid = generate_id("Method", &format!("{}:{}", file.path, name));
                            let edge_id = format!(
                                "{}_{}_{}",
                                RelationshipType::HasMethod.as_str().to_lowercase(),
                                owner_id,
                                mid
                            );
                            if !emitted.insert(edge_id.clone()) {
                                continue;
                            }
                            relationships.push(GraphRelationship {
                                id: edge_id,
                                source_id: owner_id.to_string(),
                                target_id: mid,
                                rel_type: RelationshipType::HasMethod,
                                confidence: 1.0,
                                reason: "ruby_module_nesting".to_string(),
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

/// `method` / `singleton_method` nodes declared directly in a module body (handling both
/// a `body_statement` wrapper and direct children), without descending into nested
/// classes/modules.
fn module_direct_methods<'t>(module: &tree_sitter::Node<'t>) -> Vec<tree_sitter::Node<'t>> {
    let mut out = Vec::new();
    let mut c = module.walk();
    for child in module.children(&mut c) {
        match child.kind() {
            "method" | "singleton_method" => out.push(child),
            "body_statement" => {
                let mut c2 = child.walk();
                for gc in child.children(&mut c2) {
                    if matches!(gc.kind(), "method" | "singleton_method") {
                        out.push(gc);
                    }
                }
            }
            _ => {}
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use crate::phases::parsing::parse_files;
    use crate::phases::structure::FileEntry;
    use code_explorer_core::config::languages::SupportedLanguage;
    use code_explorer_core::graph::types::*;
    use code_explorer_core::graph::KnowledgeGraph;

    #[test]
    fn test_ruby_module_methods_nest() {
        let content = r#"
module Greeter
  def hello
    "hi"
  end

  def self.version
    1
  end

  class Inner
    def deep
    end
  end
end

class Foo
  def bar
  end
end
"#;
        let file = FileEntry {
            path: "t.rb".to_string(),
            content: content.to_string(),
            size: content.len(),
            language: Some(SupportedLanguage::Ruby),
        };
        let mut graph = KnowledgeGraph::new();
        graph.add_node(GraphNode {
            id: "File:t.rb".to_string(),
            label: NodeLabel::File,
            properties: NodeProperties {
                name: "t.rb".to_string(),
                file_path: "t.rb".to_string(),
                ..Default::default()
            },
        });
        let _ = parse_files(&mut graph, &[file], None).unwrap();

        let has_method = |src: &str, tgt: &str| {
            graph
                .iter_relationships()
                .any(|r| r.rel_type == RelationshipType::HasMethod
                    && r.source_id == src
                    && r.target_id == tgt)
        };

        // Instance and singleton methods directly in the module nest under it.
        assert!(has_method("Module:t.rb:Greeter", "Method:t.rb:hello"), "module instance method nests");
        assert!(has_method("Module:t.rb:Greeter", "Method:t.rb:version"), "module singleton method nests");
        // A method inside a class nested in the module nests under the class, not the module.
        assert!(
            !has_method("Module:t.rb:Greeter", "Method:t.rb:deep"),
            "nested-class method must not nest under the module"
        );
        assert!(has_method("Class:t.rb:Inner", "Method:t.rb:deep"), "nested-class method nests under its class");
        // Sanity: top-level class methods still nest (generic path).
        assert!(has_method("Class:t.rb:Foo", "Method:t.rb:bar"), "class method nests");
    }
}

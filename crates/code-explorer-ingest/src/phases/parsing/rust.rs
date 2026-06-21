use std::collections::{HashMap, HashSet};

use code_explorer_core::graph::types::*;
use code_explorer_core::id::generate_id;

use crate::phases::structure::FileEntry;

/// Rust-specific post-pass: nest methods defined inside `impl`/`trait` blocks under
/// their owning type node via `HasMethod` edges.
///
/// The generic AST-nesting path (`find_enclosing_class_id` + the guard in
/// `create_definition_node`) cannot do this for Rust, for two reasons:
///  1. Rust impl methods are captured as `@definition.function` → labeled `Function`,
///     not `Method`, so the nesting guard at the top of `create_definition_node`
///     (`matches!(label, Method | Property | Constructor)`) skips them entirely.
///  2. An `impl_item` has no `name` field in tree-sitter-rust — it carries `type`
///     and `trait` fields — so `find_enclosing_class_id`'s `child_by_field_name("name")`
///     lookup returns `None` for impl blocks.
///
/// We instead walk the tree, resolve each `impl`/`trait` block to its owning type, and
/// emit `Struct`/`Enum`/`Trait` → `Function` `HasMethod` edges for every `function_item`
/// directly inside the block body. Methods nest under the *type* node (the node every
/// downstream consumer seeds from — e.g. `backend/local.rs`), not the intermediate
/// `Impl` node which would leave them invisible to a struct-seeded traversal.
///
/// Scope: same-file impls only. A cross-file `impl Foo` (type defined in a different
/// file) is skipped here, because the owning `Struct`/`Enum` node isn't in this file's
/// node set to disambiguate Struct vs Enum. A global reconciliation pass is a follow-up.
pub(super) fn post_parse(
    root: tree_sitter::Node,
    file: &FileEntry,
    nodes: &[GraphNode],
    relationships: &mut Vec<GraphRelationship>,
) {
    let content = file.content.as_bytes();

    // name -> owning type node id, for Struct/Enum/Trait nodes defined in this file.
    // Using the real node label keeps the target id matching an existing node and
    // resolves the Struct-vs-Enum ambiguity the `impl` block can't resolve alone.
    let mut type_node_ids: HashMap<&str, &str> = HashMap::new();
    for n in nodes {
        if matches!(
            n.label,
            NodeLabel::Struct | NodeLabel::Enum | NodeLabel::Trait
        ) {
            type_node_ids
                .entry(n.properties.name.as_str())
                .or_insert(n.id.as_str());
        }
    }
    if type_node_ids.is_empty() {
        return;
    }

    let mut emitted: HashSet<String> = HashSet::new();
    let mut stack = vec![root];
    while let Some(node) = stack.pop() {
        let type_name = match node.kind() {
            "impl_item" => rust_impl_type_base_name(&node, content),
            "trait_item" => node
                .child_by_field_name("name")
                .and_then(|n| n.utf8_text(content).ok()),
            _ => None,
        };

        if let Some(type_name) = type_name {
            if let Some(&owner_id) = type_node_ids.get(type_name) {
                if let Some(body) = rust_block_body(&node) {
                    let mut bcur = body.walk();
                    for child in body.children(&mut bcur) {
                        // Only `function_item` carries a body and becomes a Function node;
                        // `function_signature_item` (bodyless trait method) is not extracted.
                        if child.kind() != "function_item" {
                            continue;
                        }
                        let Some(name_node) = child.child_by_field_name("name") else {
                            continue;
                        };
                        let Ok(method_name) = name_node.utf8_text(content) else {
                            continue;
                        };
                        let method_id =
                            generate_id("Function", &format!("{}:{}", file.path, method_name));
                        // Key the edge on the owning type too, so two impls in one file
                        // that both define e.g. `new` each get their own HasMethod edge.
                        // (The target Function node is still shared across same-named
                        // methods in a file — a pre-existing id-model limitation.)
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
                            reason: "rust_impl_nesting".to_string(),
                            step: None,
                        });
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

/// Base type name of an `impl` block: the `type` field, unwrapping a `generic_type`
/// (`impl Foo<T>`) to its base `type_identifier` (`Foo`). Mirrors the `@definition.impl`
/// patterns in `queries/rust_lang.rs` so the resolved name matches the `Impl`/`Struct`
/// node naming.
fn rust_impl_type_base_name<'a>(impl_node: &tree_sitter::Node, content: &'a [u8]) -> Option<&'a str> {
    let type_node = impl_node.child_by_field_name("type")?;
    let base = match type_node.kind() {
        "generic_type" => type_node.child_by_field_name("type")?,
        _ => type_node,
    };
    base.utf8_text(content).ok()
}

/// The `declaration_list` body of an `impl`/`trait` block (via the `body` field, with a
/// defensive fallback to the first `declaration_list` child).
fn rust_block_body<'t>(block: &tree_sitter::Node<'t>) -> Option<tree_sitter::Node<'t>> {
    block.child_by_field_name("body").or_else(|| {
        let mut cur = block.walk();
        let found = block
            .children(&mut cur)
            .find(|ch| ch.kind() == "declaration_list");
        found
    })
}

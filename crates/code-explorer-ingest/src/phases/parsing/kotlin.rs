use code_explorer_core::graph::types::*;

use crate::phases::structure::FileEntry;

/// Kotlin methods are captured as `@definition.function` (→ `Function` label) but live
/// inside a `class_declaration`/`object_declaration` body, so the generic nesting guard
/// (which fires only for `Method | Property | Constructor`) skips them. Nest them under
/// their class via the shared lexical-method nester.
pub(super) fn post_parse(
    root: tree_sitter::Node,
    file: &FileEntry,
    nodes: &[GraphNode],
    relationships: &mut Vec<GraphRelationship>,
) {
    super::nest_function_methods(
        root,
        file,
        nodes,
        relationships,
        &["class_declaration", "object_declaration"],
        "kotlin_class_nesting",
    );
}

#[cfg(test)]
mod tests {
    use crate::phases::parsing::parse_files;
    use crate::phases::structure::FileEntry;
    use code_explorer_core::config::languages::SupportedLanguage;
    use code_explorer_core::graph::types::*;
    use code_explorer_core::graph::KnowledgeGraph;

    #[test]
    fn test_kotlin_extraction_and_nesting() {
        let content = "package p\n\nclass Foo {\n    val y = 1\n    fun bar(): Int {\n        return 1\n    }\n    fun baz() {}\n}\n\nobject Sing {\n    fun ping() {}\n}\n\nfun topLevel() {}\n";
        let file = FileEntry {
            path: "k.kt".to_string(),
            content: content.to_string(),
            size: content.len(),
            language: Some(SupportedLanguage::Kotlin),
        };
        let mut graph = KnowledgeGraph::new();
        graph.add_node(GraphNode {
            id: "File:k.kt".to_string(),
            label: NodeLabel::File,
            properties: NodeProperties {
                name: "k.kt".to_string(),
                file_path: "k.kt".to_string(),
                ..Default::default()
            },
        });
        let _ = parse_files(&mut graph, &[file], None).unwrap();

        // Extraction works (query compiles against kotlin-ng).
        assert!(graph.get_node("Class:k.kt:Foo").is_some(), "class extracted");
        assert!(graph.get_node("Function:k.kt:bar").is_some(), "method extracted");
        assert!(graph.get_node("Property:k.kt:y").is_some(), "property extracted");

        // Methods nest under their class/object.
        let has = |src: &str, tgt: &str| {
            graph.iter_relationships().any(|r| r.rel_type == RelationshipType::HasMethod
                && r.source_id == src
                && r.target_id == tgt)
        };
        assert!(has("Class:k.kt:Foo", "Function:k.kt:bar"), "method bar nests");
        assert!(has("Class:k.kt:Foo", "Function:k.kt:baz"), "method baz nests");
        assert!(has("Class:k.kt:Sing", "Function:k.kt:ping"), "object method nests");
        assert!(
            !graph.iter_relationships().any(|r| r.rel_type == RelationshipType::HasMethod
                && r.target_id == "Function:k.kt:topLevel"),
            "top-level function must not nest"
        );
    }
}

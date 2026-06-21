//! Phase 1b: TODO/FIXME inventory.
//!
//! Scans every known source file for line-comment markers (`TODO`, `FIXME`,
//! `HACK`, `XXX`) and emits `NodeLabel::TodoMarker` nodes anchored via
//! `BelongsTo` to the enclosing File.
//!
//! Why post-parsing rather than inside the tree-sitter pass?
//! - It's language-agnostic: even Kotlin/Swift (fallback-grammar skip path in
//!   the parser) still get their TODOs indexed.
//! - It's cheap: one `lines()` scan per file, no AST cost.
//! - The BelongsTo edge is simple — we don't try to attribute TODOs to
//!   individual methods (that would require walking the graph post-parse and
//!   is the kind of thing LLM enrichment can do later).

use code_explorer_core::graph::types::{
    GraphNode, GraphRelationship, NodeLabel, NodeProperties, RelationshipType,
};
use code_explorer_core::graph::KnowledgeGraph;
use code_explorer_core::id::generate_id;
use rayon::prelude::*;

use crate::phases::structure::FileEntry;

/// Number of TodoMarker nodes and edges produced.
#[derive(Debug, Default, Clone, Copy)]
pub struct TodoStats {
    pub markers: usize,
}

/// Scan every parsed source file for TODO/FIXME/HACK/XXX markers.
pub fn scan_todos(graph: &mut KnowledgeGraph, files: &[FileEntry]) -> TodoStats {
    // Parallel scan: one thread per file produces its own (nodes, edges) list.
    let per_file: Vec<(Vec<GraphNode>, Vec<GraphRelationship>)> =
        files.par_iter().map(scan_single_file).collect();

    let mut markers = 0usize;
    for (nodes, rels) in per_file {
        markers += nodes.len();
        for n in nodes {
            graph.add_node(n);
        }
        for r in rels {
            graph.add_relationship(r);
        }
    }
    TodoStats { markers }
}

fn scan_single_file(file: &FileEntry) -> (Vec<GraphNode>, Vec<GraphRelationship>) {
    let mut nodes = Vec::new();
    let mut rels = Vec::new();

    // Skip obviously non-source files we might have in the entry list.
    if file.content.is_empty() {
        return (nodes, rels);
    }

    let file_node_id = generate_id("File", &file.path);

    for (line_idx, line) in file.content.lines().enumerate() {
        let Some((kind, text)) = find_marker(line) else {
            continue;
        };

        let line_num = (line_idx + 1) as u32;
        // Stable ID: File path + line + kind so re-running is idempotent.
        let todo_id = generate_id(
            "TodoMarker",
            &format!("{}:{}:{}", file.path, line_num, kind),
        );

        nodes.push(GraphNode {
            id: todo_id.clone(),
            label: NodeLabel::TodoMarker,
            properties: NodeProperties {
                name: format!("{kind} @ {}:{}", file.path, line_num),
                file_path: file.path.clone(),
                start_line: Some(line_num),
                end_line: Some(line_num),
                language: file.language,
                todo_kind: Some(kind.to_string()),
                todo_text: Some(text.to_string()),
                ..Default::default()
            },
        });

        rels.push(GraphRelationship {
            id: format!("todo_belongs_{}", todo_id),
            source_id: todo_id,
            target_id: file_node_id.clone(),
            rel_type: RelationshipType::BelongsTo,
            confidence: 1.0,
            reason: "todo_scan".to_string(),
            step: None,
        });
    }

    (nodes, rels)
}

/// Detect a TODO-style marker in a source line. Returns `(kind, trailing_text)`.
///
/// Kinds: TODO, FIXME, HACK, XXX. We look for the marker **after** a common
/// comment start (`//`, `/*`, `#`, `--`, `;`, `<!--`) or at the start of a
/// trimmed line, because scanning for bare `TODO` anywhere would misfire on
/// identifiers like `TODO_STATUS_PENDING`.
fn find_marker(line: &str) -> Option<(&'static str, &str)> {
    let trimmed = line.trim_start();

    // Identify comment start
    let after_comment: &str = if let Some(rest) = trimmed.strip_prefix("//") {
        rest
    } else if let Some(rest) = trimmed.strip_prefix("/*") {
        rest
    } else if let Some(rest) = trimmed.strip_prefix('#') {
        rest
    } else if let Some(rest) = trimmed.strip_prefix("--") {
        rest
    } else if let Some(rest) = trimmed.strip_prefix("<!--") {
        rest
    } else if let Some(rest) = trimmed.strip_prefix(';') {
        rest
    } else if trimmed.starts_with("* ") || trimmed.starts_with('*') {
        // JSDoc / block comment continuation
        trimmed.trim_start_matches('*')
    } else {
        // Not a comment — skip. We specifically do NOT want to match TODO
        // in string literals or identifiers.
        return None;
    };

    let head = after_comment.trim_start();

    for kind in ["TODO", "FIXME", "HACK", "XXX"] {
        if let Some(after) = head.strip_prefix(kind) {
            // Next char must be a non-alphanumeric separator so `TODOS` or
            // `FIXMEish` don't trigger.
            if after.is_empty() || after.starts_with(|c: char| !c.is_alphanumeric() && c != '_') {
                // Strip leading `:` and spaces.
                let text =
                    after.trim_start_matches(|c: char| c == ':' || c == '(' || c.is_whitespace());
                // Cap to keep snapshot JSON small. Snap to a char boundary so a
                // non-ASCII byte at index 500 doesn't panic the slice.
                let trimmed_text = if text.len() > 500 {
                    &text[..super::text_util::floor_char_boundary(text, 500)]
                } else {
                    text
                };
                return Some((kind, trimmed_text.trim_end()));
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use code_explorer_core::config::languages::SupportedLanguage;

    fn fe(path: &str, content: &str) -> FileEntry {
        FileEntry {
            path: path.to_string(),
            content: content.to_string(),
            language: Some(SupportedLanguage::Rust),
            size: content.len(),
        }
    }

    #[test]
    fn test_find_marker_todo() {
        assert_eq!(find_marker("// TODO: fix this"), Some(("TODO", "fix this")));
    }

    #[test]
    fn test_find_marker_fixme_hash() {
        assert_eq!(
            find_marker("# FIXME broken logic"),
            Some(("FIXME", "broken logic"))
        );
    }

    #[test]
    fn test_find_marker_false_positive_identifier() {
        // `TODO_STATUS` should not match because the marker is inside a
        // non-comment identifier.
        assert_eq!(find_marker("let TODO_STATUS = 1;"), None);
    }

    #[test]
    fn test_find_marker_todos_no_match() {
        assert_eq!(find_marker("// TODOS list"), None);
    }

    #[test]
    fn test_find_marker_jsdoc() {
        assert_eq!(
            find_marker(" * HACK: inline for perf"),
            Some(("HACK", "inline for perf"))
        );
    }

    #[test]
    fn test_scan_single_file() {
        // Only leading-comment markers count; trailing `// XXX` after code
        // is intentionally ignored to avoid false positives on XML-like tags.
        let file = fe(
            "src/a.rs",
            "fn foo() {\n    // TODO: implement\n    // FIXME: broken\n    // XXX deprecated\n    let x = 1;\n}",
        );
        let (nodes, rels) = scan_single_file(&file);
        assert_eq!(nodes.len(), 3);
        assert_eq!(rels.len(), 3);
        let kinds: Vec<_> = nodes
            .iter()
            .filter_map(|n| n.properties.todo_kind.clone())
            .collect();
        assert!(kinds.contains(&"TODO".to_string()));
        assert!(kinds.contains(&"FIXME".to_string()));
        assert!(kinds.contains(&"XXX".to_string()));
    }

    #[test]
    fn test_scan_todos_into_graph() {
        let mut g = KnowledgeGraph::new();
        let f = fe("src/a.rs", "// TODO: x\nconst y = 1;");
        let stats = scan_todos(&mut g, &[f]);
        assert_eq!(stats.markers, 1);
        let todos: Vec<_> = g
            .iter_nodes()
            .filter(|n| n.label == NodeLabel::TodoMarker)
            .collect();
        assert_eq!(todos.len(), 1);
    }
}

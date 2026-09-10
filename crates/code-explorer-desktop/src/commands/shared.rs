//! Shared helpers for Tauri command modules.

use crate::types::CytoNode;

/// Convert a graph node to a Cytoscape-compatible node (no depth).
pub fn node_to_cyto(node: &code_explorer_core::graph::types::GraphNode) -> CytoNode {
    node_to_cyto_with_depth(node, None)
}

/// Convert a graph node to a Cytoscape-compatible node with optional depth info.
pub fn node_to_cyto_with_depth(
    node: &code_explorer_core::graph::types::GraphNode,
    depth: Option<u32>,
) -> CytoNode {
    CytoNode {
        id: node.id.clone(),
        label: node.label.as_str().to_string(),
        name: node.properties.name.clone(),
        file_path: node.properties.file_path.clone(),
        start_line: node.properties.start_line,
        end_line: node.properties.end_line,
        is_exported: node.properties.is_exported,
        community: node.properties.heuristic_label.clone(),
        // Use the canonical short name from `as_str()` ("cpp", "csharp",
        // "php", ...) instead of the Debug variant name ("CPlusPlus",
        // "CSharp", "Php"). The frontend syntax highlighter and any
        // language-keyed filters expect the short names.
        language: node.properties.language.map(|l| l.as_str().to_string()),
        description: node.properties.description.clone(),
        parameter_count: node.properties.parameter_count,
        return_type: node.properties.return_type.clone(),
        layer_type: node.properties.layer_type.clone(),
        entry_point_score: node.properties.entry_point_score,
        entry_point_reason: node.properties.entry_point_reason.clone(),
        is_traced: node.properties.is_traced,
        trace_call_count: node.properties.trace_call_count,
        is_dead_candidate: node.properties.is_dead_candidate,
        complexity: node.properties.complexity,
        depth,
    }
}

//! Shared trace parsing and method resolution utilities.
//!
//! Used by both the CLI `trace-doc` command and the MCP `analyze_execution_trace` tool
//! to avoid code duplication.

use std::collections::HashMap;
use std::path::Path;

use once_cell::sync::Lazy;
use regex::Regex;
use serde_json::Value;

use crate::graph::KnowledgeGraph;

// ─── Cached regexes for C# custom log format ───────────────────────────

static METHOD_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"\[([a-zA-Z0-9_.]+\.[a-zA-Z0-9_]+)\]").unwrap());

static PARAM_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"Variable '([^']+)'\s+\(Type:\s+[^)]+\)\s+=\s+(.*)").unwrap());

static RESULT_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"\] RESULT \| (.*)").unwrap());

// ─── Trace parsing ─────────────────────────────────────────────────────

/// Parse an execution trace from its text content.
///
/// Supports three formats:
/// - JSON array: `[{...}, {...}]`
/// - NDJSON: one JSON object per line
/// - C# custom log format: `[Class.Method] START`, `Variable '...'`, `] RESULT |`
pub fn parse_trace(content: &str) -> anyhow::Result<Vec<Value>> {
    let trimmed = content.trim();
    // JSON array: must start with `[` followed by `{` or whitespace+`{`
    if trimmed.starts_with('[') {
        if let Ok(arr) = serde_json::from_str::<Vec<Value>>(trimmed) {
            return Ok(arr);
        }
        // Not valid JSON array — fall through to line-by-line parsing
    }

    // Line-by-line: try NDJSON first, fall back to C# custom parsing
    let mut steps = Vec::new();
    let mut current_method = String::new();
    let mut current_params = serde_json::Map::new();
    let mut has_custom_format = false;

    for line in content.lines() {
        if line.trim().is_empty() {
            continue;
        }

        // Try standard JSON line first
        if line.trim().starts_with('{') {
            if let Ok(val) = serde_json::from_str::<Value>(line) {
                steps.push(val);
                continue;
            }
        }

        // C# custom log parsing below
        has_custom_format = true;

        // Extract variables/parameters
        if line.contains("Variable '") {
            if let Some(captures) = PARAM_RE.captures(line) {
                if let (Some(name), Some(val)) = (captures.get(1), captures.get(2)) {
                    current_params.insert(
                        name.as_str().to_string(),
                        Value::String(val.as_str().to_string()),
                    );
                }
            }
        }

        // Extract method name on START
        let is_start = line.contains(" START ") || line.contains("] START");
        if is_start {
            if let Some(captures) = METHOD_RE.captures(line) {
                if let Some(m) = captures.get(1) {
                    let method = m.as_str().to_string();
                    if method.contains('.') && !method.ends_with(".cs") {
                        if !current_method.is_empty() {
                            steps.push(serde_json::json!({
                                "method": current_method,
                                "params": current_params.clone(),
                            }));
                        }
                        current_method = method;
                        current_params = serde_json::Map::new();
                    }
                }
            }
        }

        // EF Insert detection
        if line.contains("===== EF INSERT =====") {
            if !current_method.is_empty() {
                steps.push(serde_json::json!({
                    "method": current_method,
                    "params": current_params.clone(),
                }));
                current_params = serde_json::Map::new();
                current_method = String::new();
            }
            steps.push(serde_json::json!({
                "method": "Database.Insert",
                "sql": line.to_string(),
            }));
        }

        // Extract return/results
        if line.contains("] RESULT |") {
            if let Some(captures) = RESULT_RE.captures(line) {
                if let Some(res) = captures.get(1) {
                    current_params.insert(
                        "return_value".to_string(),
                        Value::String(res.as_str().to_string()),
                    );
                }
            }
        }
    }

    // Flush last pending method
    if !current_method.is_empty() {
        steps.push(serde_json::json!({
            "method": current_method,
            "params": current_params.clone(),
        }));
    }

    // If we found no custom format lines either, the file might be unparseable
    if steps.is_empty() && !has_custom_format {
        anyhow::bail!("No valid trace steps found in content");
    }

    Ok(steps)
}

// ─── Name-to-node index ────────────────────────────────────────────────

/// Build a lookup map from node name → vec of node IDs.
pub fn build_name_index(graph: &KnowledgeGraph) -> HashMap<String, Vec<String>> {
    let mut index: HashMap<String, Vec<String>> = HashMap::new();
    for node in graph.iter_nodes() {
        index
            .entry(node.properties.name.clone())
            .or_default()
            .push(node.id.clone());
    }
    index
}

// ─── Method resolution ─────────────────────────────────────────────────

/// Resolve a dotted method name (e.g. `"MyService.DoWork"`) to the best-matching
/// graph node ID.
///
/// Splits on `.`, uses the last part as method name and the first part as a class
/// hint. If a class hint is available, prefers a node whose `file_path` contains
/// the class name.
pub fn resolve_method_node(
    graph: &KnowledgeGraph,
    name_index: &HashMap<String, Vec<String>>,
    full_method_name: &str,
) -> Option<String> {
    let parts: Vec<&str> = full_method_name.split('.').collect();
    let method_name = *parts.last().unwrap_or(&full_method_name);
    // For "MyApp.Services.MyService.DoWork", the class is the second-to-last segment
    let class_name = if parts.len() > 1 {
        Some(parts[parts.len() - 2])
    } else {
        None
    };

    let ids = name_index.get(method_name)?;

    // Try to disambiguate by class name in file_path
    let mut best_id = None;
    if let Some(c_name) = class_name {
        for id in ids {
            if let Some(n) = graph.get_node(id) {
                if n.properties.file_path.contains(c_name) {
                    best_id = Some(id.clone());
                    break;
                }
            }
        }
    }

    Some(best_id.unwrap_or_else(|| ids.first().unwrap().clone()))
}

// ─── Source code extraction ────────────────────────────────────────────

/// Read source lines from a file given 1-indexed start/end line numbers.
pub fn extract_source_lines(file_path: &Path, start: u32, end: u32) -> Option<String> {
    // Reject inverted ranges. Without this guard, `end.saturating_sub(start)`
    // returns 0 when end < start, then `.saturating_add(1)` makes it 1, so we
    // would silently return one line at `start` instead of nothing — which
    // misleads downstream evidence collection (process_doc.rs, trace_doc.rs)
    // about which lines actually correspond to a step. Fail closed instead.
    if end < start {
        return None;
    }
    let content = std::fs::read_to_string(file_path).ok()?;
    let line_count = end.saturating_sub(start).saturating_add(1) as usize;
    let lines: Vec<&str> = content
        .lines()
        .skip((start as usize).saturating_sub(1))
        .take(line_count)
        .collect();
    if lines.is_empty() {
        None
    } else {
        Some(lines.join("\n"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::types::*;

    fn make_graph() -> KnowledgeGraph {
        let mut g = KnowledgeGraph::new();
        g.add_node(GraphNode {
            id: "Method:src/services/MyService.cs:DoWork".to_string(),
            label: NodeLabel::Method,
            properties: NodeProperties {
                name: "DoWork".to_string(),
                file_path: "src/services/MyService.cs".to_string(),
                start_line: Some(10),
                end_line: Some(20),
                ..Default::default()
            },
        });
        g.add_node(GraphNode {
            id: "Method:src/controllers/HomeController.cs:Index".to_string(),
            label: NodeLabel::Method,
            properties: NodeProperties {
                name: "Index".to_string(),
                file_path: "src/controllers/HomeController.cs".to_string(),
                start_line: Some(5),
                end_line: Some(15),
                ..Default::default()
            },
        });
        g.add_node(GraphNode {
            id: "Method:src/other/OtherService.cs:DoWork".to_string(),
            label: NodeLabel::Method,
            properties: NodeProperties {
                name: "DoWork".to_string(),
                file_path: "src/other/OtherService.cs".to_string(),
                start_line: Some(1),
                end_line: Some(5),
                ..Default::default()
            },
        });
        g
    }

    #[test]
    fn test_parse_json_array() {
        let content = r#"[{"method": "Foo.Bar"}, {"method": "Baz.Qux"}]"#;
        let steps = parse_trace(content).unwrap();
        assert_eq!(steps.len(), 2);
        assert_eq!(steps[0]["method"], "Foo.Bar");
    }

    #[test]
    fn test_parse_ndjson() {
        let content = "{\"method\": \"Foo.Bar\"}\n{\"method\": \"Baz.Qux\"}\n";
        let steps = parse_trace(content).unwrap();
        assert_eq!(steps.len(), 2);
    }

    #[test]
    fn test_parse_csharp_custom() {
        let content = "\
[MyService.DoWork] START | 2024-01-01\n\
Variable 'input' (Type: String) = hello\n\
] RESULT | success\n\
[HomeController.Index] START | 2024-01-01\n\
Variable 'id' (Type: Int32) = 42\n";
        let steps = parse_trace(content).unwrap();
        assert_eq!(steps.len(), 2);
        assert_eq!(steps[0]["method"], "MyService.DoWork");
        assert_eq!(steps[0]["params"]["input"], "hello");
        assert_eq!(steps[0]["params"]["return_value"], "success");
        assert_eq!(steps[1]["method"], "HomeController.Index");
    }

    #[test]
    fn test_parse_ef_insert() {
        let content = "\
[MyService.Save] START\n\
===== EF INSERT =====\n\
[Other.Method] START\n";
        let steps = parse_trace(content).unwrap();
        assert!(steps.iter().any(|s| s["method"] == "Database.Insert"));
    }

    #[test]
    fn test_build_name_index() {
        let graph = make_graph();
        let index = build_name_index(&graph);
        assert_eq!(index["DoWork"].len(), 2);
        assert_eq!(index["Index"].len(), 1);
    }

    #[test]
    fn test_resolve_method_with_class_hint() {
        let graph = make_graph();
        let index = build_name_index(&graph);

        // Should prefer MyService.cs for "MyService.DoWork"
        let id = resolve_method_node(&graph, &index, "MyService.DoWork").unwrap();
        assert!(id.contains("MyService"));

        // Should prefer OtherService.cs for "OtherService.DoWork"
        let id = resolve_method_node(&graph, &index, "OtherService.DoWork").unwrap();
        assert!(id.contains("OtherService"));
    }

    #[test]
    fn test_resolve_namespace_qualified_method() {
        let graph = make_graph();
        let index = build_name_index(&graph);

        // Fully qualified C# name: second-to-last segment is the class
        let id = resolve_method_node(&graph, &index, "MyApp.Services.MyService.DoWork").unwrap();
        assert!(
            id.contains("MyService"),
            "Expected MyService node, got {}",
            id
        );
    }

    #[test]
    fn test_resolve_method_without_class() {
        let graph = make_graph();
        let index = build_name_index(&graph);

        // Without class hint, should return first match
        let id = resolve_method_node(&graph, &index, "Index").unwrap();
        assert!(id.contains("Index"));
    }

    #[test]
    fn test_resolve_unknown_method() {
        let graph = make_graph();
        let index = build_name_index(&graph);
        assert!(resolve_method_node(&graph, &index, "NonExistent.Foo").is_none());
    }
}

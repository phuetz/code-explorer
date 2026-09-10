//! MCP resource definitions.
//!
//! Resources expose graph data through URI templates that MCP clients
//! can browse and read.

use serde_json::{json, Value};

use code_explorer_core::graph::types::NodeLabel;
use code_explorer_core::storage::repo_manager::{registry_entry_id, RegistryEntry};

/// Return the list of MCP resource templates and static resources.
pub fn resource_definitions() -> Value {
    json!({
        "resources": [
            {
                "uri": "code-explorer://version",
                "name": "Code Explorer Version",
                "description": "Current Code Explorer version and build information",
                "mimeType": "application/json"
            },
            {
                "uri": "code-explorer://help",
                "name": "Code Explorer Help",
                "description": "Usage guide and available tools",
                "mimeType": "text/plain"
            }
        ],
        "resourceTemplates": [
            {
                "uriTemplate": "code-explorer://repos",
                "name": "Repository List",
                "description": "List all indexed repositories with stats"
            },
            {
                "uriTemplate": "code-explorer://repos/{repo}/schema",
                "name": "Graph Schema",
                "description": "Node and relationship types in the knowledge graph for a repository"
            },
            {
                "uriTemplate": "code-explorer://repos/{repo}/stats",
                "name": "Repository Stats",
                "description": "Statistics about the indexed knowledge graph"
            },
            {
                "uriTemplate": "code-explorer://repos/{repo}/communities",
                "name": "Community Summary",
                "description": "Detected code communities and their descriptions"
            },
            {
                "uriTemplate": "code-explorer://repos/{repo}/processes",
                "name": "Process Flows",
                "description": "Detected execution flows and their step sequences"
            },
            {
                "uriTemplate": "code-explorer://repos/{repo}/files/{path}",
                "name": "File Details",
                "description": "Detailed information about a specific file node"
            }
        ]
    })
}

fn find_registry_entry<'a>(
    registry: &'a [RegistryEntry],
    repo_name_or_id: &str,
) -> Option<&'a RegistryEntry> {
    let lower = repo_name_or_id.to_lowercase();
    registry.iter().find(|e| {
        if registry_entry_id(e) == repo_name_or_id {
            return true;
        }

        // Match by exact name first; fall back to path that ends with
        // `/repo_name` (or `\repo_name`) on a segment boundary so a
        // user passing "foo" doesn't accidentally select "myfoo".
        e.name.eq_ignore_ascii_case(repo_name_or_id) || {
            let path_lower = e.path.to_lowercase().replace('\\', "/");
            path_lower == lower || path_lower.ends_with(&format!("/{}", lower))
        }
    })
}

/// Read a resource by URI and return its contents.
///
/// Static resources (version, help) don't need registry access.
/// Template resources resolve against the registry and load graph data.
pub fn read_resource(uri: &str, registry: &[RegistryEntry]) -> Option<Value> {
    // Static resources
    match uri {
        "code-explorer://version" => {
            return Some(json!({
                "contents": [{
                    "uri": uri,
                    "mimeType": "application/json",
                    "text": serde_json::to_string_pretty(&json!({
                        "name": "code-explorer",
                        "version": env!("CARGO_PKG_VERSION"),
                        "runtime": "rust",
                        "description": "Graph-powered code intelligence for AI agents"
                    })).unwrap_or_default()
                }]
            }));
        }
        "code-explorer://help" => {
            return Some(json!({
                "contents": [{
                    "uri": uri,
                    "mimeType": "text/plain",
                    "text": HELP_TEXT
                }]
            }));
        }
        "code-explorer://repos" => {
            let repos: Vec<Value> = registry
                .iter()
                .map(|e| {
                    json!({
                        "id": registry_entry_id(e),
                        "name": e.name,
                        "path": e.path,
                        "indexedAt": e.indexed_at,
                    })
                })
                .collect();
            return Some(json!({
                "contents": [{
                    "uri": uri,
                    "mimeType": "application/json",
                    "text": serde_json::to_string_pretty(&repos).unwrap_or_default()
                }]
            }));
        }
        _ => {}
    }

    // Template resources: code-explorer://repos/{repo}/...
    if let Some(rest) = uri.strip_prefix("code-explorer://repos/") {
        let parts: Vec<&str> = rest.splitn(3, '/').collect();
        if parts.is_empty() {
            return None;
        }

        let repo_name = parts[0];
        let entry = find_registry_entry(registry, repo_name)?;

        let snap_path = std::path::Path::new(&entry.storage_path).join("graph.bin");
        let graph = code_explorer_db::snapshot::load_snapshot(&snap_path).ok()?;

        let resource_type = parts.get(1).copied().unwrap_or("");

        match resource_type {
            "schema" => {
                let mut label_counts = std::collections::HashMap::new();
                for node in graph.iter_nodes() {
                    *label_counts
                        .entry(node.label.as_str().to_string())
                        .or_insert(0u32) += 1;
                }
                let mut rel_counts = std::collections::HashMap::new();
                for rel in graph.iter_relationships() {
                    *rel_counts
                        .entry(rel.rel_type.as_str().to_string())
                        .or_insert(0u32) += 1;
                }
                let text = serde_json::to_string_pretty(&json!({
                    "nodeLabels": label_counts,
                    "relationshipTypes": rel_counts,
                }))
                .unwrap_or_default();
                Some(
                    json!({ "contents": [{ "uri": uri, "mimeType": "application/json", "text": text }] }),
                )
            }
            "stats" => {
                let node_count = graph.iter_nodes().count();
                let edge_count = graph.iter_relationships().count();
                let file_count = graph
                    .iter_nodes()
                    .filter(|n| n.label == NodeLabel::File)
                    .count();
                let text = serde_json::to_string_pretty(&json!({
                    "nodes": node_count,
                    "edges": edge_count,
                    "files": file_count,
                    "density": if node_count > 0 { edge_count as f64 / node_count as f64 } else { 0.0 },
                }))
                .unwrap_or_default();
                Some(
                    json!({ "contents": [{ "uri": uri, "mimeType": "application/json", "text": text }] }),
                )
            }
            "communities" => {
                let communities: Vec<Value> = graph
                    .iter_nodes()
                    .filter(|n| n.label == NodeLabel::Community)
                    .map(|n| {
                        json!({
                            "name": n.properties.name,
                            "description": n.properties.description,
                            "heuristicLabel": n.properties.heuristic_label,
                            "cohesion": n.properties.cohesion,
                        })
                    })
                    .collect();
                let text = serde_json::to_string_pretty(&communities).unwrap_or_default();
                Some(
                    json!({ "contents": [{ "uri": uri, "mimeType": "application/json", "text": text }] }),
                )
            }
            "processes" => {
                let processes: Vec<Value> = graph
                    .iter_nodes()
                    .filter(|n| n.label == NodeLabel::Process)
                    .map(|n| {
                        json!({
                            "name": n.properties.name,
                            "description": n.properties.description,
                        })
                    })
                    .collect();
                let text = serde_json::to_string_pretty(&processes).unwrap_or_default();
                Some(
                    json!({ "contents": [{ "uri": uri, "mimeType": "application/json", "text": text }] }),
                )
            }
            "files" => {
                // code-explorer://repos/{repo}/files/{path}
                let file_path = parts.get(2).copied().unwrap_or("");
                if file_path.contains("..") {
                    return None;
                }
                let node = graph
                    .iter_nodes()
                    .find(|n| n.label == NodeLabel::File && n.properties.file_path == file_path);
                match node {
                    Some(n) => {
                        let text = serde_json::to_string_pretty(&json!({
                            "id": n.id,
                            "name": n.properties.name,
                            "filePath": n.properties.file_path,
                            "startLine": n.properties.start_line,
                            "endLine": n.properties.end_line,
                        }))
                        .unwrap_or_default();
                        Some(
                            json!({ "contents": [{ "uri": uri, "mimeType": "application/json", "text": text }] }),
                        )
                    }
                    None => None,
                }
            }
            _ => None,
        }
    } else {
        None
    }
}

const HELP_TEXT: &str = r#"Code Explorer - Graph-powered code intelligence for AI agents

Available MCP Tools (15):
  list_repos              - List all indexed repositories
  query                   - Natural language search across the knowledge graph
  context                 - 360-degree view of a code symbol
  impact                  - Blast radius analysis for a code change
  detect_changes          - Detect uncommitted changes
  rename                  - Analyze impact of renaming a symbol
  cypher                  - Execute raw Cypher queries (read-only)
  hotspots                - File-level churn analysis
  coupling                - Temporal coupling between files
  ownership               - Code ownership by author
  coverage                - Tracing coverage and dead code detection
  diagram                 - Generate Mermaid diagrams
  report                  - Code health report (grade A-E)
  business                - Get business process documentation
  analyze_execution_trace - Map execution trace to source code steps

Cypher Operators:
  WHERE: =, <>, !=, CONTAINS, STARTS WITH, ENDS WITH
  Logic: AND, OR, NOT
  Return: DISTINCT, count()
  Clauses: ORDER BY, LIMIT

Getting Started:
  1. Index a repository: code-explorer analyze /path/to/repo
  2. Start MCP server: code-explorer mcp
  3. Query from your editor or AI agent
"#;

#[cfg(test)]
mod tests {
    use super::*;

    fn registry_entry(name: &str, path: &str) -> RegistryEntry {
        RegistryEntry {
            name: name.to_string(),
            path: path.to_string(),
            storage_path: format!("{path}/.codeexplorer"),
            indexed_at: "2026-05-06T05:00:00Z".to_string(),
            last_commit: "unknown".to_string(),
            stats: None,
        }
    }

    #[test]
    fn test_resource_definitions() {
        let defs = resource_definitions();
        let resources = defs["resources"].as_array().unwrap();
        let templates = defs["resourceTemplates"].as_array().unwrap();
        assert_eq!(resources.len(), 2);
        assert_eq!(templates.len(), 6);
    }

    #[test]
    fn test_read_version_resource() {
        let result = read_resource("code-explorer://version", &[]);
        assert!(result.is_some());
        let contents = result.unwrap();
        assert!(contents["contents"][0]["text"]
            .as_str()
            .unwrap()
            .contains("code-explorer"));
    }

    #[test]
    fn test_read_help_resource() {
        let result = read_resource("code-explorer://help", &[]);
        assert!(result.is_some());
    }

    #[test]
    fn test_read_repos_empty_registry() {
        let result = read_resource("code-explorer://repos", &[]);
        assert!(result.is_some());
        let text = result.unwrap()["contents"][0]["text"]
            .as_str()
            .unwrap()
            .to_string();
        assert_eq!(text, "[]");
    }

    #[test]
    fn test_read_repos_includes_public_id() {
        let entry = registry_entry("code-explorer", "D:/Repos/code-explorer");
        let expected_id = registry_entry_id(&entry);
        let result = read_resource("code-explorer://repos", &[entry]).unwrap();
        let text = result["contents"][0]["text"].as_str().unwrap();

        assert!(text.contains(&expected_id));
    }

    #[test]
    fn test_find_registry_entry_matches_public_id() {
        let entry = registry_entry("code-explorer", "D:/Repos/code-explorer");
        let id = registry_entry_id(&entry);
        let registry = vec![entry];

        let resolved = find_registry_entry(&registry, &id).unwrap();

        assert_eq!(resolved.name, "code-explorer");
    }

    #[test]
    fn test_read_unknown_resource() {
        assert!(read_resource("code-explorer://unknown", &[]).is_none());
    }
}

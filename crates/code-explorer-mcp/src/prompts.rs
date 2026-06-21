//! MCP prompt templates.
//!
//! Provides structured prompts that MCP clients can use to guide
//! their interactions with the knowledge graph.

use serde_json::{json, Value};

/// Return the list of available MCP prompts.
pub fn prompt_definitions() -> Value {
    json!({
        "prompts": [
            {
                "name": "detect_impact",
                "description": "Analyze the blast radius of a proposed code change",
                "arguments": [
                    {
                        "name": "target",
                        "description": "The symbol name or file path being changed",
                        "required": true
                    },
                    {
                        "name": "change_description",
                        "description": "Description of the proposed change",
                        "required": false
                    }
                ]
            },
            {
                "name": "generate_map",
                "description": "Generate a high-level map of the codebase architecture",
                "arguments": [
                    {
                        "name": "repo",
                        "description": "Repository name (optional if only one is indexed)",
                        "required": false
                    },
                    {
                        "name": "focus",
                        "description": "Optional area to focus on (e.g., 'authentication', 'database')",
                        "required": false
                    }
                ]
            },
            {
                "name": "analyze_hotspots",
                "description": "Identify the most problematic files based on churn, coupling, and ownership",
                "arguments": [
                    {
                        "name": "repo",
                        "description": "Repository name (optional if only one is indexed)",
                        "required": false
                    }
                ]
            },
            {
                "name": "find_dead_code",
                "description": "Find methods with zero incoming calls (dead code candidates) and assess tracing coverage",
                "arguments": [
                    {
                        "name": "target",
                        "description": "Optional class/service name to scope the analysis",
                        "required": false
                    },
                    {
                        "name": "repo",
                        "description": "Repository name (optional if only one is indexed)",
                        "required": false
                    }
                ]
            },
            {
                "name": "trace_dependencies",
                "description": "Trace the full dependency chain for a symbol to understand its role in the system",
                "arguments": [
                    {
                        "name": "symbol",
                        "description": "The symbol to trace",
                        "required": true
                    },
                    {
                        "name": "repo",
                        "description": "Repository name (optional if only one is indexed)",
                        "required": false
                    }
                ]
            },
            {
                "name": "describe_process",
                "description": "Analyze and describe a high-level business process (e.g., payments, calculation engines)",
                "arguments": [
                    {
                        "name": "process",
                        "description": "The process name to analyze (e.g., 'courriers', 'paiements', 'baremes')",
                        "required": true
                    },
                    {
                        "name": "repo",
                        "description": "Repository name (optional if only one is indexed)",
                        "required": false
                    }
                ]
            }
        ]
    })
}

/// Get a prompt by name with the given arguments.
pub fn get_prompt(name: &str, args: &Value) -> Option<Value> {
    match name {
        "detect_impact" => {
            let target = args
                .get("target")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");
            let change_desc = args
                .get("change_description")
                .and_then(|v| v.as_str())
                .unwrap_or("unspecified change");

            Some(json!({
                "messages": [
                    {
                        "role": "user",
                        "content": {
                            "type": "text",
                            "text": format!(
                                "I want to analyze the impact of changing `{target}`. \
                                 The proposed change is: {change_desc}\n\n\
                                 Please:\n\
                                 1. Use the `impact` tool to find upstream and downstream dependencies\n\
                                 2. Use the `context` tool to understand the symbol's role\n\
                                 3. Summarize which files and functions would be affected\n\
                                 4. Highlight any high-risk areas (heavily depended-upon code)\n\
                                 5. Suggest a safe order for making the changes"
                            )
                        }
                    }
                ]
            }))
        }
        "generate_map" => {
            let repo = args
                .get("repo")
                .and_then(|v| v.as_str())
                .map(|r| format!(" for repository '{r}'"))
                .unwrap_or_default();
            let focus = args
                .get("focus")
                .and_then(|v| v.as_str())
                .map(|f| format!(" Focus on the '{f}' area."))
                .unwrap_or_default();

            Some(json!({
                "messages": [
                    {
                        "role": "user",
                        "content": {
                            "type": "text",
                            "text": format!(
                                "Generate a high-level architecture map{repo}.{focus}\n\n\
                                 Please:\n\
                                 1. Use `list_repos` to see available repositories\n\
                                 2. Use `cypher` to query communities: \
                                    MATCH (c:Community) RETURN c.name, c.description, c.symbolCount ORDER BY c.symbolCount DESC\n\
                                 3. Use `cypher` to query key processes: \
                                    MATCH (p:Process) RETURN p.name, p.description, p.stepCount ORDER BY p.stepCount DESC\n\
                                 4. Summarize the codebase structure as a hierarchical outline\n\
                                 5. Identify the main entry points and data flows"
                            )
                        }
                    }
                ]
            }))
        }
        "analyze_hotspots" => {
            let repo = args
                .get("repo")
                .and_then(|v| v.as_str())
                .map(|r| format!(" for repository '{r}'"))
                .unwrap_or_default();

            Some(json!({
                "messages": [{
                    "role": "user",
                    "content": {
                        "type": "text",
                        "text": format!(
                            "Analyze code hotspots{repo}.\n\n\
                             Please:\n\
                             1. Use the `hotspots` tool to find files with high churn\n\
                             2. Use the `coupling` tool to find temporally coupled file pairs\n\
                             3. Use the `ownership` tool to identify files with distributed ownership\n\
                             4. Cross-reference: which hotspot files also have strong coupling or low ownership?\n\
                             5. Recommend which files to prioritize for refactoring"
                        )
                    }
                }]
            }))
        }
        "find_dead_code" => {
            let target = args
                .get("target")
                .and_then(|v| v.as_str())
                .map(|t| format!(" focusing on '{t}'"))
                .unwrap_or_default();

            Some(json!({
                "messages": [{
                    "role": "user",
                    "content": {
                        "type": "text",
                        "text": format!(
                            "Find dead code and assess tracing coverage{target}.\n\n\
                             Please:\n\
                             1. Use the `coverage` tool to get dead code candidates and tracing stats\n\
                             2. For each dead code candidate, use `context` to verify it's truly unused\n\
                             3. Check if dead methods are test helpers, entry points, or framework callbacks (false positives)\n\
                             4. Summarize findings: truly dead code vs false positives vs missing tracing"
                        )
                    }
                }]
            }))
        }
        "trace_dependencies" => {
            let symbol = args
                .get("symbol")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");

            Some(json!({
                "messages": [{
                    "role": "user",
                    "content": {
                        "type": "text",
                        "text": format!(
                            "Trace the full dependency chain for `{symbol}`.\n\n\
                             Please:\n\
                             1. Use `context` to get the 360-degree view (callers, callees, hierarchy)\n\
                             2. Use `impact` with direction 'upstream' to find all callers transitively\n\
                             3. Use `impact` with direction 'downstream' to find all callees transitively\n\
                             4. Use `diagram` to generate a visual call graph\n\
                             5. Summarize the role of this symbol in the system and its blast radius"
                        )
                    }
                }]
            }))
        }
        "describe_process" => {
            let process = args
                .get("process")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");

            Some(json!({
                "messages": [{
                    "role": "user",
                    "content": {
                        "type": "text",
                        "text": format!(
                            "Analyze and describe the business process: `{process}`.\n\n\
                             Please:\n\
                             1. Use the `business` tool to get the high-level functional overview and key entities\n\
                             2. Use the `query` tool to search for related logic if entities aren't clear\n\
                             3. Use `context` on the main entities to understand their implementation details\n\
                             4. Use `diagram` with type 'flowchart' or 'sequence' for the key controllers/services\n\
                             5. Summarize the end-to-end flow, state changes, and key business rules"
                        )
                    }
                }]
            }))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_prompt_definitions() {
        let defs = prompt_definitions();
        let prompts = defs["prompts"].as_array().unwrap();
        assert_eq!(prompts.len(), 6);
        assert_eq!(prompts[0]["name"], "detect_impact");
        assert_eq!(prompts[1]["name"], "generate_map");
    }

    #[test]
    fn test_get_detect_impact_prompt() {
        let result = get_prompt(
            "detect_impact",
            &json!({"target": "UserService", "change_description": "add caching"}),
        );
        assert!(result.is_some());
        let messages = result.unwrap();
        let text = messages["messages"][0]["content"]["text"].as_str().unwrap();
        assert!(text.contains("UserService"));
        assert!(text.contains("add caching"));
    }

    #[test]
    fn test_get_generate_map_prompt() {
        let result = get_prompt("generate_map", &json!({"focus": "auth"}));
        assert!(result.is_some());
        let messages = result.unwrap();
        let text = messages["messages"][0]["content"]["text"].as_str().unwrap();
        assert!(text.contains("auth"));
    }

    #[test]
    fn test_unknown_prompt() {
        assert!(get_prompt("nonexistent", &json!({})).is_none());
    }
}

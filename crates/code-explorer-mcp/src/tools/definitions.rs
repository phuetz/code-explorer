//! JSON Schema definitions for the MCP tools.
//!
//! Each tool has a name, description, and inputSchema following the MCP spec.

use serde_json::{json, Value};

/// A single MCP tool definition.
pub struct ToolDefinition {
    pub name: &'static str,
    pub description: &'static str,
    pub input_schema: Value,
}

/// Return definitions for all MCP tools.
pub fn tool_definitions() -> Vec<ToolDefinition> {
    vec![
        ToolDefinition {
            name: "list_repos",
            description: "List all indexed repositories with their stats and last-indexed timestamps.",
            input_schema: json!({
                "type": "object",
                "properties": {},
                "additionalProperties": false
            }),
        },
        ToolDefinition {
            name: "query",
            description: "Natural language search across the knowledge graph. By default, groups results by Process (execution flow) with step indexes; symbols not part of any process go into `definitions`. Pass group_by_process=false for the flat list.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "Natural language search query (e.g., 'authentication middleware', 'database connection pool')"
                    },
                    "repo": {
                        "type": "string",
                        "description": "Repository name or path. Optional if only one repo is indexed."
                    },
                    "limit": {
                        "type": "number",
                        "description": "Maximum results to return (default: 10)",
                        "default": 10
                    },
                    "goal": {
                        "type": "string",
                        "description": "Additional context about what you're trying to accomplish"
                    },
                    "group_by_process": {
                        "type": "boolean",
                        "description": "Group results by Process with step_index. Default: true. Set to false for the legacy flat result list.",
                        "default": true
                    }
                },
                "required": ["query"],
                "additionalProperties": false
            }),
        },
        ToolDefinition {
            name: "context",
            description: "Get 360-degree context for a code symbol: callers, callees, imports, exports, class hierarchy, and related community.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "name": {
                        "type": "string",
                        "description": "Symbol name to look up (function, class, method, etc.)"
                    },
                    "repo": {
                        "type": "string",
                        "description": "Repository name or path"
                    },
                    "uid": {
                        "type": "string",
                        "description": "Exact node ID (e.g., 'Function:src/auth.ts:validateToken')"
                    },
                    "file": {
                        "type": "string",
                        "description": "Filter to symbols in this file path"
                    }
                },
                "required": ["name"],
                "additionalProperties": false
            }),
        },
        ToolDefinition {
            name: "impact",
            description: "Blast radius analysis: find everything affected by changing a symbol. Shows upstream callers, downstream callees, and transitive impact.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "target": {
                        "type": "string",
                        "description": "Symbol name or node ID to analyze"
                    },
                    "repo": {
                        "type": "string",
                        "description": "Repository name or path"
                    },
                    "direction": {
                        "type": "string",
                        "enum": ["upstream", "downstream", "both"],
                        "description": "Analysis direction (default: both)",
                        "default": "both"
                    },
                    "max_depth": {
                        "type": "number",
                        "description": "Maximum traversal depth (default: 5)",
                        "default": 5
                    }
                },
                "required": ["target"],
                "additionalProperties": false
            }),
        },
        ToolDefinition {
            name: "detect_changes",
            description: "Analyze uncommitted changes: parse git diff hunks, map to affected symbols, BFS upstream to impacted processes, and classify risk (none/low/medium/high).",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "repo": {
                        "type": "string",
                        "description": "Repository name or path"
                    },
                    "max_upstream_depth": {
                        "type": "number",
                        "description": "BFS depth when walking upstream from changed symbols (default: 3, max: 10)",
                        "default": 3
                    }
                },
                "additionalProperties": false
            }),
        },
        ToolDefinition {
            name: "rename",
            description: "Multi-file rename: returns graph-confirmed edits (high confidence) + text-search fallback edits (for review). Defaults to dry_run=true; pass dry_run=false to actually patch files on disk.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "target": {
                        "type": "string",
                        "description": "Current symbol name or node ID"
                    },
                    "new_name": {
                        "type": "string",
                        "description": "Proposed new name"
                    },
                    "repo": {
                        "type": "string",
                        "description": "Repository name or path"
                    },
                    "dry_run": {
                        "type": "boolean",
                        "description": "If true (default), return edits without modifying files. If false, apply graph_edits to disk (text_search_edits are never auto-applied).",
                        "default": true
                    }
                },
                "required": ["target", "new_name"],
                "additionalProperties": false
            }),
        },
        ToolDefinition {
            name: "cypher",
            description: "Execute a raw Cypher query against the knowledge graph. Read-only queries only.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "Cypher query to execute (must be read-only, no CREATE/DELETE/SET/MERGE)"
                    },
                    "repo": {
                        "type": "string",
                        "description": "Repository name or path"
                    }
                },
                "required": ["query"],
                "additionalProperties": false
            }),
        },
        ToolDefinition {
            name: "hotspots",
            description: "Identify file-level hotspots: files with high churn (lines added/removed) and frequent commits. These are often sources of bugs and maintenance burden.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "repo": {
                        "type": "string",
                        "description": "Repository name or path"
                    },
                    "since_days": {
                        "type": "number",
                        "description": "Analyze commits from the last N days (default: 90)",
                        "default": 90
                    },
                    "limit": {
                        "type": "number",
                        "description": "Maximum results to return (default: 20)",
                        "default": 20
                    }
                },
                "additionalProperties": false
            }),
        },
        ToolDefinition {
            name: "coupling",
            description: "Analyze temporal coupling between files: find file pairs that frequently change together in commits, suggesting hidden dependencies.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "repo": {
                        "type": "string",
                        "description": "Repository name or path"
                    },
                    "min_shared": {
                        "type": "number",
                        "description": "Minimum shared commits to report a coupling (default: 3)",
                        "default": 3
                    },
                    "limit": {
                        "type": "number",
                        "description": "Maximum results to return (default: 20)",
                        "default": 20
                    }
                },
                "additionalProperties": false
            }),
        },
        ToolDefinition {
            name: "ownership",
            description: "Analyze code ownership by author: shows primary author, ownership percentage, and contributor distribution for each file.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "repo": {
                        "type": "string",
                        "description": "Repository name or path"
                    },
                    "limit": {
                        "type": "number",
                        "description": "Maximum results to return (default: 20)",
                        "default": 20
                    }
                },
                "additionalProperties": false
            }),
        },
        ToolDefinition {
            name: "coverage",
            description: "Analyze tracing coverage and dead code: shows which methods have tracing instrumentation and which have zero incoming calls (dead code candidates).",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "target": {
                        "type": "string",
                        "description": "Optional class/service name to analyze. If omitted, returns global coverage stats."
                    },
                    "repo": {
                        "type": "string",
                        "description": "Repository name or path"
                    }
                },
                "additionalProperties": false
            }),
        },
        ToolDefinition {
            name: "diagram",
            description: "Generate a Mermaid diagram for a code symbol: flowchart (call graph), sequence (interaction), or class (hierarchy).",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "target": {
                        "type": "string",
                        "description": "Symbol name to generate diagram for (class, controller, service, etc.)"
                    },
                    "type": {
                        "type": "string",
                        "enum": ["flowchart", "sequence", "class"],
                        "description": "Diagram type (default: flowchart)",
                        "default": "flowchart"
                    },
                    "repo": {
                        "type": "string",
                        "description": "Repository name or path"
                    }
                },
                "required": ["target"],
                "additionalProperties": false
            }),
        },
        ToolDefinition {
            name: "report",
            description: "Generate a code health report combining graph stats, hotspots, coupling, and ownership into a grade (A-E).",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "repo": {
                        "type": "string",
                        "description": "Repository name or path"
                    }
                },
                "additionalProperties": false
            }),
        },
        ToolDefinition {
            name: "search_processes",
            description: "Search business process flows in the graph. Use when the question involves a workflow, business process, or multi-step operation.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "Text to match against process names/descriptions. Empty lists top processes."
                    },
                    "repo": {
                        "type": "string",
                        "description": "Repository name or path"
                    },
                    "limit": {
                        "type": "number",
                        "description": "Maximum processes to return (default: 10)",
                        "default": 10
                    }
                },
                "additionalProperties": false
            }),
        },
        ToolDefinition {
            name: "analyze_execution_trace",
            description: "Analyze an execution trace (e.g., a JSON or NDJSON log file) to map chronological steps against the knowledge graph, retrieving the source code and parameter values for each step. Ideal for documenting business processes.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "trace_file": {
                        "type": "string",
                        "description": "Absolute path to the execution trace JSON/NDJSON file"
                    },
                    "repo": {
                        "type": "string",
                        "description": "Repository name or path"
                    }
                },
                "required": ["trace_file"],
                "additionalProperties": false
            }),
        },
        // ── New tools (search_code, read_file, get_insights, save_memory) ──
        ToolDefinition {
            name: "search_code",
            description: "Full-text search across the codebase. Returns matching code symbols with actual source code snippets, callers, and callees. More detailed than `query` which returns graph metadata only. Pass `rerank: true` for post-retrieval LLM reranking (requires ~/.codeexplorer/chat-config.json) and/or `hybrid: true` to fuse BM25 with semantic embeddings via RRF (requires running `code-explorer embed` first). Both fall back gracefully to plain BM25 on any failure.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "Search query (symbol names, keywords, file paths)"
                    },
                    "repo": {
                        "type": "string",
                        "description": "Repository name or path"
                    },
                    "limit": {
                        "type": "number",
                        "description": "Maximum results (default: 8, max: 20)",
                        "default": 8
                    },
                    "rerank": {
                        "type": "boolean",
                        "description": "Post-process top-20 BM25 with an LLM reranker (default: false)",
                        "default": false
                    },
                    "hybrid": {
                        "type": "boolean",
                        "description": "Fuse BM25 with semantic embeddings via Reciprocal Rank Fusion. Requires `code-explorer embed` to have populated .codeexplorer/embeddings.bin (default: false).",
                        "default": false
                    }
                },
                "required": ["query"],
                "additionalProperties": false
            }),
        },
        ToolDefinition {
            name: "read_file",
            description: "Read source file contents with optional line range. Returns the code along with graph context: symbols defined in this file and any enrichment metadata.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "File path relative to the repository root"
                    },
                    "repo": {
                        "type": "string",
                        "description": "Repository name or path"
                    },
                    "start_line": {
                        "type": "number",
                        "description": "First line to read (1-based, default: 1)"
                    },
                    "end_line": {
                        "type": "number",
                        "description": "Last line to read (default: start_line + 100)"
                    }
                },
                "required": ["path"],
                "additionalProperties": false
            }),
        },
        ToolDefinition {
            name: "get_insights",
            description: "Get enrichment insights for a symbol: complexity, dead code status, tracing coverage, code smells, design patterns, risk score, refactoring suggestions, and community context.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "symbol": {
                        "type": "string",
                        "description": "Symbol name or node ID"
                    },
                    "repo": {
                        "type": "string",
                        "description": "Repository name or path"
                    }
                },
                "required": ["symbol"],
                "additionalProperties": false
            }),
        },
        ToolDefinition {
            name: "save_memory",
            description: "Persist a fact or insight to memory for retrieval across sessions. Useful for recording architectural decisions, discovered patterns, or project conventions.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "fact": {
                        "type": "string",
                        "description": "The fact or insight to remember"
                    },
                    "scope": {
                        "type": "string",
                        "enum": ["global", "project"],
                        "description": "Storage scope: global (across all repos) or project (repo-specific)",
                        "default": "project"
                    },
                    "repo": {
                        "type": "string",
                        "description": "Repository name (required for project scope)"
                    }
                },
                "required": ["fact"],
                "additionalProperties": false
            }),
        },
        // ── Code Quality Suite (Theme A) ───────────────────────────
        ToolDefinition {
            name: "find_cycles",
            description: "Detect circular dependencies via strongly connected components (Tarjan). Scope: 'imports' (File→File) or 'calls' (Method→Method). Returns ordered node lists with severity (low/medium/high).",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "repo": {
                        "type": "string",
                        "description": "Repository name or path"
                    },
                    "scope": {
                        "type": "string",
                        "enum": ["imports", "calls"],
                        "description": "Cycle scope: 'imports' (File->File) or 'calls' (Method->Method). Default: 'imports'.",
                        "default": "imports"
                    },
                    "limit": {
                        "type": "number",
                        "description": "Maximum cycles to return (default: 50, max: 100)",
                        "default": 50
                    }
                },
                "additionalProperties": false
            }),
        },
        ToolDefinition {
            name: "find_similar_code",
            description: "Find duplicate / near-duplicate code using Rabin-Karp rolling hash on normalized tokens. Returns clusters of Method/Function nodes that share token windows, filtered by Jaccard similarity.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "repo": {
                        "type": "string",
                        "description": "Repository name or path"
                    },
                    "min_tokens": {
                        "type": "number",
                        "description": "Minimum window size in tokens (default: 30)",
                        "default": 30
                    },
                    "threshold": {
                        "type": "number",
                        "description": "Jaccard similarity threshold [0.0, 1.0] (default: 0.9)",
                        "default": 0.9
                    },
                    "limit": {
                        "type": "number",
                        "description": "Maximum clusters to return (default: 50, max: 100)",
                        "default": 50
                    }
                },
                "additionalProperties": false
            }),
        },
        ToolDefinition {
            name: "list_todos",
            description: "List TODO/FIXME/HACK/XXX markers found in source comments. Each entry includes kind, text, file, and line. Results ordered by severity (FIXME > HACK > TODO > XXX).",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "repo": {
                        "type": "string",
                        "description": "Repository name or path"
                    },
                    "severity": {
                        "type": "string",
                        "enum": ["TODO", "FIXME", "HACK", "XXX"],
                        "description": "Filter by marker kind (default: all)"
                    },
                    "limit": {
                        "type": "number",
                        "description": "Maximum markers to return (default: 200, max: 500)",
                        "default": 200
                    }
                },
                "additionalProperties": false
            }),
        },
        ToolDefinition {
            name: "get_complexity",
            description: "Get cyclomatic complexity report for the repo: global averages, percentiles, severity buckets, per-module stats, and top-N most complex functions/methods.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "repo": {
                        "type": "string",
                        "description": "Repository name or path"
                    },
                    "threshold": {
                        "type": "number",
                        "description": "Only list symbols with complexity >= this value (default: 0)",
                        "default": 0
                    },
                    "limit": {
                        "type": "number",
                        "description": "Maximum symbols in top_symbols list (default: 50, max: 200)",
                        "default": 50
                    }
                },
                "additionalProperties": false
            }),
        },
        // ── Schema & API Inventory (Theme D) ───────────────────────
        ToolDefinition {
            name: "list_endpoints",
            description: "List REST/GraphQL endpoints extracted from the codebase (Express, Next.js, FastAPI, Flask, Spring, ASP.NET MVC). Each entry includes HTTP method, route pattern, framework, source file, and the resolved handler method ID when available.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "repo": {
                        "type": "string",
                        "description": "Repository name or path"
                    },
                    "method": {
                        "type": "string",
                        "description": "Filter by HTTP method (GET, POST, PUT, DELETE, PATCH). Case-insensitive."
                    },
                    "pattern": {
                        "type": "string",
                        "description": "Substring filter applied to the route (case-insensitive)."
                    },
                    "limit": {
                        "type": "number",
                        "description": "Maximum endpoints to return (default: 200, max: 500)",
                        "default": 200
                    }
                },
                "additionalProperties": false
            }),
        },
        ToolDefinition {
            name: "list_db_tables",
            description: "List database tables discovered from SQL migrations, Prisma schemas, SQLAlchemy/TypeORM classes, or EF6 DbContexts. Each entry includes column count and foreign-key reference count.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "repo": {
                        "type": "string",
                        "description": "Repository name or path"
                    },
                    "limit": {
                        "type": "number",
                        "description": "Maximum tables to return (default: 200, max: 500)",
                        "default": 200
                    }
                },
                "additionalProperties": false
            }),
        },
        ToolDefinition {
            name: "list_env_vars",
            description: "List environment variables declared in config files (.env, appsettings.json, application.yml) and referenced in code (process.env.X, os.getenv, Environment.GetEnvironmentVariable). Flags unused declarations and undeclared references for audit.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "repo": {
                        "type": "string",
                        "description": "Repository name or path"
                    },
                    "unused_only": {
                        "type": "boolean",
                        "description": "When true, returns only variables declared but never referenced (audit mode).",
                        "default": false
                    },
                    "limit": {
                        "type": "number",
                        "description": "Maximum vars to return (default: 200, max: 1000)",
                        "default": 200
                    }
                },
                "additionalProperties": false
            }),
        },
        ToolDefinition {
            name: "get_endpoint_handler",
            description: "Resolve the handler method for a given endpoint (route + method) and return its first-degree call neighborhood (callers + callees).",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "repo": {
                        "type": "string",
                        "description": "Repository name or path"
                    },
                    "route": {
                        "type": "string",
                        "description": "Route path as discovered by list_endpoints (e.g. '/api/users/:id')."
                    },
                    "method": {
                        "type": "string",
                        "description": "HTTP method (GET, POST, PUT, DELETE, PATCH)."
                    }
                },
                "required": ["route", "method"],
                "additionalProperties": false
            }),
        },
        // ── SFD doc-authoring workflow (P1.1) ────────────────────────────
        // List existing module pages + drafts so an agent (or UI) can pick a
        // target before writing. Mirrors the desktop chat tool of the same
        // name; the lib lives in `code-explorer-rag::sfd`.
        ToolDefinition {
            name: "list_sfd_pages",
            description: "List existing SFD module pages under .codeexplorer/docs/modules/ and any in-progress drafts under .codeexplorer/docs/_drafts/. Use before write_sfd_draft to know what already exists.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "repo": {
                        "type": "string",
                        "description": "Repository name or path"
                    }
                },
                "additionalProperties": false
            }),
        },
        // Writes Markdown into `_drafts/`, never `modules/`. Atomic
        // (.tmp → rename) + path-traversal sandboxed.
        ToolDefinition {
            name: "write_sfd_draft",
            description: "Write Markdown content into .codeexplorer/docs/_drafts/<page>. Drafts are isolated from the live modules/ tree; promotion stays a deliberate user action. Page name is sandboxed (no '..', '\\', or absolute paths).",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "repo": {
                        "type": "string",
                        "description": "Repository name or path"
                    },
                    "page": {
                        "type": "string",
                        "description": "Bare file name, e.g. 'dossiers.md'. No path traversal."
                    },
                    "content": {
                        "type": "string",
                        "description": "Full Markdown body for the draft. Will overwrite any existing draft of the same name."
                    }
                },
                "required": ["page", "content"],
                "additionalProperties": false
            }),
        },
        // Runs the same pre-delivery linter the CLI `validate-docs` command
        // uses; returns a structured RED/YELLOW report.
        ToolDefinition {
            name: "validate_sfd",
            description: "Run the pre-delivery linter (residual TODO/TBD, unfilled GNX:* anchors, broken links, short sections, missing §4 Algorithmes). Returns RED/YELLOW counts plus per-page issue details. Pass path='_drafts' to lint only drafts.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "repo": {
                        "type": "string",
                        "description": "Repository name or path"
                    },
                    "path": {
                        "type": "string",
                        "description": "Sub-directory under .codeexplorer/docs/ to validate (e.g. '_drafts'). Empty = full docs tree.",
                        "default": ""
                    }
                },
                "additionalProperties": false
            }),
        },
    ]
}

/// Convert tool definitions to the JSON format expected by MCP tools/list.
pub fn tools_list_json() -> Value {
    let tools: Vec<Value> = tool_definitions()
        .into_iter()
        .map(|t| {
            json!({
                "name": t.name,
                "description": t.description,
                "inputSchema": t.input_schema
            })
        })
        .collect();

    json!({ "tools": tools })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tool_definitions_count() {
        assert_eq!(tool_definitions().len(), 30);
    }

    #[test]
    fn test_tool_names() {
        let tools = tool_definitions();
        let names: Vec<&str> = tools.iter().map(|t| t.name).collect();
        assert!(names.contains(&"list_repos"));
        assert!(names.contains(&"query"));
        assert!(names.contains(&"context"));
        assert!(names.contains(&"impact"));
        assert!(names.contains(&"detect_changes"));
        assert!(names.contains(&"rename"));
        assert!(names.contains(&"cypher"));
        assert!(names.contains(&"hotspots"));
        assert!(names.contains(&"coupling"));
        assert!(names.contains(&"ownership"));
        assert!(names.contains(&"coverage"));
        assert!(names.contains(&"diagram"));
        assert!(names.contains(&"report"));
        assert!(names.contains(&"search_processes"));
        assert!(names.contains(&"analyze_execution_trace"));
        assert!(names.contains(&"search_code"));
        assert!(names.contains(&"read_file"));
        assert!(names.contains(&"get_insights"));
        assert!(names.contains(&"save_memory"));
        assert!(names.contains(&"find_cycles"));
        assert!(names.contains(&"find_similar_code"));
        assert!(names.contains(&"list_todos"));
        assert!(names.contains(&"get_complexity"));
        // Schema & API Inventory (Theme D)
        assert!(names.contains(&"list_endpoints"));
        assert!(names.contains(&"list_db_tables"));
        assert!(names.contains(&"list_env_vars"));
        assert!(names.contains(&"get_endpoint_handler"));
        // SFD doc-authoring workflow (P1.1)
        assert!(names.contains(&"list_sfd_pages"));
        assert!(names.contains(&"write_sfd_draft"));
        assert!(names.contains(&"validate_sfd"));
    }

    #[test]
    fn test_tools_list_json() {
        let json = tools_list_json();
        let tools = json["tools"].as_array().unwrap();
        assert_eq!(tools.len(), 30);
        for tool in tools {
            assert!(tool.get("name").is_some());
            assert!(tool.get("description").is_some());
            assert!(tool.get("inputSchema").is_some());
        }
    }

    #[test]
    fn test_query_tool_has_required_field() {
        let tools = tool_definitions();
        let query_tool = tools.iter().find(|t| t.name == "query").unwrap();
        let required = query_tool.input_schema["required"].as_array().unwrap();
        assert!(required.contains(&json!("query")));
    }
}

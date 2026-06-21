---
name: code-explorer
description: Query and analyze codebases using the Code Explorer knowledge graph. Use for finding symbols, analyzing impact, understanding architecture, exploring code relationships, generating documentation, and asking questions about code.
argument-hint: "[command] [arguments]"
allowed-tools: Bash(code-explorer *), Bash(*/code-explorer *), Bash(cargo run -p code-explorer-cli -- *), Read, Grep, Glob
---

# Code Explorer — Knowledge Graph Code Intelligence

You have access to Code Explorer, a code-intelligence tool that builds a knowledge graph from source code. Use it to answer questions about codebases, find symbols, trace dependencies, and analyze impact — using a fraction of the context that reading files would cost.

## Binary location

The CLI is `code-explorer`. After `cargo build --release` the binary is at `target/release/code-explorer`; put it on your PATH, or call it by full path from other projects. Inside the Code Explorer repo, `cargo run -p code-explorer-cli --` also works.

## See the value (quick)

```bash
code-explorer demo [path]            # Measure the context an LLM agent saves vs reading files
```

## Available commands

### 1. Analyze a codebase (must run first)

```bash
code-explorer analyze [path]              # Index a repository
code-explorer analyze [path] --force      # Force re-index
code-explorer analyze [path] --incremental # Only re-parse changed files
code-explorer analyze [path] --embeddings  # Generate ONNX semantic embeddings (feature gated)
code-explorer analyze [path] --skip-git    # Skip git history phases (required for non-git folders)
code-explorer status                       # Check if index exists (+ last index duration / phase timings)
```

### 2. Search the knowledge graph

```bash
code-explorer query "authentication middleware"    # Natural language search
code-explorer query "user service" --limit 5       # Limit results
```

### 3. Symbol context (360-degree view)

```bash
code-explorer context UserService          # Callers, callees, imports, exports, hierarchy
code-explorer context handleRequest --repo my-project
```

### 4. Impact analysis (blast radius)

```bash
code-explorer impact handleRequest --direction both       # Upstream + downstream
code-explorer impact UserService --direction upstream      # Who calls this?
code-explorer impact UserService --direction downstream    # What does this call?
```

### 5. Ask questions (LLM-powered, requires ~/.codeexplorer/chat-config.json)

```bash
code-explorer ask "how does the bareme calculation work?" --path /path/to/project
code-explorer ask "which controllers call the external API?"
```

### 6. Code health report

```bash
code-explorer report --path /path/to/project        # Text report with grade A-E
code-explorer report --path /path/to/project --json # JSON output
```

### 7. Git analytics (requires the target to be a git repo)

```bash
code-explorer hotspots --path [path]       # Most changed files (last 90 days)
code-explorer coupling --path [path]       # Files that change together
code-explorer ownership --path [path]      # Code ownership by author
```

### 8. Tracing coverage & dead code

```bash
code-explorer coverage                             # Global tracing + dead code stats
code-explorer coverage UserService                 # Single class coverage
code-explorer coverage --json                      # JSON output
code-explorer coverage UserService --trace         # Flow trace mode
```

### 9. Raw Cypher queries

```bash
code-explorer cypher "MATCH (n:Function) RETURN n.name LIMIT 10"
code-explorer cypher "MATCH (n:Controller)-[:DEFINES]->(a:ControllerAction) RETURN n.name, a.name"
code-explorer cypher "MATCH (n:Method) WHERE n.name STARTS WITH 'Get' RETURN DISTINCT n.name"
code-explorer cypher "MATCH (n:Function) WHERE n.name CONTAINS 'auth' OR n.name CONTAINS 'login' RETURN n"
code-explorer cypher "MATCH (n:Method) WHERE NOT n.filePath ENDS WITH '.test.cs' RETURN n.name"
```

Supported Cypher operators:
- WHERE: `=`, `<>`, `!=`, `CONTAINS`, `STARTS WITH`, `ENDS WITH`
- Logic: `AND`, `OR`, `NOT` (precedence: NOT > AND > OR)
- RETURN: `DISTINCT`, `count()`
- Clauses: `ORDER BY [ASC|DESC]`, `LIMIT`

### 10. Interactive shell

```bash
code-explorer shell                           # REPL with tab completion
# Inside the shell: query, context, impact, cypher, hotspots, stats, help
```

### 11. MCP server (for AI agents)

```bash
code-explorer mcp                                  # Start MCP server (stdio, JSON-RPC 2.0)
# 29 tools, including: list_repos, query, context, impact, detect_changes, rename,
#   cypher, search_code, read_file, find_cycles, find_similar_code, hotspots, coupling,
#   ownership, coverage, diagram, report, analyze_execution_trace, get_complexity,
#   list_todos, list_endpoints, list_db_tables, list_env_vars, get_endpoint_handler,
#   get_insights, save_memory
code-explorer mcp install                          # Auto-configure the MCP server for Claude Code
```

### 12. HTTP server (REST API)

```bash
code-explorer serve --port 3000                    # JSON-RPC /mcp + REST + SSE
code-explorer serve --port 3000 --host 0.0.0.0     # Expose on all interfaces
```

### 13. Validate LLM config

```bash
code-explorer config test                          # Check API key + test connection
```

### 14. List indexed repos

```bash
code-explorer list                                 # Show all indexed repositories
```

### 15. Generate documentation (all formats)

```bash
code-explorer generate context   --path [path]    # AGENTS.md at repo root
code-explorer generate wiki      --path [path]    # /wiki/*.md (one per module)
code-explorer generate docs      --path [path]    # .codeexplorer/docs/*.md
code-explorer generate docx      --path [path]    # documentation.docx (Word, full doc with TOC)
code-explorer generate html      --path [path]    # DeepWiki-style HTML site
code-explorer generate obsidian  --path [path]    # Obsidian vault export
code-explorer generate all       --path [path]    # All formats above in one run
```

Flags on every `generate` subcommand: `--output-dir <dir>`, `--enrich` (LLM, needs chat-config.json), `--enrich-profile <fast|quality|strict>`, `--enrich-lang <auto|fr|en>`, `--enrich-citations`.

### 16. Other commands

```bash
code-explorer trace-files ClassName            # All source files involved in a feature
code-explorer diagram ClassName --type flowchart|sequence|class
code-explorer rag-import <docs-folder> --path [path]   # Import .md/.docx specs, link to code
code-explorer watch [path]                     # Re-index on file changes (debounced)
code-explorer dashboard [path]                 # Interactive terminal UI over the graph
code-explorer clean [--force|--all]            # Delete an index
code-explorer setup                            # Configure editor MCP integration
```

## How to use this skill

When the user asks about code structure, architecture, dependencies, or impact:

1. **Check if the repo is indexed**: run `code-explorer status` in the relevant directory.
2. **If not indexed**: run `code-explorer analyze [path]` first (add `--skip-git` if the target isn't a git repo).
3. **Choose the right command** based on the question:
   - "Where is X defined?" → `code-explorer query "X"` or `code-explorer context X`
   - "What calls X?" → `code-explorer impact X --direction upstream`
   - "What does X depend on?" → `code-explorer impact X --direction downstream`
   - "How healthy is the code?" → `code-explorer report`
   - "Which files change most?" → `code-explorer hotspots`
   - "Is this code used?" → `code-explorer coverage` or `code-explorer coverage ClassName`
   - "Explain how X works" → `code-explorer ask "how does X work?"`
   - "Show me the architecture" → `code-explorer generate html` then read the output
   - "Which files are involved in X?" → `code-explorer trace-files X`
   - "Generate a diagram of X" → `code-explorer diagram X --type flowchart`
4. **Parse the output** and present it clearly with file paths and relationships.
5. **Combine multiple commands** for complex questions.

## Graph node types

The knowledge graph contains 50+ node types:
- **Code**: Function, Method, Class, Interface, Struct, Enum, Module, Namespace
- **ASP.NET**: Controller, ControllerAction, Service, Repository, ExternalService
- **UI**: UiComponent (Telerik/Kendo grids), ScriptFile, AjaxCall
- **Data**: Entity (EF6), Association, NavigationProperty
- **Infrastructure**: File, Directory, Import, Export
- **RAG**: Document, DocChunk (from `rag-import`)

## Relationship types

- `Calls`, `CallsAction`, `CallsService` — invocation chains
- `Imports`, `Exports` — module dependencies
- `Inherits`, `Implements` — type hierarchy
- `HasMethod`, `HasProperty` — class membership
- `BelongsTo`, `Mentions` — RAG doc-to-code anchors

## Tips

- The `.codeexplorer/` directory in the repo root holds the serialized graph (`graph.bin`), `meta.json`, and `metrics.json` (index timing/throughput).
- Supports 14 languages: JS, TS, Python, Java, C, C++, C#, Go, Rust, Ruby, PHP, Kotlin, Swift, Razor.
- Skips `obj/`, `bin/`, `node_modules/`, `packages/` during analysis.
- Use `--skip-git` for non-git folders; `hotspots`/`coupling`/`ownership` return empty there (expected).
- All `--json` flags output machine-readable JSON.

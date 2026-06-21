# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

Code Explorer is a Rust-based code intelligence system that builds a knowledge graph from source code and exposes it via MCP (Model Context Protocol) for AI-powered code analysis. It supports 14 programming languages via tree-sitter parsing, with deep ASP.NET MVC 5 / Entity Framework 6 enrichment.

## Build Commands

```bash
# Build (debug)
cargo build

# Build (release, with thin LTO)
cargo build --release

# Build with KuzuDB backend
cargo build --features code-explorer-cli/kuzu-backend

# Build with semantic search (ONNX embeddings)
cargo build --features code-explorer-search/embeddings

# Build the desktop app (Tauri v2)
cd crates/code-explorer-desktop/ui && npm install && npm run build && cd ../../..
cargo build -p code-explorer-desktop

# Run the desktop app in dev mode (with hot reload)
# Dev server on port 1421 (Tauri's default 1420 is commonly used by other
# projects on this machine — kept at 1421 to avoid collisions; see
# tauri.conf.json devUrl/beforeDevCommand).
cd crates/code-explorer-desktop && cargo tauri dev

# Frontend only (from ui/ dir)
cd crates/code-explorer-desktop/ui && npm run dev    # Vite dev server
cd crates/code-explorer-desktop/ui && npm run lint   # ESLint
cd crates/code-explorer-desktop/ui && npm run build  # tsc + vite build

# Build the standalone web chat (Vite + React 19, was D:/CascadeProjects/code-explorer-chat)
cd chat-ui && npm install && npm run build
# Run web chat in dev (port 5174 — proxies /api /health /mcp to code-explorer serve)
cd chat-ui && npm run dev
# Backend it talks to: code-explorer serve --http 8080 (separate process)

# Build NexusBrain (separate Tauri app)
cd nexus-brain && npm install && npm run build && cd ..
cd nexus-brain/src-tauri && cargo build && cd ../..

# Run NexusBrain in dev mode
cd nexus-brain/src-tauri && cargo tauri dev

# Run the CLI
cargo run -p code-explorer-cli -- <command>

# Run tests
cargo test --workspace

# Run tests for a specific crate
cargo test -p code-explorer-ingest

# Run a single test by name
cargo test -p code-explorer-ingest -- test_name

# Check all crates compile
cargo check --workspace

# Clippy
cargo clippy --workspace
```

## Workspace Architecture

12 active crates in `crates/` (code-explorer-lsp, code-explorer-storage, and code-explorer-types are excluded from the workspace), with a strict dependency flow:

```
code-explorer-cli (binary: "code-explorer")
  ├── code-explorer-mcp        (MCP server: 30 tools, stdio/HTTP transport, JSON-RPC 2.0)
  ├── code-explorer-search      (Hybrid search: BM25 + optional ONNX semantic + RRF fusion)
  ├── code-explorer-db          (Database adapter: InMemory backend or optional KuzuDB)
  ├── code-explorer-ingest      (8-phase ingestion pipeline, parallel with rayon)
  │     └── code-explorer-lang  (14 language providers: tree-sitter queries, import resolution, type extraction)
  ├── code-explorer-query       (Query execution)
  ├── code-explorer-output      (Output formatting)
  ├── code-explorer-git         (Git analytics: hotspots, coupling, ownership)
  ├── code-explorer-rag         (GraphRAG: doc chunking and semantic anchoring into the graph)
  └── code-explorer-core        (Core types: KnowledgeGraph, NodeLabel, SymbolTable, config)
       └── code-explorer-types  (Shared type definitions)

code-explorer-desktop (Tauri v2 desktop app)
  ├── code-explorer-db          (direct graph/index/FTS access, NOT via MCP envelope)
  ├── code-explorer-git
  ├── code-explorer-search
  └── code-explorer-core

nexus-brain (separate Tauri v2 app — "Knowledge IDE", Obsidian-like vault editor)
  └── standalone — reads Markdown Vaults exported by Code Explorer, not a workspace member

chat-ui/ (standalone web chat — Vite 7 + React 19 + Tailwind v4)
  └── browser frontend that talks to `code-explorer serve --http 8080` via JSON-RPC + SSE.
      Imported in May 2026 from a separate repo (was D:/CascadeProjects/code-explorer-chat,
      no GitHub remote). Not a Cargo crate — `cd chat-ui && npm run dev` on port 5174.
      Per-folder doc: `chat-ui/CLAUDE.md`. Roadmap: `chat-ui/README.md` and the
      backlogs in `D:/CascadeProjects/claude-et-patrice/propositions/CHAT-V1*.md`.
```

**Core** (`code-explorer-core`): In-memory knowledge graph with HashMap-based O(1) node/relationship lookup. Defines `NodeLabel` (52 variants), `RelationshipType` (27 variants including `Calls`, `HasMethod`, `HasProperty`, `HasAction`), `SymbolDefinition`, and pipeline types. Node properties include `is_traced`, `is_dead_candidate`, `trace_call_count`. Not thread-safe on its own; wrapped in `Arc<RwLock<>>` when shared.

**Lang** (`code-explorer-lang`): `LanguageProvider` trait with 14 implementations (JS, TS, Python, Java, C, C++, C#, Go, Rust, PHP, Ruby, Kotlin, Swift, Razor). Each provider supplies tree-sitter query strings (`queries/`), import resolvers (`import_resolvers/`), named binding extractors (`named_bindings/`), type extractors, export detection, and call routing. Static dispatch via `registry.rs` (match on language variant, zero-cost).

**Ingest** (`code-explorer-ingest`): Pipeline orchestrator in `pipeline.rs` runs 7 phases (some with sub-phases) sequentially:
1. **Structure** — filesystem walk, File/Folder nodes
2. **Parsing** — tree-sitter AST extraction, creates `HasMethod`/`HasProperty` nesting edges via `find_enclosing_class_id` parent-chain walk. Sub-phase 2b detects `.csproj` component libraries
3. **Imports** — import resolution, File→File edges
4. **Calls** — Method→Method resolution with 4 tiers: DI field-type (0.85 confidence), static-call (0.80), same-file, global fuzzy. Handles C# `using` and `var = new` patterns
5. **Heritage** — class inheritance/implementation edges. Sub-phase 5b: ASP.NET MVC 5 / EF6 enrichment (14 passes: controllers, actions, views, entities, DbContexts, services, AJAX, UI components, StackLogger tracing propagation)
6. **Community** (6a) + **Process detection** (6b) — cluster and process grouping
7. **Dead Code** — marks methods with 0 incoming Calls as `is_dead_candidate` (entry points excluded)

Uses rayon for parallel file processing with a 20MB chunk budget and LRU AST cache (cap 50).

**DB** (`code-explorer-db`): `DatabaseBackend` trait with `InMemoryBackend` (default, includes simple Cypher executor and BM25 FTS) and `KuzuDbBackend` (feature-gated via `kuzu-backend`). Schema defines 56 node tables with a unified `CodeRelation` relationship table. Persistence via bincode snapshots (`graph.bin`). Query results returned as `Vec<serde_json::Value>`.

**Search** (`code-explorer-search`): Reciprocal Rank Fusion (K=60) merging BM25 lexical results with optional ONNX-based semantic embeddings. Optional LLM reranker (`reranker-llm` feature) post-processes top-K candidates by sending them to an OpenAI-compatible endpoint (reuses `~/.codeexplorer/chat-config.json`). Gracefully degrades without any optional feature.

**MCP** (`code-explorer-mcp`): Implements MCP protocol version 2024-11-05. Thirty tools dispatched in `backend/local.rs`:
- **Graph & query**: `list_repos`, `query`, `context`, `impact`, `detect_changes`, `rename`, `cypher`, `search_code`, `read_file`, `find_cycles`, `find_similar_code`
- **Analytics**: `hotspots`, `coupling`, `ownership`, `coverage`, `diagram`, `report`, `analyze_execution_trace`, `get_complexity`
- **Codebase introspection**: `list_todos`, `list_endpoints`, `list_db_tables`, `list_env_vars`, `get_endpoint_handler`
- **Agent support**: `get_insights`, `save_memory`
- **Doc-SFD authoring (P1.1)**: `list_sfd_pages`, `write_sfd_draft`, `validate_sfd` — backed by `code-explorer-rag::sfd`, shared between the desktop chat (Tauri) and the HTTP MCP transport (chat-ui).

Six prompts: `detect_impact`, `generate_map`, `analyze_hotspots`, `find_dead_code`, `trace_dependencies`, `describe_process`.

Stdio and HTTP transports. `LocalBackend` coordinates registry loading and tool dispatch.

**Git** (`code-explorer-git`): Git history analysis: `analyze_hotspots` (file churn scoring), `analyze_coupling` (temporal coupling between files), `analyze_ownership` (author distribution per file). Used by CLI and desktop app.

**RAG** (`code-explorer-rag`): GraphRAG integration — ingests external documentation (Markdown, PDF, DOCX), chunks it into `DocChunk` structs, and anchors chunks semantically into the knowledge graph. Uses pulldown-cmark for Markdown parsing.

**Desktop** (`code-explorer-desktop`): Tauri v2 desktop app with React 19 frontend. Accesses `KnowledgeGraph` + `GraphIndexes` + `FtsIndex` directly via Tauri IPC (not via MCP envelope — this is a deliberate performance choice). Frontend uses Sigma.js + Graphology for graph visualization, Zustand + TanStack Query for state, Tailwind CSS v4 + framer-motion for styling/animations. 35 Tauri command modules in `src/commands/` bridge frontend↔Rust. Four app modes: Explorer (graph + lenses), Analyze (hotspots/coupling/ownership/coverage/diagram/report/health), Chat (LLM Q&A with context), Manage (repo CRUD). State in `ui/src/stores/app-store.ts` (Zustand) and `ui/src/hooks/use-tauri-query.ts` (TanStack Query wrapper for IPC). Graph rendering: `ui/src/components/graph/GraphExplorer.tsx` (hot path).

**NexusBrain** (`nexus-brain/`): Separate Tauri v2 app (not a workspace member) — an Obsidian-like "Knowledge IDE" that reads Markdown Vaults exported by the Code Explorer desktop app's "Digital Brain" export feature. React 18 + Tailwind CSS 3 frontend, md-editor-rt for Markdown editing, react-force-graph-2d for graph visualization. Independent from the Rust workspace — has its own `Cargo.toml` under `src-tauri/`.

## Feature Flags

| Flag | Crate | Default | Purpose |
|------|-------|---------|---------|
| `kuzu-backend` | code-explorer-db, code-explorer-cli | off | Real KuzuDB graph database backend |
| `kotlin` | code-explorer-ingest | on | Kotlin tree-sitter grammar |
| `swift` | code-explorer-ingest | on | Swift tree-sitter grammar |
| `embeddings` | code-explorer-search | off | ONNX Runtime semantic search with tokenizers (HF); real inference on MiniLM/BGE models; enabled by default in code-explorer-cli |
| `reranker-llm` | code-explorer-search | off | LLM-based reranker for post-retrieval reordering; enabled by default in code-explorer-cli |

## Key Design Patterns

- **Node ID format**: Deterministic `"${Label}:${qualifiedName}"` (e.g., `"Function:src/main.ts:handleLogin"`). Stable across serialization.
- **Method→Method Calls**: `extract_call` uses `find_enclosing_method_id` to walk the tree-sitter parent chain and set the enclosing method as the Calls edge source (not the File node).
- **Class→Method nesting**: `create_definition_node` uses `find_enclosing_class_id` to emit `HasMethod`/`HasProperty` edges from the parent class to each member.
- **Controller→Service traversal**: Commands seed BFS with the sibling Class node (same name/file) to access `HasMethod` children that carry the Calls edges.
- **Tracing coverage**: `extract_tracing_info` detects `StackLogger.BeginMethodScope()` per method, propagated to Method nodes as `is_traced`.
- **Enum-based edge filtering**: Commands use `matches!(rel.rel_type, RelationshipType::...)` instead of string comparison. Note: `as_str()` on RelationshipType returns `SCREAMING_SNAKE_CASE`.
- **Cypher WHERE**: Supports `AND`, `OR`, `NOT` with correct precedence (NOT > AND > OR). Operators: `=`, `<>`, `!=`, `CONTAINS`, `STARTS WITH`, `ENDS WITH`. Example: `WHERE n.name STARTS WITH 'handle' AND NOT n.filePath ENDS WITH '.test.ts'`.
- **Cypher DISTINCT**: `RETURN DISTINCT n.name` deduplicates results before ORDER BY/LIMIT.
- **Pipeline phases**: Each phase in `crates/code-explorer-ingest/src/phases/` takes the graph and enriches it. Phases are sequential but file processing within each phase is parallel via rayon.
- **Database adapter pattern**: `DatabaseBackend` trait in `crates/code-explorer-db/src/adapter.rs` abstracts storage. The in-memory backend is always available; KuzuDB is opt-in.
- **Runtime data**: Indexed repos store data in `.codeexplorer/` (meta.json, graph.bin, csv/).
- **DOCX brand customisation** (`crates/code-explorer-cli/src/commands/export_docx.rs`): `BrandConfig` loaded from `~/.codeexplorer/brand.json` (or `$CODE_EXPLORER_BRAND_FILE`) overrides client_name / company_name / footer_text / document_title in header, footer, title page, and `docProps/{core,app}.xml`. Missing file = `agile-up.com` defaults — binary stays usable without setup.
- **DOCX Mermaid rendering**: Code-fence ```mermaid``` blocks are POSTed to Kroki (`https://kroki.io/mermaid/png`, 15s timeout) and embedded as `<w:drawing>` images. PNG dimensions parsed from IHDR for aspect-ratio EMU sizing. Override URL via `CODE_EXPLORER_KROKI_URL` or fall back to text via `CODE_EXPLORER_MERMAID_PLACEHOLDER=1`. See `mermaid_to_xml` in `export_docx.rs`.
- **Pre-delivery linter**: `code-explorer validate-docs --repo <project>` walks `.codeexplorer/docs/**/*.md` and reports residual TODOs / unfilled `<!-- GNX:* -->` anchors / broken markdown links / short sections / missing §4 Algorithmes (Sample méthodo). Exits with code 2 on RED — usable as CI gate. Implemented in `crates/code-explorer-cli/src/commands/validate_docs.rs`.

## Gotchas

- **MCP stdout is sacred**: MCP mode logs to stderr to avoid polluting the stdout JSON-RPC stream. Never add `println!` in MCP code paths.
- **Snapshot is JSON, not bincode**: Despite the filename `graph.bin`, snapshots are JSON. Bincode is incompatible because `NodeProperties` uses `#[serde(skip_serializing_if)]` on ~40 optional fields, which breaks bincode's positional format. Migrating requires either removing skip attributes (breaking JSON API), using bincode 2.x `Encode`/`Decode` traits, or switching to MessagePack (`rmp-serde`).
- **cxx-build pin on Windows**: `cxx-build` is pinned to `=1.0.138` to match kuzu's `cxx` version. Newer cxx-build encodes patch versions into bridge symbol names, causing `LNK2019` linker errors on Windows. Do not bump without verifying kuzu compatibility.
- **Adding a new language**: Implement `LanguageProvider` in `crates/code-explorer-lang/src/languages/`, add query strings in `queries/`, an import resolver in `import_resolvers/`, and register in `registry.rs`.
- **LLM enrichment Authorization header**: `call_structured_llm` in `crates/code-explorer-cli/src/commands/generate/enrichment.rs` MUST set the `Authorization: Bearer <key>` header for OpenAI-compatible endpoints (Gemini via `generativelanguage.googleapis.com/v1beta/openai`). Missing header → HTTP 400 "Missing or invalid Authorization header" on section-mode calls (fixed L944-946).
- **Sectioned enrichment anchors**: Pages >50KB are split into multiple LLM calls per anchor (`INTRO`, `SERVICES`, `ENTITIES`, etc.). Before the Phase A fix, only `INTRO` anchor was supported — non-INTRO anchors never triggered sectioned mode, causing systematic truncation on modules like `dossiers.md`. Fix is in `enrichment.rs` L1266-1331.
- **LLM response cache**: Enrichment responses are cached in `<repo>/.codeexplorer/docs/_meta/cache/llm/*.txt` keyed by MD5 of the full request body. A re-run reuses all cached responses gratis — extremely useful for retry with different models/settings without re-burning tokens.
- **Gemini Flash output ceiling**: Gemini 2.5 Flash truncates at ~65K output tokens (`finish_reason: length`). On large pages this fires constantly. The fallback freeform parser recovers ~60% of truncated responses; the rest go to the auto retry queue with reduced scope. For quality runs on large repos, prefer Gemini 3.1 Pro Preview (65K native, fewer truncations).
- **Big-context model fallback** (`LlmConfig::for_payload`, `enrichment.rs:1681`): set `big_context_model` (+ optional `big_context_threshold_bytes` default 40_000, `big_context_max_tokens`) in `~/.codeexplorer/chat-config.json` to route huge pages through a long-context model (e.g. `gemini-2.5-pro`). All LLM calls for that page (sectioned, monolithic, freeform fallback, review pass) use the substituted model — designed to escape the Flash 65K ceiling without paying Pro tier on small pages. Substitution is no-op when the field is unset.
- **Enrichment knobs in chat-config.json** (`EnrichmentConfig` in `enrichment.rs`): tune-without-rebuild for the constants that previously forced a `cargo build --release` to change. Add an optional `enrichment` block to `~/.codeexplorer/chat-config.json`: `sectionMaxTokens` (default 4096 — per-section LLM call), `monolithicMaxTokensFloor` (default 65_536 — full-page floor), `sectionContentSnippetBytes` (default 3000 — snippet cap), and `profiles.{fast,quality,strict}.{maxEvidence,maxRetries,timeoutSecs,minGapMs,useJsonSchema}` for per-profile overrides. All fields optional; absent block = exact prior behavior. `thinking_boost` and `review_critical` are intentionally NOT exposed — they encode the named profile's intent.
- **Reranker output tolerance**: `LlmReranker::parse_indices` in `crates/code-explorer-search/src/reranker/llm.rs` must tolerate truncated JSON arrays (`[1, 2, 0` without closing bracket). Gemini Flash cuts mid-response when max_tokens hits the ceiling, and the salvage parser scans digit runs after `[` to recover indices. Do not tighten the parser to strict JSON — it will fail on real production output.
- **Semantic search workflow**: 1) `code-explorer analyze <repo>` indexes the graph; 2) `code-explorer embed --model ~/.codeexplorer/models/<model>/model.onnx` generates `.codeexplorer/embeddings.bin` (+ `embeddings.meta.json` sidecar with the EmbeddingConfig); 3) `code-explorer query "foo" --hybrid` fuses BM25 with cosine top-K via RRF. `--rerank` can stack on top for LLM post-reranking. Default model for testing: `Xenova/all-MiniLM-L6-v2` (384d, English, ~90MB). For French content (sample-app / agile-up.com) upgrade to BGE-M3 or Qwen3-Embedding.
- **Embedding body dilution**: `code-explorer embed` uses `name + file_path + description + content` as input text. For very large functions (>500 lines) the body dilutes the sematic signal — observed on `enrich_aspnet_mvc` (1000+ LOC) which dropped out of top-5 for "ASP.NET MVC controller action extraction" under hybrid search when it was #4 under BM25. Truncating `content` to ~500 chars before embedding is a pending optimization.
- **ort 2.0.0-rc.12 + ndarray version skew**: `ort` ships with its own vendored `ndarray` version that differs from the workspace's 0.16, so `Tensor::from_array(Array2)` fails to resolve `OwnedTensorArrayData`. Use the tuple form `Tensor::from_array((shape: [i64; N], vec: Vec<T>))` instead — works regardless of which ndarray the workspace pulls. See `crates/code-explorer-search/src/embeddings/mod.rs`.
- **Reranker config duplication**: `LlmConfig` + `load_llm_config` exist in three places (`code-explorer-cli`, `code-explorer-mcp`, `code-explorer-desktop` uses its own `ChatConfig`). Acceptable tech debt until a fourth caller appears — at that point promote to `code-explorer-core::llm::config`.

## Rust Version and Toolchain

- MSRV: 1.75
- Edition: 2021
- Release profile: thin LTO, single codegen unit, stripped binaries, opt-level 3

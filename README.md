# GitNexus

Graph-powered code intelligence for AI agents. GitNexus builds a knowledge graph from your codebase and exposes it via [MCP](https://modelcontextprotocol.io/) (Model Context Protocol) for AI-powered code analysis.

Written in Rust. Supports 14 programming languages. Ships with a desktop app and an HTML documentation generator.

[Version française](README.fr.md)
[Installation Guide](INSTALLATION.md)
[Ubuntu Installation](INSTALLATION_UBUNTU.md)
[Modernization Roadmap](MODERNIZATION.md)
[React Source/Graph Navigation Plan](REACT_SOURCE_GRAPH_NAVIGATION.md)

## Why GitNexus? (vs AI coding assistants alone)

AI coding assistants like Claude Code, Cursor, or Copilot read files **one at a time, on demand**. For a large project (800+ files), they must read dozens of files to understand a single call chain, they start from scratch every conversation, and they fill their context window with raw source code.

GitNexus solves this by pre-indexing your **entire** codebase into a knowledge graph of relationships.

| | AI assistant alone | AI assistant + GitNexus |
|---|---|---|
| **Relationships** | Must read each file to discover who calls what | Pre-computed graph: instant callers, callees, hierarchy |
| **Scale** | ~50 files in context max | 800+ files indexed, queryable in 1 command |
| **Persistence** | Starts from scratch each conversation | Graph persists on disk, always available |
| **Context efficiency** | Reading 50 files = full context, no room to think | Returns only relevant relationships, context stays free |
| **Impact analysis** | Impossible without reading the whole project | `gitnexus impact handleRequest` → full chain in 1 second |
| **Git analytics** | Would need to parse `git log` every time | Pre-computed hotspots, coupling, ownership |
| **Documentation** | Can write 1-2 pages per conversation | Generates 40+ page HTML site with diagrams, navigation, search |
| **Legacy frameworks** | Doesn't understand Telerik 2011, EDMX, jQuery→Controller mappings | Specialized parsers for ASP.NET MVC, EF6, Telerik, AJAX |
| **Multi-agent** | Limited to one tool | MCP server → works with Claude, Cursor, VS Code, any agent |
| **Offline** | Needs API | Graph works 100% local, no internet required |

**In short:** an AI assistant reads code. GitNexus **understands** the entire codebase structure. Together, the AI has a "brain" that already knows all relationships -- instead of reading 50 files to find what calls `PaymentService`, it runs one command and gets the answer instantly, without consuming context.

It's the difference between asking someone to **read a book** vs giving them the **index and table of contents**.

## Features

- **Knowledge Graph** -- Parses source code into a rich graph of symbols (functions, classes, modules, imports, calls, inheritance) with 50+ node types and typed relationships
- **14 Languages** -- JavaScript, TypeScript, Python, Java, C, C++, C#, Go, Rust, Ruby, PHP, Kotlin, Swift, Razor via tree-sitter
- **ASP.NET MVC 5 Deep Support** -- Controllers, actions, Razor views, Entity Framework 6 EDMX, Telerik/Kendo UI grids, jQuery/AJAX mapping, service/repository layer detection (see below)
- **HTML Documentation Generator** -- Professional "DeepWiki" HTML site with full-text search (Ctrl+K), Lucide icons, dynamic sidebar, syntax highlighting, copy buttons, reading time estimation, and automated cross-reference linking between symbols.
- **Interactive UX** -- Single-page application (SPA) with native browser history support, breadcrumbs, Previous/Next navigation, scroll spy TOC, mobile responsive design, and interactive Mermaid diagrams with click-to-zoom/fullscreen.
- **Documentation Chat** -- Generated HTML sites include a graph-powered assistant that renders Markdown, syntax-highlighted code, Mermaid diagrams, and exports conversations as Markdown or printable PDF.
- **Business Process Documentation** -- Automated generation of high-level functional reports (B1-B5) for complex flows like Payment Lifecycles, Calculation Engines, and Document Generation, featuring rich Mermaid Sequence and Flowchart diagrams.
- **LLM Enrichment** -- Optional `--enrich` mode that augments documentation with grounded LLM prose, structured JSON payloads, evidence citations, provenance tracking, and anti-hallucination validation.
- **Ask the Codebase** -- `gitnexus ask "question"` CLI command for graph-powered Q&A with streaming responses.
- **Page Feedback** -- Built-in feedback widget on every documentation page to track content quality and utility.
- **Desktop App** -- Tauri v2 desktop application with interactive graph visualization, treemap view, intelligent chat, and command palette (Ctrl+K)
- **Intelligent Chat** -- AI-powered code Q&A with streaming responses, query complexity analysis, multi-step research plans, and deep research mode. Supports Ollama, OpenAI, Anthropic, OpenRouter, and Gemini (with reasoning/thinking mode)
- **MCP Server** -- 27 tools accessible to any MCP-compatible AI agent (Claude, Cursor, VS Code, etc.)
- **Claude Code Skill** -- Built-in `/gitnexus` skill that lets Claude query the knowledge graph during your conversation, with automatic invocation on natural language questions
- **Code Health Report** -- `gitnexus report` command combining hotspots, temporal coupling, ownership analysis, and graph metrics into a single health score (A-E)
- **Hybrid Search** -- BM25 lexical search + optional ONNX semantic embeddings, fused with Reciprocal Rank Fusion. Optional LLM-based reranker for post-retrieval reordering with graceful fallback when the model is unavailable.
- **Blast Radius Analysis** -- Trace upstream callers, downstream callees, and transitive impact of any symbol
- **Interactive Modes** -- REPL shell, TUI dashboard, file watcher with auto-reindex
- **Pluggable Storage** -- In-memory backend (default) or KuzuDB graph database

## ASP.NET MVC 5 / Legacy .NET Support

GitNexus has deep support for legacy ASP.NET MVC 5 projects, making it ideal for documenting and understanding complex enterprise applications.

### What it detects

| Feature | Detection |
|---------|-----------|
| **Controllers & Actions** | Class inheritance, `[HttpGet/Post]`, `[GridAction]`, route templates, parameter signatures |
| **Razor Views** (.cshtml) | `@model`, `@layout`, `@Html.Partial`, `@Html.RenderAction`, `@Html.BeginForm` |
| **Entity Framework 6** | DbContext, DbSet, EDMX entities, associations, navigation properties, inheritance (TPH/TPT) |
| **Telerik / Kendo UI** | `Html.Telerik().Grid<T>()`, `Html.Kendo().Grid<T>()`, DataSource bindings (`.Select()`, `.Read()`), grid columns, `ClientEvents`, `DatePickerFor`, `DropDownListFor` |
| **jQuery / AJAX** | `$.ajax()`, `$.getJSON()`, `$.post()`, `$.get()`, `$.load()`, `fetch()`, `@Url.Action()` — linked to controller actions |
| **Service Layer** | `*Service`, `*Repository`, `*Manager`, `*Provider`, `*UnitOfWork` classes with interface detection |
| **Dependency Injection** | Autofac (`RegisterType<T>().As<I>()`), Unity, Ninject, MS DI |
| **Custom Attributes** | `[AuthorizeADAttribute]`, `[VerifActionFilter]`, any `[*Attribute]`, `[*Filter]`, `[*Action]` |
| **External Services** | WebAPI client detection (`new CMCASClient(httpClient)`), WCF service references, HTTP call tracing |
| **StackLogger Tracing** | Coverage analysis — identifies which methods are instrumented with `BeginMethodScope()` |
| **Base Controllers** | Custom controller inheritance (`RootController` → `Controller`) |
| **Web.config** | Configuration file detection |

### Generated documentation

The `generate html` command produces a DeepWiki-style HTML documentation site:

```bash
gitnexus analyze D:\path\to\your\mvc5-project
gitnexus generate --path D:\path\to\your\mvc5-project html
# Open .gitnexus/docs/index.html in your browser
```

The HTML site includes:
- **Overview** with technology stack, project structure, and interactive project metrics
- **Architecture diagram** (Mermaid) showing Presentation → Business Logic → Data Access layers
- **Business Processes** (B1-B5) with high-level flows for Courriers, Payments, and Calculation Engines
- **Per-controller pages** with action signatures, parameters (linked to data model), callers, and source code
- **Data model pages** with per-entity relationship diagrams and per-domain ER diagrams
- **Functional guide** with business descriptions in French, criticality levels, and Mermaid flow diagrams
- **Interactive Elements**: Zoomable Mermaid diagrams, clickable source files with copy-to-clipboard, and native browser history support
- **Embedded Assistant**: chat over the generated documentation with Markdown/PDF transcript exports, timestamps, and rendered Mermaid diagrams
- **Dark/light theme** toggle with sidebar search, breadcrumbs, and Previous/Next navigation

## Quick Start

### Prerequisites

| Dependency | Version | Required for | Install |
|-----------|---------|-------------|---------|
| **Rust** | 1.75+ | Everything | [rustup.rs](https://rustup.rs/) |
| **C/C++ compiler** | - | tree-sitter grammars | Windows: Visual Studio Build Tools. Linux: `apt install build-essential`. macOS: `xcode-select --install` |
| **Node.js** | 18+ | Desktop app frontend | [nodejs.org](https://nodejs.org/) |
| **git** | 2.0+ | Git analytics (hotspots, coupling, ownership) | Already installed on most systems |
| **CMake** | 3.15+ | KuzuDB backend (optional) | Windows: `winget install cmake`. Linux: `apt install cmake` |

### Install & Build

```bash
# 1. Clone
git clone https://github.com/phuetz/gitnexus-rs.git
cd gitnexus-rs

# 2. Build the CLI (release mode, ~35 MB binary)
cargo build --release -p gitnexus-cli

# The binary is at:
# Windows: target\release\gitnexus.exe
# Linux/macOS: target/release/gitnexus

# 3. (Optional) Build the Desktop App
cd crates/gitnexus-desktop/ui && npm install && npm run build && cd ../../..
cargo build -p gitnexus-desktop --release
```

Build scripts are also provided:

```bash
# Windows
build-release.bat           # Build CLI + Desktop
build-release.bat cli       # CLI only
build-release.bat desktop   # Desktop only

# Linux/macOS
./build-release.sh          # Build CLI + Desktop
./build-release.sh cli      # CLI only
```

Windows helper scripts for local development:

```powershell
.\config-chatgpt.cmd        # Configure ChatGPT OAuth with gpt-5.5
.\login-chatgpt.cmd         # ChatGPT OAuth login
.\gitnexus.cmd chat         # Backend 3010 + React chat 5176
.\gitnexus.cmd chat -ChatPort 5174  # Compatibility with an old browser tab
.\start-desktop.cmd         # Desktop UI + Tauri
.\check-gitnexus.cmd        # Main lint/test/build suite
```

The React chat shows the active LLM/model/reasoning level, timestamps each message, renders Mermaid and syntax-highlighted code, exports Markdown/PDF transcripts, and offers copyable diagnostics when the backend/project list is unreachable.

### Optional Feature Builds

```bash
# With KuzuDB graph database backend (for very large repos, requires CMake)
cargo build --release -p gitnexus-cli --features gitnexus-cli/kuzu-backend

# With ONNX semantic search (hybrid BM25 + embeddings)
cargo build --release -p gitnexus-cli --features gitnexus-search/embeddings

# With LLM reranker (post-processes top-K candidates via OpenAI-compatible API)
cargo build --release -p gitnexus-cli --features gitnexus-search/reranker-llm

# With everything (KuzuDB + embeddings + reranker)
cargo build --release -p gitnexus-cli --features gitnexus-cli/kuzu-backend,gitnexus-search/embeddings,gitnexus-search/reranker-llm
```

> **Note:** the default `gitnexus-cli` build already enables `embeddings` and `reranker-llm`. The lines above are explicit feature toggles for downstream crates that opt in à la carte.

### LLM Configuration (for `ask` and `--enrich`)

Create `~/.gitnexus/chat-config.json`:

```json
{
  "provider": "gemini",
  "api_key": "YOUR_API_KEY",
  "base_url": "https://generativelanguage.googleapis.com/v1beta/openai",
  "model": "gemini-2.5-flash",
  "max_tokens": 8192,
  "reasoning_effort": "high",

  "big_context_model": "gemini-2.5-pro",
  "big_context_threshold_bytes": 40000,
  "big_context_max_tokens": 131072
}
```

Supported providers: **Gemini**, **OpenAI**, **Anthropic**, **OpenRouter**, **Ollama** (local, no API key needed), and **ChatGPT** through the Codex-style OAuth login flow for `gitnexus ask` / `gitnexus serve` chat. `--enrich` still uses OpenAI-compatible API-key providers.

To use your ChatGPT subscription instead of an API key for `gitnexus ask` and the web chat served by `gitnexus serve`:

```bash
gitnexus login
```

Then set `~/.gitnexus/chat-config.json` to the ChatGPT provider. `api_key` is intentionally empty; the access token is loaded from the OAuth login created above.

```json
{
  "provider": "chatgpt",
  "api_key": "",
  "base_url": "https://chatgpt.com/backend-api/codex",
  "model": "gpt-5.5",
  "max_tokens": 8192,
  "reasoning_effort": "high"
}
```

The three optional `big_context_*` fields route huge pages (≥ `big_context_threshold_bytes` raw markdown, default 40 KB) through a long-context model to escape the Gemini 2.5 Flash 65K output ceiling that causes `finish_reason: length` truncations. All LLM calls for that page (sectioned, monolithic, freeform fallback, review pass) use the substituted model. Leave the fields unset for legacy single-model behavior.

#### Tuning enrichment without rebuilds

The constants that previously required a `cargo build --release` to change can be overridden via an optional `enrichment` block. All fields are optional — omit any field (or the whole block) to keep the prior behavior:

```json
{
  "enrichment": {
    "sectionMaxTokens": 4096,
    "monolithicMaxTokensFloor": 65536,
    "sectionContentSnippetBytes": 3000,
    "profiles": {
      "fast":    { "maxEvidence": 10, "maxRetries": 8, "timeoutSecs": 60,  "minGapMs": 500, "useJsonSchema": false },
      "quality": { "maxEvidence": 20, "maxRetries": 1, "timeoutSecs": 180, "minGapMs": 0,   "useJsonSchema": true },
      "strict":  { "maxEvidence": 30, "maxRetries": 2, "timeoutSecs": 300, "minGapMs": 0,   "useJsonSchema": true }
    }
  }
}
```

Common uses: lower `sectionMaxTokens` to save tokens on small pages; bump `quality.timeoutSecs` if Gemini is slow during EU peak hours; bump `fast.maxEvidence` for richer drafts. The values shown above are the hardcoded defaults.

Validate your config:

```bash
gitnexus config test
```

### Run the Desktop App (dev mode with hot reload)

```bash
cd crates/gitnexus-desktop
cargo tauri dev
```

## CLI Usage

### Analyze a project

```bash
# Index the current directory
gitnexus analyze

# Index a specific path (e.g., a legacy ASP.NET MVC project)
gitnexus analyze D:\path\to\project

# Force re-index (resets the graph)
gitnexus analyze D:\path\to\project --force
```

This creates a `.gitnexus/` directory containing the serialized knowledge graph.

### Generate documentation

```bash
# Generate HTML documentation site (recommended)
gitnexus generate --path D:\path\to\project html
# → Open .gitnexus/docs/index.html in your browser

# Generate with LLM enrichment (requires configured LLM)
gitnexus generate --path D:\path\to\project html --enrich
gitnexus generate --path D:\path\to\project html --enrich --enrich-profile strict
gitnexus generate --path D:\path\to\project html --enrich --enrich-lang en

# Generate everything (AGENTS.md, wiki, skills, docs, DOCX, HTML)
gitnexus generate --path D:\path\to\project all

# Generate specific formats
gitnexus generate --path D:\path\to\project docs     # Markdown pages
gitnexus generate --path D:\path\to\project docx     # Word document (header + footer + brand)
gitnexus generate --path D:\path\to\project pdf      # PDF (Puppeteer-based, from HTML)
gitnexus generate --path D:\path\to\project context  # AGENTS.md only
gitnexus generate --path D:\path\to\project wiki     # Wiki pages
gitnexus generate --path D:\path\to\project skills   # Skill files
gitnexus generate --path D:\path\to\project inject   # Re-inject LLM fragments without full regen
```

### Word DOCX — brand customisation

Generated `.docx` files now ship with a configurable header (client name + document title), a footer (brand text + auto-paginated `Page X / Y`), and Word `Fichier > Propriétés` metadata. Override the defaults via `~/.gitnexus/brand.json`:

```json
{
  "client_name": "CCAS Alise",
  "company_name": "agile-up.com",
  "footer_text": "agile-up.com — Confidentiel — Ne pas diffuser",
  "document_title": "Documentation Technique et Fonctionnelle"
}
```

A missing or unparseable `brand.json` silently falls back to the legacy `agile-up.com` defaults — the binary stays usable without setup. Override the file location with `$GITNEXUS_BRAND_FILE`.

Mermaid diagrams in the markdown source are rendered to PNG via [Kroki](https://kroki.io) and embedded inline in the `.docx`. Set `GITNEXUS_MERMAID_PLACEHOLDER=1` to keep the legacy text fallback (no network call), or `GITNEXUS_KROKI_URL=<url>` to point at a self-hosted Kroki instance.

### Pre-delivery validation (`validate-docs`)

Before sending a Word/HTML doc to a client, run the linter to catch issues that would embarrass us:

```bash
gitnexus validate-docs --repo D:\path\to\project
gitnexus validate-docs --repo D:\path\to\project --json   # machine-readable report
```

Five checks at two severity levels:

- **RED — blocks delivery**: residual `TODO` / `TBD` / `FIXME` / `XXX`, unfilled `<!-- GNX:* -->` anchors (means LLM enrichment silently failed), broken markdown links to missing files.
- **YELLOW — should fix**: H1/H2 sections with under 50 prose words, Service / Controller pages missing a `§4 Algorithmes` heading (Alise v1.1 méthodo).

The report is also written to `<docs_dir>/_meta/validation.json` for CI / scripted consumption. **Exits with code 2 if any RED issue is found** (otherwise 0) — wire it into your CI pipeline as a fail-fast gate.

### LLM Enrichment — resilience & off-peak retry

LLM enrichment (`--enrich`) can encounter 503 errors from the Gemini API during peak hours
(typically 09:00–11:00 Paris time). GitNexus handles this automatically with three layers
of protection:

**1. Retry with jitter** — each page that fails is retried up to 10 times with
exponential backoff (5s → 30s) and ±30% jitter, matching the
[gemini-cli](https://github.com/google-gemini/gemini-cli) strategy.

**2. Persistent failure queue** — pages that still fail after all retries are saved to
`_meta/queue.json`. On the next enrichment run the queue is loaded automatically and
already-cached pages are purged from it.

**3. Automatic deferred retry** — at the end of each `--enrich` run, if any pages are
queued, the pipeline waits 30 seconds (API recovery window) and retries them once before
saving the final queue state.

#### Recovering failed pages manually

```bash
# Resume enrichment without regenerating docs (uses cache for already-enriched pages)
gitnexus generate html --path D:\path\to\project --enrich-only

# Retry only the queued (failed) pages, right now
gitnexus generate html --path D:\path\to\project --retry-queue

# Schedule the retry for off-peak hours — process sleeps until HH:MM local time
gitnexus generate html --path D:\path\to\project --retry-queue --retry-at 02:00
```

The `--retry-at HH:MM` flag makes the process sleep until the specified local time, then
run the retry automatically. If the time has already passed today it will wait until
tomorrow. Leave the terminal open (or run in a background shell) — press Ctrl+C to cancel.

`--retry-at` also works with `--enrich`: enrichment runs immediately; if failures remain,
the deferred retry waits until the specified time instead of the default 30 seconds.

```bash
# Enrich now; if any pages fail, retry them at 02:00
gitnexus generate html --path D:\path\to\project --enrich --retry-at 02:00
```

#### Queue file

The queue is stored in `<docs_dir>/_meta/queue.json` alongside `provenance.json`. Each
entry records the page name, path, error, and cumulative attempt count. The file is
deleted automatically when the queue is fully drained.

```json
[
  {
    "page_name": "ctrl-statistiquecontroller",
    "page_path": "modules/ctrl-statistiquecontroller.md",
    "attempts": 3,
    "last_error": "503 Service Unavailable after 10 retries",
    "queued_at": "2026-04-17T09:45:00Z"
  }
]
```

### Ask the codebase

```bash
# Ask a question using graph context + LLM (streaming response)
gitnexus ask "comment fonctionne le calcul des barèmes ?"
gitnexus ask "which controllers call the Erable WebAPI?" --path D:\taf\Alise_v2
```

### Search & Explore

```bash
# Natural language search (BM25 only by default)
gitnexus query "authentication middleware"

# Hybrid search: BM25 + semantic embeddings fused via Reciprocal Rank Fusion
gitnexus query "authentication middleware" --hybrid

# Add LLM reranker on top of either BM25 or hybrid (post-retrieval reordering)
gitnexus query "authentication middleware" --hybrid --rerank

# 360-degree symbol context (callers, callees, imports, hierarchy)
gitnexus context UserService

# Blast radius analysis
gitnexus impact handleRequest --direction both

# Raw Cypher query
gitnexus cypher "MATCH (n:Function) RETURN n.name LIMIT 10"
```

### Semantic search workflow

To enable `--hybrid`, you first need to generate embeddings for the indexed graph.
The default model is `Xenova/all-MiniLM-L6-v2` (384d, ~90 MB), suitable for English
and most Latin-script content. For French or multilingual corpora, consider BGE-M3
or Qwen3-Embedding (configurable via `--model`).

```bash
# 1. Index the codebase as usual
gitnexus analyze D:\path\to\project

# 2. Build embeddings (writes .gitnexus/embeddings.bin + embeddings.meta.json sidecar)
gitnexus embed --repo D:\path\to\project --model ~/.gitnexus/models/all-MiniLM-L6-v2/model.onnx
gitnexus embed --repo D:\path\to\project --model ~/.gitnexus/models/bge-m3/model.onnx
gitnexus embed --repo D:\path\to\project --model ~/.gitnexus/models/all-MiniLM-L6-v2/model.onnx --batch 16

# 3. Query with hybrid retrieval; --rerank stacks an LLM reranker on top
gitnexus query "where is request cancellation handled?" --hybrid --repo D:\path\to\project
gitnexus query "where is request cancellation handled?" --hybrid --rerank --repo D:\path\to\project
```

The LLM reranker reuses `~/.gitnexus/chat-config.json` and falls back gracefully
to the non-reranked result list when the model is unavailable (network error,
truncated response, malformed JSON), so search remains usable even if the
reranking step fails.

### Interactive modes

```bash
gitnexus shell         # Interactive REPL with auto-completion
gitnexus dashboard     # TUI dashboard with graph navigation
gitnexus watch         # Watch & auto-reindex on file changes
```

### MCP Server (for AI agents)

```bash
# Stdio transport (for Claude, Cursor, VS Code, etc.)
gitnexus mcp

# Auto-configure MCP in your editor
gitnexus setup

# HTTP server
gitnexus serve         # Default port 3010
```

`gitnexus serve` defaults to `127.0.0.1`. If you bind it to a network
interface such as `0.0.0.0`, set `GITNEXUS_HTTP_TOKEN` first; the server
refuses non-loopback binds without that bearer token.

### Other commands

```bash
gitnexus list          # List indexed repositories with stats
gitnexus status        # Show index status for current repo
gitnexus clean         # Delete index
gitnexus clean --all   # Delete all indexed repos
gitnexus report        # Combined code health report (hotspots + coupling + ownership)
gitnexus report --json # Same, as JSON
```

### Full workflow example (ASP.NET MVC project)

```bash
# 1. Build the CLI
cargo build --release -p gitnexus-cli

# 2. Analyze the project
.\target\release\gitnexus.exe analyze D:\taf\MyLegacyApp

# 3. Generate HTML documentation
.\target\release\gitnexus.exe generate --path D:\taf\MyLegacyApp html

# 4. Optionally enrich with LLM (requires chat-config.json)
.\target\release\gitnexus.exe generate --path D:\taf\MyLegacyApp html --enrich

# 5. Open in browser
start D:\taf\MyLegacyApp\.gitnexus\docs\index.html

# 6. Ask questions about the code
.\target\release\gitnexus.exe ask "how does payment validation work?" --path D:\taf\MyLegacyApp

# 7. Or launch the desktop app for interactive exploration
.\target\release\gitnexus-desktop.exe
```

## Desktop App

The GitNexus desktop app is a Tauri v2 application with a React 19 frontend. It provides a visual interface for exploring your codebase's knowledge graph and an intelligent chat system for AI-powered code analysis.

### Graph Explorer

Interactive graph visualization powered by Cytoscape.js with three zoom levels (package, module, symbol), multiple layout algorithms, and click-to-navigate. Select any node to see its full context: callers, callees, imports, exports, and inheritance chain.

### Intelligent Chat

The chat system is the core feature of the desktop app. It goes beyond simple Q&A by analyzing query complexity and executing multi-step research plans when needed.

**Query Complexity Analysis** -- Every question is classified as Simple (direct lookup), Medium (2-3 operations), or Complex (multi-step DAG). The system detects intent patterns in both French and English: definition lookups, usage analysis, impact assessment, architecture questions, comparison, refactoring, and control flow.

**Multi-Step Research Plans** -- For medium and complex queries, the planner generates a dependency-aware DAG of research steps using five tools: `search_symbols`, `get_symbol_context`, `get_impact_analysis`, `execute_cypher`, and `read_file_content`. Steps execute in order with dependency tracking; if a step fails, dependents are gracefully skipped.

**Deep Research Mode** (Ctrl+Shift+D) -- Forces comprehensive multi-step analysis regardless of query complexity. Inspired by deep research agents, it maximizes coverage across the knowledge graph.

**IDE-Style Filtering** -- Scope your questions to specific parts of the codebase using three quick-pick modals inspired by VS Code and JetBrains:

- **File Picker** (Ctrl+P) -- Fuzzy-search files with language indicators and symbol counts
- **Symbol Picker** (Ctrl+Shift+O) -- Search symbols with kind-specific icons (function, class, interface, etc.)
- **Module Picker** -- Select community clusters by name and member count

Active filters appear as removable pills in the context bar above the chat input.

**Source References** -- Every answer includes expandable source cards with code snippets, relationship context (callers, callees, community), relevance scores, and one-click navigation to the graph node.

**Research Plan Viewer** -- Watch your complex queries execute step by step with a live progress bar, tool icons, status indicators, and expandable results per step.

### Keyboard Shortcuts

| Shortcut | Action |
|----------|--------|
| Ctrl+K | Open search modal |
| Ctrl+B | Toggle sidebar |
| Ctrl+1-5 | Switch sidebar tab (Repos, Files, Graph, Impact, Docs) |
| Ctrl+\\ | Close detail panel |
| Ctrl+Shift+D | Toggle deep research mode |
| F | Fit graph to screen |
| L | Cycle graph layouts |
| 1 / 2 / 3 | Switch zoom level (package / module / symbol) |
| Escape | Close modals, deselect nodes |

### Tech Stack

The desktop app is built with:

- **Backend**: Tauri v2 + Rust, direct access to KnowledgeGraph and indexes (no MCP envelope overhead)
- **Frontend**: React 19, TypeScript, Tailwind CSS 4, Vite 8
- **State**: Zustand 5 + TanStack React Query 5
- **Graph**: Cytoscape.js with react-cytoscapejs
- **Syntax**: Shiki for code highlighting
- **Markdown**: react-markdown with remark-gfm and rehype-raw

## MCP Tools

When running as an MCP server, GitNexus exposes these tools:

| Tool | Description |
|------|-------------|
| `list_repos` | List indexed repositories with stats |
| `query` | Natural language search across the knowledge graph |
| `context` | 360-degree view of a symbol: callers, callees, imports, exports, hierarchy |
| `impact` | Blast radius analysis -- upstream, downstream, or both |
| `detect_changes` | Analyze uncommitted changes and their impact |
| `rename` | Find all references that would need updating for a symbol rename |
| `cypher` | Execute a raw read-only Cypher query |

## AI Integration: Three Ways to Use GitNexus with AI

GitNexus offers three distinct approaches to AI-powered code intelligence, each with different trade-offs:

### 1. Claude Code Skill (`/gitnexus`) -- Recommended

A built-in [Claude Code skill](https://docs.anthropic.com/en/docs/claude-code) that lets Claude directly query the knowledge graph during your conversation.

```bash
# Just type in Claude Code:
/gitnexus query "authentication middleware"
/gitnexus impact UserService --direction upstream
/gitnexus report --path D:\taf\MyProject

# Or ask naturally -- Claude invokes the skill automatically:
"What calls the PaymentService?"  # → Claude runs gitnexus impact PaymentService
```

The skill is defined in `.claude/skills/gitnexus/SKILL.md` and works out of the box for anyone cloning the repo. A personal (global) version can be installed at `~/.claude/skills/gitnexus/SKILL.md` to use across all projects.

### 2. MCP Server (for any AI agent)

A standards-based [Model Context Protocol](https://modelcontextprotocol.io/) server exposing 27 tools. Works with Claude Desktop, Cursor, VS Code Copilot, and any MCP-compatible agent.

```bash
gitnexus mcp          # stdio transport
gitnexus serve        # HTTP transport (port 3010)
gitnexus setup        # Auto-configure in your editor
```

### 3. LLM API (`--enrich` and `ask`)

Direct LLM calls via OpenAI-compatible API for documentation enrichment and code Q&A. Requires `~/.gitnexus/chat-config.json`.

```bash
gitnexus ask "how does payment validation work?" --path D:\taf\MyProject
gitnexus generate html --path D:\taf\MyProject --enrich
```

### Comparison

| | Claude Code Skill | MCP Server | LLM API |
|---|---|---|---|
| **How it works** | Claude reads the graph directly via CLI | AI agent calls tools via JSON-RPC | GitNexus calls an external LLM |
| **AI model** | Claude (your current session) | Any MCP-compatible agent | Gemini, OpenAI, Anthropic, Ollama |
| **Setup** | Zero (skill is in the repo) | `gitnexus setup` | Config file + API key |
| **Latency** | Low (local CLI) | Low (local server) | Higher (API round-trip) |
| **Cost** | Included in Claude Code | Included in your agent | Per-token API cost |
| **Best for** | Interactive exploration, dev workflow | IDE integration, multi-agent setups | Documentation enrichment, batch Q&A |
| **Context** | Full conversation context + graph | Tool-scoped (per request) | Graph context only |

**When to use which:**
- **Claude Code Skill**: You're working in Claude Code and want to explore code interactively. Claude understands your conversation history AND the graph -- best for complex questions.
- **MCP Server**: You use Cursor, VS Code, or another MCP-compatible editor. The graph is always available as a tool.
- **LLM API**: You want to batch-enrich documentation or need a standalone Q&A command without an AI agent.

## Architecture

Workspace crates with a layered dependency flow:

```
gitnexus-cli              CLI binary ("gitnexus")
  |
  +-- gitnexus-mcp          MCP server (27 tools, stdio/HTTP, JSON-RPC 2.0)
  +-- gitnexus-search        Hybrid search (BM25 + semantic + RRF)
  +-- gitnexus-db            Database adapter (in-memory or KuzuDB)
  +-- gitnexus-ingest        6-phase ingestion pipeline (parallel via rayon)
  |     +-- gitnexus-lang    13 language providers (tree-sitter)
  +-- gitnexus-core          Core types: KnowledgeGraph, NodeLabel, SymbolTable

gitnexus-desktop           Tauri v2 desktop app
  +-- gitnexus-db            Direct graph/index/FTS access
  +-- gitnexus-search        Hybrid search
  +-- gitnexus-core          Core types
```

### Ingestion Pipeline

The pipeline runs 6 sequential phases, with parallel file processing within each phase:

1. **Structure** -- Filesystem walk, create File/Folder nodes
2. **Parsing** -- Tree-sitter AST extraction, symbol creation
3. **Imports** -- Import statement extraction and resolution
4. **Calls** -- Function/method call extraction and linking
5. **Heritage** -- Class inheritance and interface implementation
6. **Community** -- Community detection and clustering

For ASP.NET MVC projects, 14 additional enrichment passes run automatically:
- Controllers, actions, areas, route extraction
- EDMX entity/association parsing with inheritance
- Razor view analysis (@model, @layout, partials, forms)
- Telerik/Kendo grid detection with DataSource bindings and columns
- jQuery/AJAX → controller action mapping
- Service/repository layer with DI detection (Autofac, Unity, Ninject)
- Custom attribute/filter detection
- External service call tracing (WebAPI, WCF)
- StackLogger tracing coverage analysis
- Base controller inheritance tracking

### Desktop IPC Commands

The desktop app communicates with the Rust backend via Tauri IPC:

| Command | Description |
|---------|-------------|
| `list_repos` | List available repositories |
| `open_repo` | Open and load a repository |
| `get_graph_data` | Get full graph for visualization |
| `get_subgraph` | Get a filtered subgraph |
| `get_neighbors` | Get neighbors of a node |
| `search_symbols` | Full-text symbol search |
| `search_autocomplete` | Fast prefix search |
| `get_symbol_context` | 360-degree symbol context |
| `get_impact_analysis` | Blast radius analysis |
| `get_file_tree` | File tree for explorer |
| `read_file_content` | Read file source code |
| `execute_cypher` | Run Cypher queries |
| `chat_ask` | Simple chat Q&A |
| `chat_analyze_query` | Analyze query complexity |
| `chat_plan_research` | Generate research plan DAG |
| `chat_execute_step` | Execute a single research step |
| `chat_execute_plan` | Execute full research plan with synthesis |
| `chat_pick_files` | Quick-pick file search |
| `chat_pick_symbols` | Quick-pick symbol search |
| `chat_pick_modules` | Quick-pick module search |

## Feature Flags

| Flag | Default | Requires | Description |
|------|---------|----------|-------------|
| `kuzu-backend` | off | CMake 3.15+, C++ compiler | Persistent KuzuDB graph database backend (for repos too large for RAM) |
| `embeddings` | off | ONNX Runtime, HF tokenizers | Semantic search via ONNX embeddings, fused with BM25 via Reciprocal Rank Fusion. Enabled by default in the `gitnexus-cli` build. |
| `reranker-llm` | off | OpenAI-compatible LLM API | LLM-based post-retrieval reranker; reuses `~/.gitnexus/chat-config.json`; falls back gracefully on API failure. Enabled by default in the `gitnexus-cli` build. |
| `kotlin` | on | - | Kotlin tree-sitter grammar |
| `swift` | on | - | Swift tree-sitter grammar |

**Note (Windows):** The `kuzu-backend` feature requires `cxx-build = "=1.0.138"` pinned in the workspace to avoid CXX bridge symbol mismatch (see dtolnay/cxx#1507). This is already configured.

## Supported Languages

| Language | Extensions |
|----------|------------|
| JavaScript | `.js` `.jsx` `.mjs` `.cjs` |
| TypeScript | `.ts` `.tsx` `.mts` `.cts` |
| Python | `.py` `.pyi` |
| Java | `.java` |
| C | `.c` `.h` |
| C++ | `.cpp` `.hpp` `.cc` `.hh` `.cxx` `.hxx` |
| C# | `.cs` `.cshtml` `.edmx` `.config` |
| Go | `.go` |
| Rust | `.rs` |
| Ruby | `.rb` |
| PHP | `.php` |
| Kotlin | `.kt` `.kts` |
| Swift | `.swift` |
| Razor | `.cshtml` `.razor` |

## Origin & Credits

GitNexus-RS is a Rust-based, high-performance implementation and extension of the original **[GitNexus](https://github.com/abhigyanpatwari/GitNexus)** project by [Abhigyan Patwari](https://github.com/abhigyanpatwari).

While the original implementation is primarily TypeScript-based, this Rust version focuses on:
- **Performance**: High-speed parallel indexing of large repositories using Rayon and Tree-sitter.
- **Native Desktop Experience**: A Tauri v2 desktop application with built-in graph visualization.
- **Enterprise Enrichment**: Specialized deep-parsers for legacy enterprise stacks (ASP.NET MVC 5, EF6, Telerik).
- **Embedded Graph DB**: Tight integration with KuzuDB for low-memory, high-query-performance persistent storage.

We are deeply grateful for the original vision and architectural foundation laid out by the [GitNexus](https://github.com/abhigyanpatwari/GitNexus) project.

## License

PolyForm Noncommercial 1.0.0

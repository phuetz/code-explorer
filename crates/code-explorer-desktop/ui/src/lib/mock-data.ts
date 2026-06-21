/**
 * Mock data for browser-mode UI development.
 * When running outside Tauri, safeInvoke returns these instead of calling IPC.
 */

import type {
  RepoInfo,
  GraphPayload,
  SearchResult,
  SymbolContext,
  ImpactResult,
  FileTreeNode,
  FileContent,
  DocContent,
  ChatResponse,
  ChatConfig,
  ChatSource,
  QueryAnalysis,
  ResearchPlan,
  StepResult,
  ChatSmartResponse,
  FileQuickPick,
  SymbolQuickPick,
  ModuleQuickPick,
  AspNetStats,
} from "./tauri-commands";

const MOCK_REPOS: RepoInfo[] = [
  {
    name: "code-explorer",
    path: "C:\\Users\\patrice\\CascadeProjects\\code-explorer",
    indexedAt: String(Math.floor(Date.now() / 1000) - 7200),
    lastCommit: "feat: add pipeline progress events",
    files: 247,
    nodes: 3842,
    edges: 12650,
    communities: 18,
  },
  {
    name: "my-react-app",
    path: "D:\\Projects\\my-react-app",
    indexedAt: String(Math.floor(Date.now() / 1000) - 86400),
    lastCommit: "fix: resolve hydration mismatch",
    files: 89,
    nodes: 1205,
    edges: 4320,
    communities: 7,
  },
  {
    name: "api-gateway",
    path: "C:\\Users\\patrice\\work\\api-gateway",
    indexedAt: String(Math.floor(Date.now() / 1000) - 259200),
    lastCommit: "chore: bump dependencies",
    files: 156,
    nodes: 2180,
    edges: 8400,
    communities: 12,
  },
];

const MOCK_GRAPH: GraphPayload = {
  nodes: [
    // Core modules
    { id: "n1", label: "Module", name: "core", filePath: "src/core/mod.rs", startLine: 1, endLine: 30, isExported: true, language: "rust" },
    { id: "n2", label: "Module", name: "ingest", filePath: "src/ingest/mod.rs", startLine: 1, endLine: 25, isExported: true, language: "rust" },
    { id: "n3", label: "Module", name: "lang", filePath: "src/lang/mod.rs", startLine: 1, endLine: 40, isExported: true, language: "rust" },
    { id: "n4", label: "Module", name: "search", filePath: "src/search/mod.rs", startLine: 1, endLine: 20, isExported: true, language: "rust" },
    { id: "n5", label: "Module", name: "db", filePath: "src/db/mod.rs", startLine: 1, endLine: 30, isExported: true, language: "rust" },

    // Core data structures
    { id: "n6", label: "Struct", name: "KnowledgeGraph", filePath: "src/core/graph.rs", startLine: 10, endLine: 80, isExported: true, language: "rust" },
    { id: "n7", label: "Struct", name: "SymbolTable", filePath: "src/core/symbol.rs", startLine: 5, endLine: 50, isExported: true, language: "rust" },
    { id: "n8", label: "Struct", name: "NodeLabel", filePath: "src/core/types.rs", startLine: 20, endLine: 95, isExported: true, language: "rust" },
    { id: "n9", label: "Struct", name: "CodeRelation", filePath: "src/core/types.rs", startLine: 100, endLine: 140, isExported: true, language: "rust" },

    // Trait definitions
    { id: "n10", label: "Trait", name: "LanguageProvider", filePath: "src/lang/provider.rs", startLine: 5, endLine: 50, isExported: true, language: "rust" },
    { id: "n11", label: "Trait", name: "DatabaseBackend", filePath: "src/db/adapter.rs", startLine: 10, endLine: 60, isExported: true, language: "rust" },

    // Classes/Structs for key implementations
    { id: "n12", label: "Struct", name: "Parser", filePath: "src/ingest/parser.rs", startLine: 8, endLine: 75, isExported: true, language: "rust" },
    { id: "n13", label: "Struct", name: "Analyzer", filePath: "src/ingest/analyzer.rs", startLine: 1, endLine: 70, isExported: true, language: "rust" },
    { id: "n14", label: "Struct", name: "GraphBuilder", filePath: "src/core/builder.rs", startLine: 5, endLine: 85, isExported: true, language: "rust" },

    // Important functions - Parsing phase
    { id: "n15", label: "Function", name: "run_pipeline", filePath: "src/pipeline.rs", startLine: 42, endLine: 120, isExported: true, language: "rust" },
    { id: "n16", label: "Function", name: "parse_file", filePath: "src/ingest/parser.rs", startLine: 15, endLine: 80, isExported: true, language: "rust" },
    { id: "n17", label: "Function", name: "build_ast", filePath: "src/ingest/parser.rs", startLine: 85, endLine: 150, isExported: false, language: "rust" },

    // Important functions - Import phase
    { id: "n18", label: "Function", name: "resolve_imports", filePath: "src/ingest/imports.rs", startLine: 30, endLine: 95, isExported: false, language: "rust" },
    { id: "n19", label: "Function", name: "extract_symbols", filePath: "src/ingest/symbols.rs", startLine: 10, endLine: 70, isExported: false, language: "rust" },

    // Important functions - Community phase
    { id: "n20", label: "Function", name: "detect_communities", filePath: "src/ingest/community.rs", startLine: 12, endLine: 68, isExported: true, language: "rust" },

    // Search and database
    { id: "n21", label: "Struct", name: "SearchIndex", filePath: "src/search/index.rs", startLine: 8, endLine: 45, isExported: true, language: "rust" },
    { id: "n22", label: "Function", name: "analyze_deps", filePath: "src/ingest/deps.rs", startLine: 5, endLine: 60, isExported: false, language: "rust" },
  ],
  edges: [
    // Pipeline orchestration
    { id: "e1", source: "n15", target: "n16", relType: "CALLS", confidence: 1.0 },
    { id: "e2", source: "n15", target: "n18", relType: "CALLS", confidence: 1.0 },
    { id: "e3", source: "n15", target: "n20", relType: "CALLS", confidence: 1.0 },
    { id: "e4", source: "n15", target: "n22", relType: "CALLS", confidence: 0.95 },

    // Parsing phase functions
    { id: "e5", source: "n16", target: "n17", relType: "CALLS", confidence: 1.0 },
    { id: "e6", source: "n16", target: "n19", relType: "CALLS", confidence: 0.9 },
    { id: "e7", source: "n17", target: "n10", relType: "USES", confidence: 0.85 },

    // Functions creating/modifying structures
    { id: "e8", source: "n16", target: "n6", relType: "CREATES", confidence: 1.0 },
    { id: "e9", source: "n19", target: "n7", relType: "CREATES", confidence: 1.0 },
    { id: "e10", source: "n18", target: "n9", relType: "CREATES", confidence: 0.9 },
    { id: "e11", source: "n20", target: "n6", relType: "MODIFIES", confidence: 0.85 },
    { id: "e12", source: "n22", target: "n6", relType: "MODIFIES", confidence: 0.8 },

    // Classes and their relationships
    { id: "e13", source: "n12", target: "n16", relType: "CONTAINS", confidence: 1.0 },
    { id: "e14", source: "n13", target: "n18", relType: "CONTAINS", confidence: 1.0 },
    { id: "e15", source: "n14", target: "n6", relType: "CREATES", confidence: 1.0 },

    // Implementations and trait usage
    { id: "e16", source: "n12", target: "n10", relType: "IMPLEMENTS", confidence: 0.95 },
    { id: "e17", source: "n21", target: "n6", relType: "READS", confidence: 1.0 },

    // Module containment
    { id: "e18", source: "n1", target: "n6", relType: "CONTAINS", confidence: 1.0 },
    { id: "e19", source: "n1", target: "n14", relType: "CONTAINS", confidence: 1.0 },
    { id: "e20", source: "n2", target: "n12", relType: "CONTAINS", confidence: 1.0 },
    { id: "e21", source: "n2", target: "n13", relType: "CONTAINS", confidence: 1.0 },
    { id: "e22", source: "n3", target: "n10", relType: "CONTAINS", confidence: 1.0 },
    { id: "e23", source: "n4", target: "n21", relType: "CONTAINS", confidence: 1.0 },
    { id: "e24", source: "n5", target: "n11", relType: "CONTAINS", confidence: 1.0 },

    // Cross-module dependencies
    { id: "e25", source: "n2", target: "n3", relType: "USES", confidence: 0.9 },
  ],
  stats: { nodeCount: 22, edgeCount: 25, truncated: false },
};

const MOCK_SEARCH_RESULTS: SearchResult[] = [
  { nodeId: "n1", name: "run_pipeline", label: "Function", filePath: "src/pipeline.rs", score: 0.95, startLine: 42 },
  { nodeId: "n3", name: "KnowledgeGraph", label: "Struct", filePath: "src/core/graph.rs", score: 0.88, startLine: 10 },
  { nodeId: "n6", name: "LanguageProvider", label: "Trait", filePath: "src/lang/mod.rs", score: 0.82, startLine: 5 },
  { nodeId: "biz-courriers", name: "Système de Courriers", label: "Process", filePath: "business/processes", score: 0.9, startLine: 1 },
];

const MOCK_FILE_TREE: FileTreeNode[] = [
  {
    name: "src", path: "src", isDir: true, children: [
      { name: "main.rs", path: "src/main.rs", isDir: false, children: [] },
      { name: "pipeline.rs", path: "src/pipeline.rs", isDir: false, children: [] },
      { name: "parser.rs", path: "src/parser.rs", isDir: false, children: [] },
      {
        name: "core", path: "src/core", isDir: true, children: [
          { name: "graph.rs", path: "src/core/graph.rs", isDir: false, children: [] },
          { name: "types.rs", path: "src/core/types.rs", isDir: false, children: [] },
          { name: "mod.rs", path: "src/core/mod.rs", isDir: false, children: [] },
        ]
      },
      {
        name: "ingest", path: "src/ingest", isDir: true, children: [
          { name: "mod.rs", path: "src/ingest/mod.rs", isDir: false, children: [] },
          { name: "imports.rs", path: "src/ingest/imports.rs", isDir: false, children: [] },
          { name: "community.rs", path: "src/ingest/community.rs", isDir: false, children: [] },
        ]
      },
      {
        name: "lang", path: "src/lang", isDir: true, children: [
          { name: "mod.rs", path: "src/lang/mod.rs", isDir: false, children: [] },
          { name: "rust.rs", path: "src/lang/rust.rs", isDir: false, children: [] },
          { name: "typescript.rs", path: "src/lang/typescript.rs", isDir: false, children: [] },
        ]
      },
      {
        name: "search", path: "src/search", isDir: true, children: [
          { name: "index.rs", path: "src/search/index.rs", isDir: false, children: [] },
        ]
      },
    ]
  },
  { name: "Cargo.toml", path: "Cargo.toml", isDir: false, children: [] },
  { name: "README.md", path: "README.md", isDir: false, children: [] },
];

/** Mock file contents keyed by path, used by mockFileContent() */
const MOCK_FILES: Record<string, { content: string; language: string }> = {
  "src/main.rs": {
    language: "rust",
    content: `//! Code Explorer — Code intelligence powered by knowledge graphs.

mod core;
mod ingest;
mod lang;
mod search;

use clap::Parser;

#[derive(Parser)]
#[command(name = "code-explorer", about = "Code intelligence CLI")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    cli.command.run()
}`,
  },
  "src/pipeline.rs": {
    language: "rust",
    content: `use crate::core::KnowledgeGraph;
use crate::ingest::{resolve_imports, detect_communities};
use crate::parser::parse_file;

/// Run the full ingestion pipeline on a repository.
pub fn run_pipeline(repo_path: &str) -> Result<KnowledgeGraph, PipelineError> {
    let mut graph = KnowledgeGraph::new();

    // Phase 1: Structure — scan filesystem
    let files = scan_directory(repo_path)?;

    // Phase 2: Parsing — extract AST nodes
    for file in &files {
        parse_file(file, &mut graph)?;
    }

    // Phase 3: Imports — resolve cross-file references
    resolve_imports(&mut graph)?;

    // Phase 4: Calls — trace function invocations
    analyze_calls(&mut graph)?;

    // Phase 5: Heritage — class hierarchy
    build_heritage(&mut graph)?;

    // Phase 6: Communities — cluster detection
    detect_communities(&mut graph)?;

    Ok(graph)
}`,
  },
  "src/parser.rs": {
    language: "rust",
    content: `use tree_sitter::{Parser, Language};
use crate::core::KnowledgeGraph;
use crate::lang::LanguageProvider;

/// Parse a single source file and add its symbols to the graph.
pub fn parse_file(path: &str, graph: &mut KnowledgeGraph) -> Result<(), ParseError> {
    let source = std::fs::read_to_string(path)?;
    let provider = LanguageProvider::detect(path)?;

    let mut parser = Parser::new();
    parser.set_language(provider.language())?;

    let tree = parser.parse(&source, None)
        .ok_or(ParseError::TreeSitterFailed)?;

    let root = tree.root_node();
    provider.extract_symbols(root, &source, path, graph);

    Ok(())
}`,
  },
  "src/core/graph.rs": {
    language: "rust",
    content: `use std::collections::HashMap;

/// The central knowledge graph that holds all code intelligence data.
pub struct KnowledgeGraph {
    nodes: HashMap<String, Node>,
    edges: Vec<Edge>,
    symbol_table: SymbolTable,
}

impl KnowledgeGraph {
    pub fn new() -> Self {
        Self {
            nodes: HashMap::new(),
            edges: Vec::new(),
            symbol_table: SymbolTable::default(),
        }
    }

    pub fn add_node(&mut self, node: Node) {
        self.nodes.insert(node.id.clone(), node);
    }

    pub fn add_edge(&mut self, edge: Edge) {
        self.edges.push(edge);
    }

    pub fn node_count(&self) -> usize { self.nodes.len() }
    pub fn edge_count(&self) -> usize { self.edges.len() }
}`,
  },
  "src/core/types.rs": {
    language: "rust",
    content: `/// Labels for nodes in the knowledge graph.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum NodeLabel {
    Function, Class, Method, Module, File,
    Interface, Enum, Struct, Trait, Import,
}

/// Types of relationships between nodes.
#[derive(Debug, Clone, PartialEq)]
pub enum RelationshipType {
    Calls, Imports, Contains,
    Extends, Implements, Returns, Accepts,
}`,
  },
  "src/core/mod.rs": {
    language: "rust",
    content: `//! Core types and data structures for the knowledge graph.

mod graph;
mod types;

pub use graph::KnowledgeGraph;
pub use types::{NodeLabel, RelationshipType};`,
  },
  "src/ingest/mod.rs": {
    language: "rust",
    content: `//! Ingestion pipeline — transforms source code into a knowledge graph.

mod imports;
mod community;

pub use imports::resolve_imports;
pub use community::detect_communities;`,
  },
  "src/ingest/imports.rs": {
    language: "rust",
    content: `use crate::core::KnowledgeGraph;

/// Resolve cross-file import references and link them in the graph.
pub fn resolve_imports(graph: &mut KnowledgeGraph) -> Result<(), ImportError> {
    let import_nodes: Vec<_> = graph.nodes_by_label("Import").collect();

    for import in &import_nodes {
        if let Some(target) = graph.resolve_symbol(&import.name) {
            graph.add_edge(Edge::imports(&import.id, &target.id));
        }
    }

    Ok(())
}`,
  },
  "src/ingest/community.rs": {
    language: "rust",
    content: `use crate::core::KnowledgeGraph;

/// Detect code communities using the Louvain algorithm.
pub fn detect_communities(graph: &mut KnowledgeGraph) -> Result<(), CommunityError> {
    let adjacency = graph.to_adjacency_matrix();
    let communities = louvain::detect(&adjacency);

    for (node_id, community_id) in communities {
        graph.set_community(&node_id, community_id);
    }

    Ok(())
}`,
  },
  "src/lang/mod.rs": {
    language: "rust",
    content: `//! Language providers for tree-sitter parsing.

mod rust;
mod typescript;

use tree_sitter::Language;
use crate::core::KnowledgeGraph;

pub trait LanguageProvider {
    fn language(&self) -> Language;
    fn detect(path: &str) -> Option<Box<dyn LanguageProvider>>;
    fn extract_symbols(&self, root: tree_sitter::Node, source: &str, path: &str, graph: &mut KnowledgeGraph);
}`,
  },
  "src/lang/rust.rs": {
    language: "rust",
    content: `use tree_sitter::Language;
use super::LanguageProvider;

pub struct RustProvider;

impl LanguageProvider for RustProvider {
    fn language(&self) -> Language {
        tree_sitter_rust::language()
    }

    fn detect(path: &str) -> Option<Box<dyn LanguageProvider>> {
        if path.ends_with(".rs") { Some(Box::new(RustProvider)) } else { None }
    }

    fn extract_symbols(&self, root: tree_sitter::Node, source: &str, path: &str, graph: &mut KnowledgeGraph) {
        let query = include_str!("../../queries/rust.scm");
        self.run_query(query, root, source, path, graph);
    }
}`,
  },
  "src/lang/typescript.rs": {
    language: "rust",
    content: `use tree_sitter::Language;
use super::LanguageProvider;

pub struct TypeScriptProvider;

impl LanguageProvider for TypeScriptProvider {
    fn language(&self) -> Language {
        tree_sitter_typescript::language_typescript()
    }

    fn detect(path: &str) -> Option<Box<dyn LanguageProvider>> {
        if path.ends_with(".ts") || path.ends_with(".tsx") { Some(Box::new(Self)) } else { None }
    }

    fn extract_symbols(&self, root: tree_sitter::Node, source: &str, path: &str, graph: &mut KnowledgeGraph) {
        let query = include_str!("../../queries/typescript.scm");
        self.run_query(query, root, source, path, graph);
    }
}`,
  },
  "src/search/index.rs": {
    language: "rust",
    content: `use std::collections::HashMap;
use crate::core::KnowledgeGraph;

/// BM25-based full-text search index over symbol names and paths.
pub struct FtsIndex {
    terms: HashMap<String, Vec<DocRef>>,
    doc_lengths: Vec<f32>,
    avg_doc_length: f32,
}

impl FtsIndex {
    pub fn build(graph: &KnowledgeGraph) -> Self {
        let mut index = Self::default();
        for node in graph.all_nodes() {
            index.add_document(&node.id, &node.name, &node.file_path);
        }
        index.compute_stats();
        index
    }

    pub fn search(&self, query: &str, limit: usize) -> Vec<SearchResult> {
        let tokens = tokenize(query);
        self.bm25_score(&tokens, limit)
    }
}`,
  },
  "Cargo.toml": {
    language: "toml",
    content: `[workspace]
members = [
    "crates/code-explorer-core",
    "crates/code-explorer-lang",
    "crates/code-explorer-ingest",
    "crates/code-explorer-db",
    "crates/code-explorer-search",
    "crates/code-explorer-mcp",
    "crates/code-explorer-cli",
    "crates/code-explorer-desktop",
]

[workspace.package]
version = "0.1.0"
edition = "2021"
rust-version = "1.75"`,
  },
  "README.md": {
    language: "markdown",
    content: `# Code Explorer

Code intelligence powered by knowledge graphs.

## Features
- 13 languages supported via tree-sitter
- Knowledge graph built from AST analysis
- MCP server for AI-powered code exploration
- Desktop app with interactive graph visualization

## Quick Start
\`\`\`bash
cargo build --release
code-explorer analyze /path/to/project
code-explorer mcp
\`\`\``,
  },
};

/** Generate mock file content dynamically based on the filePath argument */
function mockFileContent(args?: Record<string, unknown>): FileContent {
  const filePath = (args?.filePath as string) || "unknown";
  const fileName = filePath.split("/").pop() || filePath;
  const ext = fileName.split(".").pop()?.toLowerCase() || "";

  if (filePath in MOCK_FILES) {
    const mock = MOCK_FILES[filePath];
    const lines = mock.content.split("\n").length;
    return { path: filePath, content: mock.content, language: mock.language, totalLines: lines };
  }

  // Fallback: generate a placeholder for unknown files
  const langMap: Record<string, string> = { rs: "rust", ts: "typescript", js: "javascript", py: "python", toml: "toml", md: "markdown" };
  const language = langMap[ext] || ext;
  const fallback = `// ${fileName}\n// (mock preview — content not available)`;
  return { path: filePath, content: fallback, language, totalLines: 2 };
}

/** Map of Tauri command names → mock responses (static values or functions that receive args) */
export const MOCK_RESPONSES: Record<string, unknown | ((args?: Record<string, unknown>) => unknown)> = {
  list_repos: MOCK_REPOS,
  open_repo: MOCK_REPOS[0],
  get_active_repo: "code-explorer",
  analyze_repo: "Analysis started",
  generate_docs: "Docs generated",

  get_graph_data: MOCK_GRAPH,
  get_subgraph: MOCK_GRAPH,
  get_neighbors: MOCK_GRAPH,

  search_symbols: MOCK_SEARCH_RESULTS,
  search_autocomplete: MOCK_SEARCH_RESULTS,

  get_symbol_context: {
    node: MOCK_GRAPH.nodes[0],
    callers: [{ id: "n4", name: "ingest", label: "Module", filePath: "src/ingest/mod.rs" }],
    callees: [
      { id: "n2", name: "parse_file", label: "Function", filePath: "src/parser.rs" },
      { id: "n5", name: "resolve_imports", label: "Function", filePath: "src/ingest/imports.rs" },
      { id: "n7", name: "detect_communities", label: "Function", filePath: "src/ingest/community.rs" },
    ],
    imports: [],
    importedBy: [],
    inherits: [],
    inheritedBy: [],
    community: { id: "c1", name: "Pipeline Core", description: "Main ingestion pipeline", memberCount: 5, cohesion: 0.85 },
  } satisfies SymbolContext,

  get_impact_analysis: {
    target: MOCK_GRAPH.nodes[0],
    upstream: [],
    downstream: [
      { node: MOCK_GRAPH.nodes[1], depth: 1, path: ["n1", "n2"] },
      { node: MOCK_GRAPH.nodes[4], depth: 1, path: ["n1", "n5"] },
    ],
    graph: MOCK_GRAPH,
    affectedFiles: ["src/parser.rs", "src/ingest/imports.rs"],
    summary: { upstreamCount: 0, downstreamCount: 2, affectedFilesCount: 2, maxDepth: 1 },
  } satisfies ImpactResult,

  get_file_tree: MOCK_FILE_TREE,
  read_file_content: mockFileContent,
  execute_cypher: [{ name: "run_pipeline", label: "Function" }],

  get_doc_index: null,
  read_doc: { path: "/", content: "# Documentation\n\nNo docs generated yet.", title: "Docs" } satisfies DocContent,
  has_docs: false,

  chat_ask: {
    answer: "The `run_pipeline` function orchestrates the 6-phase ingestion pipeline. It takes a repository path, scans files, parses ASTs, resolves imports, analyzes calls, builds class hierarchies, and detects communities.",
    sources: [
      { nodeId: "n1", symbolName: "run_pipeline", symbolType: "Function", filePath: "src/pipeline.rs", startLine: 42, relevanceScore: 0.95 },
    ] as ChatSource[],
    model: "mock-model",
  } satisfies ChatResponse,

  chat_get_config: {
    provider: "openai",
    apiKey: "",
    baseUrl: "https://api.openai.com/v1",
    model: "gpt-4",
    maxTokens: 4096,
    reasoningEffort: "",
  } satisfies ChatConfig,

  chat_set_config: undefined,
  chat_search_context: [] as ChatSource[],

  chat_analyze_query: {
    complexity: "medium",
    suggestedTools: ["search_symbols", "get_symbol_context"],
    estimatedSteps: 2,
    reasoning: "Query requires cross-file symbol lookup and context gathering.",
    keywords: ["pipeline", "function"],
    needsCrossFile: true,
    needsImpact: false,
  } satisfies QueryAnalysis,

  chat_plan_research: {
    id: "plan-mock-001",
    query: "How does the pipeline work?",
    analysis: {
      complexity: "medium",
      suggestedTools: ["search_symbols", "get_symbol_context"],
      estimatedSteps: 2,
      reasoning: "Query requires cross-file symbol lookup and context gathering.",
      keywords: ["pipeline", "function"],
      needsCrossFile: true,
      needsImpact: false,
    },
    steps: [
      {
        id: "step-1",
        order: 0,
        tool: "search_symbols",
        description: "Search for pipeline-related symbols",
        params: { query: "pipeline" },
        dependsOn: [],
        status: "pending",
      },
      {
        id: "step-2",
        order: 1,
        tool: "get_symbol_context",
        description: "Get context for top result",
        params: { nodeId: "n1" },
        dependsOn: ["step-1"],
        status: "pending",
      },
    ],
    status: "pending",
  } satisfies ResearchPlan,

  chat_execute_step: {
    summary: "Found 3 pipeline-related symbols in src/pipeline.rs.",
    sources: [
      { nodeId: "n1", symbolName: "run_pipeline", symbolType: "Function", filePath: "src/pipeline.rs", startLine: 42, relevanceScore: 0.95 },
    ] as ChatSource[],
    durationMs: 120,
  } satisfies StepResult,

  chat_execute_plan: {
    answer: "The pipeline orchestrates 6 phases: structure scanning, AST parsing, import resolution, call analysis, heritage building, and community detection.",
    sources: [
      { nodeId: "n1", symbolName: "run_pipeline", symbolType: "Function", filePath: "src/pipeline.rs", startLine: 42, relevanceScore: 0.95 },
    ] as ChatSource[],
    model: "mock-model",
    complexity: "medium",
  } satisfies ChatSmartResponse,

  chat_pick_files: [
    { path: "src/pipeline.rs", name: "pipeline.rs", language: "rust", symbolCount: 5 },
    { path: "src/core/graph.rs", name: "graph.rs", language: "rust", symbolCount: 8 },
  ] as FileQuickPick[],

  chat_pick_symbols: [
    { nodeId: "n1", name: "run_pipeline", kind: "Function", filePath: "src/pipeline.rs", startLine: 42 },
    { nodeId: "n6", name: "KnowledgeGraph", kind: "Struct", filePath: "src/core/graph.rs", startLine: 10 },
  ] as SymbolQuickPick[],

  chat_pick_modules: [
    { communityId: "c1", name: "Pipeline Core", memberCount: 5, description: "Main ingestion pipeline" },
    { communityId: "c2", name: "Language Providers", memberCount: 13, description: "Tree-sitter language support" },
  ] as ModuleQuickPick[],

  export_docs_docx: "/tmp/code-explorer-docs.docx",
  export_obsidian_vault: "/tmp/code-explorer-obsidian-vault",

  get_aspnet_stats: {
    controllers: 12,
    actions: 85,
    apiEndpoints: 4,
    views: 42,
    entities: 28,
    dbContexts: 2,
    areas: 1,
  } satisfies AspNetStats,

  // Git analytics — return empty arrays (no git history in mock mode)
  get_hotspots: [],
  get_coupling: [],
  get_ownership: [],

  // Features (community-based groups)
  get_features: [
    { id: "c1", name: "Pipeline Core", description: "Main ingestion pipeline", memberCount: 5, cohesion: 0.85 },
    { id: "c2", name: "Language Providers", description: "Tree-sitter language support", memberCount: 13, cohesion: 0.72 },
    { id: "c3", name: "Database Layer", description: "Storage and query execution", memberCount: 4, cohesion: 0.9 },
  ],

  // Code Health
  get_code_health: {
    overallScore: 78,
    grade: "B+",
    hotspotScore: 0.65,
    cohesionScore: 0.85,
    tracingCoverage: 0,
    ownershipScore: 0.72,
    fileCount: 247,
    nodeCount: 22,
    edgeCount: 25,
    avgComplexity: 4.2,
    maxComplexity: 12,
  },

  // Coverage
  get_coverage_stats: {
    totalMethods: 7,
    tracedMethods: 0,
    deadCodeCandidates: 1,
    coveragePct: 0,
    deadMethods: [
      { nodeId: "n12", name: "parse_file", filePath: "src/ingest/parser.rs", startLine: 10 },
    ],
  },

  // Diagrams
  get_diagram: { mermaid: "graph LR\n  A[core] --> B[ingest]\n  B --> C[lang]\n  A --> D[search]\n  A --> E[db]", title: "Module Dependencies" },

  // Process Flows
  get_process_flows: [
    {
      id: "biz-courriers",
      name: "Système de Courriers",
      processType: "business",
      stepCount: 5,
      steps: [
        { nodeId: "n1", name: "Selection", label: "ControllerAction", filePath: "CourrierController.cs" },
        { nodeId: "n2", name: "Generation", label: "Method", filePath: "CourrierService.cs" },
      ],
      mermaid: "sequenceDiagram\n  Utilisateur->>Controller: Selection\n  Controller->>Service: Generer",
    }
  ],
};

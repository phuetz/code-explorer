mod auth;
mod commands;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "code-explorer",
    about = "Graph-powered code intelligence for AI agents",
    version,
    author = "Patrice Huetz <patrice.huetz@gmail.com> — agile-up.com",
    long_about = "Code Explorer builds a knowledge graph from your codebase and exposes it \
                  via MCP (Model Context Protocol) for AI-powered code analysis.\n\n\
                  Built by Patrice Huetz (agile-up.com) — the code-intelligence companion to \
                  Code Buddy. Books: patricehuetz.fr."
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Index a repository into a knowledge graph
    #[command(
        after_help = "Examples:\n  code-explorer analyze\n  code-explorer analyze D:\\taf\\MyProject\n  code-explorer analyze --force --verbose\n  code-explorer analyze --llm-enrich"
    )]
    Analyze {
        /// Path to the repository (defaults to current directory)
        #[arg(default_value = ".")]
        path: String,
        /// Force re-index even if up-to-date
        #[arg(long)]
        force: bool,
        /// Generate embeddings for semantic search
        #[arg(long)]
        embeddings: bool,
        /// Enable verbose output
        #[arg(long)]
        verbose: bool,
        /// Skip git operations (index without git context)
        #[arg(long)]
        skip_git: bool,
        /// Use incremental indexing (only re-parse changed files)
        #[arg(long)]
        incremental: bool,
        /// Run LLM enrichment (Phase 8) — annotate symbols with code smells, patterns, risk scores
        #[arg(long)]
        llm_enrich: bool,
        /// Maximum total token budget for LLM enrichment (default: 100000)
        #[arg(long)]
        llm_token_budget: Option<u64>,
        /// Maximum number of symbols to enrich (0 = unlimited, bounded by budget)
        #[arg(long)]
        llm_max_symbols: Option<usize>,
    },
    /// Start MCP server (stdio transport)
    Mcp,
    /// Authenticate with ChatGPT (Codex OAuth) so the chat can use your
    /// ChatGPT Pro / Plus subscription instead of a separate API key.
    Login,
    /// Forget the cached ChatGPT auth tokens (`code-explorer login` reverses).
    Logout,
    /// Start HTTP server for web UI
    Serve {
        /// Port to listen on
        #[arg(short, long, default_value = "3010")]
        port: u16,
        /// Host address to bind (default: 127.0.0.1)
        #[arg(long, default_value = "127.0.0.1")]
        host: String,
    },
    /// List indexed repositories
    List,
    /// Show index status for the current directory
    Status,
    /// Delete Code Explorer index
    Clean {
        /// Skip confirmation prompt
        #[arg(long)]
        force: bool,
        /// Clean all indexed repositories
        #[arg(long)]
        all: bool,
    },
    /// Search the knowledge graph
    #[command(
        after_help = "Examples:\n  code-explorer query \"authentication middleware\"\n  code-explorer query \"user service\" --limit 5\n  code-explorer query \"where is RRF fusion\" --rerank"
    )]
    Query {
        /// Natural language search query
        query: String,
        /// Repository name or path
        #[arg(short, long)]
        repo: Option<String>,
        /// Maximum number of results
        #[arg(short, long, default_value = "10")]
        limit: usize,
        /// Post-process BM25 top-20 with an LLM reranker. Requires
        /// ~/.codeexplorer/chat-config.json. Returns only `limit` results.
        #[arg(long, default_value = "false")]
        rerank: bool,
        /// Fuse BM25 with semantic (embedding) search via Reciprocal Rank
        /// Fusion. Requires 'code-explorer embed' to have run first so
        /// .codeexplorer/embeddings.bin + embeddings.meta.json exist.
        #[arg(long, default_value = "false")]
        hybrid: bool,
    },
    /// Generate embeddings from the current snapshot and save to
    /// .codeexplorer/embeddings.bin for semantic search fusion.
    #[command(
        after_help = "Examples:\n  code-explorer embed --model ~/.codeexplorer/models/all-MiniLM-L6-v2/model.onnx"
    )]
    Embed {
        /// Path to the ONNX model (.onnx).
        #[arg(short, long)]
        model: String,
        /// Path to tokenizer.json (default: next to the .onnx).
        #[arg(short = 't', long)]
        tokenizer: Option<String>,
        /// Repository name or path.
        #[arg(short, long)]
        repo: Option<String>,
        /// Embedding dimension (default 384 for MiniLM-L6-v2).
        #[arg(long, default_value = "384")]
        dim: usize,
        /// Batch size for inference.
        #[arg(long, default_value = "32")]
        batch: usize,
        /// Max token length per input.
        #[arg(long, default_value = "512")]
        max_tokens: usize,
        /// Set for models that do NOT expect token_type_ids (some newer
        /// embedding exports). Default is true (BERT-family).
        #[arg(long, default_value = "false")]
        no_token_type_ids: bool,
    },
    /// 360-degree symbol view
    #[command(
        after_help = "Examples:\n  code-explorer context UserService\n  code-explorer context handleRequest --repo my-project"
    )]
    Context {
        /// Symbol name to look up
        name: String,
        /// Repository name or path
        #[arg(short, long)]
        repo: Option<String>,
    },
    /// Measure (and show) the context an LLM agent saves by using Code Explorer
    #[command(
        after_help = "Examples:\n  code-explorer demo\n  code-explorer demo /path/to/repo --symbol PaymentService"
    )]
    Demo {
        /// Repository path to demo on (defaults to the current directory)
        path: Option<String>,
        /// Symbol to demonstrate on (defaults to the most-called function)
        #[arg(short, long)]
        symbol: Option<String>,
    },
    /// Blast radius analysis
    #[command(
        after_help = "Examples:\n  code-explorer impact handleRequest --direction both\n  code-explorer impact UserService --direction upstream"
    )]
    Impact {
        /// Symbol name or node ID
        target: String,
        /// Repository name or path
        #[arg(short, long)]
        repo: Option<String>,
        /// Analysis direction: upstream, downstream, or both
        #[arg(short, long, default_value = "both")]
        direction: String,
    },
    /// Execute a raw Cypher query
    Cypher {
        /// Cypher query string
        query: String,
        /// Repository name or path
        #[arg(short, long)]
        repo: Option<String>,
    },
    /// Configure MCP for supported editors (VS Code, Cursor, etc.)
    Setup,
    /// Install Code Explorer as an MCP server for Claude Code (writes .mcp.json)
    #[command(name = "mcp-install")]
    McpInstall {
        /// Installation scope: "project" (default, writes .mcp.json in CWD) or "global" (writes ~/.mcp.json)
        #[arg(long, default_value = "project")]
        scope: String,
    },
    /// Launch interactive REPL shell for exploring the knowledge graph
    Shell {
        /// Path to the repository (defaults to current directory)
        path: Option<String>,
    },
    /// Generate documentation from the knowledge graph
    #[command(
        after_help = "Examples:\n  code-explorer generate html --path D:\\taf\\MyProject\n  code-explorer generate agent-skill --path D:\\taf\\MyProject\n  code-explorer generate codex-skill --path D:\\taf\\MyProject\n  code-explorer generate pdf --input D:\\taf\\docs\\V2\n  code-explorer generate html --enrich --enrich-profile strict\n  code-explorer generate all --path D:\\taf\\MyProject\n  code-explorer generate inject --path D:\\taf\\MyProject --input inject.json"
    )]
    Generate {
        /// Target: context | agents | agent-skill | codex-skill | claude-skill | wiki | skills | docs | docx | pdf | html | obsidian | process-doc | inject | all
        #[arg(
            help = "Target: context | agents | agent-skill | codex-skill | claude-skill | wiki | skills | docs | docx | pdf | html | obsidian | process-doc | inject | all"
        )]
        what: String,
        /// Path to the repository (defaults to current directory)
        #[arg(short, long)]
        path: Option<String>,
        /// Output directory for generated documentation (defaults to .codeexplorer/docs)
        #[arg(short, long)]
        output_dir: Option<String>,
        /// Input Markdown file or directory (standalone mode for pdf, skips knowledge graph)
        #[arg(long)]
        input: Option<String>,
        /// Enrich documentation with LLM-generated prose (requires configured LLM)
        #[arg(long, default_value_t = false)]
        enrich: bool,
        /// Enrichment profile: fast, quality, or strict
        #[arg(long, default_value = "quality")]
        enrich_profile: String,
        /// Documentation language: auto, fr, or en
        #[arg(long, default_value = "fr")]
        enrich_lang: String,
        /// Include source citations in enriched pages
        #[arg(long, default_value_t = true)]
        enrich_citations: bool,
        /// Skip doc regeneration and only run the enrichment + HTML pass (resume after interruption)
        #[arg(long, default_value_t = false)]
        enrich_only: bool,
        /// Retry only the pages listed in _meta/queue.json (previously failed enrichments)
        #[arg(long, default_value_t = false)]
        retry_queue: bool,
        /// Schedule the retry for a specific local time (HH:MM). The process sleeps until
        /// then and runs automatically — useful for off-peak hours (e.g. --retry-at 02:00)
        #[arg(long, value_name = "HH:MM")]
        retry_at: Option<String>,
        /// Directory containing execution trace files for process documentation
        #[arg(long)]
        traces_dir: Option<String>,
    },
    /// Watch a repository for changes and incrementally update the knowledge graph
    Watch {
        /// Path to the repository (defaults to current directory)
        path: Option<String>,
    },
    /// Launch interactive TUI dashboard for exploring the knowledge graph
    Dashboard {
        /// Path to the repository (defaults to current directory)
        path: Option<String>,
    },
    /// Show file-level hotspots (most changed files)
    Hotspots {
        /// Only consider commits from the last N days
        #[arg(long, default_value = "90")]
        since: u32,
        /// Path to the repository (defaults to current directory)
        #[arg(short, long)]
        path: Option<String>,
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// Show temporally coupled file pairs
    Coupling {
        /// Minimum number of shared commits
        #[arg(long, default_value = "3")]
        min: u32,
        /// Path to the repository (defaults to current directory)
        #[arg(short, long)]
        path: Option<String>,
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// Show file ownership by author
    Ownership {
        /// Path to the repository (defaults to current directory)
        #[arg(short, long)]
        path: Option<String>,
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// Ask a question about the codebase using the knowledge graph + LLM
    #[command(
        after_help = "Examples:\n  code-explorer ask \"how does authentication work?\"\n  code-explorer ask \"quels controllers appellent le WebAPI?\" --path D:\\taf\\MyProject"
    )]
    Ask {
        /// The question to ask
        question: String,
        /// Path to the repository (defaults to current directory)
        #[arg(short, long)]
        path: Option<String>,
    },
    /// Generate a combined code health report (hotspots + coupling + ownership + graph stats)
    #[command(
        after_help = "Examples:\n  code-explorer report\n  code-explorer report --path D:\\taf\\MyProject\n  code-explorer report --json"
    )]
    Report {
        /// Path to the repository (defaults to current directory)
        #[arg(short, long)]
        path: Option<String>,
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// Test LLM configuration (validate API key and connectivity)
    #[command(after_help = "Examples:\n  code-explorer config test")]
    Config {
        /// Subcommand: test
        action: String,
    },
    /// List all source files involved in a feature (BFS traversal from a symbol)
    #[command(
        after_help = "Examples:\n  code-explorer trace-files CourrierController\n  code-explorer trace-files BenefService --depth 3 --json"
    )]
    TraceFiles {
        /// Symbol name to trace from (controller, service, class, method)
        target: String,
        /// Path to the repository (defaults to current directory)
        #[arg(short, long)]
        path: Option<String>,
        /// Maximum traversal depth (default: 3)
        #[arg(long, default_value = "3")]
        depth: usize,
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// Generate Mermaid diagrams from the knowledge graph
    #[command(
        after_help = "Examples:\n  code-explorer diagram CourrierController --type flowchart\n  code-explorer diagram BenefService --format slides\n  code-explorer diagram CourrierController --type class --output diagram.md"
    )]
    Diagram {
        /// Symbol name to diagram
        target: String,
        /// Diagram type: flowchart, sequence, or class
        #[arg(long, default_value = "flowchart")]
        r#type: String,
        /// Output format: mermaid, slides
        #[arg(long, default_value = "mermaid")]
        format: String,
        /// Path to the repository (defaults to current directory)
        #[arg(short, long)]
        path: Option<String>,
        /// Output to file instead of stdout
        #[arg(short, long)]
        output: Option<String>,
    },
    /// Import execution traces (log files) to enrich the knowledge graph
    #[command(
        after_help = "Examples:\n  code-explorer trace-import D:\\logs\\production.log\n  code-explorer trace-import trace.csv --path D:\\taf\\MyProject"
    )]
    TraceImport {
        /// Path to the log/trace file
        file: String,
        /// Path to the repository (defaults to current directory)
        #[arg(short, long)]
        path: Option<String>,
    },

    /// Generate documentation from an execution trace using LLM
    #[command(
        after_help = "Examples:\n  code-explorer trace-doc trace.json\n  code-explorer trace-doc trace.json --output doc.md"
    )]
    TraceDoc {
        /// Path to the JSON trace file
        file: String,
        /// Output file for the documentation (markdown)
        #[arg(short, long)]
        output: Option<String>,
        /// Path to the repository (defaults to current directory)
        #[arg(short, long)]
        path: Option<String>,
    },

    /// Import external documentation (Markdown, DOCX) for GraphRAG
    #[command(
        after_help = "Examples:\n  code-explorer rag-import ./docs\n  code-explorer rag-import D:\\Specs --path D:\\taf\\MyProject\n\nSupported formats: .md (Markdown), .docx (Microsoft Word OOXML). Other files are silently skipped."
    )]
    RagImport {
        /// Path to the documentation directory
        docs_dir: String,
        /// Path to the repository (defaults to current directory)
        #[arg(short, long)]
        path: Option<String>,
    },

    /// Analyze tracing coverage and detect dead code
    #[command(
        after_help = "Examples:\n  code-explorer coverage CourriersService\n  code-explorer coverage --json\n  code-explorer coverage CourriersService --path D:\\taf\\sample-app"
    )]
    Coverage {
        /// Class/Service name to analyze (omit for global report)
        target: Option<String>,
        /// Path to the repository (defaults to current directory)
        #[arg(short, long)]
        path: Option<String>,
        /// Output as JSON
        #[arg(long)]
        json: bool,
        /// Trace the full call flow and show coverage along the chain
        #[arg(long)]
        trace: bool,
    },
    /// Lint generated documentation before delivery (residual TODOs, broken
    /// links, unfilled GNX anchors, missing §4 Algorithmes per Sample méthodo).
    /// Exits with code 2 if any RED-severity issue is found.
    #[command(
        after_help = "Examples:\n  code-explorer validate-docs\n  code-explorer validate-docs --repo D:\\taf\\sample-app\n  code-explorer validate-docs --docs-dir D:\\taf\\sample-app\\livrables\\2026-04-26\n  code-explorer validate-docs --json"
    )]
    ValidateDocs {
        /// Path to the repository (defaults to current directory)
        #[arg(long)]
        repo: Option<String>,
        /// Override the docs directory (default: <repo>/.codeexplorer/docs).
        /// Use this to validate a custom output directory, e.g. one set via
        /// `code-explorer generate --output-dir`.
        #[arg(long)]
        docs_dir: Option<String>,
        /// Output the JSON report to stdout instead of the colored console summary
        #[arg(long)]
        json: bool,
    },
    /// Process a Word questionnaire from the CLI (see `code-explorer workdoc --help`)
    Workdoc {
        /// Arguments forwarded to the workdoc command parser.
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let raw_args: Vec<String> = std::env::args().collect();
    if raw_args.get(1).is_some_and(|arg| arg == "workdoc") {
        let subscriber = tracing_subscriber::fmt()
            .with_max_level(tracing::Level::INFO)
            .with_writer(std::io::stderr)
            .finish();
        let _ = tracing::subscriber::set_global_default(subscriber);
        return commands::workdoc::run_from_args(raw_args.into_iter().skip(2).collect()).await;
    }

    let cli = Cli::parse();

    // Initialize logging
    let log_level = match &cli.command {
        Commands::Analyze { verbose, .. } if *verbose => tracing::Level::DEBUG,
        Commands::Mcp => tracing::Level::WARN, // Quiet for stdio MCP
        _ => tracing::Level::INFO,
    };

    // For MCP mode, log to stderr to avoid polluting stdout JSON-RPC
    let subscriber = tracing_subscriber::fmt()
        .with_max_level(log_level)
        .with_writer(std::io::stderr)
        .finish();
    tracing::subscriber::set_global_default(subscriber).expect("Failed to set tracing subscriber");

    match cli.command {
        Commands::Analyze {
            path,
            force,
            embeddings,
            verbose,
            skip_git,
            incremental,
            llm_enrich,
            llm_token_budget,
            llm_max_symbols,
        } => {
            commands::analyze::run(
                &path,
                force,
                embeddings,
                verbose,
                skip_git,
                incremental,
                llm_enrich,
                llm_token_budget,
                llm_max_symbols,
            )
            .await
        }
        Commands::Mcp => commands::mcp::run().await,
        Commands::Login => match auth::login().await {
            Ok(()) => Ok(()),
            Err(e) => Err(anyhow::anyhow!("login failed: {e}")),
        },
        Commands::Logout => match auth::clear() {
            Ok(()) => {
                println!("Cleared cached ChatGPT credentials.");
                Ok(())
            }
            Err(e) => Err(anyhow::anyhow!("logout failed: {e}")),
        },
        Commands::Serve { port, host } => commands::serve::run(port, &host).await,
        Commands::List => commands::list::run(),
        Commands::Status => commands::status::run(),
        Commands::Clean { force, all } => commands::clean::run(force, all),
        Commands::Query {
            query,
            repo,
            limit,
            rerank,
            hybrid,
        } => commands::query_cmd::run(&query, repo.as_deref(), limit, rerank, hybrid).await,
        Commands::Embed {
            model,
            tokenizer,
            repo,
            dim,
            batch,
            max_tokens,
            no_token_type_ids,
        } => {
            commands::embed::run(
                &model,
                tokenizer.as_deref(),
                repo.as_deref(),
                dim,
                batch,
                max_tokens,
                no_token_type_ids,
            )
            .await
        }
        Commands::Context { name, repo } => {
            commands::context_cmd::run(&name, repo.as_deref()).await
        }
        Commands::Demo { path, symbol } => {
            commands::demo::run(path.as_deref(), symbol.as_deref()).await
        }
        Commands::Impact {
            target,
            repo,
            direction,
        } => commands::impact_cmd::run(&target, repo.as_deref(), &direction).await,
        Commands::Cypher { query, repo } => {
            commands::cypher_cmd::run(&query, repo.as_deref()).await
        }
        Commands::Setup => commands::setup::run(),
        Commands::McpInstall { scope } => commands::mcp_install::run(&scope),
        Commands::Shell { path } => commands::shell::run(path.as_deref()).await,
        Commands::Generate {
            what,
            path,
            output_dir,
            input,
            enrich,
            enrich_profile,
            enrich_lang,
            enrich_citations,
            enrich_only,
            retry_queue,
            retry_at,
            traces_dir,
        } => commands::generate::run(
            &what,
            path.as_deref(),
            output_dir.as_deref(),
            enrich,
            &enrich_profile,
            &enrich_lang,
            enrich_citations,
            enrich_only,
            retry_queue,
            retry_at.as_deref(),
            traces_dir.as_deref(),
            input.as_deref(),
        ),
        Commands::Watch { path } => commands::watch::run(path.as_deref()).await,
        Commands::Dashboard { path } => commands::dashboard::run(path.as_deref()),
        Commands::Hotspots { since, path, json } => {
            commands::hotspots::run(since, path.as_deref(), json)
        }
        Commands::Coupling { min, path, json } => {
            commands::coupling_cmd::run(min, path.as_deref(), json)
        }
        Commands::Ownership { path, json } => commands::ownership_cmd::run(path.as_deref(), json),
        Commands::Ask { question, path } => commands::ask::run(&question, path.as_deref()).await,
        Commands::Report { path, json } => commands::report::run(path.as_deref(), json),
        Commands::Config { action } => match action.as_str() {
            "test" => tokio::task::spawn_blocking(commands::config_cmd::run_test)
                .await
                .map_err(|e| anyhow::anyhow!("{}", e))?,
            _ => {
                println!(
                    "Unknown config action: {}. Use 'code-explorer config test'.",
                    action
                );
                Ok(())
            }
        },
        Commands::TraceFiles {
            target,
            path,
            depth,
            json,
        } => commands::trace_files::run(&target, path.as_deref(), depth, json),
        Commands::Diagram {
            target,
            r#type,
            format,
            path,
            output,
        } => commands::diagram::run(
            &target,
            &r#type,
            &format,
            path.as_deref(),
            output.as_deref(),
        ),
        Commands::TraceImport { file, path } => commands::trace_import::run(&file, path.as_deref()),
        Commands::RagImport { docs_dir, path } => {
            commands::rag_import::run(&docs_dir, path.as_deref())
        }
        Commands::TraceDoc { file, output, path } => {
            commands::trace_doc::run(&file, output.as_deref(), path.as_deref()).await
        }
        Commands::Coverage {
            target,
            path,
            json,
            trace,
        } => commands::coverage::run(target.as_deref(), path.as_deref(), json, trace),
        Commands::ValidateDocs {
            repo,
            docs_dir,
            json,
        } => commands::validate_docs::run(repo.as_deref(), docs_dir.as_deref(), json),
        Commands::Workdoc { args } => commands::workdoc::run_from_args(args).await,
    }
}

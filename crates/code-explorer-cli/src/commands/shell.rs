//! Interactive REPL shell for exploring the Code Explorer knowledge graph.

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::PathBuf;
use std::sync::Arc;

use colored::Colorize;
use rustyline::completion::{Completer, Pair};
use rustyline::error::ReadlineError;
use rustyline::highlight::Highlighter;
use rustyline::hint::Hinter;
use rustyline::validate::Validator;
use rustyline::{Config, Editor, Helper};

use code_explorer_core::graph::types::{GraphNode, NodeLabel, RelationshipType};
use code_explorer_core::graph::KnowledgeGraph;
use code_explorer_core::storage::repo_manager;
use code_explorer_output::terminal::TerminalFormatter;
use code_explorer_output::traits::OutputFormatter;

// ─── Shell Context ──────────────────────────────────────────────────────

struct ShellContext {
    graph: Arc<KnowledgeGraph>,
    repo_path: PathBuf,
    storage_path: PathBuf,
    // Pre-built indexes for fast lookups
    outgoing: HashMap<String, Vec<(String, RelationshipType, f64)>>,
    incoming: HashMap<String, Vec<(String, RelationshipType, f64)>>,
    name_index: HashMap<String, Vec<String>>, // lowercase name -> node IDs
    label_counts: HashMap<NodeLabel, usize>,
    rel_counts: HashMap<RelationshipType, usize>,
    symbol_names: Vec<String>, // for completion
}

impl ShellContext {
    fn build(graph: KnowledgeGraph, repo_path: PathBuf, storage_path: PathBuf) -> Self {
        let mut outgoing: HashMap<String, Vec<(String, RelationshipType, f64)>> = HashMap::new();
        let mut incoming: HashMap<String, Vec<(String, RelationshipType, f64)>> = HashMap::new();
        let mut name_index: HashMap<String, Vec<String>> = HashMap::new();
        let mut label_counts: HashMap<NodeLabel, usize> = HashMap::new();
        let mut rel_counts: HashMap<RelationshipType, usize> = HashMap::new();
        let mut symbol_names: Vec<String> = Vec::new();

        // Build node indexes
        for node in graph.iter_nodes() {
            *label_counts.entry(node.label).or_insert(0) += 1;
            let lower = node.properties.name.to_lowercase();
            name_index.entry(lower).or_default().push(node.id.clone());
            symbol_names.push(node.properties.name.clone());
        }
        symbol_names.sort();
        symbol_names.dedup();

        // Build relationship indexes
        for rel in graph.iter_relationships() {
            *rel_counts.entry(rel.rel_type).or_insert(0) += 1;
            outgoing.entry(rel.source_id.clone()).or_default().push((
                rel.target_id.clone(),
                rel.rel_type,
                rel.confidence,
            ));
            incoming.entry(rel.target_id.clone()).or_default().push((
                rel.source_id.clone(),
                rel.rel_type,
                rel.confidence,
            ));
        }

        Self {
            graph: Arc::new(graph),
            repo_path,
            storage_path,
            outgoing,
            incoming,
            name_index,
            label_counts,
            rel_counts,
            symbol_names,
        }
    }

    /// Find node IDs by symbol name (case-insensitive substring).
    fn find_nodes_by_name(&self, name: &str) -> Vec<String> {
        let lower = name.to_lowercase();
        let mut results = Vec::new();
        for (key, ids) in &self.name_index {
            if key.contains(&lower) {
                results.extend(ids.iter().cloned());
            }
        }
        results
    }

    /// Find node IDs by exact name (case-insensitive).
    fn find_nodes_exact(&self, name: &str) -> Vec<String> {
        let lower = name.to_lowercase();
        self.name_index.get(&lower).cloned().unwrap_or_default()
    }
}

// ─── Rustyline Helper ───────────────────────────────────────────────────

struct ShellHelper {
    commands: Vec<String>,
    symbols: Vec<String>,
}

impl ShellHelper {
    fn new(symbols: Vec<String>) -> Self {
        let commands = vec![
            "query",
            "q",
            "context",
            "ctx",
            "impact",
            "cypher",
            "stats",
            "find",
            "f",
            "community",
            "com",
            "process",
            "proc",
            "neighbors",
            "n",
            "path",
            "files",
            "export",
            "reload",
            "analyze",
            "hotspots",
            "coupling",
            "ownership",
            "help",
            "h",
            "quit",
            "exit",
        ]
        .into_iter()
        .map(String::from)
        .collect();
        Self { commands, symbols }
    }
}

impl Completer for ShellHelper {
    type Candidate = Pair;

    fn complete(
        &self,
        line: &str,
        pos: usize,
        _ctx: &rustyline::Context<'_>,
    ) -> rustyline::Result<(usize, Vec<Pair>)> {
        let line_up_to = &line[..pos];
        let parts: Vec<&str> = line_up_to.split_whitespace().collect();

        if parts.is_empty() || (parts.len() == 1 && !line_up_to.ends_with(' ')) {
            // Completing command name
            let prefix = parts.first().copied().unwrap_or("");
            let matches: Vec<Pair> = self
                .commands
                .iter()
                .filter(|c| c.starts_with(prefix))
                .map(|c| Pair {
                    display: c.clone(),
                    replacement: c.clone(),
                })
                .collect();
            let start = pos - prefix.len();
            Ok((start, matches))
        } else {
            // Completing symbol name argument.
            //
            // If the cursor sits right after a whitespace character (e.g. the
            // user typed `context ` and hit TAB to start the first argument)
            // then the *prefix being typed* is empty — not `parts.last()`,
            // which in that situation would return the previous token
            // (`context`). Using the previous token as the prefix caused the
            // completion to compute `start = pos - last_word.len()`, pointing
            // several characters INSIDE the previous token, and rustyline
            // would happily overwrite part of it with a symbol name
            // (e.g. `context ` + TAB → `cContext`, corrupting the line). Fix
            // by treating a trailing whitespace as "begin a new empty token
            // at the cursor".
            let last_word =
                if pos == 0 || line_up_to.chars().last().is_some_and(|c| c.is_whitespace()) {
                    ""
                } else {
                    parts.last().copied().unwrap_or("")
                };
            let lower = last_word.to_lowercase();
            let matches: Vec<Pair> = self
                .symbols
                .iter()
                .filter(|s| s.to_lowercase().starts_with(&lower))
                .take(30)
                .map(|s| Pair {
                    display: s.clone(),
                    replacement: s.clone(),
                })
                .collect();
            let start = pos - last_word.len();
            Ok((start, matches))
        }
    }
}

impl Hinter for ShellHelper {
    type Hint = String;
}

impl Highlighter for ShellHelper {}
impl Validator for ShellHelper {}
impl Helper for ShellHelper {}

// ─── Entry Point ────────────────────────────────────────────────────────

pub async fn run(path: Option<&str>) -> anyhow::Result<()> {
    let repo_path = match path {
        Some(p) => PathBuf::from(p)
            .canonicalize()
            .unwrap_or_else(|_| PathBuf::from(p)),
        None => std::env::current_dir()?,
    };

    let storage = repo_manager::get_storage_paths(&repo_path);
    let snap_path = code_explorer_db::snapshot::snapshot_path(&storage.storage_path);

    if !snap_path.exists() {
        eprintln!(
            "{} No graph snapshot found at {}",
            "Error:".red().bold(),
            snap_path.display()
        );
        eprintln!(
            "Run {} first to index this repository.",
            "code-explorer analyze".cyan()
        );
        return Ok(());
    }

    eprintln!(
        "{} Loading graph from {}...",
        ">>>".green(),
        snap_path.display()
    );
    let graph = code_explorer_db::snapshot::load_snapshot(&snap_path)?;
    let node_count = graph.node_count();
    let edge_count = graph.relationship_count();

    let mut ctx = ShellContext::build(graph, repo_path.clone(), storage.storage_path.clone());

    eprintln!(
        "{} Loaded {} nodes, {} edges",
        ">>>".green(),
        node_count.to_string().cyan(),
        edge_count.to_string().cyan()
    );
    eprintln!(
        "Type {} for available commands, {} to exit.\n",
        "help".yellow(),
        "quit".yellow()
    );

    // Set up rustyline
    let config = Config::builder()
        .max_history_size(1000)
        .expect("valid history size")
        .auto_add_history(true)
        .build();

    let helper = ShellHelper::new(ctx.symbol_names.clone());
    let mut rl: Editor<ShellHelper, rustyline::history::DefaultHistory> =
        Editor::with_config(config)?;
    rl.set_helper(Some(helper));

    // Load history
    let history_dir = repo_manager::get_global_dir();
    let history_path = history_dir.join("repl_history.txt");
    let _ = std::fs::create_dir_all(&history_dir);
    let _ = rl.load_history(&history_path);

    let prompt = format!("{} ", "code-explorer>".green().bold());

    loop {
        match rl.readline(&prompt) {
            Ok(line) => {
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    continue;
                }

                let (cmd, args) = match trimmed.split_once(char::is_whitespace) {
                    Some((c, a)) => (c, a.trim()),
                    None => (trimmed, ""),
                };

                if let Err(e) = dispatch(cmd, args, &mut ctx) {
                    eprintln!("{} {}", "Error:".red().bold(), e);
                }
            }
            Err(ReadlineError::Interrupted) => {
                // Ctrl+C: cancel current input, continue
                continue;
            }
            Err(ReadlineError::Eof) => {
                // Ctrl+D: quit
                eprintln!("{}", "Goodbye!".dimmed());
                break;
            }
            Err(err) => {
                eprintln!("{} {:?}", "Readline error:".red(), err);
                break;
            }
        }
    }

    let _ = rl.save_history(&history_path);
    Ok(())
}

// ─── Command Dispatch ───────────────────────────────────────────────────

fn dispatch(cmd: &str, args: &str, ctx: &mut ShellContext) -> anyhow::Result<()> {
    match cmd {
        "query" | "q" => cmd_query(args, ctx),
        "context" | "ctx" => cmd_context(args, ctx),
        "impact" => cmd_impact(args, ctx),
        "cypher" => cmd_cypher(args, ctx),
        "stats" => cmd_stats(ctx),
        "find" | "f" => cmd_find(args, ctx),
        "community" | "com" => cmd_community(args, ctx),
        "process" | "proc" => cmd_process(args, ctx),
        "neighbors" | "n" => cmd_neighbors(args, ctx),
        "path" => cmd_path(args, ctx),
        "files" => cmd_files(args, ctx),
        "export" => cmd_export(args, ctx),
        "reload" => cmd_reload(ctx),
        "analyze" => cmd_analyze(args, ctx),
        "hotspots" => cmd_hotspots(args, ctx),
        "coupling" => cmd_coupling(args, ctx),
        "ownership" => cmd_ownership(ctx),
        "help" | "h" | "?" => cmd_help(),
        "quit" | "exit" => std::process::exit(0),
        _ => {
            eprintln!(
                "Unknown command: {}. Type {} for available commands.",
                cmd.yellow(),
                "help".cyan()
            );
            Ok(())
        }
    }
}

// ─── Stats ──────────────────────────────────────────────────────────────

fn cmd_stats(ctx: &ShellContext) -> anyhow::Result<()> {
    let total_nodes: usize = ctx.label_counts.values().sum();
    let total_edges: usize = ctx.rel_counts.values().sum();

    println!();
    println!("  {}", "Graph Statistics".bold().cyan());
    println!("  {}", "\u{2500}".repeat(40).dimmed());

    // Node stats
    println!(
        "  {}  {} total",
        "Nodes:".bold(),
        format_number(total_nodes).cyan()
    );
    let mut label_vec: Vec<_> = ctx.label_counts.iter().collect();
    label_vec.sort_by(|a, b| b.1.cmp(a.1));
    let max_label_count = label_vec.first().map(|(_, c)| **c).unwrap_or(1);
    for (label, count) in &label_vec {
        let bar_len = (**count as f64 / max_label_count as f64 * 20.0) as usize;
        let bar = "\u{2588}".repeat(bar_len);
        println!(
            "    {:<14} {:>6}  {}",
            label.as_str().yellow(),
            format_number(**count),
            bar.green()
        );
    }

    println!();

    // Edge stats
    println!(
        "  {}  {} total",
        "Edges:".bold(),
        format_number(total_edges).cyan()
    );
    let mut rel_vec: Vec<_> = ctx.rel_counts.iter().collect();
    rel_vec.sort_by(|a, b| b.1.cmp(a.1));
    let max_rel_count = rel_vec.first().map(|(_, c)| **c).unwrap_or(1);
    for (rel_type, count) in &rel_vec {
        let bar_len = (**count as f64 / max_rel_count as f64 * 20.0) as usize;
        let bar = "\u{2588}".repeat(bar_len);
        println!(
            "    {:<16} {:>6}  {}",
            rel_type.as_str().yellow(),
            format_number(**count),
            bar.magenta()
        );
    }

    // Extra stats
    let file_count = ctx.label_counts.get(&NodeLabel::File).copied().unwrap_or(0);
    let community_count = ctx
        .label_counts
        .get(&NodeLabel::Community)
        .copied()
        .unwrap_or(0);
    let process_count = ctx
        .label_counts
        .get(&NodeLabel::Process)
        .copied()
        .unwrap_or(0);
    println!();
    println!("  {:<16} {}", "Files:".dimmed(), format_number(file_count));
    println!(
        "  {:<16} {}",
        "Communities:".dimmed(),
        format_number(community_count)
    );
    println!(
        "  {:<16} {}",
        "Processes:".dimmed(),
        format_number(process_count)
    );
    println!();

    Ok(())
}

// ─── Find ───────────────────────────────────────────────────────────────

fn cmd_find(pattern: &str, ctx: &ShellContext) -> anyhow::Result<()> {
    if pattern.is_empty() {
        eprintln!("Usage: find <regex_pattern>");
        return Ok(());
    }

    let re = regex::Regex::new(pattern).map_err(|e| anyhow::anyhow!("Invalid regex: {}", e))?;
    let mut matches: Vec<&GraphNode> = Vec::new();

    for node in ctx.graph.iter_nodes() {
        if re.is_match(&node.properties.name) {
            matches.push(node);
        }
    }

    matches.sort_by(|a, b| a.properties.name.cmp(&b.properties.name));

    if matches.is_empty() {
        println!("  No matches for pattern '{}'", pattern.yellow());
        return Ok(());
    }

    println!(
        "  Found {} matches for '{}':",
        matches.len().to_string().cyan(),
        pattern.yellow()
    );
    println!();

    for (i, node) in matches.iter().enumerate().take(50) {
        let loc = format_location(node);
        println!(
            "  {:>3}. {} {} {}",
            (i + 1).to_string().dimmed(),
            node.label.as_str().yellow(),
            node.properties.name.bold(),
            loc.dimmed()
        );
    }
    if matches.len() > 50 {
        println!(
            "  ... and {} more",
            (matches.len() - 50).to_string().dimmed()
        );
    }
    println!();

    Ok(())
}

// ─── Query ──────────────────────────────────────────────────────────────

fn cmd_query(text: &str, ctx: &ShellContext) -> anyhow::Result<()> {
    if text.is_empty() {
        eprintln!("Usage: query <search_text>");
        return Ok(());
    }

    let ids = ctx.find_nodes_by_name(text);
    if ids.is_empty() {
        println!("  No results for '{}'", text.yellow());
        return Ok(());
    }

    println!(
        "  {} results for '{}':",
        ids.len().to_string().cyan(),
        text.yellow()
    );
    println!();

    for (i, id) in ids.iter().enumerate().take(20) {
        if let Some(node) = ctx.graph.get_node(id) {
            let loc = format_location(node);
            println!(
                "  {:>3}. {} {} {}",
                (i + 1).to_string().dimmed(),
                node.label.as_str().yellow(),
                node.properties.name.bold(),
                loc.dimmed()
            );
        }
    }

    if ids.len() > 20 {
        println!(
            "  ... and {} more (refine your search)",
            (ids.len() - 20).to_string().dimmed()
        );
    }
    println!();

    Ok(())
}

// ─── Context ────────────────────────────────────────────────────────────

fn cmd_context(symbol: &str, ctx: &ShellContext) -> anyhow::Result<()> {
    if symbol.is_empty() {
        eprintln!("Usage: context <symbol_name>");
        return Ok(());
    }

    // Try exact match first, then substring
    let mut ids = ctx.find_nodes_exact(symbol);
    if ids.is_empty() {
        ids = ctx.find_nodes_by_name(symbol);
    }
    if ids.is_empty() {
        println!("  Symbol '{}' not found", symbol.yellow());
        return Ok(());
    }

    // Show context for the first match
    let node_id = &ids[0];
    let node = match ctx.graph.get_node(node_id) {
        Some(n) => n,
        None => {
            println!("  Node not found: {}", node_id);
            return Ok(());
        }
    };

    println!();
    println!(
        "  {} {}",
        "Symbol:".bold(),
        node.properties.name.bold().cyan()
    );
    println!("  {} {}", "Label:".bold(), node.label.as_str().yellow());
    println!(
        "  {} {}",
        "File:".bold(),
        node.properties.file_path.dimmed()
    );
    if let (Some(start), Some(end)) = (node.properties.start_line, node.properties.end_line) {
        println!(
            "  {} {}:{}",
            "Lines:".bold(),
            start.to_string().dimmed(),
            end.to_string().dimmed()
        );
    }
    if let Some(desc) = &node.properties.description {
        println!("  {} {}", "Desc:".bold(), desc);
    }

    // Callers (incoming CALLS edges)
    let callers: Vec<_> = ctx
        .incoming
        .get(node_id)
        .map(|edges| {
            edges
                .iter()
                .filter(|(_, rt, _)| *rt == RelationshipType::Calls)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    if !callers.is_empty() {
        println!();
        println!(
            "  {} ({})",
            "Callers".bold().green(),
            callers.len().to_string().dimmed()
        );
        for (src_id, _, conf) in &callers {
            if let Some(src) = ctx.graph.get_node(src_id) {
                println!(
                    "    <- {} {} [{}]",
                    src.label.as_str().yellow(),
                    src.properties.name,
                    format!("{:.0}%", conf * 100.0).dimmed()
                );
            }
        }
    }

    // Callees (outgoing CALLS edges)
    let callees: Vec<_> = ctx
        .outgoing
        .get(node_id)
        .map(|edges| {
            edges
                .iter()
                .filter(|(_, rt, _)| *rt == RelationshipType::Calls)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    if !callees.is_empty() {
        println!();
        println!(
            "  {} ({})",
            "Calls".bold().green(),
            callees.len().to_string().dimmed()
        );
        for (tgt_id, _, conf) in &callees {
            if let Some(tgt) = ctx.graph.get_node(tgt_id) {
                println!(
                    "    -> {} {} [{}]",
                    tgt.label.as_str().yellow(),
                    tgt.properties.name,
                    format!("{:.0}%", conf * 100.0).dimmed()
                );
            }
        }
    }

    // Community membership (outgoing MEMBER_OF edges)
    let communities: Vec<_> = ctx
        .outgoing
        .get(node_id)
        .map(|edges| {
            edges
                .iter()
                .filter(|(_, rt, _)| *rt == RelationshipType::MemberOf)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    if !communities.is_empty() {
        println!();
        println!("  {}", "Communities".bold().magenta());
        for (tgt_id, _, _) in &communities {
            if let Some(comm) = ctx.graph.get_node(tgt_id) {
                println!("    {} {}", "\u{2022}".dimmed(), comm.properties.name);
            }
        }
    }

    // Process steps (outgoing STEP_IN_PROCESS edges)
    let processes: Vec<_> = ctx
        .outgoing
        .get(node_id)
        .map(|edges| {
            edges
                .iter()
                .filter(|(_, rt, _)| *rt == RelationshipType::StepInProcess)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    if !processes.is_empty() {
        println!();
        println!("  {}", "Processes".bold().magenta());
        for (tgt_id, _, _) in &processes {
            if let Some(proc_node) = ctx.graph.get_node(tgt_id) {
                println!("    {} {}", "\u{2022}".dimmed(), proc_node.properties.name);
            }
        }
    }

    // Other relationships
    let other_out: Vec<_> = ctx
        .outgoing
        .get(node_id)
        .map(|edges| {
            edges
                .iter()
                .filter(|(_, rt, _)| {
                    !matches!(
                        rt,
                        RelationshipType::Calls
                            | RelationshipType::MemberOf
                            | RelationshipType::StepInProcess
                    )
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    if !other_out.is_empty() {
        println!();
        println!("  {}", "Other Relationships".bold().blue());
        for (tgt_id, rt, _) in &other_out {
            if let Some(tgt) = ctx.graph.get_node(tgt_id) {
                println!(
                    "    --[{}]--> {} {}",
                    rt.as_str().dimmed(),
                    tgt.label.as_str().yellow(),
                    tgt.properties.name
                );
            }
        }
    }

    if ids.len() > 1 {
        println!();
        println!(
            "  {} {} other symbols match this name.",
            "Note:".dimmed(),
            (ids.len() - 1).to_string().dimmed()
        );
    }

    println!();
    Ok(())
}

// ─── Impact ─────────────────────────────────────────────────────────────

fn cmd_impact(symbol: &str, ctx: &ShellContext) -> anyhow::Result<()> {
    if symbol.is_empty() {
        eprintln!("Usage: impact <symbol_name>");
        return Ok(());
    }

    let mut ids = ctx.find_nodes_exact(symbol);
    if ids.is_empty() {
        ids = ctx.find_nodes_by_name(symbol);
    }
    if ids.is_empty() {
        println!("  Symbol '{}' not found", symbol.yellow());
        return Ok(());
    }

    let start_id = &ids[0];
    let start_node = ctx.graph.get_node(start_id);
    let start_name = start_node
        .map(|n| n.properties.name.as_str())
        .unwrap_or(start_id.as_str());

    println!();
    println!(
        "  {} from {}",
        "Impact Analysis".bold().cyan(),
        start_name.bold()
    );
    println!("  {}", "\u{2500}".repeat(40).dimmed());

    // BFS through causal edges (outgoing = downstream impact). Restricting to
    // `Calls` alone misses every impact that flows through a *non-call* causal
    // relationship: changing an interface affects its implementers, changing a
    // base class affects subclasses, and changing a module affects its
    // importers. The #52 fix added Imports/Inherits/Implements/Extends but
    // stopped short — BFS still missed CallsAction (controllers), Uses
    // (symbol dependencies), Overrides (virtual method chains), RendersView,
    // HandlesRoute, Fetches, MapsToEntity. Keep this list in sync with the
    // canonical set in `desktop/commands/impact.rs::bfs_impact` — same root
    // cause as #39/#52/#55/#57 across all the impact code paths.
    let max_depth = 5;
    let mut visited: HashSet<String> = HashSet::new();
    visited.insert(start_id.clone());
    let mut queue: VecDeque<(String, usize)> = VecDeque::new();
    queue.push_back((start_id.clone(), 0));

    let mut levels: Vec<Vec<String>> = vec![Vec::new(); max_depth];
    let mut total_affected = 0;

    while let Some((node_id, depth)) = queue.pop_front() {
        if depth >= max_depth {
            continue;
        }

        if let Some(edges) = ctx.outgoing.get(&node_id) {
            for (target_id, rel_type, _) in edges {
                let is_causal = matches!(
                    rel_type,
                    RelationshipType::Calls
                        | RelationshipType::CallsAction
                        | RelationshipType::Imports
                        | RelationshipType::Inherits
                        | RelationshipType::Implements
                        | RelationshipType::Extends
                        | RelationshipType::Uses
                        | RelationshipType::Overrides
                        | RelationshipType::RendersView
                        | RelationshipType::HandlesRoute
                        | RelationshipType::Fetches
                        | RelationshipType::MapsToEntity
                );
                if is_causal && !visited.contains(target_id) {
                    visited.insert(target_id.clone());
                    levels[depth].push(target_id.clone());
                    total_affected += 1;
                    queue.push_back((target_id.clone(), depth + 1));
                }
            }
        }
    }

    for (depth, ids_at_depth) in levels.iter().enumerate() {
        if ids_at_depth.is_empty() {
            continue;
        }
        println!(
            "\n  {} ({} nodes):",
            format!("Depth {}", depth + 1).bold().yellow(),
            ids_at_depth.len().to_string().cyan()
        );
        for id in ids_at_depth.iter().take(15) {
            if let Some(node) = ctx.graph.get_node(id) {
                let loc = format_location(node);
                println!(
                    "    {} {} {}",
                    node.label.as_str().yellow(),
                    node.properties.name,
                    loc.dimmed()
                );
            }
        }
        if ids_at_depth.len() > 15 {
            println!(
                "    ... and {} more",
                (ids_at_depth.len() - 15).to_string().dimmed()
            );
        }
    }

    println!();
    println!(
        "  Total affected: {} symbols within {} hops",
        total_affected.to_string().cyan().bold(),
        max_depth.to_string().yellow()
    );
    println!();

    Ok(())
}

// ─── Neighbors ──────────────────────────────────────────────────────────

fn cmd_neighbors(args: &str, ctx: &ShellContext) -> anyhow::Result<()> {
    let parts: Vec<&str> = args.splitn(2, char::is_whitespace).collect();
    if parts.is_empty() || parts[0].is_empty() {
        eprintln!("Usage: neighbors <symbol_name> [depth=2]");
        return Ok(());
    }

    let symbol = parts[0];
    let depth: usize = parts
        .get(1)
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(2);
    let max_depth = depth.min(5);

    let mut ids = ctx.find_nodes_exact(symbol);
    if ids.is_empty() {
        ids = ctx.find_nodes_by_name(symbol);
    }
    if ids.is_empty() {
        println!("  Symbol '{}' not found", symbol.yellow());
        return Ok(());
    }

    let start_id = &ids[0];
    let start_name = ctx
        .graph
        .get_node(start_id)
        .map(|n| n.properties.name.clone())
        .unwrap_or_else(|| start_id.clone());

    println!();
    println!(
        "  {} for {} (depth {})",
        "Neighbors".bold().cyan(),
        start_name.bold(),
        max_depth.to_string().yellow()
    );
    println!("  {}", "\u{2500}".repeat(40).dimmed());

    let mut visited: HashSet<String> = HashSet::new();
    visited.insert(start_id.clone());
    let mut queue: VecDeque<(String, usize)> = VecDeque::new();
    queue.push_back((start_id.clone(), 0));
    let mut results: Vec<(String, usize, String)> = Vec::new(); // (node_id, depth, edge_type)

    while let Some((node_id, d)) = queue.pop_front() {
        if d >= max_depth {
            continue;
        }

        // Outgoing edges
        if let Some(edges) = ctx.outgoing.get(&node_id) {
            for (target_id, rel_type, _) in edges {
                if !visited.contains(target_id) {
                    visited.insert(target_id.clone());
                    results.push((
                        target_id.clone(),
                        d + 1,
                        format!("-[{}]->", rel_type.as_str()),
                    ));
                    queue.push_back((target_id.clone(), d + 1));
                }
            }
        }

        // Incoming edges
        if let Some(edges) = ctx.incoming.get(&node_id) {
            for (source_id, rel_type, _) in edges {
                if !visited.contains(source_id) {
                    visited.insert(source_id.clone());
                    results.push((
                        source_id.clone(),
                        d + 1,
                        format!("<-[{}]--", rel_type.as_str()),
                    ));
                    queue.push_back((source_id.clone(), d + 1));
                }
            }
        }
    }

    if results.is_empty() {
        println!("  No neighbors found.");
    } else {
        // Group by depth
        results.sort_by_key(|(_, d, _)| *d);
        let mut current_depth = 0;
        for (node_id, d, edge_label) in &results {
            if *d != current_depth {
                current_depth = *d;
                println!(
                    "\n  {}:",
                    format!("Depth {}", current_depth).bold().yellow()
                );
            }
            if let Some(node) = ctx.graph.get_node(node_id) {
                println!(
                    "    {} {} {} {}",
                    edge_label.dimmed(),
                    node.label.as_str().yellow(),
                    node.properties.name,
                    format_location(node).dimmed()
                );
            }
        }
        println!();
        println!(
            "  Total: {} neighbors",
            results.len().to_string().cyan().bold()
        );
    }

    println!();
    Ok(())
}

// ─── Community ──────────────────────────────────────────────────────────

fn cmd_community(name: &str, ctx: &ShellContext) -> anyhow::Result<()> {
    // Collect all community nodes
    let communities: Vec<&GraphNode> = ctx
        .graph
        .iter_nodes()
        .filter(|n| n.label == NodeLabel::Community)
        .collect();

    if name.is_empty() {
        // List all communities
        if communities.is_empty() {
            println!("  No communities found in the graph.");
            return Ok(());
        }

        println!();
        println!(
            "  {} ({}):",
            "Communities".bold().cyan(),
            communities.len().to_string().dimmed()
        );
        println!();

        for comm in &communities {
            let member_count = ctx
                .incoming
                .get(&comm.id)
                .map(|edges| {
                    edges
                        .iter()
                        .filter(|(_, rt, _)| *rt == RelationshipType::MemberOf)
                        .count()
                })
                .unwrap_or(0);
            let desc = comm
                .properties
                .description
                .as_deref()
                .unwrap_or("(no description)");
            println!(
                "    {} ({} members)",
                comm.properties.name.bold(),
                member_count.to_string().cyan()
            );
            println!("      {}", desc.dimmed());
        }
        println!();
    } else {
        // Show members of a specific community
        let lower = name.to_lowercase();
        let community = communities
            .iter()
            .find(|c| c.properties.name.to_lowercase().contains(&lower));

        match community {
            Some(comm) => {
                println!();
                println!(
                    "  {} {}",
                    "Community:".bold(),
                    comm.properties.name.bold().cyan()
                );
                if let Some(desc) = &comm.properties.description {
                    println!("  {}", desc.dimmed());
                }
                println!();

                // Find all members (incoming MEMBER_OF edges)
                let members: Vec<_> = ctx
                    .incoming
                    .get(&comm.id)
                    .map(|edges| {
                        edges
                            .iter()
                            .filter(|(_, rt, _)| *rt == RelationshipType::MemberOf)
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();

                println!(
                    "  {} ({}):",
                    "Members".bold(),
                    members.len().to_string().dimmed()
                );
                for (src_id, _, _) in &members {
                    if let Some(node) = ctx.graph.get_node(src_id) {
                        println!(
                            "    {} {} {}",
                            node.label.as_str().yellow(),
                            node.properties.name,
                            format_location(node).dimmed()
                        );
                    }
                }
                println!();
            }
            None => {
                println!("  Community '{}' not found", name.yellow());
                println!("  Use 'community' (no args) to list all communities.");
            }
        }
    }

    Ok(())
}

// ─── Process ────────────────────────────────────────────────────────────

fn cmd_process(name: &str, ctx: &ShellContext) -> anyhow::Result<()> {
    let processes: Vec<&GraphNode> = ctx
        .graph
        .iter_nodes()
        .filter(|n| n.label == NodeLabel::Process)
        .collect();

    if name.is_empty() {
        if processes.is_empty() {
            println!("  No processes found in the graph.");
            return Ok(());
        }

        println!();
        println!(
            "  {} ({}):",
            "Processes".bold().cyan(),
            processes.len().to_string().dimmed()
        );
        println!();

        for proc in &processes {
            let step_count = proc.properties.step_count.unwrap_or(0);
            let desc = proc
                .properties
                .description
                .as_deref()
                .unwrap_or("(no description)");
            println!(
                "    {} ({} steps)",
                proc.properties.name.bold(),
                step_count.to_string().cyan()
            );
            println!("      {}", desc.dimmed());
        }
        println!();
    } else {
        let lower = name.to_lowercase();
        let process = processes
            .iter()
            .find(|p| p.properties.name.to_lowercase().contains(&lower));

        match process {
            Some(proc) => {
                println!();
                println!(
                    "  {} {}",
                    "Process:".bold(),
                    proc.properties.name.bold().cyan()
                );
                if let Some(desc) = &proc.properties.description {
                    println!("  {}", desc.dimmed());
                }
                println!();

                // Find all steps (incoming STEP_IN_PROCESS edges to this process)
                let steps: Vec<_> = ctx
                    .incoming
                    .get(&proc.id)
                    .map(|edges| {
                        edges
                            .iter()
                            .filter(|(_, rt, _)| *rt == RelationshipType::StepInProcess)
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();

                // Try to sort steps by step number from the relationship
                // The step number is stored on the relationship, so we need to find it
                let mut step_items: Vec<(u32, &str, &GraphNode)> = Vec::new();
                for (src_id, _, _) in &steps {
                    if let Some(node) = ctx.graph.get_node(src_id) {
                        // Find the relationship with step info
                        let step_num = ctx
                            .graph
                            .iter_relationships()
                            .find(|r| {
                                r.source_id == *src_id
                                    && r.target_id == proc.id
                                    && r.rel_type == RelationshipType::StepInProcess
                            })
                            .and_then(|r| r.step)
                            .unwrap_or(0);
                        step_items.push((step_num, src_id, node));
                    }
                }
                step_items.sort_by_key(|(step, _, _)| *step);

                println!(
                    "  {} ({}):",
                    "Steps".bold(),
                    step_items.len().to_string().dimmed()
                );
                for (step_num, _, node) in &step_items {
                    println!(
                        "    {}. {} {} {}",
                        step_num.to_string().cyan(),
                        node.label.as_str().yellow(),
                        node.properties.name,
                        format_location(node).dimmed()
                    );
                }
                println!();
            }
            None => {
                println!("  Process '{}' not found", name.yellow());
                println!("  Use 'process' (no args) to list all processes.");
            }
        }
    }

    Ok(())
}

// ─── Path ───────────────────────────────────────────────────────────────

fn cmd_path(args: &str, ctx: &ShellContext) -> anyhow::Result<()> {
    let parts: Vec<&str> = args.splitn(2, char::is_whitespace).collect();
    if parts.len() < 2 {
        eprintln!("Usage: path <from_symbol> <to_symbol>");
        return Ok(());
    }

    let from_name = parts[0].trim();
    let to_name = parts[1].trim();

    let from_ids = ctx.find_nodes_exact(from_name);
    let to_ids = ctx.find_nodes_exact(to_name);

    // Fall back to substring match
    let from_ids = if from_ids.is_empty() {
        ctx.find_nodes_by_name(from_name)
    } else {
        from_ids
    };
    let to_ids = if to_ids.is_empty() {
        ctx.find_nodes_by_name(to_name)
    } else {
        to_ids
    };

    if from_ids.is_empty() {
        println!("  Source symbol '{}' not found", from_name.yellow());
        return Ok(());
    }
    if to_ids.is_empty() {
        println!("  Target symbol '{}' not found", to_name.yellow());
        return Ok(());
    }

    let start = &from_ids[0];
    let goal = &to_ids[0];

    // BFS for shortest path
    let mut visited: HashMap<String, String> = HashMap::new(); // node -> parent
    visited.insert(start.clone(), String::new());
    let mut queue: VecDeque<String> = VecDeque::new();
    queue.push_back(start.clone());
    let mut found = false;

    while let Some(current) = queue.pop_front() {
        if current == *goal {
            found = true;
            break;
        }

        // Explore outgoing edges
        if let Some(edges) = ctx.outgoing.get(&current) {
            for (target_id, _, _) in edges {
                if !visited.contains_key(target_id) {
                    visited.insert(target_id.clone(), current.clone());
                    queue.push_back(target_id.clone());
                }
            }
        }

        // Explore incoming edges
        if let Some(edges) = ctx.incoming.get(&current) {
            for (source_id, _, _) in edges {
                if !visited.contains_key(source_id) {
                    visited.insert(source_id.clone(), current.clone());
                    queue.push_back(source_id.clone());
                }
            }
        }
    }

    if !found {
        let from_display = ctx
            .graph
            .get_node(start)
            .map(|n| n.properties.name.as_str())
            .unwrap_or(from_name);
        let to_display = ctx
            .graph
            .get_node(goal)
            .map(|n| n.properties.name.as_str())
            .unwrap_or(to_name);
        println!(
            "  No path found from '{}' to '{}'",
            from_display.yellow(),
            to_display.yellow()
        );
        return Ok(());
    }

    // Reconstruct path
    let mut path_nodes: Vec<String> = Vec::new();
    let mut current = goal.clone();
    while !current.is_empty() {
        path_nodes.push(current.clone());
        current = visited.get(&current).cloned().unwrap_or_default();
    }
    path_nodes.reverse();

    println!();
    println!(
        "  {} ({} hops):",
        "Shortest Path".bold().cyan(),
        (path_nodes.len() - 1).to_string().yellow()
    );
    println!();

    for (i, node_id) in path_nodes.iter().enumerate() {
        if let Some(node) = ctx.graph.get_node(node_id) {
            let prefix = if i == 0 {
                "\u{25CF}".green().to_string() // start
            } else if i == path_nodes.len() - 1 {
                "\u{25CF}".red().to_string() // end
            } else {
                "\u{25CB}".to_string() // intermediate
            };
            println!(
                "  {} {} {} {}",
                prefix,
                node.label.as_str().yellow(),
                node.properties.name.bold(),
                format_location(node).dimmed()
            );
            if i < path_nodes.len() - 1 {
                // Find the edge between this and next
                let next_id = &path_nodes[i + 1];
                let edge_label = find_edge_label(node_id, next_id, ctx);
                println!("  {} {}", "\u{2502}".dimmed(), edge_label.dimmed());
            }
        }
    }
    println!();

    Ok(())
}

// ─── Files ──────────────────────────────────────────────────────────────

fn cmd_files(pattern: &str, ctx: &ShellContext) -> anyhow::Result<()> {
    let file_nodes: Vec<&GraphNode> = ctx
        .graph
        .iter_nodes()
        .filter(|n| n.label == NodeLabel::File)
        .collect();

    if pattern.is_empty() {
        println!(
            "  {} File nodes in graph:",
            file_nodes.len().to_string().cyan()
        );
        println!();
        for (i, node) in file_nodes.iter().enumerate().take(30) {
            println!(
                "  {:>3}. {}",
                (i + 1).to_string().dimmed(),
                node.properties.file_path
            );
        }
        if file_nodes.len() > 30 {
            println!(
                "  ... and {} more (use 'files <glob>' to filter)",
                (file_nodes.len() - 30).to_string().dimmed()
            );
        }
        println!();
        return Ok(());
    }

    let glob_pattern =
        glob::Pattern::new(pattern).map_err(|e| anyhow::anyhow!("Invalid glob: {}", e))?;

    let matching: Vec<_> = file_nodes
        .iter()
        .filter(|n| glob_pattern.matches(&n.properties.file_path))
        .collect();

    if matching.is_empty() {
        println!("  No files matching '{}'", pattern.yellow());
    } else {
        println!(
            "  {} files matching '{}':",
            matching.len().to_string().cyan(),
            pattern.yellow()
        );
        println!();
        for (i, node) in matching.iter().enumerate().take(50) {
            println!(
                "  {:>3}. {}",
                (i + 1).to_string().dimmed(),
                node.properties.file_path
            );
        }
        if matching.len() > 50 {
            println!(
                "  ... and {} more",
                (matching.len() - 50).to_string().dimmed()
            );
        }
    }
    println!();

    Ok(())
}

// ─── Cypher ─────────────────────────────────────────────────────────────

fn cmd_cypher(query: &str, ctx: &ShellContext) -> anyhow::Result<()> {
    if query.is_empty() {
        eprintln!("  {} Usage: cypher <query>", "Hint:".yellow().bold());
        eprintln!("  Example: cypher MATCH (n:Function) RETURN n LIMIT 10");
        return Ok(());
    }

    // Block write operations. Delegate to the canonical `is_write_query`
    // so the shell matches the same safety contract as the `cypher` CLI
    // subcommand. The previous inline tokenizer split on every non-word
    // character, which treated `'` as a separator and turned string
    // literals like `'Create'` into standalone `CREATE` tokens — causing
    // false-blocks on legitimate read queries such as:
    //   MATCH (n) WHERE n.name CONTAINS 'Create' RETURN n
    if code_explorer_db::query::is_write_query(query) {
        eprintln!(
            "  {} Only read-only queries are allowed.",
            "Error:".red().bold()
        );
        return Ok(());
    }

    // Build indexes and FTS from the in-memory graph
    let indexes = code_explorer_db::inmemory::cypher::GraphIndexes::build(&ctx.graph);
    let fts_index = code_explorer_db::inmemory::fts::FtsIndex::build(&ctx.graph);

    // Parse and execute
    let stmt = match code_explorer_db::inmemory::cypher::parse(query) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("  {} {}", "Parse error:".red().bold(), e);
            return Ok(());
        }
    };

    let results =
        match code_explorer_db::inmemory::cypher::execute(&stmt, &ctx.graph, &indexes, &fts_index) {
            Ok(r) => r,
            Err(e) => {
                eprintln!("  {} {}", "Query error:".red().bold(), e);
                return Ok(());
            }
        };

    if results.is_empty() {
        println!("  {} No results.", "Info:".yellow().bold());
        return Ok(());
    }

    println!(
        "  {} {} result{}",
        "OK".green().bold(),
        results.len(),
        if results.len() == 1 { "" } else { "s" }
    );
    println!();
    println!("{}", serde_json::to_string_pretty(&results)?);

    Ok(())
}

// ─── Export ─────────────────────────────────────────────────────────────

fn cmd_export(format: &str, ctx: &ShellContext) -> anyhow::Result<()> {
    let fmt = if format.is_empty() { "json" } else { format };

    match fmt {
        "json" => {
            let out_path = ctx.storage_path.join("export.json");
            let file = std::fs::File::create(&out_path)?;
            let writer = std::io::BufWriter::new(file);
            serde_json::to_writer_pretty(writer, ctx.graph.as_ref())?;
            println!(
                "  Exported graph to {}",
                out_path.display().to_string().cyan()
            );
        }
        "dot" => {
            let out_path = ctx.storage_path.join("export.dot");
            let mut out = String::new();
            out.push_str("digraph code-explorer {\n");
            out.push_str("  rankdir=LR;\n");
            out.push_str("  node [shape=box, fontname=\"monospace\"];\n\n");

            for node in ctx.graph.iter_nodes() {
                let label = format!(
                    "{}\\n{}",
                    node.label.as_str(),
                    node.properties.name.replace('"', "\\\"")
                );
                let escaped_id = node.id.replace('"', "\\\"");
                out.push_str(&format!("  \"{}\" [label=\"{}\"];\n", escaped_id, label));
            }
            out.push('\n');

            for rel in ctx.graph.iter_relationships() {
                let src = rel.source_id.replace('"', "\\\"");
                let tgt = rel.target_id.replace('"', "\\\"");
                out.push_str(&format!(
                    "  \"{}\" -> \"{}\" [label=\"{}\"];\n",
                    src,
                    tgt,
                    rel.rel_type.as_str()
                ));
            }
            out.push_str("}\n");

            std::fs::write(&out_path, &out)?;
            println!(
                "  Exported graph to {}",
                out_path.display().to_string().cyan()
            );
        }
        "csv" => {
            let csv_dir = ctx.storage_path.join("csv");
            if csv_dir.exists() {
                println!(
                    "  CSVs already exist at {}",
                    csv_dir.display().to_string().cyan()
                );
            } else {
                println!("  Run 'code-explorer analyze' to generate CSVs.");
            }
        }
        _ => {
            eprintln!("  Supported formats: json, dot, csv");
        }
    }

    Ok(())
}

// ─── Reload ─────────────────────────────────────────────────────────────

fn cmd_reload(ctx: &mut ShellContext) -> anyhow::Result<()> {
    let snap_path = code_explorer_db::snapshot::snapshot_path(&ctx.storage_path);
    if !snap_path.exists() {
        eprintln!(
            "  {} Snapshot not found at {}",
            "Error:".red().bold(),
            snap_path.display()
        );
        return Ok(());
    }

    eprintln!("  Reloading graph from snapshot...");
    let graph = code_explorer_db::snapshot::load_snapshot(&snap_path)?;
    let node_count = graph.node_count();
    let edge_count = graph.relationship_count();

    let new_ctx = ShellContext::build(graph, ctx.repo_path.clone(), ctx.storage_path.clone());
    *ctx = new_ctx;

    println!(
        "  {} Loaded {} nodes, {} edges",
        "Reloaded!".green().bold(),
        node_count.to_string().cyan(),
        edge_count.to_string().cyan()
    );

    Ok(())
}

// ─── Analyze ────────────────────────────────────────────────────────────

fn cmd_analyze(path: &str, ctx: &mut ShellContext) -> anyhow::Result<()> {
    let target = if path.is_empty() {
        ctx.repo_path.display().to_string()
    } else {
        path.to_string()
    };

    println!("  Running analysis on {} ...", target.cyan());
    println!(
        "  Please use 'code-explorer analyze {}' in another terminal, then 'reload' here.",
        target
    );

    Ok(())
}

// ─── Help ───────────────────────────────────────────────────────────────

fn cmd_help() -> anyhow::Result<()> {
    println!();
    println!("  {}", "Code Explorer Interactive Shell".bold().cyan());
    println!("  {}", "\u{2500}".repeat(50).dimmed());
    println!();
    println!("  {}", "Search & Explore".bold());
    println!(
        "    {}  Search symbols by name (substring match)",
        "query, q <text>".yellow()
    );
    println!(
        "    {}  Find symbols matching a regex pattern",
        "find, f <regex>".yellow()
    );
    println!(
        "    {}  List File nodes (optionally filter by glob)",
        "files [glob]".yellow()
    );
    println!();
    println!("  {}", "Analysis".bold());
    println!(
        "    {}  360-degree view of a symbol (callers, callees, etc.)",
        "context, ctx <sym>".yellow()
    );
    println!(
        "    {}  Blast radius: BFS through CALLS edges (5 levels)",
        "impact <symbol>".yellow()
    );
    println!(
        "    {}  All connected nodes within N hops (default 2)",
        "neighbors, n <sym> [d]".yellow()
    );
    println!(
        "    {}  Shortest path between two symbols",
        "path <from> <to>".yellow()
    );
    println!();
    println!("  {}", "Structure".bold());
    println!(
        "    {}  List communities or show community members",
        "community, com [name]".yellow()
    );
    println!(
        "    {}  List processes or show process steps",
        "process, proc [name]".yellow()
    );
    println!(
        "    {}  Graph statistics (node/edge counts by type)",
        "stats".yellow()
    );
    println!();
    println!("  {}", "Git Analysis".bold());
    println!(
        "    {}  Show file-level hotspots (default: last 90 days)",
        "hotspots [days]".yellow()
    );
    println!(
        "    {}  Show temporally coupled file pairs (default: min 3)",
        "coupling [min]".yellow()
    );
    println!(
        "    {}  Show file ownership by author",
        "ownership".yellow()
    );
    println!();
    println!("  {}", "Data".bold());
    println!(
        "    {}  Export graph to file (default: json)",
        "export [json|dot|csv]".yellow()
    );
    println!(
        "    {}  Execute a Cypher query against the graph",
        "cypher <query>".yellow()
    );
    println!(
        "    {}",
        "         Supports: =, <>, !=, CONTAINS, STARTS WITH, ENDS WITH".dimmed()
    );
    println!(
        "    {}",
        "         WHERE: AND, OR, NOT  |  RETURN DISTINCT  |  ORDER BY, LIMIT".dimmed()
    );
    println!("    {}  Reload graph from snapshot", "reload".yellow());
    println!("    {}  Hint to re-run pipeline", "analyze [path]".yellow());
    println!();
    println!("  {}", "General".bold());
    println!("    {}  Show this help", "help, h, ?".yellow());
    println!(
        "    {}  Exit the shell (or press Ctrl+D)",
        "quit, exit".yellow()
    );
    println!();

    Ok(())
}

// ─── Helpers ────────────────────────────────────────────────────────────

fn format_location(node: &GraphNode) -> String {
    match (node.properties.start_line, node.properties.end_line) {
        (Some(start), Some(end)) => format!("{}:{}-{}", node.properties.file_path, start, end),
        (Some(start), None) => format!("{}:{}", node.properties.file_path, start),
        _ => node.properties.file_path.clone(),
    }
}

fn format_number(n: usize) -> String {
    if n >= 1_000_000 {
        format!("{:.1}M", n as f64 / 1_000_000.0)
    } else if n >= 1_000 {
        format!("{},{:03}", n / 1000, n % 1000)
    } else {
        n.to_string()
    }
}

// ─── Hotspots ────────────────────────────────────────────────────────

fn cmd_hotspots(args: &str, ctx: &ShellContext) -> anyhow::Result<()> {
    let since_days: u32 = args.trim().parse().unwrap_or(90);

    let hotspots = code_explorer_git::hotspots::analyze_hotspots(&ctx.repo_path, since_days)
        .map_err(|e| anyhow::anyhow!("{}", e))?;

    if hotspots.is_empty() {
        println!(
            "  No file changes found in the last {} days.",
            since_days.to_string().yellow()
        );
        return Ok(());
    }

    let fmt = TerminalFormatter::new();
    let headers = &["File", "Commits", "Churn", "Authors", "Score"];
    let rows: Vec<Vec<String>> = hotspots
        .iter()
        .take(20)
        .map(|h| {
            vec![
                h.path.clone(),
                h.commit_count.to_string(),
                h.churn.to_string(),
                h.author_count.to_string(),
                format!("{:.2}", h.score),
            ]
        })
        .collect();

    let title = format!("Hotspots (last {} days)", since_days);
    print!("{}", fmt.format_table(&title, headers, &rows));

    if hotspots.len() > 20 {
        println!(
            "  ... and {} more files",
            (hotspots.len() - 20).to_string().dimmed()
        );
    }

    Ok(())
}

// ─── Coupling ────────────────────────────────────────────────────────

fn cmd_coupling(args: &str, ctx: &ShellContext) -> anyhow::Result<()> {
    let min_shared: u32 = args.trim().parse().unwrap_or(3);

    let couplings = code_explorer_git::coupling::analyze_coupling(&ctx.repo_path, min_shared, Some(180))
        .map_err(|e| anyhow::anyhow!("{}", e))?;

    if couplings.is_empty() {
        println!(
            "  No file pairs with at least {} shared commits.",
            min_shared.to_string().yellow()
        );
        return Ok(());
    }

    let fmt = TerminalFormatter::new();
    let headers = &["File A", "File B", "Shared", "Strength"];
    let rows: Vec<Vec<String>> = couplings
        .iter()
        .take(20)
        .map(|c| {
            vec![
                c.file_a.clone(),
                c.file_b.clone(),
                c.shared_commits.to_string(),
                format!("{:.2}", c.coupling_strength),
            ]
        })
        .collect();

    let title = format!("Temporal Coupling (min {} shared commits)", min_shared);
    print!("{}", fmt.format_table(&title, headers, &rows));

    if couplings.len() > 20 {
        println!(
            "  ... and {} more pairs",
            (couplings.len() - 20).to_string().dimmed()
        );
    }

    Ok(())
}

// ─── Ownership ───────────────────────────────────────────────────────

fn cmd_ownership(ctx: &ShellContext) -> anyhow::Result<()> {
    let ownerships = code_explorer_git::ownership::analyze_ownership(&ctx.repo_path)
        .map_err(|e| anyhow::anyhow!("{}", e))?;

    if ownerships.is_empty() {
        println!("  No ownership data found.");
        return Ok(());
    }

    let fmt = TerminalFormatter::new();
    let headers = &["File", "Primary Author", "Ownership %", "Authors"];
    let rows: Vec<Vec<String>> = ownerships
        .iter()
        .take(20)
        .map(|o| {
            vec![
                o.path.clone(),
                o.primary_author.clone(),
                format!("{:.0}%", o.ownership_pct),
                o.author_count.to_string(),
            ]
        })
        .collect();

    print!("{}", fmt.format_table("File Ownership", headers, &rows));

    if ownerships.len() > 20 {
        println!(
            "  ... and {} more files",
            (ownerships.len() - 20).to_string().dimmed()
        );
    }

    Ok(())
}

// ─── Edge Label ─────────────────────────────────────────────────────

fn find_edge_label(from: &str, to: &str, ctx: &ShellContext) -> String {
    // Check outgoing from -> to
    if let Some(edges) = ctx.outgoing.get(from) {
        for (target, rel_type, _) in edges {
            if target == to {
                return format!("--[{}]-->", rel_type.as_str());
            }
        }
    }
    // Check incoming to -> from (which means from <- to)
    if let Some(edges) = ctx.incoming.get(from) {
        for (source, rel_type, _) in edges {
            if source == to {
                return format!("<--[{}]--", rel_type.as_str());
            }
        }
    }
    "---".to_string()
}

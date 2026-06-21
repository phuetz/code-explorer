//! The `demo` command: measure (and show) the context an LLM agent saves by
//! using Code Explorer instead of reading source files.
//!
//! It indexes the target repo (if needed), runs a real "blast radius" query on a
//! hub symbol, and compares the tokens that answer costs against the tokens an
//! agent would burn reading the files to reconstruct the same answer.

use std::path::Path;
use std::time::Instant;

use colored::Colorize;

use code_explorer_core::graph::types::{NodeLabel, RelationshipType};
use code_explorer_core::graph::KnowledgeGraph;
use code_explorer_core::impact::{analyze_impact, ImpactAnalysis};
use code_explorer_core::storage::repo_manager;

/// Rough token estimate: ~4 characters per token (the common GPT-style heuristic).
fn tokens(bytes: usize) -> usize {
    bytes / 4
}

pub async fn run(path: Option<&str>, symbol: Option<&str>) -> anyhow::Result<()> {
    let repo_path = match path {
        Some(p) => Path::new(p)
            .canonicalize()
            .unwrap_or_else(|_| Path::new(p).to_path_buf()),
        None => std::env::current_dir()?,
    };

    // 1. Make sure the repo is indexed (build a graph if there isn't one yet).
    let storage = repo_manager::get_storage_paths(&repo_path);
    let snap = code_explorer_db::snapshot::snapshot_path(&storage.storage_path);
    if !snap.exists() {
        println!(
            "{}",
            "No index found for this repo — building one first…".dimmed()
        );
        let path_str = repo_path
            .to_str()
            .ok_or_else(|| anyhow::anyhow!("Repo path is not valid UTF-8"))?;
        crate::commands::analyze::run(
            path_str, true, false, false, false, false, false, None, None,
        )
        .await?;
        println!();
    }
    let graph = code_explorer_db::snapshot::load_snapshot(&snap)?;
    let metrics = repo_manager::load_metrics(&storage.storage_path)
        .ok()
        .flatten();

    // 2. Choose the symbol to demonstrate on: the most-called function/method,
    //    unless the user named one explicitly.
    let target = match symbol {
        Some(s) => s.to_string(),
        None => pick_hub_symbol(&graph).ok_or_else(|| {
            anyhow::anyhow!("No callable symbols found to demo. Try `--symbol <name>`.")
        })?,
    };

    // 3. WITH Code Explorer: one query returns the full blast radius.
    let t0 = Instant::now();
    let impact = analyze_impact(&graph, &target, 5);
    let with_latency = t0.elapsed();
    let Some(impact) = impact else {
        println!("Symbol '{}' not found in the graph.", target);
        return Ok(());
    };
    let answer = format_impact_answer(&graph, &impact);
    let with_tokens = tokens(answer.len());
    let affected = impact.downstream.total() + impact.upstream.total();

    // 4. WITHOUT Code Explorer: to learn the same blast radius an agent must open
    //    every file the affected symbols live in. Floor = bytes of those distinct
    //    files (graph-derived, so it's exactly the code behind the answer — not a
    //    noisy substring match). Also compute the whole-repo corpus as the upper bound.
    use std::collections::HashSet;
    let files = code_explorer_ingest::phases::structure::walk_repository(&repo_path)?;
    let mut size_by_path: std::collections::HashMap<&str, usize> =
        std::collections::HashMap::new();
    let mut corpus_bytes = 0usize;
    for f in &files {
        size_by_path.insert(f.path.as_str(), f.size);
        corpus_bytes += f.size;
    }
    let mut spanned: HashSet<&str> = HashSet::new();
    for dir in [&impact.downstream, &impact.upstream] {
        for depth in 1..=5 {
            for id in dir.node_ids_at_depth(depth) {
                if let Some(node) = graph.get_node(id) {
                    spanned.insert(node.properties.file_path.as_str());
                }
            }
        }
    }
    let ref_files = spanned.len();
    let ref_bytes: usize = spanned
        .iter()
        .filter_map(|p| size_by_path.get(p))
        .sum();
    let without_tokens = tokens(ref_bytes);
    let corpus_tokens = tokens(corpus_bytes);
    let ratio = if with_tokens > 0 {
        without_tokens as f64 / with_tokens as f64
    } else {
        0.0
    };

    // 5. Report.
    let total_nodes = graph.node_count();
    println!();
    println!("{}", "══ Code Explorer — LLM context demo ══".bold());
    let index_note = match &metrics {
        Some(m) => format!(
            "{} files · {} symbols · indexed in {:.1}s",
            files.len(),
            total_nodes,
            m.total_duration_ms as f64 / 1000.0
        ),
        None => format!("{} files · {} symbols", files.len(), total_nodes),
    };
    println!("  {}", repo_path.display().to_string().dimmed());
    println!("  {}", index_note.dimmed());
    println!();
    println!(
        "  Question: {}",
        format!("\"What's affected if I change `{target}`?\"").italic()
    );
    println!();
    println!(
        "  {}  {:>9} tokens · {:.0}ms · {} symbols, full chain",
        "WITH Code Explorer:       ".green(),
        fmt(with_tokens),
        with_latency.as_secs_f64() * 1000.0,
        affected
    );
    println!(
        "  {}  {:>9} tokens · reads {} file(s) · partial",
        "WITHOUT (agent reads code):".yellow(),
        fmt(without_tokens),
        ref_files
    );
    println!();
    if ratio >= 1.5 {
        println!(
            "  → {}",
            format!("{ratio:.0}× less context")
                .bold()
                .green()
        );
        println!("    for a complete, instant, reusable answer.");
    }
    println!();
    // A generous modern context window for an honest comparison (Claude ~200K).
    const WINDOW: usize = 200_000;
    if corpus_tokens >= WINDOW {
        let times = corpus_tokens as f64 / WINDOW as f64;
        println!(
            "  Whole repo ≈ {} tokens — about {} a 200K-token context window.",
            fmt(corpus_tokens).bold(),
            format!("{times:.0}×").bold()
        );
    } else {
        println!(
            "  Whole repo ≈ {} tokens — Code Explorer still answers from the relevant slice, not the whole file set.",
            fmt(corpus_tokens).bold()
        );
    }
    println!(
        "  {}",
        format!("Code Explorer distills it into a {total_nodes}-node graph you query in one command.")
            .dimmed()
    );
    println!();
    println!(
        "  {}",
        "tokens ≈ chars/4; WITHOUT sums the files the affected symbols live in — what an agent must read to trace the same impact."
            .dimmed()
    );
    Ok(())
}

/// The text an agent would actually receive from a blast-radius query — used to
/// size the "WITH" answer in tokens.
fn format_impact_answer(graph: &KnowledgeGraph, impact: &ImpactAnalysis) -> String {
    let mut s = String::new();
    s.push_str(&format!("Impact analysis for '{}'\n", impact.target.name));
    for (label, dir) in [
        ("Downstream (affected by changes)", &impact.downstream),
        ("Upstream (affects this)", &impact.upstream),
    ] {
        s.push_str(&format!("\n{label}: {} symbols\n", dir.total()));
        for depth in 1..=5 {
            for id in dir.node_ids_at_depth(depth) {
                if let Some(node) = graph.get_node(id) {
                    let loc = match node.properties.start_line {
                        Some(l) => format!("{}:{}", node.properties.file_path, l),
                        None => node.properties.file_path.clone(),
                    };
                    s.push_str(&format!(
                        "  {} {} ({})\n",
                        node.label.as_str(),
                        node.properties.name,
                        loc
                    ));
                }
            }
        }
    }
    s
}

/// Pick a representative hub symbol: the most-called function/method whose name
/// is UNIQUE in the graph (so `analyze_impact` resolves to exactly this node and
/// the demo numbers are unambiguous — avoids common names like `Error`/`len`).
fn pick_hub_symbol(graph: &KnowledgeGraph) -> Option<String> {
    use std::collections::HashMap;
    let mut name_freq: HashMap<&str, usize> = HashMap::new();
    for n in graph.iter_nodes() {
        *name_freq.entry(n.properties.name.as_str()).or_insert(0) += 1;
    }
    let mut incoming: HashMap<&str, usize> = HashMap::new();
    for r in graph.iter_relationships() {
        if matches!(r.rel_type, RelationshipType::Calls) {
            *incoming.entry(r.target_id.as_str()).or_insert(0) += 1;
        }
    }
    incoming
        .into_iter()
        .filter_map(|(id, count)| graph.get_node(id).map(|n| (n, count)))
        .filter(|(n, _)| {
            matches!(
                n.label,
                NodeLabel::Function | NodeLabel::Method | NodeLabel::Constructor
            ) && n.properties.name.len() > 4
                && name_freq.get(n.properties.name.as_str()) == Some(&1)
        })
        .max_by_key(|(_, count)| *count)
        .map(|(n, _)| n.properties.name.clone())
}

/// Thousands-separated number formatting (e.g. 62900 -> "62,900").
fn fmt(n: usize) -> String {
    let s = n.to_string();
    let bytes = s.as_bytes();
    let mut out = String::new();
    for (i, b) in bytes.iter().enumerate() {
        if i > 0 && (bytes.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(*b as char);
    }
    out
}

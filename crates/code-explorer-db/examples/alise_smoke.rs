//! Smoke test: exercise Theme A/C analytics on a real snapshot.
//!
//! Usage: cargo run --release -p code-explorer-db --example alise_smoke -- <snap_path> [repo_root]
use std::env;
use std::path::PathBuf;

use code_explorer_db::analytics::{
    clones::{find_clones, CloneOptions},
    complexity::{get_complexity, ComplexityOptions},
    cycles::{find_cycles, CycleScope},
    diff_graphs,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: alise_smoke <snap_path> [repo_root]");
        std::process::exit(2);
    }
    let snap_path = PathBuf::from(&args[1]);
    let repo_root = args.get(2).map(PathBuf::from).unwrap_or_else(|| {
        snap_path
            .parent()
            .and_then(|p| p.parent())
            .unwrap_or(std::path::Path::new("."))
            .to_path_buf()
    });

    println!("# Loading snapshot: {}", snap_path.display());
    let graph = code_explorer_db::snapshot::load_snapshot(&snap_path)?;
    println!(
        "  graph: {} nodes, {} edges",
        graph.iter_nodes().count(),
        graph.iter_relationships().count()
    );

    // ── Cycles ─────────────────────────────────────────────────────
    println!("\n## Cycles (imports)");
    let imp = find_cycles(&graph, CycleScope::Imports);
    println!("  total: {}", imp.len());
    for (i, c) in imp.iter().take(5).enumerate() {
        println!(
            "  #{}: len={} severity={}  {:?}",
            i + 1,
            c.length,
            c.severity,
            c.names
        );
    }

    println!("\n## Cycles (calls)");
    let calls = find_cycles(&graph, CycleScope::Calls);
    println!("  total: {}", calls.len());
    for (i, c) in calls.iter().take(5).enumerate() {
        println!(
            "  #{}: len={} severity={}  {:?}",
            i + 1,
            c.length,
            c.severity,
            c.names.iter().take(5).collect::<Vec<_>>()
        );
    }

    // ── Clones ─────────────────────────────────────────────────────
    println!("\n## Clones (min_tokens=30, threshold=0.9)");
    let started = std::time::Instant::now();
    let clones = find_clones(&graph, &repo_root, CloneOptions::default());
    println!(
        "  clusters: {}  (computed in {} ms)",
        clones.len(),
        started.elapsed().as_millis()
    );
    for (i, c) in clones.iter().take(3).enumerate() {
        println!(
            "  cluster {} (sim={:.2}, members={}):",
            i + 1,
            c.similarity,
            c.members.len()
        );
        for m in c.members.iter().take(3) {
            println!(
                "    - {} @ {}  ({} tokens)",
                m.name, m.file_path, m.token_count
            );
        }
    }

    // ── Complexity ─────────────────────────────────────────────────
    println!("\n## Complexity");
    let r = get_complexity(&graph, ComplexityOptions::default());
    println!(
        "  measured: {}/{}  avg={:.1}  max={}  p50={} p90={} p99={}",
        r.measured_symbols,
        r.total_symbols,
        r.avg_complexity,
        r.max_complexity,
        r.p50,
        r.p90,
        r.p99,
    );
    println!(
        "  severity: low={} medium={} high={} critical={}",
        r.severity_counts.low,
        r.severity_counts.medium,
        r.severity_counts.high,
        r.severity_counts.critical
    );
    println!("  top 5:");
    for t in r.top_symbols.iter().take(5) {
        println!("    {:>3}  {:<40}  {}", t.complexity, t.name, t.file_path);
    }
    println!("  top modules by avg complexity:");
    for m in r.by_module.iter().take(5) {
        println!(
            "    avg={:.1}  max={:>3}  n={}  {}",
            m.avg_complexity, m.max_complexity, m.symbol_count, m.module
        );
    }

    // ── Graph diff (self-diff must be empty) ────────────────────────
    println!("\n## Graph diff (self vs self)");
    let d = diff_graphs(&graph, &graph);
    println!(
        "  added_nodes={} removed_nodes={} added_edges={} removed_edges={} modified={}",
        d.added_nodes.len(),
        d.removed_nodes.len(),
        d.added_edges.len(),
        d.removed_edges.len(),
        d.modified.len()
    );
    assert!(
        d.added_nodes.is_empty(),
        "self-diff should have no added nodes"
    );
    assert!(
        d.removed_nodes.is_empty(),
        "self-diff should have no removed nodes"
    );
    assert!(
        d.modified.is_empty(),
        "self-diff should have no modified nodes"
    );
    println!("  ✓ self-diff is empty (as expected)");

    Ok(())
}

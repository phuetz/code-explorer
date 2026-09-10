//! The `report` command: combined code health report (hotspots + coupling + ownership + graph stats).

use anyhow::Result;
use colored::Colorize;

use code_explorer_db::snapshot;

pub fn run(path: Option<&str>, json: bool) -> Result<()> {
    let repo_path = if let Some(p) = path {
        std::path::PathBuf::from(p)
    } else {
        std::env::current_dir()?
    };

    // Load graph
    let storage_path = repo_path.join(".codeexplorer");
    let snap_path = storage_path.join("graph.bin");
    if !snap_path.exists() {
        println!(
            "{} No index found. Run 'code-explorer analyze' first.",
            "ERROR".red()
        );
        return Ok(());
    }

    let graph = snapshot::load_snapshot(&snap_path)
        .map_err(|e| anyhow::anyhow!("Failed to load graph: {}", e))?;

    let node_count = graph.iter_nodes().count();
    let edge_count = graph.iter_relationships().count();
    let file_count = graph
        .iter_nodes()
        .filter(|n| n.label == code_explorer_core::graph::types::NodeLabel::File)
        .count();
    let density = if node_count > 0 {
        edge_count as f64 / node_count as f64
    } else {
        0.0
    };

    // Git analytics
    let hotspots = code_explorer_git::hotspots::analyze_hotspots(&repo_path, 90).unwrap_or_default();
    let couplings =
        code_explorer_git::coupling::analyze_coupling(&repo_path, 3, Some(180)).unwrap_or_default();
    let ownerships = code_explorer_git::ownership::analyze_ownership(&repo_path).unwrap_or_default();

    // Compute score (0-100); healthy projects score ~85-95
    let mut score: f64 = 100.0;

    // Penalize for high-churn hotspots
    let hot_files = hotspots.iter().filter(|h| h.score > 0.7).count();
    score -= (hot_files as f64) * 3.0;

    // Penalize for strong coupling
    let strong_couples = couplings
        .iter()
        .filter(|c| c.coupling_strength > 0.7)
        .count();
    score -= (strong_couples as f64) * 2.0;

    // Penalize for low ownership
    let orphan_files = ownerships.iter().filter(|o| o.ownership_pct < 50.0).count();
    score -= (orphan_files as f64) * 0.5;

    // Bonus for reasonable graph density
    if (1.0..=3.0).contains(&density) {
        score += 5.0;
    }

    // Round score to a whole number BEFORE deriving the grade, otherwise
    // `score as u32` (truncate toward zero) disagrees with the `{:.0}`
    // text format (round-half-to-even) at the grade boundaries — e.g.,
    // score = 89.9 would display as "90/100" but be graded as B because
    // `89.9 as u32 == 89`.
    let score = (score.clamp(0.0, 100.0)).round();
    let grade = match score as u32 {
        90..=100 => "A",
        75..=89 => "B",
        60..=74 => "C",
        40..=59 => "D",
        _ => "E",
    };

    if json {
        let report = serde_json::json!({
            "score": score,
            "grade": grade,
            "graph": {
                "nodes": node_count,
                "edges": edge_count,
                "files": file_count,
                "density": (density * 10.0).round() / 10.0,
            },
            "hotspots": {
                "total": hotspots.len(),
                "high_risk": hot_files,
                "top5": hotspots.iter().take(5).map(|h| serde_json::json!({
                    "file": h.path,
                    "commits": h.commit_count,
                    "churn": h.churn,
                    "score": (h.score * 100.0).round() / 100.0,
                })).collect::<Vec<_>>(),
            },
            "coupling": {
                "total_pairs": couplings.len(),
                "strong": strong_couples,
                "top5": couplings.iter().take(5).map(|c| serde_json::json!({
                    "file_a": c.file_a,
                    "file_b": c.file_b,
                    "strength": (c.coupling_strength * 100.0).round() / 100.0,
                })).collect::<Vec<_>>(),
            },
            "ownership": {
                "total_files": ownerships.len(),
                "orphan_files": orphan_files,
            },
        });
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(());
    }

    // Text output
    println!();
    println!(
        "{}",
        "═══════════════════════════════════════════════════════════".cyan()
    );
    println!("  \u{1f4ca} Code Health Report");
    println!(
        "{}",
        "═══════════════════════════════════════════════════════════".cyan()
    );
    println!();

    let grade_colored = match grade {
        "A" => grade.green().bold(),
        "B" => grade.green(),
        "C" => grade.yellow(),
        "D" => grade.red(),
        _ => grade.red().bold(),
    };
    println!("  Score: {} ({:.0}/100)", grade_colored, score);
    println!();

    // Graph stats
    println!("  {} Graph", "─".repeat(20).dimmed());
    println!(
        "    Nodes: {}  |  Edges: {}  |  Files: {}  |  Density: {:.1}",
        node_count, edge_count, file_count, density
    );
    println!();

    // Hotspots
    println!("  {} Hotspots (90 days)", "─".repeat(20).dimmed());
    if hotspots.is_empty() {
        println!("    No git history available");
    } else {
        println!(
            "    {} files analyzed, {} high-risk files (score > 70%)",
            hotspots.len(),
            hot_files
        );
        for h in hotspots.iter().take(5) {
            let bar = "█".repeat((h.score * 10.0) as usize);
            println!(
                "    {} {:.0}%  {} ({} commits, churn {})",
                bar,
                h.score * 100.0,
                h.path.replace('\\', "/"),
                h.commit_count,
                h.churn
            );
        }
    }
    println!();

    // Coupling
    println!("  {} Temporal Coupling", "─".repeat(20).dimmed());
    if couplings.is_empty() {
        println!("    No coupling data available");
    } else {
        println!(
            "    {} pairs detected, {} highly coupled (>70%)",
            couplings.len(),
            strong_couples
        );
        for c in couplings.iter().take(5) {
            println!(
                "    {:.0}%  {} <-> {}",
                c.coupling_strength * 100.0,
                c.file_a.replace('\\', "/"),
                c.file_b.replace('\\', "/")
            );
        }
    }
    println!();

    // Ownership
    println!("  {} Ownership", "─".repeat(20).dimmed());
    if ownerships.is_empty() {
        println!("    No ownership data available");
    } else {
        println!(
            "    {} files analyzed, {} without clear ownership (<50%)",
            ownerships.len(),
            orphan_files
        );
        // Show top orphans
        let mut orphans: Vec<_> = ownerships
            .iter()
            .filter(|o| o.ownership_pct < 50.0)
            .collect();
        orphans.sort_by(|a, b| {
            a.ownership_pct
                .partial_cmp(&b.ownership_pct)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        for o in orphans.iter().take(5) {
            println!(
                "    {:.0}%  {} ({})",
                o.ownership_pct,
                o.path.replace('\\', "/"),
                o.primary_author
            );
        }
    }
    println!();
    println!(
        "{}",
        "═══════════════════════════════════════════════════════════".cyan()
    );

    Ok(())
}

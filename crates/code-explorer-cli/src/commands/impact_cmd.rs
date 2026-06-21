//! The `impact` command: blast radius analysis via in-memory snapshot.

use std::path::Path;

use code_explorer_core::impact::{analyze_impact, ImpactDirectionResult};
use code_explorer_core::storage::repo_manager;

pub async fn run(target: &str, repo: Option<&str>, direction: &str) -> anyhow::Result<()> {
    let repo_path = resolve_repo_path(repo)?;
    let storage = repo_manager::get_storage_paths(&repo_path);
    let snap = code_explorer_db::snapshot::snapshot_path(&storage.storage_path);

    if !snap.exists() {
        return Err(anyhow::anyhow!(
            "No graph snapshot found. Run 'code-explorer analyze' first."
        ));
    }

    let graph = code_explorer_db::snapshot::load_snapshot(&snap)?;
    let max_depth = 5;
    let Some(impact) = analyze_impact(&graph, target, max_depth) else {
        println!("Symbol '{}' not found.", target);
        return Ok(());
    };

    let use_downstream = direction == "downstream" || direction == "both";
    let use_upstream = direction == "upstream" || direction == "both";

    println!(
        "Impact Analysis for '{}' (direction: {})",
        impact.target.name, direction
    );
    println!("{}", "-".repeat(50));

    if use_downstream {
        println!("\nDownstream (symbols affected by changes):");
        bfs_print(&graph, &impact.downstream);
    }

    if use_upstream {
        println!("\nUpstream (symbols that affect this):");
        bfs_print(&graph, &impact.upstream);
    }

    Ok(())
}

fn bfs_print(graph: &code_explorer_core::graph::KnowledgeGraph, result: &ImpactDirectionResult) {
    for (index, count) in result.depth_counts.iter().enumerate() {
        if *count == 0 {
            continue;
        }
        let depth = index + 1;
        println!("  Depth {} ({} nodes):", depth, count);
        for id in result.node_ids_at_depth(depth).take(10) {
            if let Some(node) = graph.get_node(id) {
                let loc = match node.properties.start_line {
                    Some(l) => format!("{}:{}", node.properties.file_path, l),
                    None => node.properties.file_path.clone(),
                };
                println!(
                    "    {} {} ({})",
                    node.label.as_str(),
                    node.properties.name,
                    loc
                );
            }
        }
        if *count > 10 {
            println!("    ... and {} more", count - 10);
        }
    }

    println!("  Total affected: {} symbols", result.total());
}

fn resolve_repo_path(repo: Option<&str>) -> anyhow::Result<std::path::PathBuf> {
    match repo {
        Some(r) => {
            let p = Path::new(r);
            Ok(p.canonicalize().unwrap_or_else(|_| p.to_path_buf()))
        }
        None => Ok(std::env::current_dir()?),
    }
}

//! The `context` command: 360-degree symbol view via in-memory snapshot.

use std::path::Path;

use code_explorer_core::graph::types::{NodeLabel, RelationshipType};
use code_explorer_core::storage::repo_manager;

pub async fn run(name: &str, repo: Option<&str>) -> anyhow::Result<()> {
    let repo_path = resolve_repo_path(repo)?;
    let storage = repo_manager::get_storage_paths(&repo_path);
    let snap = code_explorer_db::snapshot::snapshot_path(&storage.storage_path);

    if !snap.exists() {
        return Err(anyhow::anyhow!(
            "No graph snapshot found. Run 'code-explorer analyze' first."
        ));
    }

    let graph = code_explorer_db::snapshot::load_snapshot(&snap)?;
    let lower = name.to_lowercase();

    // Find matching node(s)
    let mut matches = Vec::new();
    for node in graph.iter_nodes() {
        if node.properties.name.to_lowercase() == lower {
            matches.push(node.id.clone());
        }
    }
    if matches.is_empty() {
        // Fallback to substring
        for node in graph.iter_nodes() {
            if node.properties.name.to_lowercase().contains(&lower) {
                matches.push(node.id.clone());
            }
        }
    }

    if matches.is_empty() {
        println!("Symbol '{}' not found.", name);
        return Ok(());
    }

    // Sort by priority: Controller > Class > Service > Interface > Method > others
    matches.sort_by_key(|id| {
        graph
            .get_node(id)
            .map(|n| match n.label {
                NodeLabel::Controller => 0,
                NodeLabel::Class => 1,
                NodeLabel::Service => 2,
                NodeLabel::Interface => 3,
                NodeLabel::Method => 5,
                NodeLabel::File => 8,
                _ => 10,
            })
            .unwrap_or(10)
    });

    let node_id = &matches[0];
    let node = graph.get_node(node_id).unwrap();

    println!("Symbol: {} ({})", node.properties.name, node.label.as_str());
    println!("File:   {}", node.properties.file_path);
    if let (Some(s), Some(e)) = (node.properties.start_line, node.properties.end_line) {
        println!("Lines:  {}-{}", s, e);
    }

    // Collect incoming and outgoing
    let mut callers = Vec::new();
    let mut callees = Vec::new();
    let mut other_in = Vec::new();
    let mut other_out = Vec::new();

    for rel in graph.iter_relationships() {
        if rel.target_id == *node_id {
            match rel.rel_type {
                RelationshipType::Calls => callers.push(rel.source_id.clone()),
                _ => other_in.push((rel.source_id.clone(), rel.rel_type)),
            }
        }
        if rel.source_id == *node_id {
            match rel.rel_type {
                RelationshipType::Calls => callees.push(rel.target_id.clone()),
                _ => other_out.push((rel.target_id.clone(), rel.rel_type)),
            }
        }
    }

    if !callers.is_empty() {
        println!("\nCallers ({}):", callers.len());
        for caller_id in &callers {
            if let Some(c) = graph.get_node(caller_id) {
                println!("  <- {} {}", c.label.as_str(), c.properties.name);
            }
        }
    }

    if !callees.is_empty() {
        println!("\nCallees ({}):", callees.len());
        for callee_id in &callees {
            if let Some(c) = graph.get_node(callee_id) {
                println!("  -> {} {}", c.label.as_str(), c.properties.name);
            }
        }
    }

    if !other_in.is_empty() {
        println!("\nIncoming relationships:");
        for (sid, rtype) in &other_in {
            if let Some(s) = graph.get_node(sid) {
                println!(
                    "  <--[{}]-- {} {}",
                    rtype.as_str(),
                    s.label.as_str(),
                    s.properties.name
                );
            }
        }
    }

    if !other_out.is_empty() {
        println!("\nOutgoing relationships:");
        for (tid, rtype) in &other_out {
            if let Some(t) = graph.get_node(tid) {
                println!(
                    "  --[{}]--> {} {}",
                    rtype.as_str(),
                    t.label.as_str(),
                    t.properties.name
                );
            }
        }
    }

    if matches.len() > 1 {
        println!(
            "\nNote: {} other symbols also match this name.",
            matches.len() - 1
        );
    }

    Ok(())
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

//! The `context` command: 360-degree symbol view via in-memory snapshot.

use std::collections::HashMap;
use std::path::Path;

use code_explorer_core::graph::types::RelationshipType;
use code_explorer_core::storage::repo_manager;
use code_explorer_core::symbol::selection::find_symbols;

pub async fn run(name: &str, repo: Option<&str>) -> anyhow::Result<()> {
    let repo_path = resolve_repo_path(repo)?;
    crate::commands::utils::check_and_warn_stale_index(&repo_path);
    let storage = repo_manager::get_storage_paths(&repo_path);
    let snap = code_explorer_db::snapshot::snapshot_path(&storage.storage_path);

    if !snap.exists() {
        return Err(anyhow::anyhow!(
            "No graph snapshot found. Run 'code-explorer analyze' first."
        ));
    }

    let graph = code_explorer_db::snapshot::load_snapshot(&snap)?;
    let matches = find_symbols(&graph, name);

    if matches.is_empty() {
        println!("Symbol '{}' not found.", name);
        return Ok(());
    }

    let node = matches[0];
    let node_id = &node.id;

    println!("Symbol: {} ({})", node.properties.name, node.label.as_str());
    println!("ID:     {}", node.id);
    println!("File:   {}", node.properties.file_path);
    if let (Some(s), Some(e)) = (node.properties.start_line, node.properties.end_line) {
        println!("Lines:  {}-{}", s, e);
    }
    if let Some(notes) = &node.properties.ambiguous_calls {
        println!("\nAmbiguous calls ({}):", notes.len());
        for note in notes {
            println!("  {note}");
        }
    }

    // Collect incoming and outgoing
    let mut callers = Vec::new();
    let mut callees = Vec::new();
    let mut other_in = Vec::new();
    let mut other_out = Vec::new();
    let mut caller_evidence = HashMap::new();
    let mut callee_evidence = HashMap::new();

    for rel in graph.iter_relationships() {
        if rel.target_id == *node_id {
            match rel.rel_type {
                RelationshipType::Calls => {
                    callers.push(rel.source_id.clone());
                    caller_evidence
                        .insert(rel.source_id.clone(), (rel.confidence, rel.reason.as_str()));
                }
                _ => other_in.push((rel.source_id.clone(), rel.rel_type)),
            }
        }
        if rel.source_id == *node_id {
            match rel.rel_type {
                RelationshipType::Calls => {
                    callees.push(rel.target_id.clone());
                    callee_evidence
                        .insert(rel.target_id.clone(), (rel.confidence, rel.reason.as_str()));
                }
                _ => other_out.push((rel.target_id.clone(), rel.rel_type)),
            }
        }
    }

    if !callers.is_empty() {
        println!("\nCallers ({}):", callers.len());
        for caller_id in &callers {
            if let Some(c) = graph.get_node(caller_id) {
                let (confidence, reason) =
                    caller_evidence.get(caller_id).copied().unwrap_or((0.0, ""));
                println!(
                    "  <- {} {} [confiance {:.2}; {}]",
                    c.label.as_str(),
                    c.properties.name,
                    confidence,
                    reason
                );
            }
        }
        let mut paths = Vec::new();
        for caller_id in &callers {
            let Some(caller) = graph.get_node(caller_id) else {
                continue;
            };
            for rel in graph
                .iter_relationships()
                .filter(|r| r.target_id == *caller_id && r.rel_type == RelationshipType::Calls)
            {
                if let Some(source) = graph.get_node(&rel.source_id) {
                    paths.push(format!(
                        "  <- {} <- {}",
                        caller.properties.name, source.properties.name
                    ));
                }
                if paths.len() >= 12 {
                    break;
                }
            }
            if paths.len() >= 12 {
                break;
            }
        }
        if !paths.is_empty() {
            println!("\nCaller paths (2 hops):");
            for path in paths {
                println!("{path}");
            }
        }
    }

    if !callees.is_empty() {
        println!("\nCallees ({}):", callees.len());
        for callee_id in &callees {
            if let Some(c) = graph.get_node(callee_id) {
                let (confidence, reason) =
                    callee_evidence.get(callee_id).copied().unwrap_or((0.0, ""));
                println!(
                    "  -> {} {} [confiance {:.2}; {}]",
                    c.label.as_str(),
                    c.properties.name,
                    confidence,
                    reason
                );
            }
        }

        let mut paths = Vec::new();
        for callee_id in &callees {
            let Some(callee) = graph.get_node(callee_id) else {
                continue;
            };
            for rel in graph
                .iter_relationships()
                .filter(|r| r.source_id == *callee_id && r.rel_type == RelationshipType::Calls)
            {
                if let Some(next) = graph.get_node(&rel.target_id) {
                    paths.push(format!(
                        "  -> {} -> {}",
                        callee.properties.name, next.properties.name
                    ));
                }
                if paths.len() >= 12 {
                    break;
                }
            }
            if paths.len() >= 12 {
                break;
            }
        }
        if !paths.is_empty() {
            println!("\nCallee paths (2 hops):");
            for path in paths {
                println!("{path}");
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
        let mut paths = Vec::new();
        for (sid, _) in &other_in {
            let Some(intermediate) = graph.get_node(sid) else {
                continue;
            };
            for rel in graph
                .iter_relationships()
                .filter(|r| r.target_id == *sid && r.rel_type == RelationshipType::DependsOn)
            {
                if let Some(source) = graph.get_node(&rel.source_id) {
                    paths.push(format!(
                        "  <-- {} <-- {}",
                        intermediate.properties.name, source.properties.name
                    ));
                }
                if paths.len() >= 12 {
                    break;
                }
            }
            if paths.len() >= 12 {
                break;
            }
        }
        if !paths.is_empty() {
            println!("\nIncoming dependency paths (2 hops):");
            for path in paths {
                println!("{path}");
            }
        }
    }

    if !other_out.is_empty() {
        println!("\nOutgoing relationships:");
        for (tid, rtype) in &other_out {
            if let Some(t) = graph.get_node(tid) {
                let location = t
                    .properties
                    .start_line
                    .map(|line| format!(" ({}:{line})", t.properties.file_path))
                    .unwrap_or_default();
                println!(
                    "  --[{}]--> {} {}{}",
                    rtype.as_str(),
                    t.label.as_str(),
                    t.properties.name,
                    location
                );
            }
        }
    }

    if matches.len() > 1 {
        println!("\nOther matches (use ID, Type.member or file:member):");
        for alternative in matches.iter().skip(1) {
            println!(
                "  {}  {}:{}",
                alternative.id,
                alternative.properties.file_path,
                alternative.properties.start_line.unwrap_or(0)
            );
        }
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

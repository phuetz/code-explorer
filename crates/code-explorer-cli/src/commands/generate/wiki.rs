//! Wiki page generator.

use std::collections::{BTreeSet, HashSet};
use std::io::Write;
use std::path::Path;

use anyhow::Result;
use colored::Colorize;
use tracing::{debug, info};

use code_explorer_core::graph::types::*;
use code_explorer_core::graph::KnowledgeGraph;

use super::utils::*;

pub(super) fn generate_wiki(graph: &KnowledgeGraph, repo_path: &Path) -> Result<()> {
    let wiki_dir = repo_path.join("wiki");
    std::fs::create_dir_all(&wiki_dir)?;

    let communities = collect_communities(graph);
    let edge_map = build_edge_map(graph);

    if communities.is_empty() {
        println!(
            "{} No communities found. Run `code-explorer analyze` first.",
            "!".yellow()
        );
        return Ok(());
    }

    let mut used_filenames_wiki: HashSet<String> = HashSet::new();

    for info in communities.values() {
        let base = sanitize_filename(&info.label);
        let filename = if used_filenames_wiki.contains(&base) {
            let mut candidate = base.clone();
            let mut counter = 2;
            while used_filenames_wiki.contains(&candidate) {
                candidate = format!("{}_{}", base, counter);
                counter += 1;
            }
            candidate
        } else {
            base
        };
        used_filenames_wiki.insert(filename.clone());
        let out_path = wiki_dir.join(format!("{filename}.md"));
        let mut f = std::fs::File::create(&out_path)?;

        debug!("Processing community: {}", info.label);
        writeln!(f, "# {}", info.label)?;
        writeln!(f)?;
        if let Some(desc) = &info.description {
            writeln!(f, "{desc}")?;
            writeln!(f)?;
        }
        if !info.keywords.is_empty() {
            writeln!(f, "**Keywords**: {}", info.keywords.join(", "))?;
            writeln!(f)?;
        }

        // Members
        let member_set: HashSet<&str> = info.member_ids.iter().map(|s| s.as_str()).collect();

        writeln!(f, "## Members")?;
        writeln!(f)?;
        writeln!(f, "| Symbol | Type | File | Lines |")?;
        writeln!(f, "|--------|------|------|-------|")?;

        let mut files_set: BTreeSet<String> = BTreeSet::new();
        for mid in &info.member_ids {
            if let Some(node) = graph.get_node(mid) {
                let lines = match (node.properties.start_line, node.properties.end_line) {
                    (Some(s), Some(e)) => format!("{s}-{e}"),
                    (Some(s), None) => format!("{s}"),
                    _ => "-".to_string(),
                };
                writeln!(
                    f,
                    "| `{}` | {} | `{}` | {} |",
                    node.properties.name,
                    node.label.as_str(),
                    node.properties.file_path,
                    lines
                )?;
                files_set.insert(node.properties.file_path.clone());
            }
        }
        writeln!(f)?;

        // Internal calls
        let mut internal_calls: Vec<(String, String)> = Vec::new();
        let mut external_deps: Vec<(String, String)> = Vec::new();

        for mid in &info.member_ids {
            if let Some(edges) = edge_map.get(mid.as_str()) {
                for (target_id, rel_type) in edges {
                    if *rel_type == RelationshipType::Calls {
                        let src_name = graph
                            .get_node(mid)
                            .map(|n| n.properties.name.as_str())
                            .unwrap_or("?");
                        let tgt_name = graph
                            .get_node(target_id)
                            .map(|n| n.properties.name.as_str())
                            .unwrap_or("?");

                        if member_set.contains(target_id.as_str()) {
                            internal_calls.push((src_name.to_string(), tgt_name.to_string()));
                        } else {
                            external_deps.push((src_name.to_string(), tgt_name.to_string()));
                        }
                    }
                }
            }
        }

        if !internal_calls.is_empty() {
            writeln!(f, "## Internal Calls")?;
            writeln!(f)?;
            for (src, tgt) in &internal_calls {
                writeln!(f, "- `{src}` -> `{tgt}`")?;
            }
            writeln!(f)?;
        }

        if !external_deps.is_empty() {
            writeln!(f, "## External Dependencies")?;
            writeln!(f)?;
            for (src, tgt) in &external_deps {
                writeln!(f, "- `{src}` -> `{tgt}`")?;
            }
            writeln!(f)?;
        }

        // Files
        if !files_set.is_empty() {
            writeln!(f, "## Files")?;
            writeln!(f)?;
            for file_path in &files_set {
                writeln!(f, "- `{file_path}`")?;
            }
            writeln!(f)?;
        }

        println!("  {} wiki/{filename}.md", "OK".green(),);
    }

    info!("Documentation generated: {} pages", communities.len());
    println!(
        "{} Generated {} wiki pages in {}",
        "OK".green(),
        communities.len(),
        wiki_dir.display()
    );
    Ok(())
}

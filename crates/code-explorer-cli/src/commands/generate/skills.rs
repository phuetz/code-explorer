//! Skills file generator.

use std::collections::{BTreeMap, HashSet};
use std::io::Write;
use std::path::Path;

use anyhow::Result;
use colored::Colorize;
use tracing::{debug, info};

use code_explorer_core::graph::types::*;
use code_explorer_core::graph::KnowledgeGraph;

use super::utils::*;

pub(super) fn generate_skills(graph: &KnowledgeGraph, repo_path: &Path) -> Result<()> {
    let skills_dir = repo_path.join("skills");
    std::fs::create_dir_all(&skills_dir)?;

    let communities = collect_communities(graph);
    let edge_map = build_edge_map(graph);

    if communities.is_empty() {
        println!(
            "{} No communities found. Run `code-explorer analyze` first.",
            "!".yellow()
        );
        return Ok(());
    }

    // Build member->community label mapping
    let mut member_to_community: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();
    for info in communities.values() {
        for mid in &info.member_ids {
            member_to_community.insert(mid.clone(), info.label.clone());
        }
    }

    let mut used_filenames_skills: HashSet<String> = HashSet::new();

    for info in communities.values() {
        let base = sanitize_filename(&info.label);
        let filename = if used_filenames_skills.contains(&base) {
            let mut candidate = base.clone();
            let mut counter = 2;
            while used_filenames_skills.contains(&candidate) {
                candidate = format!("{}_{}", base, counter);
                counter += 1;
            }
            candidate
        } else {
            base
        };
        used_filenames_skills.insert(filename.clone());
        let out_path = skills_dir.join(format!("{filename}.md"));
        let mut f = std::fs::File::create(&out_path)?;

        debug!("Processing module: {}", info.label);
        writeln!(f, "# Skill: {}", info.label)?;
        writeln!(f)?;

        // Infer responsibility from folder/file names
        let mut folders: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
        for mid in &info.member_ids {
            if let Some(node) = graph.get_node(mid) {
                let fp = &node.properties.file_path;
                if let Some(dir) = Path::new(fp).parent() {
                    folders.insert(dir.to_string_lossy().replace('\\', "/"));
                }
            }
        }
        if let Some(desc) = &info.description {
            writeln!(f, "## Responsibility")?;
            writeln!(f)?;
            writeln!(f, "{desc}")?;
            writeln!(f)?;
        } else if !folders.is_empty() {
            writeln!(f, "## Responsibility")?;
            writeln!(f)?;
            writeln!(
                f,
                "This module manages code in: {}",
                folders
                    .iter()
                    .map(|s| format!("`{s}`"))
                    .collect::<Vec<_>>()
                    .join(", ")
            )?;
            writeln!(f)?;
        }

        // Key functions
        let member_set: HashSet<&str> = info.member_ids.iter().map(|s| s.as_str()).collect();
        let key_labels = [
            NodeLabel::Function,
            NodeLabel::Method,
            NodeLabel::Constructor,
            NodeLabel::Class,
            NodeLabel::Struct,
            NodeLabel::Trait,
            NodeLabel::Interface,
        ];

        let mut key_symbols: Vec<&GraphNode> = info
            .member_ids
            .iter()
            .filter_map(|mid| graph.get_node(mid))
            .filter(|n| key_labels.contains(&n.label))
            .collect();
        // Sort by entry_point_score descending, then name
        key_symbols.sort_by(|a, b| {
            let sa = a.properties.entry_point_score.unwrap_or(0.0);
            let sb = b.properties.entry_point_score.unwrap_or(0.0);
            sb.partial_cmp(&sa)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.properties.name.cmp(&b.properties.name))
        });

        if !key_symbols.is_empty() {
            writeln!(f, "## Key Symbols")?;
            writeln!(f)?;
            for node in key_symbols.iter().take(20) {
                let role = if node
                    .properties
                    .entry_point_score
                    .map(|s| s > 0.3)
                    .unwrap_or(false)
                {
                    " (entry point)"
                } else {
                    ""
                };
                writeln!(
                    f,
                    "- `{}` ({}) in `{}`{}",
                    node.properties.name,
                    node.label.as_str(),
                    node.properties.file_path,
                    role
                )?;
            }
            writeln!(f)?;
        }

        // Entry points into this community
        let mut entry_points: Vec<&GraphNode> = info
            .member_ids
            .iter()
            .filter_map(|mid| graph.get_node(mid))
            .filter(|n| {
                n.properties
                    .entry_point_score
                    .map(|s| s > 0.3)
                    .unwrap_or(false)
            })
            .collect();
        entry_points.sort_by(|a, b| {
            let sa = a.properties.entry_point_score.unwrap_or(0.0);
            let sb = b.properties.entry_point_score.unwrap_or(0.0);
            sb.partial_cmp(&sa).unwrap_or(std::cmp::Ordering::Equal)
        });

        if !entry_points.is_empty() {
            writeln!(f, "## Entry Points")?;
            writeln!(f)?;
            for node in entry_points.iter().take(10) {
                let score = node.properties.entry_point_score.unwrap_or(0.0);
                writeln!(
                    f,
                    "- `{}` (score: {:.2}) in `{}`",
                    node.properties.name, score, node.properties.file_path
                )?;
            }
            writeln!(f)?;
        }

        // Connections to other communities
        let mut connected_communities: BTreeMap<String, usize> = BTreeMap::new();
        for mid in &info.member_ids {
            if let Some(edges) = edge_map.get(mid.as_str()) {
                for (target_id, rel_type) in edges {
                    if *rel_type == RelationshipType::Calls
                        && !member_set.contains(target_id.as_str())
                    {
                        if let Some(target_comm) = member_to_community.get(target_id) {
                            *connected_communities
                                .entry(target_comm.clone())
                                .or_insert(0) += 1;
                        }
                    }
                }
            }
        }

        if !connected_communities.is_empty() {
            writeln!(f, "## Connections to Other Modules")?;
            writeln!(f)?;
            let mut sorted: Vec<_> = connected_communities.into_iter().collect();
            sorted.sort_by(|a, b| b.1.cmp(&a.1));
            for (comm_label, call_count) in sorted {
                writeln!(f, "- **{comm_label}**: {call_count} call(s)")?;
            }
            writeln!(f)?;
        }

        println!("  {} skills/{filename}.md", "OK".green(),);
    }

    info!("Documentation generated: {} pages", communities.len());
    println!(
        "{} Generated {} skill files in {}",
        "OK".green(),
        communities.len(),
        skills_dir.display()
    );
    Ok(())
}

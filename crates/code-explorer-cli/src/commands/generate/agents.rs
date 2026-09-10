//! AGENTS.md generator.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::io::Write;
use std::path::Path;

use anyhow::Result;
use colored::Colorize;
use tracing::{debug, info};

use code_explorer_core::graph::types::*;
use code_explorer_core::graph::KnowledgeGraph;

use super::utils::*;

pub(super) fn generate_agents_md(graph: &KnowledgeGraph, repo_path: &Path) -> Result<()> {
    let repo_name = repo_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("repository");

    let file_count = count_files(graph);
    let lang_stats = collect_language_stats(graph);
    let communities = collect_communities(graph);

    let out_path = repo_path.join("AGENTS.md");
    let mut f = std::fs::File::create(&out_path)?;

    debug!("Processing {} communities for AGENTS.md", communities.len());

    // Header
    writeln!(f, "# {repo_name}")?;
    writeln!(f)?;
    writeln!(
        f,
        "Auto-generated codebase context for AI agents. {file_count} source files indexed."
    )?;
    writeln!(f)?;

    // Languages
    writeln!(f, "## Languages")?;
    writeln!(f)?;
    for (lang, count) in &lang_stats {
        writeln!(f, "- **{lang}**: {count} files")?;
    }
    writeln!(f)?;

    // Communities
    if !communities.is_empty() {
        writeln!(f, "## Modules / Communities")?;
        writeln!(f)?;
        for info in communities.values() {
            let member_count = info.member_ids.len();
            writeln!(f, "### {}", info.label)?;
            writeln!(f)?;
            if let Some(desc) = &info.description {
                writeln!(f, "{desc}")?;
                writeln!(f)?;
            }
            writeln!(f, "- Members: {member_count} symbols")?;

            // Show key symbols (up to 8)
            let mut key_symbols: Vec<String> = Vec::new();
            for mid in info.member_ids.iter().take(8) {
                if let Some(node) = graph.get_node(mid) {
                    key_symbols.push(format!(
                        "`{}` ({})",
                        node.properties.name,
                        node.label.as_str()
                    ));
                }
            }
            if !key_symbols.is_empty() {
                writeln!(f, "- Key symbols: {}", key_symbols.join(", "))?;
            }
            if !info.keywords.is_empty() {
                writeln!(f, "- Keywords: {}", info.keywords.join(", "))?;
            }
            writeln!(f)?;
        }
    }

    // Entry points
    let mut entry_points: Vec<(&GraphNode, f64)> = graph
        .iter_nodes()
        .filter_map(|n| {
            n.properties
                .entry_point_score
                .filter(|&s| s > 0.3)
                .map(|s| (n, s))
        })
        .collect();
    entry_points.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

    if !entry_points.is_empty() {
        writeln!(f, "## Entry Points")?;
        writeln!(f)?;
        for (node, score) in entry_points.iter().take(15) {
            let reason = node.properties.entry_point_reason.as_deref().unwrap_or("");
            writeln!(
                f,
                "- `{}` in `{}` (score: {:.2}) {}",
                node.properties.name, node.properties.file_path, score, reason
            )?;
        }
        writeln!(f)?;
    }

    // Execution flows (Processes)
    let processes: Vec<&GraphNode> = graph
        .iter_nodes()
        .filter(|n| n.label == NodeLabel::Process)
        .collect();
    if !processes.is_empty() {
        writeln!(f, "## Execution Flows")?;
        writeln!(f)?;
        for proc_node in processes.iter().take(20) {
            let step_count = proc_node.properties.step_count.unwrap_or(0);
            let ptype = proc_node
                .properties
                .process_type
                .map(|t| match t {
                    ProcessType::IntraCommunity => "intra-community",
                    ProcessType::CrossCommunity => "cross-community",
                })
                .unwrap_or("unknown");
            writeln!(
                f,
                "- **{}**: {} steps ({ptype})",
                proc_node.properties.name, step_count
            )?;
            if let Some(desc) = &proc_node.properties.description {
                writeln!(f, "  {desc}")?;
            }
        }
        writeln!(f)?;
    }

    // Architecture overview: inter-community CALLS
    if communities.len() > 1 {
        writeln!(f, "## Architecture (inter-module dependencies)")?;
        writeln!(f)?;

        // Build set of member->community mappings
        let mut member_to_community: HashMap<String, String> = HashMap::new();
        for info in communities.values() {
            for mid in &info.member_ids {
                member_to_community.insert(mid.clone(), info.label.clone());
            }
        }

        let mut cross_deps: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        for rel in graph.iter_relationships() {
            if rel.rel_type == RelationshipType::Calls {
                if let (Some(src_comm), Some(tgt_comm)) = (
                    member_to_community.get(&rel.source_id),
                    member_to_community.get(&rel.target_id),
                ) {
                    if src_comm != tgt_comm {
                        cross_deps
                            .entry(src_comm.clone())
                            .or_default()
                            .insert(tgt_comm.clone());
                    }
                }
            }
        }

        for (src, targets) in &cross_deps {
            let targets_str: Vec<&str> = targets.iter().map(|s| s.as_str()).collect();
            writeln!(f, "- **{src}** depends on: {}", targets_str.join(", "))?;
        }
        writeln!(f)?;
    }

    info!("Documentation generated: 1 page");
    println!("{} Generated {}", "OK".green(), out_path.display());
    Ok(())
}

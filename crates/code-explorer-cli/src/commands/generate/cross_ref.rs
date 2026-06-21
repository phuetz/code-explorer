//! Cross-reference linking between documentation pages.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use anyhow::Result;

use code_explorer_core::graph::types::*;
use code_explorer_core::graph::KnowledgeGraph;

use super::utils::sanitize_filename;

/// Post-processing step that adds cross-reference links between documentation pages.
/// Runs after all pages are generated (and optionally enriched) but before HTML generation.
pub(super) fn apply_cross_references(docs_dir: &Path, graph: &KnowledgeGraph) -> Result<usize> {
    // 1. Build a map of known names -> page links
    let mut known_names: Vec<(String, String)> = Vec::new(); // (name, link)

    // Controllers, Services, Repositories, DbEntities, ExternalServices
    for node in graph.iter_nodes() {
        match node.label {
            NodeLabel::Controller => {
                let name = &node.properties.name;
                let filename = format!("ctrl-{}", sanitize_filename(name));
                known_names.push((name.clone(), format!("./modules/{}.md", filename)));
            }
            NodeLabel::Service | NodeLabel::Repository => {
                let anchor = sanitize_for_anchor(&node.properties.name);
                known_names.push((
                    node.properties.name.clone(),
                    format!("./modules/services.md#{}", anchor),
                ));
            }
            NodeLabel::DbEntity => {
                known_names.push((
                    node.properties.name.clone(),
                    format!("./modules/data-entities.md#{}", node.properties.name),
                ));
            }
            NodeLabel::ExternalService => {
                known_names.push((
                    node.properties.name.clone(),
                    "./modules/external-services.md".to_string(),
                ));
            }
            NodeLabel::View | NodeLabel::PartialView => {
                let views_page = docs_dir.join("modules").join("views.md");
                if views_page.exists() {
                    let anchor = sanitize_for_anchor(&node.properties.name);
                    known_names.push((
                        node.properties.name.clone(),
                        format!("./modules/views.md#{}", anchor),
                    ));
                }
            }
            NodeLabel::ViewModel => {
                let views_page = docs_dir.join("modules").join("views.md");
                if views_page.exists() {
                    let anchor = sanitize_for_anchor(&node.properties.name);
                    known_names.push((
                        node.properties.name.clone(),
                        format!("./modules/views.md#{}", anchor),
                    ));
                }
            }
            _ => {}
        }
    }

    // Sort by length descending (longest match first, avoid partial matches)
    known_names.sort_by(|a, b| b.0.len().cmp(&a.0.len()));

    // Filter out names shorter than 5 chars (too generic)
    known_names.retain(|(name, _)| name.len() >= 5);

    // 2. Process each .md file
    let mut total_links = 0;
    let mut files_to_process: Vec<PathBuf> = Vec::new();
    // backlinks: target_stem -> vec[source_stems]
    let mut backlinks: HashMap<String, Vec<String>> = HashMap::new();

    for entry in std::fs::read_dir(docs_dir)?.flatten() {
        if entry.path().extension().is_some_and(|e| e == "md") {
            files_to_process.push(entry.path());
        }
    }
    let modules_dir = docs_dir.join("modules");
    if modules_dir.exists() {
        for entry in std::fs::read_dir(&modules_dir)?.flatten() {
            if entry.path().extension().is_some_and(|e| e == "md") {
                files_to_process.push(entry.path());
            }
        }
    }
    let processes_dir = docs_dir.join("processes");
    if processes_dir.exists() {
        for entry in std::fs::read_dir(&processes_dir)?.flatten() {
            if entry.path().extension().is_some_and(|e| e == "md") {
                files_to_process.push(entry.path());
            }
        }
    }

    for file_path in &files_to_process {
        let content = std::fs::read_to_string(file_path)?;
        let mut modified = content.clone();
        let mut linked_names: HashSet<String> = HashSet::new();
        let mut page_links = 0;

        for (name, link) in &known_names {
            // Skip if already linked on this page
            if linked_names.contains(name) {
                continue;
            }

            // Skip self-references (don't link to the current page)
            if link.contains(file_path.file_stem().and_then(|s| s.to_str()).unwrap_or("")) {
                continue;
            }

            // Find FIRST whole-word occurrence that's not inside a code block,
            // heading, or existing link. The previous implementation used
            // `modified.find(name)` which matches substrings inside longer
            // identifiers — e.g. searching for "Product" in
            // "ProductCatalog shows..." matches at index 0 and produced the
            // broken output "[Product](./link.md)Catalog shows...". Walk
            // forward through every candidate position and accept only the
            // first one where the surrounding chars are NOT alphanumeric or
            // underscore.
            let bytes = modified.as_bytes();
            let name_bytes = name.as_bytes();
            let mut scan_from = 0usize;
            let mut found_idx: Option<usize> = None;
            while let Some(rel) = modified[scan_from..].find(name.as_str()) {
                let idx = scan_from + rel;
                let end = idx + name_bytes.len();
                let before_ok = idx == 0 || {
                    let prev = bytes[idx - 1];
                    !(prev.is_ascii_alphanumeric() || prev == b'_')
                };
                let after_ok = end >= bytes.len() || {
                    let next = bytes[end];
                    !(next.is_ascii_alphanumeric() || next == b'_')
                };
                if before_ok && after_ok {
                    found_idx = Some(idx);
                    break;
                }
                scan_from = idx + name_bytes.len();
                if scan_from >= modified.len() {
                    break;
                }
            }
            if let Some(idx) = found_idx {
                // Check context: skip if inside code block or already linked
                let before = &modified[..idx];

                let in_code = before.matches("```").count() % 2 == 1;
                let in_inline_code = before.ends_with('`');
                let in_link = before.ends_with('[') || before.ends_with("](");
                let in_heading = before.lines().last().is_some_and(|l| l.starts_with('#'));

                if !in_code && !in_inline_code && !in_link && !in_heading {
                    // Replace the found occurrence with a link
                    modified = format!(
                        "{}[{}]({}){}",
                        &modified[..idx],
                        name,
                        link,
                        &modified[idx + name.len()..]
                    );
                    linked_names.insert(name.clone());
                    page_links += 1;
                    // Record backlink: target_stem -> source_stem
                    let target_stem = link
                        .trim_start_matches("./modules/")
                        .trim_start_matches("./")
                        .trim_end_matches(".md")
                        .split('#')
                        .next()
                        .unwrap_or("")
                        .to_string();
                    if !target_stem.is_empty() {
                        if let Some(source_stem) = file_path.file_stem().and_then(|s| s.to_str()) {
                            backlinks
                                .entry(target_stem)
                                .or_default()
                                .push(source_stem.to_string());
                        }
                    }
                }
            }
        }

        if page_links > 0 {
            std::fs::write(file_path, &modified)?;
            total_links += page_links;
        }
    }

    // Write backlinks.json for the HTML site to embed
    if !backlinks.is_empty() {
        let meta_dir = docs_dir.join("_meta");
        std::fs::create_dir_all(&meta_dir)?;
        let backlinks_path = meta_dir.join("backlinks.json");
        let json = serde_json::to_string_pretty(&backlinks)?;
        std::fs::write(&backlinks_path, json)?;
    }

    Ok(total_links)
}

/// Convert a symbol name to a Markdown/HTML anchor slug:
/// lowercase, spaces and underscores replaced with `-`, non-alphanumeric stripped.
fn sanitize_for_anchor(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>()
        .split('-')
        .filter(|p| !p.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

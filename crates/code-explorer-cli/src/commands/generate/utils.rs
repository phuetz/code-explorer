//! Graph utility functions, CommunityInfo struct, and sanitization helpers.

#[allow(unused_imports)]
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
#[allow(unused_imports)]
use std::path::Path;

use code_explorer_core::graph::types::*;
use code_explorer_core::graph::KnowledgeGraph;

/// Collect community info: community node ID -> (heuristic_label, member node IDs).
pub(super) fn collect_communities(graph: &KnowledgeGraph) -> BTreeMap<String, CommunityInfo> {
    let mut communities: BTreeMap<String, CommunityInfo> = BTreeMap::new();

    // First pass: find Community nodes
    for node in graph.iter_nodes() {
        if node.label == NodeLabel::Community {
            let label = node
                .properties
                .heuristic_label
                .clone()
                .unwrap_or_else(|| node.properties.name.clone());
            communities.insert(
                node.id.clone(),
                CommunityInfo {
                    label,
                    description: node.properties.description.clone(),
                    keywords: node.properties.keywords.clone().unwrap_or_default(),
                    member_ids: Vec::new(),
                },
            );
        }
    }

    // Second pass: find MEMBER_OF relationships to populate members
    for rel in graph.iter_relationships() {
        if rel.rel_type == RelationshipType::MemberOf {
            if let Some(info) = communities.get_mut(&rel.target_id) {
                info.member_ids.push(rel.source_id.clone());
            }
        }
    }

    communities
}

pub(super) struct CommunityInfo {
    pub(super) label: String,
    pub(super) description: Option<String>,
    pub(super) keywords: Vec<String>,
    pub(super) member_ids: Vec<String>,
}

/// Collect language statistics.
pub(super) fn collect_language_stats(graph: &KnowledgeGraph) -> BTreeMap<String, usize> {
    let mut lang_counts: BTreeMap<String, usize> = BTreeMap::new();
    for node in graph.iter_nodes() {
        if node.label == NodeLabel::File {
            if let Some(lang) = &node.properties.language {
                *lang_counts.entry(lang.as_str().to_string()).or_insert(0) += 1;
            }
        }
    }
    lang_counts
}

/// Count files.
pub(super) fn count_files(graph: &KnowledgeGraph) -> usize {
    graph
        .iter_nodes()
        .filter(|n| n.label == NodeLabel::File)
        .count()
}

/// Build outgoing edges map: source_id -> Vec<(target_id, rel_type)>.
pub(super) fn build_edge_map(
    graph: &KnowledgeGraph,
) -> HashMap<String, Vec<(String, RelationshipType)>> {
    let mut map: HashMap<String, Vec<(String, RelationshipType)>> = HashMap::new();
    for rel in graph.iter_relationships() {
        map.entry(rel.source_id.clone())
            .or_default()
            .push((rel.target_id.clone(), rel.rel_type));
    }
    map
}

/// Sanitize a label for use as a filename.
pub(super) fn sanitize_filename(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect::<String>()
        .to_lowercase()
}

/// Escape a label for safe use inside Mermaid `["..."]` quoted strings.
/// Replaces special characters with HTML entities (`&amp;`, `&lt;`, etc.) —
/// the form that Mermaid documents and that survives downstream consumers
/// like SVG export, HTML fallback rendering and tooltip text.
///
/// A previous version of this helper emitted `#amp;` / `#quot;` / `#lt;` /
/// `#gt;`. The `#`-prefixed form is an undocumented Mermaid shortcut that
/// only works inside the live flowchart renderer — any post-render path
/// that treats the label as text (e.g. our own `strip_html_tags` search
/// index, the DOCX exporter) shows the literal `#amp;` to the user. Use
/// the canonical HTML entity form to stay consistent with the rest of the
/// codebase (`process_doc.rs`, `diagram.rs`, `export.rs`, `process.rs`).
pub(super) fn escape_mermaid_label(label: &str) -> String {
    label
        .replace('&', "&amp;")
        .replace('"', "&quot;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('[', "&#91;")
        .replace(']', "&#93;")
        .replace('\n', " ")
        .replace('\r', "")
}

/// Sanitize a string for use as a Mermaid node ID.
/// Keeps only alphanumeric characters and underscores.
pub(super) fn sanitize_mermaid_id(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

/// Generate a `<details>` block listing relevant source files.
pub(super) fn source_files_section(files: &[&str]) -> String {
    if files.is_empty() {
        return String::new();
    }
    let mut s = String::from("\n<details>\n<summary>Fichiers source pertinents</summary>\n\n");
    for f in files.iter().take(15) {
        s.push_str(&format!("- `{}`\n", f));
    }
    s.push_str("\n</details>\n\n");
    s
}

/// Format method parameters from the stored description field.
/// Input: "string id, int page" (raw from ActionInfo.parameters)
/// Output: "`string` id, `int` page"
pub(super) fn extract_params_from_content(params_str: &str, _method_name: &str) -> String {
    if params_str.is_empty() {
        return "-".to_string();
    }

    let params: Vec<String> = params_str
        .split(',')
        .map(|p| {
            // Strip default values: "string nia = null" → "string nia"
            let p_clean = p.split('=').next().unwrap_or(p).trim();
            let parts: Vec<&str> = p_clean.split_whitespace().collect();
            if parts.len() >= 2 {
                // Walk from the END: parameter name is the last token, type
                // is the token immediately before it. Anything earlier is a
                // modifier (`out`, `ref`, `in`, `params`) or an attribute
                // (`[FromBody]`, `[Required]`). Same fix as
                // `extract_all_method_signatures` — keep the three helpers in
                // sync so `[FromBody] UserModel user` renders as
                // `` `UserModel` user`` instead of `` `[FromBody]` user``.
                let param_name = parts[parts.len() - 1];
                let type_name = parts[parts.len() - 2];
                format!("`{}` {}", type_name, param_name)
            } else if parts.len() == 1 {
                format!("`{}`", parts[0])
            } else {
                p.trim().to_string()
            }
        })
        .collect();

    params.join(", ")
}

/// Format method parameters, highlighting parameters whose type matches a
/// known entity/model so the reader can spot domain types at a glance.
///
/// Earlier versions of this helper emitted a markdown link to a hard-coded
/// data model page (`./modules/data-alisev2entities.md`) which only existed
/// in one specific consumer's deployment. Generating those links for any
/// other project produced broken navigation across every controller page,
/// so we now bold known types instead — visually distinct without lying
/// about a destination page that may not exist.
pub(super) fn extract_params_linked(params_str: &str, known_types: &HashSet<String>) -> String {
    if params_str.is_empty() {
        return "-".to_string();
    }

    let params: Vec<String> = params_str
        .split(',')
        .map(|p| {
            // Strip default values: "string nia = null" → "string nia"
            let p_clean = p.split('=').next().unwrap_or(p).trim();
            let parts: Vec<&str> = p_clean.split_whitespace().collect();
            if parts.len() >= 2 {
                // Walk from the END: parameter name is the last token, type
                // is the token immediately before it. Anything earlier is a
                // modifier (`out`, `ref`, `in`, `params`) or an attribute
                // (`[FromBody]`, `[Required]`). Same fix as
                // `extract_all_method_signatures` — previously `parts[0]`
                // was treated as the type, so `[FromBody] UserModel user`
                // rendered as `` `[FromBody]` user`` and `out int result`
                // rendered as `` `out` result``, losing the actual type and
                // defeating the known-type highlighting.
                let param_name = parts[parts.len() - 1];
                let type_name = parts[parts.len() - 2];
                if known_types.contains(type_name) {
                    format!("**`{}`** {}", type_name, param_name)
                } else {
                    format!("`{}` {}", type_name, param_name)
                }
            } else if parts.len() == 1 {
                format!("`{}`", parts[0])
            } else {
                p.trim().to_string()
            }
        })
        .collect();

    params.join(", ")
}

/// Extract ALL method signatures (params + return type) from source code, including overloads.
pub(super) fn extract_all_method_signatures(
    source: &str,
    method_name: &str,
) -> Vec<(String, String)> {
    let mut results = Vec::new();

    for line in source.lines() {
        let trimmed = line.trim();
        if !trimmed.contains(method_name) || !trimmed.contains('(') {
            continue;
        }
        if !trimmed.starts_with("public")
            && !trimmed.starts_with("private")
            && !trimmed.starts_with("protected")
            && !trimmed.starts_with("async")
        {
            continue;
        }
        if trimmed.contains("await ") || trimmed.contains(".GetAwaiter") || trimmed.contains("=>") {
            continue;
        }
        // Must contain the exact method name followed by (
        let pattern = format!("{}(", method_name);
        if !trimmed.contains(&pattern) && !trimmed.contains(&format!("{} (", method_name)) {
            continue;
        }

        let before_name = trimmed.split(method_name).next().unwrap_or("");
        let words: Vec<&str> = before_name.split_whitespace().collect();
        let ret_type = if words.len() >= 2 {
            words[words.len() - 1].to_string()
        } else {
            "-".to_string()
        };

        let clean_ret = ret_type
            .replace("System.Threading.Tasks.Task<", "")
            .replace("System.Collections.Generic.ICollection<", "ICollection<")
            .trim_end_matches('>')
            .to_string();

        if let Some(paren_start) = trimmed.find('(') {
            let after = &trimmed[paren_start + 1..];
            if let Some(paren_end) = after.find(')') {
                let params_raw = after[..paren_end].trim();
                if params_raw.is_empty() || params_raw == ")" {
                    results.push(("-".to_string(), clean_ret));
                    continue;
                }

                // Format params: simplify System.* types
                let params: Vec<String> = params_raw
                    .split(',')
                    .map(|p| {
                        // Strip default values: "string nia = null" → "string nia"
                        let p_clean = p
                            .split('=')
                            .next()
                            .unwrap_or(p)
                            .trim()
                            .replace("System.Threading.CancellationToken", "CancellationToken")
                            .replace("System.Threading.Tasks.", "");
                        let parts: Vec<&str> = p_clean.split_whitespace().collect();
                        if parts.len() >= 2 {
                            // Walk from the END of the token list: the parameter
                            // name is always the last token, the type is the
                            // token immediately before it. Anything earlier is a
                            // modifier (`out`, `ref`, `in`, `params`) or an
                            // attribute (`[FromBody]`, `[Required]`). The
                            // previous implementation took `parts[0]` as the
                            // type and `parts[1]` as the name, so
                            // `[FromBody] string name` rendered as
                            // `` `[FromBody]` string`` and `out int result`
                            // rendered as `` `out` int``.
                            let param_name = parts[parts.len() - 1];
                            let type_name = parts[parts.len() - 2];
                            format!("`{}` {}", type_name, param_name)
                        } else if parts.len() == 1 {
                            format!("`{}`", parts[0])
                        } else {
                            p.trim().to_string()
                        }
                    })
                    .collect();

                // Filter out CancellationToken (internal plumbing)
                let visible_params: Vec<&String> = params
                    .iter()
                    .filter(|p| !p.contains("CancellationToken"))
                    .collect();

                let params_str = if visible_params.is_empty() {
                    "-".to_string()
                } else {
                    visible_params
                        .iter()
                        .map(|s| s.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                };

                results.push((params_str, clean_ret));
            }
        }
    }
    if results.is_empty() {
        results.push(("-".to_string(), "-".to_string()));
    }
    results
}

/// Extract a method body from source code by finding the method declaration and reading until its closing brace.
pub(super) fn extract_method_body(
    source: &str,
    method_name: &str,
    max_lines: usize,
) -> Option<String> {
    let lines: Vec<&str> = source.lines().collect();
    let pattern = format!(" {}(", method_name);

    // Find the method declaration line
    let start_idx = lines.iter().position(|line| {
        let trimmed = line.trim();
        trimmed.contains(&pattern)
            && (trimmed.starts_with("public")
                || trimmed.starts_with("private")
                || trimmed.starts_with("protected")
                || trimmed.starts_with("["))
            && !trimmed.contains("await ")
            && !trimmed.contains(".GetAwaiter")
    })?;

    // Count braces to find the method end
    let mut brace_count = 0;
    let mut found_open = false;
    let mut end_idx = start_idx;

    for (i, line) in lines[start_idx..].iter().enumerate() {
        for ch in line.chars() {
            if ch == '{' {
                brace_count += 1;
                found_open = true;
            } else if ch == '}' {
                brace_count -= 1;
            }
        }
        end_idx = start_idx + i;
        if found_open && brace_count == 0 {
            break;
        }
        // Safety: don't go past max_lines
        if i >= max_lines {
            break;
        }
    }

    let actual_end = (end_idx + 1).min(lines.len());
    let snippet_lines = &lines[start_idx..actual_end];

    if snippet_lines.is_empty() {
        return None;
    }

    let mut result = String::new();
    for line in snippet_lines {
        result.push_str(line);
        result.push('\n');
    }

    if !found_open || brace_count > 0 {
        result.push_str("// ... (méthode tronquée)\n");
    }

    Some(result)
}

/// Count nodes by label type in the graph.
pub(super) fn count_nodes_by_label(graph: &KnowledgeGraph) -> HashMap<NodeLabel, usize> {
    let mut counts: HashMap<NodeLabel, usize> = HashMap::new();
    for node in graph.iter_nodes() {
        *counts.entry(node.label).or_insert(0) += 1;
    }
    counts
}

/// Find the top N most-connected files (by total degree) in the graph.
pub(super) fn top_connected_files(graph: &KnowledgeGraph, n: usize) -> Vec<String> {
    let mut file_degree: HashMap<String, usize> = HashMap::new();
    for rel in graph.iter_relationships() {
        // Count source file
        if let Some(src_node) = graph.get_node(&rel.source_id) {
            if !src_node.properties.file_path.is_empty() {
                *file_degree
                    .entry(src_node.properties.file_path.clone())
                    .or_insert(0) += 1;
            }
        }
        // Count target file
        if let Some(tgt_node) = graph.get_node(&rel.target_id) {
            if !tgt_node.properties.file_path.is_empty() {
                *file_degree
                    .entry(tgt_node.properties.file_path.clone())
                    .or_insert(0) += 1;
            }
        }
    }
    let mut sorted: Vec<_> = file_degree.into_iter().collect();
    sorted.sort_by(|a, b| b.1.cmp(&a.1));
    sorted.into_iter().take(n).map(|(f, _)| f).collect()
}

/// Detect frameworks/libraries from graph nodes and file extensions.
pub(super) fn detect_technology_stack(
    graph: &KnowledgeGraph,
    lang_stats: &BTreeMap<String, usize>,
) -> (Vec<String>, Vec<String>, Vec<String>, String) {
    let mut languages: Vec<String> = Vec::new();
    let mut frameworks: Vec<String> = Vec::new();
    let mut ui_libs: Vec<String> = Vec::new();
    let mut description_parts: Vec<String> = Vec::new();

    // Languages
    for (lang, count) in lang_stats {
        languages.push(format!("{} ({} files)", lang, count));
    }

    // Detect frameworks from node labels
    let label_counts = count_nodes_by_label(graph);
    let has_controllers = label_counts
        .get(&NodeLabel::Controller)
        .copied()
        .unwrap_or(0)
        > 0;
    let has_db_context = label_counts
        .get(&NodeLabel::DbContext)
        .copied()
        .unwrap_or(0)
        > 0;
    let has_db_entities = label_counts.get(&NodeLabel::DbEntity).copied().unwrap_or(0) > 0;
    let has_views = label_counts.get(&NodeLabel::View).copied().unwrap_or(0) > 0;
    let has_ui_components = label_counts
        .get(&NodeLabel::UiComponent)
        .copied()
        .unwrap_or(0)
        > 0;
    let has_services = label_counts.get(&NodeLabel::Service).copied().unwrap_or(0) > 0;
    let has_external = label_counts
        .get(&NodeLabel::ExternalService)
        .copied()
        .unwrap_or(0)
        > 0;

    if has_controllers {
        frameworks.push("ASP.NET MVC 5".to_string());
        description_parts.push("ASP.NET MVC 5 application".to_string());
    }
    if has_db_context || has_db_entities {
        frameworks.push("Entity Framework 6".to_string());
        if description_parts.is_empty() {
            description_parts.push("Entity Framework application".to_string());
        } else {
            description_parts.push("Entity Framework 6".to_string());
        }
    }
    if has_ui_components {
        // Check for Telerik/Kendo
        let has_telerik = graph.iter_nodes().any(|n| {
            n.label == NodeLabel::UiComponent
                && n.properties
                    .component_type
                    .as_deref()
                    .is_some_and(|t| t.contains("Telerik") || t.contains("Kendo"))
        });
        if has_telerik {
            ui_libs.push("Telerik UI / Kendo UI".to_string());
            description_parts.push("Telerik UI components".to_string());
        } else {
            ui_libs.push("Custom UI Components".to_string());
        }
    }
    if has_external {
        description_parts.push("external service integrations".to_string());
    }
    if has_services {
        // Check if we have repository pattern too
        let has_repos = label_counts
            .get(&NodeLabel::Repository)
            .copied()
            .unwrap_or(0)
            > 0;
        if has_repos {
            frameworks.push("Repository Pattern".to_string());
        }
    }
    if has_views {
        let has_razor = graph.iter_nodes().any(|n| {
            n.label == NodeLabel::View && n.properties.view_engine.as_deref() == Some("razor")
        });
        if has_razor {
            frameworks.push("Razor Views".to_string());
        }
    }

    // If no ASP.NET detected, describe generically
    if description_parts.is_empty() {
        let primary_lang = lang_stats
            .iter()
            .max_by_key(|(_, c)| *c)
            .map(|(l, _)| l.as_str())
            .unwrap_or("multi-language");
        description_parts.push(format!("{} codebase", primary_lang));
    }

    let description = if description_parts.len() == 1 {
        format!("{}.", description_parts[0])
    } else {
        let last = description_parts.pop().unwrap_or_default();
        format!("{} with {}.", description_parts.join(", "), last)
    };

    (languages, frameworks, ui_libs, description)
}

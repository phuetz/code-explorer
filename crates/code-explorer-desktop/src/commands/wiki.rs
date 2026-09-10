//! Wiki generation — produces Markdown pages, one per detected community,
//! with cross-references between modules. Pure-graph (no LLM) so it runs
//! offline and instantly.
//!
//! Output goes to `<repo_path>/wiki/` so it sits next to the source the user
//! is browsing. Each page lists the community's key files, key symbols,
//! cross-community edges, and entry points.
//!
//! This is the desktop-native equivalent of the CLI's `generate wiki`,
//! re-implemented so the desktop doesn't need to shell out.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, State};

use code_explorer_core::graph::types::*;
use code_explorer_core::llm::PROMPT_CONTEXT_SAFETY;

use crate::state::AppState;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WikiGenerateRequest {
    /// Output directory. Default: `<repo>/wiki/`.
    #[serde(default)]
    pub out_dir: Option<String>,
    /// When true, also produces an `index.md` summarising all modules.
    #[serde(default)]
    pub with_index: Option<bool>,
    /// When true, calls the configured LLM to add a one-paragraph
    /// description at the top of each module page. Slower (one LLM call
    /// per community) but produces dramatically more readable docs.
    #[serde(default)]
    pub enrich_with_llm: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WikiGenerateResult {
    pub out_dir: String,
    pub pages: Vec<WikiPage>,
    pub total_files: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WikiPage {
    pub module: String,
    pub filename: String,
    pub path: String,
    pub member_count: u32,
    pub size_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WikiProgressEvent {
    pub current: u32,
    pub total: u32,
    pub module: String,
}

#[tauri::command]
pub async fn wiki_generate(
    app: AppHandle,
    state: State<'_, AppState>,
    request: WikiGenerateRequest,
) -> Result<WikiGenerateResult, String> {
    let with_index = request.with_index.unwrap_or(true);
    let enrich = request.enrich_with_llm.unwrap_or(false);
    let llm_config = if enrich {
        Some(crate::commands::chat::load_config_pub(&state).await)
    } else {
        None
    };
    let (graph, _idx, _fts, repo_path_str) = state.get_repo(None).await?;
    let repo_path = PathBuf::from(&repo_path_str);

    let out_dir = request
        .out_dir
        .map(PathBuf::from)
        .unwrap_or_else(|| repo_path.join("wiki"));
    std::fs::create_dir_all(&out_dir).map_err(|e| e.to_string())?;

    // ── Collect community → member ids ─────────────────────────
    let mut community_meta: BTreeMap<String, (String, Option<String>)> = BTreeMap::new(); // id → (name, label)
    for node in graph.iter_nodes() {
        if node.label == NodeLabel::Community {
            community_meta.insert(
                node.id.clone(),
                (
                    node.properties.name.clone(),
                    node.properties.heuristic_label.clone(),
                ),
            );
        }
    }

    let mut members_by_community: HashMap<String, Vec<String>> = HashMap::new();
    for rel in graph.iter_relationships() {
        if rel.rel_type == RelationshipType::MemberOf {
            members_by_community
                .entry(rel.target_id.clone())
                .or_default()
                .push(rel.source_id.clone());
        }
    }

    // ── Cross-community Calls (for See also section) ──────────
    let mut node_to_community: HashMap<String, String> = HashMap::new();
    for (cid, members) in &members_by_community {
        for m in members {
            node_to_community.insert(m.clone(), cid.clone());
        }
    }
    let mut cross_edges: HashMap<String, BTreeSet<String>> = HashMap::new();
    for rel in graph.iter_relationships() {
        if !matches!(
            rel.rel_type,
            RelationshipType::Calls | RelationshipType::Uses | RelationshipType::Imports
        ) {
            continue;
        }
        let (Some(s), Some(t)) = (
            node_to_community.get(&rel.source_id),
            node_to_community.get(&rel.target_id),
        ) else {
            continue;
        };
        if s != t {
            cross_edges.entry(s.clone()).or_default().insert(t.clone());
        }
    }

    // ── Generate pages ─────────────────────────────────────────
    let total = community_meta.len() as u32;
    let mut pages: Vec<WikiPage> = Vec::new();
    let mut used: HashSet<String> = HashSet::new();
    for (idx, (cid, (name, heuristic))) in (1_u32..).zip(community_meta.iter()) {
        let _ = app.emit(
            "wiki-progress",
            WikiProgressEvent {
                current: idx,
                total,
                module: name.clone(),
            },
        );

        let members = members_by_community.get(cid).cloned().unwrap_or_default();
        let base = sanitize_filename(heuristic.as_deref().unwrap_or(name));
        let mut filename = base.clone();
        let mut counter = 2u32;
        while used.contains(&filename) {
            filename = format!("{base}_{counter}");
            counter += 1;
        }
        used.insert(filename.clone());

        let page_path = out_dir.join(format!("{filename}.md"));

        // Optional LLM-generated overview paragraph. Skipped silently if
        // the LLM is misconfigured / unreachable so the wiki still ships.
        let mut overview: Option<String> = None;
        if let Some(cfg) = llm_config.as_ref() {
            let label = heuristic.as_deref().unwrap_or(name);
            let prompt = build_module_prompt(label, &members, &graph);
            if let Ok(text) = crate::commands::chat::call_llm_pub(
                cfg,
                &[
                    serde_json::json!({"role": "system", "content": format!("{}\n\n{}", PROMPT_CONTEXT_SAFETY, LLM_WIKI_SYSTEM)}),
                    serde_json::json!({"role": "user", "content": prompt}),
                ],
            )
            .await
            {
                let trimmed = text.trim().to_string();
                if !trimmed.is_empty() {
                    overview = Some(trimmed);
                }
            }
        }

        let md = render_page(
            heuristic.as_deref().unwrap_or(name),
            &members,
            &graph,
            cross_edges.get(cid),
            &community_meta,
            overview.as_deref(),
        );
        std::fs::write(&page_path, &md).map_err(|e| e.to_string())?;
        let size = std::fs::metadata(&page_path).map(|m| m.len()).unwrap_or(0);

        pages.push(WikiPage {
            module: heuristic.clone().unwrap_or_else(|| name.clone()),
            filename: format!("{filename}.md"),
            path: page_path.to_string_lossy().to_string(),
            member_count: members.len() as u32,
            size_bytes: size,
        });
    }

    // ── index.md ───────────────────────────────────────────────
    if with_index && !pages.is_empty() {
        let index_md = render_index(&pages);
        let index_path = out_dir.join("index.md");
        std::fs::write(&index_path, index_md).map_err(|e| e.to_string())?;
    }

    Ok(WikiGenerateResult {
        out_dir: out_dir.to_string_lossy().to_string(),
        total_files: pages.len() as u32 + if with_index { 1 } else { 0 },
        pages,
    })
}

// ─── Page rendering ─────────────────────────────────────────────

fn render_page(
    module_name: &str,
    member_ids: &[String],
    graph: &code_explorer_core::graph::KnowledgeGraph,
    cross_communities: Option<&BTreeSet<String>>,
    community_meta: &BTreeMap<String, (String, Option<String>)>,
    overview: Option<&str>,
) -> String {
    let mut md = format!("# {module_name}\n\n");

    // LLM-generated overview paragraph (when enrichment was requested).
    if let Some(text) = overview {
        md.push_str("## Overview\n\n");
        md.push_str(text);
        md.push_str("\n\n");
    }

    // Quick stats
    md.push_str(&format!("**{}** symbols\n\n", member_ids.len()));

    // Files in this module
    let mut files: BTreeSet<String> = BTreeSet::new();
    let mut entry_points: Vec<String> = Vec::new();
    let mut classes: BTreeMap<String, String> = BTreeMap::new(); // name → file
    let mut functions: BTreeMap<String, String> = BTreeMap::new();

    for mid in member_ids {
        let Some(node) = graph.get_node(mid) else {
            continue;
        };
        if !node.properties.file_path.is_empty() {
            files.insert(node.properties.file_path.clone());
        }
        if node.properties.entry_point_score.unwrap_or(0.0) > 0.5 {
            entry_points.push(format!(
                "{} ({})",
                node.properties.name, node.properties.file_path
            ));
        }
        match node.label {
            NodeLabel::Class | NodeLabel::Interface | NodeLabel::Struct => {
                classes
                    .entry(node.properties.name.clone())
                    .or_insert(node.properties.file_path.clone());
            }
            NodeLabel::Function | NodeLabel::Method => {
                functions
                    .entry(node.properties.name.clone())
                    .or_insert(node.properties.file_path.clone());
            }
            _ => {}
        }
    }

    if !entry_points.is_empty() {
        md.push_str("## Entry points\n\n");
        for ep in entry_points.iter().take(10) {
            md.push_str(&format!("- {ep}\n"));
        }
        md.push('\n');
    }

    if !files.is_empty() {
        md.push_str("## Files\n\n");
        for f in files.iter().take(30) {
            md.push_str(&format!("- `{f}`\n"));
        }
        md.push('\n');
    }

    if !classes.is_empty() {
        md.push_str("## Types\n\n");
        for (name, file) in classes.iter().take(20) {
            md.push_str(&format!("- **{name}** — `{file}`\n"));
        }
        md.push('\n');
    }

    if !functions.is_empty() {
        md.push_str("## Functions\n\n");
        for (name, file) in functions.iter().take(30) {
            md.push_str(&format!("- `{name}` — `{file}`\n"));
        }
        md.push('\n');
    }

    if let Some(crosses) = cross_communities {
        if !crosses.is_empty() {
            md.push_str("## See also\n\n");
            for cid in crosses {
                if let Some((name, heuristic)) = community_meta.get(cid) {
                    let label = heuristic.as_deref().unwrap_or(name);
                    md.push_str(&format!("- [{}]({}.md)\n", label, sanitize_filename(label)));
                }
            }
            md.push('\n');
        }
    }

    md.push_str("---\n");
    md.push_str("_Generated by Code Explorer._\n");
    md
}

fn render_index(pages: &[WikiPage]) -> String {
    let mut md = String::from("# Wiki Index\n\n");
    md.push_str(&format!("**{}** modules\n\n", pages.len()));
    md.push_str("| Module | Symbols | Page |\n|---|---:|---|\n");
    let mut sorted: Vec<&WikiPage> = pages.iter().collect();
    sorted.sort_by(|a, b| b.member_count.cmp(&a.member_count));
    for p in sorted {
        md.push_str(&format!(
            "| {} | {} | [{}]({}) |\n",
            p.module, p.member_count, p.filename, p.filename
        ));
    }
    md.push_str("\n---\n_Generated by Code Explorer._\n");
    md
}

// ─── LLM enrichment prompt ──────────────────────────────────────

const LLM_WIKI_SYSTEM: &str =
    "You write concise, factual one-paragraph descriptions of code modules. \
     Rules: 3-5 sentences, no marketing speak, no bullet lists, no headings. \
     Describe what the module does and how its files fit together. Reference \
     specific file/symbol names from the inputs — do not invent.";

fn build_module_prompt(
    label: &str,
    member_ids: &[String],
    graph: &code_explorer_core::graph::KnowledgeGraph,
) -> String {
    // Sample top key files + entry points to feed the LLM. Bound the size
    // hard so wikis with hundreds of modules stay affordable.
    let mut files: BTreeSet<String> = BTreeSet::new();
    let mut entry_points: Vec<String> = Vec::new();
    let mut classes: Vec<String> = Vec::new();
    let mut functions: Vec<String> = Vec::new();
    for mid in member_ids.iter().take(40) {
        let Some(node) = graph.get_node(mid) else {
            continue;
        };
        if !node.properties.file_path.is_empty() {
            files.insert(node.properties.file_path.clone());
        }
        if node.properties.entry_point_score.unwrap_or(0.0) > 0.5 {
            entry_points.push(node.properties.name.clone());
        }
        match node.label {
            NodeLabel::Class | NodeLabel::Interface | NodeLabel::Struct => {
                classes.push(node.properties.name.clone());
            }
            NodeLabel::Function | NodeLabel::Method => {
                functions.push(node.properties.name.clone());
            }
            _ => {}
        }
    }
    let mut p = format!(
        "Module: `{label}`\n\nFiles ({} shown):\n",
        files.len().min(10)
    );
    for f in files.iter().take(10) {
        p.push_str(&format!("- `{f}`\n"));
    }
    if !entry_points.is_empty() {
        p.push_str(&format!(
            "\nEntry points: {}\n",
            entry_points
                .iter()
                .take(8)
                .cloned()
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    if !classes.is_empty() {
        p.push_str(&format!(
            "\nKey types: {}\n",
            classes
                .iter()
                .take(10)
                .cloned()
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    if !functions.is_empty() {
        p.push_str(&format!(
            "\nKey functions: {}\n",
            functions
                .iter()
                .take(10)
                .cloned()
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    p.push_str("\nWrite the overview paragraph now.");
    p
}

fn sanitize_filename(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        if ch.is_ascii_alphanumeric() || ch == '_' || ch == '-' {
            out.push(ch.to_ascii_lowercase());
        } else if ch == ' ' || ch == '.' || ch == '/' || ch == ':' {
            out.push('_');
        }
    }
    if out.is_empty() {
        out.push_str("module");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_filename() {
        assert_eq!(sanitize_filename("Auth Service"), "auth_service");
        assert_eq!(sanitize_filename("foo/bar.baz"), "foo_bar_baz");
        assert_eq!(sanitize_filename("✨"), "module");
        assert_eq!(sanitize_filename("Already_OK-1"), "already_ok-1");
    }

    #[test]
    fn test_render_index_lists_pages() {
        let pages = vec![
            WikiPage {
                module: "Auth".into(),
                filename: "auth.md".into(),
                path: "/x/auth.md".into(),
                member_count: 12,
                size_bytes: 0,
            },
            WikiPage {
                module: "Payment".into(),
                filename: "payment.md".into(),
                path: "/x/payment.md".into(),
                member_count: 7,
                size_bytes: 0,
            },
        ];
        let md = render_index(&pages);
        assert!(md.contains("Auth"));
        assert!(md.contains("Payment"));
        // Sorted by member_count desc → Auth first.
        let auth_pos = md.find("Auth").unwrap();
        let pay_pos = md.find("Payment").unwrap();
        assert!(auth_pos < pay_pos);
    }
}

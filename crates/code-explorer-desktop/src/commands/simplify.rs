//! simplify — absorb Claude's `simplify` skill as a native capability.
//!
//! Examines a target (file, module/community, or whole repo when none given)
//! and surfaces:
//!   - high-complexity symbols
//!   - dead-code candidates
//!   - untraced symbols (high-complexity & untraced = priority)
//!   - LLM-detected smells already enriched on the graph
//!   - duplicate-name groups (potential consolidation)
//!
//! Then asks the LLM to translate signals into concrete refactor *moves*
//! (extract / delete / merge / inline / rename), each with a rationale +
//! confidence score. Falls back to a graph-only proposal list when the LLM
//! is unreachable so the tool stays useful offline.

use std::collections::HashMap;
use std::time::Instant;

use tauri::State;
use uuid::Uuid;

use code_explorer_core::graph::types::*;
use code_explorer_core::graph::KnowledgeGraph;
use code_explorer_core::llm::PROMPT_CONTEXT_SAFETY;

use crate::commands::chat;
use crate::state::AppState;
use crate::types::*;

#[tauri::command]
pub async fn simplify_run(
    state: State<'_, AppState>,
    request: SimplifyRequest,
) -> Result<SimplifyArtifact, String> {
    let t_start = Instant::now();
    let id = format!("sim_{}", Uuid::new_v4());
    let min_complexity = request.min_complexity.unwrap_or(8).max(1);

    let (graph, _idx, _fts, _repo_path) = state.get_repo(None).await?;

    // ── 1. Resolve scope ──────────────────────────────────────────
    let scope = resolve_scope(&graph, request.target.as_deref());
    let scope_label = scope.label.clone();

    // ── 2. Gather signals ─────────────────────────────────────────
    let signals = build_signals(&graph, &scope, min_complexity);

    // ── 3. Generate proposals (LLM with graph fallback) ───────────
    let config = chat::load_config_pub(&state).await;
    let user_prompt = build_prompt(&scope_label, &signals);
    let llm_md = chat::call_llm_pub(
        &config,
        &[
            serde_json::json!({"role": "system", "content": format!("{}\n\n{}", PROMPT_CONTEXT_SAFETY, SYSTEM_PROMPT)}),
            serde_json::json!({"role": "user", "content": user_prompt}),
        ],
    )
    .await
    .ok();

    let proposals = if let Some(md) = &llm_md {
        parse_proposals(md, &signals)
    } else {
        graph_only_proposals(&signals)
    };

    let markdown = render_markdown(&scope_label, &signals, &proposals);

    Ok(SimplifyArtifact {
        id,
        status: PlanStatus::Completed,
        signals,
        proposals,
        markdown,
        duration_ms: t_start.elapsed().as_millis() as u64,
    })
}

// ─── Scope resolution ───────────────────────────────────────────────

#[derive(Debug, Clone)]
struct Scope {
    label: String,
    /// Predicate over node ids to include in the scope.
    file_filter: Option<String>,
    community_filter: Option<String>,
}

fn resolve_scope(graph: &KnowledgeGraph, target: Option<&str>) -> Scope {
    let target = match target {
        Some(t) if !t.trim().is_empty() => t.trim(),
        _ => {
            // No target → whole repo, but flag the most-complex file as a hint.
            let mut counts: HashMap<String, u32> = HashMap::new();
            for n in graph.iter_nodes() {
                if let Some(c) = n.properties.complexity {
                    if !n.properties.file_path.is_empty() {
                        *counts.entry(n.properties.file_path.clone()).or_insert(0) += c;
                    }
                }
            }
            let hint = counts.into_iter().max_by_key(|(_, v)| *v).map(|(f, _)| f);
            return Scope {
                label: hint
                    .as_deref()
                    .map(|f| format!("whole repo (busiest file: {f})"))
                    .unwrap_or_else(|| "whole repo".into()),
                file_filter: None,
                community_filter: None,
            };
        }
    };

    // Disambiguate target: file path (if any node has matching file_path) → file
    // scope; community/module name (matches Community node) → community scope.
    let target_lower = target.to_lowercase();
    let is_file = graph
        .iter_nodes()
        .any(|n| n.properties.file_path.to_lowercase() == target_lower);
    if is_file {
        return Scope {
            label: format!("file `{target}`"),
            file_filter: Some(target.to_string()),
            community_filter: None,
        };
    }
    let is_community = graph.iter_nodes().any(|n| {
        n.label == NodeLabel::Community
            && (n.properties.name.to_lowercase() == target_lower
                || n.properties
                    .heuristic_label
                    .as_deref()
                    .map(|h| h.to_lowercase() == target_lower)
                    .unwrap_or(false))
    });
    if is_community {
        return Scope {
            label: format!("module `{target}`"),
            file_filter: None,
            community_filter: Some(target.to_string()),
        };
    }
    // Unknown target — search by name as a generic filter, scoped to its file.
    let by_name = graph
        .iter_nodes()
        .find(|n| n.properties.name.to_lowercase() == target_lower);
    if let Some(n) = by_name {
        if !n.properties.file_path.is_empty() {
            return Scope {
                label: format!("symbol `{target}` (in `{}`)", n.properties.file_path),
                file_filter: Some(n.properties.file_path.clone()),
                community_filter: None,
            };
        }
    }
    Scope {
        label: format!("target `{target}` (no graph match — using whole repo)"),
        file_filter: None,
        community_filter: None,
    }
}

fn matches_scope(node: &code_explorer_core::graph::types::GraphNode, scope: &Scope) -> bool {
    if let Some(f) = &scope.file_filter {
        return node.properties.file_path == *f;
    }
    if let Some(_c) = &scope.community_filter {
        // Approximate: node belongs to community if it has a MEMBER_OF edge.
        // We do the membership check at the caller side (cheaper there).
        return true;
    }
    true
}

// ─── Signal aggregation ─────────────────────────────────────────────

fn build_signals(graph: &KnowledgeGraph, scope: &Scope, min_complexity: u32) -> SimplifySignals {
    // Pre-build community membership map if we filter by community.
    let mut in_community: Option<std::collections::HashSet<String>> = None;
    if let Some(comm_name) = &scope.community_filter {
        let lower = comm_name.to_lowercase();
        let comm_id = graph
            .iter_nodes()
            .find(|n| {
                n.label == NodeLabel::Community
                    && (n.properties.name.to_lowercase() == lower
                        || n.properties
                            .heuristic_label
                            .as_deref()
                            .map(|h| h.to_lowercase() == lower)
                            .unwrap_or(false))
            })
            .map(|n| n.id.clone());
        if let Some(cid) = comm_id {
            let mut members = std::collections::HashSet::new();
            for rel in graph.iter_relationships() {
                if rel.rel_type == RelationshipType::MemberOf && rel.target_id == cid {
                    members.insert(rel.source_id.clone());
                }
            }
            in_community = Some(members);
        }
    }

    let mut s = SimplifySignals {
        scope: scope.label.clone(),
        ..Default::default()
    };

    let mut name_buckets: HashMap<String, Vec<String>> = HashMap::new(); // name → file paths
    let mut files_seen: std::collections::HashSet<String> = std::collections::HashSet::new();

    for node in graph.iter_nodes() {
        if !matches_scope(node, scope) {
            continue;
        }
        if let Some(members) = &in_community {
            if !members.contains(&node.id) {
                continue;
            }
        }

        s.total_symbols += 1;
        if !node.properties.file_path.is_empty()
            && files_seen.insert(node.properties.file_path.clone())
        {
            s.total_files += 1;
        }

        // Complexity above threshold.
        if let Some(c) = node.properties.complexity {
            if c >= min_complexity
                && matches!(
                    node.label,
                    NodeLabel::Function | NodeLabel::Method | NodeLabel::Constructor
                )
            {
                s.complex_symbols.push(ComplexSymbol {
                    name: node.properties.name.clone(),
                    file_path: node.properties.file_path.clone(),
                    complexity: c,
                    label: node.label.as_str().to_string(),
                });
            }
        }
        if node.properties.is_dead_candidate.unwrap_or(false) {
            s.dead_candidates.push(node.properties.name.clone());
        }
        if matches!(
            node.label,
            NodeLabel::Function | NodeLabel::Method | NodeLabel::Constructor
        ) && !node.properties.is_traced.unwrap_or(false)
        {
            s.untraced_symbols.push(node.properties.name.clone());
        }
        if let Some(smells) = &node.properties.llm_smells {
            for smell in smells {
                s.llm_smells
                    .push(format!("{}: {}", node.properties.name, smell));
            }
        }

        // Duplicate-name detection.
        if matches!(
            node.label,
            NodeLabel::Function | NodeLabel::Method | NodeLabel::Class
        ) && !node.properties.name.is_empty()
            && !node.properties.file_path.is_empty()
        {
            name_buckets
                .entry(node.properties.name.clone())
                .or_default()
                .push(node.properties.file_path.clone());
        }
    }

    s.complex_symbols
        .sort_by(|a, b| b.complexity.cmp(&a.complexity));
    s.complex_symbols.truncate(15);
    s.dead_candidates.sort();
    s.dead_candidates.dedup();
    s.dead_candidates.truncate(50);
    s.untraced_symbols.sort();
    s.untraced_symbols.dedup();
    s.untraced_symbols.truncate(50);
    s.llm_smells.truncate(30);

    for (name, mut files) in name_buckets {
        files.sort();
        files.dedup();
        if files.len() >= 2 {
            s.duplicate_groups.push(DuplicateGroup {
                name,
                occurrences: files.len() as u32,
                files,
            });
        }
    }
    s.duplicate_groups
        .sort_by(|a, b| b.occurrences.cmp(&a.occurrences));
    s.duplicate_groups.truncate(20);

    s
}

// ─── LLM prompting ──────────────────────────────────────────────────

const SYSTEM_PROMPT: &str =
    "You are the simplify role: read aggregated graph signals about a codebase \
section and propose concrete refactor moves.\n\n\
Rules:\n\
- Output Markdown with a single section `### Proposals`.\n\
- Each proposal is a line: `<kind>: <target> — <rationale>` where `<kind>` is one \
  of extract / delete / merge / inline / rename.\n\
- Only HIGH-VALUE moves. Skip stylistic nitpicks. Prefer deletes over rewrites \
  when a symbol is dead.\n\
- Every proposal must reference a symbol/file from the signals — no inventing.\n\
- Include a parenthetical confidence at the end: `(confidence: 0.85)`.";

fn build_prompt(scope: &str, s: &SimplifySignals) -> String {
    let mut p = format!("## Scope\n{scope}\n\n## Signals\n");
    p.push_str(&format!(
        "- {} files / {} symbols in scope\n",
        s.total_files, s.total_symbols
    ));
    if !s.complex_symbols.is_empty() {
        p.push_str("- Top-complexity symbols:\n");
        for c in s.complex_symbols.iter().take(10) {
            p.push_str(&format!(
                "  - `{}` (complexity {}) in `{}`\n",
                c.name, c.complexity, c.file_path
            ));
        }
    }
    if !s.dead_candidates.is_empty() {
        p.push_str(&format!(
            "- Dead candidates ({}): {}\n",
            s.dead_candidates.len(),
            s.dead_candidates
                .iter()
                .take(15)
                .cloned()
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    if !s.untraced_symbols.is_empty() {
        p.push_str(&format!(
            "- Untraced symbols: {} (top 10: {})\n",
            s.untraced_symbols.len(),
            s.untraced_symbols
                .iter()
                .take(10)
                .cloned()
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    if !s.llm_smells.is_empty() {
        p.push_str("- LLM-detected smells:\n");
        for sm in s.llm_smells.iter().take(8) {
            p.push_str(&format!("  - {sm}\n"));
        }
    }
    if !s.duplicate_groups.is_empty() {
        p.push_str("- Duplicate-name groups (potential consolidation):\n");
        for d in s.duplicate_groups.iter().take(8) {
            p.push_str(&format!(
                "  - `{}` ×{} ({})\n",
                d.name,
                d.occurrences,
                d.files.join(", ")
            ));
        }
    }
    p.push_str("\nProduce the proposals now.");
    p
}

fn parse_proposals(md: &str, s: &SimplifySignals) -> Vec<SimplifyProposal> {
    let mut out = Vec::new();
    let mut in_section = false;
    for line in md.lines() {
        let lower = line.to_ascii_lowercase();
        if lower.contains("### proposals") {
            in_section = true;
            continue;
        }
        if in_section && lower.starts_with("###") {
            break;
        }
        if !in_section {
            continue;
        }
        let trimmed = line.trim_start_matches(|c: char| {
            c.is_ascii_digit() || c == '.' || c == ' ' || c == '-' || c == '*'
        });
        if trimmed.is_empty() {
            continue;
        }
        if let Some((kind, rest)) = trimmed.split_once(':') {
            let kind = kind.trim().to_ascii_lowercase();
            if !matches!(
                kind.as_str(),
                "extract" | "delete" | "merge" | "inline" | "rename"
            ) {
                continue;
            }
            let (body, conf) = extract_confidence(rest.trim());
            let (target, rationale) =
                match body.split_once(" — ").or_else(|| body.split_once(" - ")) {
                    Some((t, r)) => (t.trim().to_string(), r.trim().to_string()),
                    None => (body.trim().to_string(), String::new()),
                };
            out.push(SimplifyProposal {
                kind,
                target,
                rationale,
                confidence: conf,
            });
        }
    }
    if out.is_empty() {
        return graph_only_proposals(s);
    }
    out
}

fn extract_confidence(s: &str) -> (String, f64) {
    // Looks for "(confidence: 0.NN)" at the end; falls back to 0.7.
    if let Some(idx) = s.rfind("(confidence:") {
        let head = s[..idx].trim_end().to_string();
        let tail = &s[idx..];
        let conf = tail
            .split(|c: char| !c.is_ascii_digit() && c != '.')
            .find_map(|tok| tok.parse::<f64>().ok())
            .filter(|v| *v >= 0.0 && *v <= 1.0)
            .unwrap_or(0.7);
        (head, conf)
    } else {
        (s.to_string(), 0.7)
    }
}

fn graph_only_proposals(s: &SimplifySignals) -> Vec<SimplifyProposal> {
    // Deterministic fallback so the tool ships value even without an LLM.
    let mut out = Vec::new();
    for d in s.dead_candidates.iter().take(10) {
        out.push(SimplifyProposal {
            kind: "delete".into(),
            target: d.clone(),
            rationale: "Flagged as dead-code candidate (no incoming calls).".into(),
            confidence: 0.8,
        });
    }
    for c in s.complex_symbols.iter().take(5) {
        out.push(SimplifyProposal {
            kind: "extract".into(),
            target: format!("{} (in {})", c.name, c.file_path),
            rationale: format!(
                "Complexity {} exceeds threshold — split into helper functions.",
                c.complexity
            ),
            confidence: 0.7,
        });
    }
    for d in s.duplicate_groups.iter().take(5) {
        out.push(SimplifyProposal {
            kind: "merge".into(),
            target: format!("{} ×{}", d.name, d.occurrences),
            rationale: format!(
                "Same name across {} files — investigate consolidation.",
                d.occurrences
            ),
            confidence: 0.6,
        });
    }
    out
}

// ─── Markdown rendering ─────────────────────────────────────────────

fn render_markdown(scope: &str, s: &SimplifySignals, props: &[SimplifyProposal]) -> String {
    let mut md = format!("# Simplify — {scope}\n\n");
    md.push_str(&format!(
        "**Scope**: {} files · {} symbols\n\n",
        s.total_files, s.total_symbols
    ));

    if !props.is_empty() {
        md.push_str("## Proposals\n\n");
        for (i, p) in props.iter().enumerate() {
            md.push_str(&format!(
                "{}. **{}** `{}` — {} *(confidence {:.2})*\n",
                i + 1,
                p.kind,
                p.target,
                p.rationale,
                p.confidence
            ));
        }
        md.push('\n');
    } else {
        md.push_str("_No proposals — code looks clean for this scope._\n\n");
    }

    if !s.complex_symbols.is_empty() {
        md.push_str("## Complexity hotspots\n\n");
        for c in s.complex_symbols.iter().take(10) {
            md.push_str(&format!(
                "- `{}` ({}) — complexity {} in `{}`\n",
                c.name, c.label, c.complexity, c.file_path
            ));
        }
        md.push('\n');
    }
    if !s.dead_candidates.is_empty() {
        md.push_str(&format!(
            "## Dead candidates ({})\n",
            s.dead_candidates.len()
        ));
        for d in s.dead_candidates.iter().take(20) {
            md.push_str(&format!("- `{d}`\n"));
        }
        md.push('\n');
    }
    if !s.duplicate_groups.is_empty() {
        md.push_str("## Duplicate names\n");
        for d in s.duplicate_groups.iter().take(10) {
            md.push_str(&format!("- `{}` ×{}\n", d.name, d.occurrences));
        }
        md.push('\n');
    }
    md
}

// ─── Tests ──────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_confidence_present() {
        let (body, conf) = extract_confidence("foo bar (confidence: 0.85)");
        assert_eq!(body, "foo bar");
        assert!((conf - 0.85).abs() < 0.001);
    }

    #[test]
    fn test_extract_confidence_missing() {
        let (body, conf) = extract_confidence("just some text");
        assert_eq!(body, "just some text");
        assert!((conf - 0.7).abs() < 0.001);
    }

    #[test]
    fn test_parse_proposals_recognizes_kinds() {
        let md = "### Proposals\n\
            1. delete: `foo` — unused (confidence: 0.9)\n\
            2. extract: `complexFn` — split logic (confidence: 0.8)\n\
            3. ignored: shouldn't be parsed\n";
        let s = SimplifySignals::default();
        let out = parse_proposals(md, &s);
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].kind, "delete");
        assert_eq!(out[1].kind, "extract");
    }

    #[test]
    fn test_graph_only_proposals_fallback() {
        let s = SimplifySignals {
            dead_candidates: vec!["foo".into(), "bar".into()],
            complex_symbols: vec![ComplexSymbol {
                name: "huge".into(),
                file_path: "src/big.rs".into(),
                complexity: 42,
                label: "Function".into(),
            }],
            ..SimplifySignals::default()
        };
        let out = graph_only_proposals(&s);
        assert!(out.iter().any(|p| p.kind == "delete"));
        assert!(out.iter().any(|p| p.kind == "extract"));
    }

    #[test]
    fn test_render_markdown_shows_scope() {
        let s = SimplifySignals {
            scope: "x".into(),
            total_files: 5,
            total_symbols: 50,
            ..Default::default()
        };
        let md = render_markdown("file `foo.rs`", &s, &[]);
        assert!(md.contains("Simplify"));
        assert!(md.contains("foo.rs"));
        assert!(md.contains("5 files"));
    }
}

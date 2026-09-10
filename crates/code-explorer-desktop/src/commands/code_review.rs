//! code-review — absorb Claude's `code-review` skill as a native capability.
//!
//! Takes the current uncommitted change set (or an explicit symbol list),
//! pre-computes objective graph-derived signals (blast radius, hotspots,
//! tracing gaps, dead-code flags), then asks the LLM for a confidence-
//! filtered review.
//!
//! The key design choice vs. vanilla `code-review`: instead of asking the
//! LLM to *find* the issues from raw grep results, we feed it pre-computed
//! structural evidence and ask it to *interpret*. Smaller models become
//! reliable because the heavy lifting is done by the graph.

use std::collections::HashSet;
use std::path::PathBuf;
use std::time::Instant;

use tauri::State;
use uuid::Uuid;

use code_explorer_core::graph::types::*;
use code_explorer_core::llm::PROMPT_CONTEXT_SAFETY;

use crate::commands::chat;
use crate::commands::feature_dev::parse_review_from_markdown_pub;
use crate::state::AppState;
use crate::types::*;

#[tauri::command]
pub async fn code_review_run(
    state: State<'_, AppState>,
    request: CodeReviewRequest,
) -> Result<CodeReviewArtifact, String> {
    let t_start = Instant::now();
    let artifact_id = format!("cr_{}", Uuid::new_v4());
    let min_confidence = request.min_confidence.unwrap_or(0.8).clamp(0.0, 1.0);

    let (graph, _indexes, _fts, repo_path_str) = state.get_repo(None).await?;
    let repo_path = PathBuf::from(&repo_path_str);

    // ── Derive scope ────────────────────────────────────────────────
    let changed_files = if request.target_symbols.is_empty() {
        collect_changed_files(&repo_path)
    } else {
        Vec::new()
    };

    // Build the set of changed symbol ids by intersecting file set with graph
    // nodes, OR by resolving names from the explicit list.
    let mut changed_symbol_ids: HashSet<String> = HashSet::new();
    let mut changed_symbol_names: Vec<String> = Vec::new();

    if !request.target_symbols.is_empty() {
        let lower_targets: HashSet<String> = request
            .target_symbols
            .iter()
            .map(|t| t.to_lowercase())
            .collect();
        for node in graph.iter_nodes() {
            if request.target_symbols.contains(&node.id)
                || lower_targets.contains(&node.properties.name.to_lowercase())
            {
                changed_symbol_ids.insert(node.id.clone());
                changed_symbol_names.push(node.properties.name.clone());
            }
        }
    } else {
        // File-driven: every node whose file_path is in changed_files.
        let changed_set: HashSet<&str> = changed_files.iter().map(|s| s.as_str()).collect();
        for node in graph.iter_nodes() {
            if changed_set.contains(node.properties.file_path.as_str())
                && matches!(
                    node.label,
                    NodeLabel::Function
                        | NodeLabel::Method
                        | NodeLabel::Class
                        | NodeLabel::Interface
                        | NodeLabel::Controller
                        | NodeLabel::ControllerAction
                        | NodeLabel::Service
                )
            {
                changed_symbol_ids.insert(node.id.clone());
                changed_symbol_names.push(node.properties.name.clone());
            }
        }
    }
    changed_symbol_names.sort();
    changed_symbol_names.dedup();

    // ── Pre-compute graph signals ──────────────────────────────────
    let mut signals = CodeReviewSignals {
        changed_files: changed_files.clone(),
        changed_symbols: changed_symbol_names.clone(),
        ..Default::default()
    };

    // Untraced + dead-code flags from node properties.
    for id in &changed_symbol_ids {
        if let Some(n) = graph.get_node(id) {
            if !n.properties.is_traced.unwrap_or(false) {
                signals.untraced_symbols.push(n.properties.name.clone());
            }
            if n.properties.is_dead_candidate.unwrap_or(false) {
                signals.dead_candidates.push(n.properties.name.clone());
            }
        }
    }
    signals.untraced_symbols.sort();
    signals.untraced_symbols.dedup();
    signals.dead_candidates.sort();
    signals.dead_candidates.dedup();

    // BFS upstream to count transitively affected nodes + reach Process nodes.
    let (affected_count, affected_processes) = compute_upstream_reach(&graph, &changed_symbol_ids);
    signals.affected_count = affected_count as u32;
    signals.affected_processes = affected_processes;

    // Hotspot intersection: run the hotspots computation and intersect.
    signals.hotspot_files = compute_hotspot_intersection(&repo_path, &changed_files);

    // Risk classification.
    signals.risk_level = classify_risk_str(
        changed_symbol_names.len(),
        affected_count,
        signals.affected_processes.len(),
    )
    .to_string();

    // Fast-exit: nothing to review.
    if changed_symbol_ids.is_empty() && changed_files.is_empty() {
        let review = Review {
            issues: Vec::new(),
            predicted_impact: Some("No uncommitted changes detected.".into()),
            verdict: "ready".into(),
        };
        let md = render_markdown("No changes", &signals, &review);
        return Ok(CodeReviewArtifact {
            id: artifact_id,
            scope_summary: "No uncommitted changes".into(),
            status: PlanStatus::Completed,
            signals,
            review,
            markdown: md,
            duration_ms: t_start.elapsed().as_millis() as u64,
        });
    }

    // ── LLM review ─────────────────────────────────────────────────
    let config = chat::load_config_pub(&state).await;
    let scope_summary = if !request.target_symbols.is_empty() {
        format!("{} explicit symbols", request.target_symbols.len())
    } else {
        format!(
            "{} files, {} symbols",
            changed_files.len(),
            changed_symbol_names.len()
        )
    };

    let user_prompt = build_reviewer_prompt(&signals, min_confidence);
    let llm_md = chat::call_llm_pub(
        &config,
        &[
            serde_json::json!({"role": "system", "content": format!("{}\n\n{}", PROMPT_CONTEXT_SAFETY, REVIEWER_SYSTEM_PROMPT)}),
            serde_json::json!({"role": "user", "content": user_prompt}),
        ],
    )
    .await
    // Fall back to a graph-only review if the LLM isn't reachable.
    .unwrap_or_else(|_| graph_only_review_md(&signals));

    let mut review = parse_review_from_markdown_pub(&llm_md);

    // Enforce the confidence filter + severity filter on the final review.
    review.issues.retain(|i| {
        i.confidence >= min_confidence && (request.include_all_severities || i.severity == "high")
    });

    let markdown = render_markdown(&scope_summary, &signals, &review);

    Ok(CodeReviewArtifact {
        id: artifact_id,
        scope_summary,
        status: PlanStatus::Completed,
        signals,
        review,
        markdown,
        duration_ms: t_start.elapsed().as_millis() as u64,
    })
}

// ─── Helpers ────────────────────────────────────────────────────────

const REVIEWER_SYSTEM_PROMPT: &str = "You are the code-reviewer role for a pre-commit review. \
You receive GRAPH-DERIVED SIGNALS (not raw diffs) about a change set. Your \
job is to translate those signals into actionable issues.\n\n\
Rules:\n\
- Only high-confidence issues (≥ 0.8 unless caller allows lower).\n\
- Every issue must be grounded in the signals you're given — no inventing.\n\
- Severity: high = breaks correctness/security; medium = maintainability; low = style. \
  Default mode: HIGH ONLY.\n\
- Output strict Markdown with sections: `### Verdict`, `### High-confidence issues`, `### Predicted impact`.\n\
- Issues format: `1. **<severity>/<confidence>**: <title> — <detail>`.\n\
- If signals show no red flags, say so explicitly with verdict=ready.";

fn build_reviewer_prompt(signals: &CodeReviewSignals, min_confidence: f64) -> String {
    let mut p = String::new();
    p.push_str("## Graph signals for the change set\n\n");

    p.push_str(&format!(
        "- Changed files: {}\n- Changed symbols: {}\n- Transitive affected count: {}\n- Risk level (heuristic): {}\n",
        signals.changed_files.len(),
        signals.changed_symbols.len(),
        signals.affected_count,
        signals.risk_level
    ));

    if !signals.affected_processes.is_empty() {
        p.push_str(&format!(
            "- Execution processes touched: {}\n",
            signals.affected_processes.join(", ")
        ));
    }
    if !signals.hotspot_files.is_empty() {
        p.push_str(&format!(
            "- Hotspot files in this change set (historically volatile): {}\n",
            signals.hotspot_files.join(", ")
        ));
    }
    if !signals.untraced_symbols.is_empty() {
        p.push_str(&format!(
            "- Symbols with NO tracing coverage: {}\n",
            signals.untraced_symbols.join(", ")
        ));
    }
    if !signals.dead_candidates.is_empty() {
        p.push_str(&format!(
            "- Symbols already flagged as dead candidates: {}\n",
            signals.dead_candidates.join(", ")
        ));
    }

    p.push_str(&format!(
        "\nApply minimum confidence: {min_confidence:.2}.\n\n\
         Produce the review now."
    ));
    p
}

fn graph_only_review_md(signals: &CodeReviewSignals) -> String {
    // Fallback when the LLM is unreachable. Keeps the tool useful offline
    // by turning each signal into a canned issue with clear provenance.
    let mut md = String::from("### Verdict\n");
    let has_any = !signals.affected_processes.is_empty()
        || !signals.hotspot_files.is_empty()
        || !signals.untraced_symbols.is_empty()
        || !signals.dead_candidates.is_empty();
    md.push_str(if signals.risk_level == "high" {
        "blocked\n"
    } else if has_any {
        "needs_revisions\n"
    } else {
        "ready\n"
    });

    md.push_str("\n### High-confidence issues\n");
    let mut n = 0;
    for p in &signals.affected_processes {
        n += 1;
        md.push_str(&format!(
            "{n}. **high/0.85**: Touches execution process `{p}` — Changes in this set reach a named execution flow. Review end-to-end.\n"
        ));
    }
    for f in &signals.hotspot_files {
        n += 1;
        md.push_str(&format!(
            "{n}. **high/0.82**: Hotspot file `{f}` — Historically volatile. Add tests around changes here.\n"
        ));
    }
    for s in &signals.untraced_symbols {
        n += 1;
        md.push_str(&format!(
            "{n}. **high/0.80**: No tracing on `{s}` — Behavior changes will be invisible in prod logs.\n"
        ));
    }
    for s in &signals.dead_candidates {
        n += 1;
        md.push_str(&format!(
            "{n}. **medium/0.80**: Editing dead-code candidate `{s}` — Verify the symbol is truly used before investing time.\n"
        ));
    }
    if n == 0 {
        md.push_str("_No red flags from graph signals._\n");
    }

    md.push_str("\n### Predicted impact\n");
    md.push_str(&format!(
        "Approx {} transitively affected symbol(s); risk level heuristic: {}.\n",
        signals.affected_count, signals.risk_level
    ));
    md
}

fn render_markdown(scope: &str, signals: &CodeReviewSignals, review: &Review) -> String {
    let mut md = format!("# Code Review — {scope}\n\n");
    md.push_str(&format!(
        "**Risk**: {} · **Affected**: {} symbol(s) · **Verdict**: {}\n\n",
        signals.risk_level, signals.affected_count, review.verdict
    ));

    if !review.issues.is_empty() {
        md.push_str("## Issues\n\n");
        for (i, issue) in review.issues.iter().enumerate() {
            md.push_str(&format!(
                "{}. **{}/{:.2}** — **{}**\n   {}\n",
                i + 1,
                issue.severity,
                issue.confidence,
                issue.title,
                issue.detail
            ));
        }
        md.push('\n');
    }

    if !signals.hotspot_files.is_empty() {
        md.push_str("## Hotspot files in this change set\n");
        for f in &signals.hotspot_files {
            md.push_str(&format!("- `{f}`\n"));
        }
        md.push('\n');
    }
    if !signals.untraced_symbols.is_empty() {
        md.push_str("## Symbols without tracing\n");
        for s in &signals.untraced_symbols {
            md.push_str(&format!("- `{s}`\n"));
        }
        md.push('\n');
    }
    if let Some(p) = &review.predicted_impact {
        md.push_str(&format!("## Predicted impact\n{p}\n"));
    }
    md
}

fn compute_upstream_reach(
    graph: &code_explorer_core::graph::KnowledgeGraph,
    seeds: &HashSet<String>,
) -> (usize, Vec<String>) {
    let want_rel = |rt: RelationshipType| {
        matches!(
            rt,
            RelationshipType::Calls
                | RelationshipType::CallsAction
                | RelationshipType::CallsService
                | RelationshipType::Imports
                | RelationshipType::Uses
                | RelationshipType::DependsOn
                | RelationshipType::Inherits
                | RelationshipType::Implements
                | RelationshipType::Extends
                | RelationshipType::Overrides
                | RelationshipType::RendersView
                | RelationshipType::HandlesRoute
                | RelationshipType::Fetches
                | RelationshipType::MapsToEntity
        )
    };
    let mut reverse: std::collections::HashMap<String, Vec<String>> =
        std::collections::HashMap::new();
    let mut step_in_process: std::collections::HashMap<String, Vec<String>> =
        std::collections::HashMap::new();
    for rel in graph.iter_relationships() {
        if want_rel(rel.rel_type) {
            reverse
                .entry(rel.target_id.clone())
                .or_default()
                .push(rel.source_id.clone());
        }
        if rel.rel_type == RelationshipType::StepInProcess {
            step_in_process
                .entry(rel.source_id.clone())
                .or_default()
                .push(rel.target_id.clone());
        }
    }

    let mut affected: HashSet<String> = seeds.clone();
    let mut queue: std::collections::VecDeque<(String, usize)> =
        seeds.iter().map(|id| (id.clone(), 0usize)).collect();
    let max_depth = 3usize;
    while let Some((node, depth)) = queue.pop_front() {
        if depth >= max_depth {
            continue;
        }
        if let Some(ns) = reverse.get(&node) {
            for n in ns {
                if affected.insert(n.clone()) {
                    queue.push_back((n.clone(), depth + 1));
                }
            }
        }
    }

    let mut proc_names: HashSet<String> = HashSet::new();
    for id in &affected {
        if let Some(pids) = step_in_process.get(id) {
            for pid in pids {
                if let Some(p) = graph.get_node(pid) {
                    proc_names.insert(p.properties.name.clone());
                }
            }
        }
    }
    let mut proc_vec: Vec<String> = proc_names.into_iter().collect();
    proc_vec.sort();

    (affected.len().saturating_sub(seeds.len()), proc_vec)
}

fn compute_hotspot_intersection(
    repo_path: &std::path::Path,
    changed_files: &[String],
) -> Vec<String> {
    // Call the same hotspots entry point the CLI uses. Falls back silently
    // on any error so a missing .git doesn't poison the review.
    let changed: HashSet<&str> = changed_files.iter().map(|s| s.as_str()).collect();
    match code_explorer_git::hotspots::analyze_hotspots(repo_path, 90) {
        Ok(hs) => hs
            .into_iter()
            .take(30)
            .filter(|h| changed.contains(h.path.as_str()))
            .map(|h| h.path)
            .collect(),
        Err(_) => Vec::new(),
    }
}

fn collect_changed_files(repo_path: &std::path::Path) -> Vec<String> {
    let modified = std::process::Command::new("git")
        .args(["diff", "--name-only", "HEAD"])
        .current_dir(repo_path)
        .output();
    let mut files: Vec<String> = match modified {
        Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout)
            .lines()
            .map(String::from)
            .filter(|s| !s.is_empty())
            .collect(),
        _ => Vec::new(),
    };
    let untracked = std::process::Command::new("git")
        .args(["ls-files", "--others", "--exclude-standard"])
        .current_dir(repo_path)
        .output();
    if let Ok(o) = untracked {
        if o.status.success() {
            for line in String::from_utf8_lossy(&o.stdout).lines() {
                if !line.is_empty() {
                    files.push(line.to_string());
                }
            }
        }
    }
    files
}

fn classify_risk_str(direct: usize, transitive: usize, processes: usize) -> &'static str {
    if processes >= 2 || transitive >= 20 {
        "high"
    } else if processes >= 1 || transitive >= 5 || direct >= 3 {
        "medium"
    } else if direct > 0 {
        "low"
    } else {
        "none"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_classify_risk() {
        assert_eq!(classify_risk_str(0, 0, 0), "none");
        assert_eq!(classify_risk_str(1, 0, 0), "low");
        assert_eq!(classify_risk_str(3, 0, 0), "medium");
        assert_eq!(classify_risk_str(0, 0, 2), "high");
        assert_eq!(classify_risk_str(0, 25, 0), "high");
    }

    #[test]
    fn test_graph_only_review_happy_path() {
        let signals = CodeReviewSignals {
            risk_level: "low".into(),
            ..CodeReviewSignals::default()
        };
        let md = graph_only_review_md(&signals);
        assert!(md.contains("ready"));
    }

    #[test]
    fn test_graph_only_review_with_hotspots() {
        let signals = CodeReviewSignals {
            risk_level: "medium".into(),
            hotspot_files: vec!["src/hot.rs".into()],
            ..CodeReviewSignals::default()
        };
        let md = graph_only_review_md(&signals);
        assert!(md.contains("needs_revisions"));
        assert!(md.contains("src/hot.rs"));
    }

    #[test]
    fn test_render_markdown_contains_core_sections() {
        let signals = CodeReviewSignals {
            risk_level: "low".into(),
            affected_count: 3,
            ..Default::default()
        };
        let review = Review {
            verdict: "ready".into(),
            issues: vec![],
            predicted_impact: Some("minor".into()),
        };
        let md = render_markdown("scope", &signals, &review);
        assert!(md.contains("Code Review"));
        assert!(md.contains("Verdict**: ready"));
        assert!(md.contains("Predicted impact"));
    }
}

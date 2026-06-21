//! feature-dev — absorb Claude's feature-dev skill as a native Code Explorer capability.
//!
//! Runs a three-phase pipeline on a feature description:
//!
//! 1. **Explorer** — determine what the codebase currently does in the area
//!    affected by the feature. Deterministic graph traversal feeds the LLM.
//! 2. **Architect** — produce an implementation blueprint (files to create,
//!    files to modify, component design, build sequence). LLM-driven, with
//!    the explorer output as grounding.
//! 3. **Reviewer** — stress-test the blueprint against the existing graph
//!    (impact, conventions, dead code). Returns high-confidence issues only.
//!
//! Each phase emits a `feature-dev-phase` event on start/end and a
//! `feature-dev-section` event once its artifact section is ready. The UI
//! streams them into the ArtifactPanel.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::Instant;

use tauri::{AppHandle, Emitter, State};
use uuid::Uuid;

use code_explorer_core::graph::types::*;
use code_explorer_core::graph::KnowledgeGraph;
use code_explorer_core::llm::PROMPT_CONTEXT_SAFETY;
use code_explorer_db::inmemory::cypher::GraphIndexes;
use code_explorer_db::inmemory::fts::FtsIndex;

use crate::commands::chat;
use crate::state::AppState;
use crate::types::*;

// ─── Entry-point Tauri command ──────────────────────────────────────

#[tauri::command]
pub async fn feature_dev_run(
    app: AppHandle,
    state: State<'_, AppState>,
    request: FeatureDevRequest,
) -> Result<FeatureDevArtifact, String> {
    let artifact_id = format!("fd_{}", Uuid::new_v4());
    let feature = request.feature_description.trim().to_string();
    if feature.is_empty() {
        return Err("feature_description is required".to_string());
    }

    let (graph, indexes, fts_index, repo_path_str) = state.get_repo(None).await?;
    let repo_path = PathBuf::from(&repo_path_str);

    let mut artifact = FeatureDevArtifact {
        id: artifact_id.clone(),
        feature_description: feature.clone(),
        sections: Vec::new(),
        status: PlanStatus::Running,
        summary: None,
    };

    // ── Phase 1: Explorer ──────────────────────────────────────────
    emit_phase(
        &app,
        &artifact_id,
        FeatureDevPhase::Explorer,
        PhaseStatus::Running,
        None,
    );
    let t0 = Instant::now();
    let surface = match run_explorer(&feature, &graph, &indexes, &fts_index) {
        Ok(s) => s,
        Err(e) => {
            emit_phase(
                &app,
                &artifact_id,
                FeatureDevPhase::Explorer,
                PhaseStatus::Failed,
                Some(e.clone()),
            );
            artifact.status = PlanStatus::Failed;
            return Err(e);
        }
    };
    let surface_md = render_surface_markdown(&surface);
    let explorer_section = FeatureDevSection {
        phase: FeatureDevPhase::Explorer,
        title: "Surface analysis".to_string(),
        markdown: surface_md,
        surface: Some(surface.clone()),
        blueprint: None,
        review: None,
        duration_ms: t0.elapsed().as_millis() as u64,
    };
    emit_section(&app, &artifact_id, &explorer_section);
    emit_phase(
        &app,
        &artifact_id,
        FeatureDevPhase::Explorer,
        PhaseStatus::Completed,
        None,
    );
    artifact.sections.push(explorer_section);

    if request.explorer_only {
        artifact.status = PlanStatus::Completed;
        return Ok(artifact);
    }

    // ── Phase 2: Architect ─────────────────────────────────────────
    emit_phase(
        &app,
        &artifact_id,
        FeatureDevPhase::Architect,
        PhaseStatus::Running,
        None,
    );
    let t1 = Instant::now();
    let config = chat::load_config_pub(&state).await;
    let blueprint_md = match run_architect(&feature, &surface, &graph, &repo_path, &config).await {
        Ok(md) => md,
        Err(e) => {
            emit_phase(
                &app,
                &artifact_id,
                FeatureDevPhase::Architect,
                PhaseStatus::Failed,
                Some(e.clone()),
            );
            artifact.status = PlanStatus::Failed;
            return Err(e);
        }
    };
    let blueprint = parse_blueprint_from_markdown(&blueprint_md);
    let architect_section = FeatureDevSection {
        phase: FeatureDevPhase::Architect,
        title: "Implementation blueprint".to_string(),
        markdown: blueprint_md,
        surface: None,
        blueprint: Some(blueprint.clone()),
        review: None,
        duration_ms: t1.elapsed().as_millis() as u64,
    };
    emit_section(&app, &artifact_id, &architect_section);
    emit_phase(
        &app,
        &artifact_id,
        FeatureDevPhase::Architect,
        PhaseStatus::Completed,
        None,
    );
    artifact.sections.push(architect_section);

    // ── Phase 3: Reviewer ──────────────────────────────────────────
    emit_phase(
        &app,
        &artifact_id,
        FeatureDevPhase::Reviewer,
        PhaseStatus::Running,
        None,
    );
    let t2 = Instant::now();
    let review_md =
        match run_reviewer(&feature, &surface, &blueprint, &graph, &indexes, &config).await {
            Ok(md) => md,
            Err(e) => {
                emit_phase(
                    &app,
                    &artifact_id,
                    FeatureDevPhase::Reviewer,
                    PhaseStatus::Failed,
                    Some(e.clone()),
                );
                artifact.status = PlanStatus::Failed;
                return Err(e);
            }
        };
    let review = parse_review_from_markdown(&review_md);
    let reviewer_section = FeatureDevSection {
        phase: FeatureDevPhase::Reviewer,
        title: "Pre-implementation review".to_string(),
        markdown: review_md,
        surface: None,
        blueprint: None,
        review: Some(review),
        duration_ms: t2.elapsed().as_millis() as u64,
    };
    emit_section(&app, &artifact_id, &reviewer_section);
    emit_phase(
        &app,
        &artifact_id,
        FeatureDevPhase::Reviewer,
        PhaseStatus::Completed,
        None,
    );
    artifact.sections.push(reviewer_section);

    artifact.status = PlanStatus::Completed;
    Ok(artifact)
}

// ─── Phase 1: Explorer (deterministic, graph-driven) ────────────────

/// Explorer phase: find what modules, entry points, layers, and key files
/// are relevant to the feature description. No LLM — pure graph traversal
/// so the output is reproducible.
fn run_explorer(
    feature: &str,
    graph: &KnowledgeGraph,
    _indexes: &GraphIndexes,
    fts_index: &FtsIndex,
) -> Result<SurfaceAnalysis, String> {
    let query = feature.to_string();
    let raw_hits = fts_index.search(graph, &query, None, 30);

    // Score communities and layers by hit density.
    let mut module_scores: HashMap<String, f64> = HashMap::new();
    let mut layer_set: HashSet<String> = HashSet::new();
    let mut key_files_seen: HashSet<String> = HashSet::new();
    let mut key_files: Vec<String> = Vec::new();
    let mut entry_points: Vec<String> = Vec::new();

    for hit in &raw_hits {
        let Some(node) = graph.get_node(&hit.node_id) else {
            continue;
        };

        // Aggregate file paths.
        let fp = &node.properties.file_path;
        if !fp.is_empty() && key_files_seen.insert(fp.clone()) {
            key_files.push(fp.clone());
        }

        // Layer detection via layer_type property.
        if let Some(layer) = &node.properties.layer_type {
            layer_set.insert(layer.clone());
        }

        // Entry-point collection (nodes explicitly marked).
        if let Some(score) = node.properties.entry_point_score {
            if score > 0.5 {
                entry_points.push(format!(
                    "{} ({})",
                    node.properties.name,
                    node.label.as_str()
                ));
            }
        }

        // Walk MEMBER_OF outgoing to identify the community this node belongs to.
        for rel in graph.iter_relationships() {
            if rel.source_id == hit.node_id && rel.rel_type == RelationshipType::MemberOf {
                if let Some(community) = graph.get_node(&rel.target_id) {
                    let name = community.properties.name.clone();
                    *module_scores.entry(name).or_insert(0.0) += hit.score;
                }
            }
        }
    }

    // Sort modules by cumulative hit score.
    let mut modules: Vec<(String, f64)> = module_scores.into_iter().collect();
    modules.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    let modules: Vec<String> = modules.into_iter().take(5).map(|(n, _)| n).collect();

    // Deduplicate entry_points, cap at 10.
    let mut ep_seen: HashSet<String> = HashSet::new();
    entry_points.retain(|e| ep_seen.insert(e.clone()));
    entry_points.truncate(10);

    // Cap key_files at 15.
    key_files.truncate(15);

    let mut layers: Vec<String> = layer_set.into_iter().collect();
    layers.sort();

    Ok(SurfaceAnalysis {
        modules,
        entry_points,
        layers,
        key_files,
    })
}

fn render_surface_markdown(surface: &SurfaceAnalysis) -> String {
    let mut md = String::new();
    md.push_str("### Likely affected modules\n");
    if surface.modules.is_empty() {
        md.push_str("_No modules matched — the feature may introduce a new area._\n");
    } else {
        for m in &surface.modules {
            md.push_str(&format!("- **{}**\n", m));
        }
    }
    md.push_str("\n### Architecture layers detected\n");
    if surface.layers.is_empty() {
        md.push_str("_No explicit layer annotations found._\n");
    } else {
        md.push_str(&format!("{}\n", surface.layers.join(" → ")));
    }
    md.push_str("\n### Existing entry points\n");
    if surface.entry_points.is_empty() {
        md.push_str("_None above threshold._\n");
    } else {
        for ep in &surface.entry_points {
            md.push_str(&format!("- {}\n", ep));
        }
    }
    md.push_str("\n### Key files\n");
    if surface.key_files.is_empty() {
        md.push_str("_None._\n");
    } else {
        for f in &surface.key_files {
            md.push_str(&format!("- `{}`\n", f));
        }
    }
    md
}

// ─── Phase 2: Architect (LLM-driven, grounded by explorer) ──────────

async fn run_architect(
    feature: &str,
    surface: &SurfaceAnalysis,
    graph: &KnowledgeGraph,
    _repo_path: &Path,
    config: &ChatConfig,
) -> Result<String, String> {
    // Build grounding: serialize the surface + a compact conventions hint.
    let mut context = String::new();
    context.push_str("## Existing codebase surface\n\n");
    context.push_str(&render_surface_markdown(surface));

    // Add a conventions hint: extension distribution of the key files,
    // top-level directory structure. Cheap signal, high value.
    let ext_counts = extension_distribution(&surface.key_files);
    if !ext_counts.is_empty() {
        context.push_str("\n\n## Convention hints\n");
        for (ext, count) in ext_counts.iter().take(5) {
            context.push_str(&format!("- `.{}` ({} files)\n", ext, count));
        }
    }

    // Name collisions: detect whether the feature mentions identifiers
    // that already exist as symbols. Helps the architect pick names.
    let collision_hint = detect_name_collisions(feature, graph);
    if !collision_hint.is_empty() {
        context.push_str("\n\n## Name collisions to avoid\n");
        for c in &collision_hint {
            context.push_str(&format!("- `{}` already exists\n", c));
        }
    }

    let system = ARCHITECT_SYSTEM_PROMPT;
    let user = format!(
        "{}\n\n## Feature to design\n\n{}\n\n\
         Produce the blueprint as strict Markdown with these sections, in order, \
         following the Sample v1.1 SFD template (Besoin → Exigences → Modèle → \
         **Algorithmes** → Diagrammes — §4 Algorithmes is mandatory when the feature \
         touches a calculation, transformation, or multi-step process):\n\n\
         - `### 1. Expression du besoin` (one-paragraph restatement of the goal in \
           business terms — no implementation jargon)\n\
         - `### 2. Exigences` (bullet list of functional + non-functional requirements \
           extracted from the feature description)\n\
         - `### 3. Modèle de données` (entities or DTOs touched, with the fields the \
           feature reads or writes — anchor on existing graph symbols where possible)\n\
         - `### 4. Algorithmes` (when the feature involves a calculation or workflow: \
           structured pseudo-code in a fenced ```text block, then a Mermaid \
           flowchart in a ```mermaid block tying the steps together. Skip this \
           section ONLY for trivial CRUD with no business logic — say so explicitly \
           if you do.)\n\
         - `### 5. Diagrammes` (Mermaid sequence or component diagram showing how \
           the new pieces interact with the existing surface)\n\
         - `### Files to create` (bullet list of `path` — purpose)\n\
         - `### Files to modify` (bullet list of `path` — nature of change)\n\
         - `### Build sequence` (numbered steps, in order — first the data model, \
           then the algorithm, then wiring, then UI)",
        context, feature
    );

    call_role_llm(config, system, &user).await
}

const ARCHITECT_SYSTEM_PROMPT: &str =
    "You are the code-architect role in a three-stage feature development pipeline. \
Your job: design an implementation blueprint that respects the existing \
codebase's conventions AND follows the Sample v1.1 SFD methodology (Besoin → \
Exigences → Modèle → Algorithmes → Diagrammes). The §4 Algorithmes section \
is the load-bearing one for traceability — it must contain pseudo-code that \
a developer or auditor can follow without reading the source.\n\n\
Rules:\n\
- Match existing conventions (file layout, naming, extension style).\n\
- Prefer modifying existing files over creating new ones when possible.\n\
- Every file path must sit in a plausible location given the surface analysis.\n\
- Never invent symbols or APIs — only reference things present in the surface.\n\
- Pseudo-code in §4 must be language-agnostic, structured (clear blocks, \
  loops, conditionals), and reference the entities defined in §3.\n\
- Mermaid flowcharts use the topology-only style: nodes labelled with the \
  step name, edges labelled with the trigger condition. No internal code.\n\
- Produce a build sequence that can be followed step-by-step by a developer.\n\
- Do NOT write target-language source code. Output Markdown only.";

// ─── Phase 3: Reviewer (LLM-driven + graph validation) ──────────────

async fn run_reviewer(
    feature: &str,
    surface: &SurfaceAnalysis,
    blueprint: &Blueprint,
    graph: &KnowledgeGraph,
    _indexes: &GraphIndexes,
    config: &ChatConfig,
) -> Result<String, String> {
    // Validate the blueprint against the graph first — this is objective
    // data the LLM must honor.
    let mut graph_signals = String::new();
    graph_signals.push_str("## Graph-side validation of blueprint\n\n");

    let existing_names: HashSet<String> = graph
        .iter_nodes()
        .map(|n| n.properties.name.to_lowercase())
        .collect();

    let mut collisions: Vec<String> = Vec::new();
    for fp in &blueprint.files_to_create {
        // Check if any symbol with the *file stem* already exists as a symbol.
        if let Some(stem) = Path::new(&fp.path).file_stem().and_then(|s| s.to_str()) {
            if existing_names.contains(&stem.to_lowercase()) {
                collisions.push(format!(
                    "New file `{}` has the same stem as an existing symbol",
                    fp.path
                ));
            }
        }
    }
    if collisions.is_empty() {
        graph_signals.push_str("- No obvious name collisions detected.\n");
    } else {
        for c in &collisions {
            graph_signals.push_str(&format!("- {}\n", c));
        }
    }

    // Coverage: do the files_to_modify have tracing/tests today?
    let mut untraced_modified: Vec<String> = Vec::new();
    for fp in &blueprint.files_to_modify {
        let has_trace = graph
            .iter_nodes()
            .any(|n| n.properties.file_path == fp.path && n.properties.is_traced.unwrap_or(false));
        if !has_trace {
            untraced_modified.push(fp.path.clone());
        }
    }
    if !untraced_modified.is_empty() {
        graph_signals.push_str("- Files to modify without tracing coverage:\n");
        for p in untraced_modified.iter().take(5) {
            graph_signals.push_str(&format!("  - `{}`\n", p));
        }
    }

    let system = REVIEWER_SYSTEM_PROMPT;
    let user = format!(
        "## Feature description\n{}\n\n\
         ## Surface analysis\n{}\n\n\
         ## Proposed blueprint\n\
         Files to create: {:?}\n\
         Files to modify: {:?}\n\
         Build sequence: {:?}\n\n\
         {}\n\n\
         Produce a review in strict Markdown with:\n\
         - `### Verdict` (one of: ready / needs_revisions / blocked)\n\
         - `### High-confidence issues` (numbered list; each item: **severity/confidence**: title — detail)\n\
         - `### Predicted impact` (one paragraph)",
        feature,
        render_surface_markdown(surface),
        blueprint
            .files_to_create
            .iter()
            .map(|f| &f.path)
            .collect::<Vec<_>>(),
        blueprint
            .files_to_modify
            .iter()
            .map(|f| &f.path)
            .collect::<Vec<_>>(),
        blueprint.build_sequence,
        graph_signals
    );

    call_role_llm(config, system, &user).await
}

const REVIEWER_SYSTEM_PROMPT: &str =
    "You are the code-reviewer role in a three-stage feature development pipeline. \
You review a blueprint BEFORE code is written.\n\n\
Rules:\n\
- Report only HIGH-CONFIDENCE issues (confidence ≥ 0.8).\n\
- No stylistic nitpicks. No 'maybe's. No hypothetical edge cases.\n\
- For each issue, explain *why* it is a problem with one concrete sentence.\n\
- Honor the graph-side validation signals — treat them as ground truth.\n\
- If the blueprint is sound, say so explicitly in the verdict.";

// ─── LLM plumbing ───────────────────────────────────────────────────

async fn call_role_llm(config: &ChatConfig, system: &str, user: &str) -> Result<String, String> {
    let messages = vec![
        serde_json::json!({"role": "system", "content": format!("{}\n\n{}", PROMPT_CONTEXT_SAFETY, system)}),
        serde_json::json!({"role": "user", "content": user}),
    ];
    chat::call_llm_pub(config, &messages).await
}

// ─── Parsers: Markdown → structured fields ──────────────────────────

fn parse_blueprint_from_markdown(md: &str) -> Blueprint {
    let mut bp = Blueprint::default();
    let mut current: Option<&str> = None;
    for line in md.lines() {
        let lower = line.to_ascii_lowercase();
        if lower.contains("### files to create") {
            current = Some("create");
            continue;
        }
        if lower.contains("### files to modify") {
            current = Some("modify");
            continue;
        }
        if lower.contains("### data flow") {
            current = Some("data");
            continue;
        }
        if lower.contains("### build sequence") {
            current = Some("seq");
            continue;
        }

        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        match current {
            Some("create") | Some("modify") => {
                if let Some(rest) = trimmed
                    .strip_prefix("- ")
                    .or_else(|| trimmed.strip_prefix("* "))
                {
                    let (path, purpose) = split_path_purpose(rest);
                    let plan = FilePlan { path, purpose };
                    if current == Some("create") {
                        bp.files_to_create.push(plan);
                    } else {
                        bp.files_to_modify.push(plan);
                    }
                }
            }
            Some("data") => {
                let paragraph = trimmed.to_string();
                bp.data_flow = Some(match bp.data_flow.take() {
                    Some(prev) => format!("{prev} {paragraph}"),
                    None => paragraph,
                });
            }
            Some("seq")
                if (trimmed.starts_with(|c: char| c.is_ascii_digit()) || trimmed.starts_with("- ")) => {
                    let cleaned = trimmed
                        .trim_start_matches(|c: char| {
                            c.is_ascii_digit() || c == '.' || c == ' ' || c == '-'
                        })
                        .to_string();
                    if !cleaned.is_empty() {
                        bp.build_sequence.push(cleaned);
                    }
                }
            _ => {}
        }
    }
    bp
}

/// Split a line like "`path/to/file.rs` — purpose text" into (path, purpose).
/// Falls back to the whole line as path if no separator is found.
fn split_path_purpose(s: &str) -> (String, String) {
    let s = s.trim();
    // Try backtick-delimited path first.
    if let Some(path) = s.strip_prefix('`') {
        if let Some(end) = path.find('`') {
            let path_part = path[..end].to_string();
            let purpose = path[end + 1..]
                .trim_start_matches([' ', '—', '-', ':'])
                .trim()
                .to_string();
            return (path_part, purpose);
        }
    }
    // Fallback: split on " — ", " - ", or ": ".
    for sep in [" — ", " - ", ": "] {
        if let Some((p, rest)) = s.split_once(sep) {
            return (
                p.trim().trim_matches('`').to_string(),
                rest.trim().to_string(),
            );
        }
    }
    (s.to_string(), String::new())
}

/// Public shim so sibling modules (e.g. code_review) can reuse the parser.
pub fn parse_review_from_markdown_pub(md: &str) -> Review {
    parse_review_from_markdown(md)
}

fn parse_review_from_markdown(md: &str) -> Review {
    let mut review = Review {
        verdict: "ready".to_string(),
        ..Default::default()
    };
    let mut current: Option<&str> = None;
    let mut impact_buf = String::new();

    for line in md.lines() {
        let lower = line.to_ascii_lowercase();
        if lower.contains("### verdict") {
            current = Some("verdict");
            continue;
        }
        if lower.contains("### high-confidence issues") || lower.contains("### issues") {
            current = Some("issues");
            continue;
        }
        if lower.contains("### predicted impact") || lower.contains("### impact") {
            current = Some("impact");
            continue;
        }

        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        match current {
            Some("verdict") => {
                let v = trimmed.to_ascii_lowercase();
                if v.contains("ready") {
                    review.verdict = "ready".into();
                } else if v.contains("blocked") {
                    review.verdict = "blocked".into();
                } else if v.contains("needs_revisions") || v.contains("needs revisions") {
                    review.verdict = "needs_revisions".into();
                }
            }
            Some("issues")
                if (trimmed.starts_with(|c: char| c.is_ascii_digit()) || trimmed.starts_with("- ")) => {
                    if let Some(issue) = parse_issue_line(trimmed) {
                        review.issues.push(issue);
                    }
                }
            Some("impact") => {
                if !impact_buf.is_empty() {
                    impact_buf.push(' ');
                }
                impact_buf.push_str(trimmed);
            }
            _ => {}
        }
    }
    if !impact_buf.is_empty() {
        review.predicted_impact = Some(impact_buf);
    }
    review
}

/// Try to parse a review issue line like:
///   "1. **high/0.9**: Missing empty-state handling — The dropdown..."
///   "- high (0.85): Missing validation — The Rust command..."
fn parse_issue_line(line: &str) -> Option<ReviewIssue> {
    // Strip only list-marker characters — NOT `*`, which would eat the
    // `**bold**` delimiter we rely on to extract the severity.
    let line =
        line.trim_start_matches(|c: char| c.is_ascii_digit() || c == '.' || c == ' ' || c == '-');
    // Bullet starting with `- ` leaves a leading space; clean it up.
    let line = line.trim_start();
    // Extract the first bolded chunk as "severity/confidence".
    let (sev_part, rest) = if let Some(stripped) = line.strip_prefix("**") {
        if let Some(end) = stripped.find("**") {
            (stripped[..end].to_string(), stripped[end + 2..].to_string())
        } else {
            ("medium".to_string(), line.to_string())
        }
    } else {
        ("medium".to_string(), line.to_string())
    };

    let (severity, confidence) = parse_sev_conf(&sev_part);

    let rest = rest.trim_start_matches([':', ' ']).to_string();
    let (title, detail) = match rest.split_once(" — ").or_else(|| rest.split_once(" - ")) {
        Some((t, d)) => (t.trim().to_string(), d.trim().to_string()),
        None => (rest.trim().to_string(), String::new()),
    };

    if title.is_empty() {
        return None;
    }
    Some(ReviewIssue {
        severity,
        confidence,
        title,
        detail,
        file: None,
    })
}

fn parse_sev_conf(s: &str) -> (String, f64) {
    let lower = s.to_ascii_lowercase();
    let severity = if lower.contains("high") {
        "high"
    } else if lower.contains("low") {
        "low"
    } else {
        "medium"
    }
    .to_string();
    // Try to extract a decimal confidence from the string.
    let confidence = lower
        .split(|c: char| !c.is_ascii_digit() && c != '.')
        .find_map(|tok| tok.parse::<f64>().ok())
        .filter(|v| *v >= 0.0 && *v <= 1.0)
        .unwrap_or(match severity.as_str() {
            "high" => 0.9,
            "medium" => 0.7,
            _ => 0.5,
        });
    (severity, confidence)
}

// ─── Helpers ────────────────────────────────────────────────────────

fn extension_distribution(files: &[String]) -> Vec<(String, usize)> {
    let mut counts: HashMap<String, usize> = HashMap::new();
    for f in files {
        if let Some(ext) = Path::new(f).extension().and_then(|e| e.to_str()) {
            *counts.entry(ext.to_string()).or_insert(0) += 1;
        }
    }
    let mut out: Vec<(String, usize)> = counts.into_iter().collect();
    out.sort_by(|a, b| b.1.cmp(&a.1));
    out
}

fn detect_name_collisions(feature: &str, graph: &KnowledgeGraph) -> Vec<String> {
    // Grab capitalized words from the feature description — heuristic for
    // identifier names the user might have in mind.
    let candidates: HashSet<String> = feature
        .split(|c: char| !c.is_alphanumeric() && c != '_')
        .filter(|t| t.len() >= 3)
        .filter(|t| {
            t.chars()
                .next()
                .map(|c| c.is_ascii_uppercase())
                .unwrap_or(false)
        })
        .map(|t| t.to_string())
        .collect();
    if candidates.is_empty() {
        return Vec::new();
    }
    let existing: HashSet<String> = graph
        .iter_nodes()
        .map(|n| n.properties.name.clone())
        .collect();
    candidates
        .into_iter()
        .filter(|c| existing.contains(c))
        .collect()
}

fn emit_phase(
    app: &AppHandle,
    artifact_id: &str,
    phase: FeatureDevPhase,
    status: PhaseStatus,
    message: Option<String>,
) {
    let _ = app.emit(
        "feature-dev-phase",
        FeatureDevPhaseEvent {
            artifact_id: artifact_id.to_string(),
            phase,
            status,
            message,
        },
    );
}

fn emit_section(app: &AppHandle, artifact_id: &str, section: &FeatureDevSection) {
    let _ = app.emit(
        "feature-dev-section",
        FeatureDevSectionEvent {
            artifact_id: artifact_id.to_string(),
            section: section.clone(),
        },
    );
}

// ─── Tests ──────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_split_path_purpose_backtick() {
        let (path, purpose) = split_path_purpose("`src/foo.rs` — adds new trait");
        assert_eq!(path, "src/foo.rs");
        assert_eq!(purpose, "adds new trait");
    }

    #[test]
    fn test_split_path_purpose_no_backtick() {
        let (path, purpose) = split_path_purpose("src/bar.rs - handles validation");
        assert_eq!(path, "src/bar.rs");
        assert_eq!(purpose, "handles validation");
    }

    #[test]
    fn test_parse_issue_line_high_confidence() {
        let issue =
            parse_issue_line("1. **high/0.9**: Missing validation — The input isn't sanitized")
                .unwrap();
        assert_eq!(issue.severity, "high");
        assert!(issue.confidence >= 0.85);
        assert_eq!(issue.title, "Missing validation");
        assert!(issue.detail.contains("sanitized"));
    }

    #[test]
    fn test_parse_review_verdict_ready() {
        let md = "### Verdict\nready\n### High-confidence issues\n";
        let r = parse_review_from_markdown(md);
        assert_eq!(r.verdict, "ready");
    }

    #[test]
    fn test_parse_review_verdict_blocked() {
        let md = "### Verdict\nblocked\n";
        let r = parse_review_from_markdown(md);
        assert_eq!(r.verdict, "blocked");
    }

    #[test]
    fn test_parse_blueprint_sections() {
        let md = "\
### Files to create
- `src/new.rs` — the new module
- `tests/foo.rs` — tests

### Files to modify
- `src/lib.rs` — export new module

### Build sequence
1. Create src/new.rs
2. Wire into lib.rs
3. Add tests
";
        let bp = parse_blueprint_from_markdown(md);
        assert_eq!(bp.files_to_create.len(), 2);
        assert_eq!(bp.files_to_modify.len(), 1);
        assert_eq!(bp.build_sequence.len(), 3);
        assert_eq!(bp.files_to_create[0].path, "src/new.rs");
        assert_eq!(bp.files_to_create[0].purpose, "the new module");
    }
}

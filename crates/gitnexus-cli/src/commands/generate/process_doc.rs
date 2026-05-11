//! Process documentation generator v2: produces markdown for each detected Process.
//!
//! Combines three evidence sources with explicit priority:
//! 1. Code (highest) — source code from the knowledge graph
//! 2. Traces (high) — runtime behavior, including raw trace parameters if --traces-dir provided
//! 3. RAG docs (informative) — external documentation via Mentions edges + FTS search

use std::collections::HashMap;
use std::path::Path;

use anyhow::Result;
use colored::Colorize;
use serde_json::Value;
use tracing::info;

use gitnexus_core::graph::types::*;
use gitnexus_core::graph::KnowledgeGraph;
use gitnexus_core::process_doc::{
    self, collect_full_evidence, generate_component_diagram, generate_sequence_diagram, Evidence,
    EvidenceSource, ProcessEvidence, TraceParam,
};
use gitnexus_core::trace;

use super::utils::sanitize_filename;

/// Generate process documentation pages.
/// Returns (id, title, filename) tuples for `_index.json` integration.
pub(super) fn generate_process_docs(
    graph: &KnowledgeGraph,
    repo_path: &Path,
    docs_dir: &Path,
    traces_dir: Option<&Path>,
) -> Result<Vec<(String, String, String)>> {
    let mut processes: Vec<(String, String, Option<u32>)> = graph
        .iter_nodes()
        .filter(|n| n.label == NodeLabel::Process)
        .map(|n| {
            (
                n.id.clone(),
                n.properties.name.clone(),
                n.properties.step_count,
            )
        })
        .collect();

    if processes.is_empty() {
        info!("No processes found in graph, skipping process-doc generation");
        return Ok(Vec::new());
    }

    processes.sort_by(|a, b| b.2.unwrap_or(0).cmp(&a.2.unwrap_or(0)));

    let processes_dir = docs_dir.join("processes");
    std::fs::create_dir_all(&processes_dir)?;

    println!(
        "{} Generating process documentation ({} processes)...",
        "->".cyan(),
        processes.len()
    );

    // Load raw trace data if --traces-dir provided
    let trace_params = if let Some(dir) = traces_dir {
        println!("  Loading raw traces from {}...", dir.display());
        load_all_trace_params(dir, graph)
    } else {
        HashMap::new()
    };

    // Search for relevant RAG doc chunks via FTS
    let fts_rag_evidence = search_rag_chunks_for_processes(graph, &processes);

    let mut page_entries = Vec::new();
    let mut all_evidence: Vec<(String, ProcessEvidence)> = Vec::new();
    // Track filename slugs already written so two processes with the same
    // sanitized name (e.g. flows entering at a shared method) don't have one
    // silently clobber the other on disk. The overview at the bottom of this
    // function depends on the same slug, so we have to remember which slug
    // each evidence ended up with.
    let mut used_slugs: std::collections::HashSet<String> = std::collections::HashSet::new();

    for (process_id, process_name, _) in &processes {
        let mut evidence = collect_full_evidence(graph, process_id, repo_path);

        // Enrich with FTS RAG evidence
        if let Some(extra_rag) = fts_rag_evidence.get(process_id.as_str()) {
            for rag_ev in extra_rag {
                // Deduplicate: skip if already present via Mentions edges
                if !evidence
                    .global_rag_evidence
                    .iter()
                    .any(|e| e.id == rag_ev.id)
                {
                    evidence.global_rag_evidence.push(rag_ev.clone());
                }
            }
        }

        let base_slug = sanitize_filename(process_name);
        let mut slug = base_slug.clone();
        let mut n = 2u32;
        while !used_slugs.insert(slug.clone()) {
            slug = format!("{}-{}", base_slug, n);
            n += 1;
        }
        let filename = format!("processes/process-{}.md", slug);
        let filepath = docs_dir.join(&filename);

        let content = render_process_page(&evidence, &trace_params);
        std::fs::write(&filepath, &content)?;

        page_entries.push((
            format!("process-{}", slug),
            format!("Processus : {}", process_name),
            filename,
        ));
        all_evidence.push((slug, evidence));
    }

    // Generate overview page (pass the disambiguated slugs alongside each
    // evidence so the overview links resolve to the same files written above)
    let overview_content = render_process_overview(&all_evidence);
    std::fs::write(docs_dir.join("process-overview.md"), &overview_content)?;
    page_entries.insert(
        0,
        (
            "process-overview".to_string(),
            "Vue des Processus".to_string(),
            "process-overview.md".to_string(),
        ),
    );

    println!(
        "{} Generated {} process documentation pages",
        "OK".green(),
        page_entries.len()
    );

    Ok(page_entries)
}

// ─── Raw trace loading (Axe 1) ─────────────────────────────────────────

/// Load all trace files from a directory and extract parameters per method.
/// Returns method_name → Vec<TraceParam>.
fn load_all_trace_params(
    traces_dir: &Path,
    graph: &KnowledgeGraph,
) -> HashMap<String, Vec<TraceParam>> {
    let mut result: HashMap<String, Vec<TraceParam>> = HashMap::new();
    let name_index = trace::build_name_index(graph);

    let entries = match std::fs::read_dir(traces_dir) {
        Ok(e) => e,
        Err(_) => return result,
    };

    for entry in entries.flatten() {
        let path = entry.path();
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        if !matches!(ext, "json" | "ndjson" | "log" | "txt") {
            continue;
        }

        let content = match std::fs::read_to_string(&path) {
            Ok(c) => c,
            Err(_) => continue,
        };

        let steps = match trace::parse_trace(&content) {
            Ok(s) => s,
            Err(_) => continue,
        };

        for step in &steps {
            let method_name = step
                .get("method")
                .or(step.get("name"))
                .and_then(|v| v.as_str());

            if let Some(full_name) = method_name {
                // Try to resolve to a graph node
                if let Some(_node_id) = trace::resolve_method_node(graph, &name_index, full_name) {
                    let parts: Vec<&str> = full_name.split('.').collect();
                    let short_name = parts.last().copied().unwrap_or(full_name);

                    // Extract params from trace step
                    if let Some(params) = step.get("params").and_then(|v| v.as_object()) {
                        let trace_params: Vec<TraceParam> = params
                            .iter()
                            .map(|(k, v)| TraceParam {
                                name: k.clone(),
                                value: match v {
                                    Value::String(s) => s.clone(),
                                    other => other.to_string(),
                                },
                                param_type: None,
                            })
                            .collect();

                        if !trace_params.is_empty() {
                            result
                                .entry(short_name.to_string())
                                .or_default()
                                .extend(trace_params);
                        }
                    }
                }
            }
        }
    }

    if !result.is_empty() {
        println!("  Loaded trace parameters for {} methods", result.len());
    }

    result
}

// ─── FTS RAG search (Axe 2) ────────────────────────────────────────────

/// Search for relevant DocChunk nodes using FTS, keyed by process_id.
fn search_rag_chunks_for_processes(
    graph: &KnowledgeGraph,
    processes: &[(String, String, Option<u32>)],
) -> HashMap<String, Vec<Evidence>> {
    let mut result: HashMap<String, Vec<Evidence>> = HashMap::new();

    for (process_id, process_name, _) in processes {
        // Build search query from process name + collect step class names
        let steps = process_doc::collect_process_steps(graph, process_id);
        let mut query_parts: Vec<&str> = vec![process_name.as_str()];
        for step in &steps {
            if let Some(class) = &step.class_name {
                if !query_parts.contains(&class.as_str()) {
                    query_parts.push(class.as_str());
                }
            }
        }
        let query = query_parts.join(" ");

        // Search DocChunk nodes by content matching
        let mut matches = Vec::new();
        let query_lower = query.to_lowercase();
        let query_terms: Vec<&str> = query_lower.split_whitespace().collect();

        for node in graph.iter_nodes() {
            if node.label != NodeLabel::DocChunk {
                continue;
            }
            if let Some(content) = &node.properties.content {
                let content_lower = content.to_lowercase();
                let match_count = query_terms
                    .iter()
                    .filter(|term| content_lower.contains(*term))
                    .count();

                if match_count >= 2 || (query_terms.len() == 1 && match_count == 1) {
                    let score = match_count as f64 / query_terms.len() as f64;
                    matches.push((node, score));
                }
            }
        }

        // Sort by score desc, take top 3
        matches.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        let evidence: Vec<Evidence> = matches
            .into_iter()
            .take(3)
            .map(|(node, score)| Evidence {
                id: format!("RAG:{}", node.id),
                source: EvidenceSource::Rag,
                content: node.properties.content.clone().unwrap_or_default(),
                file_path: node.properties.file_path.clone(),
                start_line: None,
                end_line: None,
                confidence: (score * 0.7).min(0.8),
                staleness_warning: true,
            })
            .collect();

        if !evidence.is_empty() {
            result.insert(process_id.clone(), evidence);
        }
    }

    result
}

// ─── Page rendering (Axes 3+4) ─────────────────────────────────────────

fn render_process_page(
    ev: &ProcessEvidence,
    trace_params: &HashMap<String, Vec<TraceParam>>,
) -> String {
    let mut md = String::with_capacity(8192);

    // Header
    md.push_str(&format!("# Processus : {}\n\n", ev.process_name));

    // Source files
    let mut files: Vec<&str> = ev.steps.iter().map(|s| s.file_path.as_str()).collect();
    files.sort();
    files.dedup();
    if !files.is_empty() {
        md.push_str("<details>\n<summary>Fichiers source pertinents</summary>\n\n");
        for f in &files {
            md.push_str(&format!("- `{}`\n", f));
        }
        md.push_str("\n</details>\n\n");
    }

    // Properties table
    let process_type = ev
        .process_type
        .map(|t| match t {
            ProcessType::IntraCommunity => "Intra-module",
            ProcessType::CrossCommunity => "Inter-modules",
        })
        .unwrap_or("Inconnu");

    let trace_assessment = if ev.trace_coverage_pct >= 80.0 {
        "Bonne"
    } else if ev.trace_coverage_pct >= 40.0 {
        "Partielle"
    } else {
        "Faible"
    };

    md.push_str("| Propriété | Valeur |\n");
    md.push_str("|-----------|--------|\n");
    md.push_str(&format!("| Type | {} |\n", process_type));
    md.push_str(&format!("| Étapes | {} |\n", ev.steps.len()));
    if let Some(entry) = &ev.entry_point_name {
        let file = ev.entry_point_file.as_deref().unwrap_or("");
        md.push_str(&format!(
            "| Point d'entrée | `{}` dans `{}` |\n",
            entry, file
        ));
    }
    if let Some(term) = &ev.terminal_name {
        md.push_str(&format!("| Terminal | `{}` |\n", term));
    }
    md.push_str(&format!(
        "| Couverture trace | {:.0}% ({}/{} étapes) — {} |\n",
        ev.trace_coverage_pct,
        ev.steps.iter().filter(|s| s.is_traced).count(),
        ev.steps.len(),
        trace_assessment
    ));
    if !ev.communities.is_empty() {
        md.push_str(&format!(
            "| Communautés | {} |\n",
            ev.communities.join(", ")
        ));
    }
    md.push('\n');

    // Process Flow
    md.push_str("## Flux d'Exécution\n\n");

    let seq_diagram = generate_sequence_diagram(&ev.steps);
    md.push_str("```mermaid\n");
    md.push_str(&seq_diagram);
    md.push_str("\n```\n\n");

    // Step-by-step table
    md.push_str("### Déroulé Étape par Étape\n\n");
    md.push_str("| Étape | Méthode | Classe | Fichier | Tracé |\n");
    md.push_str("|-------|---------|--------|---------|-------|\n");
    for step in &ev.steps {
        let class = step.class_name.as_deref().unwrap_or("-");
        let traced = if step.is_traced { "Oui" } else { "Non" };
        md.push_str(&format!(
            "| {} | `{}` | {} | `{}:L{}` | {} |\n",
            step.step_number,
            step.name,
            class,
            step.file_path,
            step.start_line.unwrap_or(0),
            traced
        ));
    }
    md.push('\n');

    // Detailed step evidence
    for (i, step) in ev.steps.iter().enumerate() {
        md.push_str(&format!(
            "#### Étape {} : `{}`\n\n",
            step.step_number, step.name
        ));

        if let Some(class) = &step.class_name {
            md.push_str(&format!(
                "**Composant** : {} ({})\n\n",
                class,
                step.label.as_str()
            ));
        }

        // Code evidence (highest priority)
        if let Some(step_ev) = ev.step_evidence.get(i) {
            for evidence in step_ev.iter().filter(|e| e.source == EvidenceSource::Code) {
                let lang = step
                    .language
                    .map(|l| l.as_str().to_lowercase())
                    .unwrap_or_else(|| detect_lang_from_path(&step.file_path));
                md.push_str(&format!("**Code source** `[{}]`\n\n", evidence.id));
                md.push_str(&format!("```{}\n{}\n```\n\n", lang, evidence.content));
            }

            // Trace evidence from graph
            for evidence in step_ev.iter().filter(|e| e.source == EvidenceSource::Trace) {
                md.push_str(&format!("**Données d'exécution** `[{}]`\n\n", evidence.id));
                md.push_str(&format!("{}\n\n", evidence.content));
            }
        }

        // Axe 4: Raw trace parameters (from --traces-dir)
        if let Some(params) = trace_params.get(&step.name) {
            if !params.is_empty() {
                md.push_str(&format!(
                    "**Paramètres runtime** `[TRACE:step_{}]`\n\n",
                    step.step_number
                ));
                md.push_str("| Paramètre | Valeur |\n");
                md.push_str("|-----------|--------|\n");
                // Deduplicate params by name, keep first occurrence
                let mut seen = std::collections::HashSet::new();
                for param in params {
                    if seen.insert(&param.name) {
                        // Use char-based truncation rather than byte slicing
                        // — `param.value` may contain multi-byte UTF-8
                        // (accents, emoji, CJK) and `&param.value[..80]`
                        // would panic if the 80th byte falls inside a code
                        // point.
                        let display_val = if param.value.chars().count() > 80 {
                            let truncated: String = param.value.chars().take(80).collect();
                            format!("{}...", truncated)
                        } else {
                            param.value.clone()
                        };
                        md.push_str(&format!("| `{}` | {} |\n", param.name, display_val));
                    }
                }
                // Check for return_value
                if let Some(ret) = params.iter().find(|p| p.name == "return_value") {
                    md.push_str(&format!("\n**Valeur de retour** : `{}`\n", ret.value));
                }
                md.push('\n');
            }
        }

        // RAG evidence (show at first step + any step that has specific RAG)
        if i == 0 && !ev.global_rag_evidence.is_empty() {
            md.push_str("> [!NOTE]\n");
            md.push_str("> Ces informations viennent d'une documentation externe et peuvent ne pas refléter le code actuel.\n\n");
            for evidence in ev.global_rag_evidence.iter().take(3) {
                // Char-based truncation: `evidence.content` is RAG text
                // pulled from arbitrary documentation and may contain
                // multi-byte UTF-8. Slicing by byte index would panic on
                // non-ASCII boundaries.
                let content = if evidence.content.chars().count() > 500 {
                    let truncated: String = evidence.content.chars().take(500).collect();
                    format!("{}...", truncated)
                } else {
                    evidence.content.clone()
                };
                md.push_str(&format!("{} `[{}]`\n\n", content, evidence.id));
            }
        }
    }

    // Data Flow
    if !ev.entities.is_empty() {
        md.push_str("## Flux de Données\n\n");

        let reads: Vec<_> = ev
            .entities
            .iter()
            .filter(|e| {
                matches!(
                    e.access_type,
                    process_doc::EntityAccessType::Read | process_doc::EntityAccessType::ReadWrite
                )
            })
            .collect();
        let writes: Vec<_> = ev
            .entities
            .iter()
            .filter(|e| {
                matches!(
                    e.access_type,
                    process_doc::EntityAccessType::Write | process_doc::EntityAccessType::ReadWrite
                )
            })
            .collect();

        if !reads.is_empty() {
            md.push_str("### Entités lues\n\n");
            md.push_str("| Entité | Table | Étape |\n");
            md.push_str("|--------|-------|-------|\n");
            for entity in reads {
                let table = entity.db_table_name.as_deref().unwrap_or("-");
                let step = entity
                    .accessed_by_step
                    .map(|s| format!("Étape {}", s))
                    .unwrap_or_else(|| "-".to_string());
                md.push_str(&format!("| {} | {} | {} |\n", entity.name, table, step));
            }
            md.push('\n');
        }

        if !writes.is_empty() {
            md.push_str("### Entités écrites et effets de bord\n\n");
            md.push_str("| Entité | Table | Étape |\n");
            md.push_str("|--------|-------|-------|\n");
            for entity in writes {
                let table = entity.db_table_name.as_deref().unwrap_or("-");
                let step = entity
                    .accessed_by_step
                    .map(|s| format!("Étape {}", s))
                    .unwrap_or_else(|| "-".to_string());
                md.push_str(&format!("| {} | {} | {} |\n", entity.name, table, step));
            }
            md.push('\n');
        }
    }

    // Component Architecture
    if !ev.components.is_empty() {
        md.push_str("## Architecture des Composants\n\n");

        let comp_diagram = generate_component_diagram(&ev.components);
        md.push_str("```mermaid\n");
        md.push_str(&comp_diagram);
        md.push_str("\n```\n\n");

        md.push_str("| Composant | Type | Fichier | Étapes |\n");
        md.push_str("|-----------|------|---------|--------|\n");
        for comp in &ev.components {
            md.push_str(&format!(
                "| {} | {} | `{}` | {} |\n",
                comp.name,
                comp.label.as_str(),
                comp.file_path,
                comp.step_count
            ));
        }
        md.push('\n');
    }

    // Quality Metrics
    md.push_str("## Qualité du Flux\n\n");
    md.push_str("| Indicateur | Valeur | Lecture |\n");
    md.push_str("|------------|--------|---------|\n");
    md.push_str(&format!(
        "| Couverture trace | {:.0}% | {} |\n",
        ev.trace_coverage_pct, trace_assessment
    ));
    md.push_str(&format!(
        "| Code mort potentiel | {} méthodes | {} |\n",
        ev.dead_code_candidates.len(),
        if ev.dead_code_candidates.is_empty() {
            "Rien à signaler"
        } else {
            "À vérifier"
        }
    ));
    md.push_str(&format!("| Type de processus | {} | |\n", process_type));
    md.push('\n');

    // References
    md.push_str("## Références\n\n");

    md.push_str("### Fichiers source\n\n");
    md.push_str("| Fichier | Lignes | Composant |\n");
    md.push_str("|---------|--------|-----------|\n");
    for step in &ev.steps {
        let class = step.class_name.as_deref().unwrap_or("-");
        let lines = match (step.start_line, step.end_line) {
            (Some(s), Some(e)) => format!("{}-{}", s, e),
            _ => "-".to_string(),
        };
        md.push_str(&format!(
            "| `{}` | {} | {} |\n",
            step.file_path, lines, class
        ));
    }
    md.push('\n');

    if !ev.global_rag_evidence.is_empty() {
        md.push_str("### Documentation liée\n\n");
        md.push_str("> [!NOTE]\n");
        md.push_str("> La documentation externe peut ne pas refléter le code actuel.\n\n");
        md.push_str("| Document | Source |\n");
        md.push_str("|----------|--------|\n");
        for rag in &ev.global_rag_evidence {
            let title = rag
                .content
                .lines()
                .next()
                .unwrap_or("Untitled")
                .chars()
                .take(60)
                .collect::<String>();
            md.push_str(&format!("| {} | `[{}]` |\n", title, rag.id));
        }
        md.push('\n');
    }

    md
}

fn render_process_overview(all_evidence: &[(String, ProcessEvidence)]) -> String {
    let mut md = String::with_capacity(4096);

    md.push_str("# Vue des Processus\n\n");

    let total = all_evidence.len();
    let fully_traced = all_evidence
        .iter()
        .filter(|(_, e)| e.trace_coverage_pct >= 100.0)
        .count();
    let partially_traced = all_evidence
        .iter()
        .filter(|(_, e)| e.trace_coverage_pct > 0.0 && e.trace_coverage_pct < 100.0)
        .count();
    let untraced = total - fully_traced - partially_traced;

    md.push_str(&format!(
        "> Cette section documente les {} flux d'exécution détectés dans le codebase.\n\n",
        total
    ));

    md.push_str("| # | Processus | Étapes | Type | Point d'entrée | Couverture trace |\n");
    md.push_str("|---|-----------|--------|------|----------------|------------------|\n");
    for (i, (slug, ev)) in all_evidence.iter().enumerate() {
        let ptype = ev
            .process_type
            .map(|t| match t {
                ProcessType::IntraCommunity => "Intra",
                ProcessType::CrossCommunity => "Inter-modules",
            })
            .unwrap_or("-");
        let entry = ev.entry_point_name.as_deref().unwrap_or("-");
        md.push_str(&format!(
            "| {} | [{}](./processes/process-{}.md) | {} | {} | `{}` | {:.0}% |\n",
            i + 1,
            ev.process_name,
            slug,
            ev.steps.len(),
            ptype,
            entry,
            ev.trace_coverage_pct
        ));
    }
    md.push('\n');

    md.push_str("## Synthèse de la Traçabilité\n\n");
    md.push_str("| Indicateur | Valeur |\n");
    md.push_str("|------------|--------|\n");
    md.push_str(&format!("| Processus total | {} |\n", total));
    md.push_str(&format!(
        "| Entièrement tracés (100 %) | {} |\n",
        fully_traced
    ));
    md.push_str(&format!(
        "| Partiellement tracés | {} |\n",
        partially_traced
    ));
    md.push_str(&format!("| Non tracés | {} |\n", untraced));
    md.push('\n');
    md
}

fn detect_lang_from_path(path: &str) -> String {
    if path.ends_with(".cs") {
        "csharp".to_string()
    } else if path.ends_with(".ts") || path.ends_with(".tsx") {
        "typescript".to_string()
    } else if path.ends_with(".js") || path.ends_with(".jsx") {
        "javascript".to_string()
    } else if path.ends_with(".rs") {
        "rust".to_string()
    } else if path.ends_with(".py") {
        "python".to_string()
    } else if path.ends_with(".java") {
        "java".to_string()
    }
    // Kotlin had previously been bucketed under "java" — render-time
    // syntax highlighters then mis-coloured every Kotlin snippet (e.g.
    // `fun`, `val`, `data class` keywords) as Java identifiers in the
    // generated process docs.
    else if path.ends_with(".kt") || path.ends_with(".kts") {
        "kotlin".to_string()
    } else if path.ends_with(".go") {
        "go".to_string()
    } else {
        String::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gitnexus_core::process_doc::ProcessStep;

    fn evidence(process_name: &str, process_type: ProcessType, coverage: f64) -> ProcessEvidence {
        ProcessEvidence {
            process_id: process_name.to_string(),
            process_name: process_name.to_string(),
            process_type: Some(process_type),
            entry_point_name: Some("start".to_string()),
            entry_point_file: Some("src/lib.rs".to_string()),
            terminal_name: Some("finish".to_string()),
            communities: Vec::new(),
            steps: Vec::new(),
            step_evidence: Vec::new(),
            global_rag_evidence: Vec::new(),
            entities: Vec::new(),
            components: Vec::new(),
            trace_coverage_pct: coverage,
            dead_code_candidates: Vec::new(),
        }
    }

    #[test]
    fn process_overview_hides_internal_markers_and_uses_french_labels() {
        let overview = render_process_overview(&[
            (
                "alpha".to_string(),
                evidence("alpha", ProcessType::CrossCommunity, 0.0),
            ),
            (
                "beta".to_string(),
                evidence("beta", ProcessType::IntraCommunity, 100.0),
            ),
        ]);

        assert!(!overview.contains("GNX:"));
        assert!(!overview.contains("Business Processes"));
        assert!(!overview.contains("Trace Coverage Summary"));
        assert!(!overview.contains("Entry Point"));
        assert!(overview.contains("# Vue des Processus"));
        assert!(overview.contains("Point d'entrée"));
        assert!(overview.contains("Inter-modules"));
        assert!(overview.contains("Synthèse de la Traçabilité"));
    }

    #[test]
    fn process_detail_hides_internal_markers_and_uses_french_labels() {
        let mut ev = evidence("start -> finish", ProcessType::CrossCommunity, 0.0);
        ev.steps = vec![ProcessStep {
            step_number: 1,
            node_id: "N1".to_string(),
            name: "start".to_string(),
            class_name: Some("Runner".to_string()),
            file_path: "src/lib.rs".to_string(),
            start_line: Some(10),
            end_line: Some(12),
            return_type: None,
            parameter_count: None,
            is_traced: false,
            trace_call_count: 0,
            label: NodeLabel::Function,
            language: None,
        }];
        ev.step_evidence = vec![vec![Evidence {
            id: "CODE:Function:src/lib.rs:start".to_string(),
            source: EvidenceSource::Code,
            content: "fn start() {}".to_string(),
            file_path: "src/lib.rs".to_string(),
            start_line: Some(10),
            end_line: Some(12),
            confidence: 1.0,
            staleness_warning: false,
        }]];

        let page = render_process_page(&ev, &HashMap::new());

        assert!(!page.contains("GNX:"));
        assert!(!page.contains("# Process:"));
        assert!(!page.contains("| Property | Value |"));
        assert!(!page.contains("Process Flow"));
        assert!(!page.contains("Step-by-Step Breakdown"));
        assert!(!page.contains("Source Code"));
        assert!(!page.contains("Quality Metrics"));
        assert!(!page.contains("References"));
        assert!(!page.contains("Trace Coverage"));
        assert!(page.contains("# Processus : start -> finish"));
        assert!(page.contains("| Propriété | Valeur |"));
        assert!(page.contains("## Flux d'Exécution"));
        assert!(page.contains("### Déroulé Étape par Étape"));
        assert!(page.contains("**Code source**"));
        assert!(page.contains("## Qualité du Flux"));
        assert!(page.contains("## Références"));
        assert!(page.contains("| Couverture trace | 0% | Faible |"));
    }
}

//! Phase 8: LLM Enrichment — annotate high-priority symbols with
//! architectural insights (code smells, patterns, risk scores).

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tracing::{info, warn};

use code_explorer_core::graph::types::{EnrichedBy, NodeLabel, RelationshipType};
use code_explorer_core::graph::KnowledgeGraph;
use code_explorer_core::llm::openai::OpenAILlmProvider;
use code_explorer_core::llm::{collect_completion, Message, Role, PROMPT_CONTEXT_SAFETY};

use crate::pipeline::ProgressSender;

// ─── Configuration ──────────────────────────────────────────────────────

/// Configuration for LLM enrichment (Phase 8).
#[derive(Debug, Clone)]
pub struct LlmEnrichmentConfig {
    /// LLM provider base URL (from chat-config.json)
    pub base_url: String,
    /// API key
    pub api_key: String,
    /// Model name
    pub model: String,
    /// Max output tokens per request
    pub max_tokens: u32,
    /// Reasoning effort (empty or "none" to skip)
    pub reasoning_effort: String,
    /// Maximum total input+output tokens budget across all calls
    pub token_budget: u64,
    /// Maximum symbols to enrich (0 = unlimited, bounded by budget)
    pub max_symbols: usize,
    /// Batch size (symbols per LLM request)
    pub batch_size: usize,
}

impl Default for LlmEnrichmentConfig {
    fn default() -> Self {
        Self {
            base_url: String::new(),
            api_key: String::new(),
            model: String::new(),
            max_tokens: 4096,
            reasoning_effort: String::new(),
            token_budget: 100_000,
            max_symbols: 0,
            batch_size: 5,
        }
    }
}

// ─── Statistics ─────────────────────────────────────────────────────────

/// Result statistics from Phase 8.
#[derive(Debug, Default)]
pub struct LlmEnrichmentStats {
    pub candidates_found: usize,
    pub symbols_enriched: usize,
    pub symbols_skipped_cached: usize,
    pub batches_sent: usize,
    pub tokens_used_estimate: u64,
    pub errors: usize,
}

// ─── LLM Response Types ────────────────────────────────────────────────

/// Structured response from the LLM for a single symbol.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SymbolInsight {
    pub symbol_id: String,
    #[serde(default)]
    pub smells: Vec<String>,
    #[serde(default)]
    pub patterns: Vec<String>,
    #[serde(default)]
    pub risk_score: u32,
    #[serde(default)]
    pub refactoring: Option<String>,
    #[serde(default)]
    pub summary: Option<String>,
}

/// Structured response from the LLM for a batch of symbols.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct BatchInsightResponse {
    pub insights: Vec<SymbolInsight>,
}

// ─── Internal: Enrichment Candidate ────────────────────────────────────

#[derive(Debug, Clone)]
struct EnrichmentCandidate {
    node_id: String,
    priority_score: f64,
    source_hash: String,
    context_snippet: String,
    label: NodeLabel,
    file_path: String,
    start_line: Option<u32>,
    end_line: Option<u32>,
    incoming_count: usize,
    outgoing_count: usize,
    is_dead: bool,
    hotspot_score: f64,
}

// ─── Target Labels ─────────────────────────────────────────────────────

fn is_enrichable_label(label: &NodeLabel) -> bool {
    matches!(
        label,
        NodeLabel::Class
            | NodeLabel::Controller
            | NodeLabel::Service
            | NodeLabel::Function
            | NodeLabel::Method
            | NodeLabel::Interface
            | NodeLabel::Struct
            | NodeLabel::Constructor
    )
}

// ─── Priority Scoring ──────────────────────────────────────────────────

/// Build a file→hotspot_score map from git history.
fn build_hotspot_map(repo_path: &Path) -> HashMap<String, f64> {
    let mut map = HashMap::new();
    match code_explorer_git::hotspots::analyze_hotspots(repo_path, 90) {
        Ok(hotspots) => {
            for h in hotspots {
                map.insert(h.path.clone(), h.score);
            }
        }
        Err(e) => {
            info!("Skipping git hotspots for enrichment priority: {e}");
        }
    }
    map
}

/// Build node_id→(incoming, outgoing) edge counts for enrichable relationship types.
fn build_coupling_map(graph: &KnowledgeGraph) -> HashMap<String, (usize, usize)> {
    let mut map: HashMap<String, (usize, usize)> = HashMap::new();
    for rel in graph.iter_relationships() {
        if matches!(
            rel.rel_type,
            RelationshipType::Calls
                | RelationshipType::Imports
                | RelationshipType::DependsOn
                | RelationshipType::Uses
        ) {
            map.entry(rel.source_id.clone()).or_default().1 += 1;
            map.entry(rel.target_id.clone()).or_default().0 += 1;
        }
    }
    map
}

/// Compute SHA-256 hash of a symbol's source code region.
fn compute_source_hash(repo_path: &Path, file_path: &str, start: u32, end: u32) -> Option<String> {
    let full_path = resolve_repo_file(repo_path, file_path)?;
    let content = std::fs::read_to_string(&full_path).ok()?;
    let lines: Vec<&str> = content.lines().collect();
    let s = start.saturating_sub(1) as usize;
    let e = (end as usize).min(lines.len());
    if s >= e {
        return None;
    }
    let region = lines[s..e].join("\n");
    let mut hasher = Sha256::new();
    hasher.update(region.as_bytes());
    Some(format!("{:x}", hasher.finalize()))
}

/// Read source code excerpt for the prompt (first N lines of the symbol).
fn read_code_excerpt(
    repo_path: &Path,
    file_path: &str,
    start: u32,
    end: u32,
    max_lines: usize,
) -> String {
    let full_path = match resolve_repo_file(repo_path, file_path) {
        Some(path) => path,
        None => return String::from("(source unavailable)"),
    };
    let content = match std::fs::read_to_string(&full_path) {
        Ok(c) => c,
        Err(_) => return String::from("(source unavailable)"),
    };
    let lines: Vec<&str> = content.lines().collect();
    let s = start.saturating_sub(1) as usize;
    let e = (end as usize).min(lines.len()).min(s + max_lines);
    if s >= lines.len() {
        return String::from("(out of range)");
    }
    lines[s..e].join("\n")
}

fn resolve_repo_file(repo_path: &Path, file_path: &str) -> Option<PathBuf> {
    let repo_root = repo_path.canonicalize().ok()?;
    let candidate = repo_root.join(file_path);
    let full_path = candidate.canonicalize().ok()?;
    if full_path.starts_with(&repo_root) && full_path.is_file() {
        Some(full_path)
    } else {
        None
    }
}

/// Select and score candidates for LLM enrichment.
fn select_candidates(
    graph: &KnowledgeGraph,
    repo_path: &Path,
    config: &LlmEnrichmentConfig,
) -> Vec<EnrichmentCandidate> {
    let hotspot_map = build_hotspot_map(repo_path);
    let coupling_map = build_coupling_map(graph);

    let mut candidates = Vec::new();

    for node in graph.iter_nodes() {
        if !is_enrichable_label(&node.label) {
            continue;
        }

        let p = &node.properties;

        // Require line range info
        let (start_line, end_line) = match (p.start_line, p.end_line) {
            (Some(s), Some(e)) if e > s => (s, e),
            _ => continue,
        };

        let line_count = end_line - start_line;

        // Compute source hash
        let source_hash = match compute_source_hash(repo_path, &p.file_path, start_line, end_line) {
            Some(h) => h,
            None => continue,
        };

        // Check if already enriched with same hash (incremental skip)
        if let Some(existing_hash) = &p.llm_source_hash {
            if *existing_hash == source_hash {
                continue; // unchanged since last enrichment
            }
        }

        // Priority components
        let hotspot_score = hotspot_map.get(&p.file_path).copied().unwrap_or(0.0);

        let (incoming, outgoing) = coupling_map.get(&node.id).copied().unwrap_or((0, 0));

        let coupling_degree_norm = ((incoming + outgoing) as f64 / 20.0).min(1.0);
        let size_score = (line_count as f64 / 500.0).min(1.0);
        let is_dead = p.is_dead_candidate.unwrap_or(false);
        let dead_code_bonus = if is_dead { 1.0 } else { 0.0 };
        // Simple untested heuristic: controllers/services without "Test" in file name
        let untested_bonus = if matches!(node.label, NodeLabel::Controller | NodeLabel::Service)
            && !p.file_path.contains("Test")
            && !p.file_path.contains("test")
        {
            1.0
        } else {
            0.0
        };

        let priority_score = 0.35 * hotspot_score
            + 0.25 * coupling_degree_norm
            + 0.20 * size_score
            + 0.10 * untested_bonus
            + 0.10 * dead_code_bonus;

        // Skip trivially low-priority symbols (< 5% score)
        if priority_score < 0.05 {
            continue;
        }

        let context_snippet = read_code_excerpt(repo_path, &p.file_path, start_line, end_line, 30);

        candidates.push(EnrichmentCandidate {
            node_id: node.id.clone(),
            priority_score,
            source_hash,
            context_snippet,
            label: node.label,
            file_path: p.file_path.clone(),
            start_line: Some(start_line),
            end_line: Some(end_line),
            incoming_count: incoming,
            outgoing_count: outgoing,
            is_dead,
            hotspot_score,
        });
    }

    // Sort by priority descending
    candidates.sort_by(|a, b| {
        b.priority_score
            .partial_cmp(&a.priority_score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    // Apply max_symbols limit
    if config.max_symbols > 0 {
        candidates.truncate(config.max_symbols);
    }

    candidates
}

// ─── Prompt Construction ───────────────────────────────────────────────

const SYSTEM_PROMPT: &str = r#"You are a senior software architect analyzing code for a knowledge graph system.
For each symbol, provide a structured JSON assessment.

Respond ONLY with valid JSON matching this schema:
{
  "insights": [
    {
      "symbol_id": "<exact ID from input>",
      "smells": ["<CodeSmell>"],
      "patterns": ["<Pattern>"],
      "risk_score": <0-100>,
      "refactoring": "<one-line suggestion or null>",
      "summary": "<one-sentence architectural role>"
    }
  ]
}

Valid code smells: GodObject, FeatureEnvy, SrpViolation, LongMethod, DataClump, PrimitiveObsession, ShotgunSurgery
Valid patterns: Repository, Factory, Observer, Mediator, Strategy, Singleton, MVC, CQRS, Facade, Adapter, Decorator

Risk score guidelines:
- 0-20: Simple, well-isolated, low coupling
- 21-40: Moderate complexity, some coupling
- 41-60: Complex, many dependencies, potential SRP issues
- 61-80: High risk, god object traits, heavy coupling
- 81-100: Critical, untested + high churn + high complexity"#;

fn build_batch_prompt(batch: &[&EnrichmentCandidate]) -> Vec<Message> {
    let mut user_parts = Vec::new();
    user_parts.push(format!(
        "Analyze these {} symbols from the codebase:\n",
        batch.len()
    ));

    for (i, candidate) in batch.iter().enumerate() {
        let line_count = candidate
            .end_line
            .unwrap_or(0)
            .saturating_sub(candidate.start_line.unwrap_or(0));

        let lang = detect_language(&candidate.file_path);

        user_parts.push(format!(
            "=== Symbol {}: {} ===\n\
             Type: {:?}\n\
             File: {}\n\
             Lines: {}-{} ({} lines)\n\
             Coupling: {} incoming, {} outgoing edges\n\
             Dead code candidate: {}\n\
             Hotspot score: {:.2}\n\
             \n```{}\n{}\n```\n",
            i + 1,
            candidate.node_id,
            candidate.label,
            candidate.file_path,
            candidate.start_line.unwrap_or(0),
            candidate.end_line.unwrap_or(0),
            line_count,
            candidate.incoming_count,
            candidate.outgoing_count,
            if candidate.is_dead { "yes" } else { "no" },
            candidate.hotspot_score,
            lang,
            candidate.context_snippet,
        ));
    }

    vec![
        Message {
            role: Role::System,
            content: Some(format!("{}\n\n{}", PROMPT_CONTEXT_SAFETY, SYSTEM_PROMPT)),
            tool_calls: None,
            tool_call_id: None,
            name: None,
        },
        Message {
            role: Role::User,
            content: Some(user_parts.join("\n")),
            tool_calls: None,
            tool_call_id: None,
            name: None,
        },
    ]
}

fn detect_language(file_path: &str) -> &'static str {
    let lower = file_path.to_lowercase();
    if lower.ends_with(".cs") {
        "csharp"
    } else if lower.ends_with(".ts") || lower.ends_with(".tsx") {
        "typescript"
    } else if lower.ends_with(".js") || lower.ends_with(".jsx") {
        "javascript"
    } else if lower.ends_with(".py") {
        "python"
    } else if lower.ends_with(".rs") {
        "rust"
    } else if lower.ends_with(".java") {
        "java"
    } else if lower.ends_with(".go") {
        "go"
    } else if lower.ends_with(".rb") {
        "ruby"
    } else if lower.ends_with(".php") {
        "php"
    } else if lower.ends_with(".kt") || lower.ends_with(".kts") {
        "kotlin"
    } else if lower.ends_with(".swift") {
        "swift"
    } else if lower.ends_with(".c") || lower.ends_with(".h") {
        "c"
    } else if lower.ends_with(".cpp") || lower.ends_with(".hpp") || lower.ends_with(".cc") {
        "cpp"
    } else {
        ""
    }
}

// ─── Response Parsing ──────────────────────────────────────────────────

fn parse_llm_response(raw: &str) -> Option<BatchInsightResponse> {
    // Try direct parse first
    if let Ok(resp) = serde_json::from_str::<BatchInsightResponse>(raw) {
        return Some(resp);
    }

    // Try to find JSON in the response (LLM may wrap in markdown code block)
    let trimmed = raw.trim();
    let json_str = if let Some(start) = trimmed.find('{') {
        if let Some(end) = trimmed.rfind('}') {
            &trimmed[start..=end]
        } else {
            trimmed
        }
    } else {
        trimmed
    };

    serde_json::from_str::<BatchInsightResponse>(json_str).ok()
}

// ─── Apply Insights to Graph ───────────────────────────────────────────

fn apply_insights(
    graph: &mut KnowledgeGraph,
    insights: &[SymbolInsight],
    source_hashes: &HashMap<String, String>,
) -> usize {
    let mut applied = 0;
    for insight in insights {
        if let Some(node) = graph.get_node_mut(&insight.symbol_id) {
            let p = &mut node.properties;

            // Set enrichment marker
            p.enriched_by = Some(EnrichedBy::Llm);

            // Code smells
            if !insight.smells.is_empty() {
                p.llm_smells = Some(insight.smells.clone());
            }

            // Design patterns
            if !insight.patterns.is_empty() {
                p.llm_patterns = Some(insight.patterns.clone());
            }

            // Risk score (clamp to 0-100)
            p.llm_risk_score = Some(insight.risk_score.min(100));

            // Refactoring suggestion
            if let Some(ref refactoring) = insight.refactoring {
                if !refactoring.is_empty() {
                    p.llm_refactoring = Some(refactoring.clone());
                }
            }

            // Summary → description
            if let Some(ref summary) = insight.summary {
                if !summary.is_empty() {
                    p.description = Some(summary.clone());
                }
            }

            // Source hash for incrementality
            if let Some(hash) = source_hashes.get(&insight.symbol_id) {
                p.llm_source_hash = Some(hash.clone());
            }

            applied += 1;
        }
    }
    applied
}

// ─── Token Estimation ──────────────────────────────────────────────────

fn estimate_tokens(text: &str) -> u64 {
    // Rough heuristic: ~4 chars per token
    (text.len() as u64).div_ceil(4)
}

// ─── Main Entry Point ──────────────────────────────────────────────────

/// Run Phase 8: LLM Enrichment.
///
/// Selects high-priority symbols, batches them for LLM analysis,
/// and writes structured insights back to the graph.
pub async fn enrich_with_llm(
    graph: &mut KnowledgeGraph,
    repo_path: &Path,
    config: &LlmEnrichmentConfig,
    progress_tx: Option<&ProgressSender>,
) -> Result<LlmEnrichmentStats, crate::IngestError> {
    let mut stats = LlmEnrichmentStats::default();

    // Create LLM provider
    let provider = match OpenAILlmProvider::new(
        config.base_url.clone(),
        config.api_key.clone(),
        config.model.clone(),
        config.max_tokens,
        config.reasoning_effort.clone(),
    ) {
        Ok(p) => p,
        Err(e) => {
            warn!("Failed to create LLM provider: {e}");
            return Ok(stats);
        }
    };

    // Select and score candidates
    send_progress(progress_tx, 5.0, "Selecting enrichment candidates...");
    let candidates = select_candidates(graph, repo_path, config);
    stats.candidates_found = candidates.len();

    if candidates.is_empty() {
        info!("No enrichment candidates found (all cached or below threshold)");
        return Ok(stats);
    }

    info!(
        candidates = candidates.len(),
        top_priority = candidates.first().map(|c| c.priority_score).unwrap_or(0.0),
        "Selected enrichment candidates"
    );

    // Build source hash map for later application
    let source_hashes: HashMap<String, String> = candidates
        .iter()
        .map(|c| (c.node_id.clone(), c.source_hash.clone()))
        .collect();

    // Chunk into batches
    let batch_size = config.batch_size.clamp(1, 10);
    let batches: Vec<Vec<&EnrichmentCandidate>> = candidates
        .iter()
        .collect::<Vec<_>>()
        .chunks(batch_size)
        .map(|chunk| chunk.to_vec())
        .collect();

    let total_batches = batches.len();
    let mut tokens_used: u64 = 0;

    for (batch_idx, batch) in batches.iter().enumerate() {
        // Check token budget
        if tokens_used >= config.token_budget {
            info!(
                tokens_used,
                budget = config.token_budget,
                "Token budget exhausted, stopping enrichment"
            );
            break;
        }

        let pct = 5.0 + (batch_idx as f64 / total_batches as f64) * 90.0;
        send_progress(
            progress_tx,
            pct,
            &format!(
                "Enriching batch {}/{} ({} symbols)...",
                batch_idx + 1,
                total_batches,
                batch.len()
            ),
        );

        // Build prompt
        let messages = build_batch_prompt(batch);

        // Estimate input tokens
        let input_estimate: u64 = messages
            .iter()
            .filter_map(|m| m.content.as_ref())
            .map(|c| estimate_tokens(c))
            .sum();

        // Send to LLM
        let raw_response = match collect_completion(&provider, &messages).await {
            Ok(r) => r,
            Err(e) => {
                warn!(batch = batch_idx, error = %e, "LLM batch failed, skipping");
                stats.errors += 1;
                continue;
            }
        };

        let output_tokens = estimate_tokens(&raw_response);
        tokens_used += input_estimate + output_tokens;
        stats.batches_sent += 1;

        // Parse response
        let batch_response = match parse_llm_response(&raw_response) {
            Some(r) => r,
            None => {
                warn!(
                    batch = batch_idx,
                    raw_len = raw_response.len(),
                    "Failed to parse LLM response as JSON, skipping batch"
                );
                stats.errors += 1;
                continue;
            }
        };

        // Apply insights to graph
        let applied = apply_insights(graph, &batch_response.insights, &source_hashes);
        stats.symbols_enriched += applied;
    }

    stats.tokens_used_estimate = tokens_used;

    send_progress(
        progress_tx,
        100.0,
        &format!(
            "Enriched {} symbols ({} batches, ~{} tokens)",
            stats.symbols_enriched, stats.batches_sent, stats.tokens_used_estimate
        ),
    );

    Ok(stats)
}

/// Helper to send progress updates.
fn send_progress(tx: Option<&ProgressSender>, pct: f64, msg: &str) {
    use code_explorer_core::pipeline::types::{PipelinePhase, PipelineProgress};
    if let Some(tx) = tx {
        let _ = tx.send(PipelineProgress {
            phase: PipelinePhase::Enriching,
            percent: pct,
            message: msg.to_string(),
            detail: None,
            stats: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_llm_response_direct() {
        let json = r#"{"insights": [{"symbol_id": "Class:foo.cs:Bar", "smells": ["GodObject"], "patterns": ["Repository"], "risk_score": 75, "refactoring": "Split into two classes", "summary": "Main data access layer"}]}"#;
        let resp = parse_llm_response(json).expect("should parse");
        assert_eq!(resp.insights.len(), 1);
        assert_eq!(resp.insights[0].risk_score, 75);
        assert_eq!(resp.insights[0].smells, vec!["GodObject"]);
    }

    #[test]
    fn test_parse_llm_response_markdown_wrapped() {
        let json = "```json\n{\"insights\": [{\"symbol_id\": \"X\", \"smells\": [], \"patterns\": [], \"risk_score\": 10}]}\n```";
        let resp = parse_llm_response(json).expect("should parse markdown-wrapped");
        assert_eq!(resp.insights[0].risk_score, 10);
    }

    #[test]
    fn test_detect_language() {
        assert_eq!(detect_language("src/main.rs"), "rust");
        assert_eq!(detect_language("Controllers/HomeController.cs"), "csharp");
        assert_eq!(detect_language("app.tsx"), "typescript");
        assert_eq!(detect_language("unknown.xyz"), "");
    }

    #[test]
    fn test_estimate_tokens() {
        assert_eq!(estimate_tokens("hello world"), 3); // 11 chars / 4 ≈ 3
        assert_eq!(estimate_tokens(""), 0); // (0+3)/4 = 0 in integer division
        assert_eq!(estimate_tokens("abcd"), 1); // 4 chars / 4 = 1
    }

    #[test]
    fn test_is_enrichable_label() {
        assert!(is_enrichable_label(&NodeLabel::Class));
        assert!(is_enrichable_label(&NodeLabel::Controller));
        assert!(is_enrichable_label(&NodeLabel::Service));
        assert!(is_enrichable_label(&NodeLabel::Method));
        assert!(!is_enrichable_label(&NodeLabel::File));
        assert!(!is_enrichable_label(&NodeLabel::Folder));
        assert!(!is_enrichable_label(&NodeLabel::Community));
    }

    #[test]
    fn resolve_repo_file_accepts_files_inside_repo() {
        let root =
            std::env::temp_dir().join(format!("code-explorer-llm-enrich-inside-{}", std::process::id()));
        let repo = root.join("repo");
        std::fs::create_dir_all(repo.join("src")).unwrap();
        let file = repo.join("src").join("service.cs");
        std::fs::write(&file, "class Service {}").unwrap();

        let resolved = resolve_repo_file(&repo, "src/service.cs").expect("file should resolve");
        assert_eq!(resolved, file.canonicalize().unwrap());

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn resolve_repo_file_rejects_parent_escape() {
        let root =
            std::env::temp_dir().join(format!("code-explorer-llm-enrich-escape-{}", std::process::id()));
        let repo = root.join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        let outside = root.join("outside.cs");
        std::fs::write(&outside, "class Outside {}").unwrap();

        assert!(resolve_repo_file(&repo, "../outside.cs").is_none());

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn read_code_excerpt_rejects_absolute_outside_path() {
        let root = std::env::temp_dir().join(format!(
            "code-explorer-llm-enrich-absolute-{}",
            std::process::id()
        ));
        let repo = root.join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        let outside = root.join("outside.cs");
        std::fs::write(&outside, "class Outside {}").unwrap();

        let excerpt = read_code_excerpt(&repo, &outside.to_string_lossy(), 1, 2, 30);
        assert_eq!(excerpt, "(source unavailable)");

        let _ = std::fs::remove_dir_all(&root);
    }
}

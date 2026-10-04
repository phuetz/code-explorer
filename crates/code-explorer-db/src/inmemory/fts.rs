//! In-memory full-text search index with BM25 scoring.

use std::collections::{HashMap, HashSet};

use code_explorer_core::graph::types::NodeLabel;
use code_explorer_core::graph::KnowledgeGraph;

/// BM25 tuning constants.
const BM25_K1: f64 = 1.2;
const BM25_B: f64 = 0.75;
/// Name and path are short and decisive for an identifier. Prose lines carry
/// the words an agent actually asks about ("Current date", "Date actuelle").
const NAME_WEIGHT: f64 = 2.4;
const BODY_WEIGHT: f64 = 1.6;
/// Natural-language results: at most this many hits from the same file, then
/// backfill. One chatty file must not occupy the whole first page.
const MAX_HITS_PER_FILE: usize = 2;

/// Labels that are indexed for full-text search.
const FTS_LABELS: &[NodeLabel] = &[
    NodeLabel::Function,
    NodeLabel::Class,
    NodeLabel::Method,
    NodeLabel::Interface,
    NodeLabel::File,
    NodeLabel::Struct,
    NodeLabel::Trait,
    NodeLabel::Enum,
    NodeLabel::Variable,
    NodeLabel::Type,
    NodeLabel::Const,
    NodeLabel::Module,
    NodeLabel::Route,
    NodeLabel::Tool,
    NodeLabel::ApiEndpoint,
    NodeLabel::CodeElement,
    // Prose: a Markdown/rst heading is the searchable symbol of a document.
    NodeLabel::Section,
    // ASP.NET MVC searchable labels
    NodeLabel::Controller,
    NodeLabel::ControllerAction,
    NodeLabel::View,
    NodeLabel::ScriptFile,
    NodeLabel::UiComponent,
    NodeLabel::Service,
    NodeLabel::Repository,
    NodeLabel::ExternalService,
];

/// A single FTS search result.
#[derive(Debug, Clone)]
pub struct FtsResult {
    pub node_id: String,
    pub score: f64,
    pub name: String,
    pub file_path: String,
    pub label: String,
    pub start_line: Option<u32>,
    pub end_line: Option<u32>,
}

/// One field of a node: the short name+path, or a single prose line.
struct FieldPosting {
    node_id: String,
    tf: u32,
    len: u32,
    is_name: bool,
}

/// In-memory inverted index with BM25 scoring.
///
/// Name and path are one short field. Each prose line (string literal, template
/// line, or leading comment) is its own short field, so a date line inside a
/// long prompt is not diluted by the rest of the body.
pub struct FtsIndex {
    /// term -> postings (a node may appear once for the name and once per line)
    inverted: HashMap<String, Vec<FieldPosting>>,
    /// term -> number of distinct nodes that contain it
    doc_freq: HashMap<String, u32>,
    /// Number of indexed nodes.
    doc_count: usize,
    avg_name_len: f64,
    avg_line_len: f64,
}

impl FtsIndex {
    /// Create an empty FTS index.
    pub fn new() -> Self {
        Self {
            inverted: HashMap::new(),
            doc_freq: HashMap::new(),
            doc_count: 0,
            avg_name_len: 1.0,
            avg_line_len: 1.0,
        }
    }

    /// Build an FTS index from a `KnowledgeGraph`.
    ///
    /// Indexes the `name` and `file_path` as one short field, and each line of
    /// `description` (or, failing that, the keyword list) as its own field.
    pub fn build(graph: &KnowledgeGraph) -> Self {
        let mut inverted: HashMap<String, Vec<FieldPosting>> = HashMap::new();
        let mut doc_count: usize = 0;
        let mut name_len_sum: u64 = 0;
        let mut name_docs: u64 = 0;
        let mut line_len_sum: u64 = 0;
        let mut line_docs: u64 = 0;

        for node in graph.iter_nodes() {
            if !FTS_LABELS.contains(&node.label) {
                continue;
            }
            doc_count += 1;
            let node_id = node.id.clone();

            let name_text = format!("{} {}", node.properties.name, node.properties.file_path);
            let name_tokens = tokenize(&name_text);
            if !name_tokens.is_empty() {
                let len = name_tokens.len() as u32;
                name_len_sum += u64::from(len);
                name_docs += 1;
                push_field(&mut inverted, &node_id, &name_tokens, len, true);
            }

            let lines = prose_fields(node);
            for line in lines {
                let tokens = tokenize(&line);
                if tokens.is_empty() {
                    continue;
                }
                let len = tokens.len() as u32;
                line_len_sum += u64::from(len);
                line_docs += 1;
                push_field(&mut inverted, &node_id, &tokens, len, false);
            }
        }

        let mut doc_freq: HashMap<String, u32> = HashMap::new();
        for (term, postings) in &inverted {
            let mut ids: Vec<&str> = postings.iter().map(|p| p.node_id.as_str()).collect();
            ids.sort_unstable();
            ids.dedup();
            doc_freq.insert(term.clone(), ids.len() as u32);
        }

        let avg_name_len = if name_docs > 0 {
            (name_len_sum as f64 / name_docs as f64).max(1.0)
        } else {
            1.0
        };
        let avg_line_len = if line_docs > 0 {
            (line_len_sum as f64 / line_docs as f64).max(1.0)
        } else {
            1.0
        };

        Self {
            inverted,
            doc_freq,
            doc_count,
            avg_name_len,
            avg_line_len,
        }
    }

    /// Search the index with a query string.
    ///
    /// `table_filter` can be a label name (e.g. `"Function"`) to restrict results.
    /// Returns up to `limit` results sorted by BM25 score descending.
    pub fn search(
        &self,
        graph: &KnowledgeGraph,
        query: &str,
        table_filter: Option<&str>,
        limit: usize,
    ) -> Vec<FtsResult> {
        self.search_with_file_type(graph, query, table_filter, None, limit)
    }

    /// Search with an optional source extension (for example `rs`, `.ts`, `cs`).
    pub fn search_with_file_type(
        &self,
        graph: &KnowledgeGraph,
        query: &str,
        table_filter: Option<&str>,
        file_type: Option<&str>,
        limit: usize,
    ) -> Vec<FtsResult> {
        let query_tokens = tokenize(query);
        if query_tokens.is_empty() {
            return Vec::new();
        }
        let identifier_query = !query.chars().any(char::is_whitespace);
        let mut terms = query_tokens.clone();
        if !identifier_query {
            let mut extra = Vec::new();
            for token in &query_tokens {
                for expansion in expand_token(token) {
                    if !terms.iter().any(|existing| existing == expansion)
                        && !extra.iter().any(|existing| existing == expansion)
                    {
                        extra.push((*expansion).to_string());
                    }
                }
            }
            // "mot(s) de passe" is one concept, not three independent words.
            if query_tokens.iter().any(|t| t == "mot" || t == "mots")
                && query_tokens.iter().any(|t| t == "passe")
            {
                for expansion in ["password", "secret", "redact", "redaction"] {
                    if !terms.iter().any(|existing| existing == expansion) {
                        terms.push(expansion.to_string());
                    }
                }
            }
            terms.extend(extra);
        }

        let mut name_score: HashMap<&str, f64> = HashMap::new();
        let mut line_score: HashMap<&str, f64> = HashMap::new();
        let mut matched_original: HashMap<&str, u32> = HashMap::new();

        for term in &terms {
            let Some(postings) = self.inverted.get(term.as_str()) else {
                continue;
            };
            let df = f64::from(*self.doc_freq.get(term.as_str()).unwrap_or(&1));
            let idf = ((self.doc_count as f64 - df + 0.5) / (df + 0.5) + 1.0)
                .ln()
                .max(0.0);
            let is_original = query_tokens.iter().any(|token| token == term);
            let mut best_line: HashMap<&str, f64> = HashMap::new();
            let mut name_add: HashMap<&str, f64> = HashMap::new();
            let mut seen: HashSet<&str> = HashSet::new();
            for posting in postings {
                let avg = if posting.is_name {
                    self.avg_name_len
                } else {
                    self.avg_line_len
                };
                let bm25 = idf * bm25_tf(f64::from(posting.tf), f64::from(posting.len.max(1)), avg);
                let id = posting.node_id.as_str();
                seen.insert(id);
                if posting.is_name {
                    *name_add.entry(id).or_insert(0.0) += bm25 * NAME_WEIGHT;
                } else {
                    let weighted = bm25 * BODY_WEIGHT;
                    let slot = best_line.entry(id).or_insert(0.0);
                    if weighted > *slot {
                        *slot = weighted;
                    }
                }
            }
            for (id, score) in name_add {
                *name_score.entry(id).or_insert(0.0) += score;
            }
            for (id, score) in best_line {
                *line_score.entry(id).or_insert(0.0) += score;
            }
            if is_original {
                for id in seen {
                    *matched_original.entry(id).or_insert(0) += 1;
                }
            }
        }

        let required = query_tokens.len() as u32;
        let mut ids: HashSet<&str> = HashSet::new();
        ids.extend(name_score.keys().copied());
        ids.extend(line_score.keys().copied());
        let mut scores: HashMap<&str, f64> = HashMap::new();
        for id in ids {
            let hits = *matched_original.get(id).unwrap_or(&0);
            if identifier_query && hits < required {
                continue;
            }
            let base = name_score.get(id).copied().unwrap_or(0.0)
                + line_score.get(id).copied().unwrap_or(0.0);
            let coordination = 1.0 + 0.35 * f64::from(hits);
            scores.insert(id, base * coordination);
        }

        // Apply relevance weighting (path + label) BEFORE sorting, then sort.
        // We look up every candidate node once to read its label + file_path —
        // O(n) cost, n = distinct docs that matched any query term.
        let mut weighted: Vec<(&str, f64, &code_explorer_core::graph::types::GraphNode)> = scores
            .into_iter()
            .filter_map(|(node_id, score)| {
                let node = graph.get_node(node_id)?;
                // Apply table filter early so we don't weight nodes we'll drop.
                if let Some(filter) = table_filter {
                    if node.label.as_str() != filter {
                        return None;
                    }
                }
                if let Some(extension) = file_type {
                    let extension = extension.trim_start_matches('.');
                    if extension.is_empty()
                        || !node
                            .properties
                            .file_path
                            .to_ascii_lowercase()
                            .ends_with(&format!(".{}", extension.to_ascii_lowercase()))
                    {
                        return None;
                    }
                }
                let weighted_score = score
                    * path_weight(&node.properties.file_path, &query_tokens)
                    * test_intent_weight(&node.properties.file_path, &query_tokens)
                    * label_weight(node.label)
                    * entry_intent_weight(node, &query_tokens);
                Some((node_id, weighted_score, node))
            })
            .collect();

        weighted.sort_by(|a, b| {
            b.1.partial_cmp(&a.1)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.2.properties.file_path.cmp(&b.2.properties.file_path))
                .then_with(|| a.0.cmp(b.0))
        });

        let chosen = diversify_files(&weighted, limit, !identifier_query);
        chosen
            .into_iter()
            .map(|(node_id, score, node)| FtsResult {
                node_id: node_id.to_string(),
                score,
                name: node.properties.name.clone(),
                file_path: node.properties.file_path.clone(),
                label: node.label.as_str().to_string(),
                start_line: node.properties.start_line,
                end_line: node.properties.end_line,
            })
            .collect()
    }
}

fn push_field(
    inverted: &mut HashMap<String, Vec<FieldPosting>>,
    node_id: &str,
    tokens: &[String],
    len: u32,
    is_name: bool,
) {
    let mut tf: HashMap<&str, u32> = HashMap::new();
    for token in tokens {
        *tf.entry(token.as_str()).or_insert(0) += 1;
    }
    for (term, freq) in tf {
        inverted
            .entry(term.to_string())
            .or_default()
            .push(FieldPosting {
                node_id: node_id.to_string(),
                tf: freq,
                len,
                is_name,
            });
    }
}

fn prose_fields(node: &code_explorer_core::graph::types::GraphNode) -> Vec<String> {
    if let Some(description) = &node.properties.description {
        let lines: Vec<String> = description
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .take(48)
            .map(str::to_string)
            .collect();
        if !lines.is_empty() {
            return lines;
        }
    }
    if let Some(keywords) = &node.properties.keywords {
        if !keywords.is_empty() {
            return vec![keywords.join(" ")];
        }
    }
    Vec::new()
}

fn bm25_tf(tf: f64, doc_len: f64, avg_len: f64) -> f64 {
    let numerator = tf * (BM25_K1 + 1.0);
    let denominator = tf + BM25_K1 * (1.0 - BM25_B + BM25_B * doc_len / avg_len.max(1.0));
    numerator / denominator
}

fn diversify_files<'a>(
    ranked: &[(
        &'a str,
        f64,
        &'a code_explorer_core::graph::types::GraphNode,
    )],
    limit: usize,
    enabled: bool,
) -> Vec<(
    &'a str,
    f64,
    &'a code_explorer_core::graph::types::GraphNode,
)> {
    if !enabled {
        return ranked.iter().take(limit).copied().collect();
    }
    let mut out = Vec::new();
    let mut counts: HashMap<&str, usize> = HashMap::new();
    let mut skipped = Vec::new();
    for item in ranked {
        let file = item.2.properties.file_path.as_str();
        let count = counts.entry(file).or_insert(0);
        if *count >= MAX_HITS_PER_FILE {
            skipped.push(*item);
            continue;
        }
        *count += 1;
        out.push(*item);
        if out.len() == limit {
            return out;
        }
    }
    for item in skipped {
        out.push(item);
        if out.len() == limit {
            break;
        }
    }
    out
}

/// French and English words an agent uses for the same code concept.
/// Keys are already accent-folded. Not a file list: the same pairs apply to
/// any repository.
fn expand_token(token: &str) -> &'static [&'static str] {
    match token {
        "heure" | "heures" => &["time"],
        "time" | "times" => &["heure"],
        "boucle" | "boucles" => &["loop", "loops"],
        "loop" | "loops" => &["boucle"],
        "tache" | "taches" => &["task", "tasks"],
        "task" | "tasks" => &["tache", "taches"],
        "arriere" | "arriereplan" => &["background"],
        "background" => &["arriere"],
        "sable" => &["sandbox", "sandboxed"],
        "sandbox" | "sandboxed" => &["sable"],
        "environnement" => &["environment", "env"],
        "environment" => &["environnement", "env"],
        "variable" | "variables" => &["env"],
        "bloquee" | "bloquees" | "bloque" | "bloques" => &["block", "blocked", "blocklist"],
        "blocked" | "blocklist" => &["bloque"],
        "commande" | "commandes" => &["command", "commands"],
        "command" | "commands" => &["commande"],
        "inconnue" | "inconnu" | "inconnues" => &["unknown"],
        "unknown" => &["inconnu"],
        "interactif" | "interactifs" | "interactive" => &["interactive", "interactif"],
        "nettoyage" | "nettoyer" | "nettoie" => &["redact", "redaction"],
        "redact" | "redaction" | "redacted" => &["nettoyage"],
        "affichage" | "afficher" => &["display", "output"],
        "chargement" | "charger" | "charge" => &["load", "loader", "loading"],
        "load" | "loaded" | "loader" | "loading" => &["chargement"],
        "injectee" | "injectees" | "injected" | "injection" | "inject" => &["inject", "prompt"],
        "prompt" | "prompts" => &["prompt", "prompts"],
        "config" | "configuration" => &["config"],
        "bash" => &["shell"],
        "shell" => &["bash"],
        _ => &[],
    }
}

/// Favor a declared CLI branch when the question names that command and asks
/// about CLI dispatch. Generic function names can otherwise bury the entry.
fn entry_intent_weight(
    node: &code_explorer_core::graph::types::GraphNode,
    tokens: &[String],
) -> f64 {
    if node.label != NodeLabel::CodeElement
        || !matches!(
            node.properties.framework.as_deref(),
            Some("clap" | "commander")
        )
        || !tokens.iter().any(|token| {
            matches!(
                token.as_str(),
                "cli" | "commande" | "command" | "commands" | "dispatch" | "dispatche" | "dispatché"
            )
        })
    {
        return 1.0;
    }
    let command = node
        .properties
        .name
        .strip_prefix("Commands::")
        .unwrap_or(&node.properties.name);
    let parts = tokenize(command);
    if !parts.is_empty() && parts.iter().all(|part| tokens.contains(part)) {
        20.0
    } else {
        1.0
    }
}

/// Deprioritize minified assets and third-party library bundles so business
/// code wins over `jquery-ui.min.js` on generic queries. Returns a multiplier
/// in `[0.1, 1.0]`.
///
/// Exposed so callers that score nodes outside `FtsIndex::search` (e.g.
/// `chat::search_relevant_context`'s name-match pass) can apply the same
/// penalty consistently.
pub fn path_weight(file_path: &str, query_tokens: &[String]) -> f64 {
    let lc = file_path.to_ascii_lowercase();

    // Substring patterns — anywhere in the path. Good for file-extension
    // markers and fragment matches.
    const SUBSTRING_PENALIZE: &[&str] = &[
        // Minified assets
        ".min.js",
        ".min.css",
        ".min.map",
        "-min.js",
        "-min.css",
        // Visual Studio doc-comment stubs
        "-vsdoc.js",
        ".vsdoc.js",
        // Generated sources (EF6, designer, XAML-gen)
        ".designer.cs",
        ".g.cs",
        ".g.i.cs",
        // Common third-party script bundles (match both fragments and prefix dirs)
        "scripts/jquery",
        "scripts/knockout",
        "scripts/kendo",
        "scripts/telerik",
        "scripts/angular",
        "scripts/bootstrap",
        "scripts/modernizr",
        "scripts/moment",
        "scripts/history",
    ];
    if SUBSTRING_PENALIZE.iter().any(|p| lc.contains(p)) {
        return 0.1;
    }

    // Directory-name patterns — must match a full path component (split on
    // `/` or `\`), so `mypackages/` doesn't trigger the `packages` rule.
    const DIR_PENALIZE: &[&str] = &[
        "packages",
        "node_modules",
        "bower_components",
        "vendor",
        "obj",
        "bin",
    ];
    let is_sep = |c: char| c == '/' || c == '\\';
    if lc.split(is_sep).any(|comp| DIR_PENALIZE.contains(&comp)) {
        return 0.1;
    }

    // Special case: `wwwroot/lib/` third-party drop (ASP.NET static assets).
    if lc.contains("wwwroot/lib/") || lc.contains("wwwroot\\lib\\") {
        return 0.1;
    }

    let test_words = ["test", "tests", "testing", "tested", "spec", "specs"];
    if is_test_file(&lc) && !query_tokens.iter().any(|t| test_words.contains(&t.as_str())) {
        return 0.3;
    }

    1.0
}

fn is_test_file(path: &str) -> bool {
    path.split(['/', '\\'])
        .any(|part| matches!(part, "test" | "tests" | "__tests__"))
        || path.ends_with("tests.cs")
        || path.ends_with("test.cs")
        || path.ends_with("_test.rs")
        || path.ends_with("_test.go")
        || path.contains(".test.")
        || path.contains(".spec.")
}

fn test_intent_weight(path: &str, tokens: &[String]) -> f64 {
    if is_test_file(&path.to_ascii_lowercase())
        && tokens.iter().any(|token| {
            matches!(
                token.as_str(),
                "test" | "tests" | "testing" | "tested" | "spec" | "specs"
            )
        })
    {
        4.0
    } else {
        1.0
    }
}

/// Boost business-logic labels (Controller, Service, Method, Class…) over
/// meta-nodes that often win BM25 purely through name-token frequency
/// (ScriptFile, Import, ExternalService).
fn label_weight(label: NodeLabel) -> f64 {
    match label {
        // Core domain code
        NodeLabel::Class
        | NodeLabel::Method
        | NodeLabel::Function
        | NodeLabel::Constructor
        | NodeLabel::Interface
        | NodeLabel::Controller
        | NodeLabel::ControllerAction
        | NodeLabel::Service
        | NodeLabel::Repository
        | NodeLabel::Route => 1.5,
        // Noisy / lightweight
        NodeLabel::ScriptFile
        | NodeLabel::Import
        | NodeLabel::ExternalService
        | NodeLabel::UiComponent => 0.4,
        _ => 1.0,
    }
}

impl Default for FtsIndex {
    fn default() -> Self {
        Self::new()
    }
}

/// Tokenize text: accent-fold, split camelCase and on non-alphanumeric characters.
pub fn tokenize(text: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current_token = String::new();
    let mut last_was_uppercase = false;

    for c in text.chars() {
        if !c.is_alphanumeric() {
            if !current_token.is_empty() {
                tokens.push(std::mem::take(&mut current_token));
            }
            last_was_uppercase = false;
            continue;
        }
        let is_upper = c.is_uppercase();
        if is_upper && !current_token.is_empty() && !last_was_uppercase {
            tokens.push(std::mem::take(&mut current_token));
        }
        current_token.push(fold_char(c));
        last_was_uppercase = is_upper;
    }
    if !current_token.is_empty() {
        tokens.push(current_token);
    }
    tokens
}

fn fold_char(c: char) -> char {
    let c = c.to_lowercase().next().unwrap_or(c);
    match c {
        'à' | 'á' | 'â' | 'ä' | 'ã' | 'å' => 'a',
        'è' | 'é' | 'ê' | 'ë' => 'e',
        'ì' | 'í' | 'î' | 'ï' => 'i',
        'ò' | 'ó' | 'ô' | 'ö' | 'õ' => 'o',
        'ù' | 'ú' | 'û' | 'ü' => 'u',
        'ý' | 'ÿ' => 'y',
        'ç' => 'c',
        'ñ' => 'n',
        _ => c,
    }
}

/// Extract a table/label filter from FTS table names like `"fts_Function"`.
pub fn parse_fts_table_filter(table_name: &str) -> Option<String> {
    table_name
        .strip_prefix("fts_")
        .map(|stripped| stripped.to_string())
}

/// Convert an `FtsResult` to a `serde_json::Value` row.
///
/// Field names are camelCase to match the Cypher RETURN aliases used by
/// `code-explorer-search::bm25::build_fts_query` (`nodeId`, `filePath`, etc.).
/// Previously this used `node_id` (snake_case) which silently broke the
/// `parse_fts_row` consumer in bm25.rs — every BM25SearchResult from the
/// in-memory backend had an empty `node_id` string, breaking downstream
/// node lookups and the RRF hybrid merge keying.
pub fn fts_result_to_json(r: &FtsResult) -> serde_json::Value {
    let mut map = serde_json::Map::new();
    map.insert(
        "nodeId".to_string(),
        serde_json::Value::String(r.node_id.clone()),
    );
    map.insert("score".to_string(), serde_json::json!(r.score));
    map.insert(
        "name".to_string(),
        serde_json::Value::String(r.name.clone()),
    );
    map.insert(
        "filePath".to_string(),
        serde_json::Value::String(r.file_path.clone()),
    );
    map.insert(
        "label".to_string(),
        serde_json::Value::String(r.label.clone()),
    );
    if let Some(sl) = r.start_line {
        map.insert("startLine".to_string(), serde_json::json!(sl));
    }
    if let Some(el) = r.end_line {
        map.insert("endLine".to_string(), serde_json::json!(el));
    }
    serde_json::Value::Object(map)
}

#[cfg(test)]
mod tests {
    use super::*;
    use code_explorer_core::graph::types::*;

    fn make_test_graph() -> KnowledgeGraph {
        let mut g = KnowledgeGraph::new();
        g.add_node(GraphNode {
            id: "Function:src/auth.ts:handleLogin".to_string(),
            label: NodeLabel::Function,
            properties: NodeProperties {
                name: "handleLogin".to_string(),
                file_path: "src/auth.ts".to_string(),
                start_line: Some(10),
                end_line: Some(30),
                ..Default::default()
            },
        });
        g.add_node(GraphNode {
            id: "Function:src/auth.ts:validateToken".to_string(),
            label: NodeLabel::Function,
            properties: NodeProperties {
                name: "validateToken".to_string(),
                file_path: "src/auth.ts".to_string(),
                start_line: Some(35),
                end_line: Some(50),
                ..Default::default()
            },
        });
        g.add_node(GraphNode {
            id: "Class:src/user.ts:UserService".to_string(),
            label: NodeLabel::Class,
            properties: NodeProperties {
                name: "UserService".to_string(),
                file_path: "src/user.ts".to_string(),
                start_line: Some(1),
                end_line: Some(100),
                ..Default::default()
            },
        });
        g
    }

    #[test]
    fn test_tokenize() {
        let tokens = tokenize("handleLogin src/auth.ts");
        assert_eq!(tokens, vec!["handle", "login", "src", "auth", "ts"]);
    }

    #[test]
    fn test_build_and_search() {
        let graph = make_test_graph();
        let index = FtsIndex::build(&graph);

        assert!(index.doc_count > 0);

        let results = index.search(&graph, "handleLogin", None, 10);
        assert!(!results.is_empty());
        assert_eq!(results[0].name, "handleLogin");
    }

    #[test]
    fn entry_points_are_searchable_by_route_and_command() {
        let mut graph = KnowledgeGraph::new();
        for (id, label, name, path) in [
            (
                "endpoint",
                NodeLabel::ApiEndpoint,
                "POST /api/chat/stream",
                "src/Program.cs",
            ),
            (
                "command",
                NodeLabel::CodeElement,
                "Commands::Compress",
                "src/main.rs",
            ),
        ] {
            graph.add_node(GraphNode {
                id: id.into(),
                label,
                properties: NodeProperties {
                    name: name.into(),
                    file_path: path.into(),
                    framework: if id == "command" {
                        Some("clap".into())
                    } else {
                        Some("aspnet-minimal".into())
                    },
                    ..Default::default()
                },
            });
        }
        graph.add_node(GraphNode {
            id: "noise".into(),
            label: NodeLabel::Function,
            properties: NodeProperties {
                name: "cli_compress_dispatch_test".into(),
                file_path: "tests/cli.rs".into(),
                ..Default::default()
            },
        });
        let index = FtsIndex::build(&graph);
        assert!(index
            .search(&graph, "/api/chat/stream", None, 10)
            .iter()
            .any(|result| result.node_id == "endpoint"));
        assert!(index
            .search(&graph, "Commands::Compress", None, 10)
            .iter()
            .any(|result| result.node_id == "command"));
        assert_eq!(
            index.search(
                &graph,
                "Où le sous-programme CLI Compress est-il dispatché ?",
                None,
                10
            )[0]
            .node_id,
            "command"
        );
    }

    #[test]
    fn test_search_with_table_filter() {
        let graph = make_test_graph();
        let index = FtsIndex::build(&graph);

        let results = index.search(&graph, "auth", Some("Function"), 10);
        for r in &results {
            assert_eq!(r.label, "Function");
        }
    }

    #[test]
    fn test_parse_fts_table_filter() {
        assert_eq!(
            parse_fts_table_filter("fts_Function"),
            Some("Function".to_string())
        );
        assert_eq!(
            parse_fts_table_filter("fts_Class"),
            Some("Class".to_string())
        );
        assert_eq!(parse_fts_table_filter("other"), None);
    }

    #[test]
    fn test_empty_query() {
        let graph = make_test_graph();
        let index = FtsIndex::build(&graph);
        let results = index.search(&graph, "", None, 10);
        assert!(results.is_empty());
    }

    #[test]
    fn test_bm25_scoring() {
        let graph = make_test_graph();
        let index = FtsIndex::build(&graph);

        // A more specific query should rank the exact match higher
        let results = index.search(&graph, "handleLogin", None, 10);
        assert!(!results.is_empty());
        assert!(results[0].score > 0.0);
    }

    #[test]
    fn test_path_weight_penalizes_minified() {
        let q: Vec<String> = vec![];
        assert_eq!(
            path_weight("Acme.Sample.ihm/Scripts/jquery-1.7.1.min.js", &q),
            0.1
        );
        assert_eq!(
            path_weight("packages/jQuery.1.7.1.1/Content/Scripts/jquery-1.7.1.js", &q),
            0.1
        );
        assert_eq!(path_weight("node_modules/react/index.js", &q), 0.1);
        assert_eq!(
            path_weight("Acme.Sample.BAL/Facture/InvoiceService.cs", &q),
            1.0
        );
        assert_eq!(path_weight("src/main.rs", &q), 1.0);
    }

    #[test]
    fn source_file_wins_over_test_for_a_generic_question() {
        let q: Vec<String> = vec![];
        assert!(
            path_weight("src/SqlitePragmaInterceptor.cs", &q)
                > path_weight("tests/SqlitePragmaInterceptorTests.cs", &q)
        );
        assert!(path_weight("src/agent-executor.ts", &q) > path_weight("src/agent-executor.test.ts", &q));
        assert!(path_weight("src/main.rs", &q) > path_weight("src/main_test.rs", &q));
    }

    #[test]
    fn test_substring_does_not_lift_test_penalty() {
        let q: Vec<String> = vec!["latest".to_string(), "invoice".to_string()];
        assert_eq!(path_weight("tests/SqlitePragmaInterceptorTests.cs", &q), 0.3);
    }

    #[test]
    fn test_explicit_test_query_lifts_penalty() {
        let q: Vec<String> = vec!["how".to_string(), "to".to_string(), "test".to_string()];
        assert_eq!(path_weight("tests/SqlitePragmaInterceptorTests.cs", &q), 1.0);
    }

    #[test]
    fn test_label_weight_boosts_business_code() {
        assert!(label_weight(NodeLabel::Method) > label_weight(NodeLabel::ScriptFile));
        assert!(label_weight(NodeLabel::Controller) > 1.0);
        assert!(label_weight(NodeLabel::Service) > 1.0);
        assert!(label_weight(NodeLabel::ScriptFile) < 1.0);
    }

    #[test]
    fn test_business_code_wins_over_minified_on_same_score() {
        // Two nodes with the same raw BM25 (name = "paiement" in both);
        // one is a .cs Method, the other a minified .js file. After weighting
        // the Method must rank first.
        let mut g = KnowledgeGraph::new();
        g.add_node(GraphNode {
            id: "Method:BAL/Facture/InvoiceService.cs:Paiement".into(),
            label: NodeLabel::Method,
            properties: NodeProperties {
                name: "Paiement".into(),
                file_path: "Acme.Sample.BAL/Facture/InvoiceService.cs".into(),
                ..Default::default()
            },
        });
        g.add_node(GraphNode {
            id: "ScriptFile:Scripts/telerik-Paiement.min.js:x".into(),
            label: NodeLabel::ScriptFile,
            properties: NodeProperties {
                name: "Paiement".into(),
                file_path: "Acme.Sample.ihm/Scripts/telerik-Paiement.min.js".into(),
                ..Default::default()
            },
        });
        let idx = FtsIndex::build(&g);
        let results = idx.search(&g, "Paiement", None, 10);
        assert!(!results.is_empty());
        assert_eq!(
            results[0].label, "Method",
            "business code must outrank minified"
        );
    }

    #[test]
    fn test_tokenize_folds_accents_like_the_unaccented_form() {
        assert_eq!(tokenize("détection"), tokenize("detection"));
        assert_eq!(tokenize("injectées"), vec!["injectees".to_string()]);
    }

    #[test]
    fn camel_case_identifier_does_not_match_a_partial_sibling() {
        let mut graph = KnowledgeGraph::new();
        graph.add_node(GraphNode {
            id: "Class:src/old.rs:OldSymbol".into(),
            label: NodeLabel::Class,
            properties: NodeProperties {
                name: "OldSymbol".into(),
                file_path: "src/old.rs".into(),
                ..Default::default()
            },
        });
        graph.add_node(GraphNode {
            id: "Method:src/old.rs:old_method".into(),
            label: NodeLabel::Method,
            properties: NodeProperties {
                name: "old_method".into(),
                file_path: "src/old.rs".into(),
                ..Default::default()
            },
        });
        let index = FtsIndex::build(&graph);
        let results = index.search(&graph, "OldSymbol", None, 10);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "OldSymbol");
    }

    #[test]
    fn french_date_question_ranks_the_system_prompt_file() {
        let mut graph = KnowledgeGraph::new();
        let mut filler = String::new();
        for i in 0..24 {
            filler.push_str(&format!("identity rule {i} stay concise do not repeat\n"));
        }
        filler.push_str("- Current date and time zone\n");
        filler.push_str("the value is injected into the system prompt\n");
        graph.add_node(GraphNode {
            id: "fn-system".into(),
            label: NodeLabel::Function,
            properties: NodeProperties {
                name: "getBaseSystemPrompt".into(),
                file_path: "src/prompts/system-base.ts".into(),
                description: Some(filler),
                ..Default::default()
            },
        });
        for i in 0..6 {
            graph.add_node(GraphNode {
                id: format!("clock-{i}"),
                label: NodeLabel::Function,
                properties: NodeProperties {
                    name: format!("promptHelper{i}"),
                    file_path: format!("src/noise/prompt-{i}.ts"),
                    description: Some("date helper for a prompt".into()),
                    ..Default::default()
                },
            });
        }
        graph.add_node(GraphNode {
            id: "voice".into(),
            label: NodeLabel::Function,
            properties: NodeProperties {
                name: "voiceClockPromptBlock".into(),
                file_path: "src/sensory/voice-clock.ts".into(),
                description: Some("Il est l'heure et la date du jour".into()),
                ..Default::default()
            },
        });
        let index = FtsIndex::build(&graph);
        let french = index.search(
            &graph,
            "date et heure injectées dans le prompt",
            None,
            5,
        );
        assert!(
            french
                .iter()
                .any(|hit| hit.file_path == "src/prompts/system-base.ts"),
            "system prompt file missing from {:?}",
            french
                .iter()
                .map(|hit| hit.file_path.as_str())
                .collect::<Vec<_>>()
        );
        let english = index.search(
            &graph,
            "where is the current date and time injected into the system prompt",
            None,
            5,
        );
        assert_eq!(english[0].file_path, "src/prompts/system-base.ts");
    }
}

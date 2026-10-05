//! In-memory full-text search index with BM25 scoring.

use std::collections::HashMap;

use code_explorer_core::graph::types::NodeLabel;
use code_explorer_core::graph::KnowledgeGraph;

/// BM25 tuning constants.
const BM25_K1: f64 = 1.2;
const BM25_B: f64 = 0.75;

/// Term-frequency weight of a token that only comes from the description
/// (documentation excerpt). Name and path tokens weigh 1.0: the description
/// adds recall for questions phrased like the docs without letting a symbol
/// whose documentation merely mentions a word outrank the symbol named so.
const DESCRIPTION_TF_WEIGHT: f64 = 0.5;

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

/// In-memory inverted index with BM25 scoring.
pub struct FtsIndex {
    /// term -> Vec<(node_id, term_frequency)>
    inverted: HashMap<String, Vec<(String, f64)>>,
    /// node_id -> weighted document length (description tokens count
    /// `DESCRIPTION_TF_WEIGHT`)
    doc_lengths: HashMap<String, f64>,
    /// Total number of indexed documents.
    doc_count: usize,
    /// Average document length across the corpus.
    avg_doc_len: f64,
}

impl FtsIndex {
    /// Create an empty FTS index.
    pub fn new() -> Self {
        Self {
            inverted: HashMap::new(),
            doc_lengths: HashMap::new(),
            doc_count: 0,
            avg_doc_len: 0.0,
        }
    }

    /// Build an FTS index from a `KnowledgeGraph`.
    ///
    /// Indexes the `name` and `file_path` properties for nodes whose labels
    /// are in `FTS_LABELS`.
    pub fn build(graph: &KnowledgeGraph) -> Self {
        let mut inverted: HashMap<String, Vec<(String, f64)>> = HashMap::new();
        let mut doc_lengths: HashMap<String, f64> = HashMap::new();
        let mut doc_count: usize = 0;
        let mut total_tokens: f64 = 0.0;

        for node in graph.iter_nodes() {
            if !FTS_LABELS.contains(&node.label) {
                continue;
            }

            // Build the document from name + file_path, plus the optional
            // description at a lower term weight.
            let text = format!("{} {}", node.properties.name, node.properties.file_path);
            let tokens = tokenize(&text);
            let desc_tokens = node
                .properties
                .description
                .as_deref()
                .map(tokenize)
                .unwrap_or_default();
            let doc_len = tokens.len() as f64 + DESCRIPTION_TF_WEIGHT * desc_tokens.len() as f64;

            doc_lengths.insert(node.id.clone(), doc_len);
            doc_count += 1;
            total_tokens += doc_len;

            // Count (weighted) term frequencies for this document
            let mut tf_map: HashMap<&str, f64> = HashMap::new();
            for token in &tokens {
                *tf_map.entry(token.as_str()).or_insert(0.0) += 1.0;
            }
            for token in &desc_tokens {
                *tf_map.entry(token.as_str()).or_insert(0.0) += DESCRIPTION_TF_WEIGHT;
            }

            for (term, tf) in tf_map {
                inverted
                    .entry(term.to_string())
                    .or_default()
                    .push((node.id.clone(), tf));
            }
        }

        let avg_doc_len = if doc_count > 0 {
            (total_tokens / doc_count as f64).max(1.0)
        } else {
            1.0
        };

        Self {
            inverted,
            doc_lengths,
            doc_count,
            avg_doc_len,
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
        let query_tokens = tokenize_query(query);
        if query_tokens.is_empty() {
            return Vec::new();
        }

        // Accumulate BM25 scores per document
        let mut scores: HashMap<&str, f64> = HashMap::new();

        for token in &query_tokens {
            if let Some(postings) = self.inverted.get(token.as_str()) {
                let df = postings.len() as f64;
                let idf = ((self.doc_count as f64 - df + 0.5) / (df + 0.5) + 1.0)
                    .ln()
                    .max(0.0);

                for (doc_id, tf) in postings {
                    let doc_len = *self.doc_lengths.get(doc_id.as_str()).unwrap_or(&1.0);
                    let tf_f = *tf;
                    let numerator = tf_f * (BM25_K1 + 1.0);
                    let denominator =
                        tf_f + BM25_K1 * (1.0 - BM25_B + BM25_B * doc_len / self.avg_doc_len);
                    let bm25 = idf * numerator / denominator;

                    *scores.entry(doc_id.as_str()).or_insert(0.0) += bm25;
                }
            }
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
                    * path_weight(&node.properties.file_path)
                    * test_intent_weight(&node.properties.file_path, &query_tokens)
                    * auxiliary_dir_weight(&node.properties.file_path, &query_tokens)
                    * label_weight(node.label)
                    * entry_intent_weight(node, &query_tokens)
                    * name_match_weight(&node.properties.name, &query_tokens);
                Some((node_id, weighted_score, node))
            })
            .collect();

        weighted.sort_by(|a, b| {
            b.1.partial_cmp(&a.1)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.2.properties.file_path.cmp(&b.2.properties.file_path))
                .then_with(|| a.0.cmp(b.0))
        });

        weighted
            .into_iter()
            .take(limit)
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
                "cli" | "commande" | "command" | "dispatch" | "dispatché"
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
pub fn path_weight(file_path: &str) -> f64 {
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
    // `packages/` is a NuGet restore folder only when its children are
    // versioned package directories (`packages/jQuery.1.7.1.1/...`). In a
    // JavaScript/TypeScript monorepo (`packages/reactivity/src/...`) it holds
    // the project's own source and must not be buried.
    let components: Vec<&str> = lc.split(is_sep).collect();
    if components
        .windows(2)
        .any(|pair| pair[0] == "packages" && is_versioned_package_dir(pair[1]))
    {
        return 0.1;
    }

    // Special case: `wwwroot/lib/` third-party drop (ASP.NET static assets).
    if lc.contains("wwwroot/lib/") || lc.contains("wwwroot\\lib\\") {
        return 0.1;
    }

    if is_test_file(&lc) {
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
        || path.contains(".test-d.")
        || path.contains(".spec.")
}

/// `packages/<Name>.<major>.<minor>...` — a NuGet package restored on disk.
fn is_versioned_package_dir(component: &str) -> bool {
    component
        .split('.')
        .skip(1)
        .any(|part| !part.is_empty() && part.chars().all(|c| c.is_ascii_digit()))
}

/// Benchmarks and examples call the public API by its name, so they compete
/// with the implementation on every exact-name question. Halve them unless
/// the question asks for an example or a benchmark.
fn auxiliary_dir_weight(path: &str, tokens: &[String]) -> f64 {
    let lc = path.to_ascii_lowercase();
    let auxiliary = lc.split(['/', '\\']).any(|part| {
        matches!(
            part,
            "bench" | "benches" | "benchmark" | "benchmarks" | "example" | "examples"
        )
    });
    if !auxiliary {
        return 1.0;
    }
    if tokens
        .iter()
        .any(|t| matches!(t.as_str(), "bench" | "benchmark" | "example"))
    {
        1.0
    } else {
        0.5
    }
}

/// BM25 runs over `name + file_path`, so a path-only match (every symbol of
/// `sync/broadcast.rs` contains `broadcast`) ties with a name match. Reward
/// the share of distinct query terms found in the symbol name itself:
/// multiplier in `[1.0, 2.0]`.
fn name_match_weight(name: &str, tokens: &[String]) -> f64 {
    let mut distinct: Vec<&str> = tokens.iter().map(String::as_str).collect();
    distinct.sort_unstable();
    distinct.dedup();
    if distinct.is_empty() {
        return 1.0;
    }
    let name_tokens = tokenize(name);
    let matched = distinct
        .iter()
        .filter(|t| name_tokens.iter().any(|n| n == *t))
        .count();
    1.0 + matched as f64 / distinct.len() as f64
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

/// Tokenize text for BM25.
///
/// Every word (a run of alphanumerics and `_`) is split on `_` and on
/// camelCase / PascalCase humps, so `nextTick`, `next_tick` and `NextTick`
/// all yield `next` + `tick`. A multi-part identifier also keeps its joined
/// form (`next_tick`), so an exact identifier still outranks loose words.
/// Each part goes through a light English stemmer (`ticks`, `ticking` ->
/// `tick`). Identical rules run on documents and queries.
fn tokenize(text: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    for word in raw_words(text) {
        push_word_tokens(word, &mut tokens, false);
    }
    tokens
}

/// Tokenize a search query: like [`tokenize`], but drops standalone English
/// and French function words. Symbol names and paths almost never contain
/// `the` or `a`, which gives them a high IDF: left in, they make one-letter
/// symbols and long test names win natural-language questions. Parts of an
/// identifier (`block_on`, `isRef`) are kept. Falls back to the unfiltered
/// tokens when only stopwords remain.
fn tokenize_query(query: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    for word in raw_words(query) {
        push_word_tokens(word, &mut tokens, true);
    }
    if tokens.is_empty() {
        tokenize(query)
    } else {
        tokens
    }
}

fn raw_words(text: &str) -> impl Iterator<Item = &str> {
    text.split(|c: char| !(c.is_alphanumeric() || c == '_'))
        .filter(|s| !s.is_empty())
}

fn push_word_tokens(word: &str, tokens: &mut Vec<String>, drop_stopwords: bool) {
    let parts = identifier_parts(word);
    if parts.is_empty() {
        return;
    }
    if parts.len() == 1 {
        if drop_stopwords && is_stopword(&parts[0]) {
            return;
        }
        tokens.push(stem(&parts[0]));
        return;
    }
    for part in &parts {
        tokens.push(stem(part));
    }
    tokens.push(parts.join("_"));
}

/// Split one word on `_` and camelCase humps, lowercased.
/// `HTMLParser` -> `html`, `parser`; `toRefs` -> `to`, `refs`.
fn identifier_parts(word: &str) -> Vec<String> {
    let mut parts = Vec::new();
    for chunk in word.split('_').filter(|c| !c.is_empty()) {
        let chars: Vec<char> = chunk.chars().collect();
        let mut current = String::new();
        for (i, &c) in chars.iter().enumerate() {
            if i > 0 && c.is_uppercase() {
                let prev = chars[i - 1];
                let next_is_lower = chars.get(i + 1).is_some_and(|n| n.is_lowercase());
                if prev.is_lowercase()
                    || prev.is_ascii_digit()
                    || (prev.is_uppercase() && next_is_lower)
                {
                    if !current.is_empty() {
                        parts.push(std::mem::take(&mut current));
                    }
                }
            }
            current.extend(c.to_lowercase());
        }
        if !current.is_empty() {
            parts.push(current);
        }
    }
    parts
}

fn is_stopword(word: &str) -> bool {
    matches!(
        word,
        // English
        "a" | "an" | "the" | "is" | "are" | "was" | "were" | "be" | "been" | "being"
            | "to" | "of" | "in" | "on" | "at" | "by" | "for" | "from" | "with" | "into"
            | "and" | "or" | "it" | "its" | "this" | "that" | "these" | "those" | "where"
            | "what" | "which" | "who" | "whom" | "how" | "why" | "when" | "do" | "does"
            | "did" | "can" | "could" | "should" | "would" | "will" | "there" | "here"
            | "as" | "so" | "such" | "than" | "then" | "my" | "our" | "your" | "their"
            | "we" | "you" | "they" | "i" | "me" | "us"
            // French
            | "le" | "la" | "les" | "l" | "un" | "une" | "des" | "du" | "de" | "d"
            | "et" | "ou" | "où" | "est" | "sont" | "que" | "qui" | "quoi" | "comment"
            | "quel" | "quelle" | "quels" | "quelles" | "dans" | "sur" | "pour" | "par"
            | "avec" | "ce" | "cette" | "ces" | "se" | "sa" | "son" | "ses" | "au"
            | "aux" | "il" | "elle" | "ne" | "pas"
    )
}

/// Conservative English suffix stripping, applied to lowercase ASCII words
/// only (identifiers in other scripts, digits and short words pass through).
/// Not a full Porter stemmer: it only needs to map the same word family to
/// the same key on both the query and the document side.
fn stem(word: &str) -> String {
    if word.len() <= 3 || !word.bytes().all(|b| b.is_ascii_lowercase()) {
        return word.to_string();
    }
    let mut w = word.to_string();
    // Plurals.
    if w.ends_with("ies") && w.len() > 4 {
        w.truncate(w.len() - 3);
        w.push('y');
    } else if w.ends_with("sses") {
        w.truncate(w.len() - 2);
    } else if w.ends_with('s')
        && !w.ends_with("ss")
        && !w.ends_with("us")
        && !w.ends_with("is")
    {
        w.truncate(w.len() - 1);
    }
    // Derivational / inflectional suffixes, repeated so `renderer` and
    // `render` (or `rendering`) meet on the same key.
    for _ in 0..3 {
        let before = w.len();
        strip_suffix(&mut w);
        if w.len() == before {
            break;
        }
    }
    if w.len() > 4 && w.ends_with('e') {
        w.truncate(w.len() - 1);
    }
    w
}

fn strip_suffix(w: &mut String) {
    const RULES: &[(&str, &str)] = &[
        ("ization", "iz"),
        ("ation", "at"),
        ("ator", "at"),
        ("tion", "t"),
        ("sion", "s"),
        ("ing", ""),
        ("ed", ""),
        ("er", ""),
    ];
    for (suffix, replacement) in RULES {
        if let Some(stem) = w.strip_suffix(suffix) {
            if stem.len() + replacement.len() < 4 {
                return;
            }
            let mut next = format!("{stem}{replacement}");
            if replacement.is_empty() {
                // `running` -> `runn` -> `run`, `dotted` -> `dot`.
                let b = next.as_bytes();
                let n = b.len();
                if n >= 2 && b[n - 1] == b[n - 2] && !matches!(b[n - 1], b'l' | b's' | b'z') {
                    next.truncate(n - 1);
                }
            }
            *w = next;
            return;
        }
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
        assert_eq!(
            tokens,
            vec!["handl", "login", "handle_login", "src", "auth", "ts"]
        );
    }

    #[test]
    fn camel_and_snake_identifiers_share_parts_and_compound() {
        assert_eq!(tokenize("nextTick"), tokenize("next_tick"));
        assert_eq!(tokenize("NextTick"), vec!["next", "tick", "next_tick"]);
        assert_eq!(
            tokenize("HTMLParser"),
            vec!["html", "pars", "html_parser"]
        );
        assert_eq!(tokenize("escapeHtml.ts"), vec!["escap", "html", "escape_html", "ts"]);
    }

    #[test]
    fn light_stemmer_maps_word_families_to_one_key() {
        assert_eq!(stem("ticks"), stem("tick"));
        assert_eq!(stem("tracking"), stem("track"));
        assert_eq!(stem("validation"), stem("validator"));
        assert_eq!(stem("validator"), stem("validate"));
        assert_eq!(stem("renderer"), stem("render"));
        assert_eq!(stem("dependencies"), stem("dependency"));
        assert_eq!(stem("blocking"), "block");
        // Short words, non-ASCII words and words whose stem would be too
        // short stay as they are.
        assert_eq!(stem("string"), "string");
        assert_eq!(stem("rs"), "rs");
        assert_eq!(stem("dispatché"), "dispatché");
    }

    #[test]
    fn query_drops_standalone_stopwords_but_keeps_identifier_parts() {
        assert_eq!(
            tokenize_query("where is the timeout of a future"),
            vec!["timeout", "futur"]
        );
        assert_eq!(tokenize_query("block_on"), vec!["block", "on", "block_on"]);
        assert_eq!(tokenize_query("où est le cache"), vec!["cach"]);
        // Only stopwords: fall back to every token rather than to nothing.
        assert_eq!(tokenize_query("the"), vec!["the"]);
    }

    fn node(id: &str, label: NodeLabel, name: &str, path: &str) -> GraphNode {
        GraphNode {
            id: id.into(),
            label,
            properties: NodeProperties {
                name: name.into(),
                file_path: path.into(),
                ..Default::default()
            },
        }
    }

    #[test]
    fn one_letter_symbol_no_longer_wins_a_question_through_a_stopword() {
        let mut g = KnowledgeGraph::new();
        g.add_node(node("a", NodeLabel::Function, "a", "examples/dump.rs"));
        g.add_node(node("t", NodeLabel::Function, "timeout", "src/time/timeout.rs"));
        let idx = FtsIndex::build(&g);
        let r = idx.search(&g, "where is the timeout applied to a future", None, 10);
        assert_eq!(r[0].node_id, "t");
        assert!(r.iter().all(|x| x.node_id != "a"));
    }

    #[test]
    fn camel_case_symbol_matches_spaced_words() {
        let mut g = KnowledgeGraph::new();
        g.add_node(node("e", NodeLabel::Function, "escapeHtml", "packages/shared/src/escapeHtml.ts"));
        g.add_node(node("h", NodeLabel::Function, "html", "packages/vue/__tests__/e2e/utils.ts"));
        let idx = FtsIndex::build(&g);
        let r = idx.search(&g, "escape HTML special characters", None, 10);
        assert_eq!(r[0].node_id, "e");
    }

    #[test]
    fn name_match_outranks_path_only_match() {
        let mut g = KnowledgeGraph::new();
        g.add_node(node("c", NodeLabel::Function, "receiver_count", "src/sync/broadcast.rs"));
        g.add_node(node("b", NodeLabel::Function, "channel", "src/sync/broadcast.rs"));
        let idx = FtsIndex::build(&g);
        let r = idx.search(&g, "broadcast channel", None, 10);
        assert_eq!(r[0].node_id, "b");
    }

    #[test]
    fn monorepo_packages_are_source_but_nuget_packages_are_not() {
        assert_eq!(path_weight("packages/reactivity/src/reactive.ts"), 1.0);
        assert_eq!(path_weight("packages/Newtonsoft.Json.13.0.1/lib/x.cs"), 0.1);
        assert_eq!(path_weight("src/packages/jQuery.1.7.1.1/jquery.js"), 0.1);
    }

    #[test]
    fn description_adds_recall_but_name_still_wins() {
        let mut g = KnowledgeGraph::new();
        let mut sleep = node("sleep", NodeLabel::Function, "sleep", "src/time/sleep.rs");
        sleep.properties.description = Some("Waits until duration has elapsed.".into());
        g.add_node(sleep);
        let mut other = node("other", NodeLabel::Function, "block_in_place", "src/task/blocking.rs");
        other.properties.description = Some("Runs code; prefer spawn_blocking for long work.".into());
        g.add_node(other);
        g.add_node(node("spawn", NodeLabel::Function, "spawn_blocking", "src/task/blocking.rs"));
        let idx = FtsIndex::build(&g);
        assert_eq!(
            idx.search(&g, "wait until a duration has elapsed", None, 10)[0].node_id,
            "sleep"
        );
        assert_eq!(idx.search(&g, "spawn_blocking", None, 10)[0].node_id, "spawn");
    }

    #[test]
    fn type_test_files_are_tests() {
        assert_eq!(
            path_weight("packages-private/dts-test/defineComponent.test-d.tsx"),
            0.3
        );
    }

    #[test]
    fn examples_and_benches_yield_to_the_implementation() {
        let mut g = KnowledgeGraph::new();
        g.add_node(node("bench", NodeLabel::Function, "spawn_blocking", "benches/spawn_blocking.rs"));
        g.add_node(node("impl", NodeLabel::Function, "spawn_blocking", "src/task/blocking.rs"));
        let idx = FtsIndex::build(&g);
        assert_eq!(idx.search(&g, "spawn_blocking", None, 10)[0].node_id, "impl");
        assert_eq!(
            idx.search(&g, "spawn_blocking benchmark", None, 10)[0].node_id,
            "bench"
        );
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
        assert_eq!(
            path_weight("Acme.Sample.ihm/Scripts/jquery-1.7.1.min.js"),
            0.1
        );
        assert_eq!(
            path_weight("packages/jQuery.1.7.1.1/Content/Scripts/jquery-1.7.1.js"),
            0.1
        );
        assert_eq!(path_weight("node_modules/react/index.js"), 0.1);
        assert_eq!(
            path_weight("Acme.Sample.BAL/Facture/InvoiceService.cs"),
            1.0
        );
        assert_eq!(path_weight("src/main.rs"), 1.0);
    }

    #[test]
    fn source_file_wins_over_test_for_a_generic_question() {
        assert!(
            path_weight("src/SqlitePragmaInterceptor.cs")
                > path_weight("tests/SqlitePragmaInterceptorTests.cs")
        );
        assert!(path_weight("src/agent-executor.ts") > path_weight("src/agent-executor.test.ts"));
        assert!(path_weight("src/main.rs") > path_weight("src/main_test.rs"));
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
}

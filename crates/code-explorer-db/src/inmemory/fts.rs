//! In-memory full-text search index with BM25 scoring.

use std::collections::HashMap;

use code_explorer_core::graph::types::NodeLabel;
use code_explorer_core::graph::KnowledgeGraph;

/// BM25 tuning constants.
const BM25_K1: f64 = 1.2;
const BM25_B: f64 = 0.75;

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
    inverted: HashMap<String, Vec<(String, u32)>>,
    /// node_id -> document length (number of tokens)
    doc_lengths: HashMap<String, u32>,
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
        let mut inverted: HashMap<String, Vec<(String, u32)>> = HashMap::new();
        let mut doc_lengths: HashMap<String, u32> = HashMap::new();
        let mut doc_count: usize = 0;
        let mut total_tokens: usize = 0;

        for node in graph.iter_nodes() {
            if !FTS_LABELS.contains(&node.label) {
                continue;
            }

            // Build document text from name + file_path + optional description
            let mut text = format!("{} {}", node.properties.name, node.properties.file_path);
            if let Some(ref desc) = node.properties.description {
                text.push(' ');
                text.push_str(desc);
            }
            let tokens = tokenize(&text);
            let doc_len = tokens.len() as u32;

            doc_lengths.insert(node.id.clone(), doc_len);
            doc_count += 1;
            total_tokens += tokens.len();

            // Count term frequencies for this document
            let mut tf_map: HashMap<&str, u32> = HashMap::new();
            for token in &tokens {
                *tf_map.entry(token.as_str()).or_insert(0) += 1;
            }

            for (term, tf) in tf_map {
                inverted
                    .entry(term.to_string())
                    .or_default()
                    .push((node.id.clone(), tf));
            }
        }

        let avg_doc_len = if doc_count > 0 {
            (total_tokens as f64 / doc_count as f64).max(1.0)
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
        let query_tokens = tokenize(query);
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
                    let doc_len = *self.doc_lengths.get(doc_id.as_str()).unwrap_or(&1) as f64;
                    let tf_f = *tf as f64;
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
                let weighted_score =
                    score * path_weight(&node.properties.file_path) * label_weight(node.label);
                Some((node_id, weighted_score, node))
            })
            .collect();

        weighted.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

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

    1.0
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

/// Tokenize text: lowercase, split on non-alphanumeric characters, filter empty tokens.
fn tokenize(text: &str) -> Vec<String> {
    text.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .collect()
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
        assert_eq!(tokens, vec!["handlelogin", "src", "auth", "ts"]);
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
        assert_eq!(path_weight("Acme.Sample.BAL/Facture/FactureService.cs"), 1.0);
        assert_eq!(path_weight("src/main.rs"), 1.0);
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
            id: "Method:BAL/Facture/FactureService.cs:Paiement".into(),
            label: NodeLabel::Method,
            properties: NodeProperties {
                name: "Paiement".into(),
                file_path: "Acme.Sample.BAL/Facture/FactureService.cs".into(),
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

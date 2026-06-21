//! Duplicate-code / clone detection via Rabin-Karp rolling hash on
//! normalized tokens.
//!
//! Approach:
//! 1. For each `Method` / `Function` / `Constructor` node, read its source
//!    from disk using `file_path` + `start_line` / `end_line`.
//! 2. Tokenize: split on non-alphanumeric boundaries; normalize identifiers
//!    → `ID`, string/number literals → `LIT`; skip pure whitespace &
//!    comments.
//! 3. Slide a window of `min_tokens` over the token stream; compute a
//!    polynomial rolling hash per window.
//! 4. Group nodes by window-hash. Any bucket with ≥ 2 distinct nodes is a
//!    clone cluster.
//! 5. Compute pairwise similarity (Jaccard on the token multiset) and filter
//!    by threshold (default 0.9).
//!
//! Fast path: under the hood this is essentially a hash-based nearest-neighbor
//! search. It's O(total_tokens) time and O(buckets) memory — cheap enough to
//! run on every repo.

use std::collections::HashMap;
use std::path::Path;

use code_explorer_core::graph::types::NodeLabel;
use code_explorer_core::graph::KnowledgeGraph;
use serde::{Deserialize, Serialize};

/// Options for clone detection.
#[derive(Debug, Clone, Copy)]
pub struct CloneOptions {
    /// Minimum window size in tokens. Default: 30.
    pub min_tokens: usize,
    /// Jaccard similarity threshold [0.0, 1.0]. Default: 0.9.
    pub threshold: f64,
    /// Maximum clusters to return. Default: 100.
    pub max_clusters: usize,
}

impl Default for CloneOptions {
    fn default() -> Self {
        Self {
            min_tokens: 30,
            threshold: 0.9,
            max_clusters: 100,
        }
    }
}

/// One member of a clone cluster.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CloneMember {
    pub node_id: String,
    pub name: String,
    pub file_path: String,
    pub start_line: Option<u32>,
    pub end_line: Option<u32>,
    pub token_count: usize,
    /// Preview snippet (first ~8 lines).
    pub snippet: String,
}

/// A group of methods/functions that appear to be clones of each other.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CloneCluster {
    pub cluster_id: String,
    pub members: Vec<CloneMember>,
    /// Average pairwise Jaccard similarity across members (0.0 – 1.0).
    pub similarity: f64,
    /// Window size used for the hash match.
    pub min_tokens: usize,
}

/// Detect clone clusters within the repo rooted at `repo_path`.
///
/// `repo_path` is used to resolve each node's `file_path` (which is relative).
/// If a file can't be read, the node is silently skipped.
pub fn find_clones(
    graph: &KnowledgeGraph,
    repo_path: &Path,
    opts: CloneOptions,
) -> Vec<CloneCluster> {
    // 1. Collect callable nodes.
    let callables: Vec<&code_explorer_core::graph::types::GraphNode> = graph
        .iter_nodes()
        .filter(|n| {
            matches!(
                n.label,
                NodeLabel::Method | NodeLabel::Function | NodeLabel::Constructor
            )
        })
        .collect();

    if callables.is_empty() {
        return Vec::new();
    }

    // 2. Read source ranges, tokenize, normalize.
    // Cache file contents to avoid re-reading the same file N times.
    let mut file_cache: HashMap<String, Option<String>> = HashMap::new();

    let mut prepared: Vec<Prepared> = Vec::with_capacity(callables.len());
    for node in callables {
        let file_rel = &node.properties.file_path;
        if file_rel.is_empty() {
            continue;
        }
        let content = file_cache
            .entry(file_rel.clone())
            .or_insert_with(|| {
                let full = repo_path.join(file_rel);
                std::fs::read_to_string(full).ok()
            })
            .clone();
        let Some(content) = content else { continue };

        let start = node.properties.start_line.unwrap_or(1).max(1) as usize;
        let end = node.properties.end_line.unwrap_or(start as u32) as usize;
        if end < start {
            continue;
        }

        let lines: Vec<&str> = content.lines().collect();
        if start > lines.len() {
            continue;
        }
        let slice_end = end.min(lines.len());
        let slice = &lines[(start - 1)..slice_end];
        let source = slice.join("\n");

        let tokens = normalize_tokens(&source);
        if tokens.len() < opts.min_tokens {
            continue;
        }

        // 8-line preview
        let preview_end = slice.len().min(8);
        let snippet = slice[..preview_end].join("\n");

        prepared.push(Prepared {
            node_id: node.id.clone(),
            name: node.properties.name.clone(),
            file_path: node.properties.file_path.clone(),
            start_line: node.properties.start_line,
            end_line: node.properties.end_line,
            tokens,
            snippet,
        });
    }

    if prepared.len() < 2 {
        return Vec::new();
    }

    // 3. Bucket nodes by windowed rolling hash. Any distinct nodes sharing
    //    at least one hash are potential clones.
    // hash -> set of node indices
    let mut buckets: HashMap<u64, Vec<usize>> = HashMap::new();
    for (idx, p) in prepared.iter().enumerate() {
        for h in rolling_hashes(&p.tokens, opts.min_tokens) {
            let entry = buckets.entry(h).or_default();
            // Avoid pushing the same node twice per bucket.
            if entry.last().copied() != Some(idx) {
                entry.push(idx);
            }
        }
    }

    // 4. Build a union-find over node indices using every shared bucket.
    let mut uf = UnionFind::new(prepared.len());
    for indices in buckets.values() {
        if indices.len() < 2 {
            continue;
        }
        let pivot = indices[0];
        for &other in &indices[1..] {
            uf.union(pivot, other);
        }
    }

    // 5. Collect clusters, filter by threshold.
    let mut groups: HashMap<usize, Vec<usize>> = HashMap::new();
    for i in 0..prepared.len() {
        let r = uf.find(i);
        groups.entry(r).or_default().push(i);
    }

    let mut clusters: Vec<CloneCluster> = Vec::new();
    for (cluster_id_seed, indices) in groups {
        if indices.len() < 2 {
            continue;
        }

        // Pairwise Jaccard on token multisets (cheap enough for typical
        // cluster sizes; bail out if a cluster grows very large).
        let similarity = average_jaccard(&indices, &prepared);
        if similarity < opts.threshold {
            continue;
        }

        let members: Vec<CloneMember> = indices
            .iter()
            .map(|&i| {
                let p = &prepared[i];
                CloneMember {
                    node_id: p.node_id.clone(),
                    name: p.name.clone(),
                    file_path: p.file_path.clone(),
                    start_line: p.start_line,
                    end_line: p.end_line,
                    token_count: p.tokens.len(),
                    snippet: p.snippet.clone(),
                }
            })
            .collect();

        clusters.push(CloneCluster {
            cluster_id: format!("clone_{}", cluster_id_seed),
            members,
            similarity,
            min_tokens: opts.min_tokens,
        });
    }

    // Sort: biggest clusters first, then highest similarity.
    clusters.sort_by(|a, b| {
        b.members.len().cmp(&a.members.len()).then_with(|| {
            b.similarity
                .partial_cmp(&a.similarity)
                .unwrap_or(std::cmp::Ordering::Equal)
        })
    });

    clusters.truncate(opts.max_clusters);
    clusters
}

/// Tokenize source and normalize identifiers/literals.
fn normalize_tokens(src: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut in_string = false;
    let mut string_quote = '"';
    let mut in_line_comment = false;
    let mut in_block_comment = false;
    let mut prev_char = '\0';

    for c in src.chars() {
        // Handle comments
        if in_line_comment {
            if c == '\n' {
                in_line_comment = false;
            }
            prev_char = c;
            continue;
        }
        if in_block_comment {
            if prev_char == '*' && c == '/' {
                in_block_comment = false;
            }
            prev_char = c;
            continue;
        }

        // Handle strings
        if in_string {
            if c == string_quote && prev_char != '\\' {
                in_string = false;
                // Every completed string literal becomes a single LIT token,
                // regardless of content (content was skipped during the scan).
                tokens.push("LIT".to_string());
                current.clear();
            }
            prev_char = c;
            continue;
        }

        // Detect start of comment
        if prev_char == '/' && c == '/' {
            in_line_comment = true;
            current.pop(); // drop the first '/'
            prev_char = c;
            continue;
        }
        if prev_char == '/' && c == '*' {
            in_block_comment = true;
            current.pop();
            prev_char = c;
            continue;
        }

        // String start
        if c == '"' || c == '\'' || c == '`' {
            flush_token(&mut current, &mut tokens);
            in_string = true;
            string_quote = c;
            prev_char = c;
            continue;
        }

        // Separator
        if c.is_whitespace() {
            flush_token(&mut current, &mut tokens);
            prev_char = c;
            continue;
        }

        if !c.is_alphanumeric() && c != '_' {
            flush_token(&mut current, &mut tokens);
            // Keep non-alphanumeric punctuation as its own token so structural
            // differences matter — `a.b` and `a->b` should not hash-collide.
            tokens.push(c.to_string());
            prev_char = c;
            continue;
        }

        current.push(c);
        prev_char = c;
    }

    flush_token(&mut current, &mut tokens);
    tokens
}

fn flush_token(buf: &mut String, out: &mut Vec<String>) {
    if buf.is_empty() {
        return;
    }
    // Classify: pure digits = LIT, otherwise IDentifier (but keep common
    // keywords unchanged so real structural differences count).
    let t: String = if buf.chars().all(|c| c.is_ascii_digit() || c == '.') {
        "LIT".to_string()
    } else if is_keyword(buf) {
        buf.clone()
    } else {
        "ID".to_string()
    };
    out.push(t);
    buf.clear();
}

fn is_keyword(s: &str) -> bool {
    // Non-exhaustive but covers the structural markers we want to preserve
    // across multiple languages. Missing a keyword just makes it look like
    // an identifier — no false positives, maybe a few missed clones.
    matches!(
        s,
        "if" | "else"
            | "for"
            | "while"
            | "do"
            | "return"
            | "break"
            | "continue"
            | "switch"
            | "case"
            | "default"
            | "try"
            | "catch"
            | "finally"
            | "throw"
            | "new"
            | "this"
            | "super"
            | "class"
            | "function"
            | "def"
            | "fn"
            | "let"
            | "const"
            | "var"
            | "public"
            | "private"
            | "protected"
            | "static"
            | "async"
            | "await"
            | "yield"
            | "match"
            | "in"
            | "of"
            | "as"
            | "is"
            | "null"
            | "true"
            | "false"
            | "None"
            | "True"
            | "False"
    )
}

/// Compute all `window`-sized rolling hashes over `tokens`.
fn rolling_hashes(tokens: &[String], window: usize) -> Vec<u64> {
    if tokens.len() < window {
        return Vec::new();
    }
    const BASE: u64 = 131;
    const MOD: u64 = 1_000_000_007;

    // Convert each token to a small numeric code via fxhash-ish folding.
    let codes: Vec<u64> = tokens.iter().map(|t| token_code(t)).collect();

    // Precompute BASE^window mod MOD for the rolling removal step.
    let mut base_pow_w = 1u64;
    for _ in 0..window {
        base_pow_w = (base_pow_w * BASE) % MOD;
    }

    let mut hashes = Vec::with_capacity(codes.len() - window + 1);
    let mut h = 0u64;
    for &c in codes.iter().take(window) {
        h = (h * BASE + c) % MOD;
    }
    hashes.push(h);

    for i in window..codes.len() {
        let out = codes[i - window];
        // Subtract base_pow_w * out mod MOD (add MOD before subtract to avoid underflow).
        let sub = (out * base_pow_w) % MOD;
        h = (h + MOD - sub) % MOD;
        h = (h * BASE + codes[i]) % MOD;
        hashes.push(h);
    }

    hashes
}

fn token_code(t: &str) -> u64 {
    // Tiny deterministic hash (FNV-1a).
    let mut h: u64 = 0xcbf29ce484222325;
    for b in t.as_bytes() {
        h ^= *b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h % 1_000_000_007
}

/// Average pairwise Jaccard similarity over a cluster of token sequences.
fn average_jaccard(indices: &[usize], prepared: &[Prepared]) -> f64 {
    if indices.len() < 2 {
        return 0.0;
    }
    let mut total = 0.0;
    let mut count = 0;
    for i in 0..indices.len() {
        for j in (i + 1)..indices.len() {
            total += jaccard(&prepared[indices[i]].tokens, &prepared[indices[j]].tokens);
            count += 1;
        }
        // Cap at ~50 comparisons for huge clusters.
        if count >= 50 {
            break;
        }
    }
    if count == 0 {
        0.0
    } else {
        total / count as f64
    }
}

struct Prepared {
    node_id: String,
    name: String,
    file_path: String,
    start_line: Option<u32>,
    end_line: Option<u32>,
    tokens: Vec<String>,
    snippet: String,
}

fn jaccard(a: &[String], b: &[String]) -> f64 {
    use std::collections::HashSet;
    let sa: HashSet<&String> = a.iter().collect();
    let sb: HashSet<&String> = b.iter().collect();
    let inter = sa.intersection(&sb).count();
    let union = sa.union(&sb).count();
    if union == 0 {
        0.0
    } else {
        inter as f64 / union as f64
    }
}

/// Disjoint-set / union-find.
struct UnionFind {
    parent: Vec<usize>,
    rank: Vec<u32>,
}

impl UnionFind {
    fn new(n: usize) -> Self {
        Self {
            parent: (0..n).collect(),
            rank: vec![0; n],
        }
    }

    fn find(&mut self, x: usize) -> usize {
        if self.parent[x] != x {
            self.parent[x] = self.find(self.parent[x]);
        }
        self.parent[x]
    }

    fn union(&mut self, a: usize, b: usize) {
        let ra = self.find(a);
        let rb = self.find(b);
        if ra == rb {
            return;
        }
        if self.rank[ra] < self.rank[rb] {
            self.parent[ra] = rb;
        } else if self.rank[ra] > self.rank[rb] {
            self.parent[rb] = ra;
        } else {
            self.parent[rb] = ra;
            self.rank[ra] += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_identifiers() {
        let toks = normalize_tokens("let foo = bar + 42;");
        // Expect: let ID = ID + LIT ;
        assert!(toks.contains(&"let".to_string()));
        assert!(toks.contains(&"ID".to_string()));
        assert!(toks.contains(&"LIT".to_string()));
    }

    #[test]
    fn test_normalize_strings() {
        let toks = normalize_tokens("let msg = \"hello\";");
        // String literal → LIT
        assert!(toks.iter().any(|t| t == "LIT"));
    }

    #[test]
    fn test_normalize_comments_skipped() {
        let toks = normalize_tokens("let x = 1; // this is a comment\nlet y = 2;");
        let comment_text: Vec<&String> = toks.iter().filter(|t| t.contains("comment")).collect();
        assert!(comment_text.is_empty());
    }

    #[test]
    fn test_rolling_hashes_empty_when_too_short() {
        let tokens = vec!["a".to_string(), "b".to_string()];
        let hashes = rolling_hashes(&tokens, 5);
        assert!(hashes.is_empty());
    }

    #[test]
    fn test_rolling_hashes_count() {
        let tokens: Vec<String> = (0..10).map(|i| format!("t{i}")).collect();
        let hashes = rolling_hashes(&tokens, 3);
        assert_eq!(hashes.len(), 8);
    }

    #[test]
    fn test_jaccard_identical() {
        let a = vec!["x".to_string(), "y".to_string(), "z".to_string()];
        let b = a.clone();
        assert!((jaccard(&a, &b) - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_jaccard_disjoint() {
        let a = vec!["x".to_string()];
        let b = vec!["y".to_string()];
        assert!(jaccard(&a, &b).abs() < f64::EPSILON);
    }
}

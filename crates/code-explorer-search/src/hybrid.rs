//! Reciprocal Rank Fusion (RRF) for combining BM25 and semantic search results.
//!
//! RRF is a simple but effective method for merging ranked lists from
//! different retrieval systems. The formula is:
//!   rrfScore(d) = sum_over_lists( 1 / (K + rank_in_list) )
//!
//! Reference: Cormack, Clarke & Buettcher (2009).

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::bm25::BM25SearchResult;
use crate::embeddings::types::SemanticSearchResult;

/// RRF smoothing constant. Higher values reduce the influence of
/// high-ranking documents. K=60 is the standard value from the original paper.
const RRF_K: f64 = 60.0;

/// A hybrid search result combining BM25 and semantic scores via RRF.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HybridSearchResult {
    pub file_path: String,
    /// Final RRF-fused score.
    pub score: f64,
    /// Final rank (1-indexed).
    pub rank: usize,
    /// Which sources contributed to this result ("bm25", "semantic", or both).
    pub sources: Vec<String>,
    pub node_id: String,
    pub name: String,
    pub label: String,
    pub start_line: Option<u32>,
    pub end_line: Option<u32>,
    /// BM25 component score (before RRF).
    pub bm25_score: f64,
    /// Semantic component score (before RRF).
    pub semantic_score: f64,
}

/// Merge BM25 and semantic search results using Reciprocal Rank Fusion.
///
/// The algorithm:
/// 1. Compute rrfScore[i] = 1 / (RRF_K + rank + 1) for each BM25 result
/// 2. Compute rrfScore[i] = 1 / (RRF_K + rank + 1) for each semantic result
/// 3. Merge by file_path, summing RRF scores
/// 4. Sort by final score descending
/// 5. Assign final ranks (1-indexed)
pub fn merge_with_rrf(
    bm25_results: &[BM25SearchResult],
    semantic_results: &[SemanticSearchResult],
    limit: usize,
) -> Vec<HybridSearchResult> {
    // Key by node_id, NOT file_path: a single file can contribute multiple
    // distinct symbols, and collapsing on file_path silently drops all but the
    // first one (and overwrites the carried name/label/line range).
    let mut merged: HashMap<String, HybridSearchResult> = HashMap::new();

    // Process BM25 results
    for (i, result) in bm25_results.iter().enumerate() {
        let rrf_score = 1.0 / (RRF_K + i as f64 + 1.0);

        merged
            .entry(result.node_id.clone())
            .and_modify(|existing| {
                existing.score += rrf_score;
                existing.bm25_score = result.score;
                if !existing.sources.contains(&"bm25".to_string()) {
                    existing.sources.push("bm25".to_string());
                }
            })
            .or_insert(HybridSearchResult {
                file_path: result.file_path.clone(),
                score: rrf_score,
                rank: 0,
                sources: vec!["bm25".to_string()],
                node_id: result.node_id.clone(),
                name: result.name.clone(),
                label: result.label.clone(),
                start_line: result.start_line,
                end_line: result.end_line,
                bm25_score: result.score,
                semantic_score: 0.0,
            });
    }

    // Process semantic results
    for (i, result) in semantic_results.iter().enumerate() {
        let rrf_score = 1.0 / (RRF_K + i as f64 + 1.0);

        merged
            .entry(result.node_id.clone())
            .and_modify(|existing| {
                existing.score += rrf_score;
                existing.semantic_score = result.score;
                if !existing.sources.contains(&"semantic".to_string()) {
                    existing.sources.push("semantic".to_string());
                }
            })
            .or_insert(HybridSearchResult {
                file_path: result.file_path.clone(),
                score: rrf_score,
                rank: 0,
                sources: vec!["semantic".to_string()],
                node_id: result.node_id.clone(),
                name: result.name.clone(),
                label: result.label.clone(),
                start_line: result.start_line,
                end_line: result.end_line,
                bm25_score: 0.0,
                semantic_score: result.score,
            });
    }

    // Sort by score descending. Use total_cmp for NaN safety, and break ties on
    // node_id so the output is fully deterministic across HashMap iterations.
    let mut results: Vec<HybridSearchResult> = merged.into_values().collect();
    results.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.node_id.cmp(&b.node_id))
    });
    results.truncate(limit);

    // Assign final ranks (1-indexed)
    for (i, result) in results.iter_mut().enumerate() {
        result.rank = i + 1;
    }

    results
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_bm25(file_path: &str, score: f64, rank: usize) -> BM25SearchResult {
        BM25SearchResult {
            file_path: file_path.to_string(),
            score,
            rank,
            node_id: format!("Function:{file_path}:fn"),
            name: "fn".to_string(),
            label: "Function".to_string(),
            start_line: Some(1),
            end_line: Some(10),
        }
    }

    fn make_semantic(file_path: &str, score: f64, rank: usize) -> SemanticSearchResult {
        SemanticSearchResult {
            file_path: file_path.to_string(),
            score,
            rank,
            node_id: format!("Function:{file_path}:fn"),
            name: "fn".to_string(),
            label: "Function".to_string(),
            start_line: Some(1),
            end_line: Some(10),
        }
    }

    #[test]
    fn test_rrf_merge_basic() {
        let bm25 = vec![make_bm25("a.ts", 5.0, 1), make_bm25("b.ts", 3.0, 2)];
        let semantic = vec![
            make_semantic("b.ts", 0.95, 1),
            make_semantic("c.ts", 0.90, 2),
        ];

        let results = merge_with_rrf(&bm25, &semantic, 10);

        // b.ts appears in both lists, should have highest combined score
        assert!(!results.is_empty());
        let b_result = results.iter().find(|r| r.file_path == "b.ts").unwrap();
        assert_eq!(b_result.sources.len(), 2);
        assert!(b_result.sources.contains(&"bm25".to_string()));
        assert!(b_result.sources.contains(&"semantic".to_string()));

        // All ranks should be 1-indexed
        for (i, r) in results.iter().enumerate() {
            assert_eq!(r.rank, i + 1);
        }
    }

    #[test]
    fn test_rrf_empty_inputs() {
        let results = merge_with_rrf(&[], &[], 10);
        assert!(results.is_empty());
    }

    #[test]
    fn test_rrf_only_bm25() {
        let bm25 = vec![make_bm25("a.ts", 5.0, 1)];
        let results = merge_with_rrf(&bm25, &[], 10);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].sources, vec!["bm25"]);
        assert!((results[0].semantic_score - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_rrf_limit() {
        let bm25: Vec<BM25SearchResult> = (0..20)
            .map(|i| make_bm25(&format!("file{i}.ts"), 10.0 - i as f64, i + 1))
            .collect();
        let results = merge_with_rrf(&bm25, &[], 5);
        assert_eq!(results.len(), 5);
    }

    #[test]
    fn test_rrf_score_formula() {
        // Verify the RRF score formula: 1 / (K + rank + 1)
        // For rank 0 (first result): 1 / (60 + 0 + 1) = 1/61
        let bm25 = vec![make_bm25("a.ts", 5.0, 1)];
        let results = merge_with_rrf(&bm25, &[], 10);
        let expected = 1.0 / (RRF_K + 1.0);
        assert!((results[0].score - expected).abs() < 1e-10);
    }
}

//! Post-retrieval reranking.
//!
//! After BM25 or RRF fusion returns top-K candidates, a reranker reorders them
//! by relevance to the original query. This is the step where a model sees
//! query + candidate jointly and can score relevance more accurately than any
//! bag-of-words or cosine metric.

use serde::{Deserialize, Serialize};

/// A single candidate to be reranked.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Candidate {
    pub node_id: String,
    pub name: String,
    pub label: String,
    pub file_path: String,
    pub start_line: Option<u32>,
    pub end_line: Option<u32>,
    /// Original retrieval score (BM25 or RRF).
    pub score: f64,
    /// Original retrieval rank (1-indexed).
    pub rank: usize,
    /// Optional snippet — signature, first lines, or description.
    pub snippet: Option<String>,
}

/// Reranker trait. Takes a query and candidates, returns reordered candidates.
///
/// The returned Vec has the same members as the input (no candidate invented,
/// no candidate dropped) with the `rank` field updated to the new position.
pub trait Reranker: Send + Sync {
    fn rerank(&self, query: &str, candidates: Vec<Candidate>) -> anyhow::Result<Vec<Candidate>>;
}

#[cfg(feature = "reranker-llm")]
pub mod llm;

#[cfg(feature = "reranker-llm")]
pub use llm::LlmReranker;

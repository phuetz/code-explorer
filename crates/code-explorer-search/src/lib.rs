pub mod bm25;
pub mod embeddings;
pub mod fusion;
pub mod hybrid;
pub mod reranker;

use bm25::BM25SearchResult;
use code_explorer_db::adapter::DbAdapter;
use code_explorer_db::error::DbError;

/// Perform a search using the best available strategy.
///
/// Currently uses BM25 full-text search. When stored embeddings are available
/// in the future, this will automatically use hybrid RRF fusion to combine
/// BM25 and semantic results for better ranking.
pub fn search(
    adapter: &DbAdapter,
    query_text: &str,
    limit: usize,
) -> Result<Vec<BM25SearchResult>, DbError> {
    // TODO: when embeddings storage is implemented, load stored embeddings
    // from the repo's .codeexplorer/ directory and use hybrid::merge_with_rrf
    // to combine BM25 + semantic results.
    bm25::search_fts(adapter, query_text, limit)
}

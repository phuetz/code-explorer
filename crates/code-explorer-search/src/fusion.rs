//! Hybrid search fusion with pre-loaded embeddings.
//!
//! Both `code-explorer query --hybrid` (CLI, one-shot) and the desktop chat
//! (long-lived, embeddings cached in `LoadedRepo`) need the same fusion
//! pipeline. The CLI loads embeddings from disk on every invocation; the chat
//! loads them once at `open_repo`. This module exposes the fusion step alone,
//! so callers stay in charge of how embeddings are sourced and cached.

use std::path::Path;

use code_explorer_core::graph::KnowledgeGraph;

use crate::bm25::BM25SearchResult;
use crate::embeddings::{
    generate_embeddings, load_embeddings, search_semantic, EmbeddingConfig, EmbeddingStore,
};
use crate::hybrid::{merge_with_rrf, HybridSearchResult};

/// Run BM25 + semantic RRF fusion using already-loaded embeddings.
///
/// The caller supplies the BM25 results (already retrieved from whatever
/// adapter or in-memory index they have) and the embedding store (already
/// loaded into RAM). We embed the query with the same model the store was
/// produced with, take cosine top-K matches, enrich them with graph metadata,
/// and fuse via RRF.
///
/// Returns the fused list of length `<= top_k`. The `score` field is the RRF
/// score (0..1 range), not BM25 nor cosine.
pub fn hybrid_with_preloaded(
    query: &str,
    bm25_results: &[BM25SearchResult],
    embeddings: &[(String, Vec<f32>)],
    embedding_config: &EmbeddingConfig,
    graph: &KnowledgeGraph,
    top_k: usize,
) -> anyhow::Result<Vec<HybridSearchResult>> {
    // Embed the query with the same model the store was built with.
    let q_vecs = generate_embeddings(&[query.to_string()], embedding_config);
    let q_vec = q_vecs
        .into_iter()
        .next()
        .ok_or_else(|| anyhow::anyhow!("generate_embeddings returned no output"))?;
    if q_vec.iter().all(|&v| v == 0.0) {
        // The fallback path inside generate_embeddings fired (model missing,
        // tokenizer missing, runtime error). Fusing against zero vectors would
        // give every entry the same cosine score and silently degrade ranking.
        return Err(anyhow::anyhow!(
            "query embedding is all zeros — model path or tokenizer likely missing"
        ));
    }

    // Cosine top-K from the stored corpus.
    let mut semantic_results = search_semantic(&q_vec, embeddings, top_k);

    // search_semantic only fills node_id + score + rank; enrich with graph
    // metadata so RRF and downstream UI have file_path / name / lines.
    for s in &mut semantic_results {
        if let Some(n) = graph.get_node(&s.node_id) {
            s.file_path = n.properties.file_path.clone();
            s.name = n.properties.name.clone();
            s.label = format!("{:?}", n.label);
            s.start_line = n.properties.start_line;
            s.end_line = n.properties.end_line;
        }
    }

    Ok(merge_with_rrf(bm25_results, &semantic_results, top_k))
}

/// Convenience wrapper that loads embeddings + sidecar config from a repo's
/// `.codeexplorer/` directory. Returns `Ok(None)` when neither file is present —
/// callers should treat this as "semantic search not configured for this repo,
/// fall back to BM25 only" rather than as an error.
///
/// Returns `Err` when files exist but are malformed, since silent corruption
/// would be worse than a loud failure during a one-shot CLI run.
pub fn try_load_embeddings_from_storage(
    storage_path: &Path,
) -> anyhow::Result<Option<(EmbeddingStore, EmbeddingConfig)>> {
    let emb_path = storage_path.join("embeddings.bin");
    let meta_path = storage_path.join("embeddings.meta.json");
    if !emb_path.exists() || !meta_path.exists() {
        return Ok(None);
    }
    let cfg: EmbeddingConfig = serde_json::from_str(&std::fs::read_to_string(&meta_path)?)?;
    let store = load_embeddings(&emb_path)?;
    if store.header.dimension != cfg.dimension {
        anyhow::bail!(
            "embeddings.bin dim {} differs from meta dim {}",
            store.header.dimension,
            cfg.dimension
        );
    }
    Ok(Some((store, cfg)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn try_load_returns_none_when_files_missing() {
        let tmp = tempfile::tempdir().unwrap();
        let result = try_load_embeddings_from_storage(tmp.path()).unwrap();
        assert!(result.is_none(), "missing files should yield Ok(None)");
    }

    #[test]
    fn try_load_returns_none_when_only_one_file_present() {
        let tmp = tempfile::tempdir().unwrap();
        // Only the meta file — without embeddings.bin we still want None,
        // not a partial state that the caller would have to handle separately.
        std::fs::write(tmp.path().join("embeddings.meta.json"), "{}").unwrap();
        let result = try_load_embeddings_from_storage(tmp.path()).unwrap();
        assert!(result.is_none());
    }
}

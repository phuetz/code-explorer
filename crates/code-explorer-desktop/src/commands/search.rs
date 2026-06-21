use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use code_explorer_core::graph::KnowledgeGraph;
use code_explorer_db::inmemory::fts::FtsResult;
use code_explorer_search::bm25::BM25SearchResult;
use code_explorer_search::embeddings::{
    generate_embeddings, load_embeddings, search_semantic, EmbeddingConfig,
};
use code_explorer_search::hybrid as hybrid_rrf;
use code_explorer_search::reranker::{Candidate, LlmReranker, Reranker};
use tauri::State;

use crate::state::AppState;
use crate::types::SearchResult;

/// Full-text search using BM25 with optional LLM rerank and/or semantic
/// hybrid RRF fusion.
///
/// * `rerank=true` pulls a wider pool and sends top-20 to the configured
///   LLM for reranking. Requires a chat config with an API key.
/// * `hybrid=true` loads `.codeexplorer/embeddings.bin` + `embeddings.meta.json`
///   (produced by `code-explorer embed`) and fuses BM25 with cosine top-K via RRF.
/// * Both can be combined — hybrid runs first, then rerank on the fused
///   pool. On any failure (missing files, HTTP 503, parse error) the UI
///   always sees valid BM25 results — nothing is silently dropped.
#[tauri::command]
pub async fn search_symbols(
    state: State<'_, AppState>,
    query: String,
    limit: Option<usize>,
    rerank: Option<bool>,
    hybrid: Option<bool>,
) -> Result<Vec<SearchResult>, String> {
    let (graph, _indexes, fts_index, _repo_path) = state.get_repo(None).await?;
    let max_results = limit.unwrap_or(20);
    let use_rerank = rerank.unwrap_or(false);
    let use_hybrid = hybrid.unwrap_or(false);

    let pool_size = if use_rerank || use_hybrid {
        max_results.max(20)
    } else {
        max_results
    };
    let mut results = fts_index.search(&graph, &query, None, pool_size);

    if use_hybrid && results.len() > 1 {
        let storage = match state.active_storage_path().await {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!("hybrid requested but no active storage path: {e}");
                String::new()
            }
        };
        if !storage.is_empty() {
            results = maybe_hybrid_fuse(&query, &graph, &storage, results).await;
        }
    }

    if use_rerank && results.len() > 1 {
        results = maybe_rerank_with_chat_config(&state, &query, results).await;
    }
    results.truncate(max_results);

    Ok(results
        .into_iter()
        .map(|r| SearchResult {
            node_id: r.node_id,
            name: r.name,
            label: r.label,
            file_path: r.file_path,
            score: r.score,
            start_line: r.start_line,
            end_line: r.end_line,
        })
        .collect())
}

async fn maybe_hybrid_fuse(
    query: &str,
    graph: &Arc<KnowledgeGraph>,
    storage: &str,
    fts_results: Vec<FtsResult>,
) -> Vec<FtsResult> {
    let storage = PathBuf::from(storage);
    let emb_path = storage.join("embeddings.bin");
    let meta_path = storage.join("embeddings.meta.json");
    if !emb_path.exists() || !meta_path.exists() {
        tracing::warn!(
            "hybrid requested but embeddings files missing at {} — run 'code-explorer embed' first",
            storage.display()
        );
        return fts_results;
    }
    let query = query.to_string();
    let graph = graph.clone();
    let fts_clone = fts_results.clone();

    let outcome = tokio::task::spawn_blocking(move || -> anyhow::Result<Vec<FtsResult>> {
        let cfg: EmbeddingConfig = serde_json::from_str(&std::fs::read_to_string(&meta_path)?)?;
        let store = load_embeddings(&emb_path)?;
        if store.header.dimension != cfg.dimension {
            anyhow::bail!(
                "embeddings.bin dim {} != meta dim {}",
                store.header.dimension,
                cfg.dimension
            );
        }
        let q_vecs = generate_embeddings(std::slice::from_ref(&query), &cfg);
        let q_vec = q_vecs
            .into_iter()
            .next()
            .ok_or_else(|| anyhow::anyhow!("empty embeddings output"))?;
        if q_vec.iter().all(|&v| v == 0.0) {
            anyhow::bail!("query embedding all-zero");
        }
        let stored: Vec<(String, Vec<f32>)> = store.entries;
        let top_k = fts_clone.len();
        let mut semantic_results = search_semantic(&q_vec, &stored, top_k);
        for s in &mut semantic_results {
            if let Some(n) = graph.get_node(&s.node_id) {
                s.file_path = n.properties.file_path.clone();
                s.name = n.properties.name.clone();
                s.label = format!("{:?}", n.label);
                s.start_line = n.properties.start_line;
                s.end_line = n.properties.end_line;
            }
        }
        let bm25_wrapped: Vec<BM25SearchResult> = fts_clone
            .iter()
            .enumerate()
            .map(|(i, r)| BM25SearchResult {
                file_path: r.file_path.clone(),
                score: r.score,
                rank: i + 1,
                node_id: r.node_id.clone(),
                name: r.name.clone(),
                label: r.label.clone(),
                start_line: r.start_line,
                end_line: r.end_line,
            })
            .collect();
        let fused = hybrid_rrf::merge_with_rrf(&bm25_wrapped, &semantic_results, top_k);
        Ok(fused
            .into_iter()
            .map(|h| FtsResult {
                node_id: h.node_id,
                score: h.score,
                name: h.name,
                file_path: h.file_path,
                label: h.label,
                start_line: h.start_line,
                end_line: h.end_line,
            })
            .collect())
    })
    .await;

    match outcome {
        Ok(Ok(v)) => v,
        Ok(Err(e)) => {
            tracing::warn!(error = %e, "search hybrid fuse failed; using BM25 order");
            fts_results
        }
        Err(e) => {
            tracing::warn!(error = %e, "search hybrid task panicked; using BM25 order");
            fts_results
        }
    }
}

async fn maybe_rerank_with_chat_config(
    state: &State<'_, AppState>,
    query: &str,
    fts_results: Vec<FtsResult>,
) -> Vec<FtsResult> {
    let config = state.chat_config().await;
    let (base_url, model, api_key) = match config {
        Some(c) if !c.api_key.is_empty() => (c.base_url, c.model, c.api_key),
        _ => {
            tracing::warn!(
                "search rerank requested but chat config missing or API key empty; using BM25 order"
            );
            return fts_results;
        }
    };

    let candidates: Vec<Candidate> = fts_results
        .iter()
        .enumerate()
        .map(|(i, r)| Candidate {
            node_id: r.node_id.clone(),
            name: r.name.clone(),
            label: r.label.clone(),
            file_path: r.file_path.clone(),
            start_line: r.start_line,
            end_line: r.end_line,
            score: r.score,
            rank: i + 1,
            snippet: None,
        })
        .collect();

    let reranker = LlmReranker::new(base_url, model, Some(api_key));
    let q = query.to_string();
    let reranked = match tokio::task::spawn_blocking(move || reranker.rerank(&q, candidates)).await
    {
        Ok(Ok(r)) => r,
        Ok(Err(e)) => {
            tracing::warn!(error = %e, "search reranker failed; using BM25 order");
            return fts_results;
        }
        Err(e) => {
            tracing::warn!(error = %e, "search reranker task panicked; using BM25 order");
            return fts_results;
        }
    };

    let mut by_id: HashMap<String, FtsResult> = fts_results
        .into_iter()
        .map(|r| (r.node_id.clone(), r))
        .collect();
    let mut out = Vec::with_capacity(by_id.len());
    for c in reranked {
        if let Some(r) = by_id.remove(&c.node_id) {
            out.push(r);
        }
    }
    out.extend(by_id.into_values());
    out
}

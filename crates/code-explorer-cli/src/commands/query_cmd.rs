//! The `query` command: search the knowledge graph via the in-memory snapshot.

use std::path::{Path, PathBuf};

use code_explorer_core::storage::repo_manager;
use code_explorer_db::inmemory::fts::{FtsIndex, FtsResult};
use code_explorer_search::bm25::BM25SearchResult;
use code_explorer_search::fusion;
use code_explorer_search::reranker::{Candidate, LlmReranker, Reranker};

/// When `--rerank` is active, we pull a larger BM25 top-K to give the LLM a
/// broader pool to reorder, then truncate to `limit` after reranking.
const RERANK_CANDIDATE_POOL: usize = 20;

pub async fn run(
    query: &str,
    repo: Option<&str>,
    limit: usize,
    rerank: bool,
    hybrid_mode: bool,
) -> anyhow::Result<()> {
    let repo_path = resolve_repo_path(repo)?;
    let storage = repo_manager::get_storage_paths(&repo_path);
    let snap = code_explorer_db::snapshot::snapshot_path(&storage.storage_path);

    if !snap.exists() {
        return Err(anyhow::anyhow!(
            "No graph snapshot found. Run 'code-explorer analyze' first."
        ));
    }

    let graph = code_explorer_db::snapshot::load_snapshot(&snap)?;
    let fts = FtsIndex::build(&graph);

    // Pull a larger pool when reranking or fusing so there's room to reorder.
    let pool = if rerank || hybrid_mode {
        limit.max(RERANK_CANDIDATE_POOL)
    } else {
        limit
    };
    let bm25 = fts.search(&graph, query, None, pool);

    if bm25.is_empty() && !hybrid_mode {
        println!("No results for '{query}'.");
        return Ok(());
    }

    // Hybrid: fuse BM25 with semantic via RRF BEFORE any LLM rerank.
    let fused: Vec<FtsResult> = if hybrid_mode {
        match run_hybrid(query, &bm25, &graph, Path::new(&storage.storage_path), pool) {
            Ok(r) => r,
            Err(e) => {
                eprintln!("Warning: hybrid path failed ({e}); falling back to BM25-only.");
                bm25.clone()
            }
        }
    } else {
        bm25.clone()
    };

    if fused.is_empty() {
        println!("No results for '{query}'.");
        return Ok(());
    }

    let candidates = if rerank {
        match run_reranker(query, &fused).await {
            Ok(r) => r,
            Err(e) => {
                eprintln!("Warning: reranker failed, falling back to pre-rerank order: {e}");
                fts_to_candidates(&fused)
            }
        }
    } else {
        fts_to_candidates(&fused)
    };

    let display = &candidates[..candidates.len().min(limit)];
    println!("Found {} results for '{}':", display.len(), query);
    let mut mods: Vec<&str> = Vec::new();
    if hybrid_mode {
        mods.push("hybrid BM25+semantic RRF");
    }
    if rerank {
        mods.push("LLM rerank");
    }
    if !mods.is_empty() {
        println!("  ({}, pool={})", mods.join(" + "), pool);
    }
    println!();
    for (i, r) in display.iter().enumerate() {
        let loc = match (r.start_line, r.end_line) {
            (Some(s), Some(e)) => format!("{}:{}-{}", r.file_path, s, e),
            (Some(s), None) => format!("{}:{}", r.file_path, s),
            _ => r.file_path.clone(),
        };
        println!("  {:>3}. [{:<10}] {:<30}  {}", i + 1, r.label, r.name, loc);
    }

    Ok(())
}

fn fts_to_candidates(bm25: &[FtsResult]) -> Vec<Candidate> {
    bm25.iter()
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
        .collect()
}

/// Perform BM25+semantic RRF fusion. Loads `.codeexplorer/embeddings.bin` and
/// `embeddings.meta.json` from disk, then delegates to `fusion::hybrid_with_preloaded`.
///
/// Returns the fused results as FtsResult (so downstream rerank/display
/// don't need a different branch). The `score` field on each result is
/// the RRF score (0–1 range), not the BM25 or cosine score.
fn run_hybrid(
    query: &str,
    bm25: &[FtsResult],
    graph: &code_explorer_core::graph::KnowledgeGraph,
    storage_path: &Path,
    top_k: usize,
) -> anyhow::Result<Vec<FtsResult>> {
    let (store, cfg) =
        fusion::try_load_embeddings_from_storage(storage_path)?.ok_or_else(|| {
            let emb_path = storage_path.join("embeddings.bin");
            let meta_path = storage_path.join("embeddings.meta.json");
            anyhow::anyhow!(
                "embeddings not found — run 'code-explorer embed --model <path>' first \
                 (expected {} and {})",
                emb_path.display(),
                meta_path.display()
            )
        })?;

    let bm25_wrapped: Vec<BM25SearchResult> = bm25
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

    let fused =
        fusion::hybrid_with_preloaded(query, &bm25_wrapped, &store.entries, &cfg, graph, top_k)?;

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
}

async fn run_reranker(query: &str, fts: &[FtsResult]) -> anyhow::Result<Vec<Candidate>> {
    let config = super::generate::load_llm_config().ok_or_else(|| {
        anyhow::anyhow!(
            "--rerank requires an LLM config at ~/.codeexplorer/chat-config.json. \
             Run 'code-explorer config test' to see the expected format."
        )
    })?;

    let candidates = fts_to_candidates(fts);
    let api_key = (!config.api_key.is_empty()).then_some(config.api_key);
    let reranker = LlmReranker::new(config.base_url, config.model, api_key)
        .with_max_candidates(RERANK_CANDIDATE_POOL);

    let q = query.to_string();
    let result = tokio::task::spawn_blocking(move || reranker.rerank(&q, candidates)).await??;
    Ok(result)
}

fn resolve_repo_path(repo: Option<&str>) -> anyhow::Result<PathBuf> {
    match repo {
        Some(r) => {
            let p = Path::new(r);
            Ok(p.canonicalize().unwrap_or_else(|_| p.to_path_buf()))
        }
        None => Ok(std::env::current_dir()?),
    }
}

//! The `query` command: search the knowledge graph via the in-memory snapshot.

use std::collections::HashSet;
use std::io::{BufRead, BufReader};
use std::path::{Component, Path, PathBuf};

use code_explorer_core::graph::types::NodeLabel;
use code_explorer_core::graph::KnowledgeGraph;
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
    file_type: Option<&str>,
    page: usize,
    compact: bool,
    rerank: bool,
    hybrid_mode: bool,
) -> anyhow::Result<()> {
    let (offset, page_size, pool) = page_window(page, limit)?;
    let repo_path = resolve_repo_path(repo)?;
    crate::commands::utils::check_and_warn_stale_index(&repo_path);
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
    let mut pool = if rerank || hybrid_mode {
        pool.max(RERANK_CANDIDATE_POOL)
    } else {
        pool
    };
    if asks_for_files(query) && !rerank && !hybrid_mode {
        pool = pool.max(200);
    }
    let bm25 = fts.search_with_file_type(&graph, query, None, file_type, pool);

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

    let mut candidates = if rerank {
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
    if !rerank && !hybrid_mode {
        let source_matches = source_code_hits(&graph, &repo_path, query, file_type);
        if !source_matches.is_empty() {
            let source_paths: HashSet<&str> = source_matches
                .iter()
                .map(|candidate| candidate.file_path.as_str())
                .collect();
            candidates.retain(|candidate| !source_paths.contains(candidate.file_path.as_str()));
            candidates.splice(0..0, source_matches);
        }
    }
    if let Some(extension) = file_type {
        candidates.retain(|candidate| {
            candidate.file_path.to_ascii_lowercase().ends_with(&format!(
                ".{}",
                extension.trim_start_matches('.').to_ascii_lowercase()
            ))
        });
    }
    if asks_for_files(query) {
        distinct_files(&mut candidates);
    }

    let start = offset.min(candidates.len());
    let end = (start + page_size).min(candidates.len());
    let display = &candidates[start..end];
    if compact {
        println!(
            "Results {}-{} for '{}' (page {}):",
            start + usize::from(!display.is_empty()),
            end,
            query,
            page
        );
    } else {
        println!(
            "Found {} results for '{}' (page {}):",
            display.len(),
            query,
            page
        );
    }
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
    if !compact {
        println!();
    }
    for (i, r) in display.iter().enumerate() {
        let loc = match (r.start_line, r.end_line) {
            (Some(s), Some(e)) => format!("{}:{}-{}", r.file_path, s, e),
            (Some(s), None) => format!("{}:{}", r.file_path, s),
            _ => r.file_path.clone(),
        };
        let evidence = source_excerpt(&repo_path, &r.file_path, r.start_line);
        if compact {
            println!(
                "{} [{:<10}] {} | {}",
                loc,
                r.label,
                r.name,
                evidence.unwrap_or_default()
            );
        } else {
            println!(
                "  {:>3}. [{:<10}] {:<30}  {}",
                offset + i + 1,
                r.label,
                r.name,
                loc
            );
            if let Some(line) = evidence {
                println!("       preuve: {}", line);
            }
        }
    }
    if candidates.len() > end {
        println!("Next page: --page {}", page + 1);
    }

    Ok(())
}

fn page_window(page: usize, limit: usize) -> anyhow::Result<(usize, usize, usize)> {
    if page == 0 || limit == 0 || limit > 50 {
        anyhow::bail!("--page must be >= 1 and --limit must be between 1 and 50");
    }
    let offset = (page - 1)
        .checked_mul(limit)
        .ok_or_else(|| anyhow::anyhow!("page overflow"))?;
    let pool = offset
        .checked_add(limit + 1)
        .ok_or_else(|| anyhow::anyhow!("page overflow"))?;
    if pool > 5000 {
        anyhow::bail!("at most 5000 search results can be paged");
    }
    Ok((offset, limit, pool))
}

fn source_excerpt(repo: &Path, file_path: &str, line: Option<u32>) -> Option<String> {
    let line = line?.checked_sub(1)? as usize;
    let relative = Path::new(file_path);
    if relative.is_absolute()
        || relative
            .components()
            .any(|part| !matches!(part, Component::Normal(_)))
    {
        return None;
    }
    let file = std::fs::File::open(repo.join(relative)).ok()?;
    let text = BufReader::new(file).lines().nth(line)?.ok()?;
    let excerpt: String = text.trim().chars().take(120).collect();
    Some(excerpt)
}

fn asks_for_files(query: &str) -> bool {
    query
        .to_ascii_lowercase()
        .split(|ch: char| !ch.is_ascii_alphabetic())
        .any(|word| matches!(word, "fichier" | "fichiers" | "file" | "files"))
}

fn distinct_files(candidates: &mut Vec<Candidate>) {
    let mut seen = HashSet::new();
    candidates.retain(|candidate| seen.insert(candidate.file_path.clone()));
}

/// When a question names concrete code markers, inspect source files for the
/// markers together. This finds implementations whose symbol names omit them
/// (for example PRAGMA constants), while keeping the scan bounded.
fn source_code_hits(
    graph: &KnowledgeGraph,
    repo: &Path,
    query: &str,
    file_type: Option<&str>,
) -> Vec<Candidate> {
    let markers: Vec<String> = query
        .split(|ch: char| !ch.is_ascii_alphanumeric() && ch != '_')
        .filter(|word| {
            word.len() >= 3
                && (word.contains('_') || word.chars().all(|ch| ch.is_ascii_uppercase()))
        })
        .take(4)
        .map(|word| word.to_ascii_lowercase())
        .collect();
    if markers.len() < 2 {
        return Vec::new();
    }
    let mut paths = HashSet::new();
    let mut bytes_read = 0u64;
    let mut hits = Vec::new();
    for node in graph
        .iter_nodes()
        .filter(|node| node.label == NodeLabel::File)
    {
        let path = node.properties.file_path.as_str();
        if !paths.insert(path) || is_test_source(path) || !is_source_file(path, file_type) {
            continue;
        }
        let relative = Path::new(path);
        if relative.is_absolute()
            || relative
                .components()
                .any(|part| !matches!(part, Component::Normal(_)))
        {
            continue;
        }
        let path_on_disk = repo.join(relative);
        let Ok(metadata) = std::fs::metadata(&path_on_disk) else {
            continue;
        };
        if metadata.len() > 1_000_000 || bytes_read + metadata.len() > 64_000_000 {
            continue;
        }
        bytes_read += metadata.len();
        let Ok(content) = std::fs::read_to_string(path_on_disk) else {
            continue;
        };
        let lower = content.to_ascii_lowercase();
        if !markers.iter().all(|marker| lower.contains(marker)) {
            continue;
        }
        let line = content
            .lines()
            .enumerate()
            .find(|(_, text)| {
                let text = text.to_ascii_lowercase();
                markers.iter().all(|marker| text.contains(marker))
            })
            .or_else(|| {
                content.lines().enumerate().find(|(_, text)| {
                    let text = text.to_ascii_lowercase();
                    markers.iter().any(|marker| text.contains(marker))
                })
            })
            .map(|(index, _)| (index + 1) as u32);
        hits.push(Candidate {
            node_id: node.id.clone(),
            name: node.properties.name.clone(),
            label: "File".to_string(),
            file_path: path.to_string(),
            start_line: line,
            end_line: line,
            score: 0.0,
            rank: 0,
            snippet: None,
        });
        if hits.len() >= 10 {
            break;
        }
    }
    hits.sort_by(|a, b| a.file_path.cmp(&b.file_path));
    hits
}

fn is_test_source(path: &str) -> bool {
    let path = path.to_ascii_lowercase();
    path.split(['/', '\\'])
        .any(|part| matches!(part, "test" | "tests" | "__tests__"))
        || path.ends_with("tests.cs")
        || path.ends_with("_test.rs")
        || path.contains(".test.")
        || path.contains(".spec.")
}

fn is_source_file(path: &str, file_type: Option<&str>) -> bool {
    let lower = path.to_ascii_lowercase();
    if let Some(extension) = file_type {
        return lower.ends_with(&format!(
            ".{}",
            extension.trim_start_matches('.').to_ascii_lowercase()
        ));
    }
    [
        ".rs", ".cs", ".ts", ".tsx", ".js", ".jsx", ".py", ".go", ".java", ".cpp", ".c", ".h",
    ]
    .iter()
    .any(|extension| lower.ends_with(extension))
}

#[cfg(test)]
mod pagination_tests {
    use super::*;
    use code_explorer_core::graph::types::{GraphNode, NodeProperties};

    #[test]
    fn pages_are_bounded_and_do_not_overlap() {
        assert_eq!(page_window(1, 10).unwrap(), (0, 10, 11));
        assert_eq!(page_window(2, 10).unwrap(), (10, 10, 21));
        assert!(page_window(0, 10).is_err());
        assert!(page_window(1, 51).is_err());
        assert!(page_window(501, 10).is_err());
    }

    #[test]
    fn evidence_is_read_from_the_declared_line() {
        let dir = std::env::temp_dir().join(format!("ce-query-proof-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("sample.rs"), "first\nfn expected() {}\nthird\n").unwrap();
        assert_eq!(
            source_excerpt(&dir, "sample.rs", Some(2)).as_deref(),
            Some("fn expected() {}")
        );
        assert!(source_excerpt(&dir, "../sample.rs", Some(2)).is_none());
        std::fs::remove_file(dir.join("sample.rs")).unwrap();
        std::fs::remove_dir(dir).unwrap();
    }

    #[test]
    fn code_markers_find_source_files_before_test_files() {
        let dir = std::env::temp_dir().join(format!("ce-query-markers-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("src")).unwrap();
        std::fs::create_dir_all(dir.join("tests")).unwrap();
        std::fs::write(
            dir.join("src/Pragma.cs"),
            "PRAGMA journal_mode=WAL;\nPRAGMA busy_timeout=5000;",
        )
        .unwrap();
        std::fs::write(dir.join("tests/PragmaTests.cs"), "WAL busy_timeout").unwrap();
        let mut graph = KnowledgeGraph::new();
        for path in ["src/Pragma.cs", "tests/PragmaTests.cs"] {
            graph.add_node(GraphNode {
                id: format!("File:{path}"),
                label: NodeLabel::File,
                properties: NodeProperties {
                    name: path.to_string(),
                    file_path: path.to_string(),
                    ..Default::default()
                },
            });
        }
        let hits = source_code_hits(
            &graph,
            &dir,
            "Where are WAL and busy_timeout set?",
            Some("cs"),
        );
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].file_path, "src/Pragma.cs");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn file_question_is_detected_in_french_and_english() {
        assert!(asks_for_files("Quels fichiers relient les modules ?"));
        assert!(asks_for_files("Which files configure the database?"));
        assert!(!asks_for_files("Où est définie cette méthode ?"));
    }

    #[test]
    fn file_question_returns_one_result_per_source_file() {
        let rows = [("a", "src/a.ts"), ("b", "src/a.ts"), ("c", "src/c.ts")].map(|(name, path)| {
            FtsResult {
                node_id: name.to_string(),
                score: 1.0,
                name: name.to_string(),
                file_path: path.to_string(),
                label: "Function".to_string(),
                start_line: Some(1),
                end_line: Some(1),
            }
        });
        let mut candidates = fts_to_candidates(&rows);
        distinct_files(&mut candidates);
        assert_eq!(
            candidates
                .iter()
                .map(|r| r.name.as_str())
                .collect::<Vec<_>>(),
            vec!["a", "c"]
        );
    }
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

pub fn resolve_repo_path(repo: Option<&str>) -> anyhow::Result<PathBuf> {
    match repo {
        Some(r) => {
            let p = Path::new(r);
            Ok(p.canonicalize().unwrap_or_else(|_| p.to_path_buf()))
        }
        None => Ok(std::env::current_dir()?),
    }
}

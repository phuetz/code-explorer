//! The `embed` command: generate embeddings from the indexed graph snapshot.
//!
//! Produces `<repo>/.codeexplorer/embeddings.bin` using the given ONNX model. Each
//! indexed Function / Class / Method / Interface / Struct / Trait / Enum /
//! Module node contributes one vector, keyed by its node_id. The file is
//! consumed by `code_explorer_search::search()` when RRF fusion is enabled.

use std::path::{Path, PathBuf};
use std::time::Instant;

use anyhow::{anyhow, Result};
use chrono::Utc;

use code_explorer_core::graph::types::NodeLabel;
use code_explorer_core::storage::repo_manager;
use code_explorer_search::embeddings::{
    generate_embeddings, save_embeddings, EmbeddingConfig, EmbeddingHeader, EmbeddingStore,
};

/// Labels we generate embeddings for — same symbols the FTS indexes plus
/// DocChunk (GraphRAG). We skip File/Folder/raw structural nodes that BM25
/// name-indexes but that have no meaningful textual content for embedding.
const EMBED_LABELS: &[NodeLabel] = &[
    NodeLabel::Function,
    NodeLabel::Class,
    NodeLabel::Method,
    NodeLabel::Interface,
    NodeLabel::Struct,
    NodeLabel::Trait,
    NodeLabel::Enum,
    NodeLabel::Module,
    NodeLabel::Type,
    NodeLabel::Route,
    NodeLabel::Controller,
    NodeLabel::ControllerAction,
    NodeLabel::Service,
    NodeLabel::Repository,
    NodeLabel::DocChunk,
];

#[allow(clippy::too_many_arguments)]
pub async fn run(
    model: &str,
    tokenizer: Option<&str>,
    repo: Option<&str>,
    dim: usize,
    batch: usize,
    max_tokens: usize,
    no_token_type_ids: bool,
) -> Result<()> {
    let repo_path = resolve_repo_path(repo)?;
    let storage = repo_manager::get_storage_paths(&repo_path);
    let snap = code_explorer_db::snapshot::snapshot_path(&storage.storage_path);
    if !snap.exists() {
        return Err(anyhow!(
            "No graph snapshot found at {}. Run 'code-explorer analyze' first.",
            snap.display()
        ));
    }
    let graph = code_explorer_db::snapshot::load_snapshot(&snap)?;

    let model_path = PathBuf::from(model);
    if !model_path.exists() {
        return Err(anyhow!("ONNX model not found at {}", model_path.display()));
    }

    let cfg = EmbeddingConfig {
        model_name: model_path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("unknown")
            .to_string(),
        dimension: dim,
        max_tokens,
        normalize: true,
        batch_size: batch,
        model_path: Some(model_path.to_string_lossy().into_owned()),
        tokenizer_path: tokenizer.map(|s| s.to_string()),
        needs_token_type_ids: !no_token_type_ids,
    };

    // Collect (node_id, text) pairs from the graph, skipping build-artifact
    // paths which would otherwise pollute semantic search (observed on
    // `target-codex/` where `flag_check.c` files matched "feature flag"
    // queries and crowded out the real code-explorer-search code).
    let mut tasks: Vec<(String, String)> = Vec::new();
    for node in graph.iter_nodes() {
        if !EMBED_LABELS.contains(&node.label) {
            continue;
        }
        let fp = node.properties.file_path.as_str();
        if is_build_artifact(fp) {
            continue;
        }
        let text = build_embedding_text(node);
        if text.is_empty() {
            continue;
        }
        tasks.push((node.id.clone(), text));
    }

    if tasks.is_empty() {
        return Err(anyhow!(
            "No embeddable nodes in graph (labels: {:?}). \
             Did 'code-explorer analyze' run successfully?",
            EMBED_LABELS
        ));
    }
    println!("Embedding {} symbols with {}…", tasks.len(), cfg.model_name);

    let start = Instant::now();
    let mut entries: Vec<(String, Vec<f32>)> = Vec::with_capacity(tasks.len());
    let mut offset = 0;
    while offset < tasks.len() {
        let end = (offset + batch).min(tasks.len());
        let texts: Vec<String> = tasks[offset..end].iter().map(|(_, t)| t.clone()).collect();
        let vecs = generate_embeddings(&texts, &cfg);
        if vecs.len() != texts.len() {
            return Err(anyhow!(
                "embedder returned {} vectors for {} inputs",
                vecs.len(),
                texts.len()
            ));
        }
        for (i, v) in vecs.into_iter().enumerate() {
            // Skip all-zero vectors — these indicate the fallback path fired
            // (model or tokenizer missing, inference error). Saving them
            // would pollute the similarity space with uniform-score matches.
            if v.iter().all(|&x| x == 0.0) {
                if entries.is_empty() {
                    return Err(anyhow!(
                        "embedder returned zero vectors — check --model path, \
                         tokenizer.json presence, and --dim match"
                    ));
                }
                continue;
            }
            entries.push((tasks[offset + i].0.clone(), v));
        }
        offset = end;
        if offset % (batch * 10) == 0 || offset == tasks.len() {
            println!(
                "  {} / {} ({}s elapsed)",
                offset,
                tasks.len(),
                start.elapsed().as_secs()
            );
        }
    }
    println!(
        "Generated {} embeddings in {:.1}s",
        entries.len(),
        start.elapsed().as_secs_f64()
    );

    let out_path = Path::new(&storage.storage_path).join("embeddings.bin");
    let store = EmbeddingStore {
        header: EmbeddingHeader {
            model_name: cfg.model_name.clone(),
            dimension: cfg.dimension,
            count: entries.len(),
            generated_at: Utc::now().to_rfc3339(),
        },
        entries,
    };
    save_embeddings(&out_path, &store)?;

    // Also write a sidecar meta.json with the full EmbeddingConfig so `code-explorer
    // query --hybrid` can rebuild the embedder at query time without the user
    // re-specifying --model / --tokenizer. Kept JSON-only so it's human-readable.
    let meta_path = Path::new(&storage.storage_path).join("embeddings.meta.json");
    std::fs::write(&meta_path, serde_json::to_string_pretty(&cfg)?)?;

    println!("Saved to {}", out_path.display());
    println!("Sidecar config at {}", meta_path.display());
    Ok(())
}

/// Max characters of `content` to include in the embedding input. Larger
/// function bodies dilute the semantic signal — MiniLM/BGE are trained on
/// sequences of ~128 tokens on average, and letting a 1000-line function
/// fill the 512-token window means the name/description signal gets buried
/// under boilerplate. 1000 chars ≈ 250 tokens leaves room for everything
/// else in the prompt and captures the "gist" of the body (docstring +
/// early returns + first branch typically).
///
/// Observed problem before this cap : `enrich_aspnet_mvc` (1000+ LOC)
/// dropped out of top-5 on "ASP.NET MVC controller action extraction"
/// under hybrid search when it was #4 under BM25 — the full body
/// embedded a long `match` table that drifted off the query topic.
const CONTENT_CHAR_CAP: usize = 1000;

/// Build a short text snippet for embedding a node. Mirrors the fields BM25
/// indexes so the two ranking paths see the same surface — plus a capped
/// excerpt of `content` for the richer semantic signal on function bodies.
fn build_embedding_text(node: &code_explorer_core::graph::types::GraphNode) -> String {
    let mut parts: Vec<String> = Vec::with_capacity(4);
    parts.push(node.properties.name.clone());
    parts.push(node.properties.file_path.clone());
    if let Some(desc) = &node.properties.description {
        parts.push(desc.clone());
    }
    if let Some(content) = &node.properties.content {
        let trimmed = content.trim();
        if trimmed.len() > CONTENT_CHAR_CAP {
            let mut cut = CONTENT_CHAR_CAP;
            // Avoid splitting mid-UTF8 codepoint.
            while cut > 0 && !trimmed.is_char_boundary(cut) {
                cut -= 1;
            }
            parts.push(trimmed[..cut].to_string());
        } else {
            parts.push(trimmed.to_string());
        }
    }
    parts.join(" ")
}

/// Skip files under common build-artifact directories that leak into the
/// graph when `code-explorer analyze` is run on a working copy that has been
/// built. These paths contain generated code (tree-sitter parser tables,
/// rustc build scripts' `flag_check.c`, etc.) whose names match natural
/// search queries by accident ("flag", "check", "parse") and pollute
/// semantic results. The matcher is cheap string-contains, tuned for the
/// layouts we've actually observed; extend as needed.
fn is_build_artifact(file_path: &str) -> bool {
    // Normalize Windows backslashes so the matcher is OS-agnostic.
    let lower = file_path.replace('\\', "/").to_lowercase();
    const ARTIFACT_MARKERS: &[&str] = &[
        "target/debug/",
        "target/release/",
        "target-codex/",
        ".codex-target/",
        "node_modules/",
        "dist/",
        ".codeexplorer/",
    ];
    ARTIFACT_MARKERS.iter().any(|m| lower.contains(m))
}

fn resolve_repo_path(repo: Option<&str>) -> Result<PathBuf> {
    match repo {
        Some(r) => {
            let p = Path::new(r);
            Ok(p.canonicalize().unwrap_or_else(|_| p.to_path_buf()))
        }
        None => Ok(std::env::current_dir()?),
    }
}

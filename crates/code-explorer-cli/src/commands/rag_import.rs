//! The `rag-import` command: ingest external documentation for GraphRAG.

use anyhow::Result;
use colored::Colorize;
use regex::Regex;
use std::path::Path;
use walkdir::WalkDir;

use code_explorer_core::graph::{NodeLabel, NodeProperties, RelationshipType};
use code_explorer_db::snapshot;
use code_explorer_rag::chunk_document;
use code_explorer_search::embeddings::{generate_embeddings, EmbeddingConfig};

/// Minimum symbol name length to consider for mention matching.
/// Names shorter than this (e.g., "i", "get", "set") produce too many false positives.
const MIN_SYMBOL_NAME_LEN: usize = 5;

pub fn run(docs_dir: &str, repo_path_str: Option<&str>) -> Result<()> {
    let repo_path = Path::new(repo_path_str.unwrap_or(".")).canonicalize()?;
    let storage = code_explorer_core::storage::repo_manager::get_storage_paths(&repo_path);
    let snap_path = snapshot::snapshot_path(&storage.storage_path);

    if !snap_path.exists() {
        println!(
            "{} No index found. Run 'code-explorer analyze' first.",
            "ERROR".red()
        );
        return Ok(());
    }

    println!("{} Loading graph...", "->".cyan());
    let mut graph = snapshot::load_snapshot(&snap_path)?;

    let docs_path = Path::new(docs_dir);
    if !docs_path.exists() {
        println!(
            "{} Documentation directory not found: {}",
            "ERROR".red(),
            docs_dir
        );
        return Ok(());
    }

    // Purge existing RAG nodes to avoid duplicates on re-import
    let removed = graph.remove_nodes_by_label(NodeLabel::Document)
        + graph.remove_nodes_by_label(NodeLabel::DocChunk);
    if removed > 0 {
        println!("  Cleared {} existing RAG nodes", removed);
    }

    println!(
        "{} Ingesting documentation from {}...",
        "->".cyan(),
        docs_dir
    );

    // Filter relevant nodes for semantic matching.
    // Only match against main symbols to avoid false positives.
    let mut known_symbols: Vec<(String, Regex)> = Vec::new();
    for node in graph.iter_nodes() {
        match node.label {
            NodeLabel::Class
            | NodeLabel::Function
            | NodeLabel::Method
            | NodeLabel::Interface
            | NodeLabel::Service
            | NodeLabel::Controller
            | NodeLabel::Struct
                if node.properties.name.len() >= MIN_SYMBOL_NAME_LEN => {
                    let pattern = format!(r"\b{}\b", regex::escape(&node.properties.name));
                    if let Ok(re) = Regex::new(&pattern) {
                        known_symbols.push((node.id.clone(), re));
                    }
                }
            _ => {}
        }
    }

    let config = EmbeddingConfig::default();
    let mut total_chunks = 0;
    let mut total_mentions = 0;

    let mut total_files: usize = 0;
    let mut skipped_files: usize = 0;

    for entry_result in WalkDir::new(docs_path) {
        let entry = match entry_result {
            Ok(e) => e,
            Err(_) => continue,
        };

        if !entry.file_type().is_file() {
            continue;
        }

        let ext = entry
            .path()
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        // Supported ingestion formats. Extend `chunk_document` + this match
        // when adding new extractors (e.g. html, pdf).
        if !matches!(ext.as_str(), "md" | "docx") {
            continue;
        }

        // Use relative path from docs_dir for unique, deterministic IDs
        let relative_path = entry
            .path()
            .strip_prefix(docs_path)
            .unwrap_or(entry.path())
            .to_string_lossy()
            .replace('\\', "/");
        let full_path = entry.path().to_string_lossy().to_string();

        let chunks = match chunk_document(entry.path()) {
            Ok(c) => c,
            Err(e) => {
                tracing::warn!("rag-import: failed to extract {}: {}", full_path, e);
                skipped_files += 1;
                continue;
            }
        };
        if chunks.is_empty() {
            skipped_files += 1;
            continue;
        }

        // Create Document Node
        let doc_id = format!("Document:{}", relative_path);
        graph.add_node(code_explorer_core::graph::GraphNode {
            id: doc_id.clone(),
            label: NodeLabel::Document,
            properties: NodeProperties {
                name: relative_path.clone(),
                file_path: full_path.clone(),
                ..Default::default()
            },
        });
        total_files += 1;

        // Batch embed all chunks for this file
        let texts: Vec<String> = chunks.iter().map(|c| c.content.clone()).collect();
        let embeddings = generate_embeddings(&texts, &config);

        // Validate embedding count matches chunk count
        let embeddings_valid = embeddings.len() == texts.len();
        if !embeddings_valid && !texts.is_empty() {
            tracing::warn!(
                "Embedding count mismatch for {}: expected {}, got {}. Skipping embeddings.",
                relative_path,
                texts.len(),
                embeddings.len()
            );
        }

        for (i, chunk) in chunks.into_iter().enumerate() {
            let chunk_id = format!("DocChunk:{}:{}", relative_path, chunk.index);

            let emb = if embeddings_valid {
                embeddings.get(i).cloned()
            } else {
                None
            };

            graph.add_node(code_explorer_core::graph::GraphNode {
                id: chunk_id.clone(),
                label: NodeLabel::DocChunk,
                properties: NodeProperties {
                    name: format!("Chunk {} of {}", chunk.index, relative_path),
                    content: Some(chunk.content.clone()),
                    file_path: full_path.clone(),
                    title: Some(chunk.title.clone()),
                    page_number: Some(chunk.index),
                    embedding: emb.map(|e| e.into_iter().map(|v| v as f64).collect()),
                    ..Default::default()
                },
            });

            // Link Chunk to Document
            let rel_id = format!("rel_chunk_{}", uuid::Uuid::new_v4());
            graph.add_relationship(code_explorer_core::graph::GraphRelationship {
                id: rel_id,
                source_id: chunk_id.clone(),
                target_id: doc_id.clone(),
                rel_type: RelationshipType::BelongsTo,
                confidence: 1.0,
                reason: "Extracted from file".to_string(),
                step: None,
            });
            total_chunks += 1;

            // NER / Mention extraction with word boundary matching
            for (symbol_id, symbol_re) in &known_symbols {
                if symbol_re.is_match(&chunk.content) {
                    let men_rel_id = format!("rel_men_{}", uuid::Uuid::new_v4());
                    graph.add_relationship(code_explorer_core::graph::GraphRelationship {
                        id: men_rel_id,
                        source_id: chunk_id.clone(),
                        target_id: symbol_id.clone(),
                        rel_type: RelationshipType::Mentions,
                        confidence: 0.8,
                        reason: "Text mentions symbol name".to_string(),
                        step: None,
                    });
                    total_mentions += 1;
                }
            }
        }
    }

    println!(
        "  Ingested {} file(s), skipped {} (unsupported or empty)",
        total_files, skipped_files
    );

    // Save the enriched graph
    println!(
        "{} Saving GraphRAG snapshot ({} chunks, {} semantic mentions)...",
        "->".cyan(),
        total_chunks,
        total_mentions
    );
    snapshot::save_snapshot(&graph, &snap_path)
        .map_err(|e| anyhow::anyhow!("Failed to save graph: {}", e))?;

    println!(
        "{} Successfully ingested documentation for GraphRAG.",
        "OK".green()
    );
    Ok(())
}

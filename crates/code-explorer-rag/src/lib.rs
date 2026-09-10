//! GraphRAG integration for Code Explorer.
//!
//! This crate handles the ingestion, chunking, and semantic anchoring
//! of external documentation (Markdown, DOCX) into the knowledge graph.

pub mod chunker;
pub mod docx;
pub mod sfd;
pub mod validator;

use std::path::Path;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

/// Represents a semantic chunk of a document.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocChunk {
    /// The original file path or URL
    pub source_path: String,
    /// Document title or header
    pub title: String,
    /// The extracted text content for this chunk
    pub content: String,
    /// Page number or section index
    pub index: u32,
}

/// Unified entry point: reads a file from disk, dispatches to the right
/// extractor based on its extension, and returns the header-driven chunks.
///
/// Supported formats:
/// - `.md` — markdown, read as UTF-8
/// - `.docx` — Office Open XML, extracted via [`docx::docx_to_markdown`]
///
/// Returns `Ok(vec![])` for unsupported extensions (caller can decide to
/// skip silently).
pub fn chunk_document(path: &Path) -> Result<Vec<DocChunk>> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let source_path = path.to_string_lossy().to_string();

    match ext.as_str() {
        "md" => {
            let content = std::fs::read_to_string(path)
                .with_context(|| format!("read markdown: {}", path.display()))?;
            chunker::chunk_markdown(&source_path, &content)
        }
        "docx" => {
            let md = docx::docx_to_markdown(path)?;
            if md.trim().is_empty() {
                return Ok(Vec::new());
            }
            chunker::chunk_markdown(&source_path, &md)
        }
        _ => {
            tracing::debug!(
                "chunk_document: skipping unsupported extension {:?} for {}",
                ext,
                path.display()
            );
            Ok(Vec::new())
        }
    }
}

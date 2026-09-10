//! Type definitions for the embeddings pipeline.

use serde::{Deserialize, Serialize};

/// Configuration for the embedding model.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EmbeddingConfig {
    /// Model name or path (e.g., "all-MiniLM-L6-v2").
    pub model_name: String,
    /// Embedding vector dimension.
    pub dimension: usize,
    /// Maximum token length per chunk.
    pub max_tokens: usize,
    /// Whether to normalize vectors to unit length.
    pub normalize: bool,
    /// Batch size for inference.
    pub batch_size: usize,
    /// Path to the ONNX model file (optional; used when "embeddings" feature is enabled).
    pub model_path: Option<String>,
    /// Path to the HuggingFace `tokenizer.json` matching the model (optional).
    /// When absent and model_path is set, we look next to the .onnx for
    /// `tokenizer.json`.
    #[serde(default)]
    pub tokenizer_path: Option<String>,
    /// Does the ONNX graph expect `token_type_ids` as an input? True for
    /// classical BERT-family exports (MiniLM, distilbert, BGE < v1.5). False
    /// for DistilBERT-less models and some newer embedding models.
    #[serde(default = "default_token_type_ids")]
    pub needs_token_type_ids: bool,
}

fn default_token_type_ids() -> bool {
    true
}

impl Default for EmbeddingConfig {
    fn default() -> Self {
        Self {
            model_name: "all-MiniLM-L6-v2".to_string(),
            dimension: 384,
            max_tokens: 512,
            normalize: true,
            batch_size: 32,
            model_path: None,
            tokenizer_path: None,
            needs_token_type_ids: true,
        }
    }
}

/// A single semantic (embedding-based) search result.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SemanticSearchResult {
    pub file_path: String,
    /// Cosine similarity score (0.0 to 1.0).
    pub score: f64,
    /// Rank (1-indexed).
    pub rank: usize,
    pub node_id: String,
    pub name: String,
    pub label: String,
    pub start_line: Option<u32>,
    pub end_line: Option<u32>,
}

/// Chunk of text with metadata, ready for embedding.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EmbeddingChunk {
    /// Unique identifier (node ID or content hash).
    pub id: String,
    /// The text to embed.
    pub text: String,
    /// Source file path.
    pub file_path: String,
    /// Node label (Function, Class, etc.)
    pub label: String,
    /// Start line in source file.
    pub start_line: Option<u32>,
    /// End line in source file.
    pub end_line: Option<u32>,
}

/// Stored embedding vector with metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoredEmbedding {
    pub id: String,
    pub vector: Vec<f32>,
    pub file_path: String,
    pub label: String,
    pub name: String,
}

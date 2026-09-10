//! Embeddings pipeline for semantic search.
//!
//! When the `embeddings` feature is enabled, uses ONNX Runtime + HuggingFace
//! tokenizers for sentence-transformer–style models (MiniLM, BGE, etc.).
//! Otherwise, falls back to zero vectors so downstream code keeps working
//! and semantic search silently degrades to BM25-only.

pub mod store;
pub mod types;

pub use store::{load_embeddings, save_embeddings, EmbeddingHeader, EmbeddingStore};
pub use types::{EmbeddingConfig, SemanticSearchResult};

#[cfg(feature = "embeddings")]
mod onnx {
    use ort::session::builder::GraphOptimizationLevel;
    use ort::session::Session;
    use ort::value::Tensor;
    use std::path::{Path, PathBuf};
    use tokenizers::Tokenizer;

    const INPUT_IDS: &str = "input_ids";
    const ATTENTION_MASK: &str = "attention_mask";
    const TOKEN_TYPE_IDS: &str = "token_type_ids";

    pub struct OnnxEmbedder {
        session: Session,
        tokenizer: Tokenizer,
        dims: usize,
        max_len: usize,
        normalize: bool,
        /// Whether the ONNX graph expects `token_type_ids` as an input.
        /// We can't reliably introspect this on ort 2.0.0-rc.12 (the `inputs`
        /// field is private), so callers pass this via config. Defaults to
        /// true which covers BERT-family models (MiniLM, DistilBERT, BGE).
        needs_token_type_ids: bool,
    }

    impl OnnxEmbedder {
        pub fn new(
            model_path: &Path,
            tokenizer_path: &Path,
            dims: usize,
            max_len: usize,
            normalize: bool,
            needs_token_type_ids: bool,
        ) -> Result<Self, Box<dyn std::error::Error>> {
            let session = Session::builder()?
                .with_optimization_level(GraphOptimizationLevel::Level3)?
                .commit_from_file(model_path)?;
            let tokenizer = Tokenizer::from_file(tokenizer_path)
                .map_err(|e| format!("tokenizer load failed: {e}"))?;
            Ok(Self {
                session,
                tokenizer,
                dims,
                max_len,
                normalize,
                needs_token_type_ids,
            })
        }

        /// Resolve the tokenizer path from a model path. Looks for
        /// `tokenizer.json` next to the `.onnx` file.
        pub fn resolve_tokenizer_path(model_path: &Path) -> PathBuf {
            model_path
                .parent()
                .map(|p| p.join("tokenizer.json"))
                .unwrap_or_else(|| PathBuf::from("tokenizer.json"))
        }

        #[allow(dead_code)]
        pub fn dims(&self) -> usize {
            self.dims
        }

        /// Encode a batch of texts into embedding vectors.
        pub fn embed(
            &mut self,
            texts: &[&str],
        ) -> Result<Vec<Vec<f32>>, Box<dyn std::error::Error>> {
            if texts.is_empty() {
                return Ok(Vec::new());
            }

            let encodings = self
                .tokenizer
                .encode_batch(
                    texts.iter().map(|s| (*s).to_string()).collect::<Vec<_>>(),
                    true,
                )
                .map_err(|e| format!("tokenize failed: {e}"))?;

            let batch_size = encodings.len();
            let batch_max_len = encodings
                .iter()
                .map(|e| e.get_ids().len().min(self.max_len))
                .max()
                .unwrap_or(0)
                .max(1);

            let total = batch_size * batch_max_len;
            let mut input_ids = vec![0i64; total];
            let mut attention = vec![0i64; total];
            for (row, enc) in encodings.iter().enumerate() {
                let ids = enc.get_ids();
                let mask = enc.get_attention_mask();
                let len = ids.len().min(batch_max_len);
                let off = row * batch_max_len;
                for col in 0..len {
                    input_ids[off + col] = ids[col] as i64;
                    attention[off + col] = mask[col] as i64;
                }
            }

            let shape: [i64; 2] = [batch_size as i64, batch_max_len as i64];
            let attention_for_pooling = attention.clone();
            let input_ids_tensor = Tensor::from_array((shape, input_ids))?;
            let attention_tensor = Tensor::from_array((shape, attention))?;

            let outputs = if self.needs_token_type_ids {
                let token_type_ids = vec![0i64; total];
                let tt_tensor = Tensor::from_array((shape, token_type_ids))?;
                self.session.run(ort::inputs![
                    INPUT_IDS => input_ids_tensor,
                    ATTENTION_MASK => attention_tensor,
                    TOKEN_TYPE_IDS => tt_tensor,
                ])?
            } else {
                self.session.run(ort::inputs![
                    INPUT_IDS => input_ids_tensor,
                    ATTENTION_MASK => attention_tensor,
                ])?
            };

            // Grab the first output — sentence-transformer exports put the
            // pooled or last_hidden_state first, which covers both the
            // already-pooled and pool-after-inference paths below.
            let (_, first) = outputs
                .iter()
                .next()
                .ok_or("ONNX session returned no outputs")?;
            let tensor = first.try_extract_tensor::<f32>()?;
            let out_shape: Vec<usize> = tensor.0.iter().map(|&d| d as usize).collect();
            let data: &[f32] = tensor.1;

            match out_shape.len() {
                // [batch, dim] — already pooled by the model
                2 => {
                    let dim = out_shape[1];
                    if dim != self.dims {
                        return Err(format!(
                            "model output dim {} does not match config dim {}",
                            dim, self.dims
                        )
                        .into());
                    }
                    let mut result: Vec<Vec<f32>> = Vec::with_capacity(out_shape[0]);
                    for b in 0..out_shape[0] {
                        let start = b * dim;
                        let mut v: Vec<f32> = data[start..start + dim].to_vec();
                        if self.normalize {
                            normalize_in_place(&mut v);
                        }
                        result.push(v);
                    }
                    Ok(result)
                }
                // [batch, seq, dim] — apply mean pooling with attention mask
                3 => {
                    let (bsz, seq, dim) = (out_shape[0], out_shape[1], out_shape[2]);
                    if dim != self.dims {
                        return Err(format!(
                            "model output dim {} does not match config dim {}",
                            dim, self.dims
                        )
                        .into());
                    }
                    let mut result: Vec<Vec<f32>> = Vec::with_capacity(bsz);
                    for b in 0..bsz {
                        let mut pooled = vec![0.0f32; dim];
                        let mut denom = 0.0f32;
                        for t in 0..seq {
                            // Use the (possibly-shorter) attention mask we
                            // actually fed to the model for pooling weights.
                            let m_idx = b * batch_max_len + t.min(batch_max_len - 1);
                            let m = if t < batch_max_len {
                                attention_for_pooling[m_idx] as f32
                            } else {
                                0.0
                            };
                            if m > 0.0 {
                                denom += m;
                                let token_off = (b * seq + t) * dim;
                                for d in 0..dim {
                                    pooled[d] += data[token_off + d] * m;
                                }
                            }
                        }
                        if denom > 0.0 {
                            for v in pooled.iter_mut().take(dim) {
                                *v /= denom;
                            }
                        }
                        if self.normalize {
                            normalize_in_place(&mut pooled);
                        }
                        result.push(pooled);
                    }
                    Ok(result)
                }
                other => Err(format!(
                    "unexpected ONNX output rank {} (shape {:?})",
                    other, out_shape
                )
                .into()),
            }
        }
    }

    fn normalize_in_place(v: &mut [f32]) {
        let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm > 0.0 {
            for x in v.iter_mut() {
                *x /= norm;
            }
        }
    }
}

/// Generate embeddings for a batch of texts.
///
/// When the `embeddings` feature is enabled AND `config.model_path` points to
/// a valid ONNX file with a matching tokenizer, this runs real inference.
/// Otherwise (feature off, model missing, tokenizer missing, runtime error)
/// it returns zero-vectors so callers never crash — downstream code degrades
/// to BM25-only scoring with a one-shot warning so the operator knows why
/// semantic similarity is flat.
pub fn generate_embeddings(texts: &[String], config: &EmbeddingConfig) -> Vec<Vec<f32>> {
    #[cfg(feature = "embeddings")]
    {
        use std::sync::atomic::{AtomicBool, Ordering};
        static WARNED_FALLBACK: AtomicBool = AtomicBool::new(false);
        static WARNED_NO_PATH: AtomicBool = AtomicBool::new(false);

        let Some(model_path) = config.model_path.as_deref() else {
            if !WARNED_NO_PATH.swap(true, Ordering::Relaxed) {
                tracing::warn!(
                    "embeddings feature enabled but config.model_path is empty — \
                     returning zero vectors. Set EmbeddingConfig.model_path to an \
                     .onnx file (e.g. all-MiniLM-L6-v2) to enable semantic search."
                );
            }
            return zero_vectors(texts.len(), config.dimension);
        };
        let model_path = std::path::Path::new(model_path);
        let tokenizer_path: std::path::PathBuf = match config.tokenizer_path.as_deref() {
            Some(p) => std::path::PathBuf::from(p),
            None => onnx::OnnxEmbedder::resolve_tokenizer_path(model_path),
        };
        match onnx::OnnxEmbedder::new(
            model_path,
            &tokenizer_path,
            config.dimension,
            config.max_tokens,
            config.normalize,
            config.needs_token_type_ids,
        ) {
            Ok(mut embedder) => {
                let refs: Vec<&str> = texts.iter().map(|s| s.as_str()).collect();
                match embedder.embed(&refs) {
                    Ok(v) => return v,
                    Err(e) => {
                        if !WARNED_FALLBACK.swap(true, Ordering::Relaxed) {
                            tracing::warn!(
                                error = %e,
                                "embedding inference failed — falling back to zero vectors"
                            );
                        }
                    }
                }
            }
            Err(e) => {
                if !WARNED_FALLBACK.swap(true, Ordering::Relaxed) {
                    tracing::warn!(
                        error = %e,
                        model_path = %model_path.display(),
                        tokenizer_path = %tokenizer_path.display(),
                        "failed to load embedder — falling back to zero vectors"
                    );
                }
            }
        }
    }

    zero_vectors(texts.len(), config.dimension)
}

fn zero_vectors(count: usize, dim: usize) -> Vec<Vec<f32>> {
    (0..count).map(|_| vec![0.0f32; dim]).collect()
}

/// Search for similar embeddings using cosine similarity.
pub fn search_semantic(
    query_embedding: &[f32],
    stored_embeddings: &[(String, Vec<f32>)],
    limit: usize,
) -> Vec<SemanticSearchResult> {
    let mut results: Vec<SemanticSearchResult> = stored_embeddings
        .iter()
        .map(|(id, embedding)| {
            let similarity = cosine_similarity(query_embedding, embedding);
            SemanticSearchResult {
                file_path: String::new(),
                score: similarity,
                rank: 0,
                node_id: id.clone(),
                name: String::new(),
                label: String::new(),
                start_line: None,
                end_line: None,
            }
        })
        .collect();

    results.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    results.truncate(limit);

    for (i, r) in results.iter_mut().enumerate() {
        r.rank = i + 1;
    }

    results
}

fn cosine_similarity(a: &[f32], b: &[f32]) -> f64 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    let dot: f64 = a
        .iter()
        .zip(b.iter())
        .map(|(x, y)| (*x as f64) * (*y as f64))
        .sum();
    let norm_a: f64 = a
        .iter()
        .map(|x| (*x as f64) * (*x as f64))
        .sum::<f64>()
        .sqrt();
    let norm_b: f64 = b
        .iter()
        .map(|x| (*x as f64) * (*x as f64))
        .sum::<f64>()
        .sqrt();
    if norm_a == 0.0 || norm_b == 0.0 {
        return 0.0;
    }
    dot / (norm_a * norm_b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cosine_similarity_identical() {
        let a = vec![1.0f32, 2.0, 3.0];
        let b = vec![1.0f32, 2.0, 3.0];
        let sim = cosine_similarity(&a, &b);
        assert!(
            (sim - 1.0).abs() < 1e-6,
            "identical vectors should have similarity ~1.0, got {sim}"
        );
    }

    #[test]
    fn test_cosine_similarity_orthogonal() {
        let a = vec![1.0f32, 0.0, 0.0];
        let b = vec![0.0f32, 1.0, 0.0];
        let sim = cosine_similarity(&a, &b);
        assert!(
            sim.abs() < 1e-6,
            "orthogonal vectors should have similarity ~0.0, got {sim}"
        );
    }

    #[test]
    fn test_cosine_similarity_opposite() {
        let a = vec![1.0f32, 0.0];
        let b = vec![-1.0f32, 0.0];
        let sim = cosine_similarity(&a, &b);
        assert!(
            (sim - (-1.0)).abs() < 1e-6,
            "opposite vectors should have similarity ~-1.0, got {sim}"
        );
    }

    #[test]
    fn test_cosine_similarity_empty() {
        let sim = cosine_similarity(&[], &[]);
        assert_eq!(sim, 0.0);
    }

    #[test]
    fn test_cosine_similarity_mismatched_lengths() {
        let a = vec![1.0f32, 2.0];
        let b = vec![1.0f32];
        let sim = cosine_similarity(&a, &b);
        assert_eq!(sim, 0.0);
    }

    #[test]
    fn test_cosine_similarity_zero_vector() {
        let a = vec![0.0f32, 0.0, 0.0];
        let b = vec![1.0f32, 2.0, 3.0];
        let sim = cosine_similarity(&a, &b);
        assert_eq!(sim, 0.0);
    }

    #[test]
    fn test_search_semantic_ordering() {
        let query = vec![1.0f32, 0.0, 0.0];
        let stored = vec![
            ("node_a".to_string(), vec![0.0f32, 1.0, 0.0]), // orthogonal
            ("node_b".to_string(), vec![1.0f32, 0.0, 0.0]), // identical
            ("node_c".to_string(), vec![0.5f32, 0.5, 0.0]), // partial match
        ];

        let results = search_semantic(&query, &stored, 10);

        assert_eq!(results.len(), 3);
        assert_eq!(results[0].node_id, "node_b");
        assert_eq!(results[0].rank, 1);
        assert_eq!(results[1].node_id, "node_c");
        assert_eq!(results[1].rank, 2);
        assert_eq!(results[2].node_id, "node_a");
        assert_eq!(results[2].rank, 3);
    }

    #[test]
    fn test_search_semantic_limit() {
        let query = vec![1.0f32, 0.0];
        let stored: Vec<(String, Vec<f32>)> = (0..10)
            .map(|i| (format!("node_{i}"), vec![1.0f32, i as f32]))
            .collect();

        let results = search_semantic(&query, &stored, 3);
        assert_eq!(results.len(), 3);
        assert_eq!(results[0].rank, 1);
        assert_eq!(results[1].rank, 2);
        assert_eq!(results[2].rank, 3);
    }

    #[test]
    fn test_search_semantic_empty() {
        let query = vec![1.0f32, 0.0];
        let results = search_semantic(&query, &[], 10);
        assert!(results.is_empty());
    }

    #[test]
    fn test_generate_embeddings_fallback_no_model() {
        // No model_path set → always zero vectors even with feature enabled.
        let config = EmbeddingConfig::default();
        let texts = vec!["hello world".to_string(), "test".to_string()];
        let embeddings = generate_embeddings(&texts, &config);
        assert_eq!(embeddings.len(), 2);
        assert_eq!(embeddings[0].len(), config.dimension);
        assert!(embeddings[0].iter().all(|&v| v == 0.0));
    }

    /// Real inference smoke test. Requires an ONNX model + tokenizer.json on
    /// disk and an env var `CODE_EXPLORER_TEST_MODEL_PATH` pointing at the `.onnx`.
    /// Ignored by default so CI and first-time contributors don't need a 90MB
    /// model checkout. Run with:
    ///   $env:CODE_EXPLORER_TEST_MODEL_PATH = "C:/path/to/minilm/model.onnx"
    ///   cargo test -p code-explorer-search --features embeddings -- --ignored embed_real
    #[cfg(feature = "embeddings")]
    #[test]
    #[ignore]
    fn embed_real_model_similarity() {
        let Ok(model_path) = std::env::var("CODE_EXPLORER_TEST_MODEL_PATH") else {
            eprintln!("skipped — CODE_EXPLORER_TEST_MODEL_PATH not set");
            return;
        };
        let config = EmbeddingConfig {
            model_path: Some(model_path),
            ..EmbeddingConfig::default()
        };

        let texts = vec![
            "A user logs into the application".to_string(),
            "Authentication flow for an account".to_string(),
            "The cat sat on the mat".to_string(),
        ];
        let vecs = generate_embeddings(&texts, &config);
        assert_eq!(vecs.len(), 3);
        assert_eq!(vecs[0].len(), config.dimension);
        // Non-zero (i.e. inference ran)
        assert!(
            vecs[0].iter().any(|&v| v.abs() > 1e-6),
            "expected non-zero embedding, got all zeros — tokenizer or model path wrong?"
        );

        let sim_related = cosine_similarity(&vecs[0], &vecs[1]);
        let sim_unrelated = cosine_similarity(&vecs[0], &vecs[2]);
        eprintln!(
            "cosine(login, auth flow) = {sim_related:.3}, cosine(login, cat on mat) = {sim_unrelated:.3}"
        );
        // For small models (MiniLM 22M) on indirectly-related sentences,
        // typical cosine is 0.3-0.6. What matters is clear separation from
        // the unrelated pair (which should be ≤ ~0.2 after normalization).
        assert!(
            sim_related > sim_unrelated + 0.1,
            "related texts should be clearly closer than unrelated: \
             related={sim_related:.3}, unrelated={sim_unrelated:.3}"
        );
        assert!(
            sim_related > 0.3,
            "related texts similarity should be > 0.3, got {sim_related:.3} — \
             likely a model/tokenizer/pooling issue"
        );
    }
}

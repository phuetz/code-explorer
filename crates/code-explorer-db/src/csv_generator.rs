//! Streaming CSV generation from a KnowledgeGraph.
//!
//! Produces one CSV file per node label and one for CodeRelation edges.
//! RFC 4180 compliant output via the `csv` crate. Includes LRU caching
//! for source file content, binary detection, UTF-8 sanitization, and
//! content truncation.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use lru::LruCache;
use tracing::warn;

use code_explorer_core::graph::types::{GraphNode, NodeLabel};
use code_explorer_core::graph::KnowledgeGraph;

use crate::error::{DbError, Result};

/// Maximum characters for symbol content (Function, Class, etc.)
const SYMBOL_CONTENT_MAX: usize = 5000;
/// Maximum characters for file content
const FILE_CONTENT_MAX: usize = 10000;
/// LRU cache capacity for source file contents
const FILE_CACHE_CAPACITY: usize = 3000;
/// Number of rows to buffer before flushing to disk
const FLUSH_INTERVAL: usize = 500;
/// Threshold: if more than 10% of bytes are non-printable, treat as binary
const BINARY_THRESHOLD: f64 = 0.10;

/// Check if content is likely binary (>10% non-printable characters).
fn is_binary_content(data: &[u8]) -> bool {
    if data.is_empty() {
        return false;
    }
    let non_printable = data
        .iter()
        .filter(|&&b| {
            // Allow tab, newline, carriage return
            b < 0x20 && b != b'\t' && b != b'\n' && b != b'\r'
        })
        .count();
    (non_printable as f64 / data.len() as f64) > BINARY_THRESHOLD
}

/// Sanitize a string for CSV output: remove null bytes and control characters
/// (except tab/newline/CR), and ensure valid UTF-8.
fn sanitize_utf8(s: &str) -> String {
    s.chars()
        .filter(|&c| {
            // Keep printable chars, tab, newline, CR
            c == '\t' || c == '\n' || c == '\r' || (c >= ' ' && c != '\x7f')
        })
        .collect()
}

/// Truncate content to the specified max length, respecting char boundaries.
fn truncate_content(s: &str, max_len: usize) -> &str {
    if s.len() <= max_len {
        return s;
    }
    // Find the last char boundary at or before max_len
    let mut end = max_len;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}

/// Source file content cache with LRU eviction.
struct FileContentCache {
    cache: LruCache<PathBuf, Option<String>>,
}

impl FileContentCache {
    fn new() -> Self {
        Self {
            cache: LruCache::new(std::num::NonZeroUsize::new(FILE_CACHE_CAPACITY).unwrap()),
        }
    }

    /// Read file content, returning None for binary files or read errors.
    fn get_content(&mut self, file_path: &Path) -> Option<String> {
        if let Some(cached) = self.cache.get(file_path) {
            return cached.clone();
        }

        let result = match std::fs::read(file_path) {
            Ok(bytes) => {
                if is_binary_content(&bytes) {
                    None
                } else {
                    String::from_utf8(bytes).ok().map(|s| sanitize_utf8(&s))
                }
            }
            Err(_) => None,
        };

        self.cache.put(file_path.to_path_buf(), result.clone());
        result
    }
}

/// Extract the source content for a node from the file system, applying
/// line range extraction and truncation.
fn extract_node_content(
    node: &GraphNode,
    repo_root: &Path,
    cache: &mut FileContentCache,
) -> String {
    // Folders never have readable content — bail before touching the cache.
    // The previous order called `cache.get_content` (which `fs::read`s the
    // path) on a directory first, which always errored, returned `None`,
    // and short-circuited at `return String::new()` below — so the Folder
    // branch further down was unreachable dead code and every Folder node
    // produced a spurious filesystem read error. Handle Folder explicitly
    // first.
    if node.label == NodeLabel::Folder {
        return String::new();
    }

    let file_path = repo_root.join(&node.properties.file_path);
    let full_content = match cache.get_content(&file_path) {
        Some(c) => c,
        None => return String::new(),
    };

    let max_len = if node.label == NodeLabel::File {
        FILE_CONTENT_MAX
    } else {
        SYMBOL_CONTENT_MAX
    };

    // For files, return the whole (truncated) content
    if node.label == NodeLabel::File {
        return truncate_content(&full_content, max_len).to_string();
    }

    // For symbols with line ranges, extract the relevant slice
    let start = node.properties.start_line.unwrap_or(1).max(1) as usize;
    let end = node.properties.end_line.unwrap_or(u32::MAX) as usize;

    let lines: Vec<&str> = full_content.lines().collect();
    if start > lines.len() {
        return String::new();
    }

    let slice_start = start - 1;
    let slice_end = end.min(lines.len());
    // Guard against malformed line ranges where end < start (e.g. corrupted
    // snapshot data or a future parser bug). `lines[4..3]` would panic with
    // "slice index starts at 4 but ends at 3", crashing CSV generation for
    // an entire node label table.
    if slice_start >= slice_end {
        return String::new();
    }
    let extracted: String = lines[slice_start..slice_end].join("\n");
    truncate_content(&extracted, max_len).to_string()
}

/// Generate all CSV files for nodes and relationships from a KnowledgeGraph.
///
/// Creates one CSV per node label (e.g., `Function.csv`, `Class.csv`) and
/// one `CodeRelation.csv` for edges. Files are written to `output_dir`.
///
/// Returns the paths of all generated CSV files.
pub fn generate_all_csvs(
    graph: &KnowledgeGraph,
    repo_root: &Path,
    output_dir: &Path,
) -> Result<Vec<PathBuf>> {
    std::fs::create_dir_all(output_dir).map_err(|e| DbError::CsvError {
        table: "output_dir".into(),
        cause: e.to_string(),
    })?;

    let mut generated = Vec::new();

    // Group nodes by label
    let node_csvs = generate_node_csvs(graph, repo_root, output_dir)?;
    generated.extend(node_csvs);

    let rel_csv = generate_relation_csv(graph, output_dir)?;
    generated.push(rel_csv);

    Ok(generated)
}

/// Generate one CSV file per node label from the knowledge graph.
pub fn generate_node_csvs(
    graph: &KnowledgeGraph,
    repo_root: &Path,
    output_dir: &Path,
) -> Result<Vec<PathBuf>> {
    // Group nodes by label string
    let mut by_label: HashMap<String, Vec<&GraphNode>> = HashMap::new();
    for node in graph.iter_nodes() {
        by_label
            .entry(node.label.as_str().to_string())
            .or_default()
            .push(node);
    }

    let mut cache = FileContentCache::new();
    let mut generated = Vec::new();

    for (label, nodes) in &by_label {
        let csv_path = output_dir.join(format!("{label}.csv"));
        let file = std::fs::File::create(&csv_path).map_err(|e| DbError::CsvError {
            table: label.clone(),
            cause: e.to_string(),
        })?;
        let mut writer = csv::WriterBuilder::new()
            .has_headers(true)
            .from_writer(std::io::BufWriter::new(file));

        // Write header
        writer
            .write_record([
                "id",
                "name",
                "filePath",
                "content",
                "startLine",
                "endLine",
                "language",
                "isExported",
            ])
            .map_err(|e| DbError::CsvError {
                table: label.clone(),
                cause: e.to_string(),
            })?;

        let mut rows_since_flush = 0;

        for node in nodes {
            let content = extract_node_content(node, repo_root, &mut cache);
            let start_line = node
                .properties
                .start_line
                .map(|v| v.to_string())
                .unwrap_or_default();
            let end_line = node
                .properties
                .end_line
                .map(|v| v.to_string())
                .unwrap_or_default();
            let language = node
                .properties
                .language
                .map(|l| l.as_str().to_string())
                .unwrap_or_default();
            let is_exported = node
                .properties
                .is_exported
                .map(|b| if b { "true" } else { "false" })
                .unwrap_or("false")
                .to_string();

            writer
                .write_record([
                    &node.id,
                    &node.properties.name,
                    &node.properties.file_path,
                    &content,
                    &start_line,
                    &end_line,
                    &language,
                    &is_exported,
                ])
                .map_err(|e| DbError::CsvError {
                    table: label.clone(),
                    cause: e.to_string(),
                })?;

            rows_since_flush += 1;
            if rows_since_flush >= FLUSH_INTERVAL {
                writer.flush().map_err(|e| DbError::CsvError {
                    table: label.clone(),
                    cause: e.to_string(),
                })?;
                rows_since_flush = 0;
            }
        }

        writer.flush().map_err(|e| DbError::CsvError {
            table: label.clone(),
            cause: e.to_string(),
        })?;

        generated.push(csv_path);
    }

    Ok(generated)
}

/// Generate the CodeRelation CSV from graph relationships.
pub fn generate_relation_csv(graph: &KnowledgeGraph, output_dir: &Path) -> Result<PathBuf> {
    let csv_path = output_dir.join("CodeRelation.csv");
    let file = std::fs::File::create(&csv_path).map_err(|e| DbError::CsvError {
        table: "CodeRelation".into(),
        cause: e.to_string(),
    })?;
    let mut writer = csv::WriterBuilder::new()
        .has_headers(true)
        .from_writer(std::io::BufWriter::new(file));

    writer
        .write_record(["from", "to", "type", "confidence", "reason", "step"])
        .map_err(|e| DbError::CsvError {
            table: "CodeRelation".into(),
            cause: e.to_string(),
        })?;

    let mut rows_since_flush = 0;

    graph.for_each_relationship(|rel| {
        let confidence = format!("{:.4}", rel.confidence);
        let step = rel.step.map(|s| s.to_string()).unwrap_or_default();
        let reason = sanitize_utf8(&rel.reason);

        if let Err(e) = writer.write_record([
            &rel.source_id,
            &rel.target_id,
            rel.rel_type.as_str(),
            &confidence,
            &reason,
            &step,
        ]) {
            warn!("Failed to write relation row: {e}");
        }

        rows_since_flush += 1;
        if rows_since_flush >= FLUSH_INTERVAL {
            let _ = writer.flush();
            rows_since_flush = 0;
        }
    });

    writer.flush().map_err(|e| DbError::CsvError {
        table: "CodeRelation".into(),
        cause: e.to_string(),
    })?;

    Ok(csv_path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_binary_content() {
        assert!(!is_binary_content(b"Hello, world!\n"));
        assert!(!is_binary_content(b"fn main() {\n\tprintln!(\"hi\");\n}"));
        // More than 10% null bytes => binary
        let mut data = vec![0u8; 20];
        data.extend_from_slice(b"some text here");
        assert!(is_binary_content(&data));
        assert!(!is_binary_content(b""));
    }

    #[test]
    fn test_sanitize_utf8() {
        assert_eq!(sanitize_utf8("hello\x00world"), "helloworld");
        assert_eq!(sanitize_utf8("tab\there"), "tab\there");
        assert_eq!(sanitize_utf8("line\nbreak"), "line\nbreak");
        assert_eq!(sanitize_utf8("bell\x07ring"), "bellring");
    }

    #[test]
    fn test_truncate_content() {
        let s = "abcdefghij";
        assert_eq!(truncate_content(s, 5), "abcde");
        assert_eq!(truncate_content(s, 100), "abcdefghij");
        // Multi-byte: ensure we don't split a character
        let emoji = "Hello 🌍 world";
        let truncated = truncate_content(emoji, 8);
        assert!(truncated.len() <= 8);
        // Should be valid UTF-8
        assert!(std::str::from_utf8(truncated.as_bytes()).is_ok());
    }

    #[test]
    fn test_extract_node_content_inverted_range() {
        use code_explorer_core::graph::types::{GraphNode, NodeProperties};

        // Create a temp file with known content
        let tmp_dir = std::env::temp_dir().join("code_explorer_csv_test_inverted");
        let _ = std::fs::create_dir_all(&tmp_dir);
        let file_name = "src.txt";
        let file_path = tmp_dir.join(file_name);
        std::fs::write(&file_path, "line1\nline2\nline3\nline4\nline5\n").unwrap();

        // Malformed node: end_line (3) < start_line (5) — would have panicked
        // before the slice_start >= slice_end guard in extract_node_content.
        let mut node = GraphNode {
            id: "Function:src.txt:bad".into(),
            label: NodeLabel::Function,
            properties: NodeProperties::default(),
        };
        node.properties.name = "bad".into();
        node.properties.file_path = file_name.into();
        node.properties.start_line = Some(5);
        node.properties.end_line = Some(3);

        let mut cache = FileContentCache::new();
        // Must not panic, must return empty string
        let content = extract_node_content(&node, &tmp_dir, &mut cache);
        assert_eq!(content, "");

        // Also test the original happy path still works
        node.properties.start_line = Some(2);
        node.properties.end_line = Some(4);
        let content = extract_node_content(&node, &tmp_dir, &mut cache);
        assert_eq!(content, "line2\nline3\nline4");

        // Cleanup
        let _ = std::fs::remove_dir_all(&tmp_dir);
    }
}

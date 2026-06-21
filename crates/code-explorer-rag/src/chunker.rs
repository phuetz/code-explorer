//! Text chunking strategies for different document types.

use crate::DocChunk;
use anyhow::Result;
use pulldown_cmark::{CodeBlockKind, Event, Parser, Tag};

/// Maximum byte size of a single chunk before it gets split on paragraph
/// boundaries. Set to ~6 KB which is roughly 1 500 tokens for typical prose
/// (assuming ~4 chars/token for English/French) — well within the embedding
/// window of MiniLM-L6-v2 (512 tokens, ~2 KB) without diluting the signal,
/// and large enough that most natural section bodies stay as one chunk.
///
/// Pages exceeding this when chunked by heading alone (the previous behavior)
/// caused embedding signal dilution observed on Sample's `enrich_aspnet_mvc`
/// (1 000+ LOC method docs) which dropped out of top-5 hybrid search results
/// despite being highly relevant under BM25 alone.
const MAX_CHUNK_BYTES: usize = 6_000;

/// Chunk a Markdown document intelligently based on Headers.
///
/// Each heading starts a new chunk. Content between headings is accumulated
/// into the current chunk. Code blocks and inline code are preserved.
/// Sections whose body exceeds [`MAX_CHUNK_BYTES`] are split on paragraph
/// boundaries (`\n\n`) into multiple sub-chunks sharing the heading's title
/// suffixed with `(part N/M)` so the embedding can still distinguish them.
pub fn chunk_markdown(source_path: &str, content: &str) -> Result<Vec<DocChunk>> {
    let parser = Parser::new(content);
    let mut chunks = Vec::new();

    let mut current_title = String::from("Document Start");
    let mut current_content = String::new();
    let mut index: u32 = 0;
    let mut in_heading = false;
    let mut in_code_block = false;

    for event in parser {
        match event {
            Event::Start(Tag::Heading(_, _, _)) => {
                // Save accumulated content as a chunk (may be split if oversized)
                if !current_content.trim().is_empty() {
                    push_chunks(
                        &mut chunks,
                        &mut index,
                        source_path,
                        &current_title,
                        current_content.trim(),
                    );
                    current_content.clear();
                }
                current_title.clear();
                in_heading = true;
            }
            Event::End(Tag::Heading(_, _, _)) => {
                in_heading = false;
            }
            Event::Start(Tag::CodeBlock(kind)) => {
                in_code_block = true;
                current_content.push_str("\n```");
                if let CodeBlockKind::Fenced(lang) = kind {
                    current_content.push_str(&lang);
                }
                current_content.push('\n');
            }
            Event::End(Tag::CodeBlock(_)) => {
                in_code_block = false;
                current_content.push_str("```\n");
            }
            Event::Text(text) => {
                if in_heading {
                    current_title.push_str(&text);
                } else if in_code_block {
                    // Preserve code block text verbatim (no trailing space)
                    current_content.push_str(&text);
                } else {
                    current_content.push_str(&text);
                }
            }
            Event::Code(text) => {
                current_content.push('`');
                current_content.push_str(&text);
                current_content.push_str("` ");
            }
            Event::SoftBreak | Event::HardBreak => {
                current_content.push('\n');
            }
            // Preserve paragraph boundaries with a blank line so the
            // oversized-chunk splitter (split_oversized) can find them.
            // Without this, pulldown_cmark drops the inter-paragraph newlines
            // and the whole section body merges into one undelimited blob.
            Event::End(Tag::Paragraph) => {
                if !current_content.is_empty() && !current_content.ends_with("\n\n") {
                    current_content.push_str("\n\n");
                }
            }
            _ => {}
        }
    }

    // Push the last chunk. Emit even when the body is empty, as long as we
    // have a title — otherwise a document ending with a trailing heading
    // like `# Conclusion` with no body underneath is silently dropped, and
    // the heading vanishes from the RAG index.
    if !current_content.trim().is_empty() {
        push_chunks(
            &mut chunks,
            &mut index,
            source_path,
            &current_title,
            current_content.trim(),
        );
    } else if !current_title.is_empty() {
        chunks.push(DocChunk {
            source_path: source_path.to_string(),
            title: current_title.clone(),
            content: String::new(),
            index,
        });
    }

    Ok(chunks)
}

/// Append one or more chunks to `out`, splitting `body` on paragraph
/// boundaries when it exceeds [`MAX_CHUNK_BYTES`]. Pieces inherit the same
/// `title` suffixed with `(part N/M)` when there's more than one.
fn push_chunks(
    out: &mut Vec<DocChunk>,
    index: &mut u32,
    source_path: &str,
    title: &str,
    body: &str,
) {
    let pieces = split_oversized(body, MAX_CHUNK_BYTES);
    let total = pieces.len();
    for (i, piece) in pieces.iter().enumerate() {
        let part_title = if total > 1 {
            format!("{} (part {}/{})", title, i + 1, total)
        } else {
            title.to_string()
        };
        out.push(DocChunk {
            source_path: source_path.to_string(),
            title: part_title,
            content: piece.clone(),
            index: *index,
        });
        *index += 1;
    }
}

/// Greedy packing: walk paragraph boundaries (`\n\n`) and accumulate them
/// into the current chunk; flush whenever adding the next paragraph would
/// exceed `max_bytes`. A single paragraph larger than `max_bytes` is kept
/// intact (we never split mid-paragraph because that breaks code blocks
/// and citation precision — accepting a few oversized chunks is the lesser
/// evil per the agile-up.com 2026 RAG analysis).
fn split_oversized(body: &str, max_bytes: usize) -> Vec<String> {
    if body.len() <= max_bytes {
        return vec![body.to_string()];
    }
    let mut out = Vec::new();
    let mut current = String::new();
    for paragraph in body.split("\n\n") {
        let separator_len = if current.is_empty() { 0 } else { 2 };
        if !current.is_empty() && current.len() + separator_len + paragraph.len() > max_bytes {
            out.push(current.trim().to_string());
            current.clear();
        }
        if !current.is_empty() {
            current.push_str("\n\n");
        }
        current.push_str(paragraph);
    }
    if !current.trim().is_empty() {
        out.push(current.trim().to_string());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_markdown_chunking() {
        let md = "
# Intro
This is the intro.

## Details
Here are the details.
        ";

        let chunks = chunk_markdown("test.md", md).unwrap();
        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[0].title, "Intro");
        assert!(chunks[0].content.contains("This is the intro."));
        assert_eq!(chunks[1].title, "Details");
        assert!(chunks[1].content.contains("Here are the details."));
    }

    #[test]
    fn test_heading_with_emphasis() {
        let md = "# Hello *World*\nSome content here.";
        let chunks = chunk_markdown("test.md", md).unwrap();
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].title, "Hello World");
        assert!(chunks[0].content.contains("Some content here."));
    }

    #[test]
    fn test_code_block_preserved() {
        let md = "# API\n\n```rust\nfn main() {}\n```\n";
        let chunks = chunk_markdown("test.md", md).unwrap();
        assert_eq!(chunks.len(), 1);
        assert!(chunks[0].content.contains("fn main()"));
        assert!(chunks[0].content.contains("```rust"));
        assert!(chunks[0].content.contains("```"));
    }

    #[test]
    fn test_empty_section_skipped() {
        let md = "# First\nContent.\n\n# Empty\n\n# Third\nMore content.";
        let chunks = chunk_markdown("test.md", md).unwrap();
        // Empty section between First and Third should be skipped
        assert!(chunks.len() >= 2);
        assert!(chunks.iter().all(|c| !c.content.trim().is_empty()));
    }

    #[test]
    fn test_inline_code() {
        let md = "# Docs\nUse `foo()` to start.";
        let chunks = chunk_markdown("test.md", md).unwrap();
        assert!(chunks[0].content.contains("`foo()`"));
    }

    #[test]
    fn test_oversized_section_is_split_on_paragraph_boundaries() {
        // Build a section whose body comfortably exceeds MAX_CHUNK_BYTES.
        // Each paragraph is ~2 KB; we generate 5 of them = ~10 KB total.
        let para = "lorem ipsum dolor sit amet ".repeat(80); // ~2 200 bytes
        let body = (0..5)
            .map(|_| para.clone())
            .collect::<Vec<_>>()
            .join("\n\n");
        let md = format!("# Big Section\n\n{}", body);

        let chunks = chunk_markdown("test.md", &md).unwrap();

        // Should have produced several chunks, each ≤ MAX_CHUNK_BYTES.
        assert!(
            chunks.len() > 1,
            "expected multi-part split, got {}",
            chunks.len()
        );
        for c in &chunks {
            assert!(
                c.content.len() <= MAX_CHUNK_BYTES,
                "chunk too big: {} bytes (limit {})",
                c.content.len(),
                MAX_CHUNK_BYTES
            );
        }
        // Titles should be annotated as parts so embeddings can distinguish them.
        assert!(
            chunks[0].title.contains("(part 1/"),
            "got: {}",
            chunks[0].title
        );
        assert!(chunks
            .last()
            .unwrap()
            .title
            .contains(&format!("/{})", chunks.len())));
    }

    #[test]
    fn test_small_section_keeps_single_chunk_with_unchanged_title() {
        // Sanity check: small bodies must NOT get the (part N/M) suffix.
        let md = "# Small\n\nShort body.";
        let chunks = chunk_markdown("test.md", md).unwrap();
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].title, "Small");
    }

    #[test]
    fn test_single_oversized_paragraph_kept_intact() {
        // A single paragraph bigger than the cap is preserved as-is —
        // splitting mid-paragraph would break code blocks and citation
        // precision. We accept oversized chunks in that pathological case.
        let huge = "x".repeat(MAX_CHUNK_BYTES * 2);
        let md = format!("# Huge\n\n{}", huge);
        let chunks = chunk_markdown("test.md", &md).unwrap();
        assert_eq!(chunks.len(), 1);
        assert!(chunks[0].content.len() > MAX_CHUNK_BYTES);
    }
}

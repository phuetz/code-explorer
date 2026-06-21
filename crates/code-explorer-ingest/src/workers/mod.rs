use rayon::prelude::*;
use std::sync::atomic::{AtomicUsize, Ordering};

use crate::phases::parsing::ExtractedData;
use crate::phases::structure::FileEntry;

pub const CHUNK_BYTE_BUDGET: usize = 20 * 1024 * 1024; // 20MB

/// Split files into chunks by total byte size.
pub fn chunk_files(files: &[FileEntry], budget: usize) -> Vec<Vec<usize>> {
    let mut chunks = Vec::new();
    let mut current_chunk = Vec::new();
    let mut current_size = 0;

    for (i, file) in files.iter().enumerate() {
        if current_size + file.size > budget && !current_chunk.is_empty() {
            chunks.push(std::mem::take(&mut current_chunk));
            current_size = 0;
        }
        current_chunk.push(i);
        current_size += file.size;
    }
    if !current_chunk.is_empty() {
        chunks.push(current_chunk);
    }
    chunks
}

/// Process files in parallel using rayon, reporting progress via callback.
pub fn parallel_parse<F>(
    files: &[FileEntry],
    process_fn: F,
    progress_counter: &AtomicUsize,
) -> Vec<ExtractedData>
where
    F: Fn(&FileEntry) -> ExtractedData + Send + Sync,
{
    files
        .par_iter()
        .map(|file| {
            let result = process_fn(file);
            progress_counter.fetch_add(1, Ordering::Relaxed);
            result
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use code_explorer_core::config::languages::SupportedLanguage;

    fn make_file(path: &str, size: usize) -> FileEntry {
        FileEntry {
            path: path.to_string(),
            content: String::new(),
            size,
            language: SupportedLanguage::from_filename(path),
        }
    }

    #[test]
    fn test_chunk_files_single_chunk() {
        let files = vec![
            make_file("a.ts", 100),
            make_file("b.ts", 200),
            make_file("c.ts", 300),
        ];
        let chunks = chunk_files(&files, 1000);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0], vec![0, 1, 2]);
    }

    #[test]
    fn test_chunk_files_multiple_chunks() {
        let files = vec![
            make_file("a.ts", 500),
            make_file("b.ts", 600),
            make_file("c.ts", 400),
            make_file("d.ts", 300),
        ];
        let chunks = chunk_files(&files, 1000);
        // a(500) + b(600) = 1100 > 1000, so b starts a new chunk
        // b(600) + c(400) = 1000 <= 1000, so c stays in same chunk
        // b(600) + c(400) + d(300) = 1300 > 1000, so d starts a new chunk
        assert_eq!(chunks.len(), 3);
        assert_eq!(chunks[0], vec![0]);
        assert_eq!(chunks[1], vec![1, 2]);
        assert_eq!(chunks[2], vec![3]);
    }

    #[test]
    fn test_chunk_files_empty() {
        let files: Vec<FileEntry> = vec![];
        let chunks = chunk_files(&files, 1000);
        assert!(chunks.is_empty());
    }

    #[test]
    fn test_chunk_files_single_large_file() {
        // A single file larger than the budget should still be in its own chunk
        let files = vec![make_file("big.ts", 50_000_000)];
        let chunks = chunk_files(&files, CHUNK_BYTE_BUDGET);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0], vec![0]);
    }

    #[test]
    fn test_chunk_files_all_same_size() {
        let files: Vec<_> = (0..10)
            .map(|i| make_file(&format!("{i}.ts"), 100))
            .collect();
        // Budget 350 means ~3 files per chunk (300 <= 350, 400 > 350)
        let chunks = chunk_files(&files, 350);
        // 0,1,2 (300) then 3 (400>350 triggers new), 3,4,5 (300), 6 triggers new, 6,7,8 (300), 9 triggers new
        assert_eq!(chunks.len(), 4);
        assert_eq!(chunks[0], vec![0, 1, 2]);
        assert_eq!(chunks[1], vec![3, 4, 5]);
        assert_eq!(chunks[2], vec![6, 7, 8]);
        assert_eq!(chunks[3], vec![9]);
    }

    #[test]
    fn test_parallel_parse_progress() {
        let files = vec![
            make_file("a.ts", 100),
            make_file("b.ts", 200),
            make_file("c.ts", 300),
        ];
        let counter = AtomicUsize::new(0);
        let results = parallel_parse(&files, |_file| ExtractedData::default(), &counter);
        assert_eq!(results.len(), 3);
        assert_eq!(counter.load(Ordering::Relaxed), 3);
    }
}

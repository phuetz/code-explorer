//! On-disk storage for embedding vectors.
//!
//! Flat binary format to minimise cost vs JSON (a 50k-symbol repo at 384d
//! would be ~150MB as JSON numbers, vs ~75MB as raw f32 bytes).
//!
//! File layout (little-endian):
//!   magic      : 8 bytes = "GNEMB001"
//!   header_len : u32
//!   header     : UTF-8 JSON of `EmbeddingHeader`
//!   for each of `header.count` entries:
//!       node_id_len : u32
//!       node_id     : UTF-8 bytes (length = node_id_len)
//!       vector      : [f32; header.dimension]

use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::{self, BufReader, BufWriter, Read, Write};
use std::path::Path;

const MAGIC: &[u8; 8] = b"GNEMB001";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct EmbeddingHeader {
    pub model_name: String,
    pub dimension: usize,
    pub count: usize,
    /// ISO-8601 UTC timestamp of when the embeddings were computed.
    pub generated_at: String,
}

#[derive(Debug, Clone)]
pub struct EmbeddingStore {
    pub header: EmbeddingHeader,
    /// (node_id, vector) pairs. Vectors must all have length `header.dimension`.
    pub entries: Vec<(String, Vec<f32>)>,
}

pub fn save_embeddings(path: &Path, store: &EmbeddingStore) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let file = File::create(path)?;
    let mut w = BufWriter::new(file);

    w.write_all(MAGIC)?;

    let header_json = serde_json::to_vec(&store.header)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    let hlen = u32::try_from(header_json.len())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "header too large"))?;
    w.write_all(&hlen.to_le_bytes())?;
    w.write_all(&header_json)?;

    let dim = store.header.dimension;
    if store.entries.len() != store.header.count {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "header.count={} but entries={}",
                store.header.count,
                store.entries.len()
            ),
        ));
    }

    for (id, vec) in &store.entries {
        if vec.len() != dim {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "vector for {} has dim {} but header.dimension={}",
                    id,
                    vec.len(),
                    dim
                ),
            ));
        }
        let id_bytes = id.as_bytes();
        let id_len = u32::try_from(id_bytes.len())
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "node_id too long"))?;
        w.write_all(&id_len.to_le_bytes())?;
        w.write_all(id_bytes)?;
        // Write f32 values as little-endian bytes. On little-endian hosts
        // (x86, ARM LE) this is equivalent to a direct slice cast; we do it
        // by-element for portability.
        for v in vec {
            w.write_all(&v.to_le_bytes())?;
        }
    }
    w.flush()?;
    Ok(())
}

pub fn load_embeddings(path: &Path) -> io::Result<EmbeddingStore> {
    let file = File::open(path)?;
    let mut r = BufReader::new(file);

    let mut magic = [0u8; 8];
    r.read_exact(&mut magic)?;
    if &magic != MAGIC {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "bad magic: expected {:?}, got {:?}",
                std::str::from_utf8(MAGIC).unwrap(),
                String::from_utf8_lossy(&magic)
            ),
        ));
    }

    let mut hlen_bytes = [0u8; 4];
    r.read_exact(&mut hlen_bytes)?;
    let hlen = u32::from_le_bytes(hlen_bytes) as usize;
    let mut header_bytes = vec![0u8; hlen];
    r.read_exact(&mut header_bytes)?;
    let header: EmbeddingHeader = serde_json::from_slice(&header_bytes)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

    let dim = header.dimension;
    let mut entries = Vec::with_capacity(header.count);
    for _ in 0..header.count {
        let mut idlen_bytes = [0u8; 4];
        r.read_exact(&mut idlen_bytes)?;
        let idlen = u32::from_le_bytes(idlen_bytes) as usize;
        let mut id_bytes = vec![0u8; idlen];
        r.read_exact(&mut id_bytes)?;
        let id = String::from_utf8(id_bytes)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

        let mut vec_bytes = vec![0u8; dim * 4];
        r.read_exact(&mut vec_bytes)?;
        let mut vec = Vec::with_capacity(dim);
        for i in 0..dim {
            let off = i * 4;
            vec.push(f32::from_le_bytes([
                vec_bytes[off],
                vec_bytes[off + 1],
                vec_bytes[off + 2],
                vec_bytes[off + 3],
            ]));
        }
        entries.push((id, vec));
    }

    Ok(EmbeddingStore { header, entries })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;

    #[test]
    fn roundtrip_basic() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("emb.bin");
        let store = EmbeddingStore {
            header: EmbeddingHeader {
                model_name: "test".into(),
                dimension: 3,
                count: 2,
                generated_at: "2026-04-24T00:00:00Z".into(),
            },
            entries: vec![
                ("node:a".into(), vec![1.0, 2.0, 3.0]),
                ("node:b".into(), vec![-0.5, 0.25, 1e-6]),
            ],
        };
        save_embeddings(&path, &store).unwrap();
        let loaded = load_embeddings(&path).unwrap();
        assert_eq!(loaded.header, store.header);
        assert_eq!(loaded.entries.len(), store.entries.len());
        for (a, b) in loaded.entries.iter().zip(store.entries.iter()) {
            assert_eq!(a.0, b.0);
            assert_eq!(a.1, b.1);
        }
    }

    #[test]
    fn roundtrip_empty_corpus() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("emb.bin");
        let store = EmbeddingStore {
            header: EmbeddingHeader {
                model_name: "test".into(),
                dimension: 5,
                count: 0,
                generated_at: "2026-04-24T00:00:00Z".into(),
            },
            entries: vec![],
        };
        save_embeddings(&path, &store).unwrap();
        let loaded = load_embeddings(&path).unwrap();
        assert_eq!(loaded.header.count, 0);
        assert!(loaded.entries.is_empty());
    }

    #[test]
    fn bad_magic_is_rejected() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("bad.bin");
        {
            let mut f = File::create(&path).unwrap();
            f.write_all(b"XXXXXXXX").unwrap();
            f.write_all(&0u32.to_le_bytes()).unwrap();
        }
        let err = load_embeddings(&path).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::InvalidData);
    }

    #[test]
    fn mismatched_count_rejected_on_save() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("emb.bin");
        let store = EmbeddingStore {
            header: EmbeddingHeader {
                model_name: "test".into(),
                dimension: 2,
                count: 99,
                generated_at: "2026-04-24T00:00:00Z".into(),
            },
            entries: vec![("only".into(), vec![1.0, 2.0])],
        };
        let err = save_embeddings(&path, &store).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::InvalidData);
    }

    #[test]
    fn dim_mismatch_rejected_on_save() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("emb.bin");
        let store = EmbeddingStore {
            header: EmbeddingHeader {
                model_name: "test".into(),
                dimension: 5,
                count: 1,
                generated_at: "2026-04-24T00:00:00Z".into(),
            },
            entries: vec![("x".into(), vec![1.0, 2.0, 3.0])], // dim=3 != 5
        };
        let err = save_embeddings(&path, &store).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::InvalidData);
    }

    #[test]
    fn truncated_body_fails_load() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("emb.bin");
        let store = EmbeddingStore {
            header: EmbeddingHeader {
                model_name: "test".into(),
                dimension: 4,
                count: 3,
                generated_at: "2026-04-24T00:00:00Z".into(),
            },
            entries: vec![
                ("a".into(), vec![1.0, 2.0, 3.0, 4.0]),
                ("b".into(), vec![5.0, 6.0, 7.0, 8.0]),
                ("c".into(), vec![9.0, 10.0, 11.0, 12.0]),
            ],
        };
        save_embeddings(&path, &store).unwrap();
        // Truncate the file mid-body
        let full = std::fs::read(&path).unwrap();
        let truncated = &full[..full.len() - 10];
        std::fs::write(&path, truncated).unwrap();
        let err = load_embeddings(&path).unwrap_err();
        assert!(matches!(
            err.kind(),
            io::ErrorKind::UnexpectedEof | io::ErrorKind::InvalidData
        ));
    }
}

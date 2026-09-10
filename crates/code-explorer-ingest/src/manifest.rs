//! File manifest for incremental indexing.
//!
//! Tracks the SHA-256 hash, size, and modification time of each source file
//! so that subsequent runs can detect added / modified / removed files
//! without re-parsing the entire repository.

use std::collections::HashMap;
use std::fs;
use std::io::{self, Read};
use std::path::Path;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tracing::debug;

// ─── Types ───────────────────────────────────────────────────────────────

/// Content-addressable digest for a single file.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FileDigest {
    /// SHA-256 hex string of the file content.
    pub hash: String,
    /// File size in bytes.
    pub size: u64,
    /// Last-modified Unix timestamp (seconds since epoch).
    pub modified: u64,
}

/// Complete manifest: relative-path -> FileDigest.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct FileManifest {
    pub files: HashMap<String, FileDigest>,
}

/// A detected change between two manifests.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileChange {
    Added(String),
    Modified(String),
    Removed(String),
}

// ─── Hash computation ────────────────────────────────────────────────────

/// Compute the SHA-256 hex digest of a file's content.
pub fn compute_hash(path: &Path) -> io::Result<String> {
    let mut file = fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 8192];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

/// Compute SHA-256 hex digest from an in-memory string.
pub fn compute_hash_from_content(content: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());
    format!("{:x}", hasher.finalize())
}

// ─── Manifest construction ───────────────────────────────────────────────

/// Build a [`FileManifest`] from a list of `(relative_path, absolute_path)` pairs.
pub fn build_manifest(files: &[(&str, &Path)]) -> FileManifest {
    let mut manifest = FileManifest::default();

    for &(rel_path, abs_path) in files {
        match build_digest(abs_path) {
            Ok(digest) => {
                manifest.files.insert(rel_path.to_string(), digest);
            }
            Err(e) => {
                debug!(path = %rel_path, error = %e, "Skipping file in manifest");
            }
        }
    }

    manifest
}

/// Build a [`FileManifest`] from file entries (using content hashing).
pub fn build_manifest_from_entries(
    entries: &[crate::phases::structure::FileEntry],
) -> FileManifest {
    let mut manifest = FileManifest::default();

    for entry in entries {
        let digest = FileDigest {
            hash: compute_hash_from_content(&entry.content),
            size: entry.size as u64,
            modified: 0, // Not available from FileEntry
        };
        manifest.files.insert(entry.path.clone(), digest);
    }

    manifest
}

fn build_digest(abs_path: &Path) -> io::Result<FileDigest> {
    let metadata = fs::metadata(abs_path)?;
    let modified = metadata
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let hash = compute_hash(abs_path)?;

    Ok(FileDigest {
        hash,
        size: metadata.len(),
        modified,
    })
}

// ─── Diff ────────────────────────────────────────────────────────────────

/// Compare two manifests and return a list of changes.
///
/// A file is **added** if it appears in `new` but not in `old`.
/// A file is **modified** if it appears in both but the hash or size differs.
/// A file is **removed** if it appears in `old` but not in `new`.
pub fn diff_manifests(old: &FileManifest, new: &FileManifest) -> Vec<FileChange> {
    let mut changes = Vec::new();

    // Check for added / modified files
    for (path, new_digest) in &new.files {
        match old.files.get(path) {
            None => changes.push(FileChange::Added(path.clone())),
            Some(old_digest) => {
                if old_digest.hash != new_digest.hash || old_digest.size != new_digest.size {
                    changes.push(FileChange::Modified(path.clone()));
                }
            }
        }
    }

    // Check for removed files
    for path in old.files.keys() {
        if !new.files.contains_key(path) {
            changes.push(FileChange::Removed(path.clone()));
        }
    }

    // Sort for deterministic ordering
    changes.sort_by(|a, b| {
        let a_path = match a {
            FileChange::Added(p) | FileChange::Modified(p) | FileChange::Removed(p) => p,
        };
        let b_path = match b {
            FileChange::Added(p) | FileChange::Modified(p) | FileChange::Removed(p) => p,
        };
        a_path.cmp(b_path)
    });

    changes
}

// ─── Persistence ─────────────────────────────────────────────────────────

/// Save a manifest as JSON to disk atomically (write to temp + rename).
///
/// The temp filename includes pid + nanosecond suffix so concurrent saves
/// from multiple processes/threads cannot collide on a single shared
/// temp path. A crash mid-write leaves the temp file behind but never
/// produces a half-written `manifest.json` — readers always observe
/// either the previous good manifest or the new one.
pub fn save_manifest(manifest: &FileManifest, path: &Path) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(manifest)
        .map_err(|e| io::Error::other(format!("JSON serialize error: {e}")))?;

    let pid = std::process::id();
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let tmp_path = match path.parent() {
        Some(parent) => {
            let file_name = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("manifest.json");
            parent.join(format!("{file_name}.tmp.{pid}.{nanos}"))
        }
        None => path.with_extension(format!("tmp.{pid}.{nanos}")),
    };

    fs::write(&tmp_path, json)?;
    if let Err(e) = fs::rename(&tmp_path, path) {
        // Best-effort cleanup of the orphaned temp file so we don't leak
        // it if rename fails (e.g., dest path locked on Windows).
        let _ = fs::remove_file(&tmp_path);
        return Err(e);
    }
    Ok(())
}

/// Load a manifest from JSON on disk. Returns `None` if file doesn't exist.
pub fn load_manifest(path: &Path) -> io::Result<Option<FileManifest>> {
    match fs::read_to_string(path) {
        Ok(json) => {
            let manifest: FileManifest = serde_json::from_str(&json)
                .map_err(|e| io::Error::other(format!("JSON deserialize error: {e}")))?;
            Ok(Some(manifest))
        }
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e),
    }
}

/// Get the manifest path for a repository's storage directory.
pub fn manifest_path(storage_path: &Path) -> std::path::PathBuf {
    storage_path.join("manifest.json")
}

// ─── Tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compute_hash_from_content() {
        let hash1 = compute_hash_from_content("hello world");
        let hash2 = compute_hash_from_content("hello world");
        let hash3 = compute_hash_from_content("hello world!");

        assert_eq!(hash1, hash2);
        assert_ne!(hash1, hash3);
        // SHA-256 produces a 64-char hex string
        assert_eq!(hash1.len(), 64);
    }

    #[test]
    fn test_diff_empty_manifests() {
        let old = FileManifest::default();
        let new = FileManifest::default();
        let changes = diff_manifests(&old, &new);
        assert!(changes.is_empty());
    }

    #[test]
    fn test_diff_added_files() {
        let old = FileManifest::default();
        let mut new = FileManifest::default();
        new.files.insert(
            "src/main.ts".into(),
            FileDigest {
                hash: "abc123".into(),
                size: 100,
                modified: 1000,
            },
        );

        let changes = diff_manifests(&old, &new);
        assert_eq!(changes, vec![FileChange::Added("src/main.ts".into())]);
    }

    #[test]
    fn test_diff_removed_files() {
        let mut old = FileManifest::default();
        old.files.insert(
            "src/main.ts".into(),
            FileDigest {
                hash: "abc123".into(),
                size: 100,
                modified: 1000,
            },
        );
        let new = FileManifest::default();

        let changes = diff_manifests(&old, &new);
        assert_eq!(changes, vec![FileChange::Removed("src/main.ts".into())]);
    }

    #[test]
    fn test_diff_modified_files() {
        let digest_old = FileDigest {
            hash: "old_hash".into(),
            size: 100,
            modified: 1000,
        };
        let digest_new = FileDigest {
            hash: "new_hash".into(),
            size: 150,
            modified: 2000,
        };

        let mut old = FileManifest::default();
        old.files.insert("src/main.ts".into(), digest_old);

        let mut new = FileManifest::default();
        new.files.insert("src/main.ts".into(), digest_new);

        let changes = diff_manifests(&old, &new);
        assert_eq!(changes, vec![FileChange::Modified("src/main.ts".into())]);
    }

    #[test]
    fn test_diff_unchanged_files() {
        let digest = FileDigest {
            hash: "same_hash".into(),
            size: 100,
            modified: 1000,
        };

        let mut old = FileManifest::default();
        old.files.insert("src/main.ts".into(), digest.clone());

        let mut new = FileManifest::default();
        // Same hash/size, different modified time -> no change (we check hash+size, not mtime)
        new.files.insert(
            "src/main.ts".into(),
            FileDigest {
                modified: 2000,
                ..digest
            },
        );

        let changes = diff_manifests(&old, &new);
        assert!(changes.is_empty());
    }

    #[test]
    fn test_diff_mixed_changes() {
        let mut old = FileManifest::default();
        old.files.insert(
            "a.ts".into(),
            FileDigest {
                hash: "hash_a".into(),
                size: 50,
                modified: 1000,
            },
        );
        old.files.insert(
            "b.ts".into(),
            FileDigest {
                hash: "hash_b_old".into(),
                size: 60,
                modified: 1000,
            },
        );
        old.files.insert(
            "c.ts".into(),
            FileDigest {
                hash: "hash_c".into(),
                size: 70,
                modified: 1000,
            },
        );

        let mut new = FileManifest::default();
        // a.ts unchanged
        new.files.insert(
            "a.ts".into(),
            FileDigest {
                hash: "hash_a".into(),
                size: 50,
                modified: 1000,
            },
        );
        // b.ts modified
        new.files.insert(
            "b.ts".into(),
            FileDigest {
                hash: "hash_b_new".into(),
                size: 65,
                modified: 2000,
            },
        );
        // c.ts removed (not in new)
        // d.ts added
        new.files.insert(
            "d.ts".into(),
            FileDigest {
                hash: "hash_d".into(),
                size: 80,
                modified: 2000,
            },
        );

        let changes = diff_manifests(&old, &new);
        assert_eq!(changes.len(), 3);
        assert!(changes.contains(&FileChange::Modified("b.ts".into())));
        assert!(changes.contains(&FileChange::Removed("c.ts".into())));
        assert!(changes.contains(&FileChange::Added("d.ts".into())));
    }

    #[test]
    fn test_manifest_roundtrip() {
        let mut manifest = FileManifest::default();
        manifest.files.insert(
            "src/main.ts".into(),
            FileDigest {
                hash: "abc123".into(),
                size: 100,
                modified: 1000,
            },
        );

        let dir = std::env::temp_dir().join("code_explorer_manifest_test");
        let path = dir.join("manifest.json");

        save_manifest(&manifest, &path).unwrap();

        let loaded = load_manifest(&path).unwrap().unwrap();
        assert_eq!(loaded.files.len(), 1);
        assert_eq!(loaded.files["src/main.ts"].hash, "abc123");

        // Cleanup
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_load_manifest_missing_file() {
        let path = Path::new("/tmp/does_not_exist_code_explorer_manifest_xyz.json");
        let result = load_manifest(path).unwrap();
        assert!(result.is_none());
    }
}

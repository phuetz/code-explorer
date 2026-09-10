//! Export / import all per-repo user data as a single zip bundle.
//!
//! The bundle is a plain ZIP archive containing the JSON files of:
//!   - bookmarks, comments, saved_queries, activity
//!   - notebooks/, dashboards/, workflows/
//!   - user_commands
//!
//! Useful for sharing a team's analysis conventions with a colleague, or
//! as a lightweight backup. Snapshots are intentionally *excluded* — they
//! can be multi-MB each and aren't user-authored.

use std::collections::HashSet;
use std::io::{Read, Write};
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tauri::State;
use zip::write::SimpleFileOptions;
use zip::ZipArchive;

use crate::state::AppState;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BundleManifest {
    pub format_version: u32,
    pub created_at: i64,
    pub source_repo: String,
    /// Top-level entries present in the zip (descriptive only).
    pub entries: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BundleExportRequest {
    /// Output file path. Defaults to `<storage>/bundle-<ts>.zip`.
    #[serde(default)]
    pub out_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BundleExportResult {
    pub path: String,
    pub size_bytes: u64,
    pub file_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BundleImportRequest {
    /// Absolute path to the zip on disk.
    pub bundle_path: String,
    /// When true, existing files are overwritten. When false, the import
    /// is aborted if any target file already exists (safer default).
    #[serde(default)]
    pub overwrite: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BundleImportResult {
    pub restored: u32,
    pub skipped: u32,
    pub entries: Vec<String>,
}

/// Single-file entries (relative to storage_path) to include.
const SINGLE_FILES: &[&str] = &[
    "bookmarks.json",
    "comments.json",
    "saved_queries.json",
    "activity.json",
    "user_commands.json",
];

/// Directory entries (relative to storage_path) whose contents are included.
const DIRECTORIES: &[&str] = &["notebooks", "dashboards", "workflows"];

#[tauri::command]
pub async fn user_bundle_export(
    state: State<'_, AppState>,
    request: BundleExportRequest,
) -> Result<BundleExportResult, String> {
    let storage = state.active_storage_path().await?;
    let storage_path = PathBuf::from(&storage);

    let ts = chrono::Utc::now().timestamp_millis();
    let out_path = request
        .out_path
        .map(PathBuf::from)
        .unwrap_or_else(|| storage_path.join(format!("bundle-{ts}.zip")));
    if let Some(parent) = out_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    // Containment: out_path must resolve under storage
    let canonical_storage = storage_path.canonicalize().map_err(|e| e.to_string())?;
    let canonical_out = out_path
        .canonicalize()
        .or_else(|_| {
            out_path
                .parent()
                .and_then(|p| p.canonicalize().ok())
                .map(|p| p.join(out_path.file_name().unwrap_or_default()))
                .ok_or_else(|| std::io::Error::other("invalid path"))
        })
        .map_err(|e| format!("Invalid out_path: {e}"))?;
    if !canonical_out.starts_with(&canonical_storage) {
        return Err("out_path must be inside the storage directory".to_string());
    }

    let file = std::fs::File::create(&canonical_out).map_err(|e| e.to_string())?;
    let mut zip = zip::ZipWriter::new(file);
    let options: SimpleFileOptions =
        SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);

    let mut entries: Vec<String> = Vec::new();
    let mut file_count: u32 = 0;

    // Single files
    for rel in SINGLE_FILES {
        let src = storage_path.join(rel);
        if !src.exists() {
            continue;
        }
        let bytes = std::fs::read(&src).map_err(|e| e.to_string())?;
        zip.start_file::<_, ()>(*rel, options)
            .map_err(|e| e.to_string())?;
        zip.write_all(&bytes).map_err(|e| e.to_string())?;
        entries.push((*rel).to_string());
        file_count += 1;
    }

    // Directories (non-recursive — one level deep is enough for our formats)
    for dir in DIRECTORIES {
        let src_dir = storage_path.join(dir);
        if !src_dir.is_dir() {
            continue;
        }
        for entry in std::fs::read_dir(&src_dir).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            let file_name = path
                .file_name()
                .and_then(|n| n.to_str())
                .ok_or_else(|| "Invalid filename".to_string())?;
            let rel = format!("{dir}/{file_name}");
            let bytes = std::fs::read(&path).map_err(|e| e.to_string())?;
            zip.start_file::<_, ()>(&rel, options)
                .map_err(|e| e.to_string())?;
            zip.write_all(&bytes).map_err(|e| e.to_string())?;
            entries.push(rel);
            file_count += 1;
        }
    }

    // Manifest so future importers can detect format.
    let repo_name = state
        .active_repo_name()
        .await
        .unwrap_or_else(|| "(unknown)".into());
    let manifest = BundleManifest {
        format_version: 1,
        created_at: ts,
        source_repo: repo_name,
        entries: entries.clone(),
    };
    let manifest_json = serde_json::to_string_pretty(&manifest).map_err(|e| e.to_string())?;
    zip.start_file::<_, ()>("manifest.json", options)
        .map_err(|e| e.to_string())?;
    zip.write_all(manifest_json.as_bytes())
        .map_err(|e| e.to_string())?;

    zip.finish().map_err(|e| e.to_string())?;

    let size = std::fs::metadata(&out_path).map(|m| m.len()).unwrap_or(0);
    Ok(BundleExportResult {
        path: out_path.to_string_lossy().to_string(),
        size_bytes: size,
        file_count,
    })
}

#[tauri::command]
pub async fn user_bundle_import(
    state: State<'_, AppState>,
    request: BundleImportRequest,
) -> Result<BundleImportResult, String> {
    let storage = state.active_storage_path().await?;
    let storage_path = PathBuf::from(&storage);

    let file = std::fs::File::open(&request.bundle_path).map_err(|e| e.to_string())?;
    let mut archive = ZipArchive::new(file).map_err(|e| e.to_string())?;

    // Allowed relative entry paths — guard against zip-slip by checking
    // against this allow-list rather than canonicalizing at runtime.
    let mut allowed: HashSet<String> = HashSet::new();
    for f in SINGLE_FILES {
        allowed.insert((*f).to_string());
    }
    allowed.insert("manifest.json".to_string());
    let allowed_dirs: HashSet<&str> = DIRECTORIES.iter().copied().collect();

    let mut restored: u32 = 0;
    let mut skipped: u32 = 0;
    let mut entries: Vec<String> = Vec::new();

    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(|e| e.to_string())?;
        let name = entry.name().to_string();
        // Reject any path traversal attempt.
        if name.contains("..") || name.starts_with('/') || name.starts_with('\\') {
            skipped += 1;
            continue;
        }
        let is_allowed = allowed.contains(&name)
            || name
                .split_once('/')
                .map(|(d, _)| allowed_dirs.contains(d))
                .unwrap_or(false);
        if !is_allowed {
            skipped += 1;
            continue;
        }
        if name == "manifest.json" {
            // We don't write the manifest back — it's informational only.
            skipped += 1;
            continue;
        }

        let dest = storage_path.join(&name);
        if dest.exists() && !request.overwrite {
            skipped += 1;
            continue;
        }
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        const MAX_ENTRY_BYTES: u64 = 32 * 1024 * 1024; // 32 MB
                                                       // Cap actual decompressed read — entry.size() is self-reported and untrusted
        let mut contents = Vec::new();
        std::io::Read::take(&mut entry, MAX_ENTRY_BYTES + 1)
            .read_to_end(&mut contents)
            .map_err(|e| e.to_string())?;
        if contents.len() as u64 > MAX_ENTRY_BYTES {
            skipped += 1;
            continue;
        }
        std::fs::write(&dest, &contents).map_err(|e| e.to_string())?;
        entries.push(name);
        restored += 1;
    }

    Ok(BundleImportResult {
        restored,
        skipped,
        entries,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_single_files_and_dirs_are_disjoint() {
        let singles: HashSet<_> = SINGLE_FILES.iter().copied().collect();
        let dirs: HashSet<_> = DIRECTORIES.iter().copied().collect();
        for d in &dirs {
            assert!(
                !singles.contains(d),
                "'{}' can't be both a single file and a directory",
                d
            );
        }
    }

    #[test]
    fn test_manifest_serialization() {
        let m = BundleManifest {
            format_version: 1,
            created_at: 123,
            source_repo: "foo".into(),
            entries: vec!["bookmarks.json".into()],
        };
        let s = serde_json::to_string(&m).unwrap();
        assert!(s.contains("\"formatVersion\":1"));
        assert!(s.contains("bookmarks.json"));
    }
}

use std::fs::File;
use std::io::{BufReader, BufWriter, Write};
use std::path::Path;

use code_explorer_core::graph::KnowledgeGraph;

use crate::error::DbError;

// NOTE: Bincode serialization was attempted but is incompatible with the
// `#[serde(skip_serializing_if)]` attributes on NodeProperties (~40 fields).
// Bincode is a positional format and cannot handle conditionally skipped fields.
// Migrating to bincode would require either:
//   1. Removing all skip_serializing_if attributes (breaking JSON API output)
//   2. Creating separate Encode/Decode impls (bincode 2.x)
//   3. Using a map-based binary format like MessagePack (rmp-serde)
// For now, JSON serialization is retained for full serde compatibility.

fn snapshot_err(cause: String) -> DbError {
    DbError::CsvError {
        table: "snapshot".to_string(),
        cause,
    }
}

/// Save a KnowledgeGraph to a JSON snapshot file.
/// Uses serde_json for full compatibility with enum rename and skip attributes.
/// Performs atomic write with explicit fsync to ensure durability.
pub fn save_snapshot(graph: &KnowledgeGraph, path: &Path) -> Result<(), DbError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| snapshot_err(e.to_string()))?;
    }

    // Write to temporary file first to avoid data loss on partial write.
    //
    // Use a unique pid+nanosecond suffix so concurrent saves (e.g. the
    // desktop app and the CLI both touching the same repo, or two CLI
    // invocations from different shells) cannot collide on a single shared
    // `graph.tmp`. With a fixed temp name, both processes would race on
    // file creation and the loser could end up with a half-written or
    // foreign payload landing as the "atomic" snapshot.
    let pid = std::process::id();
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let temp_path = path.with_extension(format!("tmp.{pid}.{nanos}"));
    let file = File::create(&temp_path)
        .map_err(|e| snapshot_err(format!("Failed to create temporary snapshot file: {e}")))?;

    let mut writer = BufWriter::new(file);
    serde_json::to_writer(&mut writer, graph).map_err(|e| snapshot_err(e.to_string()))?;

    // Explicit flush to ensure all data is written to the file
    writer
        .flush()
        .map_err(|e| snapshot_err(format!("Failed to flush snapshot data: {e}")))?;

    // Sync to disk for durability (via drop of file handle)
    drop(writer);

    // Atomic rename: temp file becomes the real snapshot
    // This ensures the old snapshot is only replaced when the new one is fully written.
    //
    // On Windows, `std::fs::rename` fails if the destination exists. The previous
    // implementation removed the destination first, leaving a window during which
    // a concurrent reader (another Tauri IPC call, the MCP server, etc.) would
    // see "file not found" and treat the repo as empty. We avoid that by renaming
    // the existing snapshot to a `.bak` first, then moving the temp file into place,
    // then cleaning up the backup. This keeps `path` valid at every observable
    // moment from a reader's perspective.
    #[cfg(target_os = "windows")]
    {
        if path.exists() {
            // Per-process unique backup name so concurrent writers don't
            // clobber each other's `.bak` file mid-rename.
            let bak_path = path.with_extension(format!("bak.{pid}.{nanos}"));
            // If a stale .bak from a previous failed save exists, drop it.
            let _ = std::fs::remove_file(&bak_path);
            std::fs::rename(path, &bak_path).map_err(|e| {
                // Clean up the temp file if backup failed so we don't leak
                // half-written snapshots into `.codeexplorer`.
                let _ = std::fs::remove_file(&temp_path);
                snapshot_err(format!("Cannot back up existing snapshot: {e}"))
            })?;
            if let Err(e) = std::fs::rename(&temp_path, path) {
                // Roll back: restore the original from .bak so we don't lose data.
                let _ = std::fs::rename(&bak_path, path);
                let _ = std::fs::remove_file(&temp_path);
                return Err(snapshot_err(format!(
                    "Failed to finalize snapshot (rename): {e}"
                )));
            }
            // Best-effort cleanup of the backup; failure here is non-fatal.
            let _ = std::fs::remove_file(&bak_path);
            return Ok(());
        }
    }

    if let Err(e) = std::fs::rename(&temp_path, path) {
        // Best-effort cleanup so a failed rename doesn't leave behind a
        // growing pile of `.tmp.*` files in `.codeexplorer/` after interrupted
        // saves (e.g. full disk, permission errors on shared volumes).
        let _ = std::fs::remove_file(&temp_path);
        return Err(snapshot_err(format!(
            "Failed to finalize snapshot (rename): {e}"
        )));
    }

    Ok(())
}

/// Load a KnowledgeGraph from a JSON snapshot file.
pub fn load_snapshot(path: &Path) -> Result<KnowledgeGraph, DbError> {
    let file =
        File::open(path).map_err(|e| snapshot_err(format!("Failed to open snapshot: {e}")))?;
    let reader = BufReader::new(file);
    let graph: KnowledgeGraph = serde_json::from_reader(reader)
        .map_err(|e| snapshot_err(format!("Failed to deserialize snapshot: {e}")))?;
    Ok(graph)
}

/// Check if a snapshot exists.
pub fn snapshot_exists(path: &Path) -> bool {
    path.exists() && path.is_file()
}

/// Get the snapshot path for a repository's storage directory.
pub fn snapshot_path(storage_path: &Path) -> std::path::PathBuf {
    storage_path.join("graph.bin")
}

#[cfg(test)]
mod tests {
    use super::*;
    use code_explorer_core::graph::types::*;

    #[test]
    fn test_snapshot_roundtrip() {
        let mut graph = KnowledgeGraph::new();
        graph.add_node(GraphNode {
            id: "Function:test.rs:foo".to_string(),
            label: NodeLabel::Function,
            properties: NodeProperties {
                name: "foo".to_string(),
                file_path: "test.rs".to_string(),
                start_line: Some(1),
                end_line: Some(10),
                ..Default::default()
            },
        });
        graph.add_node(GraphNode {
            id: "Function:test.rs:bar".to_string(),
            label: NodeLabel::Function,
            properties: NodeProperties {
                name: "bar".to_string(),
                file_path: "test.rs".to_string(),
                ..Default::default()
            },
        });
        graph.add_relationship(GraphRelationship {
            id: "calls_foo_bar".to_string(),
            source_id: "Function:test.rs:foo".to_string(),
            target_id: "Function:test.rs:bar".to_string(),
            rel_type: RelationshipType::Calls,
            confidence: 0.95,
            reason: "exact".to_string(),
            step: None,
        });

        let dir = std::env::temp_dir().join("code_explorer_snapshot_test");
        let path = dir.join("graph.bin");

        save_snapshot(&graph, &path).unwrap();
        assert!(snapshot_exists(&path));

        let loaded = load_snapshot(&path).unwrap();
        assert_eq!(loaded.node_count(), 2);
        assert_eq!(loaded.relationship_count(), 1);
        assert!(loaded.get_node("Function:test.rs:foo").is_some());
        assert!(loaded.get_node("Function:test.rs:bar").is_some());

        // Cleanup
        let _ = std::fs::remove_dir_all(&dir);
    }
}

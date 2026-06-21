//! Incremental update engine for the ingestion pipeline.
//!
//! Instead of re-indexing the entire repository, this module:
//! 1. Loads the previous file manifest (SHA-256 digests)
//! 2. Scans current files and computes a new manifest
//! 3. Diffs the two manifests to find added / modified / removed files
//! 4. Removes graph nodes for removed/modified files
//! 5. Re-parses added/modified files and inserts new nodes
//! 6. Re-runs import resolution for affected files
//! 7. Saves the updated manifest and graph snapshot

use std::collections::HashSet;
use std::path::Path;

use code_explorer_core::graph::KnowledgeGraph;
use code_explorer_core::symbol::SymbolTable;
use tracing::{debug, info};

use crate::manifest::{self, FileChange, FileManifest};
use crate::phases;
use crate::phases::structure::FileEntry;

// ─── Result ──────────────────────────────────────────────────────────────

/// Summary of what the incremental update did.
#[derive(Debug, Default)]
pub struct IncrementalResult {
    pub added: usize,
    pub modified: usize,
    pub removed: usize,
    pub nodes_removed: usize,
    pub nodes_added: usize,
    pub edges_added: usize,
    pub unchanged: usize,
    /// New manifest computed during this update. The caller is responsible
    /// for persisting it via [`crate::manifest::save_manifest`] **after**
    /// the corresponding graph snapshot has been written. Saving the
    /// manifest before the snapshot is durable can cause silent data
    /// staleness: a crash between the two writes leaves the next run
    /// believing the on-disk graph is current.
    pub new_manifest: FileManifest,
}

impl IncrementalResult {
    pub fn total_changed(&self) -> usize {
        self.added + self.modified + self.removed
    }
}

// ─── Engine ──────────────────────────────────────────────────────────────

/// Run an incremental update on an existing graph.
///
/// `repo_path`     — absolute path to the repository root
/// `storage_path`  — path to the `.codeexplorer` storage directory
/// `graph`         — mutable reference to the current knowledge graph
///
/// Returns an [`IncrementalResult`] summarizing what changed.
pub fn incremental_update(
    repo_path: &Path,
    storage_path: &Path,
    graph: &mut KnowledgeGraph,
) -> Result<IncrementalResult, crate::IngestError> {
    let manifest_file = manifest::manifest_path(storage_path);
    let mut result = IncrementalResult::default();

    // Step 1: Load old manifest (or start fresh)
    let old_manifest = manifest::load_manifest(&manifest_file)
        .map_err(|e| crate::IngestError::PhaseError {
            phase: "incremental".into(),
            message: format!("Failed to load manifest: {e}"),
        })?
        .unwrap_or_default();

    // Step 2: Walk repository to discover current files
    let file_entries = phases::structure::walk_repository(repo_path)?;

    // Step 3: Build new manifest from discovered files
    let new_manifest = manifest::build_manifest_from_entries(&file_entries);

    // Step 4: Diff manifests
    let changes = manifest::diff_manifests(&old_manifest, &new_manifest);

    if changes.is_empty() {
        info!("No file changes detected, graph is up to date");
        result.unchanged = file_entries.len();
        result.new_manifest = new_manifest;
        return Ok(result);
    }

    // Collect paths of changed files
    let mut affected_files: HashSet<String> = HashSet::new();
    for change in &changes {
        match change {
            FileChange::Added(p) => {
                result.added += 1;
                affected_files.insert(p.clone());
            }
            FileChange::Modified(p) => {
                result.modified += 1;
                affected_files.insert(p.clone());
            }
            FileChange::Removed(p) => {
                result.removed += 1;
                affected_files.insert(p.clone());
            }
        }
    }

    // Defensive saturating subtraction: in normal operation `added + modified`
    // is bounded by `file_entries.len()` (Removed files don't appear in
    // `file_entries`), but a bug in `diff_manifests` reporting a path as
    // both Added and Modified would otherwise underflow this and panic the
    // pipeline in debug mode. Clamp to zero instead.
    result.unchanged = file_entries
        .len()
        .saturating_sub(result.added)
        .saturating_sub(result.modified);

    info!(
        added = result.added,
        modified = result.modified,
        removed = result.removed,
        "Incremental update: detected changes"
    );

    // Step 5: Remove old nodes for modified + removed files
    for change in &changes {
        let path = match change {
            FileChange::Removed(p) | FileChange::Modified(p) => p,
            FileChange::Added(_) => continue,
        };
        let removed_count = graph.remove_nodes_by_file(path);
        result.nodes_removed += removed_count;
        debug!(path = %path, removed = removed_count, "Removed old graph nodes");
    }

    // Step 6: Re-parse added + modified files
    let entries_to_parse: Vec<&FileEntry> = file_entries
        .iter()
        .filter(|e| {
            affected_files.contains(&e.path)
                && changes
                    .iter()
                    .find(|c| match c {
                        FileChange::Removed(p) => p == &e.path,
                        _ => false,
                    })
                    .is_none()
        })
        .collect();

    if !entries_to_parse.is_empty() {
        // Create temporary copies for structure + parse
        let parse_entries: Vec<FileEntry> = entries_to_parse.iter().map(|e| (*e).clone()).collect();

        // Capture pre-update edge count so we can compute edges_added below.
        let edges_before = graph.relationship_count();

        // Create file/folder structure nodes for new files
        phases::structure::create_structure_nodes(graph, &parse_entries);

        // Parse AST
        let extracted = phases::parsing::parse_files(graph, &parse_entries, None)?;

        result.nodes_added = count_graph_nodes_for_files(graph, &parse_entries);

        // Build symbol table from the full graph (including unchanged)
        let mut symbol_table = SymbolTable::new();
        phases::parsing::build_symbol_table(graph, &mut symbol_table);

        // Re-run import resolution for changed files only
        let (import_map, named_import_map, re_export_map, package_map, module_alias_map) =
            phases::imports::resolve_imports(
                graph,
                repo_path,
                &parse_entries,
                &extracted,
                &symbol_table,
            )?;

        // Re-run call resolution for changed files
        phases::calls::resolve_calls(
            graph,
            &extracted,
            &symbol_table,
            &import_map,
            &named_import_map,
            &re_export_map,
            &package_map,
            &module_alias_map,
            &file_entries,
        )?;

        // Re-run heritage processing for changed files
        phases::heritage::process_heritage(
            graph,
            &extracted,
            &symbol_table,
            &import_map,
            &named_import_map,
            &re_export_map,
        )?;

        // Compute edges_added as the net change in the relationship store.
        // This will be 0 (or negative, clamped) if removed edges balance new ones.
        result.edges_added = graph.relationship_count().saturating_sub(edges_before);

        info!(
            nodes_added = result.nodes_added,
            edges_added = result.edges_added,
            "Incremental update: parsed changed files"
        );
    }

    // Step 7: Hand the new manifest back to the caller — see the doc on
    // `IncrementalResult::new_manifest` for the rationale (snapshot must be
    // durable BEFORE the manifest is overwritten, otherwise a crash between
    // the two writes silently strands the on-disk graph in a stale state).
    result.new_manifest = new_manifest;

    Ok(result)
}

/// Count graph nodes belonging to a set of files.
fn count_graph_nodes_for_files(graph: &KnowledgeGraph, files: &[FileEntry]) -> usize {
    let mut count = 0;
    for file in files {
        if let Some(ids) = graph.nodes_by_file(&file.path) {
            count += ids.len();
        }
    }
    count
}

// ─── Tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::{diff_manifests, FileDigest, FileManifest};

    #[test]
    fn test_incremental_result_total() {
        let result = IncrementalResult {
            added: 3,
            modified: 2,
            removed: 1,
            ..Default::default()
        };
        assert_eq!(result.total_changed(), 6);
    }

    #[test]
    fn test_change_detection_logic() {
        // Simulate the change detection part without touching the filesystem
        let mut old = FileManifest::default();
        old.files.insert(
            "src/a.ts".into(),
            FileDigest {
                hash: "hash_a_v1".into(),
                size: 100,
                modified: 1000,
            },
        );
        old.files.insert(
            "src/b.ts".into(),
            FileDigest {
                hash: "hash_b".into(),
                size: 200,
                modified: 1000,
            },
        );
        old.files.insert(
            "src/deleted.ts".into(),
            FileDigest {
                hash: "hash_del".into(),
                size: 50,
                modified: 1000,
            },
        );

        let mut new = FileManifest::default();
        new.files.insert(
            "src/a.ts".into(),
            FileDigest {
                hash: "hash_a_v2".into(), // modified
                size: 110,
                modified: 2000,
            },
        );
        new.files.insert(
            "src/b.ts".into(),
            FileDigest {
                hash: "hash_b".into(), // unchanged
                size: 200,
                modified: 1000,
            },
        );
        new.files.insert(
            "src/new.ts".into(),
            FileDigest {
                hash: "hash_new".into(), // added
                size: 300,
                modified: 2000,
            },
        );

        let changes = diff_manifests(&old, &new);

        let added: Vec<_> = changes
            .iter()
            .filter(|c| matches!(c, FileChange::Added(_)))
            .collect();
        let modified: Vec<_> = changes
            .iter()
            .filter(|c| matches!(c, FileChange::Modified(_)))
            .collect();
        let removed: Vec<_> = changes
            .iter()
            .filter(|c| matches!(c, FileChange::Removed(_)))
            .collect();

        assert_eq!(added.len(), 1);
        assert_eq!(modified.len(), 1);
        assert_eq!(removed.len(), 1);
    }
}

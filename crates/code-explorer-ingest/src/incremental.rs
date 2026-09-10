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

use code_explorer_core::graph::types::NodeLabel;

use crate::manifest::{self, FileChange, FileManifest};
use crate::phases;
use crate::phases::docs::{self as docs_phase, DocEntry};
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
    /// Prose documents re-indexed in this run.
    pub documents_updated: usize,
    /// Empty `Folder` nodes pruned after removals and renames.
    pub folders_pruned: usize,
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

    // Prose participates only if the index already holds documents: a code
    // repository must not start paying for a documentation pass because it
    // was watched. Without this, a deleted or renamed Markdown file kept its
    // File and Section nodes forever — the manifest never mentioned it.
    let track_documents = graph_has_documents(graph);
    let doc_entries: Vec<DocEntry> = if track_documents {
        docs_phase::walk_documents(repo_path)?
    } else {
        Vec::new()
    };

    // Step 3: Build new manifest from discovered files (code + prose)
    let mut new_manifest = manifest::build_manifest_from_entries(&file_entries);
    if !doc_entries.is_empty() {
        let doc_paths: Vec<(String, std::path::PathBuf)> = doc_entries
            .iter()
            .map(|d| (d.path.clone(), repo_path.join(&d.path)))
            .collect();
        let borrowed: Vec<(&str, &Path)> = doc_paths
            .iter()
            .map(|(rel, abs)| (rel.as_str(), abs.as_path()))
            .collect();
        let doc_manifest = manifest::build_manifest(&borrowed);
        new_manifest.files.extend(doc_manifest.files);
    }

    // Step 4: Diff manifests
    let changes = manifest::diff_manifests(&old_manifest, &new_manifest);

    if changes.is_empty() {
        info!("No file changes detected, graph is up to date");
        result.unchanged = file_entries.len() + doc_entries.len();
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

    // Step 6b: Re-index prose documents that were added or modified. Their
    // old nodes were already removed in step 5 (a document's File and Section
    // nodes share its `file_path`).
    if track_documents {
        let changed_docs: Vec<DocEntry> = doc_entries
            .iter()
            .filter(|d| affected_files.contains(&d.path))
            .cloned()
            .collect();
        if !changed_docs.is_empty() {
            let mut to_index = changed_docs;
            docs_phase::load_document_contents(repo_path, &mut to_index);
            let known_files: HashSet<String> = file_entries
                .iter()
                .map(|e| e.path.clone())
                .chain(doc_entries.iter().map(|d| d.path.clone()))
                .collect();
            let stats = docs_phase::create_document_nodes(graph, &to_index, &known_files);
            result.documents_updated = stats.documents;
            debug!(
                documents = stats.documents,
                headings = stats.headings,
                "Incremental update: re-indexed documents"
            );
        }
    }

    // Step 6c: A deletion or a rename empties folders. Their `Folder` nodes
    // survive `remove_nodes_by_file` (a folder is not a file) and keep showing
    // directories that no longer exist, so prune them.
    result.folders_pruned = graph.remove_empty_folders();
    if result.folders_pruned > 0 {
        debug!(
            folders = result.folders_pruned,
            "Incremental update: pruned empty folders"
        );
    }

    // Step 7: Hand the new manifest back to the caller — see the doc on
    // `IncrementalResult::new_manifest` for the rationale (snapshot must be
    // durable BEFORE the manifest is overwritten, otherwise a crash between
    // the two writes silently strands the on-disk graph in a stale state).
    result.new_manifest = new_manifest;

    Ok(result)
}

/// Whether the graph already holds prose documents (a `Section` node, or a
/// `File` node tagged with a document kind).
fn graph_has_documents(graph: &KnowledgeGraph) -> bool {
    graph.iter_nodes().any(|n| {
        n.label == NodeLabel::Section
            || (n.label == NodeLabel::File
                && n.properties
                    .heuristic_label
                    .as_deref()
                    .is_some_and(|l| matches!(l, "markdown" | "text" | "rst")))
    })
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

#[cfg(test)]
mod watch_engine_tests {
    use super::*;
    use code_explorer_core::graph::types::NodeLabel;

    struct Sandbox {
        root: std::path::PathBuf,
    }

    impl Sandbox {
        fn new(tag: &str) -> Self {
            let root = std::env::temp_dir().join(format!(
                "code-explorer-incr-{tag}-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            std::fs::create_dir_all(root.join(".codeexplorer")).unwrap();
            Self { root }
        }

        fn storage(&self) -> std::path::PathBuf {
            self.root.join(".codeexplorer")
        }

        fn write(&self, rel: &str, content: &str) {
            let path = self.root.join(rel);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, content).unwrap();
        }

        fn remove(&self, rel: &str) {
            std::fs::remove_file(self.root.join(rel)).unwrap();
        }
    }

    impl Drop for Sandbox {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }

    /// Bring the graph and the manifest to a consistent starting state, the
    /// way `watch` does after its first full index.
    fn seed(sandbox: &Sandbox, graph: &mut KnowledgeGraph) {
        let files = phases::structure::walk_repository(&sandbox.root).unwrap();
        phases::structure::create_structure_nodes(graph, &files);
        let mut manifest = manifest::build_manifest_from_entries(&files);

        let mut docs = docs_phase::walk_documents(&sandbox.root).unwrap();
        if !docs.is_empty() {
            docs_phase::load_document_contents(&sandbox.root, &mut docs);
            let known: HashSet<String> = files.iter().map(|f| f.path.clone()).collect();
            docs_phase::create_document_nodes(graph, &docs, &known);
            let owned: Vec<(String, std::path::PathBuf)> = docs
                .iter()
                .map(|d| (d.path.clone(), sandbox.root.join(&d.path)))
                .collect();
            let borrowed: Vec<(&str, &Path)> = owned
                .iter()
                .map(|(rel, abs)| (rel.as_str(), abs.as_path()))
                .collect();
            manifest.files.extend(manifest::build_manifest(&borrowed).files);
        }
        manifest::save_manifest(&manifest, &manifest::manifest_path(&sandbox.storage())).unwrap();
    }

    fn paths_in_graph(graph: &KnowledgeGraph) -> HashSet<String> {
        graph
            .iter_nodes()
            .map(|n| n.properties.file_path.clone())
            .collect()
    }

    #[test]
    fn a_deleted_source_file_leaves_no_node_and_no_empty_folder() {
        let sandbox = Sandbox::new("delete");
        sandbox.write("src/keep.rs", "pub fn keep() {}\n");
        sandbox.write("legacy/old.rs", "pub fn old() {}\n");
        let mut graph = KnowledgeGraph::new();
        seed(&sandbox, &mut graph);
        assert!(paths_in_graph(&graph).contains("legacy/old.rs"));
        assert!(paths_in_graph(&graph).contains("legacy"));

        sandbox.remove("legacy/old.rs");
        let result = incremental_update(&sandbox.root, &sandbox.storage(), &mut graph).unwrap();

        assert_eq!(result.removed, 1);
        let paths = paths_in_graph(&graph);
        assert!(
            !paths.iter().any(|p| p.starts_with("legacy")),
            "a deleted file must leave neither its nodes nor its folder: {paths:?}"
        );
        assert!(paths.contains("src/keep.rs"));
        assert_eq!(result.folders_pruned, 1);
    }

    #[test]
    fn a_renamed_source_file_moves_instead_of_duplicating() {
        let sandbox = Sandbox::new("rename");
        sandbox.write("src/before.rs", "pub fn subject() {}\n");
        let mut graph = KnowledgeGraph::new();
        seed(&sandbox, &mut graph);

        std::fs::rename(
            sandbox.root.join("src/before.rs"),
            sandbox.root.join("src/after.rs"),
        )
        .unwrap();
        let result = incremental_update(&sandbox.root, &sandbox.storage(), &mut graph).unwrap();

        assert_eq!(result.added, 1);
        assert_eq!(result.removed, 1);
        let paths = paths_in_graph(&graph);
        assert!(!paths.contains("src/before.rs"), "{paths:?}");
        assert!(paths.contains("src/after.rs"), "{paths:?}");
        // No relationship may still point at a node that is gone.
        let ids: HashSet<&str> = graph.iter_nodes().map(|n| n.id.as_str()).collect();
        for rel in graph.iter_relationships() {
            assert!(ids.contains(rel.source_id.as_str()), "dangling source {rel:?}");
            assert!(ids.contains(rel.target_id.as_str()), "dangling target {rel:?}");
        }
    }

    #[test]
    fn a_deleted_document_leaves_no_section_node() {
        let sandbox = Sandbox::new("doc-delete");
        sandbox.write("chapters/one.md", "# Chapter one\n\nbody\n");
        sandbox.write("chapters/two.md", "# Chapter two\n\nbody\n");
        let mut graph = KnowledgeGraph::new();
        seed(&sandbox, &mut graph);
        let sections = |g: &KnowledgeGraph| -> Vec<String> {
            g.iter_nodes()
                .filter(|n| n.label == NodeLabel::Section)
                .map(|n| n.properties.name.clone())
                .collect()
        };
        assert_eq!(sections(&graph).len(), 2);

        sandbox.remove("chapters/one.md");
        let result = incremental_update(&sandbox.root, &sandbox.storage(), &mut graph).unwrap();

        assert_eq!(result.removed, 1);
        assert_eq!(sections(&graph), vec!["Chapter two".to_string()]);
        assert!(!paths_in_graph(&graph).contains("chapters/one.md"));
    }

    #[test]
    fn an_edited_document_gets_its_new_headings() {
        let sandbox = Sandbox::new("doc-edit");
        sandbox.write("guide.md", "# Old title\n\nbody\n");
        let mut graph = KnowledgeGraph::new();
        seed(&sandbox, &mut graph);

        sandbox.write("guide.md", "# New title\n\nbody\n\n## Added section\n\nmore\n");
        let result = incremental_update(&sandbox.root, &sandbox.storage(), &mut graph).unwrap();

        assert_eq!(result.modified, 1);
        assert_eq!(result.documents_updated, 1);
        let names: HashSet<String> = graph
            .iter_nodes()
            .filter(|n| n.label == NodeLabel::Section)
            .map(|n| n.properties.name.clone())
            .collect();
        assert!(names.contains("New title"), "{names:?}");
        assert!(names.contains("Added section"), "{names:?}");
        assert!(!names.contains("Old title"), "{names:?}");
    }

    #[test]
    fn a_code_repository_is_not_turned_into_a_prose_repository_by_watching_it() {
        let sandbox = Sandbox::new("no-doc-drift");
        sandbox.write("src/a.rs", "pub fn a() {}\n");
        sandbox.write("README.md", "# Readme\n\nbody\n");
        let mut graph = KnowledgeGraph::new();
        // Seed WITHOUT documents, as a code-only index would be.
        let files = phases::structure::walk_repository(&sandbox.root).unwrap();
        phases::structure::create_structure_nodes(&mut graph, &files);
        manifest::save_manifest(
            &manifest::build_manifest_from_entries(&files),
            &manifest::manifest_path(&sandbox.storage()),
        )
        .unwrap();

        sandbox.write("src/a.rs", "pub fn a() { let _ = 1; }\n");
        incremental_update(&sandbox.root, &sandbox.storage(), &mut graph).unwrap();

        assert!(
            !graph.iter_nodes().any(|n| n.label == NodeLabel::Section),
            "an index without documents must stay without documents"
        );
    }
}

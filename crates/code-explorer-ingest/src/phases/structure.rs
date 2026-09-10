use std::path::Path;

use code_explorer_core::config::exclusions::ExclusionRules;
use code_explorer_core::config::languages::SupportedLanguage;
use code_explorer_core::graph::types::*;
use code_explorer_core::graph::KnowledgeGraph;
use code_explorer_core::id::generate_id;

use super::file_type::detect_language_for_path;

/// A file entry discovered during repository scan.
#[derive(Debug, Clone)]
pub struct FileEntry {
    pub path: String,    // Relative path with forward slashes
    pub content: String, // File content (read lazily or eagerly)
    pub size: usize,
    pub language: Option<SupportedLanguage>,
}

/// Walk repository and discover all source files, using the exclusion rules
/// the repository itself declares (see [`ExclusionRules::for_repo`]).
pub fn walk_repository(repo_path: &Path) -> Result<Vec<FileEntry>, crate::IngestError> {
    walk_repository_with(repo_path, &ExclusionRules::for_repo(repo_path))
}

/// Build the walker every phase shares: `.gitignore` plus `rules`, with
/// excluded directories **pruned** rather than filtered after the fact — a
/// 2.2 GB `node_modules/` that git does not ignore is never descended into.
pub fn build_walker(repo_path: &Path, rules: &ExclusionRules) -> ignore::Walk {
    use ignore::WalkBuilder;

    let root = repo_path.to_path_buf();
    let rules = rules.clone();
    WalkBuilder::new(repo_path)
        .hidden(true) // Respect .gitignore
        .git_ignore(true)
        .git_global(false)
        .git_exclude(true)
        .filter_entry(move |entry| {
            let Ok(rel) = entry.path().strip_prefix(&root) else {
                return true;
            };
            if rel.as_os_str().is_empty() {
                return true; // never prune the root itself
            }
            !rules.is_excluded(&rel.to_string_lossy().replace('\\', "/"))
        })
        .build()
}

/// How many files one directory contributes to the walk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirTally {
    /// Top-level directory, relative to the repository root. Files sitting at
    /// the root are tallied under `.`.
    pub path: String,
    /// Files a language provider could parse.
    pub candidates: usize,
    /// Every file walked, parseable or not.
    pub walked: usize,
    /// Bytes of the walked files.
    pub bytes: u64,
}

impl DirTally {
    /// The directory's weight, in a unit a human reads without counting zeros.
    pub fn human_bytes(&self) -> String {
        const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
        let mut value = self.bytes as f64;
        let mut unit = 0;
        while value >= 1024.0 && unit < UNITS.len() - 1 {
            value /= 1024.0;
            unit += 1;
        }
        if unit == 0 {
            format!("{} B", self.bytes)
        } else {
            format!("{value:.1} {}", UNITS[unit])
        }
    }
}

/// What an indexing run is about to cost, measured before paying for it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CandidateScan {
    /// Files a language provider could parse — the number that drives the
    /// parsing phase, and the one the budget is spent on.
    pub candidates: usize,
    /// Every file the walk kept, parseable or not.
    pub walked: usize,
    /// Per top-level directory, sorted by candidate count, biggest first.
    pub dirs: Vec<DirTally>,
}

impl CandidateScan {
    /// The `n` directories that would cost the most, biggest first.
    pub fn largest(&self, n: usize) -> &[DirTally] {
        &self.dirs[..self.dirs.len().min(n)]
    }
}

/// Count what a walk would yield **without reading a single file**.
///
/// `walk_repository` reads every file it keeps into memory; this does not. It
/// exists so `analyze` can say how big the job is, and refuse it, before the
/// job starts.
pub fn scan_candidates(
    repo_path: &Path,
    rules: &ExclusionRules,
) -> Result<CandidateScan, crate::IngestError> {
    use std::collections::HashMap;

    let mut per_dir: HashMap<String, DirTally> = HashMap::new();
    let mut scan = CandidateScan::default();

    for result in build_walker(repo_path, rules) {
        let entry = result.map_err(|e| crate::IngestError::PhaseError {
            phase: "structure".to_string(),
            message: e.to_string(),
        })?;
        if !entry.file_type().is_some_and(|ft| ft.is_file()) {
            continue;
        }
        let abs_path = entry.path();
        let rel_path = abs_path
            .strip_prefix(repo_path)
            .unwrap_or(abs_path)
            .to_string_lossy()
            .replace('\\', "/");

        let size = entry
            .metadata()
            .map(|m| m.len())
            .unwrap_or_else(|_| std::fs::metadata(abs_path).map(|m| m.len()).unwrap_or(0));
        let is_candidate = size <= 2 * 1024 * 1024
            && detect_language_for_path(abs_path, &rel_path).is_some();

        let top = match rel_path.split_once('/') {
            Some((head, _)) => head.to_string(),
            None => ".".to_string(),
        };
        let tally = per_dir.entry(top.clone()).or_insert(DirTally {
            path: top,
            candidates: 0,
            walked: 0,
            bytes: 0,
        });
        tally.walked += 1;
        tally.bytes += size;
        scan.walked += 1;
        if is_candidate {
            tally.candidates += 1;
            scan.candidates += 1;
        }
    }

    scan.dirs = per_dir.into_values().collect();
    scan.dirs.sort_by(|a, b| {
        b.candidates
            .cmp(&a.candidates)
            .then(b.walked.cmp(&a.walked))
            .then(a.path.cmp(&b.path))
    });
    Ok(scan)
}

/// Walk repository and discover all source files under explicit rules.
pub fn walk_repository_with(
    repo_path: &Path,
    rules: &ExclusionRules,
) -> Result<Vec<FileEntry>, crate::IngestError> {
    let mut entries = Vec::new();

    for result in build_walker(repo_path, rules) {
        let entry = result.map_err(|e| crate::IngestError::PhaseError {
            phase: "structure".to_string(),
            message: e.to_string(),
        })?;

        if !entry.file_type().is_some_and(|ft| ft.is_file()) {
            continue;
        }

        let abs_path = entry.path();
        let rel_path = abs_path
            .strip_prefix(repo_path)
            .unwrap_or(abs_path)
            .to_string_lossy()
            .replace('\\', "/");

        // Skip very large files (>2MB likely generated/minified)
        let metadata = std::fs::metadata(abs_path).ok();
        let size = metadata.as_ref().map_or(0, |m| m.len() as usize);
        if size > 2 * 1024 * 1024 {
            tracing::debug!("Skipping large file ({} KB): {}", size / 1024, rel_path);
            continue;
        }

        let language = detect_language_for_path(abs_path, &rel_path);

        // Only include files with supported languages
        if language.is_some() {
            let content = match std::fs::read_to_string(abs_path) {
                Ok(c) => c,
                Err(e) => {
                    tracing::warn!("Cannot read {}: {}", rel_path, e);
                    String::new()
                }
            };
            entries.push(FileEntry {
                path: rel_path,
                content,
                size,
                language,
            });
        }
    }

    entries.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(entries)
}

/// Create File and Folder nodes with CONTAINS edges.
pub fn create_structure_nodes(graph: &mut KnowledgeGraph, files: &[FileEntry]) {
    let mut created_folders: std::collections::HashSet<String> = std::collections::HashSet::new();

    for file in files {
        let parts: Vec<&str> = file.path.split('/').collect();
        let mut current_path = String::new();
        let mut parent_id: Option<String> = None;

        for (i, part) in parts.iter().enumerate() {
            if !current_path.is_empty() {
                current_path.push('/');
            }
            current_path.push_str(part);

            let is_file = i == parts.len() - 1;
            let label = if is_file {
                NodeLabel::File
            } else {
                NodeLabel::Folder
            };
            let node_id = generate_id(label.as_str(), &current_path);

            // Create node if not already created
            if is_file || !created_folders.contains(&current_path) {
                let node = GraphNode {
                    id: node_id.clone(),
                    label,
                    properties: NodeProperties {
                        name: part.to_string(),
                        file_path: current_path.clone(),
                        language: if is_file { file.language } else { None },
                        ..Default::default()
                    },
                };
                graph.add_node(node);

                if !is_file {
                    created_folders.insert(current_path.clone());
                }
            }

            // Create CONTAINS edge from parent
            if let Some(pid) = &parent_id {
                let edge_id = format!("contains_{}_{}", pid, node_id);
                graph.add_relationship(GraphRelationship {
                    id: edge_id,
                    source_id: pid.clone(),
                    target_id: node_id.clone(),
                    rel_type: RelationshipType::Contains,
                    confidence: 1.0,
                    reason: "filesystem".to_string(),
                    step: None,
                });
            }

            parent_id = Some(node_id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_entries(paths: &[&str]) -> Vec<FileEntry> {
        paths
            .iter()
            .map(|p| FileEntry {
                path: p.to_string(),
                content: String::new(),
                size: 0,
                language: SupportedLanguage::from_filename(p),
            })
            .collect()
    }

    #[test]
    fn test_create_structure_single_file() {
        let mut graph = KnowledgeGraph::new();
        let entries = make_entries(&["src/main.ts"]);
        create_structure_nodes(&mut graph, &entries);

        // Should have: Folder:src, File:src/main.ts
        assert!(graph.get_node("Folder:src").is_some());
        assert!(graph.get_node("File:src/main.ts").is_some());

        // Check folder properties
        let folder = graph.get_node("Folder:src").unwrap();
        assert_eq!(folder.label, NodeLabel::Folder);
        assert_eq!(folder.properties.name, "src");

        // Check file properties
        let file = graph.get_node("File:src/main.ts").unwrap();
        assert_eq!(file.label, NodeLabel::File);
        assert_eq!(file.properties.name, "main.ts");
        assert_eq!(
            file.properties.language,
            Some(SupportedLanguage::TypeScript)
        );
    }

    #[test]
    fn test_create_structure_shared_folders() {
        let mut graph = KnowledgeGraph::new();
        let entries = make_entries(&["src/a.ts", "src/b.ts"]);
        create_structure_nodes(&mut graph, &entries);

        // src folder should only be created once
        assert!(graph.get_node("Folder:src").is_some());
        assert!(graph.get_node("File:src/a.ts").is_some());
        assert!(graph.get_node("File:src/b.ts").is_some());

        // Count nodes: 1 folder + 2 files = 3
        assert_eq!(graph.node_count(), 3);
    }

    #[test]
    fn test_create_structure_nested_folders() {
        let mut graph = KnowledgeGraph::new();
        let entries = make_entries(&["src/components/Button.tsx"]);
        create_structure_nodes(&mut graph, &entries);

        assert!(graph.get_node("Folder:src").is_some());
        assert!(graph.get_node("Folder:src/components").is_some());
        assert!(graph.get_node("File:src/components/Button.tsx").is_some());

        // Should have CONTAINS edges: src -> components -> Button.tsx
        let mut contains_count = 0;
        graph.for_each_relationship(|rel| {
            if rel.rel_type == RelationshipType::Contains {
                contains_count += 1;
            }
        });
        assert_eq!(contains_count, 2);
    }

    #[test]
    fn test_create_structure_root_file() {
        let mut graph = KnowledgeGraph::new();
        let entries = make_entries(&["index.ts"]);
        create_structure_nodes(&mut graph, &entries);

        // Root file has no parent folder
        assert!(graph.get_node("File:index.ts").is_some());
        assert_eq!(graph.node_count(), 1);
        assert_eq!(graph.relationship_count(), 0);
    }

    #[test]
    fn test_create_structure_contains_edges() {
        let mut graph = KnowledgeGraph::new();
        let entries = make_entries(&["src/utils/helpers.ts"]);
        create_structure_nodes(&mut graph, &entries);

        // Verify CONTAINS edge from src -> utils
        let edge_id = "contains_Folder:src_Folder:src/utils";
        let rel = graph.get_relationship(edge_id).unwrap();
        assert_eq!(rel.source_id, "Folder:src");
        assert_eq!(rel.target_id, "Folder:src/utils");
        assert_eq!(rel.rel_type, RelationshipType::Contains);

        // Verify CONTAINS edge from utils -> helpers.ts
        let edge_id2 = "contains_Folder:src/utils_File:src/utils/helpers.ts";
        let rel2 = graph.get_relationship(edge_id2).unwrap();
        assert_eq!(rel2.source_id, "Folder:src/utils");
        assert_eq!(rel2.target_id, "File:src/utils/helpers.ts");
    }

    #[test]
    fn test_create_structure_empty_files() {
        let mut graph = KnowledgeGraph::new();
        let entries: Vec<FileEntry> = Vec::new();
        create_structure_nodes(&mut graph, &entries);

        assert_eq!(graph.node_count(), 0);
        assert_eq!(graph.relationship_count(), 0);
    }
}

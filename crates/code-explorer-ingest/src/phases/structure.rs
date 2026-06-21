use std::path::Path;

use code_explorer_core::config::languages::SupportedLanguage;
use code_explorer_core::graph::types::*;
use code_explorer_core::graph::KnowledgeGraph;
use code_explorer_core::id::generate_id;

/// A file entry discovered during repository scan.
#[derive(Debug, Clone)]
pub struct FileEntry {
    pub path: String,    // Relative path with forward slashes
    pub content: String, // File content (read lazily or eagerly)
    pub size: usize,
    pub language: Option<SupportedLanguage>,
}

/// Walk repository and discover all source files.
pub fn walk_repository(repo_path: &Path) -> Result<Vec<FileEntry>, crate::IngestError> {
    use ignore::WalkBuilder;

    let mut entries = Vec::new();
    let walker = WalkBuilder::new(repo_path)
        .hidden(true) // Respect .gitignore
        .git_ignore(true)
        .git_global(false)
        .git_exclude(true)
        .build();

    for result in walker {
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

        // Skip build output and package directories (often not in .gitignore for legacy repos)
        {
            let lower = rel_path.to_lowercase();
            if lower.contains("/obj/")
                || lower.contains("/bin/")
                || lower.starts_with("obj/")
                || lower.starts_with("bin/")
                || lower.contains("/node_modules/")
                || lower.contains("/packages/")
                || lower.contains("/.nuget/")
            {
                continue;
            }
        }

        // Skip very large files (>2MB likely generated/minified)
        let metadata = std::fs::metadata(abs_path).ok();
        let size = metadata.as_ref().map_or(0, |m| m.len() as usize);
        if size > 2 * 1024 * 1024 {
            tracing::debug!("Skipping large file ({} KB): {}", size / 1024, rel_path);
            continue;
        }

        let language = SupportedLanguage::from_filename(&rel_path);

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

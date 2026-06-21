use std::collections::BTreeMap;
use std::path::Path;

use tauri::State;

use code_explorer_core::config::languages::SupportedLanguage;
use code_explorer_core::graph::types::NodeLabel;

use crate::state::AppState;
use crate::types::{FileContent, FileTreeNode};

/// Build a file tree from the graph's file nodes.
#[tauri::command]
pub async fn get_file_tree(state: State<'_, AppState>) -> Result<Vec<FileTreeNode>, String> {
    let (graph, _indexes, _fts, _repo_path) = state.get_repo(None).await?;

    // Collect all unique file paths from File/Folder nodes
    let mut file_paths: Vec<String> = Vec::new();

    for node in graph.iter_nodes() {
        if node.label == NodeLabel::File {
            file_paths.push(node.properties.file_path.clone());
        }
    }

    file_paths.sort();

    // Build tree structure
    let tree = build_tree_from_paths(&file_paths);
    Ok(tree)
}

/// Read file content from disk.
#[tauri::command]
pub async fn read_file_content(
    state: State<'_, AppState>,
    file_path: String,
    start_line: Option<u32>,
    end_line: Option<u32>,
) -> Result<FileContent, String> {
    let (_graph, _indexes, _fts, repo_path) = state.get_repo(None).await?;

    let full_path = Path::new(&repo_path).join(&file_path);

    // Security: prevent path traversal outside the repo
    let full_canonical = full_path
        .canonicalize()
        .map_err(|e| format!("Failed to resolve path '{}': {}", file_path, e))?;
    let repo_canonical = Path::new(&repo_path)
        .canonicalize()
        .map_err(|e| format!("Failed to resolve repo path: {}", e))?;
    if !full_canonical.starts_with(&repo_canonical) {
        return Err("Access denied: path is outside the repository".to_string());
    }

    let content = std::fs::read_to_string(&full_canonical)
        .map_err(|e| format!("Failed to read file '{}': {}", file_path, e))?;

    let total_lines = content.lines().count();

    // If line range specified, extract subset
    let content = match (start_line, end_line) {
        (Some(start), Some(end)) => {
            // Reject inverted ranges instead of silently returning empty.
            // Without this, callers passing reversed args (or off-by-one)
            // would see an empty file with no indication something is wrong.
            if start == 0 || end < start {
                return Err(format!(
                    "Invalid line range: start={}, end={} (start must be >= 1 and end >= start)",
                    start, end
                ));
            }
            let start_idx = (start as usize) - 1;
            let take_count = (end as usize) - start_idx;
            content
                .lines()
                .skip(start_idx)
                .take(take_count)
                .collect::<Vec<_>>()
                .join("\n")
        }
        _ => content,
    };

    // Detect language from extension. Use the canonical short name from
    // `as_str()` instead of the Debug representation: the latter returns
    // variant names like "CPlusPlus" / "CSharp" / "Php", while frontend
    // syntax highlighters expect the canonical "cpp" / "csharp" / "php".
    let language = Path::new(&file_path)
        .extension()
        .and_then(|ext| ext.to_str())
        .and_then(|ext| SupportedLanguage::from_extension(&format!(".{}", ext)))
        .map(|l| l.as_str().to_string());

    Ok(FileContent {
        path: file_path,
        content,
        language,
        total_lines,
    })
}

fn build_tree_from_paths(paths: &[String]) -> Vec<FileTreeNode> {
    // Normalize Windows backslashes to forward slashes
    let paths: Vec<String> = paths.iter().map(|p| p.replace('\\', "/")).collect();
    let paths_ref: Vec<&str> = paths.iter().map(|s| s.as_str()).collect();

    build_tree_impl(&paths_ref, "")
}

/// Recursive tree builder. `prefix` is the accumulated path prefix from the
/// root, used so leaf and folder nodes carry the full repo-relative path
/// (which the frontend uses to invoke read_file_content).
fn build_tree_impl(paths: &[&str], prefix: &str) -> Vec<FileTreeNode> {
    let mut dir_children: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut local_files: Vec<String> = Vec::new();

    for path in paths {
        let parts: Vec<&str> = path.splitn(2, '/').collect();
        if parts.len() > 1 {
            dir_children
                .entry(parts[0].to_string())
                .or_default()
                .push(parts[1].to_string());
        } else {
            local_files.push(path.to_string());
        }
    }

    let mut result = Vec::new();

    let join = |base: &str, name: &str| -> String {
        if base.is_empty() {
            name.to_string()
        } else {
            format!("{base}/{name}")
        }
    };

    // Add directories
    for (dir_name, child_paths) in &dir_children {
        let child_refs: Vec<&str> = child_paths.iter().map(|s| s.as_str()).collect();
        let dir_path = join(prefix, dir_name);
        let children = build_tree_impl(&child_refs, &dir_path);
        result.push(FileTreeNode {
            name: dir_name.clone(),
            path: dir_path,
            is_dir: true,
            children,
        });
    }

    // Add files at this level
    for file_name in &local_files {
        result.push(FileTreeNode {
            name: file_name.clone(),
            path: join(prefix, file_name),
            is_dir: false,
            children: Vec::new(),
        });
    }

    result
}

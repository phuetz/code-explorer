//! Multi-file rename refactor — desktop equivalent of the MCP `rename` tool.
//!
//! Two confidence buckets:
//!   - `graph_edits`: scoped scans of node files at known line ranges, gated
//!     by graph edges (Calls/Uses/Imports/...). High confidence (0.9-1.0).
//!   - `text_search_edits`: ripgrep-style fallback over the whole repo,
//!     respecting .gitignore. Lower confidence — for review.
//!
//! When `dry_run = false`, only graph_edits are applied to disk.

use std::collections::HashSet;
use std::path::PathBuf;

use tauri::State;

use code_explorer_core::graph::types::*;

use crate::state::AppState;
use crate::types::*;

#[tauri::command]
pub async fn rename_run(
    state: State<'_, AppState>,
    request: RenameRequest,
) -> Result<RenameResult, String> {
    let target = request.target.trim();
    let new_name = request.new_name.trim();
    if target.is_empty() || new_name.is_empty() {
        return Err("target and new_name must be non-empty".into());
    }
    if target == new_name {
        return Err("target and new_name are identical".into());
    }
    let dry_run = request.dry_run.unwrap_or(true);

    let (graph, _idx, _fts, repo_path_str) = state.get_repo(None).await?;
    let repo_path = PathBuf::from(&repo_path_str);

    // ── Resolve target node ids ────────────────────────────────
    let lower = target.to_lowercase();
    let target_ids: HashSet<String> = graph
        .iter_nodes()
        .filter(|n| n.id == target || n.properties.name.to_lowercase() == lower)
        .map(|n| n.id.clone())
        .collect();
    if target_ids.is_empty() {
        return Err(format!("Symbol '{target}' not found in graph"));
    }

    // ── Collect referencing source nodes ───────────────────────
    let want = |rt: RelationshipType| {
        matches!(
            rt,
            RelationshipType::Calls
                | RelationshipType::Uses
                | RelationshipType::Imports
                | RelationshipType::Inherits
                | RelationshipType::Implements
                | RelationshipType::Extends
                | RelationshipType::Overrides
                | RelationshipType::CallsAction
                | RelationshipType::CallsService
        )
    };
    let mut ref_sources: HashSet<String> = HashSet::new();
    for rel in graph.iter_relationships() {
        if want(rel.rel_type) && target_ids.contains(&rel.target_id) {
            ref_sources.insert(rel.source_id.clone());
        }
    }

    // ── Build word-boundary regex once ─────────────────────────
    let word_re = match regex::Regex::new(&format!(r"\b{}\b", regex::escape(target))) {
        Ok(re) => re,
        Err(e) => return Err(format!("Invalid identifier '{target}': {e}")),
    };

    // ── graph_edits: definitions + scoped reference sites ──────
    let mut graph_edits: Vec<RenameEdit> = Vec::new();
    let mut covered: HashSet<(String, u32)> = HashSet::new();

    for tid in &target_ids {
        if let Some(tn) = graph.get_node(tid) {
            for edit in scan_node(&repo_path, tn, &word_re, new_name, "definition", 1.0) {
                let key = (edit.file.clone(), edit.line);
                if covered.insert(key) {
                    graph_edits.push(edit);
                }
            }
        }
    }
    for src_id in &ref_sources {
        if let Some(src) = graph.get_node(src_id) {
            for edit in scan_node(&repo_path, src, &word_re, new_name, "reference", 0.9) {
                let key = (edit.file.clone(), edit.line);
                if covered.insert(key) {
                    graph_edits.push(edit);
                }
            }
        }
    }

    // ── text_search_edits: walk repo, skip already-covered lines ─
    let text_search_edits = walk_repo(&repo_path, &word_re, new_name, &covered);

    let files_affected: HashSet<String> = graph_edits
        .iter()
        .chain(text_search_edits.iter())
        .map(|e| e.file.clone())
        .collect();

    let applied = if !dry_run && !graph_edits.is_empty() {
        apply(&repo_path, &graph_edits).ok()
    } else {
        None
    };

    Ok(RenameResult {
        target: target.to_string(),
        new_name: new_name.to_string(),
        dry_run,
        files_affected: files_affected.len() as u32,
        graph_edits,
        text_search_edits,
        applied,
    })
}

fn scan_node(
    repo: &std::path::Path,
    node: &code_explorer_core::graph::types::GraphNode,
    re: &regex::Regex,
    new_name: &str,
    reason: &str,
    confidence: f64,
) -> Vec<RenameEdit> {
    let fp = &node.properties.file_path;
    if fp.is_empty() {
        return Vec::new();
    }
    let full = repo.join(fp);
    let canonical_repo = match repo.canonicalize() {
        Ok(p) => p,
        Err(_) => return Vec::new(),
    };
    let canonical_file = match full.canonicalize() {
        Ok(p) => p,
        Err(_) => return Vec::new(),
    };
    if !canonical_file.starts_with(&canonical_repo) {
        return Vec::new();
    }
    let content = match std::fs::read_to_string(&canonical_file) {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };
    let start = node.properties.start_line.unwrap_or(1).saturating_sub(1) as usize;
    let end = node.properties.end_line.unwrap_or(u32::MAX) as usize;

    let mut out = Vec::new();
    for (idx, line) in content.lines().enumerate() {
        if idx < start || idx > end.saturating_sub(1) {
            continue;
        }
        for m in re.find_iter(line) {
            out.push(RenameEdit {
                file: fp.clone(),
                line: (idx + 1) as u32,
                col: (m.start() + 1) as u32,
                old_text: m.as_str().to_string(),
                new_text: new_name.to_string(),
                snippet: truncate(line, 160),
                confidence,
                reason: reason.to_string(),
            });
        }
    }
    out
}

fn walk_repo(
    repo: &std::path::Path,
    re: &regex::Regex,
    new_name: &str,
    covered: &HashSet<(String, u32)>,
) -> Vec<RenameEdit> {
    let mut out = Vec::new();
    let walker = ignore::WalkBuilder::new(repo)
        .hidden(false)
        .git_ignore(true)
        .build();
    for entry in walker.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            if matches!(
                ext,
                "png"
                    | "jpg"
                    | "jpeg"
                    | "gif"
                    | "ico"
                    | "pdf"
                    | "zip"
                    | "tar"
                    | "gz"
                    | "bin"
                    | "exe"
                    | "dll"
                    | "so"
                    | "dylib"
                    | "jar"
                    | "class"
                    | "o"
                    | "a"
                    | "lib"
                    | "wasm"
                    | "mp4"
                    | "mp3"
                    | "woff"
                    | "woff2"
                    | "ttf"
                    | "eot"
            ) {
                continue;
            }
        }
        let content = match std::fs::read_to_string(path) {
            Ok(c) => c,
            Err(_) => continue,
        };
        let rel = match path.strip_prefix(repo) {
            Ok(p) => p.to_string_lossy().replace('\\', "/"),
            Err(_) => continue,
        };
        for (idx, line) in content.lines().enumerate() {
            let line_no = (idx + 1) as u32;
            if covered.contains(&(rel.clone(), line_no)) {
                continue;
            }
            if let Some(m) = re.find(line) {
                out.push(RenameEdit {
                    file: rel.clone(),
                    line: line_no,
                    col: (m.start() + 1) as u32,
                    old_text: m.as_str().to_string(),
                    new_text: new_name.to_string(),
                    snippet: truncate(line, 160),
                    confidence: 0.5,
                    reason: "text-search".into(),
                });
            }
        }
        if out.len() > 500 {
            break;
        }
    }
    out
}

fn truncate(s: &str, max: usize) -> String {
    let t = s.trim();
    if t.len() <= max {
        t.to_string()
    } else {
        let end = t
            .char_indices()
            .take_while(|(i, _)| *i < max)
            .last()
            .map(|(i, c)| i + c.len_utf8())
            .unwrap_or(0);
        format!("{}…", &t[..end])
    }
}

fn apply(repo: &std::path::Path, edits: &[RenameEdit]) -> std::io::Result<serde_json::Value> {
    use std::collections::BTreeMap;
    let mut by_file: BTreeMap<String, Vec<&RenameEdit>> = BTreeMap::new();
    for e in edits {
        by_file.entry(e.file.clone()).or_default().push(e);
    }
    let mut applied_per_file: Vec<serde_json::Value> = Vec::new();
    for (file, mut edits) in by_file {
        edits.sort_by(|a, b| b.line.cmp(&a.line).then(b.col.cmp(&a.col)));
        let full = repo.join(&file);
        let content = std::fs::read_to_string(&full)?;
        let mut lines: Vec<String> = content.lines().map(String::from).collect();
        let mut count = 0u32;
        for e in &edits {
            let idx = (e.line as usize).saturating_sub(1);
            if idx >= lines.len() {
                continue;
            }
            let col = (e.col as usize).saturating_sub(1);
            let line = &lines[idx];
            let end = col + e.old_text.len();
            if end <= line.len() && line[col..end] == *e.old_text {
                let mut patched = String::with_capacity(line.len() + e.new_text.len());
                patched.push_str(&line[..col]);
                patched.push_str(&e.new_text);
                patched.push_str(&line[end..]);
                lines[idx] = patched;
                count += 1;
            }
        }
        let mut new_content = lines.join("\n");
        if content.ends_with('\n') {
            new_content.push('\n');
        }
        std::fs::write(&full, new_content)?;
        applied_per_file.push(serde_json::json!({"file": file, "applied": count}));
    }
    Ok(serde_json::json!({"files": applied_per_file}))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_truncate_short() {
        assert_eq!(truncate("hello", 10), "hello");
    }

    #[test]
    fn test_truncate_long() {
        assert_eq!(truncate("abcdefghij", 5), "abcde…");
    }
}

//! Prose indexing: Markdown, plain text and reStructuredText.
//!
//! The code walker only keeps files whose extension maps to a
//! [`SupportedLanguage`](code_explorer_core::config::languages::SupportedLanguage),
//! so a repository of prose — a book, a documentation site, a knowledge base —
//! indexed to a handful of nodes and `context`/`search_code` had nothing to
//! answer with. This phase gives those repositories the same treatment code
//! gets: one `File` node per document, one `Section` node per heading, and a
//! `File --Imports--> File` edge per resolved internal link.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use code_explorer_core::graph::types::{
    GraphNode, GraphRelationship, NodeLabel, NodeProperties, RelationshipType,
};
use code_explorer_core::graph::KnowledgeGraph;
use code_explorer_core::id::generate_id;

/// Kind of prose document.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DocKind {
    Markdown,
    Text,
    RestructuredText,
}

impl DocKind {
    /// Recognise a prose document from its path. `None` for everything else.
    pub fn from_path(rel_path: &str) -> Option<Self> {
        let ext = Path::new(rel_path)
            .extension()
            .and_then(|e| e.to_str())?
            .to_lowercase();
        match ext.as_str() {
            "md" | "markdown" | "mdown" | "mkd" | "mdx" => Some(Self::Markdown),
            "txt" | "text" => Some(Self::Text),
            "rst" => Some(Self::RestructuredText),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Markdown => "markdown",
            Self::Text => "text",
            Self::RestructuredText => "rst",
        }
    }
}

/// A prose file discovered during the scan.
#[derive(Debug, Clone)]
pub struct DocEntry {
    /// Relative path with forward slashes.
    pub path: String,
    pub content: String,
    pub size: usize,
    pub kind: DocKind,
}

/// A heading extracted from a document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Heading {
    /// 1 for `#`, 2 for `##`, …
    pub level: u8,
    pub text: String,
    /// 1-indexed line of the heading itself.
    pub line: u32,
}

/// An internal link extracted from a document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocLink {
    /// Link target exactly as written.
    pub target: String,
    pub line: u32,
}

/// What this phase added to the graph.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct DocStats {
    pub documents: usize,
    pub headings: usize,
    pub links_resolved: usize,
    pub links_unresolved: usize,
}

// ─── Discovery ───────────────────────────────────────────────────────────

/// Walk the repository and collect prose files, **without reading them**.
///
/// Mirrors `structure::walk_repository`: same walker, same exclusion rules,
/// same 2 MB ceiling — only the extension filter differs. Contents
/// are left empty so the caller can decide whether prose is worth indexing
/// before paying to read it; [`load_document_contents`] fills them in.
pub fn walk_documents(repo_path: &Path) -> Result<Vec<DocEntry>, crate::IngestError> {
    walk_documents_with(
        repo_path,
        &code_explorer_core::config::exclusions::ExclusionRules::for_repo(repo_path),
    )
}

/// [`walk_documents`] under explicit exclusion rules.
pub fn walk_documents_with(
    repo_path: &Path,
    rules: &code_explorer_core::config::exclusions::ExclusionRules,
) -> Result<Vec<DocEntry>, crate::IngestError> {
    let mut entries = Vec::new();

    for result in super::structure::build_walker(repo_path, rules) {
        let entry = result.map_err(|e| crate::IngestError::PhaseError {
            phase: "docs".to_string(),
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
        let Some(kind) = DocKind::from_path(&rel_path) else {
            continue;
        };
        let size = std::fs::metadata(abs_path).map(|m| m.len() as usize).unwrap_or(0);
        if size > 2 * 1024 * 1024 {
            tracing::debug!("Skipping large document ({} KB): {}", size / 1024, rel_path);
            continue;
        }
        entries.push(DocEntry {
            path: rel_path,
            content: String::new(),
            size,
            kind,
        });
    }

    entries.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(entries)
}

/// Read the contents of documents discovered by [`walk_documents`].
/// Unreadable files are dropped with a warning.
pub fn load_document_contents(repo_path: &Path, docs: &mut Vec<DocEntry>) {
    docs.retain_mut(|doc| match std::fs::read_to_string(repo_path.join(&doc.path)) {
        Ok(content) => {
            doc.content = content;
            true
        }
        Err(e) => {
            tracing::warn!("Cannot read {}: {}", doc.path, e);
            false
        }
    });
}

/// Default decision: index prose when the repository is mostly prose.
///
/// A code project with a README should not pay for a documentation pass; a
/// book repository with a build script must not lose 99 % of its content.
/// The threshold is on the share of code files: below 30 %, prose wins.
/// `--include-docs` / `--no-docs` override this.
pub fn should_index_docs(code_files: usize, doc_files: usize) -> bool {
    if doc_files == 0 {
        return false;
    }
    let total = code_files + doc_files;
    // code_files / total < 0.30, in integer arithmetic.
    code_files * 100 < total * 30
}

// ─── Parsing ─────────────────────────────────────────────────────────────

/// Iterate over lines that are not inside a fenced code block.
///
/// Returns `(line_number_1_indexed, line)`; fences (``` or ~~~) are skipped
/// along with their contents, so `# not a heading` inside an example stays an
/// example.
fn prose_lines(content: &str) -> Vec<(u32, &str)> {
    let mut out = Vec::new();
    let mut fence: Option<char> = None;
    let mut fence_len = 0usize;
    for (idx, line) in content.lines().enumerate() {
        let trimmed = line.trim_start();
        let fence_char = trimmed.chars().next().filter(|c| *c == '`' || *c == '~');
        let run = fence_char
            .map(|c| trimmed.chars().take_while(|x| *x == c).count())
            .unwrap_or(0);
        if run >= 3 {
            match fence {
                None => {
                    fence = fence_char;
                    fence_len = run;
                    continue;
                }
                Some(open) if Some(open) == fence_char && run >= fence_len => {
                    fence = None;
                    continue;
                }
                Some(_) => {}
            }
        }
        if fence.is_none() {
            out.push((idx as u32 + 1, line));
        }
    }
    out
}

/// Extract headings from a document.
///
/// Markdown ATX (`# Title`) and Setext (`Title` over `===` / `---`); for
/// reStructuredText, a line underlined by a run of punctuation; plain text has
/// no heading syntax, so its documents keep a single node.
pub fn extract_headings(content: &str, kind: DocKind) -> Vec<Heading> {
    let mut headings = Vec::new();
    match kind {
        DocKind::Text => return headings,
        DocKind::Markdown => {
            let lines = prose_lines(content);
            for (i, (line_no, line)) in lines.iter().enumerate() {
                let trimmed = line.trim_start();
                if trimmed.starts_with('#') {
                    let level = trimmed.chars().take_while(|c| *c == '#').count();
                    if (1..=6).contains(&level) {
                        let rest = &trimmed[level..];
                        // "#Heading" is not a heading in CommonMark.
                        if rest.starts_with(' ') || rest.is_empty() {
                            let text = clean_heading(rest);
                            if !text.is_empty() {
                                headings.push(Heading {
                                    level: level as u8,
                                    text,
                                    line: *line_no,
                                });
                            }
                        }
                    }
                    continue;
                }
                // Setext: the *next* prose line is all '=' or all '-'.
                if let Some((_, next)) = lines.get(i + 1) {
                    let underline = next.trim();
                    if underline.len() >= 2
                        && (underline.chars().all(|c| c == '=') || underline.chars().all(|c| c == '-'))
                        && !trimmed.is_empty()
                        && !trimmed.starts_with('|')
                    {
                        let text = clean_heading(trimmed);
                        if !text.is_empty() {
                            headings.push(Heading {
                                level: if underline.starts_with('=') { 1 } else { 2 },
                                text,
                                line: *line_no,
                            });
                        }
                    }
                }
            }
        }
        DocKind::RestructuredText => {
            let lines: Vec<(u32, &str)> = content
                .lines()
                .enumerate()
                .map(|(i, l)| (i as u32 + 1, l))
                .collect();
            for (i, (line_no, line)) in lines.iter().enumerate() {
                let title = line.trim();
                if title.is_empty() {
                    continue;
                }
                let Some((_, next)) = lines.get(i + 1) else {
                    continue;
                };
                let underline = next.trim();
                if underline.len() >= title.len()
                    && underline.len() >= 2
                    && underline.chars().all(|c| "=-~^\"'`#*+".contains(c))
                    && underline.chars().all(|c| c == underline.chars().next().unwrap())
                {
                    headings.push(Heading {
                        level: 1,
                        text: clean_heading(title),
                        line: *line_no,
                    });
                }
            }
        }
    }
    headings
}

/// Strip decoration around a heading: leading `#`, trailing `#`, emphasis,
/// inline code ticks and surrounding whitespace.
fn clean_heading(raw: &str) -> String {
    raw.trim()
        .trim_end_matches('#')
        .trim()
        .trim_matches(|c| c == '*' || c == '_' || c == '`')
        .trim()
        .to_string()
}

/// Extract inline Markdown links `[text](target)`, ignoring images
/// (`![alt](src)`) and code fences.
pub fn extract_links(content: &str, kind: DocKind) -> Vec<DocLink> {
    let mut links = Vec::new();
    if kind != DocKind::Markdown {
        return links;
    }
    for (line_no, line) in prose_lines(content) {
        let bytes: Vec<char> = line.chars().collect();
        let mut i = 0usize;
        while i < bytes.len() {
            if bytes[i] != '[' {
                i += 1;
                continue;
            }
            // Skip image syntax.
            if i > 0 && bytes[i - 1] == '!' {
                i += 1;
                continue;
            }
            let Some(close) = find_char(&bytes, i + 1, ']') else {
                break;
            };
            if close + 1 >= bytes.len() || bytes[close + 1] != '(' {
                i = close + 1;
                continue;
            }
            let Some(paren) = find_char(&bytes, close + 2, ')') else {
                break;
            };
            let target: String = bytes[close + 2..paren].iter().collect();
            let target = target.trim();
            if !target.is_empty() {
                links.push(DocLink {
                    target: target.to_string(),
                    line: line_no,
                });
            }
            i = paren + 1;
        }
    }
    links
}

fn find_char(chars: &[char], from: usize, needle: char) -> Option<usize> {
    (from..chars.len()).find(|&i| chars[i] == needle)
}

/// Resolve a link target to a repository-relative path.
///
/// Returns `None` for anything that does not point at a file in this
/// repository: absolute URLs, mail links, protocol-relative URLs, bare
/// anchors, and paths that climb above the repository root.
pub fn resolve_link(from_rel_path: &str, target: &str) -> Option<String> {
    let target = target.split_whitespace().next()?; // drop `(path "title")`
    if target.is_empty() || target.starts_with('#') {
        return None;
    }
    let lower = target.to_lowercase();
    if lower.starts_with("http://")
        || lower.starts_with("https://")
        || lower.starts_with("mailto:")
        || lower.starts_with("ftp://")
        || lower.starts_with("//")
        || lower.starts_with("tel:")
        || lower.starts_with("data:")
    {
        return None;
    }
    // Strip the anchor and any query string.
    let path_part = target.split(['#', '?']).next().unwrap_or(target);
    if path_part.is_empty() {
        return None;
    }
    let path_part = path_part.replace("%20", " ");

    let mut segments: Vec<String> = Vec::new();
    if !path_part.starts_with('/') {
        // Relative to the directory holding the document.
        if let Some((dir, _)) = from_rel_path.rsplit_once('/') {
            segments.extend(dir.split('/').map(|s| s.to_string()));
        }
    }
    for segment in path_part.trim_start_matches('/').split('/') {
        match segment {
            "" | "." => {}
            ".." => {
                // A link that climbs above the repository root points outside
                // the index; drop it rather than inventing a path.
                segments.pop()?;
            }
            other => segments.push(other.to_string()),
        }
    }
    if segments.is_empty() {
        return None;
    }
    Some(segments.join("/"))
}

// ─── Graph construction ──────────────────────────────────────────────────

/// Add File nodes, Section nodes and internal-link edges for `docs`.
///
/// `known_files` holds the relative paths already present in the graph (the
/// code files), so a link from a document to a source file resolves too.
pub fn create_document_nodes(
    graph: &mut KnowledgeGraph,
    docs: &[DocEntry],
    known_files: &HashSet<String>,
) -> DocStats {
    let mut stats = DocStats::default();
    let mut doc_paths: HashSet<String> = HashSet::new();
    for doc in docs {
        doc_paths.insert(doc.path.clone());
    }

    // Folder chain + File node, so prose sits in the same tree as code.
    let mut created_folders: HashSet<String> = HashSet::new();
    for doc in docs {
        let parts: Vec<&str> = doc.path.split('/').collect();
        let mut current = String::new();
        let mut parent_id: Option<String> = None;
        for (i, part) in parts.iter().enumerate() {
            if !current.is_empty() {
                current.push('/');
            }
            current.push_str(part);
            let is_file = i == parts.len() - 1;
            let label = if is_file { NodeLabel::File } else { NodeLabel::Folder };
            let node_id = generate_id(label.as_str(), &current);
            if is_file || created_folders.insert(current.clone()) {
                graph.add_node(GraphNode {
                    id: node_id.clone(),
                    label,
                    properties: NodeProperties {
                        name: part.to_string(),
                        file_path: current.clone(),
                        heuristic_label: is_file.then(|| doc.kind.as_str().to_string()),
                        ..Default::default()
                    },
                });
            }
            if let Some(pid) = &parent_id {
                graph.add_relationship(GraphRelationship {
                    id: format!("contains_{pid}_{node_id}"),
                    source_id: pid.clone(),
                    target_id: node_id.clone(),
                    rel_type: RelationshipType::Contains,
                    confidence: 1.0,
                    reason: String::new(),
                    step: None,
                });
            }
            parent_id = Some(node_id);
        }
        stats.documents += 1;

        // Headings become Section nodes, nested by level.
        let file_id = generate_id(NodeLabel::File.as_str(), &doc.path);
        let headings = extract_headings(&doc.content, doc.kind);
        let total_lines = doc.content.lines().count() as u32;
        // A section runs until the next heading of the same or higher rank.
        let mut section_ids: Vec<(u8, String)> = Vec::new();
        let mut seen_ids: HashMap<String, u32> = HashMap::new();
        for (idx, heading) in headings.iter().enumerate() {
            let end_line = headings
                .iter()
                .skip(idx + 1)
                .find(|h| h.level <= heading.level)
                .map(|h| h.line.saturating_sub(1))
                .unwrap_or(total_lines)
                .max(heading.line);

            let base = format!("{}#{}", doc.path, heading.text);
            let unique = match seen_ids.entry(base.clone()) {
                std::collections::hash_map::Entry::Occupied(mut e) => {
                    let n = e.get() + 1;
                    e.insert(n);
                    format!("{base}~{n}")
                }
                std::collections::hash_map::Entry::Vacant(e) => {
                    e.insert(1);
                    base.clone()
                }
            };
            let section_id = generate_id(NodeLabel::Section.as_str(), &unique);
            graph.add_node(GraphNode {
                id: section_id.clone(),
                label: NodeLabel::Section,
                properties: NodeProperties {
                    name: heading.text.clone(),
                    file_path: doc.path.clone(),
                    start_line: Some(heading.line),
                    end_line: Some(end_line),
                    heuristic_label: Some(format!("h{}", heading.level)),
                    ..Default::default()
                },
            });
            graph.add_relationship(GraphRelationship {
                id: format!("defines_{file_id}_{section_id}"),
                source_id: file_id.clone(),
                target_id: section_id.clone(),
                rel_type: RelationshipType::Defines,
                confidence: 1.0,
                reason: "document-heading".to_string(),
                step: None,
            });
            // Nest under the closest enclosing heading.
            while section_ids
                .last()
                .is_some_and(|(level, _)| *level >= heading.level)
            {
                section_ids.pop();
            }
            if let Some((_, parent)) = section_ids.last() {
                graph.add_relationship(GraphRelationship {
                    id: format!("contains_{parent}_{section_id}"),
                    source_id: parent.clone(),
                    target_id: section_id.clone(),
                    rel_type: RelationshipType::Contains,
                    confidence: 1.0,
                    reason: "document-subheading".to_string(),
                    step: None,
                });
            }
            section_ids.push((heading.level, section_id));
            stats.headings += 1;
        }
    }

    // Internal links, once every document node exists.
    for doc in docs {
        let file_id = generate_id(NodeLabel::File.as_str(), &doc.path);
        let mut linked: HashSet<String> = HashSet::new();
        for link in extract_links(&doc.content, doc.kind) {
            let Some(resolved) = resolve_link(&doc.path, &link.target) else {
                continue;
            };
            let candidates = [
                resolved.clone(),
                format!("{resolved}.md"),
                format!("{resolved}/README.md"),
                format!("{resolved}/index.md"),
            ];
            let Some(target_path) = candidates
                .iter()
                .find(|c| doc_paths.contains(*c) || known_files.contains(*c))
            else {
                stats.links_unresolved += 1;
                continue;
            };
            if target_path == &doc.path || !linked.insert(target_path.clone()) {
                continue;
            }
            let target_id = generate_id(NodeLabel::File.as_str(), target_path);
            graph.add_relationship(GraphRelationship {
                id: format!("doclink_{file_id}_{target_id}"),
                source_id: file_id.clone(),
                target_id,
                rel_type: RelationshipType::Imports,
                confidence: 1.0,
                reason: "markdown-link".to_string(),
                step: None,
            });
            stats.links_resolved += 1;
        }
    }

    stats
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognises_prose_extensions() {
        assert_eq!(DocKind::from_path("a/b.md"), Some(DocKind::Markdown));
        assert_eq!(DocKind::from_path("NOTES.MARKDOWN"), Some(DocKind::Markdown));
        assert_eq!(DocKind::from_path("x.mdx"), Some(DocKind::Markdown));
        assert_eq!(DocKind::from_path("readme.txt"), Some(DocKind::Text));
        assert_eq!(DocKind::from_path("guide.rst"), Some(DocKind::RestructuredText));
        assert_eq!(DocKind::from_path("main.rs"), None);
        assert_eq!(DocKind::from_path("Makefile"), None);
    }

    #[test]
    fn prose_repositories_index_docs_by_default_code_repositories_do_not() {
        // A book: 300 chapters, a build script.
        assert!(should_index_docs(1, 300));
        // A code project with a README.
        assert!(!should_index_docs(500, 3));
        // Exactly at the 30 % boundary: 30 code files out of 100 is not "less
        // than 30 % of code".
        assert!(!should_index_docs(30, 70));
        assert!(should_index_docs(29, 71));
        // No prose at all.
        assert!(!should_index_docs(10, 0));
    }

    #[test]
    fn extracts_atx_and_setext_headings() {
        let md = "# Title\n\nintro\n\n## Chapter one\n\ntext\n\nSetext heading\n====\n\n### Deep ###\n";
        let headings = extract_headings(md, DocKind::Markdown);
        let names: Vec<&str> = headings.iter().map(|h| h.text.as_str()).collect();
        assert_eq!(names, vec!["Title", "Chapter one", "Setext heading", "Deep"]);
        assert_eq!(headings[0].level, 1);
        assert_eq!(headings[1].level, 2);
        assert_eq!(headings[2].level, 1);
        assert_eq!(headings[3].level, 3);
        assert_eq!(headings[0].line, 1);
        assert_eq!(headings[1].line, 5);
    }

    #[test]
    fn headings_inside_code_fences_are_not_headings() {
        let md = "# Real\n\n```sh\n# not a heading\n```\n\n## Also real\n";
        let names: Vec<String> = extract_headings(md, DocKind::Markdown)
            .into_iter()
            .map(|h| h.text)
            .collect();
        assert_eq!(names, vec!["Real", "Also real"]);
    }

    #[test]
    fn hash_without_a_space_is_not_a_heading() {
        assert!(extract_headings("#hashtag\n", DocKind::Markdown).is_empty());
    }

    #[test]
    fn extracts_rst_headings() {
        let rst = "Title\n=====\n\nbody\n\nSection\n-------\n";
        let names: Vec<String> = extract_headings(rst, DocKind::RestructuredText)
            .into_iter()
            .map(|h| h.text)
            .collect();
        assert_eq!(names, vec!["Title", "Section"]);
    }

    #[test]
    fn extracts_links_but_not_images_or_fenced_examples() {
        let md = "See [one](a/b.md) and ![pic](img.png).\n\n```\n[fake](x.md)\n```\n\n[two](../c.md)\n";
        let links = extract_links(md, DocKind::Markdown);
        let targets: Vec<&str> = links.iter().map(|l| l.target.as_str()).collect();
        assert_eq!(targets, vec!["a/b.md", "../c.md"]);
    }

    #[test]
    fn resolves_relative_links_and_rejects_external_ones() {
        assert_eq!(
            resolve_link("docs/guide/intro.md", "../api/reference.md"),
            Some("docs/api/reference.md".to_string())
        );
        assert_eq!(
            resolve_link("docs/intro.md", "./deep/page.md#anchor"),
            Some("docs/deep/page.md".to_string())
        );
        assert_eq!(
            resolve_link("docs/intro.md", "/README.md"),
            Some("README.md".to_string())
        );
        assert_eq!(resolve_link("a.md", "https://example.com/x.md"), None);
        assert_eq!(resolve_link("a.md", "mailto:someone@example.invalid"), None);
        assert_eq!(resolve_link("a.md", "#section"), None);
        assert_eq!(resolve_link("a.md", "../../outside.md"), None);
    }

    #[test]
    fn builds_sections_and_link_edges() {
        let docs = vec![
            DocEntry {
                path: "index.md".to_string(),
                content: "# Home\n\nSee [chapter](chapters/one.md).\n\n## Sub\n\ntext\n".to_string(),
                size: 0,
                kind: DocKind::Markdown,
            },
            DocEntry {
                path: "chapters/one.md".to_string(),
                content: "# Chapter One\n\nBack to [home](../index.md).\n".to_string(),
                size: 0,
                kind: DocKind::Markdown,
            },
        ];
        let mut graph = KnowledgeGraph::new();
        let stats = create_document_nodes(&mut graph, &docs, &HashSet::new());

        assert_eq!(stats.documents, 2);
        assert_eq!(stats.headings, 3);
        assert_eq!(stats.links_resolved, 2);

        let names: Vec<String> = graph
            .nodes()
            .iter()
            .filter(|n| n.label == NodeLabel::Section)
            .map(|n| n.properties.name.clone())
            .collect();
        assert!(names.contains(&"Home".to_string()));
        assert!(names.contains(&"Chapter One".to_string()));
        assert!(names.contains(&"Sub".to_string()));

        let link_edges = graph
            .relationships()
            .iter()
            .filter(|r| r.reason == "markdown-link")
            .count();
        assert_eq!(link_edges, 2);

        // `Sub` nests under `Home`.
        let home = generate_id(NodeLabel::Section.as_str(), "index.md#Home");
        let sub = generate_id(NodeLabel::Section.as_str(), "index.md#Sub");
        assert!(graph
            .relationships()
            .iter()
            .any(|r| r.source_id == home && r.target_id == sub));
    }

    #[test]
    fn repeated_headings_get_distinct_nodes() {
        let docs = vec![DocEntry {
            path: "d.md".to_string(),
            content: "## Notes\n\na\n\n## Notes\n\nb\n".to_string(),
            size: 0,
            kind: DocKind::Markdown,
        }];
        let mut graph = KnowledgeGraph::new();
        let stats = create_document_nodes(&mut graph, &docs, &HashSet::new());
        assert_eq!(stats.headings, 2);
        assert_eq!(
            graph
                .nodes()
                .iter()
                .filter(|n| n.label == NodeLabel::Section)
                .count(),
            2
        );
    }

    #[test]
    fn a_link_to_a_source_file_resolves_too() {
        let docs = vec![DocEntry {
            path: "README.md".to_string(),
            content: "Start at [the entry point](src/main.rs).\n".to_string(),
            size: 0,
            kind: DocKind::Markdown,
        }];
        let mut known = HashSet::new();
        known.insert("src/main.rs".to_string());
        let mut graph = KnowledgeGraph::new();
        let stats = create_document_nodes(&mut graph, &docs, &known);
        assert_eq!(stats.links_resolved, 1);
    }

    #[test]
    fn a_dangling_link_is_counted_not_created() {
        let docs = vec![DocEntry {
            path: "a.md".to_string(),
            content: "[gone](missing.md)\n".to_string(),
            size: 0,
            kind: DocKind::Markdown,
        }];
        let mut graph = KnowledgeGraph::new();
        let stats = create_document_nodes(&mut graph, &docs, &HashSet::new());
        assert_eq!(stats.links_resolved, 0);
        assert_eq!(stats.links_unresolved, 1);
        assert!(graph.relationships().iter().all(|r| r.reason != "markdown-link"));
    }
}

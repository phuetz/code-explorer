//! Pre-delivery linter for the generated documentation.
//!
//! Walks `<docs_dir>/**/*.md` and reports issues that would embarrass us in
//! front of a client: residual TODO/TBD markers, unfilled `<!-- GNX:* -->`
//! enrichment anchors, broken markdown links, sections under 50 words, and
//! Sample v1.1 §4 Algorithmes missing from service / controller pages.
//!
//! Lives in `code-explorer-rag` (rather than the CLI) so both the `code-explorer
//! validate-docs` command and the desktop chat tool `validate_sfd` can call
//! the same checker — extracted from the CLI in P1.1 to avoid drift between
//! the shell-pipeline gate and the in-app preview.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Red,
    Yellow,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Issue {
    pub severity: Severity,
    pub kind: String,
    pub line: Option<usize>,
    pub detail: String,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct PageReport {
    pub path: String,
    pub issues: Vec<Issue>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationReport {
    pub repo: String,
    pub generated_at: String,
    pub total_pages: usize,
    pub pages_with_issues: usize,
    pub red_count: usize,
    pub yellow_count: usize,
    pub by_kind: BTreeMap<String, usize>,
    pub pages: Vec<PageReport>,
}

/// Library entry point — runs every check against the docs tree and returns
/// a structured report without touching stdout, stderr, or the exit code.
pub fn validate(docs_dir: &Path, repo_label: &str) -> Result<ValidationReport> {
    if !docs_dir.exists() {
        anyhow::bail!(
            "No documentation found at {}. Run `code-explorer generate docs` first \
             (or point at an explicit docs directory).",
            docs_dir.display()
        );
    }

    let md_files = collect_md_files(docs_dir)?;
    let mut pages: Vec<PageReport> = Vec::new();
    let mut by_kind: BTreeMap<String, usize> = BTreeMap::new();
    let mut red_count = 0usize;
    let mut yellow_count = 0usize;

    let known_files: Vec<PathBuf> = md_files.clone();

    for path in &md_files {
        let content = std::fs::read_to_string(path).unwrap_or_default();
        let mut report = PageReport {
            path: relativize(docs_dir, path),
            issues: Vec::new(),
        };
        check_residual_todos(&content, &mut report.issues);
        check_unfilled_anchors(&content, &mut report.issues);
        check_short_sections(&content, &mut report.issues);
        check_broken_internal_links(path, &content, &known_files, &mut report.issues);
        check_methodo_alise_section_4(path, &content, &mut report.issues);

        for iss in &report.issues {
            *by_kind.entry(iss.kind.clone()).or_default() += 1;
            match iss.severity {
                Severity::Red => red_count += 1,
                Severity::Yellow => yellow_count += 1,
            }
        }
        if !report.issues.is_empty() {
            pages.push(report);
        }
    }

    Ok(ValidationReport {
        repo: repo_label.to_string(),
        generated_at: chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
        total_pages: md_files.len(),
        pages_with_issues: pages.len(),
        red_count,
        yellow_count,
        by_kind,
        pages,
    })
}

// ─── Walking the docs tree ─────────────────────────────────────────────

fn collect_md_files(docs_dir: &Path) -> Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    walk(docs_dir, &mut out)?;
    out.sort();
    Ok(out)
}

fn walk(dir: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    for entry in std::fs::read_dir(dir).with_context(|| format!("read {}", dir.display()))? {
        let entry = entry?;
        let path = entry.path();
        // Skip the meta directory — we don't validate cache or queue payloads.
        if path
            .file_name()
            .and_then(|n| n.to_str())
            .map(|n| n == "_meta")
            .unwrap_or(false)
        {
            continue;
        }
        if path.is_dir() {
            walk(&path, out)?;
        } else if path.extension().and_then(|e| e.to_str()) == Some("md") {
            out.push(path);
        }
    }
    Ok(())
}

fn relativize(root: &Path, p: &Path) -> String {
    p.strip_prefix(root)
        .map(|r| r.display().to_string().replace('\\', "/"))
        .unwrap_or_else(|_| p.display().to_string())
}

// ─── Individual checks ─────────────────────────────────────────────────

/// RED — TODO / TBD / FIXME / XXX in the body. Symptom of an LLM that
/// echoed the prompt or a hand-written stub that was never finished.
fn check_residual_todos(content: &str, issues: &mut Vec<Issue>) {
    for (idx, line) in content.lines().enumerate() {
        if line.trim_start().starts_with("```") {
            continue;
        }
        for marker in ["TODO", "TBD", "FIXME", "XXX"] {
            if line.contains(marker) {
                issues.push(Issue {
                    severity: Severity::Red,
                    kind: "residual_todo".to_string(),
                    line: Some(idx + 1),
                    detail: format!("contains marker `{}`", marker),
                });
                break;
            }
        }
    }
}

/// RED — `<!-- GNX:TIP:foo -->` comments still in the file mean enrichment
/// silently failed for that section. Word renders the comment as visible
/// HTML, so this leaks straight to the client.
fn check_unfilled_anchors(content: &str, issues: &mut Vec<Issue>) {
    for (idx, line) in content.lines().enumerate() {
        if let Some(start) = line.find("<!-- GNX:") {
            let after = &line[start..];
            if let Some(end) = after.find("-->") {
                let anchor = &after[..end + 3];
                issues.push(Issue {
                    severity: Severity::Red,
                    kind: "unfilled_anchor".to_string(),
                    line: Some(idx + 1),
                    detail: format!("anchor `{}` was never replaced by enrichment", anchor),
                });
            }
        }
    }
}

/// YELLOW — H1/H2 sections shorter than 50 words read as stubs in a Word
/// doc. Body text is counted between the heading and the next heading or
/// EOF, excluding code fences and tables.
fn check_short_sections(content: &str, issues: &mut Vec<Issue>) {
    let lines: Vec<&str> = content.lines().collect();
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        let level = heading_level(line);
        if level == 1 || level == 2 {
            let title = line.trim_start_matches('#').trim().to_string();
            let start = i + 1;
            let mut end = start;
            let mut in_code = false;
            while end < lines.len() {
                let l = lines[end];
                if l.trim_start().starts_with("```") {
                    in_code = !in_code;
                    end += 1;
                    continue;
                }
                if !in_code && heading_level(l) > 0 && heading_level(l) <= level {
                    break;
                }
                end += 1;
            }
            let words = lines[start..end]
                .iter()
                .filter(|l| !l.trim_start().starts_with("```") && !l.trim_start().starts_with('|'))
                .flat_map(|l| l.split_whitespace())
                .count();
            if words < 50 && !title.is_empty() {
                issues.push(Issue {
                    severity: Severity::Yellow,
                    kind: "short_section".to_string(),
                    line: Some(i + 1),
                    detail: format!("section `{}` has only {} words", title, words),
                });
            }
            i = end;
            continue;
        }
        i += 1;
    }
}

fn heading_level(line: &str) -> usize {
    let trimmed = line.trim_start();
    let mut n = 0;
    for c in trimmed.chars() {
        if c == '#' {
            n += 1;
        } else {
            break;
        }
    }
    if n > 0 && trimmed[n..].starts_with(' ') {
        n
    } else {
        0
    }
}

/// RED — relative `[label](file.md)` links that don't resolve against the
/// docs tree. External http/https links are skipped (out of scope), as are
/// in-page anchors (`#section`).
fn check_broken_internal_links(
    page_path: &Path,
    content: &str,
    known_files: &[PathBuf],
    issues: &mut Vec<Issue>,
) {
    let parent = page_path.parent().unwrap_or_else(|| Path::new("."));
    for (idx, line) in content.lines().enumerate() {
        let bytes = line.as_bytes();
        let mut i = 0;
        while i + 1 < bytes.len() {
            if bytes[i] == b']' && bytes[i + 1] == b'(' {
                let url_start = i + 2;
                if let Some(rel_close) = line[url_start..].find(')') {
                    let url = &line[url_start..url_start + rel_close];
                    let clean = url.split('#').next().unwrap_or(url);
                    let clean = clean.split_whitespace().next().unwrap_or(clean);
                    if !clean.is_empty()
                        && !clean.starts_with("http://")
                        && !clean.starts_with("https://")
                        && !clean.starts_with("mailto:")
                        && clean.ends_with(".md")
                    {
                        let target = parent.join(clean);
                        let canonical = target.canonicalize().ok();
                        let resolves = canonical
                            .map(|c| {
                                known_files
                                    .iter()
                                    .any(|k| k.canonicalize().ok() == Some(c.clone()))
                            })
                            .unwrap_or(false);
                        if !resolves {
                            issues.push(Issue {
                                severity: Severity::Red,
                                kind: "broken_link".to_string(),
                                line: Some(idx + 1),
                                detail: format!("link target `{}` does not exist", clean),
                            });
                        }
                    }
                    i = url_start + rel_close + 1;
                    continue;
                }
            }
            i += 1;
        }
    }
}

/// YELLOW — Sample v1.1 méthodo requires a `§4 Algorithmes` section on
/// service and controller pages. We detect those pages by filename.
fn check_methodo_alise_section_4(page_path: &Path, content: &str, issues: &mut Vec<Issue>) {
    let stem = page_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_lowercase();
    if !(stem.contains("service") || stem.contains("controller")) {
        return;
    }
    let lower = content.to_lowercase();
    let has_section_4 =
        lower.contains("§4") || lower.contains("section 4") || lower.contains("algorithm");
    if !has_section_4 {
        issues.push(Issue {
            severity: Severity::Yellow,
            kind: "missing_section_4".to_string(),
            line: None,
            detail: "Sample v1.1 méthodo requires §4 Algorithmes on service / controller pages"
                .to_string(),
        });
    }
}

//! SFD draft workflow helpers shared between the desktop chat (Tauri) and
//! the HTTP MCP transport (chat-ui, agents).
//!
//! Three operations:
//! - [`list_pages`] enumerates `.codeexplorer/docs/modules/*.md` plus drafts
//!   under `_drafts/`, so a caller knows what already exists before writing.
//! - [`write_draft`] writes Markdown into `_drafts/`, never into the live
//!   `modules/` tree. Path traversal is rejected so a misbehaving LLM cannot
//!   write outside the drafts directory.
//! - [`validate_draft`] runs the full pre-delivery linter against either the
//!   whole docs tree or a sub-path (typically `_drafts`), returning the same
//!   structured `ValidationReport` the CLI `validate-docs` command emits.
//!
//! The desktop tool implementations were inlined in `chat.rs:589-767` until
//! P1.1.b — extracting them here so a single call path serves both UIs.

use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};

use crate::validator::{self, ValidationReport};

/// Enumeration of the existing module pages and in-progress drafts under a
/// repo's `.codeexplorer/docs/` tree.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SfdPagesList {
    /// File names (e.g. `dossiers.md`) under `.codeexplorer/docs/modules/`.
    pub pages: Vec<String>,
    /// File names under `.codeexplorer/docs/_drafts/`.
    pub drafts: Vec<String>,
    /// Absolute path of the docs dir that was inspected, for context.
    pub docs_dir: PathBuf,
    /// True if the docs directory does not exist yet (caller should
    /// suggest running `code-explorer generate docs`).
    pub missing: bool,
}

/// Outcome of a successful draft write.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DraftWritten {
    /// Absolute path of the written `.md` file.
    pub path: PathBuf,
    /// Byte size of the content written.
    pub bytes: usize,
}

/// Walk `.codeexplorer/docs/modules/` and `.codeexplorer/docs/_drafts/` and return
/// the sorted list of `.md` file names. The returned struct also flags the
/// "no docs yet" case so callers can surface a friendly hint.
pub fn list_pages(repo_path: &Path) -> SfdPagesList {
    let docs_dir = repo_path.join(".codeexplorer").join("docs");
    if !docs_dir.exists() {
        return SfdPagesList {
            docs_dir,
            missing: true,
            ..Default::default()
        };
    }

    let mut pages = collect_md_filenames(&docs_dir.join("modules"));
    let mut drafts = collect_md_filenames(&docs_dir.join("_drafts"));
    pages.sort();
    drafts.sort();

    SfdPagesList {
        pages,
        drafts,
        docs_dir,
        missing: false,
    }
}

fn collect_md_filenames(dir: &Path) -> Vec<String> {
    if !dir.exists() {
        return Vec::new();
    }
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return Vec::new(),
    };
    entries
        .flatten()
        .filter_map(|e| {
            let p = e.path();
            if p.extension().and_then(|x| x.to_str()) != Some("md") {
                return None;
            }
            p.file_name()
                .and_then(|n| n.to_str())
                .map(|s| s.to_string())
        })
        .collect()
}

/// Write Markdown into `<repo>/.codeexplorer/docs/_drafts/<page>` atomically
/// (write to `.tmp` then rename, so a partial flush never leaves a half-
/// written `.md` for the validator to choke on).
///
/// `page` must be a bare file name (e.g. `dossiers.md`). Path traversal,
/// backslashes, leading slashes and empty names are rejected — drafts must
/// land inside the drafts directory, never anywhere else on disk.
pub fn write_draft(repo_path: &Path, page: &str, content: &str) -> Result<DraftWritten> {
    let trimmed = page.trim();
    if trimmed.is_empty()
        || trimmed.contains("..")
        || trimmed.contains('\\')
        || trimmed.starts_with('/')
    {
        return Err(anyhow!(
            "invalid page name '{}' (no '..', '\\', leading '/', or empty)",
            page
        ));
    }
    let drafts_dir = repo_path.join(".codeexplorer").join("docs").join("_drafts");
    std::fs::create_dir_all(&drafts_dir)
        .with_context(|| format!("create drafts dir: {}", drafts_dir.display()))?;
    let target = drafts_dir.join(trimmed);
    let tmp = target.with_extension("md.tmp");
    std::fs::write(&tmp, content).with_context(|| format!("write draft tmp: {}", tmp.display()))?;
    std::fs::rename(&tmp, &target)
        .with_context(|| format!("rename {} -> {}", tmp.display(), target.display()))?;
    Ok(DraftWritten {
        path: target,
        bytes: content.len(),
    })
}

/// Run the validator against `<repo>/.codeexplorer/docs/` (when `sub_path` is
/// empty) or against a sub-directory like `_drafts`.
///
/// `sub_path` is sandboxed: `..` and absolute paths are rejected so callers
/// can pass it straight from untrusted LLM output.
pub fn validate_draft(repo_path: &Path, sub_path: &str) -> Result<ValidationReport> {
    let docs_dir = if sub_path.is_empty() {
        repo_path.join(".codeexplorer").join("docs")
    } else {
        if sub_path.contains("..") || sub_path.starts_with('/') || sub_path.starts_with('\\') {
            return Err(anyhow!(
                "invalid path '{}' (no '..' or absolute paths)",
                sub_path
            ));
        }
        repo_path.join(".codeexplorer").join("docs").join(sub_path)
    };
    validator::validate(&docs_dir, &repo_path.display().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn list_pages_missing_docs_returns_missing_flag() {
        let tmp = TempDir::new().unwrap();
        let result = list_pages(tmp.path());
        assert!(result.missing);
        assert!(result.pages.is_empty());
        assert!(result.drafts.is_empty());
    }

    #[test]
    fn list_pages_returns_sorted_md_names() {
        let tmp = TempDir::new().unwrap();
        let modules = tmp.path().join(".codeexplorer/docs/modules");
        let drafts = tmp.path().join(".codeexplorer/docs/_drafts");
        std::fs::create_dir_all(&modules).unwrap();
        std::fs::create_dir_all(&drafts).unwrap();
        std::fs::write(modules.join("z.md"), "").unwrap();
        std::fs::write(modules.join("a.md"), "").unwrap();
        std::fs::write(modules.join("ignore.txt"), "").unwrap();
        std::fs::write(drafts.join("b.md"), "").unwrap();
        let result = list_pages(tmp.path());
        assert!(!result.missing);
        assert_eq!(result.pages, vec!["a.md", "z.md"]);
        assert_eq!(result.drafts, vec!["b.md"]);
    }

    #[test]
    fn write_draft_rejects_path_traversal() {
        let tmp = TempDir::new().unwrap();
        for bad in ["../escape.md", "../../etc/passwd", "/abs.md", "  ", ""] {
            let err = write_draft(tmp.path(), bad, "x").unwrap_err();
            assert!(
                err.to_string().contains("invalid page name"),
                "{} should be rejected, got {}",
                bad,
                err
            );
        }
    }

    #[test]
    fn write_draft_writes_atomically_into_drafts_dir() {
        let tmp = TempDir::new().unwrap();
        let written = write_draft(tmp.path(), "dossiers.md", "# Dossiers\n").unwrap();
        assert_eq!(written.bytes, 11);
        assert!(written.path.ends_with(".codeexplorer/docs/_drafts/dossiers.md"));
        let body = std::fs::read_to_string(&written.path).unwrap();
        assert_eq!(body, "# Dossiers\n");
        let leftover = written.path.with_extension("md.tmp");
        assert!(!leftover.exists(), "tmp file should have been renamed away");
    }

    #[test]
    fn validate_draft_rejects_unsafe_sub_path() {
        let tmp = TempDir::new().unwrap();
        let err = validate_draft(tmp.path(), "../etc").unwrap_err();
        assert!(err.to_string().contains("invalid path"));
    }
}

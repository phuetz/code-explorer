//! Worktree versus HEAD, used when the indexed commit already equals HEAD.
//!
//! What `status` compared before this module (see `commands/status.rs`): the
//! string from `git::current_commit` (`git rev-parse HEAD`) against
//! `meta.last_commit`. Equality printed `Index is up-to-date.` The manifest
//! SHA-256 in `code-explorer-ingest` is not consulted on that path.
//!
//! Detection added here:
//!
//! * Tracked files. `git diff --name-only -z HEAD`: Git applies `core.autocrlf`,
//!   `.gitattributes` and clean/smudge filters itself, so a checkout whose line
//!   endings Git rewrote is clean, and same bytes with a new mtime stay clean.
//!   Deleted, modified and unmerged paths are differences. Submodules compare
//!   the recorded commit only (`--ignore-submodules=dirty`).
//! * Untracked, non-ignored files. `git ls-files -z --others --exclude-standard`,
//!   which applies `.gitignore`, `.git/info/exclude`, and the global excludes
//!   file. Those paths have no HEAD blob, so presence is the difference.
//! * `.codeexplorer/` is omitted. `analyze` creates it, and counting it would
//!   mark every fresh index dirty.
//!
//! These git invocations are read-only and run with `GIT_OPTIONAL_LOCKS=0`, so
//! they never write the index.

use std::path::Path;
use std::process::Command;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum HeadFreshness {
    UpToDate,
    Unknown,
    Dirty(Vec<String>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ExistingIndex {
    /// Keep the index. Either nothing differs, or git could not be asked and
    /// `--include-dirty` was not passed (historical shortcut).
    Keep,
    /// Commit matches, but the worktree differs. Do not re-index unless asked.
    KeepButWarn { count: usize },
    /// `--include-dirty`: re-index so the working tree is what gets stored.
    /// `count == 0` means the worktree could not be compared to HEAD.
    Reindex { count: usize },
}

pub(crate) fn existing_index_action(include_dirty: bool, dirty: Option<&[String]>) -> ExistingIndex {
    match (include_dirty, dirty) {
        (_, Some([])) => ExistingIndex::Keep,
        (false, None) => ExistingIndex::Keep,
        (true, None) => ExistingIndex::Reindex { count: 0 },
        (true, Some(paths)) => ExistingIndex::Reindex { count: paths.len() },
        (false, Some(paths)) => ExistingIndex::KeepButWarn { count: paths.len() },
    }
}

pub(crate) fn freshness_when_commit_matches(repo: &Path) -> HeadFreshness {
    match paths_differing_from_head(repo) {
        Some(paths) if paths.is_empty() => HeadFreshness::UpToDate,
        Some(paths) => HeadFreshness::Dirty(paths),
        None => HeadFreshness::Unknown,
    }
}

pub(crate) fn freshness_lines(state: &HeadFreshness) -> Vec<String> {
    match state {
        HeadFreshness::UpToDate => vec!["  Index is up-to-date.".to_string()],
        HeadFreshness::Unknown => vec![
            "  WARNING: worktree could not be compared to HEAD; not claiming the index is up-to-date."
                .to_string(),
        ],
        HeadFreshness::Dirty(paths) => {
            let mut lines = vec![
                "  WARNING: Index matches the commit but the working tree differs.".to_string(),
                format!("    {} path(s) differ:", paths.len()),
            ];
            for path in paths.iter().take(20) {
                lines.push(format!("    {path}"));
            }
            if paths.len() > 20 {
                lines.push(format!("    ... {} more", paths.len() - 20));
            }
            lines.push(
                "    Run `code-explorer analyze --include-dirty` to include these changes."
                    .to_string(),
            );
            lines
        }
    }
}

pub(crate) fn paths_differing_from_head(repo: &Path) -> Option<Vec<String>> {
    // Ask Git itself, so that core.autocrlf, .gitattributes and clean/smudge
    // filters are applied exactly as Git applies them. Comparing raw worktree
    // bytes with the raw HEAD blob reported a clean checkout as dirty as soon as
    // Git had rewritten line endings. `git diff` refreshes the index in memory
    // only (GIT_OPTIONAL_LOCKS=0 stops it writing it back), so a touched file
    // with identical content is not a difference. Submodule worktrees are not
    // inspected: only the recorded commit is compared.
    let tracked = parse_z_paths(&git_bytes(
        repo,
        &[
            "-c",
            "core.quotepath=off",
            "diff",
            "--name-only",
            "-z",
            "--no-renames",
            "--no-ext-diff",
            "--ignore-submodules=dirty",
            "HEAD",
            "--",
        ],
    )?)?;
    let untracked = parse_z_paths(&git_bytes(
        repo,
        &["ls-files", "-z", "--others", "--exclude-standard"],
    )?)?;
    let mut dirty: Vec<String> = tracked
        .into_iter()
        .chain(untracked)
        .filter(|path| !is_storage(path))
        .collect();
    dirty.sort();
    dirty.dedup();
    Some(dirty)
}

fn is_storage(path: &str) -> bool {
    let path = path.trim_end_matches('/');
    path == ".codeexplorer" || path.starts_with(".codeexplorer/")
}

fn git_bytes(repo: &Path, args: &[&str]) -> Option<Vec<u8>> {
    let output = Command::new("git")
        .args(args)
        .current_dir(repo)
        .env("GIT_OPTIONAL_LOCKS", "0")
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    Some(output.stdout)
}

fn parse_z_paths(raw: &[u8]) -> Option<Vec<String>> {
    let mut out = Vec::new();
    for entry in raw.split(|byte| *byte == 0) {
        if entry.is_empty() {
            continue;
        }
        out.push(std::str::from_utf8(entry).ok()?.to_string());
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    #[test]
    fn parses_nul_separated_paths() {
        assert!(parse_z_paths(b".codeexplorer/meta.json\x00b.rs\x00")
            .unwrap()
            .iter()
            .any(|path| path == "b.rs"));
    }

    #[test]
    fn include_dirty_reindexes_only_when_the_worktree_differs_or_cannot_be_read() {
        let one = vec!["a.rs".to_string()];
        assert_eq!(existing_index_action(false, Some(&[])), ExistingIndex::Keep);
        assert_eq!(existing_index_action(true, Some(&[])), ExistingIndex::Keep);
        assert_eq!(existing_index_action(false, None), ExistingIndex::Keep);
        assert_eq!(
            existing_index_action(true, None),
            ExistingIndex::Reindex { count: 0 }
        );
        assert_eq!(
            existing_index_action(false, Some(&one)),
            ExistingIndex::KeepButWarn { count: 1 }
        );
        assert_eq!(
            existing_index_action(true, Some(&one)),
            ExistingIndex::Reindex { count: 1 }
        );
    }

    #[test]
    fn up_to_date_line_is_the_historical_sentence() {
        let lines = freshness_lines(&HeadFreshness::UpToDate);
        assert_eq!(lines, vec!["  Index is up-to-date.".to_string()]);
    }

    fn git_available() -> bool {
        Command::new("git")
            .arg("--version")
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false)
    }

    struct Scratch(std::path::PathBuf);

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn scratch(name: &str) -> Scratch {
        let dir = std::env::temp_dir().join(format!(
            "ce-fraicheur-1004-{name}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|elapsed| elapsed.as_nanos())
                .unwrap_or(0)
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        Scratch(dir)
    }

    fn git(repo: &Path, args: &[&str]) {
        let output = Command::new("git")
            .args(args)
            .current_dir(repo)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "git {args:?} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    fn init_repo(repo: &Path) {
        git(repo, &["init", "-q"]);
        git(repo, &["config", "user.email", "qa@example.invalid"]);
        git(repo, &["config", "user.name", "QA"]);
        git(repo, &["config", "core.autocrlf", "false"]);
        std::fs::write(repo.join("a.rs"), "fn a() {}\n").unwrap();
        git(repo, &["add", "a.rs"]);
        git(repo, &["commit", "-q", "-m", "baseline"]);
    }

    fn assert_up_to_date(repo: &Path) {
        let state = freshness_when_commit_matches(repo);
        let lines = freshness_lines(&state);
        assert_eq!(state, HeadFreshness::UpToDate, "{lines:?}");
        assert_eq!(lines, vec!["  Index is up-to-date.".to_string()]);
    }

    fn assert_warns_for(repo: &Path, path: &str) {
        let state = freshness_when_commit_matches(repo);
        let lines = freshness_lines(&state);
        match &state {
            HeadFreshness::Dirty(paths) => assert!(paths.iter().any(|item| item == path), "{paths:?}"),
            other => panic!("expected a warning for {path}, got {other:?}"),
        }
        let text = lines.join("\n");
        assert!(text.contains("WARNING"), "{text}");
        assert!(text.contains("working tree differs"), "{text}");
        assert!(text.contains(path), "{text}");
        assert!(text.contains("--include-dirty"), "{text}");
        assert!(!text.contains("Index is up-to-date."), "{text}");
    }

    #[test]
    fn clean_tree_is_up_to_date() {
        if !git_available() {
            eprintln!("skipping: git is not installed");
            return;
        }
        let dir = scratch("clean");
        init_repo(&dir.0);
        std::fs::create_dir_all(dir.0.join(".codeexplorer")).unwrap();
        std::fs::write(dir.0.join(".codeexplorer/meta.json"), "{}\n").unwrap();
        assert_up_to_date(&dir.0);
    }

    #[test]
    fn modified_tracked_file_warns() {
        if !git_available() {
            eprintln!("skipping: git is not installed");
            return;
        }
        let dir = scratch("tracked");
        init_repo(&dir.0);
        std::fs::write(dir.0.join("a.rs"), "fn a() { let _ = 1; }\n").unwrap();
        assert_warns_for(&dir.0, "a.rs");
    }

    #[test]
    fn untracked_non_ignored_file_warns() {
        if !git_available() {
            eprintln!("skipping: git is not installed");
            return;
        }
        let dir = scratch("untracked");
        init_repo(&dir.0);
        std::fs::write(dir.0.join("extra.rs"), "fn extra() {}\n").unwrap();
        assert_warns_for(&dir.0, "extra.rs");
    }

    #[test]
    fn ignored_file_does_not_warn() {
        if !git_available() {
            eprintln!("skipping: git is not installed");
            return;
        }
        let dir = scratch("ignored");
        init_repo(&dir.0);
        std::fs::write(dir.0.join(".gitignore"), "ignored.rs\n").unwrap();
        git(&dir.0, &["add", ".gitignore"]);
        git(&dir.0, &["commit", "-q", "-m", "ignore"]);
        std::fs::write(dir.0.join("ignored.rs"), "fn ignored() {}\n").unwrap();
        assert_up_to_date(&dir.0);
    }

    #[test]
    fn same_bytes_with_a_new_mtime_stay_up_to_date() {
        if !git_available() {
            eprintln!("skipping: git is not installed");
            return;
        }
        let dir = scratch("mtime");
        init_repo(&dir.0);
        let path = dir.0.join("a.rs");
        let bytes = std::fs::read(&path).unwrap();
        std::fs::write(&path, &bytes).unwrap();
        assert_up_to_date(&dir.0);
    }

    #[test]
    fn autocrlf_checkout_is_not_reported_as_dirty() {
        if !git_available() {
            eprintln!("skipping: git is not installed");
            return;
        }
        let dir = scratch("autocrlf");
        init_repo(&dir.0);
        git(&dir.0, &["config", "core.autocrlf", "true"]);
        std::fs::remove_file(dir.0.join("a.rs")).unwrap();
        git(&dir.0, &["checkout", "--", "a.rs"]);
        // Git rewrote the line endings: the worktree bytes differ from the blob,
        // yet `git diff --quiet HEAD` says the tree is clean.
        let bytes = std::fs::read(dir.0.join("a.rs")).unwrap();
        assert!(bytes.windows(2).any(|w| w == b"\r\n"), "expected CRLF, got {bytes:?}");
        assert_up_to_date(&dir.0);
        // A real edit is still seen.
        std::fs::write(dir.0.join("a.rs"), "fn a() { let _ = 2; }\r\n").unwrap();
        assert_warns_for(&dir.0, "a.rs");
    }
}

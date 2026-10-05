//! Worktree versus HEAD, used when the indexed commit already equals HEAD.
//!
//! What `status` compared before this module (see `commands/status.rs`): the
//! string from `git::current_commit` (`git rev-parse HEAD`) against
//! `meta.last_commit`. Equality printed `Index is up-to-date.` The manifest
//! SHA-256 in `code-explorer-ingest` is not consulted on that path.
//!
//! Detection added here:
//!
//! * Tracked files. `git ls-tree -r -z HEAD` and `git ls-files -z -s`. For a
//!   regular file or symlink present on both sides, SHA-256 of the worktree
//!   bytes (symlink: the link target, not the followed file) compared with
//!   SHA-256 of the raw HEAD blob (`git cat-file --batch`). A missing side, a
//!   hash mismatch, or an unmerged index stage is a difference. Gitlinks
//!   (mode `160000`) compare the recorded commit id only; the submodule
//!   worktree is not hashed.
//! * Untracked, non-ignored files. `git ls-files -z --others --exclude-standard`,
//!   which applies `.gitignore`, `.git/info/exclude`, and the global excludes
//!   file. Those paths have no HEAD blob, so presence is the difference.
//! * `.codeexplorer/` is omitted. `analyze` creates it, and counting it would
//!   mark every fresh index dirty.
//!
//! These git invocations are read-only (`ls-tree`, `ls-files`, `cat-file`).
//! They do not refresh the index.

use std::collections::{BTreeSet, HashMap};
use std::io::{BufRead, BufReader, Read, Write};
use std::path::Path;
use std::process::{Command, Stdio};

use sha2::{Digest, Sha256};

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
    let head = parse_ls_tree_z(&git_bytes(repo, &["ls-tree", "-r", "-z", "HEAD"])?)?;
    let index = parse_ls_files_s_z(&git_bytes(repo, &["ls-files", "-z", "-s"])?)?;
    let untracked = parse_z_paths(&git_bytes(
        repo,
        &["ls-files", "-z", "--others", "--exclude-standard"],
    )?)?;

    let mut head_map: HashMap<String, (u32, String)> = HashMap::new();
    for (path, mode, oid) in head {
        head_map.insert(path, (mode, oid));
    }

    let mut index_map: HashMap<String, (u32, String)> = HashMap::new();
    let mut conflicted: BTreeSet<String> = BTreeSet::new();
    for (path, mode, oid, stage) in index {
        if is_storage(&path) {
            continue;
        }
        if stage != 0 {
            conflicted.insert(path);
            continue;
        }
        index_map.insert(path, (mode, oid));
    }

    let mut names: BTreeSet<String> = BTreeSet::new();
    names.extend(head_map.keys().cloned());
    names.extend(index_map.keys().cloned());

    let mut dirty: Vec<String> = conflicted.iter().cloned().collect();
    // path, worktree sha256, HEAD blob oid
    let mut need: Vec<(String, String, String)> = Vec::new();

    for path in names {
        if is_storage(&path) || conflicted.contains(&path) {
            continue;
        }
        let head_e = head_map.get(&path);
        let index_e = index_map.get(&path);
        let mode = index_e
            .map(|entry| entry.0)
            .or_else(|| head_e.map(|entry| entry.0))
            .unwrap_or(0);
        if mode == 0o160000 {
            match (
                head_e.map(|entry| entry.1.as_str()),
                index_e.map(|entry| entry.1.as_str()),
            ) {
                (Some(head_oid), Some(index_oid)) if head_oid == index_oid => {}
                _ => dirty.push(path),
            }
            continue;
        }
        match (worktree_hash(&repo.join(&path)), head_e) {
            (Some(work), Some((_, oid))) => need.push((path, work, oid.clone())),
            _ => dirty.push(path),
        }
    }

    let oids: Vec<String> = need.iter().map(|(_, _, oid)| oid.clone()).collect();
    let head_hashes = blob_sha256s(repo, &oids)?;
    if head_hashes.len() != need.len() {
        return None;
    }
    for ((path, work, _), head_hash) in need.into_iter().zip(head_hashes) {
        match head_hash {
            Some(head) if head == work => {}
            _ => dirty.push(path),
        }
    }

    for path in untracked {
        if is_storage(&path) {
            continue;
        }
        dirty.push(path);
    }

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
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    Some(output.stdout)
}

fn parse_ls_tree_z(raw: &[u8]) -> Option<Vec<(String, u32, String)>> {
    let mut out = Vec::new();
    for entry in raw.split(|byte| *byte == 0) {
        if entry.is_empty() {
            continue;
        }
        let entry = std::str::from_utf8(entry).ok()?;
        let (meta, path) = entry.split_once('\t')?;
        let mut parts = meta.split_whitespace();
        let mode = u32::from_str_radix(parts.next()?, 8).ok()?;
        let _kind = parts.next()?;
        let oid = parts.next()?.to_string();
        if parts.next().is_some() {
            return None;
        }
        out.push((path.to_string(), mode, oid));
    }
    Some(out)
}

fn parse_ls_files_s_z(raw: &[u8]) -> Option<Vec<(String, u32, String, u32)>> {
    let mut out = Vec::new();
    for entry in raw.split(|byte| *byte == 0) {
        if entry.is_empty() {
            continue;
        }
        let entry = std::str::from_utf8(entry).ok()?;
        let (meta, path) = entry.split_once('\t')?;
        let mut parts = meta.split_whitespace();
        let mode = u32::from_str_radix(parts.next()?, 8).ok()?;
        let oid = parts.next()?.to_string();
        let stage: u32 = parts.next()?.parse().ok()?;
        if parts.next().is_some() {
            return None;
        }
        out.push((path.to_string(), mode, oid, stage));
    }
    Some(out)
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

fn worktree_hash(path: &Path) -> Option<String> {
    let meta = std::fs::symlink_metadata(path).ok()?;
    let file_type = meta.file_type();
    if file_type.is_symlink() {
        let target = std::fs::read_link(path).ok()?;
        #[cfg(unix)]
        {
            use std::os::unix::ffi::OsStrExt;
            return Some(sha256_bytes(target.as_os_str().as_bytes()));
        }
        #[cfg(not(unix))]
        {
            return Some(sha256_bytes(target.to_string_lossy().as_bytes()));
        }
    }
    if !file_type.is_file() {
        return None;
    }
    let mut file = std::fs::File::open(path).ok()?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 8192];
    loop {
        let n = file.read(&mut buf).ok()?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Some(format!("{:x}", hasher.finalize()))
}

fn sha256_bytes(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

fn blob_sha256s(repo: &Path, oids: &[String]) -> Option<Vec<Option<String>>> {
    if oids.is_empty() {
        return Some(Vec::new());
    }
    let mut child = Command::new("git")
        .args(["cat-file", "--batch"])
        .current_dir(repo)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let mut stdin = match child.stdin.take() {
        Some(stdin) => stdin,
        None => {
            let _ = child.wait();
            return None;
        }
    };
    let stdout = match child.stdout.take() {
        Some(stdout) => stdout,
        None => {
            drop(stdin);
            let _ = child.wait();
            return None;
        }
    };
    let to_send = oids.to_vec();
    let writer = std::thread::spawn(move || {
        for oid in to_send {
            if writeln!(stdin, "{oid}").is_err() {
                break;
            }
        }
    });
    let result = read_batch(stdout, oids.len());
    let _ = writer.join();
    let status_ok = child
        .wait()
        .ok()
        .map(|status| status.success())
        .unwrap_or(false);
    let hashes = result?;
    if !status_ok {
        return None;
    }
    Some(hashes)
}

fn read_batch(stdout: std::process::ChildStdout, count: usize) -> Option<Vec<Option<String>>> {
    let mut reader = BufReader::new(stdout);
    let mut hashes = Vec::with_capacity(count);
    for _ in 0..count {
        let mut header = String::new();
        let got = reader.read_line(&mut header).ok()?;
        if got == 0 {
            return None;
        }
        let header = header.trim_end_matches(['\n', '\r']);
        if header.ends_with(" missing") {
            hashes.push(None);
            continue;
        }
        let size: usize = header.split_whitespace().last()?.parse().ok()?;
        let mut hasher = Sha256::new();
        let mut left = size;
        let mut buf = [0u8; 8192];
        while left > 0 {
            let chunk = left.min(buf.len());
            reader.read_exact(&mut buf[..chunk]).ok()?;
            hasher.update(&buf[..chunk]);
            left -= chunk;
        }
        let mut newline = [0u8; 1];
        reader.read_exact(&mut newline).ok()?;
        if newline[0] != b'\n' {
            return None;
        }
        hashes.push(Some(format!("{:x}", hasher.finalize())));
    }
    Some(hashes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    #[test]
    fn parses_ls_tree_and_index_bytes() {
        let tree = b"100644 blob ca05282d78fc4edbdc73e5ec2ce1fe1e16725e42\ta.rs\x00";
        let index = b"100644 ca05282d78fc4edbdc73e5ec2ce1fe1e16725e42 0\ta.rs\x00";
        let parsed = parse_ls_tree_z(tree).unwrap();
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].0, "a.rs");
        assert_eq!(parsed[0].1, 0o100644);
        assert_eq!(parsed[0].2, "ca05282d78fc4edbdc73e5ec2ce1fe1e16725e42");
        let indexed = parse_ls_files_s_z(index).unwrap();
        assert_eq!(indexed[0].3, 0);
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
}

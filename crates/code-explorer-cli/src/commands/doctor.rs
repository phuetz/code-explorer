//! The `doctor` command: one pass over an index, telling what is wrong and
//! which command repairs it.
//!
//! `status` answers "is this directory indexed?". `doctor` answers the
//! question that actually costs time: "the tools say my repository is not
//! there / is out of date / is missing half my files — why, and what do I
//! run?". Every check produces a machine-readable verdict, so the same
//! diagnosis drives the text report, `--json`, and the exit code.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use code_explorer_core::config::exclusions::ExclusionRules;
use code_explorer_core::config::languages::SupportedLanguage;
use code_explorer_ingest::phases::docs::DocKind;
use code_explorer_core::storage::{git, repo_manager};
use serde::Serialize;

// ─── Verdict model ───────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Level {
    Ok,
    Warn,
    Error,
}

impl Level {
    fn label(self) -> &'static str {
        match self {
            Level::Ok => "OK   ",
            Level::Warn => "WARN ",
            Level::Error => "ERROR",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Check {
    /// Stable identifier, safe to grep or match on in a script.
    pub id: &'static str,
    pub level: Level,
    pub summary: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub details: Vec<String>,
    /// Shell command that repairs this check, when one exists.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fix: Option<String>,
}

impl Check {
    fn new(id: &'static str, level: Level, summary: impl Into<String>) -> Self {
        Self {
            id,
            level,
            summary: summary.into(),
            details: Vec::new(),
            fix: None,
        }
    }

    fn detail(mut self, line: impl Into<String>) -> Self {
        self.details.push(line.into());
        self
    }

    fn fix(mut self, command: impl Into<String>) -> Self {
        self.fix = Some(command.into());
        self
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DoctorReport {
    /// Path exactly as the user typed it.
    pub requested_path: String,
    /// Canonical path, when it could be resolved.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub canonical_path: Option<String>,
    pub checks: Vec<Check>,
    /// Worst level across all checks.
    pub status: Level,
}

impl DoctorReport {
    fn finish(requested_path: String, canonical_path: Option<String>, checks: Vec<Check>) -> Self {
        let status = checks.iter().map(|c| c.level).max().unwrap_or(Level::Ok);
        Self {
            requested_path,
            canonical_path,
            checks,
            status,
        }
    }
}

// ─── Diagnosis ───────────────────────────────────────────────────────────

/// Run every check against `requested`. Never fails: an unusable repository
/// is a report with errors in it, not an `Err`.
pub fn diagnose(requested: &str, registry_path: &Path) -> DoctorReport {
    let requested_path = Path::new(requested);
    let canonical = requested_path.canonicalize().ok();

    let mut checks = Vec::new();

    // 1. The path itself.
    let repo_path = match &canonical {
        None => {
            checks.push(
                Check::new(
                    "path",
                    Level::Error,
                    format!("'{requested}' does not exist on this machine"),
                )
                .fix("check the path, then run `code-explorer analyze <path>`"),
            );
            return DoctorReport::finish(requested.to_string(), None, checks);
        }
        Some(p) if !p.is_dir() => {
            checks.push(Check::new(
                "path",
                Level::Error,
                format!("'{}' is not a directory", p.display()),
            ));
            return DoctorReport::finish(
                requested.to_string(),
                Some(p.display().to_string()),
                checks,
            );
        }
        Some(p) => p.clone(),
    };
    checks.push(path_check(requested, &repo_path));

    // 2/3. The index and its layout version.
    let storage = repo_manager::get_storage_paths(&repo_path);
    let meta = repo_manager::load_meta(&storage.storage_path).ok().flatten();
    let (index_check, index_usable) = index_check(&repo_path, &storage.storage_path, meta.as_ref());
    checks.push(index_check);
    checks.push(schema_check(meta.as_ref()));

    // 4. Registry coherence — the check that explains "Repository not found".
    checks.push(registry_check(&repo_path, &storage.storage_path, registry_path));

    // 5. Coverage: what the repository holds versus what the index holds.
    checks.push(coverage_check(&repo_path, meta.as_ref()));

    // 6. Bulk: directories git does not ignore that would drown the index.
    checks.push(bulk_check(&repo_path));

    // 7. Freshness against git.
    checks.push(freshness_check(&repo_path, meta.as_ref(), index_usable));

    DoctorReport::finish(
        requested.to_string(),
        Some(repo_path.display().to_string()),
        checks,
    )
}

fn path_check(requested: &str, repo_path: &Path) -> Check {
    let mut check = Check::new("path", Level::Ok, format!("{}", repo_path.display()));
    if Path::new(requested) != repo_path {
        check = check.detail(format!("requested as '{requested}'"));
    }
    if !git::is_git_repo(repo_path) {
        return check.detail("not a git working tree (git-based checks are skipped)");
    }
    match (git::git_dir(repo_path), git::git_common_dir(repo_path)) {
        (Some(dir), Some(common)) => {
            let dir_abs = absolutize(repo_path, &dir);
            let common_abs = absolutize(repo_path, &common);
            if dir_abs != common_abs {
                check = check
                    .detail("linked git worktree")
                    .detail(format!("shared git dir: {}", common_abs.display()));
            } else {
                check = check.detail("git checkout");
            }
        }
        _ => check = check.detail("git checkout"),
    }
    if let Some(branch) = git::current_branch(repo_path) {
        check = check.detail(format!("branch: {branch}"));
    }
    check
}

/// Resolve a path git printed (possibly relative to the working tree).
fn absolutize(base: &Path, raw: &str) -> PathBuf {
    let p = PathBuf::from(raw);
    let joined = if p.is_absolute() { p } else { base.join(p) };
    joined.canonicalize().unwrap_or(joined)
}

fn index_check(repo_path: &Path, storage: &Path, meta: Option<&repo_manager::RepoMeta>) -> (Check, bool) {
    let analyze = format!("code-explorer analyze {}", repo_path.display());
    if !storage.exists() {
        return (
            Check::new(
                "index",
                Level::Error,
                format!("no index directory at {}", storage.display()),
            )
            .fix(analyze),
            false,
        );
    }
    let meta_path = storage.join("meta.json");
    if meta.is_none() {
        return (
            Check::new(
                "index",
                Level::Error,
                format!("{} is missing or unreadable", meta_path.display()),
            )
            .fix(format!("{analyze} --force")),
            false,
        );
    }
    let snapshot = storage.join("graph.bin");
    let snapshot_len = std::fs::metadata(&snapshot).map(|m| m.len()).ok();
    match snapshot_len {
        None => (
            Check::new(
                "index",
                Level::Error,
                format!("graph snapshot missing at {}", snapshot.display()),
            )
            .fix(format!("{analyze} --force")),
            false,
        ),
        Some(0) => (
            Check::new("index", Level::Error, "graph snapshot is empty").fix(format!("{analyze} --force")),
            false,
        ),
        Some(len) => match code_explorer_db::snapshot::load_snapshot(&snapshot) {
            Err(e) => (
                Check::new("index", Level::Error, format!("graph snapshot is unreadable: {e}"))
                    .fix(format!("{analyze} --force")),
                false,
            ),
            Ok(graph) => (
                Check::new(
                    "index",
                    Level::Ok,
                    format!(
                        "{} nodes, {} edges ({:.1} MB)",
                        graph.node_count(),
                        graph.relationship_count(),
                        len as f64 / (1024.0 * 1024.0)
                    ),
                )
                .detail(format!("storage: {}", storage.display())),
                true,
            ),
        },
    }
}

fn schema_check(meta: Option<&repo_manager::RepoMeta>) -> Check {
    let current = repo_manager::INDEX_SCHEMA_VERSION;
    let Some(meta) = meta else {
        return Check::new(
            "schema",
            Level::Warn,
            "no index metadata to read (see the `index` check)",
        );
    };
    match meta.schema_version {
        Some(v) if v == current => Check::new("schema", Level::Ok, format!("index schema {v} (current)")),
        Some(v) if v < current => Check::new(
            "schema",
            Level::Warn,
            format!("index schema {v}, this build writes {current}"),
        )
        .fix("code-explorer analyze <path> --force"),
        Some(v) => Check::new(
            "schema",
            Level::Warn,
            format!("index schema {v} is newer than this build ({current})"),
        )
        .detail("upgrade code-explorer, or re-index with this build"),
        None => Check::new(
            "schema",
            Level::Warn,
            format!("index carries no schema stamp (written before schema {current})"),
        )
        .fix("code-explorer analyze <path> --force"),
    }
}

fn registry_check(repo_path: &Path, storage: &Path, registry_path: &Path) -> Check {
    let entries = match repo_manager::read_registry_from(registry_path) {
        Ok(e) => e,
        Err(e) => {
            return Check::new("registry", Level::Error, format!("registry unreadable: {e}"))
                .detail(format!("file: {}", registry_path.display()))
                .fix(format!("code-explorer analyze {}", repo_path.display()));
        }
    };

    let canonical = repo_path.to_path_buf();
    let matching: Vec<_> = entries
        .iter()
        .filter(|e| {
            Path::new(&e.path)
                .canonicalize()
                .map(|p| p == canonical)
                .unwrap_or_else(|_| Path::new(&e.path) == canonical)
        })
        .collect();

    if matching.is_empty() {
        // The repository may still be usable — the MCP server falls back to the
        // index on disk — but analytics and `list_repos` will not show it.
        return Check::new(
            "registry",
            Level::Warn,
            format!("not listed in {}", registry_path.display()),
        )
        .detail(format!("{} repositories registered", entries.len()))
        .detail("MCP tools fall back to the on-disk index, but `list_repos` will not show it")
        .fix(format!("code-explorer analyze {}", repo_path.display()));
    }

    if matching.len() > 1 {
        return Check::new(
            "registry",
            Level::Warn,
            format!("{} registry entries point at this repository", matching.len()),
        )
        .detail("a stale spelling of the path is still registered")
        .fix(format!("code-explorer analyze {}", repo_path.display()));
    }

    let entry = matching[0];
    // Compare through the filesystem: on macOS the temp dir is registered as
    // `/var/...` while the resolved repo path is `/private/var/...`.
    let registered_storage_raw = Path::new(&entry.storage_path);
    let registered_storage = registered_storage_raw
        .canonicalize()
        .unwrap_or_else(|_| registered_storage_raw.to_path_buf());
    let storage_canonical = storage.canonicalize().unwrap_or_else(|_| storage.to_path_buf());
    if registered_storage != storage_canonical {
        return Check::new(
            "registry",
            Level::Error,
            "registry points at a different index directory",
        )
        .detail(format!("registered: {}", registered_storage.display()))
        .detail(format!("on disk:    {}", storage.display()))
        .fix(format!("code-explorer analyze {} --force", repo_path.display()));
    }

    Check::new("registry", Level::Ok, format!("registered as '{}'", entry.name))
        .detail(format!("indexed at {}", entry.indexed_at))
}

/// Count files on disk by extension, using the same exclusions as the indexer,
/// and compare with what the index actually holds — code against code, prose
/// against prose. A prose repository whose documents were never indexed is the
/// single most misleading state a user can be in ("search finds nothing"), so
/// it gets its own verdict and its own fix.
fn coverage_check(repo_path: &Path, meta: Option<&repo_manager::RepoMeta>) -> Check {
    let counts = count_repo_files(repo_path);
    let stats = meta.and_then(|m| m.stats.as_ref());
    let indexed_code = stats.and_then(|s| s.files);
    let indexed_docs = stats.and_then(|s| s.documents);

    let code_on_disk: usize = counts.code.values().sum();
    let prose_on_disk: usize = counts.prose.values().sum();
    let other_on_disk: usize = counts.other.values().sum();
    let total = code_on_disk + prose_on_disk + other_on_disk;
    if total == 0 {
        return Check::new("coverage", Level::Warn, "no files found under this path");
    }

    let analyze = format!("code-explorer analyze {}", repo_path.display());
    let mut check = match (indexed_code, indexed_docs) {
        // Prose on disk, none of it indexed: the case that makes `search_code`
        // useless on a book or a documentation repository.
        (_, docs) if prose_on_disk > 0 && docs.unwrap_or(0) == 0 => Check::new(
            "coverage",
            Level::Warn,
            format!("{prose_on_disk} prose files on disk, none indexed"),
        )
        .detail("headings and internal links are invisible to `context` / `search_code`")
        .fix(format!("{analyze} --force --include-docs")),
        (Some(n), _) if n + n / 10 < code_on_disk => Check::new(
            "coverage",
            Level::Warn,
            format!("{n} code files indexed for {code_on_disk} on disk"),
        )
        .fix(format!("{analyze} --force")),
        (Some(n), docs) => Check::new(
            "coverage",
            Level::Ok,
            format!(
                "{n} code files and {} prose documents indexed",
                docs.unwrap_or(0)
            ),
        ),
        (None, _) => Check::new(
            "coverage",
            Level::Warn,
            format!("{code_on_disk} code files on disk, index reports no file count"),
        ),
    };

    check = check.detail(format!(
        "{total} files walked: {code_on_disk} code, {prose_on_disk} prose, {other_on_disk} other"
    ));
    for (ext, n) in top_extensions(&counts.code, 4) {
        check = check.detail(format!("  code   {ext:<10} {n}"));
    }
    for (ext, n) in top_extensions(&counts.prose, 4) {
        check = check.detail(format!("  prose  {ext:<10} {n}"));
    }
    for (ext, n) in top_extensions(&counts.other, 4) {
        check = check.detail(format!("  other  {ext:<10} {n}"));
    }
    check
}

#[derive(Default)]
struct FileCounts {
    /// Files a language provider can parse.
    code: BTreeMap<String, usize>,
    /// Markdown / text / reStructuredText.
    prose: BTreeMap<String, usize>,
    /// Everything else — never indexed by design.
    other: BTreeMap<String, usize>,
}

fn count_repo_files(repo_path: &Path) -> FileCounts {
    let rules = ExclusionRules::for_repo(repo_path);
    let mut counts = FileCounts::default();

    for entry in code_explorer_ingest::phases::structure::build_walker(repo_path, &rules).flatten()
    {
        if !entry.file_type().is_some_and(|ft| ft.is_file()) {
            continue;
        }
        let abs = entry.path();
        let rel = abs
            .strip_prefix(repo_path)
            .unwrap_or(abs)
            .to_string_lossy()
            .replace('\\', "/");
        if rules.is_excluded(&rel) {
            continue;
        }
        let ext = abs
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| format!(".{}", e.to_lowercase()))
            .unwrap_or_else(|| "(none)".to_string());
        let bucket = if SupportedLanguage::from_filename(&rel).is_some() {
            &mut counts.code
        } else if DocKind::from_path(&rel).is_some() {
            &mut counts.prose
        } else {
            &mut counts.other
        };
        *bucket.entry(ext).or_insert(0) += 1;
    }
    counts
}

// ─── Bulk directories ────────────────────────────────────────────────────

/// A directory holding at least this many files is worth naming.
const BULK_FILE_THRESHOLD: usize = 500;
/// ...and so is one weighing at least this much, however few files it holds.
const BULK_BYTE_THRESHOLD: u64 = 50 * 1024 * 1024;

/// One oversized directory, and whether anything currently stops it.
#[derive(Debug, Clone)]
struct BulkDir {
    tally: code_explorer_ingest::phases::structure::DirTally,
    /// The exclusion pattern that already drops it, when there is one.
    excluded_by: Option<String>,
    /// True when this directory holds most of the repository's parseable
    /// files — in which case it *is* the repository, and excluding it would
    /// be advice worth ignoring.
    dominant: bool,
}

impl BulkDir {
    /// Does this directory carry a parsing cost nobody asked for?
    fn is_costly(&self) -> bool {
        self.excluded_by.is_none()
            && !self.dominant
            && self.tally.candidates >= BULK_FILE_THRESHOLD
    }
}

/// Which top-level directories are big, and which of those the index will
/// actually swallow.
///
/// `.gitignore` is the only filter applied to the scan, so a directory the
/// default exclusions save you from is still *seen* here — and named, because
/// "why is `node_modules` not in my graph" is a question worth answering
/// before it is asked.
fn scan_bulk_dirs(repo_path: &Path) -> Vec<BulkDir> {
    let rules = ExclusionRules::for_repo(repo_path);
    let Ok(scan) =
        code_explorer_ingest::phases::structure::scan_candidates(repo_path, &ExclusionRules::none())
    else {
        return Vec::new();
    };
    let indexed_candidates: usize = scan
        .dirs
        .iter()
        .filter(|d| !rules.is_excluded(&d.path))
        .map(|d| d.candidates)
        .sum();
    scan.dirs
        .into_iter()
        .filter(|d| {
            d.path != "." && (d.walked >= BULK_FILE_THRESHOLD || d.bytes >= BULK_BYTE_THRESHOLD)
        })
        .map(|tally| {
            let excluded_by = rules.excluded_by(&tally.path).map(str::to_string);
            let dominant =
                excluded_by.is_none() && tally.candidates * 2 > indexed_candidates.max(1);
            BulkDir {
                tally,
                excluded_by,
                dominant,
            }
        })
        .collect()
}

fn bulk_check(repo_path: &Path) -> Check {
    let dirs = scan_bulk_dirs(repo_path);
    if dirs.is_empty() {
        return Check::new("bulk", Level::Ok, "no oversized directory in the walk");
    }

    let costly: Vec<&BulkDir> = dirs.iter().filter(|d| d.is_costly()).collect();
    let mut check = if costly.is_empty() {
        Check::new(
            "bulk",
            Level::Ok,
            format!(
                "{} large director{}, none of them a problem",
                dirs.len(),
                if dirs.len() == 1 { "y" } else { "ies" }
            ),
        )
    } else {
        Check::new(
            "bulk",
            Level::Warn,
            format!(
                "{} large director{} would be indexed for nothing",
                costly.len(),
                if costly.len() == 1 { "y" } else { "ies" }
            ),
        )
    };

    for dir in &dirs {
        let verdict = if let Some(pattern) = &dir.excluded_by {
            format!("dropped by '{pattern}'")
        } else if dir.dominant {
            format!(
                "{} parseable - this repository's own source",
                dir.tally.candidates
            )
        } else if dir.is_costly() {
            format!(
                "{} parseable - consider --exclude {}",
                dir.tally.candidates, dir.tally.path
            )
        } else {
            format!("{} parseable - walked, barely parsed", dir.tally.candidates)
        };
        check = check.detail(format!(
            "  {:<24} {:>7} files {:>10}  {verdict}",
            dir.tally.path,
            dir.tally.walked,
            dir.tally.human_bytes(),
        ));
    }

    if !costly.is_empty() {
        let flags: Vec<String> = costly
            .iter()
            .take(3)
            .map(|d| format!("--exclude {}", d.tally.path))
            .collect();
        check = check.fix(format!(
            "code-explorer analyze {} --force {}",
            repo_path.display(),
            flags.join(" ")
        ));
    }
    check
}

fn top_extensions(counts: &BTreeMap<String, usize>, limit: usize) -> Vec<(String, usize)> {
    let mut v: Vec<(String, usize)> = counts.iter().map(|(k, n)| (k.clone(), *n)).collect();
    v.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    v.truncate(limit);
    v
}

fn freshness_check(
    repo_path: &Path,
    meta: Option<&repo_manager::RepoMeta>,
    index_usable: bool,
) -> Check {
    let Some(meta) = meta else {
        return Check::new("freshness", Level::Warn, "no index metadata to compare with HEAD");
    };
    if !git::is_git_repo(repo_path) {
        return Check::new("freshness", Level::Ok, "not a git repository, nothing to compare");
    }
    let Some(head) = git::current_commit(repo_path) else {
        return Check::new("freshness", Level::Warn, "git HEAD could not be read");
    };
    let indexed = meta.last_commit.as_str();
    let analyze = format!("code-explorer analyze {}", repo_path.display());

    if indexed == "unknown" || indexed.is_empty() {
        return Check::new("freshness", Level::Warn, "index records no commit")
            .detail(format!("HEAD is {}", short(&head)))
            .fix(analyze);
    }
    if indexed == head {
        let check = Check::new("freshness", Level::Ok, format!("index at HEAD ({})", short(&head)));
        return if index_usable {
            check
        } else {
            check.detail("the snapshot itself is unusable, see the `index` check")
        };
    }
    if !git::commit_exists(repo_path, indexed) {
        return Check::new(
            "freshness",
            Level::Warn,
            format!("indexed commit {} is unknown to this checkout", short(indexed)),
        )
        .detail("the branch was switched, rebased or the commit was pruned")
        .detail(format!("HEAD is {}", short(&head)))
        .fix(analyze);
    }
    match git::commits_between(repo_path, indexed, &head) {
        Some(0) => Check::new(
            "freshness",
            Level::Warn,
            format!("index is on a diverged commit ({})", short(indexed)),
        )
        .detail(format!("HEAD is {}", short(&head)))
        .fix(analyze),
        Some(n) => Check::new("freshness", Level::Warn, format!("index is {n} commit(s) behind HEAD"))
            .detail(format!("indexed {} → HEAD {}", short(indexed), short(&head)))
            .fix(analyze),
        None => Check::new("freshness", Level::Warn, "index and HEAD could not be compared")
            .detail(format!("indexed {} → HEAD {}", short(indexed), short(&head)))
            .fix(analyze),
    }
}

fn short(commit: &str) -> String {
    commit.chars().take(8).collect()
}

// ─── Rendering ───────────────────────────────────────────────────────────

pub fn render_text(report: &DoctorReport) -> String {
    let mut out = String::new();
    out.push_str("Code Explorer Doctor\n");
    out.push_str(&format!("  Repository: {}\n", report.requested_path));
    if let Some(c) = &report.canonical_path {
        if c != &report.requested_path {
            out.push_str(&format!("  Resolved:   {c}\n"));
        }
    }
    out.push('\n');
    for check in &report.checks {
        out.push_str(&format!(
            "  [{}] {:<10} {}\n",
            check.level.label(),
            check.id,
            check.summary
        ));
        for d in &check.details {
            out.push_str(&format!("               {d}\n"));
        }
        if let Some(fix) = &check.fix {
            out.push_str(&format!("               fix: {fix}\n"));
        }
    }
    out.push('\n');
    let (errors, warns) = (
        report.checks.iter().filter(|c| c.level == Level::Error).count(),
        report.checks.iter().filter(|c| c.level == Level::Warn).count(),
    );
    out.push_str(&match report.status {
        Level::Ok => "  Verdict: healthy.\n".to_string(),
        Level::Warn => format!("  Verdict: usable, {warns} warning(s).\n"),
        Level::Error => format!("  Verdict: broken, {errors} error(s), {warns} warning(s).\n"),
    });
    out
}

// ─── Entry point ─────────────────────────────────────────────────────────

/// Returns the process exit code: 0 when the index is usable, 1 when a check
/// failed hard enough that the tools will not work.
pub fn run(path: &str, json: bool) -> anyhow::Result<i32> {
    let report = diagnose(path, &repo_manager::get_global_registry_path());
    if json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        print!("{}", render_text(&report));
    }
    Ok(if report.status == Level::Error { 1 } else { 0 })
}

#[cfg(test)]
mod tests {
    use super::*;
    use code_explorer_core::graph::types::{GraphNode, NodeLabel, NodeProperties};
    use code_explorer_core::graph::KnowledgeGraph;

    struct Sandbox {
        root: PathBuf,
    }

    impl Sandbox {
        fn new(tag: &str) -> Self {
            let root = std::env::temp_dir().join(format!(
                "code-explorer-doctor-{tag}-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            std::fs::create_dir_all(&root).unwrap();
            Self { root }
        }

        fn repo(&self, name: &str) -> PathBuf {
            let p = self.root.join(name);
            std::fs::create_dir_all(&p).unwrap();
            p
        }

        fn registry(&self, entries: &[repo_manager::RegistryEntry]) -> PathBuf {
            let p = self.root.join("registry.json");
            std::fs::write(&p, serde_json::to_string_pretty(entries).unwrap()).unwrap();
            p
        }

        fn empty_registry(&self) -> PathBuf {
            self.registry(&[])
        }
    }

    impl Drop for Sandbox {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }

    /// Write a usable index (meta.json + a real snapshot) under `repo`.
    fn write_index(repo: &Path, commit: &str, files: Option<usize>) {
        let storage = repo.join(".codeexplorer");
        std::fs::create_dir_all(&storage).unwrap();
        let mut graph = KnowledgeGraph::new();
        graph.add_node(GraphNode {
            id: "File:src/lib.rs".to_string(),
            label: NodeLabel::File,
            properties: NodeProperties {
                name: "lib.rs".to_string(),
                file_path: "src/lib.rs".to_string(),
                ..Default::default()
            },
        });
        code_explorer_db::snapshot::save_snapshot(&graph, &storage.join("graph.bin")).unwrap();
        let meta = repo_manager::RepoMeta {
            repo_path: repo.display().to_string(),
            last_commit: commit.to_string(),
            indexed_at: "2026-09-08T00:00:00Z".to_string(),
            stats: files.map(|n| repo_manager::RepoStats {
                files: Some(n),
                nodes: Some(1),
                edges: Some(0),
                communities: None,
                processes: None,
                embeddings: None,
                documents: None,
                index_duration_ms: None,
            }),
            schema_version: None,
        };
        repo_manager::save_meta(&storage, &meta).unwrap();
    }

    /// Like `write_index`, but records prose documents in the stats.
    fn write_index_with_documents(repo: &Path, commit: &str, files: usize, documents: usize) {
        write_index(repo, commit, Some(files));
        let storage = repo.join(".codeexplorer");
        let mut meta = repo_manager::load_meta(&storage).unwrap().unwrap();
        if let Some(stats) = meta.stats.as_mut() {
            stats.documents = Some(documents);
        }
        repo_manager::save_meta(&storage, &meta).unwrap();
    }

    fn entry_for(repo: &Path) -> repo_manager::RegistryEntry {
        repo_manager::RegistryEntry {
            name: repo.file_name().unwrap().to_string_lossy().to_string(),
            path: repo.display().to_string(),
            storage_path: repo.join(".codeexplorer").display().to_string(),
            indexed_at: "2026-09-08T00:00:00Z".to_string(),
            last_commit: "unknown".to_string(),
            stats: None,
        }
    }

    fn check<'a>(report: &'a DoctorReport, id: &str) -> &'a Check {
        report
            .checks
            .iter()
            .find(|c| c.id == id)
            .unwrap_or_else(|| panic!("no check '{id}' in {:?}", report.checks))
    }

    /// Fill `dir` with `n` trivial TypeScript modules.
    fn seed(repo: &Path, dir: &str, n: usize) {
        let target = repo.join(dir);
        std::fs::create_dir_all(&target).unwrap();
        for i in 0..n {
            std::fs::write(target.join(format!("m{i}.ts")), "export const a = 1;\n").unwrap();
        }
    }

    #[test]
    fn bulk_names_the_big_directory_the_index_would_swallow() {
        let sandbox = Sandbox::new("bulk-warn");
        let repo = sandbox.repo("app");
        seed(&repo, "src", 700); // the repository's own source: dominant
        seed(&repo, "vendor", 600); // neither gitignored nor excluded by default
        seed(&repo, "node_modules/left-pad", 600); // excluded by default

        let report = diagnose(&repo.display().to_string(), &sandbox.empty_registry());
        let bulk = check(&report, "bulk");

        assert_eq!(bulk.level, Level::Warn);
        assert!(
            bulk.summary.contains("1 large directory would be indexed for nothing"),
            "{}",
            bulk.summary
        );

        let vendor = bulk
            .details
            .iter()
            .find(|d| d.contains("vendor"))
            .expect("vendor must be named");
        assert!(vendor.contains("600 files"), "{vendor}");
        assert!(vendor.contains("KB") || vendor.contains("MB"), "size must be shown: {vendor}");
        assert!(vendor.contains("consider --exclude vendor"), "{vendor}");

        let vendored = bulk
            .details
            .iter()
            .find(|d| d.contains("node_modules"))
            .expect("an already-dropped directory is still worth naming");
        assert!(vendored.contains("dropped by 'node_modules'"), "{vendored}");

        let fix = bulk.fix.as_deref().expect("a warn must carry a fix");
        assert!(fix.contains("--exclude vendor"), "{fix}");
        assert!(!fix.contains("--exclude node_modules"), "already handled: {fix}");
        assert!(!fix.contains("--exclude src"), "never offer to drop the source: {fix}");
    }

    #[test]
    fn bulk_never_offers_to_exclude_the_repository_itself() {
        // The real WorkflowBuilder shape: one directory holds the code.
        let sandbox = Sandbox::new("bulk-dominant");
        let repo = sandbox.repo("app");
        seed(&repo, "src", 900);

        let report = diagnose(&repo.display().to_string(), &sandbox.empty_registry());
        let bulk = check(&report, "bulk");

        assert_eq!(bulk.level, Level::Ok, "a big src/ is not a defect");
        assert!(bulk.fix.is_none(), "there is nothing to fix: {:?}", bulk.fix);
        assert!(
            bulk.details.iter().any(|d| d.contains("this repository's own source")),
            "{:?}",
            bulk.details
        );
    }

    #[test]
    fn bulk_is_quiet_on_a_small_repository() {
        let sandbox = Sandbox::new("bulk-ok");
        let repo = sandbox.repo("app");
        seed(&repo, "src", 10);

        let report = diagnose(&repo.display().to_string(), &sandbox.empty_registry());
        let bulk = check(&report, "bulk");
        assert_eq!(bulk.level, Level::Ok);
        assert!(bulk.fix.is_none());
        assert_eq!(bulk.summary, "no oversized directory in the walk");
    }

    #[test]
    fn bulk_only_reports_what_the_defaults_already_handle_as_ok() {
        let sandbox = Sandbox::new("bulk-handled");
        let repo = sandbox.repo("app");
        seed(&repo, "src", 10);
        seed(&repo, "_archive", 600);

        let report = diagnose(&repo.display().to_string(), &sandbox.empty_registry());
        let bulk = check(&report, "bulk");
        assert_eq!(bulk.level, Level::Ok, "a default exclusion is not a problem");
        assert!(bulk.summary.contains("none of them a problem"), "{}", bulk.summary);
        assert!(bulk.details.iter().any(|d| d.contains("dropped by '_archive'")));
        assert!(bulk.fix.is_none());
    }

    #[test]
    fn missing_path_is_an_error_with_a_fix() {
        let sandbox = Sandbox::new("missing");
        let report = diagnose(
            &sandbox.root.join("nowhere").display().to_string(),
            &sandbox.empty_registry(),
        );
        assert_eq!(report.status, Level::Error);
        assert_eq!(check(&report, "path").level, Level::Error);
        assert!(check(&report, "path").fix.is_some());
    }

    #[test]
    fn unindexed_directory_reports_the_analyze_command() {
        let sandbox = Sandbox::new("unindexed");
        let repo = sandbox.repo("plain");
        let report = diagnose(&repo.display().to_string(), &sandbox.empty_registry());

        let index = check(&report, "index");
        assert_eq!(index.level, Level::Error);
        assert!(index.fix.as_deref().unwrap().contains("code-explorer analyze"));
        assert_eq!(report.status, Level::Error);
    }

    #[test]
    fn healthy_index_passes_every_check() {
        let sandbox = Sandbox::new("healthy");
        let repo = sandbox.repo("healthy");
        std::fs::write(repo.join("lib.rs"), "pub fn f() {}\n").unwrap();
        write_index(&repo, "unknown", Some(1));
        let registry = sandbox.registry(&[entry_for(&repo)]);

        let report = diagnose(&repo.display().to_string(), &registry);

        assert_eq!(check(&report, "index").level, Level::Ok, "{:?}", report.checks);
        assert_eq!(check(&report, "registry").level, Level::Ok);
        assert_eq!(check(&report, "coverage").level, Level::Ok);
        // `schema` is stamped by save_meta, `freshness` has no git to compare.
        assert_eq!(check(&report, "schema").level, Level::Ok);
        assert_ne!(report.status, Level::Error);
    }

    #[test]
    fn schema_stamp_is_written_and_read_back() {
        let sandbox = Sandbox::new("schema");
        let repo = sandbox.repo("stamped");
        write_index(&repo, "unknown", Some(1));

        let raw = std::fs::read_to_string(repo.join(".codeexplorer/meta.json")).unwrap();
        let meta: repo_manager::RepoMeta = serde_json::from_str(&raw).unwrap();
        assert_eq!(meta.schema_version, Some(repo_manager::INDEX_SCHEMA_VERSION));

        let report = diagnose(&repo.display().to_string(), &sandbox.empty_registry());
        assert_eq!(check(&report, "schema").level, Level::Ok);
    }

    #[test]
    fn an_index_from_a_newer_build_is_flagged() {
        let sandbox = Sandbox::new("newer");
        let repo = sandbox.repo("future");
        write_index(&repo, "unknown", Some(1));
        let meta_path = repo.join(".codeexplorer/meta.json");
        let raw = std::fs::read_to_string(&meta_path).unwrap();
        let bumped = raw.replace(
            &format!("\"schemaVersion\": {}", repo_manager::INDEX_SCHEMA_VERSION),
            &format!("\"schemaVersion\": {}", repo_manager::INDEX_SCHEMA_VERSION + 7),
        );
        assert_ne!(raw, bumped, "meta.json should carry a schemaVersion field");
        std::fs::write(&meta_path, bumped).unwrap();

        let report = diagnose(&repo.display().to_string(), &sandbox.empty_registry());
        assert_eq!(check(&report, "schema").level, Level::Warn);
        assert!(check(&report, "schema").summary.contains("newer"));
    }

    #[test]
    fn a_repo_absent_from_the_registry_is_a_warning_not_a_failure() {
        let sandbox = Sandbox::new("registry-absent");
        let repo = sandbox.repo("orphan");
        write_index(&repo, "unknown", Some(1));

        let report = diagnose(&repo.display().to_string(), &sandbox.empty_registry());

        let registry = check(&report, "registry");
        assert_eq!(registry.level, Level::Warn);
        assert!(registry.fix.as_deref().unwrap().contains("analyze"));
    }

    #[test]
    fn a_registry_pointing_elsewhere_is_an_error() {
        let sandbox = Sandbox::new("registry-wrong");
        let repo = sandbox.repo("moved");
        write_index(&repo, "unknown", Some(1));
        let mut entry = entry_for(&repo);
        entry.storage_path = sandbox.root.join("elsewhere/.codeexplorer").display().to_string();
        let registry = sandbox.registry(&[entry]);

        let report = diagnose(&repo.display().to_string(), &registry);

        assert_eq!(check(&report, "registry").level, Level::Error);
        assert_eq!(report.status, Level::Error);
    }

    #[test]
    fn coverage_counts_files_by_extension() {
        let sandbox = Sandbox::new("coverage");
        let repo = sandbox.repo("mixed");
        std::fs::create_dir_all(repo.join("docs")).unwrap();
        for i in 0..3 {
            std::fs::write(repo.join(format!("f{i}.rs")), "fn f() {}\n").unwrap();
        }
        for i in 0..7 {
            std::fs::write(repo.join(format!("docs/d{i}.md")), "# Title\n").unwrap();
        }
        write_index(&repo, "unknown", Some(3));

        let report = diagnose(&repo.display().to_string(), &sandbox.empty_registry());
        let coverage = check(&report, "coverage");

        let joined = coverage.details.join("\n");
        assert!(joined.contains("code   .rs"), "{joined}");
        assert!(joined.contains("prose  .md"), "{joined}");
        assert!(joined.contains("3 code, 7 prose"), "{joined}");
        // Prose sitting unindexed is the D2 symptom, and doctor says so.
        assert_eq!(coverage.level, Level::Warn);
        assert!(
            coverage.fix.as_deref().unwrap().contains("--include-docs"),
            "{:?}",
            coverage.fix
        );
    }

    #[test]
    fn a_prose_repository_with_indexed_documents_is_healthy() {
        let sandbox = Sandbox::new("prose-ok");
        let repo = sandbox.repo("book");
        for i in 0..4 {
            std::fs::write(repo.join(format!("c{i}.md")), "# Chapter\n").unwrap();
        }
        write_index_with_documents(&repo, "unknown", 0, 4);

        let report = diagnose(&repo.display().to_string(), &sandbox.empty_registry());
        let coverage = check(&report, "coverage");
        assert_eq!(coverage.level, Level::Ok, "{:?}", coverage);
        assert!(coverage.summary.contains("4 prose documents"), "{}", coverage.summary);
    }

    #[test]
    fn a_thin_index_is_flagged_as_incomplete_coverage() {
        let sandbox = Sandbox::new("thin");
        let repo = sandbox.repo("thin");
        for i in 0..20 {
            std::fs::write(repo.join(format!("f{i}.rs")), "fn f() {}\n").unwrap();
        }
        write_index(&repo, "unknown", Some(2));

        let report = diagnose(&repo.display().to_string(), &sandbox.empty_registry());
        let coverage = check(&report, "coverage");
        assert_eq!(coverage.level, Level::Warn);
        assert!(coverage.fix.as_deref().unwrap().contains("--force"));
    }

    fn git(repo: &Path, args: &[&str]) -> bool {
        std::process::Command::new("git")
            .args(args)
            .current_dir(repo)
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    fn git_available() -> bool {
        std::process::Command::new("git")
            .arg("--version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    fn init_git_repo(repo: &Path) {
        assert!(git(repo, &["init", "-q"]));
        assert!(git(repo, &["config", "user.email", "qa@example.invalid"]));
        assert!(git(repo, &["config", "user.name", "QA"]));
    }

    fn commit_all(repo: &Path, message: &str) -> String {
        assert!(git(repo, &["add", "-A"]));
        assert!(git(repo, &["commit", "-q", "-m", message]));
        git::current_commit(repo).expect("HEAD after commit")
    }

    #[test]
    fn freshness_counts_the_commits_between_the_index_and_head() {
        if !git_available() {
            eprintln!("skipping: git is not installed");
            return;
        }
        let sandbox = Sandbox::new("freshness");
        let repo = sandbox.repo("aging");
        init_git_repo(&repo);
        std::fs::write(repo.join("a.rs"), "fn a() {}\n").unwrap();
        let first = commit_all(&repo, "first");

        write_index(&repo, &first, Some(1));
        let at_head = diagnose(&repo.display().to_string(), &sandbox.empty_registry());
        assert_eq!(check(&at_head, "freshness").level, Level::Ok);

        std::fs::write(repo.join("b.rs"), "fn b() {}\n").unwrap();
        commit_all(&repo, "second");
        std::fs::write(repo.join("c.rs"), "fn c() {}\n").unwrap();
        commit_all(&repo, "third");

        let behind = diagnose(&repo.display().to_string(), &sandbox.empty_registry());
        let freshness = check(&behind, "freshness");
        assert_eq!(freshness.level, Level::Warn);
        assert!(
            freshness.summary.contains("2 commit(s) behind"),
            "{}",
            freshness.summary
        );
        assert!(freshness.fix.as_deref().unwrap().contains("analyze"));
    }

    #[test]
    fn json_output_is_stable_and_machine_readable() {
        let sandbox = Sandbox::new("json");
        let repo = sandbox.repo("jsonrepo");
        write_index(&repo, "unknown", Some(1));

        let report = diagnose(&repo.display().to_string(), &sandbox.empty_registry());
        let value: serde_json::Value = serde_json::to_value(&report).unwrap();

        assert!(value["canonicalPath"].is_string());
        assert!(value["status"].is_string());
        let ids: Vec<&str> = value["checks"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| c["id"].as_str().unwrap())
            .collect();
        assert_eq!(
            ids,
            vec!["path", "index", "schema", "registry", "coverage", "bulk", "freshness"]
        );
    }

    #[test]
    fn text_report_shows_every_check_and_a_verdict() {
        let sandbox = Sandbox::new("text");
        let repo = sandbox.repo("textrepo");
        write_index(&repo, "unknown", Some(1));

        let text = render_text(&diagnose(&repo.display().to_string(), &sandbox.empty_registry()));

        for id in ["path", "index", "schema", "registry", "coverage", "freshness"] {
            assert!(text.contains(id), "missing '{id}' in:\n{text}");
        }
        assert!(text.contains("Verdict:"), "{text}");
    }
}

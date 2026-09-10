//! Directories the indexer must not walk into.
//!
//! `.gitignore` covers a well-kept repository. It does not cover the two cases
//! that actually cost an indexing run: a legacy checkout where `node_modules/`
//! or `target/` was never ignored, and a backup directory (`_archive/`,
//! `backups/`) that git tracks on purpose but that nobody wants in a code
//! graph. Both used to be paid for at full price — the walker descended into
//! them and the parser chewed through them.
//!
//! These rules are applied as a *walk filter*, so an excluded directory is
//! never descended into, and they are shared by the ingester, the CLI and
//! `doctor` so a repository, a budget refusal and a diagnosis all agree on
//! what "the files of this repository" means.

use std::path::Path;

/// Directory names dropped by default, on top of `.gitignore`.
///
/// The first block is the mission list; the second preserves the hard-coded
/// legacy ASP.NET skips that used to live inside the walker.
pub const DEFAULT_EXCLUDE_DIRS: &[&str] = &[
    // Package managers and build output.
    "node_modules",
    "dist",
    "build",
    "target",
    ".next",
    ".venv",
    "venv",
    "__pycache__",
    "coverage",
    ".git",
    // Obvious backup folders.
    "_archive",
    "archive",
    // Legacy .NET skips, previously hard-coded in `walk_repository`.
    "obj",
    "bin",
    "packages",
    ".nuget",
];

/// Glob patterns dropped by default. `*` matches any run of characters.
pub const DEFAULT_EXCLUDE_GLOBS: &[&str] = &["backup*", "*.bak"];

/// Name of the per-repository, line-based exclusion file.
pub const CONFIG_FILE: &str = "config";

/// A resolved set of exclusion rules.
///
/// Patterns match a **path segment**, case-insensitively — never a substring
/// of one. `node_modules` therefore drops `a/node_modules/b.js` and leaves
/// `src/node_modules_helper.ts` alone.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ExclusionRules {
    /// Patterns that exclude. Literal names and `*` globs both live here.
    patterns: Vec<String>,
    /// Patterns that win over `patterns` — the escape hatch for a repository
    /// whose real source really does live under `build/`.
    includes: Vec<String>,
}

impl ExclusionRules {
    /// The built-in defaults only.
    pub fn with_defaults() -> Self {
        Self {
            patterns: DEFAULT_EXCLUDE_DIRS
                .iter()
                .chain(DEFAULT_EXCLUDE_GLOBS.iter())
                .map(|s| s.to_lowercase())
                .collect(),
            includes: Vec::new(),
        }
    }

    /// No rule at all: everything `.gitignore` keeps is indexed.
    pub fn none() -> Self {
        Self::default()
    }

    /// Assemble the rules a run should use.
    ///
    /// * `use_defaults` — `false` is `--no-default-excludes`; explicit
    ///   `extra` patterns still apply.
    /// * `extra` — `--exclude`, and the patterns read from the repository.
    /// * `includes` — `--include`, applied last and winning over everything.
    pub fn from_parts<S: AsRef<str>>(use_defaults: bool, extra: &[S], includes: &[S]) -> Self {
        let mut rules = if use_defaults {
            Self::with_defaults()
        } else {
            Self::none()
        };
        for p in extra {
            rules.push_pattern(p.as_ref());
        }
        for p in includes {
            let norm = normalize_pattern(p.as_ref());
            if !norm.is_empty() && !rules.includes.contains(&norm) {
                rules.includes.push(norm);
            }
        }
        rules
    }

    /// Defaults plus whatever the repository declares in
    /// `.codeexplorer/config` and `code-explorer.toml`.
    pub fn for_repo(repo_path: &Path) -> Self {
        let (extra, includes) = read_repo_patterns(repo_path);
        Self::from_parts(true, &extra, &includes)
    }

    /// Add more patterns to an already-resolved set. `includes` win over
    /// every exclusion, whenever they were added.
    pub fn extend<S: AsRef<str>>(&mut self, extra: &[S], includes: &[S]) {
        for p in extra {
            self.push_pattern(p.as_ref());
        }
        for p in includes {
            let norm = normalize_pattern(p.as_ref());
            if !norm.is_empty() && !self.includes.contains(&norm) {
                self.includes.push(norm);
            }
        }
    }

    fn push_pattern(&mut self, raw: &str) {
        let norm = normalize_pattern(raw);
        if norm.is_empty() {
            return;
        }
        if let Some(rest) = norm.strip_prefix('!') {
            let rest = rest.to_string();
            if !rest.is_empty() && !self.includes.contains(&rest) {
                self.includes.push(rest);
            }
        } else if !self.patterns.contains(&norm) {
            self.patterns.push(norm);
        }
    }

    /// Every exclusion pattern in effect, for display.
    pub fn patterns(&self) -> &[String] {
        &self.patterns
    }

    /// Every re-inclusion pattern in effect, for display.
    pub fn includes(&self) -> &[String] {
        &self.includes
    }

    /// True when no rule at all is in effect.
    pub fn is_empty(&self) -> bool {
        self.patterns.is_empty()
    }

    /// Should `rel_path` (relative to the repository root, `/`-separated) be
    /// dropped? Works for files and for directories: every segment is tested,
    /// so pruning a directory and dropping a file use the same answer.
    pub fn is_excluded(&self, rel_path: &str) -> bool {
        self.excluded_by(rel_path).is_some()
    }

    /// Like [`Self::is_excluded`], but says *which* pattern fired — so a
    /// message can name the rule the user has to override.
    pub fn excluded_by(&self, rel_path: &str) -> Option<&str> {
        if self.patterns.is_empty() {
            return None;
        }
        let mut hit: Option<&str> = None;
        for segment in rel_path.split('/').filter(|s| !s.is_empty() && *s != ".") {
            let lower = segment.to_lowercase();
            if self.includes.iter().any(|p| glob_match(p, &lower)) {
                return None;
            }
            if hit.is_none() {
                hit = self
                    .patterns
                    .iter()
                    .find(|p| glob_match(p, &lower))
                    .map(|p| p.as_str());
            }
        }
        hit
    }
}

/// Lower-case, strip surrounding slashes and whitespace.
fn normalize_pattern(raw: &str) -> String {
    raw.trim()
        .trim_matches('/')
        .trim()
        .to_lowercase()
}

/// Minimal glob: `*` matches any run of characters, everything else is literal.
/// Both sides are expected to be lower-case already.
fn glob_match(pattern: &str, value: &str) -> bool {
    if !pattern.contains('*') {
        return pattern == value;
    }
    let parts: Vec<&str> = pattern.split('*').collect();
    let mut pos = 0usize;
    for (i, part) in parts.iter().enumerate() {
        if part.is_empty() {
            continue;
        }
        if i == 0 {
            if !value[pos..].starts_with(part) {
                return false;
            }
            pos += part.len();
            continue;
        }
        if i == parts.len() - 1 && !pattern.ends_with('*') {
            return value.len() >= pos + part.len() && value[pos..].ends_with(part);
        }
        match value[pos..].find(part) {
            Some(off) => pos += off + part.len(),
            None => return false,
        }
    }
    true
}

/// Read `.codeexplorer/config` and `code-explorer.toml` for extra patterns.
///
/// The line-based file is deliberately dumb: one pattern per line, `#`
/// comments, `!pattern` to re-include. Nothing to learn, nothing to break.
fn read_repo_patterns(repo_path: &Path) -> (Vec<String>, Vec<String>) {
    let mut extra = Vec::new();
    let mut includes = Vec::new();

    let config = repo_path.join(".codeexplorer").join(CONFIG_FILE);
    if let Ok(text) = std::fs::read_to_string(&config) {
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            match line.strip_prefix('!') {
                Some(rest) => includes.push(rest.to_string()),
                None => extra.push(line.to_string()),
            }
        }
    }

    // `[ingestion] ignored_dirs` in code-explorer.toml existed, was tested,
    // and was read by nobody. It is a documented knob: honour it.
    let project = super::project::ProjectConfig::load(repo_path);
    extra.extend(project.ingestion.ignored_dirs.iter().cloned());

    (extra, includes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_drop_the_usual_suspects() {
        let r = ExclusionRules::with_defaults();
        assert!(r.is_excluded("node_modules/react/index.js"));
        assert!(r.is_excluded("frontend/node_modules/react/index.js"));
        assert!(r.is_excluded("_archive/old/app.ts"));
        assert!(r.is_excluded("target/debug/build.rs"));
        assert!(r.is_excluded("coverage/lcov-report/index.html"));
        assert!(r.is_excluded("backups/2024/app.ts"));
        assert!(r.is_excluded("src/app.ts.bak"));
    }

    #[test]
    fn defaults_keep_real_source() {
        let r = ExclusionRules::with_defaults();
        assert!(!r.is_excluded("src/app.ts"));
        assert!(!r.is_excluded("src/node_modules_helper.ts"));
        assert!(!r.is_excluded("src/archives.ts"));
        assert!(!r.is_excluded("crates/builder/src/lib.rs"));
    }

    #[test]
    fn no_defaults_means_no_rule() {
        let r = ExclusionRules::from_parts(false, &[] as &[&str], &[] as &[&str]);
        assert!(r.is_empty());
        assert!(!r.is_excluded("node_modules/react/index.js"));
    }

    #[test]
    fn explicit_excludes_survive_no_defaults() {
        let r = ExclusionRules::from_parts(false, &["vendor"], &[] as &[&str]);
        assert!(r.is_excluded("vendor/lib/a.php"));
        assert!(!r.is_excluded("node_modules/react/index.js"));
    }

    #[test]
    fn include_wins_over_exclude() {
        let r = ExclusionRules::from_parts(true, &[] as &[&str], &["build"]);
        assert!(!r.is_excluded("build/generated/app.ts"));
        assert!(r.is_excluded("node_modules/react/index.js"));
    }

    #[test]
    fn bang_prefix_is_an_include() {
        let r = ExclusionRules::from_parts(true, &["!coverage"], &[] as &[&str]);
        assert!(!r.is_excluded("coverage/report.ts"));
    }

    #[test]
    fn patterns_are_case_and_slash_insensitive() {
        let r = ExclusionRules::from_parts(true, &["/Vendor/"], &[] as &[&str]);
        assert!(r.is_excluded("VENDOR/a.php"));
        assert!(r.is_excluded("vendor/a.php"));
    }

    #[test]
    fn excluded_by_names_the_rule() {
        let r = ExclusionRules::with_defaults();
        assert_eq!(r.excluded_by("web/node_modules/a.js"), Some("node_modules"));
        assert_eq!(r.excluded_by("src/a.ts"), None);
    }

    #[test]
    fn glob_matching_is_anchored() {
        assert!(glob_match("backup*", "backups"));
        assert!(glob_match("backup*", "backup"));
        assert!(!glob_match("backup*", "mybackup"));
        assert!(glob_match("*.bak", "app.ts.bak"));
        assert!(!glob_match("*.bak", "bak"));
        assert!(glob_match("a*c", "abc"));
        assert!(!glob_match("a*c", "abcd"));
    }

    #[test]
    fn repo_config_file_is_read() {
        let dir = std::env::temp_dir().join(format!(
            "ce-exclusions-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(dir.join(".codeexplorer")).unwrap();
        std::fs::write(
            dir.join(".codeexplorer").join(CONFIG_FILE),
            "# my rules\nvendor\n!coverage\n\n",
        )
        .unwrap();
        std::fs::write(
            dir.join("code-explorer.toml"),
            "[ingestion]\nignored_dirs = [\"legacy\"]\n",
        )
        .unwrap();

        let r = ExclusionRules::for_repo(&dir);
        assert!(r.is_excluded("vendor/a.php"));
        assert!(r.is_excluded("legacy/a.php"));
        assert!(!r.is_excluded("coverage/a.ts"));
        assert!(r.is_excluded("node_modules/a.js"));

        std::fs::remove_dir_all(&dir).ok();
    }
}

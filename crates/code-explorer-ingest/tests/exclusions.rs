//! The indexing walk on a repository that never had a `.gitignore`.
//!
//! This is the WorkflowBuilder shape reduced to something a test can build:
//! a `node_modules/` nobody ignores and an `_archive/` of backups git tracks
//! on purpose. Without default exclusions both are walked, read and parsed;
//! with them, neither is.

use std::path::{Path, PathBuf};

use code_explorer_core::config::exclusions::ExclusionRules;
use code_explorer_ingest::phases::structure::walk_repository_with;

struct Sandbox {
    root: PathBuf,
}

impl Sandbox {
    fn new(tag: &str) -> Self {
        let root = std::env::temp_dir().join(format!(
            "ce-exclusions-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        Self { root }
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn write(path: &Path, body: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, body).unwrap();
}

/// Build a repository with `node_modules_files` vendored files, an `_archive/`
/// of backups, and a handful of real sources. No `.git`, no `.gitignore`:
/// nothing but the default exclusions can save this walk.
fn build_repo(root: &Path, node_modules_files: usize) {
    for i in 0..node_modules_files {
        write(
            &root.join("node_modules").join(format!("pkg{}", i / 100)).join(format!("m{i}.js")),
            "module.exports = {};\n",
        );
    }
    for i in 0..40 {
        write(
            &root.join("_archive").join(format!("backup-{i}.ts")),
            "export const old = 1;\n",
        );
    }
    for i in 0..5 {
        write(
            &root.join("src").join(format!("mod{i}.ts")),
            "export const kept = 1;\n",
        );
    }
    write(&root.join("index.ts"), "export * from './src/mod0';\n");
}

#[test]
fn default_exclusions_drop_node_modules_and_archive() {
    let sandbox = Sandbox::new("defaults");
    build_repo(&sandbox.root, 5_000);

    let files = walk_repository_with(&sandbox.root, &ExclusionRules::with_defaults()).unwrap();
    let paths: Vec<&str> = files.iter().map(|f| f.path.as_str()).collect();

    assert!(
        !paths.iter().any(|p| p.starts_with("node_modules/")),
        "node_modules must not be indexed, got {:?}",
        paths.iter().filter(|p| p.starts_with("node_modules/")).take(3).collect::<Vec<_>>()
    );
    assert!(
        !paths.iter().any(|p| p.starts_with("_archive/")),
        "_archive must not be indexed"
    );
    assert!(paths.contains(&"src/mod0.ts"));
    assert!(paths.contains(&"index.ts"));
    assert_eq!(files.len(), 6, "only the six real sources survive");
}

#[test]
fn no_default_excludes_indexes_everything() {
    let sandbox = Sandbox::new("nodefaults");
    build_repo(&sandbox.root, 5_000);

    let files = walk_repository_with(&sandbox.root, &ExclusionRules::none()).unwrap();
    let node_modules = files.iter().filter(|f| f.path.starts_with("node_modules/")).count();
    let archive = files.iter().filter(|f| f.path.starts_with("_archive/")).count();

    assert_eq!(node_modules, 5_000, "--no-default-excludes must keep the 5000 vendored files");
    assert_eq!(archive, 40, "--no-default-excludes must keep the backups");
    assert_eq!(files.len(), 5_046);
}

#[test]
fn an_excluded_directory_is_never_descended_into() {
    // The point of `filter_entry`: the walk must not even look inside an
    // excluded directory. A directory made unreadable would surface as a walk
    // error if it were entered; with the rule in place the walk succeeds.
    let sandbox = Sandbox::new("prune");
    build_repo(&sandbox.root, 20);

    let files = walk_repository_with(&sandbox.root, &ExclusionRules::with_defaults()).unwrap();
    assert_eq!(files.len(), 6);

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let vendored = sandbox.root.join("node_modules");
        std::fs::set_permissions(&vendored, std::fs::Permissions::from_mode(0o000)).unwrap();
        let result = walk_repository_with(&sandbox.root, &ExclusionRules::with_defaults());
        std::fs::set_permissions(&vendored, std::fs::Permissions::from_mode(0o755)).unwrap();
        let files = result.expect("an excluded directory must not be opened at all");
        assert_eq!(files.len(), 6);
    }
}

#[test]
fn an_explicit_include_reopens_a_default_exclusion() {
    let sandbox = Sandbox::new("include");
    build_repo(&sandbox.root, 10);

    let rules = ExclusionRules::from_parts(true, &[] as &[&str], &["_archive"]);
    let files = walk_repository_with(&sandbox.root, &rules).unwrap();
    assert_eq!(files.iter().filter(|f| f.path.starts_with("_archive/")).count(), 40);
    assert!(!files.iter().any(|f| f.path.starts_with("node_modules/")));
}

#[test]
fn scanning_counts_the_job_without_reading_it() {
    use code_explorer_ingest::phases::structure::scan_candidates;

    let sandbox = Sandbox::new("scan");
    build_repo(&sandbox.root, 5_000);
    // A file no language provider claims: walked, never a parsing candidate.
    write(&sandbox.root.join("src").join("notes.bin"), "\u{0}\u{1}");

    let with_defaults = scan_candidates(&sandbox.root, &ExclusionRules::with_defaults()).unwrap();
    assert_eq!(with_defaults.candidates, 6);
    assert_eq!(with_defaults.walked, 7);
    assert!(
        with_defaults.dirs.iter().all(|d| d.path != "node_modules" && d.path != "_archive"),
        "excluded directories must not even be tallied"
    );

    let without = scan_candidates(&sandbox.root, &ExclusionRules::none()).unwrap();
    assert_eq!(without.candidates, 5_046);
    let largest = without.largest(2);
    assert_eq!(largest[0].path, "node_modules");
    assert_eq!(largest[0].candidates, 5_000);
    assert_eq!(largest[1].path, "_archive");
    assert_eq!(largest[1].candidates, 40);
    assert!(largest[0].bytes > 0, "the tally must carry real sizes");

    // `largest` must never panic when asked for more than there is.
    assert_eq!(without.largest(50).len(), without.dirs.len());
}

#[test]
fn a_tally_reports_its_weight_in_readable_units() {
    use code_explorer_ingest::phases::structure::DirTally;
    let tally = |bytes| DirTally { path: "x".into(), candidates: 0, walked: 0, bytes };
    assert_eq!(tally(512).human_bytes(), "512 B");
    assert_eq!(tally(2048).human_bytes(), "2.0 KB");
    assert_eq!(tally(2_200_000_000).human_bytes(), "2.0 GB");
}

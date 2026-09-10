//! Integration tests for the `code-explorer` CLI binary.
//!
//! These tests run the compiled binary and check exit codes + output.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::{SystemTime, UNIX_EPOCH};

fn code_explorer() -> Command {
    Command::new(env!("CARGO_BIN_EXE_code-explorer"))
}

struct TestRepo {
    root: PathBuf,
    explorer_home: PathBuf,
}

impl TestRepo {
    fn new(name: &str) -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock should be after Unix epoch")
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "code-explorer-cli-{name}-{}-{nonce}",
            std::process::id()
        ));
        let explorer_home = root.join("explorer-home");
        fs::create_dir_all(root.join("src")).expect("failed to create test repository");
        fs::create_dir_all(&explorer_home).expect("failed to create isolated explorer home");

        let fixture = Self {
            root,
            explorer_home,
        };
        fixture.git(&["init", "--quiet"]);
        fixture.git(&[
            "config",
            "user.email",
            "code-explorer-tests@example.invalid",
        ]);
        fixture.git(&["config", "user.name", "Code Explorer Tests"]);
        // The index lives inside the working tree; without this, a `git add -A`
        // in a test would commit the index and a later `git checkout` would
        // refuse to overwrite it.
        fs::write(
            fixture.root.join(".git/info/exclude"),
            ".codeexplorer/\n",
        )
        .expect("failed to write git exclude");
        fixture
    }

    fn git(&self, args: &[&str]) {
        let output = Command::new("git")
            .args(args)
            .current_dir(&self.root)
            .output()
            .expect("failed to run git for test repository");
        assert_success(&output, &format!("git {}", args.join(" ")));
    }

    fn explorer(&self, args: &[&str]) -> Output {
        code_explorer()
            .args(args)
            .env("CODE_EXPLORER_HOME", &self.explorer_home)
            .output()
            .expect("failed to run code-explorer")
    }

    fn path(&self) -> &Path {
        &self.root
    }
}

impl Drop for TestRepo {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn assert_success(output: &Output, operation: &str) {
    assert!(
        output.status.success(),
        "{operation} failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn cli_help_shows_usage() {
    let output = code_explorer()
        .arg("--help")
        .output()
        .expect("failed to run code-explorer");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("analyze"),
        "help should mention analyze command"
    );
    assert!(
        stdout.contains("generate"),
        "help should mention generate command"
    );
    assert!(
        stdout.contains("report"),
        "help should mention report command"
    );
}

#[test]
fn cli_analyze_help() {
    let output = code_explorer()
        .args(["analyze", "--help"])
        .output()
        .expect("failed to run code-explorer analyze --help");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("--force"),
        "analyze help should mention --force"
    );
}

#[test]
fn cli_incremental_analyze_updates_an_existing_index() {
    let repo = TestRepo::new("incremental-analyze");
    fs::write(
        repo.path().join("src/lib.rs"),
        "pub fn initial_symbol() -> usize { 1 }\n",
    )
    .expect("failed to write initial source");
    repo.git(&["add", "src/lib.rs"]);
    repo.git(&[
        "-c",
        "commit.gpgsign=false",
        "commit",
        "--quiet",
        "-m",
        "initial fixture",
    ]);

    let repo_arg = repo.path().to_string_lossy().into_owned();
    let initial = repo.explorer(&["analyze", &repo_arg, "--skip-git"]);
    assert_success(&initial, "initial analyze");

    fs::write(
        repo.path().join("src/added.rs"),
        "pub fn incremental_probe_symbol() -> usize { 42 }\n",
    )
    .expect("failed to write added source");

    let incremental = repo.explorer(&["analyze", &repo_arg, "--incremental", "--skip-git"]);
    assert_success(&incremental, "incremental analyze");
    let incremental_stdout = String::from_utf8_lossy(&incremental.stdout);
    assert!(
        !incremental_stdout.contains("Repository already indexed"),
        "--incremental must update an existing index\nstdout:\n{incremental_stdout}"
    );

    let query = repo.explorer(&["query", "incremental_probe_symbol", "--repo", &repo_arg]);
    assert_success(&query, "query after incremental analyze");
    let query_stdout = String::from_utf8_lossy(&query.stdout);
    assert!(
        query_stdout.contains("incremental_probe_symbol"),
        "query should return the symbol added after the initial index\nstdout:\n{query_stdout}"
    );
}

#[test]
fn cli_incremental_analyze_preserves_callers_from_unchanged_files() {
    let repo = TestRepo::new("incremental-callers");
    fs::write(
        repo.path().join("src/a.ts"),
        concat!(
            "import { target } from \"./b\";\n\n",
            "export function caller(): number {\n",
            "  return target();\n",
            "}\n",
        ),
    )
    .expect("failed to write caller source");
    fs::write(
        repo.path().join("src/b.ts"),
        "export function target(): number {\n  return 1;\n}\n",
    )
    .expect("failed to write target source");
    repo.git(&["add", "src/a.ts", "src/b.ts"]);
    repo.git(&[
        "-c",
        "commit.gpgsign=false",
        "commit",
        "--quiet",
        "-m",
        "initial TypeScript fixture",
    ]);

    let repo_arg = repo.path().to_string_lossy().into_owned();
    let initial = repo.explorer(&["analyze", &repo_arg, "--skip-git"]);
    assert_success(&initial, "initial analyze");

    fs::write(
        repo.path().join("src/b.ts"),
        concat!(
            "export function target(): number {\n",
            "  const current = 1;\n",
            "  return current;\n",
            "}\n",
        ),
    )
    .expect("failed to modify target source");

    let incremental = repo.explorer(&["analyze", &repo_arg, "--incremental", "--skip-git"]);
    assert_success(&incremental, "incremental analyze");

    let context = repo.explorer(&["context", "target", "--repo", &repo_arg]);
    assert_success(&context, "context after incremental analyze");
    let context_stdout = String::from_utf8_lossy(&context.stdout);
    assert!(
        context_stdout.contains("Lines:  1-4"),
        "incremental request should refresh the changed target location\nstdout:\n{context_stdout}"
    );
    assert!(
        context_stdout.contains("Callers (1):") && context_stdout.contains("Function caller"),
        "incremental request must preserve callers from unchanged files\nstdout:\n{context_stdout}"
    );

    let incremental_stdout = String::from_utf8_lossy(&incremental.stdout);
    assert!(
        incremental_stdout.contains("Parsed:      1"),
        "CLI should report only the modified callee as parsed\nstdout:\n{incremental_stdout}"
    );
}

#[test]
fn cli_report_help() {
    let output = code_explorer()
        .args(["report", "--help"])
        .output()
        .expect("failed to run code-explorer report --help");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("--json"),
        "report help should mention --json"
    );
}

#[test]
fn cli_config_test_without_config() {
    // Should succeed even without config (graceful error message)
    let output = code_explorer()
        .args(["config", "test"])
        .output()
        .expect("failed to run code-explorer config test");
    assert!(output.status.success());
}

#[test]
fn cli_status_runs() {
    // Status should succeed (may say no index found, but shouldn't crash)
    let output = code_explorer()
        .arg("status")
        .output()
        .expect("failed to run code-explorer status");
    assert!(output.status.success());
}

#[test]
fn cli_list_runs() {
    let output = code_explorer()
        .arg("list")
        .output()
        .expect("failed to run code-explorer list");
    assert!(output.status.success());
}

#[test]
fn cli_hotspots_on_self() {
    // Run hotspots on the code-explorer repo itself
    let repo = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();

    let output = code_explorer()
        .args(["hotspots", "--path", repo.to_str().unwrap()])
        .output()
        .expect("failed to run code-explorer hotspots");
    assert!(
        output.status.success(),
        "hotspots should succeed on a git repo"
    );
}

#[test]
fn cli_coupling_on_self() {
    let repo = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();

    let output = code_explorer()
        .args(["coupling", "--path", repo.to_str().unwrap()])
        .output()
        .expect("failed to run code-explorer coupling");
    assert!(
        output.status.success(),
        "coupling should succeed on a git repo"
    );
}

#[test]
fn cli_ownership_on_self() {
    let repo = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();

    let output = code_explorer()
        .args(["ownership", "--path", repo.to_str().unwrap()])
        .output()
        .expect("failed to run code-explorer ownership");
    assert!(
        output.status.success(),
        "ownership should succeed on a git repo"
    );
}

#[test]
fn cli_cypher_no_index() {
    // Cypher without an index should exit gracefully (not panic)
    let output = code_explorer()
        .args(["cypher", "MATCH (n) RETURN n LIMIT 1"])
        .output()
        .expect("failed to run code-explorer cypher");
    // Should succeed (prints error message but exits 0)
    assert!(output.status.success());
}

#[test]
fn cli_doctor_reports_a_healthy_index_and_exits_zero() {
    let repo = TestRepo::new("doctor-healthy");
    fs::write(
        repo.path().join("src/lib.rs"),
        "pub fn doctor_symbol() -> usize { 1 }\n",
    )
    .expect("failed to write source");
    repo.git(&["add", "src/lib.rs"]);
    repo.git(&[
        "-c",
        "commit.gpgsign=false",
        "commit",
        "--quiet",
        "-m",
        "doctor fixture",
    ]);

    let repo_arg = repo.path().to_string_lossy().into_owned();
    assert_success(&repo.explorer(&["analyze", &repo_arg]), "analyze");

    let output = repo.explorer(&["doctor", &repo_arg]);
    assert_success(&output, "doctor");
    let stdout = String::from_utf8_lossy(&output.stdout);
    for id in ["path", "index", "schema", "registry", "coverage", "freshness"] {
        assert!(stdout.contains(id), "doctor should report '{id}':\n{stdout}");
    }
    assert!(
        stdout.contains("[OK   ] index"),
        "a freshly indexed repository should have a healthy index:\n{stdout}"
    );
    assert!(
        stdout.contains("[OK   ] registry"),
        "analyze should have registered the repository:\n{stdout}"
    );
}

#[test]
fn cli_doctor_json_is_parsable_and_exits_one_when_unindexed() {
    let repo = TestRepo::new("doctor-unindexed");
    let repo_arg = repo.path().to_string_lossy().into_owned();

    let output = repo.explorer(&["doctor", &repo_arg, "--json"]);
    assert_eq!(
        output.status.code(),
        Some(1),
        "an unindexed repository must fail the doctor\nstdout:\n{}",
        String::from_utf8_lossy(&output.stdout)
    );

    let parsed: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("doctor --json must emit valid JSON");
    assert_eq!(parsed["status"], "error");
    let index = parsed["checks"]
        .as_array()
        .expect("checks array")
        .iter()
        .find(|c| c["id"] == "index")
        .expect("index check");
    assert_eq!(index["level"], "error");
    assert!(index["fix"]
        .as_str()
        .expect("index check must carry a fix")
        .contains("code-explorer analyze"));
}

/// Build a synthetic prose repository: `count` Markdown chapters, each with a
/// title, two sub-headings and a link to the next chapter.
fn write_prose_corpus(root: &Path, count: usize) {
    fs::create_dir_all(root.join("chapters")).expect("failed to create chapters directory");
    for i in 0..count {
        let next = (i + 1) % count;
        let body = format!(
            "# Chapter {i} title\n\n\
             Opening paragraph of chapter {i}.\n\n\
             ## Section {i} alpha\n\n\
             Some prose about topic alpha.\n\n\
             ## Section {i} beta\n\n\
             Continue with [chapter {next}](chapter-{next:03}.md).\n"
        );
        fs::write(root.join(format!("chapters/chapter-{i:03}.md")), body)
            .expect("failed to write chapter");
    }
    fs::write(
        root.join("README.md"),
        "# Book\n\nStart at [chapter zero](chapters/chapter-000.md).\n",
    )
    .expect("failed to write README");
}

#[test]
fn cli_prose_repository_indexes_every_markdown_file_with_its_headings() {
    let repo = TestRepo::new("prose-corpus");
    write_prose_corpus(repo.path(), 300);
    // One build script, so the repository is not 100% prose but still far
    // below the 30% code threshold that turns prose indexing on.
    fs::write(repo.path().join("src/build.rs"), "fn main() {}\n").expect("failed to write source");
    repo.git(&["add", "-A"]);
    repo.git(&[
        "-c",
        "commit.gpgsign=false",
        "commit",
        "--quiet",
        "-m",
        "prose fixture",
    ]);

    let repo_arg = repo.path().to_string_lossy().into_owned();
    let analyze = repo.explorer(&["analyze", &repo_arg]);
    assert_success(&analyze, "analyze prose repository");
    let stdout = String::from_utf8_lossy(&analyze.stdout);
    assert!(
        stdout.contains("Documents:   301"),
        "every Markdown file should be indexed:\n{stdout}"
    );

    // Headings became searchable symbols.
    let context = repo.explorer(&["context", "Chapter 42 title", "--repo", &repo_arg]);
    assert_success(&context, "context on a heading");
    let context_out = String::from_utf8_lossy(&context.stdout);
    assert!(
        context_out.contains("chapter-042.md"),
        "context should locate the heading in its document:\n{context_out}"
    );

    // Full-text search reaches prose.
    let query = repo.explorer(&["query", "Section 7 beta", "--repo", &repo_arg, "--limit", "5"]);
    assert_success(&query, "query prose");
    let query_out = String::from_utf8_lossy(&query.stdout);
    assert!(
        query_out.contains("chapter-007.md"),
        "search should reach document headings:\n{query_out}"
    );

    // Internal links became edges: the README points at chapter zero.
    let impact = repo.explorer(&[
        "impact",
        "chapter-000.md",
        "--repo",
        &repo_arg,
        "--direction",
        "upstream",
    ]);
    assert_success(&impact, "impact on a linked document");
    let impact_out = String::from_utf8_lossy(&impact.stdout);
    assert!(
        impact_out.contains("README.md") || impact_out.contains("chapter-299.md"),
        "a resolved Markdown link should be traversable:\n{impact_out}"
    );
}

#[test]
fn cli_no_docs_keeps_a_prose_repository_code_only() {
    let repo = TestRepo::new("prose-optout");
    write_prose_corpus(repo.path(), 5);
    fs::write(repo.path().join("src/lib.rs"), "pub fn f() {}\n").expect("failed to write source");
    repo.git(&["add", "-A"]);
    repo.git(&[
        "-c",
        "commit.gpgsign=false",
        "commit",
        "--quiet",
        "-m",
        "prose optout fixture",
    ]);

    let repo_arg = repo.path().to_string_lossy().into_owned();
    let analyze = repo.explorer(&["analyze", &repo_arg, "--no-docs"]);
    assert_success(&analyze, "analyze with --no-docs");
    let stdout = String::from_utf8_lossy(&analyze.stdout);
    assert!(
        !stdout.contains("Documents:"),
        "--no-docs must not index prose:\n{stdout}"
    );
}

#[test]
fn cli_include_docs_indexes_prose_in_a_code_repository() {
    let repo = TestRepo::new("prose-optin");
    fs::write(repo.path().join("NOTES.md"), "# Design notes\n\nbody\n")
        .expect("failed to write notes");
    for i in 0..10 {
        fs::write(
            repo.path().join(format!("src/mod{i}.rs")),
            format!("pub fn f{i}() {{}}\n"),
        )
        .expect("failed to write source");
    }
    repo.git(&["add", "-A"]);
    repo.git(&[
        "-c",
        "commit.gpgsign=false",
        "commit",
        "--quiet",
        "-m",
        "code fixture",
    ]);

    let repo_arg = repo.path().to_string_lossy().into_owned();

    // Default: a code repository does not pay for a documentation pass.
    let plain = repo.explorer(&["analyze", &repo_arg]);
    assert_success(&plain, "analyze code repository");
    assert!(
        !String::from_utf8_lossy(&plain.stdout).contains("Documents:"),
        "a mostly-code repository should not index prose by default"
    );

    // Opt in explicitly.
    let opted = repo.explorer(&["analyze", &repo_arg, "--force", "--include-docs"]);
    assert_success(&opted, "analyze --include-docs");
    let stdout = String::from_utf8_lossy(&opted.stdout);
    assert!(
        stdout.contains("Documents:   1"),
        "--include-docs must index prose:\n{stdout}"
    );
}

// ─── Incremental indexing: renames, deletions, branch switches ───────────
//
// The oracle is equivalence: after an incremental run, the graph must be the
// one a full re-index would have produced. That catches orphan nodes, stale
// folders and dangling edges in a single assertion, without depending on
// timing (the timing oracle stays `#[ignore]`, as in the earlier slices).

fn load_graph(repo: &Path) -> code_explorer_core::graph::KnowledgeGraph {
    code_explorer_db::snapshot::load_snapshot(&repo.join(".codeexplorer/graph.bin"))
        .expect("snapshot should load")
}

fn node_ids(graph: &code_explorer_core::graph::KnowledgeGraph) -> Vec<String> {
    let mut ids: Vec<String> = graph.iter_nodes().map(|n| n.id.clone()).collect();
    ids.sort();
    ids
}

fn relationship_ids(graph: &code_explorer_core::graph::KnowledgeGraph) -> Vec<String> {
    let mut ids: Vec<String> = graph.iter_relationships().map(|r| r.id.clone()).collect();
    ids.sort();
    ids
}

fn assert_no_dangling_edges(graph: &code_explorer_core::graph::KnowledgeGraph) {
    let ids: std::collections::HashSet<String> =
        graph.iter_nodes().map(|n| n.id.clone()).collect();
    for rel in graph.iter_relationships() {
        assert!(
            ids.contains(&rel.source_id),
            "relationship {} points at a missing source {}",
            rel.id,
            rel.source_id
        );
        assert!(
            ids.contains(&rel.target_id),
            "relationship {} points at a missing target {}",
            rel.id,
            rel.target_id
        );
    }
}

fn parsed_files_in_last_run(repo: &Path) -> u64 {
    let raw = fs::read_to_string(repo.join(".codeexplorer/analyze.json"))
        .expect("analyze.json should exist");
    let value: serde_json::Value = serde_json::from_str(&raw).expect("analyze.json is JSON");
    value["parsed_files"].as_u64().expect("parsed_files")
}

fn commit_all(repo: &TestRepo, message: &str) {
    repo.git(&["add", "-A"]);
    repo.git(&["-c", "commit.gpgsign=false", "commit", "--quiet", "-m", message]);
}

fn seed_modules(repo: &TestRepo, count: usize) {
    for i in 0..count {
        fs::write(
            repo.path().join(format!("src/mod{i}.rs")),
            format!("pub fn symbol_{i}() -> usize {{ {i} }}\n"),
        )
        .expect("failed to write module");
    }
}

#[test]
fn cli_incremental_after_a_rename_matches_a_full_reindex() {
    let repo = TestRepo::new("incr-rename");
    seed_modules(&repo, 6);
    fs::create_dir_all(repo.path().join("legacy")).expect("failed to create legacy dir");
    fs::write(
        repo.path().join("legacy/old_name.rs"),
        "pub fn moved_symbol() -> usize { 41 }\n",
    )
    .expect("failed to write file");
    commit_all(&repo, "seed");

    let repo_arg = repo.path().to_string_lossy().into_owned();
    assert_success(&repo.explorer(&["analyze", &repo_arg]), "initial analyze");

    fs::rename(
        repo.path().join("legacy/old_name.rs"),
        repo.path().join("src/new_name.rs"),
    )
    .expect("failed to rename");
    commit_all(&repo, "rename");

    assert_success(
        &repo.explorer(&["analyze", &repo_arg, "--incremental"]),
        "incremental analyze after rename",
    );
    let incremental = load_graph(repo.path());
    // Only the moved file is re-parsed; the six untouched modules are reused.
    assert!(
        parsed_files_in_last_run(repo.path()) <= 2,
        "an incremental run must not re-parse unchanged files"
    );

    assert_success(
        &repo.explorer(&["analyze", &repo_arg, "--force"]),
        "full re-index",
    );
    let full = load_graph(repo.path());

    assert_eq!(
        node_ids(&incremental),
        node_ids(&full),
        "incremental graph must equal a full re-index after a rename"
    );
    assert_eq!(relationship_ids(&incremental), relationship_ids(&full));
    assert_no_dangling_edges(&incremental);
    assert!(
        !incremental
            .iter_nodes()
            .any(|n| n.properties.file_path.starts_with("legacy")),
        "the old path must leave no node behind, folder included"
    );
}

#[test]
fn cli_incremental_after_a_deletion_matches_a_full_reindex() {
    let repo = TestRepo::new("incr-delete");
    seed_modules(&repo, 5);
    fs::create_dir_all(repo.path().join("doomed")).expect("failed to create dir");
    fs::write(
        repo.path().join("doomed/gone.rs"),
        "pub fn about_to_vanish() {}\n",
    )
    .expect("failed to write file");
    commit_all(&repo, "seed");

    let repo_arg = repo.path().to_string_lossy().into_owned();
    assert_success(&repo.explorer(&["analyze", &repo_arg]), "initial analyze");

    fs::remove_dir_all(repo.path().join("doomed")).expect("failed to delete");
    commit_all(&repo, "delete");

    assert_success(
        &repo.explorer(&["analyze", &repo_arg, "--incremental"]),
        "incremental analyze after deletion",
    );
    let incremental = load_graph(repo.path());

    assert_success(
        &repo.explorer(&["analyze", &repo_arg, "--force"]),
        "full re-index",
    );
    let full = load_graph(repo.path());

    assert_eq!(node_ids(&incremental), node_ids(&full));
    assert_eq!(relationship_ids(&incremental), relationship_ids(&full));
    assert_no_dangling_edges(&incremental);
    assert!(
        !incremental
            .iter_nodes()
            .any(|n| n.properties.file_path.starts_with("doomed")),
        "a deleted directory must leave no node behind"
    );
}

#[test]
fn cli_incremental_after_a_branch_switch_matches_a_full_reindex() {
    let repo = TestRepo::new("incr-branch");
    seed_modules(&repo, 8);
    commit_all(&repo, "base");

    let repo_arg = repo.path().to_string_lossy().into_owned();
    assert_success(&repo.explorer(&["analyze", &repo_arg]), "analyze on the base branch");

    // The default branch name depends on the host git configuration.
    let base_branch = String::from_utf8(
        Command::new("git")
            .args(["rev-parse", "--abbrev-ref", "HEAD"])
            .current_dir(repo.path())
            .output()
            .expect("failed to read the current branch")
            .stdout,
    )
    .expect("branch name is utf-8")
    .trim()
    .to_string();

    // A feature branch touches two files out of eight.
    repo.git(&["checkout", "--quiet", "-b", "feature"]);
    fs::write(
        repo.path().join("src/mod0.rs"),
        "pub fn symbol_0() -> usize { 100 }\npub fn only_on_feature() {}\n",
    )
    .expect("failed to write");
    fs::remove_file(repo.path().join("src/mod7.rs")).expect("failed to remove");
    commit_all(&repo, "feature work");

    assert_success(
        &repo.explorer(&["analyze", &repo_arg, "--incremental"]),
        "incremental analyze after branch switch",
    );
    let incremental = load_graph(repo.path());
    assert!(
        parsed_files_in_last_run(repo.path()) <= 2,
        "switching branch must only re-parse what actually differs"
    );

    assert_success(
        &repo.explorer(&["analyze", &repo_arg, "--force"]),
        "full re-index on the feature branch",
    );
    let full = load_graph(repo.path());

    assert_eq!(node_ids(&incremental), node_ids(&full));
    assert_eq!(relationship_ids(&incremental), relationship_ids(&full));
    assert!(incremental
        .iter_nodes()
        .any(|n| n.properties.name == "only_on_feature"));
    assert!(!incremental
        .iter_nodes()
        .any(|n| n.properties.file_path == "src/mod7.rs"));

    // Going back must be just as incremental, and just as exact.
    repo.git(&["checkout", "--quiet", &base_branch]);
    assert_success(
        &repo.explorer(&["analyze", &repo_arg, "--incremental"]),
        "incremental analyze back on the base branch",
    );
    let back = load_graph(repo.path());
    assert!(back
        .iter_nodes()
        .any(|n| n.properties.file_path == "src/mod7.rs"));
    assert!(!back
        .iter_nodes()
        .any(|n| n.properties.name == "only_on_feature"));
    assert_no_dangling_edges(&back);
}

#[test]
fn cli_doctor_reports_a_branch_switch_as_an_out_of_date_index() {
    let repo = TestRepo::new("incr-doctor");
    seed_modules(&repo, 3);
    commit_all(&repo, "base");

    let repo_arg = repo.path().to_string_lossy().into_owned();
    assert_success(&repo.explorer(&["analyze", &repo_arg]), "analyze on the base branch");

    repo.git(&["checkout", "--quiet", "-b", "feature"]);
    fs::write(repo.path().join("src/mod0.rs"), "pub fn changed() {}\n").expect("failed to write");
    commit_all(&repo, "one");
    fs::write(repo.path().join("src/mod1.rs"), "pub fn changed_too() {}\n")
        .expect("failed to write");
    commit_all(&repo, "two");

    let output = repo.explorer(&["doctor", &repo_arg, "--json"]);
    let parsed: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("doctor --json must emit valid JSON");
    let freshness = parsed["checks"]
        .as_array()
        .expect("checks")
        .iter()
        .find(|c| c["id"] == "freshness")
        .expect("freshness check");
    assert_eq!(freshness["level"], "warn");
    assert!(
        freshness["summary"]
            .as_str()
            .unwrap()
            .contains("2 commit(s) behind"),
        "doctor should count the commits: {}",
        freshness["summary"]
    );
}

/// Timing oracle: kept `#[ignore]` like the earlier incremental slices — wall
/// clock is not a deterministic assertion on a shared machine. Run with
/// `cargo test -p code-explorer-cli --test cli_integration -- --ignored`.
#[test]
#[ignore = "timing oracle: wall-clock, not deterministic on a shared machine"]
fn cli_incremental_is_faster_than_a_full_reindex() {
    use std::time::Instant;

    let repo = TestRepo::new("incr-timing");
    seed_modules(&repo, 120);
    commit_all(&repo, "seed");

    let repo_arg = repo.path().to_string_lossy().into_owned();
    assert_success(&repo.explorer(&["analyze", &repo_arg]), "initial analyze");

    fs::write(repo.path().join("src/mod0.rs"), "pub fn touched() {}\n").expect("failed to write");

    let start = Instant::now();
    assert_success(
        &repo.explorer(&["analyze", &repo_arg, "--incremental"]),
        "incremental analyze",
    );
    let incremental = start.elapsed();

    let start = Instant::now();
    assert_success(&repo.explorer(&["analyze", &repo_arg, "--force"]), "full analyze");
    let full = start.elapsed();

    assert!(
        incremental < full,
        "incremental {incremental:?} should beat a full re-index {full:?}"
    );
}

// ─── Indexing budget: counting before parsing ────────────────────────────

/// Fill `repo` with `n` trivial TypeScript modules under `dir`.
fn seed_budget_modules(repo: &TestRepo, dir: &str, n: usize) {
    let target = repo.path().join(dir);
    fs::create_dir_all(&target).expect("failed to create module directory");
    for i in 0..n {
        fs::write(
            target.join(format!("mod{i}.ts")),
            format!("export const value{i} = {i};\n"),
        )
        .expect("failed to write module");
    }
}

#[test]
fn analyze_counts_candidates_before_parsing() {
    let repo = TestRepo::new("candidates");
    seed_budget_modules(&repo, "src", 12);

    let output = repo.explorer(&["analyze", repo.path().to_str().unwrap()]);
    assert_success(&output, "analyze");
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(
        stdout.contains("Candidates:"),
        "analyze must say how big the job is before running it:\n{stdout}"
    );
    assert!(
        stdout.contains("12 parseable"),
        "the candidate count must be the real one:\n{stdout}"
    );
    assert!(
        stdout.contains("Exclusions:") && stdout.contains("node_modules"),
        "analyze must name the rules it applied:\n{stdout}"
    );
}

#[test]
fn analyze_refuses_a_repository_over_the_file_budget() {
    let repo = TestRepo::new("budget");
    seed_budget_modules(&repo, "src", 12);
    seed_budget_modules(&repo, "generated", 30);
    seed_budget_modules(&repo, "fixtures", 20);

    let output = repo.explorer(&[
        "analyze",
        repo.path().to_str().unwrap(),
        "--max-files",
        "20",
    ]);
    assert!(
        !output.status.success(),
        "analyze must refuse, not run: {output:?}"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("Refusing to index 62 candidate files"),
        "the refusal must state the count:\n{stderr}"
    );
    assert!(
        stderr.contains("Largest directories after exclusions") && stderr.contains("generated"),
        "the refusal must name the directories that carry the cost:\n{stderr}"
    );
    assert!(
        stderr.contains("--exclude generated"),
        "the refusal must hand over a runnable command:\n{stderr}"
    );
    // And it must refuse *before* writing an index.
    assert!(
        !repo.path().join(".codeexplorer/graph.bin").exists(),
        "a refused run must leave no index behind"
    );
}

#[test]
fn excluding_the_heavy_directory_brings_the_repository_under_budget() {
    let repo = TestRepo::new("budget-fixed");
    seed_budget_modules(&repo, "src", 12);
    seed_budget_modules(&repo, "generated", 30);
    seed_budget_modules(&repo, "fixtures", 20);

    let output = repo.explorer(&[
        "analyze",
        repo.path().to_str().unwrap(),
        "--max-files",
        "20",
        "--exclude",
        "generated",
        "--exclude",
        "fixtures",
    ]);
    assert_success(&output, "analyze --exclude generated --exclude fixtures");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("12 parseable"),
        "the exclusion must remove the generated modules:\n{stdout}"
    );
    assert!(repo.path().join(".codeexplorer/graph.bin").exists());
}

#[test]
fn max_files_zero_removes_the_guard() {
    let repo = TestRepo::new("budget-off");
    seed_budget_modules(&repo, "src", 12);

    let output = repo.explorer(&[
        "analyze",
        repo.path().to_str().unwrap(),
        "--max-files",
        "0",
    ]);
    assert_success(&output, "analyze --max-files 0");
}

#[test]
fn default_exclusions_keep_vendored_code_out_of_the_index() {
    let repo = TestRepo::new("vendored");
    seed_budget_modules(&repo, "src", 5);
    seed_budget_modules(&repo, "node_modules/left-pad", 40);
    seed_budget_modules(&repo, "_archive", 25);

    let output = repo.explorer(&["analyze", repo.path().to_str().unwrap()]);
    assert_success(&output, "analyze");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("5 parseable"),
        "vendored and archived code must not be indexed:\n{stdout}"
    );

    let output = repo.explorer(&[
        "analyze",
        repo.path().to_str().unwrap(),
        "--force",
        "--no-default-excludes",
    ]);
    assert_success(&output, "analyze --no-default-excludes");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("70 parseable"),
        "--no-default-excludes must bring all 70 files back:\n{stdout}"
    );
}

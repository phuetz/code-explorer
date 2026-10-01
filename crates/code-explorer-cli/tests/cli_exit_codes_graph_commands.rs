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
        fs::write(
            fixture.root.join(".git/info/exclude"),
            ".codeexplorer/\n",
        )
        .expect("exclude .codeexplorer");
        fixture
    }

    fn path(&self) -> &Path {
        &self.root
    }

    fn git(&self, args: &[&str]) -> Output {
        let output = Command::new("git")
            .args(args)
            .current_dir(&self.root)
            .output()
            .expect("failed to run git");
        assert!(
            output.status.success(),
            "git {args:?} failed:\nSTDOUT:\n{}\nSTDERR:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        output
    }

    fn explorer(&self, args: &[&str]) -> Output {
        code_explorer()
            .args(args)
            .current_dir(&self.root)
            .env("CODE_EXPLORER_HOME", &self.explorer_home)
            .output()
            .expect("failed to run code-explorer")
    }
}

impl Drop for TestRepo {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn assert_success(output: &Output, context: &str) {
    assert!(
        output.status.success(),
        "{context} failed (expected exit code 0):\nSTDOUT:\n{}\nSTDERR:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn assert_failure(output: &Output, context: &str) {
    assert!(
        !output.status.success(),
        "{context} succeeded but was expected to fail (expected non-zero exit code):\nSTDOUT:\n{}\nSTDERR:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn diagram_and_trace_fail_if_no_index_or_symbol() {
    let repo = TestRepo::new("diagram-fail");
    let repo_path = repo.path().to_str().unwrap();

    // 1. Empty dir: no index
    let out = repo.explorer(&["diagram", "X", "--path", repo_path]);
    assert_failure(&out, "diagram without index");
    assert!(String::from_utf8_lossy(&out.stderr).contains("No index found") || String::from_utf8_lossy(&out.stdout).contains("No index found"));

    let out = repo.explorer(&["trace-files", "X", "--path", repo_path]);
    assert_failure(&out, "trace-files without index");

    let out = repo.explorer(&["trace-import", "log.txt", "--path", repo_path]);
    assert_failure(&out, "trace-import without index");

    // Index it
    fs::write(repo.path().join("src/mod.rs"), "pub fn target() {}\n").unwrap();
    repo.git(&["add", "-A"]);
    repo.git(&["commit", "-m", "init"]);
    assert_success(&repo.explorer(&["analyze", repo_path]), "analyze");

    // 2. Index exists but symbol/file not found
    let out = repo.explorer(&["diagram", "nosuchsymbol", "--path", repo_path]);
    assert_failure(&out, "diagram missing symbol");

    let out = repo.explorer(&["trace-files", "nosuchsymbol", "--path", repo_path]);
    assert_failure(&out, "trace-files missing symbol");

    let out = repo.explorer(&["trace-import", "absent.log", "--path", repo_path]);
    assert_failure(&out, "trace-import missing log");

    // 3. Normal run should succeed
    let out = repo.explorer(&["diagram", "target", "--path", repo_path]);
    assert_success(&out, "diagram with target");
    assert!(String::from_utf8_lossy(&out.stdout).contains("mermaid"));
}

use std::fs;
use std::process::Command;

#[test]
fn test_exit_codes_report_coverage() {
    let empty_dir = tempfile::tempdir().unwrap();
    let empty_path = empty_dir.path().to_str().unwrap();

    let exe = env!("CARGO_BIN_EXE_code-explorer");

    // report --path <dossier vide>
    let output = Command::new(&exe)
        .args(["report", "--path", empty_path])
        .output()
        .unwrap();
    assert!(!output.status.success(), "report on empty dir should fail");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("No index found"),
        "report on empty dir should print 'No index found' on stderr"
    );
    assert!(output.stdout.is_empty(), "stdout should be empty");

    // coverage --path <dossier vide>
    let output = Command::new(&exe)
        .args(["coverage", "NoSuchClass", "--path", empty_path])
        .output()
        .unwrap();
    assert!(
        !output.status.success(),
        "coverage on empty dir should fail"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("No index found"),
        "coverage on empty dir should print 'No index found' on stderr"
    );
    assert!(output.stdout.is_empty(), "stdout should be empty");

    // rag-import <dossier existant> --path <dossier vide>
    let docs_dir = tempfile::tempdir().unwrap();
    let docs_path = docs_dir.path().to_str().unwrap();
    let output = Command::new(&exe)
        .args(["rag-import", docs_path, "--path", empty_path])
        .output()
        .unwrap();
    assert!(
        !output.status.success(),
        "rag-import on empty dir should fail"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("No index found"),
        "rag-import on empty dir should print 'No index found' on stderr"
    );
    assert!(output.stdout.is_empty(), "stdout should be empty");
}

#[test]
fn test_exit_codes_with_indexed_repo() {
    let repo_dir = tempfile::tempdir().unwrap();
    let repo_path = repo_dir.path().to_str().unwrap();
    let explorer_home = repo_dir.path().join("explorer-home");

    // Create a dummy target function so it's a valid analyze target
    let src_dir = repo_dir.path().join("src");
    fs::create_dir(&src_dir).unwrap();
    fs::write(src_dir.join("main.rs"), "fn target() {}").unwrap();

    let exe = env!("CARGO_BIN_EXE_code-explorer");

    let output = Command::new(&exe)
        .args(["analyze", repo_path, "--skip-git"])
        .env("CODE_EXPLORER_HOME", &explorer_home)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "analyze should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let output = Command::new(&exe)
        .args(["coverage", "NoSuchClass", "--path", repo_path])
        .env("CODE_EXPLORER_HOME", &explorer_home)
        .output()
        .unwrap();
    assert!(!output.status.success(), "coverage NoSuchClass should fail");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("Class 'NoSuchClass' not found."),
        "coverage on invalid class should print error on stderr"
    );
    assert!(output.stdout.is_empty(), "stdout should be empty");

    let output = Command::new(&exe)
        .args(["coverage", "NoSuchClass", "--trace", "--path", repo_path])
        .env("CODE_EXPLORER_HOME", &explorer_home)
        .output()
        .unwrap();
    assert!(
        !output.status.success(),
        "coverage NoSuchClass --trace should fail"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("Class 'NoSuchClass' not found."),
        "coverage on invalid class should print error on stderr"
    );
    assert!(output.stdout.is_empty(), "stdout should be empty");

    let output = Command::new(&exe)
        .args([
            "rag-import",
            "/path/that/does/not/exist",
            "--path",
            repo_path,
        ])
        .env("CODE_EXPLORER_HOME", &explorer_home)
        .output()
        .unwrap();
    assert!(
        !output.status.success(),
        "rag-import with invalid docs dir should fail"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("Documentation directory not found"),
        "rag-import on invalid dir should print error on stderr"
    );
    assert!(output.stdout.is_empty(), "stdout should be empty");
}

#[test]
fn test_report_success_exit_code() {
    let repo_dir = tempfile::tempdir().unwrap();
    let repo_path = repo_dir.path().to_str().unwrap();
    let explorer_home = repo_dir.path().join("explorer-home");

    let src_dir = repo_dir.path().join("src");
    fs::create_dir(&src_dir).unwrap();
    fs::write(src_dir.join("main.rs"), "fn target() {}").unwrap();

    let exe = env!("CARGO_BIN_EXE_code-explorer");

    let output = Command::new(&exe)
        .args(["analyze", repo_path, "--skip-git"])
        .env("CODE_EXPLORER_HOME", &explorer_home)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "analyze should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let output = Command::new(&exe)
        .args(["report", "--path", repo_path])
        .env("CODE_EXPLORER_HOME", &explorer_home)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "report on indexed dir should succeed"
    );
}

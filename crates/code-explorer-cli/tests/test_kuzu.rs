#![cfg(feature = "kuzu-backend")]

use std::process::Command;

#[test]
fn test_kuzu_experimental_flag() {
    let output = Command::new(env!("CARGO_BIN_EXE_code-explorer"))
        .args(["analyze", "--help"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("--kuzu"));
    assert!(stdout.contains("expérimental"));
}

#[test]
fn test_kuzu_fails_early() {
    let repo = std::env::temp_dir().join(format!("code-explorer-kuzu-refusal-{}", std::process::id()));
    std::fs::create_dir_all(&repo).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_code-explorer"))
        .args(["analyze", "--kuzu"])
        .arg(&repo)
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("expérimental"));
    assert!(!repo.join(".codeexplorer").exists(), "indexation lancée avant le refus");
    std::fs::remove_dir(&repo).unwrap();
}

use std::process::Command;

#[test]
fn test_cli_symbol_not_found() {
    let base_dir = std::env::temp_dir().join(format!("ce_test_{}", std::process::id()));
    let repo_dir = base_dir.join("repo");
    let home_dir = base_dir.join("home");
    std::fs::create_dir_all(&repo_dir).unwrap();
    std::fs::create_dir_all(&home_dir).unwrap();

    let source_file = repo_dir.join("main.rs");
    std::fs::write(&source_file, "fn target() {}\n").unwrap();

    // Init git repo to allow analyze to run
    Command::new("git")
        .arg("init")
        .current_dir(&repo_dir)
        .output()
        .unwrap();

    let cargo_bin = env!("CARGO_BIN_EXE_code-explorer");

    // Run analyze (path argument instead of --repo)
    let analyze_status = Command::new(cargo_bin)
        .arg("analyze")
        .arg(&repo_dir)
        .env("CODE_EXPLORER_HOME", &home_dir)
        .status()
        .unwrap();
    assert!(analyze_status.success());

    // Test context known symbol
    let context_known_output = Command::new(cargo_bin)
        .arg("context")
        .arg("target")
        .arg("--repo")
        .arg(&repo_dir)
        .env("CODE_EXPLORER_HOME", &home_dir)
        .output()
        .unwrap();
    assert!(context_known_output.status.success());

    // Test impact known symbol
    let impact_known_output = Command::new(cargo_bin)
        .arg("impact")
        .arg("target")
        .arg("--repo")
        .arg(&repo_dir)
        .env("CODE_EXPLORER_HOME", &home_dir)
        .output()
        .unwrap();
    assert!(impact_known_output.status.success());

    // Test context unknown symbol
    let context_unknown_output = Command::new(cargo_bin)
        .arg("context")
        .arg("nosuchsymbol")
        .arg("--repo")
        .arg(&repo_dir)
        .env("CODE_EXPLORER_HOME", &home_dir)
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&context_unknown_output.stderr);
    let stdout = String::from_utf8_lossy(&context_unknown_output.stdout);
    assert!(!context_unknown_output.status.success(), "context output success was {}, stderr: {stderr}", context_unknown_output.status.success());
    assert!(stderr.contains("nosuchsymbol"));
    assert!(stdout.trim().is_empty());

    // Test impact unknown symbol
    let impact_unknown_output = Command::new(cargo_bin)
        .arg("impact")
        .arg("nosuchsymbol")
        .arg("--repo")
        .arg(&repo_dir)
        .env("CODE_EXPLORER_HOME", &home_dir)
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&impact_unknown_output.stderr);
    let stdout = String::from_utf8_lossy(&impact_unknown_output.stdout);
    assert!(!impact_unknown_output.status.success());
    assert!(stderr.contains("nosuchsymbol"));
    assert!(stdout.trim().is_empty());
}

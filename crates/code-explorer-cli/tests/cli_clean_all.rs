use std::fs;
use std::path::PathBuf;
use std::process::Command;
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
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "code-explorer-cli-clean-{name}-{}-{nonce}",
            std::process::id()
        ));
        let explorer_home = root.join("explorer-home");
        fs::create_dir_all(&explorer_home).unwrap();

        Self {
            root,
            explorer_home,
        }
    }
}

impl Drop for TestRepo {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

#[test]
fn test_clean_all_safety() {
    let repo = TestRepo::new("safety");

    // Create global dir
    let global_dir = repo.explorer_home.join(".codeexplorer");
    fs::create_dir_all(&global_dir).unwrap();

    // Create a precious directory
    let precious_dir = repo.root.join("precious");
    fs::create_dir_all(&precious_dir).unwrap();
    fs::write(precious_dir.join("secret.txt"), "secret data").unwrap();

    // Create a valid index directory
    let valid_repo_dir = repo.root.join("repo");
    let valid_index_dir = valid_repo_dir.join(".codeexplorer");
    fs::create_dir_all(&valid_index_dir).unwrap();
    fs::write(valid_index_dir.join("index.db"), "data").unwrap();

    // Create registry.json
    let registry_content = serde_json::json!([
        {
            "name": "dangerous_entry",
            "path": repo.root.display().to_string(),
            "storagePath": precious_dir.display().to_string(),
            "indexedAt": "2023-01-01T00:00:00Z",
            "lastCommit": "abc1234"
        },
        {
            "name": "global_alias_entry",
            "path": repo.root.display().to_string(),
            "storagePath": global_dir.join("..").join(".codeexplorer").display().to_string(),
            "indexedAt": "2023-01-01T00:00:00Z",
            "lastCommit": "alias123"
        },
        {
            "name": "valid_entry",
            "path": valid_repo_dir.display().to_string(),
            "storagePath": valid_index_dir.display().to_string(),
            "indexedAt": "2023-01-01T00:00:00Z",
            "lastCommit": "def5678"
        },
        {
            "name": "global_dir_entry",
            "path": repo.root.display().to_string(),
            "storagePath": global_dir.display().to_string(),
            "indexedAt": "2023-01-01T00:00:00Z",
            "lastCommit": "ghi9012"
        }
    ]);

    let registry_path = global_dir.join("registry.json");
    fs::write(
        &registry_path,
        serde_json::to_string(&registry_content).unwrap(),
    )
    .unwrap();

    let mut cmd = code_explorer();
    cmd.env("CODE_EXPLORER_HOME", &repo.explorer_home);
    cmd.arg("clean").arg("--all").arg("--force");

    let output = cmd.output().unwrap();

    // Output should indicate failure since some entries are invalid
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    println!("STDOUT:\n{}", stdout);
    println!("STDERR:\n{}", stderr);

    assert!(
        !output.status.success(),
        "Command should fail because of invalid storage paths"
    );

    // Check precious dir is still there
    assert!(precious_dir.exists(), "Precious directory was deleted!");
    assert!(
        precious_dir.join("secret.txt").exists(),
        "Secret file was deleted!"
    );

    // Check valid index dir was deleted
    assert!(
        !valid_index_dir.exists(),
        "Valid index directory was not deleted!"
    );

    // Check global dir is still there
    assert!(global_dir.exists(), "Global directory was deleted!");

    // Check registry content - should keep failed entries, remove successful ones
    let new_registry: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&registry_path).unwrap()).unwrap();
    let entries = new_registry.as_array().unwrap();

    assert_eq!(
        entries.len(),
        3,
        "Registry should retain every refused entry"
    );
    let names: Vec<&str> = entries
        .iter()
        .map(|e| e["name"].as_str().unwrap())
        .collect();
    assert!(
        names.contains(&"dangerous_entry"),
        "Registry should keep dangerous_entry"
    );
    assert!(
        names.contains(&"global_dir_entry"),
        "Registry should keep global_dir_entry"
    );
    assert!(
        names.contains(&"global_alias_entry"),
        "Registry should keep global_alias_entry"
    );
    assert!(
        !names.contains(&"valid_entry"),
        "Registry should not keep valid_entry"
    );
}

use std::fs;
use std::path::{Component, Path, PathBuf};

#[test]
fn repository_sources_do_not_contain_live_llm_api_keys() {
    let repo_root = workspace_root();
    let mut findings = Vec::new();
    scan_dir(&repo_root, &repo_root, &mut findings);

    assert!(
        findings.is_empty(),
        "Potential live LLM API keys found:\n{}",
        findings.join("\n")
    );
}

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("cli crate should live under crates/code-explorer-cli")
        .to_path_buf()
}

fn scan_dir(root: &Path, dir: &Path, findings: &mut Vec<String>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if !should_skip_dir(&path) {
                scan_dir(root, &path, findings);
            }
            continue;
        }
        if !should_scan_file(&path) {
            continue;
        }
        let Ok(content) = fs::read_to_string(&path) else {
            continue;
        };
        let rel = path
            .strip_prefix(root)
            .unwrap_or(&path)
            .display()
            .to_string();
        for token in suspicious_tokens(&content) {
            if !is_allowed_fixture(&token) {
                findings.push(format!("{rel}: {token}"));
            }
        }
    }
}

fn should_skip_dir(path: &Path) -> bool {
    path.components().any(|component| {
        let Component::Normal(name) = component else {
            return false;
        };
        matches!(
            name.to_string_lossy().as_ref(),
            ".git"
                | ".codeexplorer"
                | ".omx"
                | ".playwright-mcp"
                | "node_modules"
                | "target"
                | "target-codex"
                | "wiki"
        )
    })
}

fn should_scan_file(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
        return false;
    };
    if matches!(
        name,
        ".env"
            | ".env.local"
            | ".gitignore"
            | "AGENTS.md"
            | "Cargo.toml"
            | "package.json"
            | "tsconfig.json"
    ) {
        return true;
    }
    matches!(
        path.extension().and_then(|e| e.to_str()),
        Some(
            "rs" | "toml"
                | "json"
                | "md"
                | "ts"
                | "tsx"
                | "js"
                | "jsx"
                | "yml"
                | "yaml"
                | "ps1"
                | "sh"
        )
    )
}

fn suspicious_tokens(content: &str) -> Vec<String> {
    let mut out = Vec::new();
    out.extend(
        tokens_with_prefix(content, "sk-")
            .into_iter()
            .filter(|token| {
                token.starts_with("sk-proj-") && token.len() >= 24 || token.len() >= 32
            }),
    );
    out.extend(
        tokens_with_prefix(content, "AIza")
            .into_iter()
            .filter(|token| token.len() >= 24),
    );
    out
}

fn tokens_with_prefix(content: &str, prefix: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut start_at = 0;
    while let Some(offset) = content[start_at..].find(prefix) {
        let start = start_at + offset;
        let mut end = start + prefix.len();
        for ch in content[end..].chars() {
            if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_') {
                end += ch.len_utf8();
            } else {
                break;
            }
        }
        tokens.push(content[start..end].to_string());
        start_at = end;
    }
    tokens
}

fn is_allowed_fixture(token: &str) -> bool {
    matches!(token, "sk-proj-1234567890abcdef" | "sk-test-secret") || token.contains("example")
}

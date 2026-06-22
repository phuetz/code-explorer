//! The `mcp-install` command: auto-configure Code Explorer as an MCP server.

use std::path::PathBuf;

pub fn run(scope: &str, client: &str) -> anyhow::Result<()> {
    let exe_path = std::env::current_exe()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| "code-explorer".to_string());

    match client {
        "claude" | "claude-code" => install_claude(scope, &exe_path),
        "claude-desktop" => install_claude_desktop(scope, &exe_path),
        "codex" => install_codex(scope, &exe_path),
        "cursor" => install_cursor(scope, &exe_path),
        "vscode" | "vs-code" => install_vscode(scope, &exe_path),
        "both" => {
            install_claude(scope, &exe_path)?;
            println!();
            install_codex("global", &exe_path)
        }
        "all" => {
            install_claude(scope, &exe_path)?;
            println!();
            install_codex("global", &exe_path)?;
            println!();
            install_cursor(scope, &exe_path)?;
            println!();
            install_vscode(scope, &exe_path)
        }
        other => Err(anyhow::anyhow!(
            "unsupported client '{other}'. Use --client claude, codex, claude-desktop, cursor, vscode, both, or all"
        )),
    }
}

fn install_claude(scope: &str, exe_path: &str) -> anyhow::Result<()> {
    match scope {
        "global" => install_claude_global(exe_path),
        "project" => install_claude_project(exe_path),
        other => Err(anyhow::anyhow!(
            "unsupported scope '{other}'. Use --scope project or global"
        )),
    }
}

fn install_claude_project(exe_path: &str) -> anyhow::Result<()> {
    let mcp_json_path = PathBuf::from(".mcp.json");

    let config = build_claude_mcp_config(exe_path, &mcp_json_path)?;
    std::fs::write(&mcp_json_path, config)?;

    println!("Code Explorer MCP server configured for Claude Code (project scope).");
    println!("  Created: {}", mcp_json_path.display());
    println!();
    println!("Restart Claude Code to pick up the new MCP server.");

    Ok(())
}

fn install_claude_global(exe_path: &str) -> anyhow::Result<()> {
    let home = std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .map_err(|_| anyhow::anyhow!("Cannot determine home directory"))?;

    let mcp_json_path = PathBuf::from(&home).join(".mcp.json");

    let config = build_claude_mcp_config(exe_path, &mcp_json_path)?;
    std::fs::write(&mcp_json_path, config)?;

    println!("Code Explorer MCP server configured for Claude Code (global scope).");
    println!("  Created: {}", mcp_json_path.display());
    println!();
    println!("Restart Claude Code to pick up the new MCP server.");

    Ok(())
}

fn install_codex(scope: &str, exe_path: &str) -> anyhow::Result<()> {
    match scope {
        "global" => install_codex_global(exe_path),
        "project" => Err(anyhow::anyhow!(
            "Codex CLI does not auto-load project-local MCP config. Use --client codex --scope global"
        )),
        other => Err(anyhow::anyhow!(
            "unsupported scope '{other}'. Use --scope global for Codex"
        )),
    }
}

fn install_codex_global(exe_path: &str) -> anyhow::Result<()> {
    let config_path = codex_home_dir()?.join("config.toml");
    install_codex_config(exe_path, &config_path, "global")
}

fn install_codex_config(exe_path: &str, config_path: &PathBuf, scope: &str) -> anyhow::Result<()> {
    if let Some(parent) = config_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let existing = if config_path.exists() {
        std::fs::read_to_string(config_path)?
    } else {
        String::new()
    };
    let next = build_codex_config(exe_path, &existing)?;
    std::fs::write(config_path, next)?;

    println!("Code Explorer MCP server configured for Codex ({scope} scope).");
    println!("  Updated: {}", config_path.display());
    println!();
    println!("Restart Codex to pick up the new MCP server.");

    Ok(())
}

fn install_claude_desktop(scope: &str, exe_path: &str) -> anyhow::Result<()> {
    if scope != "global" {
        return Err(anyhow::anyhow!(
            "Claude Desktop only supports --scope global because it reads a user config file"
        ));
    }

    let config_path = claude_desktop_config_path()?;
    install_json_mcp_servers_config(
        exe_path,
        &config_path,
        JsonMcpFormat::McpServers,
        "Claude Desktop",
        scope,
    )
}

fn install_cursor(scope: &str, exe_path: &str) -> anyhow::Result<()> {
    let config_path = match scope {
        "project" => PathBuf::from(".cursor").join("mcp.json"),
        "global" => home_dir()?.join(".cursor").join("mcp.json"),
        other => {
            return Err(anyhow::anyhow!(
                "unsupported scope '{other}'. Use --scope project or global"
            ));
        }
    };

    install_json_mcp_servers_config(
        exe_path,
        &config_path,
        JsonMcpFormat::McpServers,
        "Cursor",
        scope,
    )
}

fn install_vscode(scope: &str, exe_path: &str) -> anyhow::Result<()> {
    let config_path = match scope {
        "project" => PathBuf::from(".vscode").join("mcp.json"),
        "global" => {
            return Err(anyhow::anyhow!(
                "VS Code global MCP config lives in the active VS Code profile; use --scope project or the VS Code 'MCP: Add Server' command for user scope"
            ));
        }
        other => {
            return Err(anyhow::anyhow!(
                "unsupported scope '{other}'. Use --scope project"
            ));
        }
    };

    install_json_mcp_servers_config(
        exe_path,
        &config_path,
        JsonMcpFormat::Servers,
        "VS Code",
        scope,
    )
}

fn install_json_mcp_servers_config(
    exe_path: &str,
    config_path: &PathBuf,
    format: JsonMcpFormat,
    client_name: &str,
    scope: &str,
) -> anyhow::Result<()> {
    if let Some(parent) = config_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let config = build_json_mcp_config(exe_path, config_path, format)?;
    std::fs::write(config_path, config)?;

    println!("Code Explorer MCP server configured for {client_name} ({scope} scope).");
    println!("  Updated: {}", config_path.display());
    println!();
    println!("Restart {client_name} to pick up the new MCP server.");

    Ok(())
}

#[derive(Clone, Copy)]
enum JsonMcpFormat {
    McpServers,
    Servers,
}

/// Build the .mcp.json content, merging with existing config if present.
fn build_claude_mcp_config(exe_path: &str, mcp_json_path: &PathBuf) -> anyhow::Result<String> {
    build_json_mcp_config(exe_path, mcp_json_path, JsonMcpFormat::McpServers)
}

fn build_json_mcp_config(
    exe_path: &str,
    config_path: &PathBuf,
    format: JsonMcpFormat,
) -> anyhow::Result<String> {
    let mut config: serde_json::Value = if config_path.exists() {
        let content = std::fs::read_to_string(config_path)?;
        serde_json::from_str(&content).unwrap_or_else(|_| serde_json::json!({}))
    } else {
        serde_json::json!({})
    };

    let root_key = match format {
        JsonMcpFormat::McpServers => "mcpServers",
        JsonMcpFormat::Servers => "servers",
    };

    if config.get(root_key).is_none() {
        config[root_key] = serde_json::json!({});
    }

    let mut server = serde_json::json!({
        "command": exe_path,
        "args": ["mcp"]
    });
    if matches!(format, JsonMcpFormat::Servers) {
        server["type"] = serde_json::json!("stdio");
    }

    config[root_key]["code-explorer"] = server;

    let formatted = serde_json::to_string_pretty(&config)?;
    Ok(formatted)
}

fn build_codex_config(exe_path: &str, existing: &str) -> anyhow::Result<String> {
    let mut content = remove_toml_table(existing, "mcp_servers.code_explorer");
    trim_trailing_blank_lines(&mut content);

    let command = serde_json::to_string(exe_path)?;
    if !content.is_empty() {
        content.push_str("\n\n");
    }
    content.push_str("# Code Explorer MCP server\n");
    content.push_str("[mcp_servers.code_explorer]\n");
    content.push_str(&format!("command = {command}\n"));
    content.push_str("args = [\"mcp\"]\n");
    content.push_str("enabled = true\n");
    content.push_str("startup_timeout_sec = 30\n");

    Ok(content)
}

fn remove_toml_table(existing: &str, table_name: &str) -> String {
    let table_header = format!("[{table_name}]");
    let mut output = Vec::new();
    let mut skipping = false;

    for line in existing.lines() {
        let trimmed = line.trim();
        if trimmed == table_header {
            skipping = true;
            continue;
        }

        if skipping && trimmed.starts_with('[') && trimmed.ends_with(']') {
            skipping = false;
        }

        if !skipping {
            output.push(line);
        }
    }

    output.join("\n")
}

fn trim_trailing_blank_lines(content: &mut String) {
    while content.ends_with('\n') || content.ends_with('\r') {
        content.pop();
    }
}

fn home_dir() -> anyhow::Result<PathBuf> {
    std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .map(PathBuf::from)
        .map_err(|_| anyhow::anyhow!("Cannot determine home directory"))
}

fn codex_home_dir() -> anyhow::Result<PathBuf> {
    if let Ok(codex_home) = std::env::var("CODEX_HOME") {
        return Ok(PathBuf::from(codex_home));
    }
    Ok(home_dir()?.join(".codex"))
}

fn claude_desktop_config_path() -> anyhow::Result<PathBuf> {
    #[cfg(target_os = "windows")]
    {
        let appdata = std::env::var("APPDATA")
            .map_err(|_| anyhow::anyhow!("Cannot determine APPDATA directory"))?;
        Ok(PathBuf::from(appdata)
            .join("Claude")
            .join("claude_desktop_config.json"))
    }

    #[cfg(target_os = "macos")]
    {
        Ok(home_dir()?
            .join("Library")
            .join("Application Support")
            .join("Claude")
            .join("claude_desktop_config.json"))
    }

    #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
    {
        Ok(home_dir()?
            .join(".config")
            .join("Claude")
            .join("claude_desktop_config.json"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_claude_mcp_config_new() {
        let config = build_claude_mcp_config(
            "code-explorer.exe",
            &PathBuf::from("/nonexistent/.mcp.json"),
        )
        .expect("should build config");
        let parsed: serde_json::Value = serde_json::from_str(&config).unwrap();
        assert_eq!(
            parsed["mcpServers"]["code-explorer"]["command"],
            "code-explorer.exe"
        );
        assert_eq!(parsed["mcpServers"]["code-explorer"]["args"][0], "mcp");
    }

    #[test]
    fn test_build_vscode_mcp_config_new() {
        let config = build_json_mcp_config(
            "code-explorer.exe",
            &PathBuf::from("/nonexistent/mcp.json"),
            JsonMcpFormat::Servers,
        )
        .expect("should build config");
        let parsed: serde_json::Value = serde_json::from_str(&config).unwrap();
        assert_eq!(
            parsed["servers"]["code-explorer"]["command"],
            "code-explorer.exe"
        );
        assert_eq!(parsed["servers"]["code-explorer"]["type"], "stdio");
        assert_eq!(parsed["servers"]["code-explorer"]["args"][0], "mcp");
    }

    #[test]
    fn test_build_codex_config_new() {
        let config = build_codex_config("C:\\tools\\code-explorer.exe", "").unwrap();
        assert!(config.contains("[mcp_servers.code_explorer]"));
        assert!(config.contains("command = \"C:\\\\tools\\\\code-explorer.exe\""));
        assert!(config.contains("args = [\"mcp\"]"));
    }

    #[test]
    fn test_build_codex_config_replaces_existing_table() {
        let existing = r#"model = "gpt-5"

[mcp_servers.code_explorer]
command = "old"
args = ["mcp"]

[mcp_servers.other]
command = "node"
"#;
        let config = build_codex_config("new-code-explorer", existing).unwrap();
        assert_eq!(config.matches("[mcp_servers.code_explorer]").count(), 1);
        assert!(config.contains("model = \"gpt-5\""));
        assert!(config.contains("command = \"new-code-explorer\""));
        assert!(config.contains("[mcp_servers.other]"));
        assert!(!config.contains("command = \"old\""));
    }
}

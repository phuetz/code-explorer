//! The `mcp-install` command: auto-configure Code Explorer as an MCP server for Claude Code.

use std::path::PathBuf;

pub fn run(scope: &str) -> anyhow::Result<()> {
    let exe_path = std::env::current_exe()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| "code-explorer".to_string());

    match scope {
        "global" => install_global(&exe_path),
        _ => install_project(&exe_path),
    }
}

fn install_project(exe_path: &str) -> anyhow::Result<()> {
    let mcp_json_path = PathBuf::from(".mcp.json");

    let config = build_mcp_config(exe_path, &mcp_json_path)?;
    std::fs::write(&mcp_json_path, config)?;

    println!("Code Explorer MCP server configured for Claude Code (project scope).");
    println!("  Created: {}", mcp_json_path.display());
    println!();
    println!("Restart Claude Code to pick up the new MCP server.");

    Ok(())
}

fn install_global(exe_path: &str) -> anyhow::Result<()> {
    let home = std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .map_err(|_| anyhow::anyhow!("Cannot determine home directory"))?;

    let mcp_json_path = PathBuf::from(&home).join(".mcp.json");

    let config = build_mcp_config(exe_path, &mcp_json_path)?;
    std::fs::write(&mcp_json_path, config)?;

    println!("Code Explorer MCP server configured for Claude Code (global scope).");
    println!("  Created: {}", mcp_json_path.display());
    println!();
    println!("Restart Claude Code to pick up the new MCP server.");

    Ok(())
}

/// Build the .mcp.json content, merging with existing config if present.
fn build_mcp_config(exe_path: &str, mcp_json_path: &PathBuf) -> anyhow::Result<String> {
    let mut config: serde_json::Value = if mcp_json_path.exists() {
        let content = std::fs::read_to_string(mcp_json_path)?;
        serde_json::from_str(&content).unwrap_or_else(|_| serde_json::json!({}))
    } else {
        serde_json::json!({})
    };

    // Ensure mcpServers object exists
    if config.get("mcpServers").is_none() {
        config["mcpServers"] = serde_json::json!({});
    }

    // Add/update the code-explorer server entry
    config["mcpServers"]["code-explorer"] = serde_json::json!({
        "command": exe_path,
        "args": ["mcp"]
    });

    let formatted = serde_json::to_string_pretty(&config)?;
    Ok(formatted)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_mcp_config_new() {
        let config = build_mcp_config("code-explorer.exe", &PathBuf::from("/nonexistent/.mcp.json"))
            .expect("should build config");
        let parsed: serde_json::Value = serde_json::from_str(&config).unwrap();
        assert_eq!(
            parsed["mcpServers"]["code-explorer"]["command"],
            "code-explorer.exe"
        );
        assert_eq!(parsed["mcpServers"]["code-explorer"]["args"][0], "mcp");
    }
}

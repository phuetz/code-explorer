//! The `setup` command: configure MCP for supported editors.

use std::path::PathBuf;

pub fn run() -> anyhow::Result<()> {
    println!("Code Explorer MCP Setup");
    println!();

    // Detect available editors
    let editors = detect_editors();

    if editors.is_empty() {
        println!("No supported editors detected.");
        println!();
        println!("Manual setup:");
        println!("  Add the following to your MCP client configuration:");
        println!();
        print_mcp_config();
        return Ok(());
    }

    println!("Detected editors:");
    for (name, config_path) in &editors {
        println!("  - {name}: {}", config_path.display());
    }
    println!();

    // For each detected editor, show how to configure
    for (name, config_path) in &editors {
        println!("--- {name} ---");
        println!("Add this to: {}", config_path.display());
        println!();
        print_mcp_config();
        println!();
    }

    println!("After updating the configuration, restart your editor.");

    Ok(())
}

fn print_mcp_config() {
    let exe_path = std::env::current_exe()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| "code-explorer".to_string());

    // Escape backslashes and quotes for valid JSON output
    let escaped = exe_path.replace('\\', "\\\\").replace('"', "\\\"");

    println!(
        r#"{{
  "mcpServers": {{
    "code-explorer": {{
      "command": "{escaped}",
      "args": ["mcp"],
      "env": {{}}
    }}
  }}
}}"#
    );
}

fn detect_editors() -> Vec<(&'static str, PathBuf)> {
    let mut editors = Vec::new();

    #[cfg(target_os = "windows")]
    {
        if let Ok(appdata) = std::env::var("APPDATA") {
            let vscode_path = PathBuf::from(&appdata)
                .join("Code")
                .join("User")
                .join("settings.json");
            if vscode_path.parent().map(|p| p.exists()).unwrap_or(false) {
                editors.push(("VS Code", vscode_path));
            }

            let cursor_path = PathBuf::from(&appdata)
                .join("Cursor")
                .join("User")
                .join("settings.json");
            if cursor_path.parent().map(|p| p.exists()).unwrap_or(false) {
                editors.push(("Cursor", cursor_path));
            }
        }
    }

    #[cfg(not(target_os = "windows"))]
    {
        if let Ok(home) = std::env::var("HOME") {
            let vscode_path = PathBuf::from(&home)
                .join(".config")
                .join("Code")
                .join("User")
                .join("settings.json");
            if vscode_path.parent().map(|p| p.exists()).unwrap_or(false) {
                editors.push(("VS Code", vscode_path));
            }

            let cursor_path = PathBuf::from(&home)
                .join(".config")
                .join("Cursor")
                .join("User")
                .join("settings.json");
            if cursor_path.parent().map(|p| p.exists()).unwrap_or(false) {
                editors.push(("Cursor", cursor_path));
            }
        }
    }

    editors
}

//! Shared, offline Playwright/Chromium renderer for PDF and DOCX diagrams.
use anyhow::{bail, Context, Result};
use std::path::Path;
use std::process::Command;

const PRINT_JS: &str = include_str!("generate/print-pdf.js");

pub(crate) fn render_pdf(html: &Path, output: &Path) -> Result<()> {
    run(html, output, false)
}

pub(crate) fn render_mermaid_png(source: &str) -> Result<Vec<u8>> {
    let temp = tempfile::tempdir()?;
    let input = temp.path().join("diagram.json");
    let output = temp.path().join("diagram.png");
    std::fs::write(&input, serde_json::to_vec(source)?)?;
    run(&input, &output, true)?;
    Ok(std::fs::read(output)?)
}

fn run(input: &Path, output: &Path, mermaid: bool) -> Result<()> {
    let temp = tempfile::tempdir()?;
    let script = temp.path().join("print-pdf.cjs");
    std::fs::write(&script, PRINT_JS)?;
    if mermaid {
        std::fs::write(
            temp.path().join("mermaid.min.js"),
            code_explorer_output::assets::MERMAID_JS,
        )?;
    }
    let node = find_node()?;
    let mut command = Command::new(&node);
    command.arg(&script).arg(input).arg(output);
    if mermaid {
        command.arg("--mermaid");
    }
    // Respect an explicitly provided NODE_PATH before falling back to npm's global modules.
    if std::env::var_os("NODE_PATH").is_none() {
        if let Some(path) = find_global_node_modules() {
            command.env("NODE_PATH", path);
        }
    }
    let result = command
        .output()
        .context("Cannot start the local Playwright renderer")?;
    if !result.status.success() {
        bail!("Local Mermaid/PDF renderer unavailable or failed: {}. Install Node.js, Playwright and its Chromium browser locally.", String::from_utf8_lossy(&result.stderr).trim());
    }
    Ok(())
}

fn find_node() -> Result<String> {
    // Try 'node' in PATH
    let check = if cfg!(windows) {
        Command::new("where").arg("node").output()
    } else {
        Command::new("which").arg("node").output()
    };

    match check {
        Ok(output) if output.status.success() => Ok("node".to_string()),
        _ => {
            // On Windows, try common paths
            if cfg!(windows) {
                let common_paths = [
                    r"C:\Program Files\nodejs\node.exe",
                    r"C:\Program Files (x86)\nodejs\node.exe",
                ];
                for path in &common_paths {
                    if Path::new(path).exists() {
                        return Ok(path.to_string());
                    }
                }
            }
            bail!(
                "Node.js not found. PDF generation requires Node.js.\n\
                 Install from: https://nodejs.org/\n\
                 Then install Playwright: npm install -g playwright && npx playwright install chromium"
            )
        }
    }
}

/// Discover the global node_modules directory for NODE_PATH.
fn find_global_node_modules() -> Option<String> {
    let npm_cmd = if cfg!(windows) { "npm.cmd" } else { "npm" };
    let output = Command::new(npm_cmd).args(["root", "-g"]).output().ok()?;
    if output.status.success() {
        let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if !path.is_empty() && Path::new(&path).exists() {
            return Some(path);
        }
    }
    None
}

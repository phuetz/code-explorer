//! `code-explorer generate inject` — inject fragments into existing documentation pages.
//!
//! Patches .md files in-place using anchor comments, without regenerating anything.
//! Idempotent: anchors with existing :END sentinels are replaced, not duplicated.
//!
//! Anchor formats in markdown files:
//!   <!-- GNX:IMAGE:<key> -->          — insert an image
//!   <!-- GNX:FRAGMENT:<key> -->       — insert arbitrary markdown
//!
//! Manifest format (inject.json at repo root or passed via --manifest):
//! ```json
//! {
//!   "fragments": [
//!     {
//!       "page": "modules/gestionplafonds",
//!       "anchor": "GNX:IMAGE:schema",
//!       "type": "image",
//!       "source": "docs/assets/schema.png",
//!       "alt": "Schéma calcul plafonds"
//!     },
//!     {
//!       "page": "modules/elodie",
//!       "anchor": "GNX:FRAGMENT:flux3",
//!       "type": "markdown",
//!       "content": "## Flux 3\n\nExplication..."
//!     },
//!     {
//!       "page": "modules/elodie",
//!       "anchor": "GNX:FRAGMENT:diagram",
//!       "type": "mermaid",
//!       "content": "graph LR\n  A --> B"
//!     }
//!   ]
//! }
//! ```

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context, Result};
use colored::Colorize;
use serde::Deserialize;

// ─── Manifest types ──────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct InjectManifest {
    pub fragments: Vec<Fragment>,
}

#[derive(Debug, Deserialize)]
pub struct Fragment {
    /// Relative page id, e.g. "modules/gestionplafonds" or "architecture"
    pub page: String,
    /// Anchor key, e.g. "GNX:IMAGE:schema" or "GNX:FRAGMENT:algo"
    pub anchor: String,
    /// "image" | "markdown" | "mermaid"
    #[serde(rename = "type")]
    pub kind: String,
    /// For image: relative path to the image file (relative to docs_dir)
    pub source: Option<String>,
    /// Alt text for images
    pub alt: Option<String>,
    /// For markdown/mermaid: raw content to inject
    pub content: Option<String>,
}

// ─── Public entry point ───────────────────────────────────────────────────────

/// Apply all fragments from the manifest to the docs directory.
/// Returns the number of injections applied.
pub fn apply_inject(docs_dir: &Path, manifest_path: &Path) -> Result<usize> {
    let raw = fs::read_to_string(manifest_path)
        .with_context(|| format!("Cannot read manifest: {}", manifest_path.display()))?;
    let manifest: InjectManifest =
        serde_json::from_str(&raw).with_context(|| "Invalid manifest JSON")?;

    // Group fragments by page for efficient file-at-a-time patching
    let mut by_page: HashMap<String, Vec<&Fragment>> = HashMap::new();
    for frag in &manifest.fragments {
        by_page.entry(frag.page.clone()).or_default().push(frag);
    }

    let mut total = 0usize;
    for (page, fragments) in &by_page {
        let md_path = resolve_page(docs_dir, page);
        if !md_path.exists() {
            eprintln!(
                "{} Page not found, skipping: {}",
                "WARN".yellow(),
                md_path.display()
            );
            continue;
        }
        let count = patch_file(&md_path, fragments)
            .with_context(|| format!("Failed to patch {}", md_path.display()))?;
        if count > 0 {
            println!(
                "  {} {} ({} injection{})",
                "OK".green(),
                page,
                count,
                if count == 1 { "" } else { "s" }
            );
            total += count;
        }
    }

    Ok(total)
}

// ─── File patching ────────────────────────────────────────────────────────────

fn resolve_page(docs_dir: &Path, page: &str) -> PathBuf {
    // page is relative to docs_dir, e.g. "modules/gestionplafonds" → .../modules/gestionplafonds.md
    docs_dir.join(format!("{}.md", page))
}

/// Patch a single .md file by injecting all matching fragments.
/// Returns the number of anchors that were patched.
fn patch_file(path: &Path, fragments: &[&Fragment]) -> Result<usize> {
    let original = fs::read_to_string(path)?;
    let mut patched = String::with_capacity(original.len() + 2048);
    let mut count = 0usize;

    let lines: Vec<&str> = original.lines().collect();
    let mut i = 0;

    while i < lines.len() {
        let line = lines[i];
        let trimmed = line.trim();

        // Detect anchor comment: <!-- GNX:<type>:<key> -->
        if let Some(anchor_key) = parse_anchor(trimmed) {
            // Find a matching fragment
            if let Some(frag) = fragments.iter().find(|f| f.anchor == anchor_key) {
                // Write the anchor line itself
                patched.push_str(line);
                patched.push('\n');

                // Build the injection content
                let injection = build_injection(frag)?;
                let end_sentinel = format!("<!-- {}:END -->", anchor_key);

                // Skip existing injected content up to the :END sentinel (idempotent)
                i += 1;
                while i < lines.len() {
                    if lines[i].trim() == end_sentinel {
                        i += 1; // skip the old :END line too
                        break;
                    }
                    i += 1;
                }

                // Write new injection + :END sentinel
                patched.push_str(&injection);
                patched.push('\n');
                patched.push_str(&end_sentinel);
                patched.push('\n');
                count += 1;
                continue;
            }
        }

        patched.push_str(line);
        patched.push('\n');
        i += 1;
    }

    // Preserve trailing newline behaviour of original
    if !original.ends_with('\n') && patched.ends_with('\n') {
        patched.pop();
    }

    if patched != original {
        // Atomic write via temp file
        let tmp = path.with_extension("inject.tmp");
        fs::write(&tmp, &patched)?;
        fs::rename(&tmp, path)?;
    }

    Ok(count)
}

/// Parse `<!-- GNX:<key> -->` and return the key, or None if not an anchor.
fn parse_anchor(line: &str) -> Option<String> {
    let line = line.trim();
    if !line.starts_with("<!-- GNX:") || !line.ends_with(" -->") {
        return None;
    }
    // Don't match :END sentinels
    if line.contains(":END -->") {
        return None;
    }
    let inner = &line[5..line.len() - 4]; // strip "<!-- " and " -->"
    Some(inner.trim().to_string())
}

/// Build the markdown/HTML content to inject after an anchor.
fn build_injection(frag: &Fragment) -> Result<String> {
    match frag.kind.as_str() {
        "image" => {
            let src = frag
                .source
                .as_deref()
                .ok_or_else(|| anyhow!("image fragment missing 'source'"))?;
            let alt = frag.alt.as_deref().unwrap_or("Image");
            // Use <div> wrapper so the custom markdown-to-html parser passes it through
            Ok(format!("\n<div class=\"gnx-inject-image\">\n<img src=\"{}\" alt=\"{}\" style=\"max-width:100%;border-radius:8px;\"/>\n</div>\n", src, html_escape_attr(alt)))
        }
        "markdown" => {
            let content = frag
                .content
                .as_deref()
                .ok_or_else(|| anyhow!("markdown fragment missing 'content'"))?;
            Ok(format!("\n{}\n", content))
        }
        "mermaid" => {
            let content = frag
                .content
                .as_deref()
                .ok_or_else(|| anyhow!("mermaid fragment missing 'content'"))?;
            Ok(format!("\n```mermaid\n{}\n```\n", content))
        }
        other => Err(anyhow!(
            "Unknown fragment type: '{}'. Expected image, markdown, or mermaid.",
            other
        )),
    }
}

fn html_escape_attr(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('"', "&quot;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

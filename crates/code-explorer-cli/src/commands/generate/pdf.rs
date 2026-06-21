//! PDF generation: Markdown → self-contained HTML → PDF via Playwright/Chromium.
//!
//! Two modes:
//! - **Standalone** (`--input`): reads Markdown file(s) directly, no knowledge graph needed.
//! - **Knowledge graph**: reads docs from `.codeexplorer/docs/`, same pipeline as `generate docx`.

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{bail, Context, Result};
use colored::Colorize;
use tracing::info;

use super::markdown::{html_escape, markdown_to_html};

// ─── Embedded print-pdf.js ────────────────────────────────────────────────
const PRINT_PDF_JS: &str = include_str!("print-pdf.js");

// ─── Public entry points ──────────────────────────────────────────────────

/// Generate PDF from knowledge graph docs (same flow as docx).
pub(super) fn generate_pdf_from_docs(
    docs_dir: &Path,
    output_path: &Path,
    project_name: &str,
) -> Result<()> {
    let md_files = collect_md_from_docs_dir(docs_dir)?;
    if md_files.is_empty() {
        bail!("No Markdown files found in {}", docs_dir.display());
    }

    let metadata = PdfMetadata {
        title: project_name.to_string(),
        subtitle: "Documentation Technique et Fonctionnelle".to_string(),
        version: String::new(),
        date: chrono_date(),
        author: String::new(),
    };

    generate_pdf(&md_files, output_path, &metadata)
}

/// Generate PDF from a standalone Markdown file or directory.
pub(super) fn generate_pdf_from_input(input_path: &Path, output_path: &Path) -> Result<()> {
    let md_files = if input_path.is_dir() {
        collect_md_from_directory(input_path)?
    } else {
        let content = std::fs::read_to_string(input_path)
            .with_context(|| format!("Cannot read {}", input_path.display()))?;
        let name = input_path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("document")
            .to_string();
        vec![(name, input_path.display().to_string(), content)]
    };

    if md_files.is_empty() {
        bail!("No Markdown files found at {}", input_path.display());
    }

    // Extract metadata from the first document
    let metadata = extract_metadata(&sanitize_pdf_markdown(&md_files[0].2));

    generate_pdf(&md_files, output_path, &metadata)
}

// ─── Core pipeline ────────────────────────────────────────────────────────

struct PdfMetadata {
    title: String,
    subtitle: String,
    version: String,
    date: String,
    author: String,
}

/// Main pipeline: Markdown files → HTML → Playwright → PDF.
fn generate_pdf(
    md_files: &[(String, String, String)], // (name, path, content)
    output_path: &Path,
    metadata: &PdfMetadata,
) -> Result<()> {
    println!(
        "{} Generating PDF from {} document(s)...",
        ">>".blue(),
        md_files.len()
    );

    // Step 1: Convert each markdown to HTML body fragments
    let mut body_sections = Vec::new();
    for (name, _path, content) in md_files {
        let sanitized = sanitize_pdf_markdown(content);
        let section_html = markdown_to_html(&sanitized);
        body_sections.push((name.clone(), section_html));
    }

    // Step 2: Build TOC and inject heading IDs
    let mut full_body = String::new();
    let mut toc_entries: Vec<TocEntry> = Vec::new();
    let mut heading_counter: u32 = 0;

    for (_name, section_html) in &body_sections {
        let (processed, entries) =
            inject_heading_ids_and_collect(section_html, &mut heading_counter);
        toc_entries.extend(entries);
        full_body.push_str(&processed);
    }

    // Step 3: Build the complete HTML document
    let toc_html = build_toc_html(&toc_entries);
    let html = build_full_html(metadata, &toc_html, &full_body);

    // Step 4: Write HTML to temp file
    let temp_dir = std::env::temp_dir();
    let html_path = temp_dir.join("code-explorer-pdf-temp.html");
    std::fs::write(&html_path, &html)
        .with_context(|| format!("Cannot write temp HTML to {}", html_path.display()))?;

    info!("Wrote intermediate HTML to {}", html_path.display());

    // Step 5: Run Playwright to convert HTML → PDF
    let result = run_playwright(&html_path, output_path);

    // Step 6: Cleanup temp file
    let _ = std::fs::remove_file(&html_path);

    result?;

    let file_size = std::fs::metadata(output_path).map(|m| m.len()).unwrap_or(0);
    let size_str = if file_size > 1_048_576 {
        format!("{:.1} Mo", file_size as f64 / 1_048_576.0)
    } else {
        format!("{} Ko", file_size / 1024)
    };

    println!(
        "{} Generated PDF: {} ({})",
        "OK".green(),
        output_path.display(),
        size_str
    );

    Ok(())
}

// ─── Markdown collection ──────────────────────────────────────────────────

/// Collect .md files from a directory, sorted by filename.
fn collect_md_from_directory(dir: &Path) -> Result<Vec<(String, String, String)>> {
    let mut entries: Vec<PathBuf> = std::fs::read_dir(dir)?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|ext| ext == "md"))
        .collect();
    entries.sort();

    let mut files = Vec::new();
    for path in entries {
        let content = std::fs::read_to_string(&path)?;
        let name = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("doc")
            .to_string();
        files.push((name, path.display().to_string(), content));
    }
    Ok(files)
}

/// Collect .md files from a .codeexplorer/docs directory using _index.json if present.
fn collect_md_from_docs_dir(docs_dir: &Path) -> Result<Vec<(String, String, String)>> {
    let index_path = docs_dir.join("_index.json");
    if index_path.exists() {
        // Use index for ordering
        let index_content = std::fs::read_to_string(&index_path)?;
        let index: serde_json::Value = serde_json::from_str(&index_content)?;
        let mut files = Vec::new();
        if let Some(pages) = index.get("pages").and_then(|p| p.as_array()) {
            collect_pages_recursive(docs_dir, pages, &mut files)?;
        }
        if !files.is_empty() {
            return Ok(files);
        }
    }
    // Fallback: alphabetical
    collect_md_from_directory(docs_dir)
}

fn collect_pages_recursive(
    docs_dir: &Path,
    pages: &[serde_json::Value],
    out: &mut Vec<(String, String, String)>,
) -> Result<()> {
    for page in pages {
        if let Some(path_str) = page.get("path").and_then(|p| p.as_str()) {
            let md_path = docs_dir.join(path_str);
            if md_path.exists() {
                let content = std::fs::read_to_string(&md_path)?;
                let id = page
                    .get("id")
                    .and_then(|i| i.as_str())
                    .unwrap_or(path_str)
                    .to_string();
                out.push((id, md_path.display().to_string(), content));
            }
        }
        if let Some(children) = page.get("children").and_then(|c| c.as_array()) {
            collect_pages_recursive(docs_dir, children, out)?;
        }
    }
    Ok(())
}

// ─── Metadata extraction ──────────────────────────────────────────────────

fn extract_metadata(content: &str) -> PdfMetadata {
    let mut title = String::new();
    let mut subtitle = String::new();
    let mut version = String::new();
    let mut date = chrono_date();
    let mut author = String::new();
    let mut found_first_h1 = false;

    for line in content.lines().take(50) {
        let trimmed = line.trim();

        // Extract titles from # headings
        if let Some(rest) = trimmed.strip_prefix("# ") {
            if !found_first_h1 {
                title = rest.trim().to_string();
                found_first_h1 = true;
            } else if subtitle.is_empty() {
                subtitle = rest.trim().to_string();
            }
            continue;
        }

        // Extract metadata from cartouche table rows: | **Key** | Value |
        if trimmed.starts_with('|') && trimmed.contains("**") {
            let lower = trimmed.to_lowercase();
            let value = extract_table_value(trimmed);
            if lower.contains("version") && version.is_empty() {
                version = value;
            } else if lower.contains("date") {
                date = value;
            } else if lower.contains("auteur") || lower.contains("author") {
                author = value;
            }
        }
    }

    if title.is_empty() {
        title = "Document".to_string();
    }

    PdfMetadata {
        title,
        subtitle,
        version,
        date,
        author,
    }
}

/// Extract the value cell from a table row like `| **Key** | Value |`.
fn extract_table_value(row: &str) -> String {
    let cells: Vec<&str> = row.split('|').collect();
    if cells.len() >= 3 {
        // cells[0] is empty (before first |), cells[1] is key, cells[2] is value
        cells[2]
            .trim()
            .trim_start_matches("**")
            .trim_end_matches("**")
            .trim()
            .to_string()
    } else {
        String::new()
    }
}

fn sanitize_pdf_markdown(content: &str) -> String {
    content
        .replace(['\u{200B}', '\u{200C}', '\u{200D}', '\u{FE0F}'], "")
}

fn chrono_date() -> String {
    // Simple date without chrono dependency
    let now = std::time::SystemTime::now();
    let since_epoch = now
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let days = since_epoch.as_secs() / 86400;
    // Approximate date calculation (good enough for display)
    let year = 1970 + (days / 365);
    let remaining = days % 365;
    let month = remaining / 30 + 1;
    let day = remaining % 30 + 1;
    format!("{:02}/{:02}/{}", day.min(28), month.min(12), year)
}

// ─── Heading IDs and TOC ──────────────────────────────────────────────────

struct TocEntry {
    level: u8,
    id: String,
    text: String,
}

/// Inject `id` attributes on heading tags and collect TOC entries.
fn inject_heading_ids_and_collect(html: &str, counter: &mut u32) -> (String, Vec<TocEntry>) {
    let mut result = String::with_capacity(html.len() + 1024);
    let mut entries = Vec::new();

    for line in html.lines() {
        let trimmed = line.trim();
        if let Some((level, tag_end)) = detect_heading(trimmed) {
            *counter += 1;
            let inner = extract_heading_text(trimmed, tag_end);
            let slug = slugify(&inner, *counter);
            entries.push(TocEntry {
                level,
                id: slug.clone(),
                text: strip_html_inline(&inner),
            });
            // Replace <hN> with <hN id="slug">
            let old_open = format!("<h{}>", level);
            let new_open = format!("<h{} id=\"{}\">", level, html_escape(&slug));
            let new_line = trimmed.replacen(&old_open, &new_open, 1);
            result.push_str(&new_line);
            result.push('\n');
        } else {
            result.push_str(line);
            result.push('\n');
        }
    }

    (result, entries)
}

fn detect_heading(line: &str) -> Option<(u8, &str)> {
    if line.starts_with("<h1>") {
        Some((1, "</h1>"))
    } else if line.starts_with("<h2>") {
        Some((2, "</h2>"))
    } else if line.starts_with("<h3>") {
        Some((3, "</h3>"))
    } else if line.starts_with("<h4>") {
        Some((4, "</h4>"))
    } else {
        None
    }
}

fn extract_heading_text(line: &str, close_tag: &str) -> String {
    if let Some(start) = line.find('>') {
        if let Some(end) = line.rfind(close_tag) {
            return line[start + 1..end].to_string();
        }
    }
    String::new()
}

fn strip_html_inline(s: &str) -> String {
    let mut out = String::new();
    let mut inside_tag = false;
    for c in s.chars() {
        if c == '<' {
            inside_tag = true;
        } else if c == '>' {
            inside_tag = false;
        } else if !inside_tag {
            out.push(c);
        }
    }
    // Unescape basic HTML entities
    out.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
}

fn slugify(text: &str, counter: u32) -> String {
    let base: String = strip_html_inline(text)
        .to_lowercase()
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else if c == ' ' {
                '-'
            } else {
                // Map accented chars
                match c {
                    'é' | 'è' | 'ê' | 'ë' => 'e',
                    'à' | 'â' | 'ä' => 'a',
                    'ù' | 'û' | 'ü' => 'u',
                    'î' | 'ï' => 'i',
                    'ô' | 'ö' => 'o',
                    'ç' => 'c',
                    _ => '-',
                }
            }
        })
        .collect();
    // Collapse multiple dashes and trim
    let collapsed: String = base
        .split('-')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    if collapsed.is_empty() {
        format!("heading-{}", counter)
    } else {
        format!("{}-{}", collapsed, counter)
    }
}

fn build_toc_html(entries: &[TocEntry]) -> String {
    let mut html = String::from("<nav class=\"toc\">\n<h2>TABLE DES MATI\u{00C8}RES</h2>\n<ul>\n");
    for entry in entries {
        // Only include h1, h2, h3 in TOC (not h4 — too deep)
        if entry.level > 3 {
            continue;
        }
        let indent = match entry.level {
            1 => "",
            2 => "  ",
            3 => "    ",
            _ => "      ",
        };
        let class = format!("toc-h{}", entry.level);
        html.push_str(&format!(
            "{}<li class=\"{}\"><a href=\"#{}\">{}</a></li>\n",
            indent,
            class,
            html_escape(&entry.id),
            html_escape(&entry.text)
        ));
    }
    html.push_str("</ul>\n</nav>\n");
    html
}

// ─── HTML template ────────────────────────────────────────────────────────

fn build_full_html(metadata: &PdfMetadata, toc_html: &str, body_html: &str) -> String {
    let version_line = if metadata.version.is_empty() {
        String::new()
    } else {
        format!(
            "<div class=\"cover-meta\">Version {} | {}</div>",
            html_escape(&metadata.version),
            html_escape(&metadata.date)
        )
    };

    let author_line = if metadata.author.is_empty() {
        String::new()
    } else {
        format!(
            "<div class=\"cover-author\">{}</div>",
            html_escape(&metadata.author)
        )
    };

    format!(
        r#"<!DOCTYPE html>
<html lang="fr">
<head>
  <meta charset="UTF-8">
  <title>{title}</title>
  <script src="https://cdn.jsdelivr.net/npm/mermaid@11/dist/mermaid.min.js"></script>
  <style>
{css}
  </style>
</head>
<body data-document-profile="technical-book">

  <!-- Cover page -->
  <div class="cover-page">
    <div class="cover-brand">DOCUMENTATION</div>
    <div class="cover-title">{title}</div>
    <div class="cover-subtitle">{subtitle}</div>
    {version_line}
    {author_line}
    <div class="cover-footer">G&eacute;n&eacute;r&eacute; par Code Explorer</div>
  </div>

  <!-- Table of contents -->
  {toc_html}

  <!-- Document body -->
  <div class="document-body">
    {body_html}
  </div>

  <script>
{mermaid_bootstrap}
  </script>
</body>
</html>"#,
        title = html_escape(&metadata.title),
        subtitle = html_escape(&metadata.subtitle),
        css = CSS_CORPORATE,
        version_line = version_line,
        author_line = author_line,
        toc_html = toc_html,
        body_html = body_html,
        mermaid_bootstrap = MERMAID_BOOTSTRAP_JS,
    )
}

const CSS_CORPORATE: &str = r#"
    /* ─── Page geometry ─────────────────────────────────────── */
    @page {
      size: A4;
      margin: 2.5cm 2cm 2.5cm 2.5cm;
    }
    @page :first {
      margin: 0;
    }

    /* ─── Base typography ───────────────────────────────────── */
    * {
      box-sizing: border-box;
      -webkit-print-color-adjust: exact;
      print-color-adjust: exact;
    }

    body {
      font-family: 'Segoe UI', Calibri, 'Helvetica Neue', Arial, sans-serif;
      font-size: 10.5pt;
      line-height: 1.55;
      color: #1a1a1a;
      margin: 0;
      padding: 0;
    }

    /* ─── Cover page ────────────────────────────────────────── */
    .cover-page {
      height: 100vh;
      display: flex;
      flex-direction: column;
      justify-content: center;
      align-items: center;
      text-align: center;
      background: linear-gradient(180deg, #003366 0%, #004488 60%, #0055aa 100%);
      color: white;
      padding: 4cm 3cm;
      page-break-after: always;
    }
    .cover-brand {
      font-size: 11pt;
      letter-spacing: 6px;
      text-transform: uppercase;
      opacity: 0.7;
      margin-bottom: 2cm;
    }
    .cover-title {
      font-size: 28pt;
      font-weight: 700;
      line-height: 1.2;
      margin-bottom: 0.8cm;
    }
    .cover-subtitle {
      font-size: 14pt;
      font-weight: 300;
      opacity: 0.85;
      margin-bottom: 1.5cm;
    }
    .cover-meta {
      font-size: 10pt;
      opacity: 0.7;
      margin-bottom: 0.3cm;
    }
    .cover-author {
      font-size: 10pt;
      opacity: 0.7;
      margin-bottom: 1cm;
    }
    .cover-footer {
      position: absolute;
      bottom: 2cm;
      font-size: 8pt;
      opacity: 0.4;
    }

    /* ─── Table of contents ─────────────────────────────────── */
    .toc {
      page-break-after: always;
      padding: 1cm 0;
    }
    .toc h2 {
      color: #003366;
      font-size: 18pt;
      border-bottom: 3px solid #003366;
      padding-bottom: 8px;
      margin-bottom: 16px;
    }
    .toc ul {
      list-style: none;
      padding-left: 0;
    }
    .toc li {
      padding: 3px 0;
      border-bottom: 1px dotted #ddd;
    }
    .toc li a {
      color: #003366;
      text-decoration: none;
    }
    .toc li a:hover {
      text-decoration: underline;
    }
    .toc .toc-h1 {
      font-size: 11pt;
      font-weight: 700;
      margin-top: 8px;
    }
    .toc .toc-h2 {
      font-size: 10pt;
      padding-left: 20px;
    }
    .toc .toc-h3 {
      font-size: 9pt;
      padding-left: 40px;
      color: #555;
    }

    /* ─── Headings ──────────────────────────────────────────── */
    h1 {
      font-size: 20pt;
      color: #003366;
      border-bottom: 2px solid #003366;
      padding-bottom: 6px;
      margin-top: 24px;
      margin-bottom: 16px;
      page-break-before: always;
    }
    /* First h1 in body should not force page break (already after TOC) */
    .document-body > h1:first-child {
      page-break-before: avoid;
    }

    h2 {
      font-size: 15pt;
      color: #004488;
      border-bottom: 1px solid #ccc;
      padding-bottom: 4px;
      margin-top: 20px;
      margin-bottom: 12px;
    }

    h2, h3, h4 {
      break-after: avoid;
      page-break-after: avoid;
    }

    h3 {
      font-size: 12pt;
      color: #2a5a8a;
      margin-top: 16px;
      margin-bottom: 10px;
    }

    h4 {
      font-size: 10.5pt;
      color: #3a6a9a;
      font-weight: 600;
      margin-top: 12px;
      margin-bottom: 8px;
    }

    /* ─── Tables ────────────────────────────────────────────── */
    table {
      border-collapse: collapse;
      width: 100%;
      margin: 12px 0;
      font-size: 9.5pt;
      page-break-inside: auto;
    }
    th {
      background: #003366;
      color: white;
      font-weight: 600;
      padding: 8px 10px;
      text-align: left;
      border: 1px solid #002244;
    }
    td {
      padding: 6px 10px;
      border: 1px solid #ccc;
      vertical-align: top;
    }
    tr:nth-child(even) td {
      background: #f5f8fc;
    }
    tr:hover td {
      background: #eef3f9;
    }

    /* ─── Blockquotes (Acme citations) ──────────────────────── */
    blockquote {
      border-left: 4px solid #003366;
      background: #f0f4f8;
      margin: 12px 0;
      padding: 10px 16px;
      font-style: italic;
      color: #333;
      page-break-inside: avoid;
    }
    blockquote + blockquote {
      margin-top: -4px;
    }

    /* ─── Code blocks ───────────────────────────────────────── */
    pre {
      background: #f8f8f8;
      border: 1px solid #e0e0e0;
      border-left: 3px solid #003366;
      padding: 12px 16px;
      border-radius: 3px;
      overflow-x: auto;
      font-size: 9pt;
      line-height: 1.45;
      overflow-wrap: anywhere;
      page-break-inside: auto;
      white-space: pre-wrap;
      word-break: break-word;
    }
    code {
      font-family: 'Cascadia Code', 'Consolas', 'Courier New', monospace;
      font-size: 9pt;
    }
    p code, li code, td code {
      background: #f0f0f0;
      padding: 1px 4px;
      border-radius: 2px;
      font-size: 9pt;
    }

    /* ─── Mermaid diagrams ──────────────────────────────────── */
    .mermaid,
    .mermaid-figure {
      page-break-inside: avoid;
      break-inside: avoid;
    }
    .mermaid-figure {
      margin: 1.5em 0;
    }
    .mermaid {
      text-align: center;
      padding: 16px;
      background: #fafbfc;
      border: 1px solid #e8e8e8;
      border-radius: 4px;
    }
    .mermaid-caption {
      color: #0f766e;
      font-size: 8.5pt;
      font-weight: 700;
      margin-bottom: 6px;
      text-transform: uppercase;
    }
    .mermaid-source {
      margin-top: 8px;
    }
    .mermaid-source summary {
      color: #7a4a00;
      cursor: pointer;
      font-size: 8.5pt;
      font-weight: 700;
    }
    .mermaid-source pre {
      background: #fff8e6;
      border-left-color: #ff9800;
      margin-top: 6px;
    }
    .mermaid-error .mermaid {
      border-color: #ffb74d;
      color: #7a4a00;
      font-size: 9pt;
      min-height: 32px;
    }
    .mermaid-error .mermaid::before {
      content: "Diagramme Mermaid non rendu dans cet environnement. Source ci-dessous.";
    }

    /* ─── Lists ─────────────────────────────────────────────── */
    ul, ol {
      margin: 8px 0;
      padding-left: 24px;
    }
    li {
      break-inside: avoid;
      margin-bottom: 3px;
    }

    img {
      break-inside: avoid;
      max-width: 100%;
    }

    /* ─── Horizontal rules ──────────────────────────────────── */
    hr {
      border: none;
      border-top: 1px solid #ddd;
      margin: 16px 0;
    }

    /* ─── Paragraphs ────────────────────────────────────────── */
    p {
      margin: 6px 0;
      text-align: justify;
    }

    /* ─── Links ─────────────────────────────────────────────── */
    a {
      color: #003366;
      text-decoration: none;
    }

    /* ─── Callouts ──────────────────────────────────────────── */
    .callout {
      border-left: 4px solid #666;
      background: #f8f8f8;
      padding: 10px 16px;
      margin: 12px 0;
      border-radius: 0 4px 4px 0;
      page-break-inside: avoid;
    }
    .callout-note { border-left-color: #2196F3; background: #e8f4fd; }
    .callout-tip { border-left-color: #4CAF50; background: #e8f5e9; }
    .callout-warning { border-left-color: #FF9800; background: #fff3e0; }
    .callout-danger { border-left-color: #f44336; background: #fce8e6; }
    .callout-title { font-weight: 700; margin-bottom: 4px; }

    /* ─── Technical book profile ───────────────────────────── */
    body[data-document-profile="technical-book"] .document-body {
      counter-reset: figure;
    }
    body[data-document-profile="technical-book"] h1 + p,
    body[data-document-profile="technical-book"] h2 + p {
      color: #334155;
      font-size: 11pt;
      line-height: 1.65;
    }
    .document-quality,
    .quality-panel {
      background: #f8fafc;
      border: 1px solid #cbd5e1;
      border-left: 5px solid #0f766e;
      border-radius: 6px;
      margin: 16px 0;
      padding: 12px 16px;
      page-break-inside: avoid;
    }
    figure {
      margin: 16px 0;
      page-break-inside: avoid;
    }
    figcaption {
      color: #64748b;
      font-size: 8.5pt;
      font-style: italic;
      margin-top: 6px;
      text-align: center;
    }
    .chapter-kicker {
      color: #0f766e;
      font-size: 8pt;
      font-weight: 800;
      letter-spacing: .08em;
      text-transform: uppercase;
    }

    /* ─── Print tweaks ──────────────────────────────────────── */
    @media print {
      body { -webkit-print-color-adjust: exact; print-color-adjust: exact; }
      .cover-page { break-after: page; }
      .toc { break-after: page; }
      h1 { break-before: page; }
      table { break-inside: auto; }
      tr { break-inside: avoid; }
    }
"#;

const MERMAID_BOOTSTRAP_JS: &str = r#"
    (function () {
      window.__code-explorerMermaidReady = false;

      function createCodeBlock(source) {
        var pre = document.createElement('pre');
        var code = document.createElement('code');
        code.textContent = source;
        pre.appendChild(code);
        return pre;
      }

      function revealSource(figure) {
        figure.classList.add('mermaid-error');
        var source = figure.querySelector('.mermaid-source');
        if (source) {
          source.open = true;
        }
      }

      function diagramLabel(source) {
        var text = source.trim();
        if (text.indexOf('sequenceDiagram') === 0) return 'Diagramme de séquence';
        if (text.indexOf('classDiagram') === 0) return 'Diagramme de classes';
        if (text.indexOf('erDiagram') === 0) return 'Diagramme entité-relation';
        if (text.indexOf('journey') === 0) return 'Parcours utilisateur';
        if (text.indexOf('gantt') === 0) return 'Planning';
        if (text.indexOf('flowchart') === 0 || text.indexOf('graph ') === 0) return 'Diagramme de flux';
        return 'Diagramme Mermaid';
      }

      document.querySelectorAll('pre code.language-mermaid, pre code.language-mmd').forEach(function (block) {
        var source = block.textContent || '';
        var figure = document.createElement('figure');
        figure.className = 'mermaid-figure';

        var caption = document.createElement('figcaption');
        caption.className = 'mermaid-caption';
        caption.textContent = diagramLabel(source);

        var diagram = document.createElement('div');
        diagram.className = 'mermaid';
        diagram.textContent = source;

        var details = document.createElement('details');
        details.className = 'mermaid-source';
        var summary = document.createElement('summary');
        summary.textContent = 'Source Mermaid';
        details.appendChild(summary);
        details.appendChild(createCodeBlock(source));

        figure.appendChild(caption);
        figure.appendChild(diagram);
        figure.appendChild(details);
        if (block.parentElement) {
          block.parentElement.replaceWith(figure);
        }
      });

      if (!window.mermaid) {
        document.querySelectorAll('.mermaid-figure').forEach(revealSource);
        window.__code-explorerMermaidReady = true;
        return;
      }

      window.mermaid.initialize({
        theme: 'base',
        startOnLoad: false,
        securityLevel: 'strict',
        themeVariables: {
          primaryColor: '#e0f2fe',
          primaryBorderColor: '#0284c7',
          primaryTextColor: '#0f172a',
          secondaryColor: '#ecfdf5',
          tertiaryColor: '#fff7ed',
          lineColor: '#64748b',
          fontFamily: 'Segoe UI, Arial, sans-serif'
        }
      });

      Promise.resolve(window.mermaid.run({ querySelector: '.mermaid' }))
        .catch(function () {
          document.querySelectorAll('.mermaid-figure').forEach(revealSource);
        })
        .finally(function () {
          window.__code-explorerMermaidReady = true;
        });
    })();
"#;

// ─── Playwright orchestration ─────────────────────────────────────────────

fn run_playwright(html_path: &Path, output_path: &Path) -> Result<()> {
    // Write the bundled print-pdf.js to a temp file
    let temp_dir = std::env::temp_dir();
    let js_path = temp_dir.join("code-explorer-print-pdf.js");
    std::fs::write(&js_path, PRINT_PDF_JS)
        .with_context(|| "Cannot write print-pdf.js to temp directory")?;

    // Find node and global node_modules
    let node = find_node()?;
    let node_path = find_global_node_modules();

    println!("{} Running Playwright (Chromium headless)...", ">>".blue());

    let mut cmd = Command::new(&node);
    cmd.arg(js_path.to_str().unwrap_or(""))
        .arg(html_path.to_str().unwrap_or(""))
        .arg(output_path.to_str().unwrap_or(""));

    // Set NODE_PATH so globally installed playwright can be found
    if let Some(ref np) = node_path {
        cmd.env("NODE_PATH", np);
    }

    let output = cmd
        .output()
        .with_context(|| format!("Failed to run: {} print-pdf.js", node))?;

    // Cleanup JS temp file
    let _ = std::fs::remove_file(&js_path);

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    if !stdout.is_empty() {
        info!("Playwright stdout: {}", stdout.trim());
        println!("   {}", stdout.trim());
    }

    if !output.status.success() {
        let err_msg = if stderr.contains("Cannot find module 'playwright'") {
            format!(
                "Playwright is not installed. Run:\n  npm install -g playwright\n  npx playwright install chromium\n\nOriginal error: {}",
                stderr.trim()
            )
        } else if stderr.contains("Executable doesn't exist")
            || stderr.contains("browserType.launch")
        {
            format!(
                "Chromium browser not installed for Playwright. Run:\n  npx playwright install chromium\n\nOriginal error: {}",
                stderr.trim()
            )
        } else {
            format!(
                "Playwright failed (exit {}): {}",
                output.status,
                stderr.trim()
            )
        };
        bail!("{}", err_msg);
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

// ─── Tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_metadata_from_sfd() {
        let content = r#"# ALISE V2 — SPÉCIFICATIONS FONCTIONNELLES

# CYCLE DE VIE D'UN DOSSIER D'AIDE

---

| | |
|---|---|
| **Version** | 2.0 |
| **Date** | 12/04/2026 |
| **Auteur** | Équipe DaMadi |
"#;
        let meta = extract_metadata(content);
        assert_eq!(
            meta.title,
            "ALISE V2 \u{2014} SP\u{00C9}CIFICATIONS FONCTIONNELLES"
        );
        assert_eq!(meta.subtitle, "CYCLE DE VIE D'UN DOSSIER D'AIDE");
        assert_eq!(meta.version, "2.0");
        assert_eq!(meta.date, "12/04/2026");
        assert_eq!(meta.author, "\u{00C9}quipe DaMadi");
    }

    #[test]
    fn test_slugify() {
        assert_eq!(
            slugify("Introduction générale", 1),
            "introduction-generale-1"
        );
        assert_eq!(
            slugify("2.1. Écrans de saisie", 5),
            "2-1-ecrans-de-saisie-5"
        );
        assert_eq!(slugify("", 42), "heading-42");
    }

    #[test]
    fn test_inject_heading_ids() {
        let html = "<h1>Title</h1>\n<p>Content</p>\n<h2>Section</h2>\n";
        let mut counter = 0;
        let (result, entries) = inject_heading_ids_and_collect(html, &mut counter);
        assert!(result.contains("id=\"title-1\""));
        assert!(result.contains("id=\"section-2\""));
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].level, 1);
        assert_eq!(entries[1].level, 2);
    }

    #[test]
    fn test_build_toc_html() {
        let entries = vec![
            TocEntry {
                level: 1,
                id: "intro-1".into(),
                text: "Introduction".into(),
            },
            TocEntry {
                level: 2,
                id: "context-2".into(),
                text: "Contexte".into(),
            },
        ];
        let toc = build_toc_html(&entries);
        assert!(toc.contains("toc-h1"));
        assert!(toc.contains("toc-h2"));
        assert!(toc.contains("#intro-1"));
        assert!(toc.contains("Introduction"));
    }

    #[test]
    fn test_strip_html_inline() {
        assert_eq!(strip_html_inline("<strong>Bold</strong> text"), "Bold text");
        assert_eq!(strip_html_inline("No tags"), "No tags");
        assert_eq!(
            strip_html_inline("<em>italic</em> &amp; more"),
            "italic & more"
        );
    }

    #[test]
    fn test_extract_table_value() {
        assert_eq!(extract_table_value("| **Version** | 2.0 |"), "2.0");
        assert_eq!(
            extract_table_value("| **Date** | 12/04/2026 |"),
            "12/04/2026"
        );
    }

    #[test]
    fn test_sanitize_pdf_markdown_removes_invisible_chars() {
        assert_eq!(
            sanitize_pdf_markdown("A\u{200B}B\u{200C}C\u{200D}D\u{FE0F}"),
            "ABCD"
        );
    }

    #[test]
    fn test_build_full_html_uses_safe_mermaid_bootstrap() {
        let metadata = PdfMetadata {
            title: "Doc".into(),
            subtitle: "Sub".into(),
            version: String::new(),
            date: "01/01/2026".into(),
            author: String::new(),
        };
        let html = build_full_html(
            &metadata,
            "",
            "<pre><code class=\"language-mermaid\">flowchart TD\nA --> B</code></pre>",
        );

        assert!(html.contains("securityLevel: 'strict'"));
        assert!(html.contains("__code-explorerMermaidReady"));
        assert!(html.contains("mermaid-source"));
        assert!(html.contains("data-document-profile=\"technical-book\""));
        assert!(html.contains("diagramLabel"));
        assert!(html.contains("theme: 'base'"));
        assert!(html.contains("print-color-adjust: exact"));
    }
}

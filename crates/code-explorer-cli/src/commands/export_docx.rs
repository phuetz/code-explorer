//! DOCX export engine — converts Markdown documentation into Word documents.
//!
//! Generates professional .docx files using Open XML (OOXML) format by:
//! 1. Reading all `.md` files from `.codeexplorer/docs/` (including ASP.NET pages)
//! 2. Reading `_index.json` to determine section order and hierarchy
//! 3. Parsing Markdown into a structured document tree
//! 4. Writing OOXML (ZIP-packaged XML) with proper styles, headers, tables, TOC
//!
//! The generated document includes:
//! - Title page with project name, stats summary, and Code Explorer branding
//! - Table of contents (TOC field) — auto-updatable in Word
//! - All documentation pages in logical order with proper heading hierarchy
//! - Tables rendered adaptively: compact Word tables or readable cards for wide narrative data
//! - Code blocks with monospace font and grey background
//! - Mermaid diagrams rendered as embedded PNGs, with a text fallback when rendering is unavailable
//! - Bullet lists, numbered lists, blockquotes
//! - Inline formatting: **bold**, *italic*, `code`, [links](url)

use std::io::Write;
use std::path::Path;

use anyhow::Result;
use base64::{engine::general_purpose, Engine as _};
use serde_json::Value;
use zip::write::{FileOptions, SimpleFileOptions};
use zip::ZipWriter;

/// Export all documentation as a single DOCX file.
/// Reads `_index.json` to determine page order, then converts all Markdown files.
pub fn export_docs_as_docx(docs_dir: &Path, output_path: &Path, project_name: &str) -> Result<()> {
    // Brand overrides — silently falls back to defaults if no brand.json exists.
    let brand = load_brand_config();

    // Read _index.json for ordered page list and stats
    let index_path = docs_dir.join("_index.json");
    let (ordered_files, stats) = if index_path.exists() {
        let index_str = std::fs::read_to_string(&index_path)?;
        let index: Value = serde_json::from_str(&index_str)?;
        let files = collect_pages_from_index(&index);
        let stats = extract_stats(&index);
        (files, stats)
    } else {
        // Fallback: read files in hardcoded order
        (fallback_file_order(), DocStats::default())
    };

    // Read all markdown files in order (with path traversal protection)
    let docs_canonical = docs_dir
        .canonicalize()
        .unwrap_or_else(|_| docs_dir.to_path_buf());
    let mut md_files: Vec<(String, String, String)> = Vec::new(); // (id, title, content)
    for (id, title, filename) in &ordered_files {
        let path = docs_dir.join(filename);
        let canonical = match path.canonicalize() {
            Ok(p) => p,
            Err(_) => continue, // file doesn't exist, skip
        };
        if !canonical.starts_with(&docs_canonical) {
            eprintln!(
                "Warning: skipping path outside docs directory: {}",
                filename
            );
            continue;
        }
        let content = std::fs::read_to_string(&canonical)?;
        md_files.push((id.clone(), title.clone(), content));
    }

    if md_files.is_empty() {
        anyhow::bail!("No documentation files found in {}", docs_dir.display());
    }

    write_docx_package(
        output_path,
        project_name,
        &md_files,
        &stats,
        &brand,
        DocxProfile::Documentation,
    )
}

/// Export a single Markdown document as a polished DOCX file.
///
/// Used by the chat work-document flow where the final deliverable is built
/// in memory instead of coming from `.codeexplorer/docs/`.
pub fn export_markdown_as_docx(
    markdown: &str,
    output_path: &Path,
    document_title: &str,
) -> Result<()> {
    let brand = load_brand_config();
    let stats = DocStats::default();
    let md_files = vec![(
        "work-document".to_string(),
        document_title.to_string(),
        markdown.to_string(),
    )];
    write_docx_package(
        output_path,
        document_title,
        &md_files,
        &stats,
        &brand,
        DocxProfile::WorkDocument(extract_workdoc_cover_metrics(markdown)),
    )
}

fn write_docx_package(
    output_path: &Path,
    project_name: &str,
    md_files: &[(String, String, String)],
    stats: &DocStats,
    brand: &BrandConfig,
    profile: DocxProfile,
) -> Result<()> {
    let file = std::fs::File::create(output_path)?;
    let mut zip = ZipWriter::new(file);
    let options: SimpleFileOptions =
        FileOptions::default().compression_method(zip::CompressionMethod::Deflated);

    // 1. [Content_Types].xml
    zip.start_file("[Content_Types].xml", options)?;
    zip.write_all(CONTENT_TYPES_XML.as_bytes())?;

    // 2. _rels/.rels
    zip.start_file("_rels/.rels", options)?;
    zip.write_all(RELS_XML.as_bytes())?;

    // 4. word/styles.xml
    zip.start_file("word/styles.xml", options)?;
    zip.write_all(generate_styles_xml().as_bytes())?;

    // 5. word/numbering.xml
    zip.start_file("word/numbering.xml", options)?;
    zip.write_all(NUMBERING_XML.as_bytes())?;

    // 6. word/document.xml (main content) and collect hyperlinks + images
    let (document_xml, links, images) =
        generate_document_xml(project_name, md_files, stats, brand, &profile);
    zip.start_file("word/document.xml", options)?;
    zip.write_all(document_xml.as_bytes())?;

    // 3. word/_rels/document.xml.rels (with dynamic hyperlinks + image rels)
    let doc_rels_xml = generate_document_rels(&links, &images);
    zip.start_file("word/_rels/document.xml.rels", options)?;
    zip.write_all(doc_rels_xml.as_bytes())?;

    // 6b. word/media/imageN.png — Mermaid diagrams and source captures.
    // No-op when no images were rendered or imported.
    for img in &images {
        zip.start_file(format!("word/media/image{}.png", img.index), options)?;
        zip.write_all(&img.png_bytes)?;
    }
    if !images.is_empty() {
        eprintln!(
            "OK Embedded {} document images as PNG (largest: {} px wide)",
            images.len(),
            images.iter().map(|i| i.dimensions.0).max().unwrap_or(0)
        );
    }

    // 7. word/header1.xml (rId3 — first page suppressed via <w:titlePg/>)
    zip.start_file("word/header1.xml", options)?;
    zip.write_all(generate_header_xml(project_name, brand).as_bytes())?;

    // 8. word/footer1.xml (rId4 — paginated via PAGE / NUMPAGES fields)
    zip.start_file("word/footer1.xml", options)?;
    zip.write_all(generate_footer_xml(brand).as_bytes())?;

    // 9. docProps/core.xml (Word "Fichier > Propriétés" core metadata)
    zip.start_file("docProps/core.xml", options)?;
    zip.write_all(generate_core_props_xml(project_name, brand).as_bytes())?;

    // 10. docProps/app.xml (Application + Company in Détails panel)
    zip.start_file("docProps/app.xml", options)?;
    zip.write_all(generate_app_props_xml(brand).as_bytes())?;

    zip.finish()?;
    Ok(())
}

// ─── Index.json Parsing ─────────────────────────────────────────────────

#[derive(Clone)]
enum DocxProfile {
    Documentation,
    WorkDocument(WorkdocCoverMetrics),
}

#[derive(Clone, Debug)]
struct WorkdocCoverMetrics {
    questions: String,
    answered: String,
    source_files: String,
    status: String,
}

#[derive(Default)]
struct DocStats {
    files: usize,
    nodes: usize,
    edges: usize,
    modules: usize,
}

fn extract_stats(index: &Value) -> DocStats {
    let s = &index["stats"];
    DocStats {
        files: s["files"].as_u64().unwrap_or(0) as usize,
        nodes: s["nodes"].as_u64().unwrap_or(0) as usize,
        edges: s["edges"].as_u64().unwrap_or(0) as usize,
        modules: s["modules"].as_u64().unwrap_or(0) as usize,
    }
}

fn extract_workdoc_cover_metrics(markdown: &str) -> WorkdocCoverMetrics {
    let chapter_count = markdown
        .lines()
        .filter(|line| line.trim_start().starts_with("### Chapitre "))
        .count();
    let questions = markdown_table_metric(markdown, "Questions extraites")
        .unwrap_or_else(|| fallback_metric_count(chapter_count));
    let answered =
        markdown_table_metric(markdown, "Questions répondues").unwrap_or_else(|| "0".to_string());
    let source_files = markdown_table_metric(markdown, "Fichiers sources cités")
        .unwrap_or_else(|| "0".to_string());
    let status = markdown_table_metric(markdown, "Statut du livrable")
        .unwrap_or_else(|| "A relire".to_string());

    WorkdocCoverMetrics {
        questions,
        answered,
        source_files,
        status,
    }
}

fn fallback_metric_count(count: usize) -> String {
    if count == 0 {
        "-".to_string()
    } else {
        count.to_string()
    }
}

fn markdown_table_metric(markdown: &str, label: &str) -> Option<String> {
    let expected = normalize_metric_label(label);
    markdown.lines().find_map(|line| {
        let trimmed = line.trim();
        if !trimmed.starts_with('|') {
            return None;
        }
        let cells = parse_table_row(trimmed);
        if cells.len() < 2 || normalize_metric_label(&cells[0]) != expected {
            return None;
        }
        let value = cells[1].trim();
        (!value.is_empty()).then(|| value.to_string())
    })
}

fn normalize_metric_label(value: &str) -> String {
    value.trim().trim_matches('*').trim().to_lowercase()
}

/// Walk the _index.json page tree and return a flat list of (id, title, path).
fn collect_pages_from_index(index: &Value) -> Vec<(String, String, String)> {
    let mut result = Vec::new();
    if let Some(pages) = index["pages"].as_array() {
        for page in pages {
            collect_page_recursive(page, &mut result);
        }
    }
    result
}

fn collect_page_recursive(page: &Value, out: &mut Vec<(String, String, String)>) {
    let id = page["id"].as_str().unwrap_or("").to_string();
    let title = page["title"].as_str().unwrap_or("").to_string();

    // If page has a path, it's a leaf page
    if let Some(path) = page["path"].as_str() {
        out.push((id, title, path.to_string()));
    }

    // If page has children, recurse into them
    if let Some(children) = page["children"].as_array() {
        for child in children {
            collect_page_recursive(child, out);
        }
    }
}

fn fallback_file_order() -> Vec<(String, String, String)> {
    let files = [
        ("overview", "Overview", "overview.md"),
        ("architecture", "Architecture", "architecture.md"),
        ("getting-started", "Getting Started", "getting-started.md"),
        (
            "aspnet-controllers",
            "Controllers & Actions",
            "aspnet-controllers.md",
        ),
        ("aspnet-routes", "API & Route Table", "aspnet-routes.md"),
        ("aspnet-entities", "Entity Data Model", "aspnet-entities.md"),
        (
            "aspnet-data-model",
            "Entity Relationship Diagram",
            "aspnet-data-model.md",
        ),
        ("aspnet-views", "Views & Templates", "aspnet-views.md"),
        ("aspnet-areas", "MVC Areas", "aspnet-areas.md"),
        (
            "aspnet-seq-http",
            "Sequence: HTTP Request Flow",
            "aspnet-seq-http.md",
        ),
        (
            "aspnet-seq-data",
            "Sequence: Data Access Flow",
            "aspnet-seq-data.md",
        ),
    ];
    files
        .iter()
        .map(|(id, title, path)| (id.to_string(), title.to_string(), path.to_string()))
        .collect()
}

// ─── Document XML Generation ────────────────────────────────────────────

fn generate_document_xml(
    project_name: &str,
    md_files: &[(String, String, String)],
    stats: &DocStats,
    brand: &BrandConfig,
    profile: &DocxProfile,
) -> (String, Vec<(String, String)>, Vec<MermaidImage>) {
    let mut body = String::new();
    let mut links = Vec::new();
    let mut images: Vec<MermaidImage> = Vec::new();

    // ── Title page ──
    body.push_str(&title_page(project_name, stats, brand, profile));
    body.push_str(PAGE_BREAK);

    // ── Table of contents ──
    body.push_str(&toc_field());
    body.push_str(PAGE_BREAK);

    // ── Document body: each markdown file as a section ──
    for (i, (_id, _title, content)) in md_files.iter().enumerate() {
        let (ooxml, doc_links) = markdown_to_ooxml(content, &mut images);
        body.push_str(&ooxml);
        links.extend(doc_links);
        // Page break between sections (but not after last)
        if i < md_files.len() - 1 {
            body.push_str(PAGE_BREAK);
        }
    }

    let doc_xml = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:wpc="http://schemas.microsoft.com/office/word/2010/wordprocessingCanvas"
            xmlns:mc="http://schemas.openxmlformats.org/markup-compatibility/2006"
            xmlns:o="urn:schemas-microsoft-com:office:office"
            xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"
            xmlns:m="http://schemas.openxmlformats.org/officeDocument/2006/math"
            xmlns:v="urn:schemas-microsoft-com:vml"
            xmlns:wp="http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing"
            xmlns:w10="urn:schemas-microsoft-com:office:word"
            xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"
            xmlns:wne="http://schemas.microsoft.com/office/word/2006/wordml">
  <w:body>
{body}
    <w:sectPr>
      <w:headerReference w:type="default" r:id="rId3"/>
      <w:footerReference w:type="default" r:id="rId4"/>
      <w:pgSz w:w="11906" w:h="16838"/>
      <w:pgMar w:top="2160" w:right="1440" w:bottom="1440" w:left="1440" w:header="708" w:footer="708" w:gutter="0"/>
      <w:titlePg/>
    </w:sectPr>
  </w:body>
</w:document>"#
    );
    (doc_xml, links, images)
}

const PAGE_BREAK: &str = r#"<w:p><w:r><w:br w:type="page"/></w:r></w:p>"#;

fn title_page(
    project_name: &str,
    stats: &DocStats,
    brand: &BrandConfig,
    profile: &DocxProfile,
) -> String {
    if let DocxProfile::WorkDocument(metrics) = profile {
        return workdoc_title_page(project_name, metrics);
    }
    documentation_title_page(project_name, stats, brand)
}

fn documentation_title_page(project_name: &str, stats: &DocStats, brand: &BrandConfig) -> String {
    let date = chrono::Local::now().format("%d/%m/%Y").to_string();
    let display_project = brand.client_name.as_deref().unwrap_or(project_name);
    let mut s = format!(
        r#"
    <w:p><w:pPr><w:spacing w:before="3000"/><w:jc w:val="center"/></w:pPr>
      <w:r><w:rPr><w:rFonts w:ascii="Segoe UI" w:hAnsi="Segoe UI"/><w:b/><w:sz w:val="60"/><w:color w:val="1B3A6B"/></w:rPr>
        <w:t>{project}</w:t>
      </w:r>
    </w:p>
    <w:p><w:pPr><w:spacing w:before="200"/><w:jc w:val="center"/></w:pPr>
      <w:r><w:rPr><w:rFonts w:ascii="Segoe UI" w:hAnsi="Segoe UI"/><w:sz w:val="32"/><w:color w:val="4472C4"/></w:rPr>
        <w:t>{subtitle}</w:t>
      </w:r>
    </w:p>
    <w:p><w:pPr><w:spacing w:before="120"/><w:jc w:val="center"/></w:pPr>
      <w:r><w:rPr><w:rFonts w:ascii="Segoe UI" w:hAnsi="Segoe UI"/><w:sz w:val="22"/><w:color w:val="888888"/></w:rPr>
        <w:t xml:space="preserve">Audit de code automatise — {date}</w:t>
      </w:r>
    </w:p>"#,
        project = xml_escape(display_project),
        subtitle = xml_escape(brand.document_title()),
        date = date
    );

    // Stats summary table on title page
    if stats.files > 0 || stats.nodes > 0 {
        let stat_cell = |label: &str, value: usize| -> String {
            format!(
                r#"<w:tc><w:tcPr><w:shd w:val="clear" w:color="auto" w:fill="F8F9FA"/><w:tcMar><w:top w:w="120" w:type="dxa"/><w:bottom w:w="120" w:type="dxa"/></w:tcMar></w:tcPr>
                    <w:p><w:pPr><w:jc w:val="center"/><w:spacing w:after="0"/></w:pPr>
                      <w:r><w:rPr><w:b/><w:sz w:val="28"/><w:color w:val="1B3A6B"/></w:rPr><w:t>{}</w:t></w:r>
                    </w:p>
                    <w:p><w:pPr><w:jc w:val="center"/><w:spacing w:after="0"/></w:pPr>
                      <w:r><w:rPr><w:sz w:val="18"/><w:color w:val="888888"/></w:rPr><w:t>{}</w:t></w:r>
                    </w:p></w:tc>"#,
                value, label
            )
        };
        s.push_str(&format!(
            r#"
    <w:p><w:pPr><w:spacing w:before="600"/><w:jc w:val="center"/></w:pPr></w:p>
    <w:tbl>
      <w:tblPr><w:tblW w:w="7000" w:type="dxa"/><w:jc w:val="center"/>
        <w:tblBorders>
          <w:top w:val="single" w:sz="4" w:space="0" w:color="D0D0D0"/>
          <w:bottom w:val="single" w:sz="4" w:space="0" w:color="D0D0D0"/>
          <w:insideH w:val="single" w:sz="4" w:space="0" w:color="D0D0D0"/>
          <w:insideV w:val="single" w:sz="4" w:space="0" w:color="D0D0D0"/>
        </w:tblBorders>
      </w:tblPr>
      <w:tblGrid><w:gridCol w:w="1750"/><w:gridCol w:w="1750"/><w:gridCol w:w="1750"/><w:gridCol w:w="1750"/></w:tblGrid>
      <w:tr>
        {cell_fichiers}
        {cell_noeuds}
        {cell_relations}
        {cell_modules}
      </w:tr>
    </w:tbl>"#,
            cell_fichiers = stat_cell("Fichiers", stats.files),
            cell_noeuds = stat_cell("Noeuds", stats.nodes),
            cell_relations = stat_cell("Relations", stats.edges),
            cell_modules = stat_cell("Modules", stats.modules),
        ));
    }

    // Generator branding
    s.push_str(
        r#"
    <w:p><w:pPr><w:spacing w:before="800"/><w:jc w:val="center"/></w:pPr>
      <w:r><w:rPr><w:rFonts w:ascii="Segoe UI" w:hAnsi="Segoe UI"/><w:sz w:val="18"/><w:color w:val="AAAAAA"/></w:rPr>
        <w:t xml:space="preserve">Genere automatiquement par Code Explorer — Code Intelligence Engine</w:t>
      </w:r>
    </w:p>"#,
    );

    s
}

fn workdoc_title_page(project_name: &str, metrics: &WorkdocCoverMetrics) -> String {
    let date = chrono::Local::now().format("%d/%m/%Y").to_string();
    let stat_cell = |label: &str, value: &str| -> String {
        format!(
            r#"<w:tc><w:tcPr><w:shd w:val="clear" w:color="auto" w:fill="FFFFFF"/><w:tcMar><w:top w:w="140" w:type="dxa"/><w:bottom w:w="100" w:type="dxa"/><w:left w:w="90" w:type="dxa"/><w:right w:w="90" w:type="dxa"/></w:tcMar><w:tcBorders><w:top w:val="single" w:sz="4" w:space="0" w:color="D8DEE9"/><w:left w:val="single" w:sz="4" w:space="0" w:color="D8DEE9"/></w:tcBorders></w:tcPr>
                    <w:p><w:pPr><w:jc w:val="center"/><w:spacing w:after="0"/></w:pPr>
                      <w:r><w:rPr><w:rFonts w:ascii="Georgia" w:hAnsi="Georgia"/><w:b/><w:sz w:val="28"/><w:color w:val="1F4E79"/></w:rPr><w:t>{}</w:t></w:r>
                    </w:p>
                    <w:p><w:pPr><w:jc w:val="center"/><w:spacing w:after="0"/></w:pPr>
                      <w:r><w:rPr><w:rFonts w:ascii="Georgia" w:hAnsi="Georgia"/><w:sz w:val="17"/><w:color w:val="6B7280"/></w:rPr><w:t>{}</w:t></w:r>
                    </w:p></w:tc>"#,
            xml_escape(value),
            xml_escape(label)
        )
    };

    format!(
        r#"
    <w:p><w:pPr><w:spacing w:before="2600" w:after="220"/><w:jc w:val="center"/></w:pPr>
      <w:r><w:rPr><w:rFonts w:ascii="Georgia" w:hAnsi="Georgia"/><w:b/><w:caps/><w:sz w:val="17"/><w:color w:val="9CA3AF"/><w:spacing w:val="80"/></w:rPr>
        <w:t>CODE EXPLORER DOCUMENT DE TRAVAIL</w:t>
      </w:r>
    </w:p>
    <w:p><w:pPr><w:jc w:val="center"/><w:spacing w:before="180" w:after="180"/></w:pPr>
      <w:r><w:rPr><w:rFonts w:ascii="Georgia" w:hAnsi="Georgia"/><w:b/><w:sz w:val="52"/><w:color w:val="1F4E79"/></w:rPr>
        <w:t>{title}</w:t>
      </w:r>
    </w:p>
    <w:p><w:pPr><w:jc w:val="center"/><w:pBdr><w:bottom w:val="single" w:sz="8" w:space="8" w:color="1F4E79"/></w:pBdr><w:spacing w:before="120" w:after="360"/></w:pPr>
      <w:r><w:rPr><w:rFonts w:ascii="Georgia" w:hAnsi="Georgia"/><w:sz w:val="23"/><w:color w:val="3F3F46"/></w:rPr>
        <w:t>Questions extraites, réponses vérifiées et livre technique final</w:t>
      </w:r>
    </w:p>
    <w:p><w:pPr><w:jc w:val="center"/><w:spacing w:before="80" w:after="520"/></w:pPr>
      <w:r><w:rPr><w:rFonts w:ascii="Georgia" w:hAnsi="Georgia"/><w:sz w:val="20"/><w:color w:val="6B7280"/></w:rPr>
        <w:t xml:space="preserve">Document technique généré le {date}</w:t>
      </w:r>
    </w:p>
    <w:tbl>
      <w:tblPr><w:tblW w:w="7600" w:type="dxa"/><w:jc w:val="center"/>
        <w:tblBorders>
          <w:top w:val="single" w:sz="4" w:space="0" w:color="D8DEE9"/>
          <w:bottom w:val="single" w:sz="4" w:space="0" w:color="D8DEE9"/>
          <w:insideV w:val="single" w:sz="4" w:space="0" w:color="D8DEE9"/>
        </w:tblBorders>
      </w:tblPr>
      <w:tblGrid><w:gridCol w:w="1900"/><w:gridCol w:w="1900"/><w:gridCol w:w="1900"/><w:gridCol w:w="1900"/></w:tblGrid>
      <w:tr>
        {questions}
        {answered}
        {sources}
        {quality}
      </w:tr>
    </w:tbl>
    <w:p><w:pPr><w:spacing w:before="760"/><w:jc w:val="center"/></w:pPr>
      <w:r><w:rPr><w:rFonts w:ascii="Georgia" w:hAnsi="Georgia"/><w:sz w:val="17"/><w:color w:val="A0A4AA"/></w:rPr>
        <w:t xml:space="preserve">Généré automatiquement par Code Explorer Chat — atelier document Word</w:t>
      </w:r>
    </w:p>"#,
        title = xml_escape(project_name),
        date = date,
        questions = stat_cell("Questions", &metrics.questions),
        answered = stat_cell("Réponses", &metrics.answered),
        sources = stat_cell("Sources", &metrics.source_files),
        quality = stat_cell("Statut", &metrics.status),
    )
}

fn toc_field() -> String {
    r#"
    <w:p><w:pPr><w:pStyle w:val="Heading1"/></w:pPr>
      <w:r><w:t>Table des matieres</w:t></w:r>
    </w:p>
    <w:p>
      <w:r>
        <w:fldChar w:fldCharType="begin"/>
      </w:r>
      <w:r>
        <w:instrText xml:space="preserve"> TOC \o "1-4" \h \z \u </w:instrText>
      </w:r>
      <w:r>
        <w:fldChar w:fldCharType="separate"/>
      </w:r>
      <w:r>
        <w:rPr><w:i/><w:color w:val="999999"/></w:rPr>
        <w:t>Ouvrez ce document dans Word et appuyez sur Ctrl+A, F9 pour actualiser la table des matieres.</w:t>
      </w:r>
      <w:r>
        <w:fldChar w:fldCharType="end"/>
      </w:r>
    </w:p>
"#
    .to_string()
}

// ─── Markdown to OOXML Conversion ────────────────────────────────────────

/// Convert Markdown content to OOXML paragraphs.
/// Returns (ooxml_string, vec_of_links) where links are (rId, url) pairs.
/// `images` accumulates Mermaid PNGs across all pages — its length is the
/// global image counter (1-based) and must keep growing across calls so
/// rIdImg<N> stays unique.
fn markdown_to_ooxml(
    markdown: &str,
    images: &mut Vec<MermaidImage>,
) -> (String, Vec<(String, String)>) {
    let mut result = String::new();
    let mut links = Vec::new();
    let mut link_counter = 10;
    let lines: Vec<&str> = markdown.lines().collect();
    let mut i = 0;

    while i < lines.len() {
        let line = lines[i];
        let trimmed = line.trim();

        // Empty line
        if trimmed.is_empty() {
            i += 1;
            continue;
        }

        // Headings (H1 to H6, check longest prefix first)
        if let Some(rest) = trimmed.strip_prefix("###### ") {
            let (ooxml, doc_links) = heading(rest, 6);
            append_ooxml_fragment(&mut result, &mut links, &mut link_counter, ooxml, doc_links);
            i += 1;
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("##### ") {
            let (ooxml, doc_links) = heading(rest, 5);
            append_ooxml_fragment(&mut result, &mut links, &mut link_counter, ooxml, doc_links);
            i += 1;
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("#### ") {
            let (ooxml, doc_links) = heading(rest, 4);
            append_ooxml_fragment(&mut result, &mut links, &mut link_counter, ooxml, doc_links);
            i += 1;
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("### ") {
            let (ooxml, doc_links) = heading(rest, 3);
            append_ooxml_fragment(&mut result, &mut links, &mut link_counter, ooxml, doc_links);
            i += 1;
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("## ") {
            let (ooxml, doc_links) = heading(rest, 2);
            append_ooxml_fragment(&mut result, &mut links, &mut link_counter, ooxml, doc_links);
            i += 1;
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("# ") {
            let (ooxml, doc_links) = heading(rest, 1);
            append_ooxml_fragment(&mut result, &mut links, &mut link_counter, ooxml, doc_links);
            i += 1;
            continue;
        }

        // Full-line Markdown images generated by the DOCX work-document
        // importer. They become real embedded Word images, not text links.
        if let Some(image) = parse_markdown_data_image(trimmed) {
            let next_index = images.len() + 1;
            result.push_str(&drawing_xml(next_index, image.dimensions, &image.alt));
            result.push_str(&figure_caption_xml(next_index, &image.alt));
            images.push(MermaidImage {
                index: next_index,
                png_bytes: image.png_bytes,
                dimensions: image.dimensions,
            });
            i += 1;
            continue;
        }

        // Code blocks (fenced)
        if trimmed.starts_with("```") {
            let lang = trimmed.strip_prefix("```").unwrap_or("").trim().to_string();
            let mut code_lines = Vec::new();
            i += 1;
            while i < lines.len() && !lines[i].trim().starts_with("```") {
                code_lines.push(lines[i]);
                i += 1;
            }
            i += 1; // skip closing ```

            if lang == "mermaid" {
                result.push_str(&mermaid_to_xml(&code_lines.join("\n"), images));
            } else {
                result.push_str(&code_block(&code_lines.join("\n"), &lang));
            }
            continue;
        }

        // Tables: collect contiguous lines starting with |
        if trimmed.starts_with('|') && trimmed.contains('|') {
            let mut table_lines = Vec::new();
            while i < lines.len() && lines[i].trim().starts_with('|') {
                table_lines.push(lines[i].trim());
                i += 1;
            }
            let (ooxml, table_links) = table_to_ooxml(&table_lines);
            append_ooxml_fragment(
                &mut result,
                &mut links,
                &mut link_counter,
                ooxml,
                table_links,
            );
            continue;
        }

        // Nested bullet list (indented 2+ spaces + -)
        if (trimmed.starts_with("- ") || trimmed.starts_with("* "))
            && (line.starts_with("  ") || line.starts_with('\t'))
        {
            let content = &trimmed[2..];
            let (ooxml, item_links) = bullet_item(content, 1);
            append_ooxml_fragment(
                &mut result,
                &mut links,
                &mut link_counter,
                ooxml,
                item_links,
            );
            i += 1;
            continue;
        }

        // Bullet list
        if trimmed.starts_with("- ") || trimmed.starts_with("* ") {
            let content = &trimmed[2..];
            let (ooxml, item_links) = bullet_item(content, 0);
            append_ooxml_fragment(
                &mut result,
                &mut links,
                &mut link_counter,
                ooxml,
                item_links,
            );
            i += 1;
            continue;
        }

        // Numbered list
        if trimmed.len() > 2
            && trimmed.chars().next().is_some_and(|c| c.is_ascii_digit())
            && trimmed.contains(". ")
        {
            let dot_pos = trimmed.find(". ").unwrap_or(0);
            let content = &trimmed[dot_pos + 2..];
            let (ooxml, item_links) = numbered_item(content);
            append_ooxml_fragment(
                &mut result,
                &mut links,
                &mut link_counter,
                ooxml,
                item_links,
            );
            i += 1;
            continue;
        }

        // Obsidian-style callouts: > [!NOTE], > [!WARNING], ...
        if let Some((kind, title)) = parse_callout_marker(trimmed) {
            let mut callout_lines = Vec::new();
            i += 1;
            while i < lines.len() && lines[i].trim().starts_with('>') {
                callout_lines.push(
                    lines[i]
                        .trim()
                        .strip_prefix('>')
                        .unwrap_or("")
                        .trim_start()
                        .to_string(),
                );
                i += 1;
            }
            let (ooxml, callout_links) = callout_block(kind, &title, &callout_lines);
            append_ooxml_fragment(
                &mut result,
                &mut links,
                &mut link_counter,
                ooxml,
                callout_links,
            );
            continue;
        }

        // Blockquote
        if let Some(rest) = trimmed.strip_prefix("> ") {
            let (ooxml, blockquote_links) = blockquote(rest);
            append_ooxml_fragment(
                &mut result,
                &mut links,
                &mut link_counter,
                ooxml,
                blockquote_links,
            );
            i += 1;
            continue;
        }

        // Horizontal rule
        if trimmed == "---" || trimmed == "***" || trimmed == "___" {
            result.push_str(HORIZONTAL_RULE);
            i += 1;
            continue;
        }

        // Regular paragraph
        let (ooxml, para_links) = paragraph(trimmed);
        append_ooxml_fragment(
            &mut result,
            &mut links,
            &mut link_counter,
            ooxml,
            para_links,
        );
        i += 1;
    }

    (result, links)
}

fn append_ooxml_fragment(
    result: &mut String,
    links: &mut Vec<(String, String)>,
    link_counter: &mut usize,
    mut fragment: String,
    fragment_links: Vec<(String, String)>,
) {
    for (old_rid, url) in fragment_links {
        let new_rid = format!("rId{}", *link_counter);
        *link_counter += 1;
        fragment = fragment.replace(
            &format!(r#"r:id="{old_rid}""#),
            &format!(r#"r:id="{new_rid}""#),
        );
        links.push((new_rid, url));
    }
    result.push_str(&fragment);
}

// ─── OOXML Element Builders ──────────────────────────────────────────────

fn heading(text: &str, level: u32) -> (String, Vec<(String, String)>) {
    let style = format!("Heading{}", level);
    let (runs, links) = inline_runs(text);
    (
        format!(
            r#"<w:p><w:pPr><w:pStyle w:val="{style}"/></w:pPr>{}</w:p>"#,
            runs
        ),
        links,
    )
}

fn paragraph(text: &str) -> (String, Vec<(String, String)>) {
    let (runs, links) = inline_runs(text);
    (
        format!(
            r#"<w:p><w:pPr><w:spacing w:after="120"/></w:pPr>{}</w:p>"#,
            runs
        ),
        links,
    )
}

fn bullet_item(text: &str, level: u32) -> (String, Vec<(String, String)>) {
    let indent = (level + 1) * 360;
    let (runs, links) = inline_runs(text);
    (
        format!(
            r#"<w:p><w:pPr><w:pStyle w:val="ListBullet"/><w:numPr><w:ilvl w:val="{level}"/><w:numId w:val="1"/></w:numPr><w:ind w:left="{indent}" w:hanging="360"/></w:pPr>{}</w:p>"#,
            runs
        ),
        links,
    )
}

fn numbered_item(text: &str) -> (String, Vec<(String, String)>) {
    let (runs, links) = inline_runs(text);
    (
        format!(
            r#"<w:p><w:pPr><w:pStyle w:val="ListNumber"/><w:numPr><w:ilvl w:val="0"/><w:numId w:val="2"/></w:numPr></w:pPr>{}</w:p>"#,
            runs
        ),
        links,
    )
}

fn blockquote(text: &str) -> (String, Vec<(String, String)>) {
    let (runs, links) = inline_runs(text);
    (
        format!(
            r#"<w:p><w:pPr><w:pBdr><w:left w:val="single" w:sz="18" w:space="8" w:color="4472C4"/></w:pBdr><w:ind w:left="360"/><w:shd w:val="clear" w:color="auto" w:fill="F0F4FA"/><w:spacing w:after="120"/></w:pPr>{}</w:p>"#,
            runs
        ),
        links,
    )
}

#[derive(Clone, Copy)]
enum CalloutKind {
    Note,
    Tip,
    Warning,
    Danger,
}

impl CalloutKind {
    fn title(self) -> &'static str {
        match self {
            Self::Note => "Note",
            Self::Tip => "Conseil",
            Self::Warning => "Point d'attention",
            Self::Danger => "Risque",
        }
    }

    fn border(self) -> &'static str {
        match self {
            Self::Note => "2E74B5",
            Self::Tip => "0F766E",
            Self::Warning => "D97706",
            Self::Danger => "DC2626",
        }
    }

    fn fill(self) -> &'static str {
        match self {
            Self::Note => "EAF3FB",
            Self::Tip => "ECFDF5",
            Self::Warning => "FFF7ED",
            Self::Danger => "FEF2F2",
        }
    }
}

fn parse_callout_marker(text: &str) -> Option<(CalloutKind, String)> {
    let rest = text.strip_prefix('>')?.trim_start();
    if !rest.starts_with("[!") {
        return None;
    }
    let end = rest.find(']')?;
    let marker = rest[2..end].trim().to_ascii_uppercase();
    let kind = match marker.as_str() {
        "NOTE" | "INFO" => CalloutKind::Note,
        "TIP" | "SUCCESS" => CalloutKind::Tip,
        "WARNING" | "CAUTION" | "IMPORTANT" => CalloutKind::Warning,
        "DANGER" | "ERROR" => CalloutKind::Danger,
        _ => return None,
    };
    let custom_title = rest[end + 1..].trim();
    let title = if custom_title.is_empty() {
        kind.title().to_string()
    } else {
        custom_title.to_string()
    };
    Some((kind, title))
}

fn callout_block(
    kind: CalloutKind,
    title: &str,
    lines: &[String],
) -> (String, Vec<(String, String)>) {
    let mut result = String::new();
    let mut links = Vec::new();
    let border = kind.border();
    let fill = kind.fill();

    result.push_str(&format!(
        r#"<w:p><w:pPr><w:pBdr><w:left w:val="single" w:sz="22" w:space="8" w:color="{border}"/></w:pBdr><w:ind w:left="300"/><w:shd w:val="clear" w:color="auto" w:fill="{fill}"/><w:spacing w:before="80" w:after="40"/></w:pPr><w:r><w:rPr><w:b/><w:color w:val="{border}"/></w:rPr><w:t xml:space="preserve">{}</w:t></w:r></w:p>"#,
        xml_escape(title)
    ));

    for line in lines.iter().filter(|line| !line.trim().is_empty()) {
        let (runs, line_links) = inline_runs(line);
        links.extend(line_links);
        result.push_str(&format!(
            r#"<w:p><w:pPr><w:pBdr><w:left w:val="single" w:sz="22" w:space="8" w:color="{border}"/></w:pBdr><w:ind w:left="300"/><w:shd w:val="clear" w:color="auto" w:fill="{fill}"/><w:spacing w:after="80"/></w:pPr>{}</w:p>"#,
            runs
        ));
    }

    result.push_str(r#"<w:p><w:pPr><w:spacing w:after="80"/></w:pPr></w:p>"#);
    (result, links)
}

fn code_block(code: &str, lang: &str) -> String {
    let mut result = String::new();

    if !lang.is_empty() {
        result.push_str(&format!(
            r#"<w:p><w:pPr><w:spacing w:before="100" w:after="30"/></w:pPr><w:r><w:rPr><w:rFonts w:ascii="Consolas" w:hAnsi="Consolas"/><w:sz w:val="16"/><w:b/><w:color w:val="6B7280"/><w:caps/></w:rPr><w:t xml:space="preserve">{}</w:t></w:r></w:p>"#,
            xml_escape(lang)
        ));
    }

    // A single-cell table gives Word a continuous, padded code panel instead
    // of a stack of visually disconnected shaded paragraphs.
    result.push_str(
        r#"<w:tbl><w:tblPr><w:tblW w:w="9000" w:type="dxa"/><w:jc w:val="center"/><w:tblBorders><w:top w:val="single" w:sz="4" w:space="0" w:color="D7DEE8"/><w:left w:val="single" w:sz="4" w:space="0" w:color="D7DEE8"/><w:bottom w:val="single" w:sz="4" w:space="0" w:color="D7DEE8"/><w:right w:val="single" w:sz="4" w:space="0" w:color="D7DEE8"/></w:tblBorders></w:tblPr><w:tblGrid><w:gridCol w:w="9000"/></w:tblGrid><w:tr><w:tc><w:tcPr><w:shd w:val="clear" w:color="auto" w:fill="F6F8FA"/><w:tcMar><w:top w:w="120" w:type="dxa"/><w:bottom w:w="120" w:type="dxa"/><w:left w:w="160" w:type="dxa"/><w:right w:w="160" w:type="dxa"/></w:tcMar></w:tcPr>"#,
    );

    for line in code
        .lines()
        .chain(if code.is_empty() { Some("") } else { None })
    {
        result.push_str(&format!(
            r#"<w:p><w:pPr><w:spacing w:after="0" w:line="240" w:lineRule="auto"/></w:pPr><w:r><w:rPr><w:rFonts w:ascii="Consolas" w:hAnsi="Consolas" w:cs="Consolas"/><w:sz w:val="16"/><w:color w:val="24292F"/></w:rPr><w:t xml:space="preserve">{}</w:t></w:r></w:p>"#,
            xml_escape(line)
        ));
    }

    result.push_str("</w:tc></w:tr></w:tbl>");
    result.push_str(r#"<w:p><w:pPr><w:spacing w:after="80"/></w:pPr></w:p>"#);
    result
}

/// One Mermaid diagram rendered to PNG and waiting to be ZIPped under
/// `word/media/imageN.png`. The `index` is 1-based and matches both the
/// filename suffix and the `rIdImg<N>` referenced from document.xml.
struct MermaidImage {
    index: usize,
    png_bytes: Vec<u8>,
    /// (width_px, height_px) read from the PNG IHDR — used to keep the
    /// embedded `<wp:extent>` aspect-ratio correct.
    dimensions: (u32, u32),
}

struct MarkdownDataImage {
    alt: String,
    png_bytes: Vec<u8>,
    dimensions: (u32, u32),
}

fn parse_markdown_data_image(line: &str) -> Option<MarkdownDataImage> {
    let trimmed = line.trim();
    let rest = trimmed.strip_prefix("![")?;
    let alt_end = rest.find("](")?;
    if !trimmed.ends_with(')') {
        return None;
    }
    let alt = rest[..alt_end].trim();
    let uri_start = 2 + alt_end + 2;
    let uri = &trimmed[uri_start..trimmed.len() - 1];
    let encoded = uri.strip_prefix("data:image/png;base64,")?;
    let png_bytes = general_purpose::STANDARD.decode(encoded).ok()?;
    let dimensions = png_dimensions(&png_bytes)?;
    Some(MarkdownDataImage {
        alt: if alt.is_empty() {
            "Capture fonctionnelle".to_string()
        } else {
            alt.to_string()
        },
        png_bytes,
        dimensions,
    })
}

/// Diagram type label, used in fallback placeholder and as alt-text.
fn mermaid_diagram_label(code: &str) -> &'static str {
    if code.starts_with("sequenceDiagram") {
        "Diagramme de Sequence"
    } else if code.starts_with("erDiagram") {
        "Diagramme Entite-Relation"
    } else if code.starts_with("graph TD") || code.starts_with("graph LR") {
        "Diagramme de Dependances"
    } else if code.starts_with("classDiagram") {
        "Diagramme de Classes"
    } else if code.starts_with("flowchart") {
        "Diagramme de Flux"
    } else {
        "Diagramme Mermaid"
    }
}

/// Convert a Mermaid code block to either an embedded PNG (best case) or
/// the legacy text placeholder (fallback when rendering is disabled / fails).
///
/// Rendering is ON by default via Kroki HTTP. Set `CODE_EXPLORER_MERMAID_PLACEHOLDER=1`
/// to force the legacy text-only behavior (useful offline or for CI snapshots).
fn mermaid_to_xml(code: &str, images: &mut Vec<MermaidImage>) -> String {
    let label = mermaid_diagram_label(code);

    // Opt-out: force placeholder. Useful when the host has no network access
    // or the user wants the deterministic text output for diffing.
    let force_placeholder = std::env::var("CODE_EXPLORER_MERMAID_PLACEHOLDER")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false);

    if !force_placeholder {
        match render_mermaid_via_kroki(code) {
            Ok(png_bytes) => {
                let dimensions = png_dimensions(&png_bytes).unwrap_or((1200, 800));
                let next_index = images.len() + 1;
                let drawing = drawing_xml(next_index, dimensions, label);
                let caption = figure_caption_xml(next_index, label);
                images.push(MermaidImage {
                    index: next_index,
                    png_bytes,
                    dimensions,
                });
                return format!("{drawing}{caption}");
            }
            Err(e) => {
                eprintln!(
                    "Warning: Mermaid render failed ({}). Falling back to text placeholder. \
                     Set CODE_EXPLORER_MERMAID_PLACEHOLDER=1 to silence.",
                    e
                );
            }
        }
    }

    mermaid_placeholder_xml(code, label)
}

/// Legacy text-only rendering — kept as fallback when Kroki is unavailable.
fn mermaid_placeholder_xml(code: &str, label: &str) -> String {
    let mut result = String::new();
    result.push_str(&format!(
        r#"<w:p><w:pPr><w:shd w:val="clear" w:color="auto" w:fill="E8F0FE"/><w:spacing w:after="60"/><w:pBdr><w:top w:val="single" w:sz="4" w:space="4" w:color="4472C4"/><w:bottom w:val="single" w:sz="4" w:space="4" w:color="4472C4"/></w:pBdr></w:pPr><w:r><w:rPr><w:b/><w:color w:val="1B3A6B"/><w:sz w:val="22"/></w:rPr><w:t xml:space="preserve">  {label}</w:t></w:r></w:p>"#,
    ));
    result.push_str(
        r#"<w:p><w:pPr><w:shd w:val="clear" w:color="auto" w:fill="E8F0FE"/><w:spacing w:after="60"/></w:pPr><w:r><w:rPr><w:i/><w:sz w:val="18"/><w:color w:val="666666"/></w:rPr><w:t xml:space="preserve">Copiez le code ci-dessous dans mermaid.live ou un viewer Mermaid pour voir le rendu visuel.</w:t></w:r></w:p>"#,
    );
    for line in code.lines() {
        result.push_str(&format!(
            r#"<w:p><w:pPr><w:shd w:val="clear" w:color="auto" w:fill="E8F0FE"/><w:spacing w:after="0"/></w:pPr><w:r><w:rPr><w:rFonts w:ascii="Consolas" w:hAnsi="Consolas"/><w:sz w:val="16"/><w:color w:val="3366AA"/></w:rPr><w:t xml:space="preserve">{}</w:t></w:r></w:p>"#,
            xml_escape(line)
        ));
    }
    result.push_str(r#"<w:p><w:pPr><w:spacing w:after="120"/></w:pPr></w:p>"#);
    result
}

fn figure_caption_xml(index: usize, label: &str) -> String {
    format!(
        r#"<w:p><w:pPr><w:jc w:val="center"/><w:spacing w:after="180"/></w:pPr><w:r><w:rPr><w:i/><w:sz w:val="18"/><w:color w:val="666666"/></w:rPr><w:t xml:space="preserve">Figure {index} - {label}</w:t></w:r></w:p>"#,
        label = xml_escape(label)
    )
}

/// POST a Mermaid source to Kroki and get back the rendered PNG bytes.
/// 15s timeout per diagram — Kroki is fast (~500ms) but spikes happen.
fn render_mermaid_via_kroki(code: &str) -> Result<Vec<u8>> {
    let url = std::env::var("CODE_EXPLORER_KROKI_URL")
        .unwrap_or_else(|_| "https://kroki.io/mermaid/png".to_string());
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .map_err(|e| anyhow::anyhow!("kroki client: {}", e))?;
    let resp = client
        .post(&url)
        .header("Content-Type", "text/plain")
        .body(code.to_string())
        .send()
        .map_err(|e| anyhow::anyhow!("kroki request: {}", e))?;
    if !resp.status().is_success() {
        anyhow::bail!("kroki HTTP {}", resp.status());
    }
    Ok(resp.bytes()?.to_vec())
}

/// Parse PNG width/height from the IHDR chunk. Returns None for non-PNG
/// or truncated input. The 8-byte signature is followed by a 4-byte chunk
/// length, the "IHDR" chunk type, then width (BE u32) and height (BE u32).
fn png_dimensions(bytes: &[u8]) -> Option<(u32, u32)> {
    if bytes.len() < 24 || &bytes[..8] != b"\x89PNG\r\n\x1a\n" {
        return None;
    }
    let w = u32::from_be_bytes([bytes[16], bytes[17], bytes[18], bytes[19]]);
    let h = u32::from_be_bytes([bytes[20], bytes[21], bytes[22], bytes[23]]);
    Some((w, h))
}

/// Build the `<w:drawing>` paragraph that embeds an image referenced by
/// `rIdImg<index>`. Width is capped at ~6.3 inches (page-width minus margins);
/// height scales by the PNG's aspect ratio. EMU = English Metric Units, the
/// OOXML coordinate system (1 inch = 914 400 EMU; 1 px @96dpi = 9 525 EMU).
fn drawing_xml(index: usize, (w_px, h_px): (u32, u32), alt_text: &str) -> String {
    const MAX_WIDTH_EMU: u64 = 5_731_510; // ~6.27 inches, fits A4 with 1.5" margins
    const MAX_HEIGHT_EMU: u64 = 8_229_600; // ~9 inches, keeps inline images within an A4 page
    let aspect = if w_px == 0 {
        1.0
    } else {
        h_px as f64 / w_px as f64
    };
    let mut width_emu = MAX_WIDTH_EMU;
    let mut height_emu = ((width_emu as f64) * aspect).round() as u64;
    if height_emu > MAX_HEIGHT_EMU {
        height_emu = MAX_HEIGHT_EMU;
        width_emu = ((height_emu as f64) / aspect.max(0.01)).round() as u64;
    }
    let alt = xml_escape(alt_text);
    format!(
        r#"<w:p><w:pPr><w:jc w:val="center"/><w:spacing w:before="120" w:after="120"/></w:pPr><w:r><w:rPr><w:noProof/></w:rPr><w:drawing><wp:inline distT="0" distB="0" distL="0" distR="0" xmlns:wp="http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing"><wp:extent cx="{width_emu}" cy="{height_emu}"/><wp:effectExtent l="0" t="0" r="0" b="0"/><wp:docPr id="{index}" name="Diagramme {index}" descr="{alt}"/><wp:cNvGraphicFramePr><a:graphicFrameLocks xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" noChangeAspect="1"/></wp:cNvGraphicFramePr><a:graphic xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"><a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/picture"><pic:pic xmlns:pic="http://schemas.openxmlformats.org/drawingml/2006/picture"><pic:nvPicPr><pic:cNvPr id="{index}" name="Diagramme {index}" descr="{alt}"/><pic:cNvPicPr/></pic:nvPicPr><pic:blipFill><a:blip xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" r:embed="rIdImg{index}"/><a:stretch><a:fillRect/></a:stretch></pic:blipFill><pic:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="{width_emu}" cy="{height_emu}"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom></pic:spPr></pic:pic></a:graphicData></a:graphic></wp:inline></w:drawing></w:r></w:p>"#
    )
}

const HORIZONTAL_RULE: &str = r#"<w:p><w:pPr><w:pBdr><w:bottom w:val="single" w:sz="4" w:space="4" w:color="CCCCCC"/></w:pBdr><w:spacing w:before="200" w:after="200"/></w:pPr></w:p>"#;

fn table_to_ooxml(lines: &[&str]) -> (String, Vec<(String, String)>) {
    if lines.is_empty() {
        return (String::new(), Vec::new());
    }

    let mut result = String::new();
    let mut links = Vec::new();

    // Parse header row
    let header = parse_table_row(lines[0]);

    // Skip separator row (|---|---|)
    let data_start = if lines.len() > 1 && lines[1].contains("---") {
        2
    } else {
        1
    };
    let rows: Vec<Vec<String>> = lines
        .iter()
        .skip(data_start)
        .map(|line| parse_table_row(line))
        .filter(|cells| cells.iter().any(|cell| !cell.trim().is_empty()))
        .collect();

    let col_count = header.len();
    if col_count == 0 {
        return (String::new(), Vec::new());
    }

    if should_render_table_as_cards(&header, &rows) {
        return table_cards_to_ooxml(&header, &rows);
    }

    // Calculate column widths (total page width ~9000 twips)
    let col_width = 9000 / col_count;

    // Table start
    result.push_str(r#"<w:tbl><w:tblPr><w:tblStyle w:val="TableGrid"/><w:tblW w:w="0" w:type="auto"/><w:tblBorders>"#);
    result.push_str(r#"<w:top w:val="single" w:sz="4" w:space="0" w:color="BFBFBF"/>"#);
    result.push_str(r#"<w:left w:val="single" w:sz="4" w:space="0" w:color="BFBFBF"/>"#);
    result.push_str(r#"<w:bottom w:val="single" w:sz="4" w:space="0" w:color="BFBFBF"/>"#);
    result.push_str(r#"<w:right w:val="single" w:sz="4" w:space="0" w:color="BFBFBF"/>"#);
    result.push_str(r#"<w:insideH w:val="single" w:sz="4" w:space="0" w:color="BFBFBF"/>"#);
    result.push_str(r#"<w:insideV w:val="single" w:sz="4" w:space="0" w:color="BFBFBF"/>"#);
    result.push_str(r#"</w:tblBorders></w:tblPr>"#);

    // Grid columns
    result.push_str("<w:tblGrid>");
    for _ in 0..col_count {
        result.push_str(&format!(r#"<w:gridCol w:w="{col_width}"/>"#));
    }
    result.push_str("</w:tblGrid>");

    // Header row (bold, dark blue background, white text)
    result.push_str("<w:tr>");
    for cell in &header {
        result.push_str(&format!(
            r#"<w:tc><w:tcPr><w:shd w:val="clear" w:color="auto" w:fill="1B3A6B"/><w:tcMar><w:top w:w="40" w:type="dxa"/><w:bottom w:w="40" w:type="dxa"/><w:left w:w="80" w:type="dxa"/><w:right w:w="80" w:type="dxa"/></w:tcMar></w:tcPr><w:p><w:pPr><w:spacing w:after="0"/></w:pPr><w:r><w:rPr><w:b/><w:color w:val="FFFFFF"/><w:sz w:val="20"/></w:rPr><w:t xml:space="preserve">{}</w:t></w:r></w:p></w:tc>"#,
            xml_escape(cell)
        ));
    }
    result.push_str("</w:tr>");

    // Data rows with alternating background
    for (i, cells) in rows.iter().enumerate() {
        let bg = if i % 2 == 0 { "FFFFFF" } else { "F5F7FA" };

        result.push_str("<w:tr>");
        for (j, cell) in cells.iter().enumerate() {
            if j < col_count {
                let (runs, cell_links) = inline_runs(cell);
                links.extend(cell_links);
                result.push_str(&format!(
                    r#"<w:tc><w:tcPr><w:shd w:val="clear" w:color="auto" w:fill="{bg}"/><w:tcMar><w:top w:w="30" w:type="dxa"/><w:bottom w:w="30" w:type="dxa"/><w:left w:w="80" w:type="dxa"/><w:right w:w="80" w:type="dxa"/></w:tcMar></w:tcPr><w:p><w:pPr><w:spacing w:after="0"/></w:pPr>{}</w:p></w:tc>"#,
                    runs
                ));
            }
        }
        // Fill missing cells
        for _ in cells.len()..col_count {
            result.push_str(&format!(
                r#"<w:tc><w:tcPr><w:shd w:val="clear" w:color="auto" w:fill="{bg}"/></w:tcPr><w:p></w:p></w:tc>"#
            ));
        }
        result.push_str("</w:tr>");
    }

    result.push_str("</w:tbl>");
    result.push_str(r#"<w:p><w:pPr><w:spacing w:after="160"/></w:pPr></w:p>"#);
    (result, links)
}

fn should_render_table_as_cards(header: &[String], rows: &[Vec<String>]) -> bool {
    let col_count = header.len();
    if col_count == 0 || rows.is_empty() {
        return false;
    }

    let has_medium_long_cell = rows
        .iter()
        .flat_map(|row| row.iter())
        .any(|cell| cell.chars().count() > 55);
    let has_long_cell = rows
        .iter()
        .flat_map(|row| row.iter())
        .any(|cell| cell.chars().count() > 90);
    let has_very_long_cell = rows
        .iter()
        .flat_map(|row| row.iter())
        .any(|cell| cell.chars().count() > 180);
    let has_code_like_cell = rows
        .iter()
        .flat_map(|row| row.iter())
        .any(|cell| looks_like_code_or_path(cell));
    let narrative_header = header.iter().any(|cell| {
        let normalized = cell.trim().to_lowercase();
        normalized.contains("question")
            || normalized.contains("description")
            || normalized.contains("réponse")
            || normalized.contains("reponse")
            || normalized.contains("sources")
            || normalized.contains("élément")
            || normalized.contains("element")
            || normalized.contains("commentaire")
            || normalized.contains("impact")
            || normalized.contains("canal")
            || normalized.contains("localisation")
            || normalized.contains("rôle")
            || normalized.contains("role")
            || normalized.contains("fichier")
    });

    col_count >= 5
        || (col_count >= 4 && (has_long_cell || narrative_header || has_code_like_cell))
        || (col_count >= 3 && narrative_header && (has_medium_long_cell || has_code_like_cell))
        || has_very_long_cell
}

fn looks_like_code_or_path(cell: &str) -> bool {
    let value = cell.trim();
    value.contains('`')
        || value.contains(".cs")
        || value.contains(".cshtml")
        || value.contains(".js")
        || value.contains('/')
        || value.contains('\\')
        || value.matches('.').count() >= 2
}

fn table_cards_to_ooxml(
    header: &[String],
    rows: &[Vec<String>],
) -> (String, Vec<(String, String)>) {
    let mut result = String::new();
    let mut links = Vec::new();

    for (row_index, row) in rows.iter().enumerate() {
        let fill = if row_index % 2 == 0 {
            "F8FBFF"
        } else {
            "FFFFFF"
        };
        let title = table_card_title(header, row);

        result.push_str(&format!(
            r#"<w:tbl><w:tblPr><w:tblW w:w="9000" w:type="dxa"/><w:jc w:val="center"/><w:tblBorders><w:top w:val="single" w:sz="6" w:space="0" w:color="D7E3F1"/><w:left w:val="single" w:sz="6" w:space="0" w:color="D7E3F1"/><w:bottom w:val="single" w:sz="6" w:space="0" w:color="D7E3F1"/><w:right w:val="single" w:sz="6" w:space="0" w:color="D7E3F1"/></w:tblBorders></w:tblPr><w:tblGrid><w:gridCol w:w="9000"/></w:tblGrid><w:tr><w:tc><w:tcPr><w:shd w:val="clear" w:color="auto" w:fill="{fill}"/><w:tcMar><w:top w:w="120" w:type="dxa"/><w:bottom w:w="120" w:type="dxa"/><w:left w:w="160" w:type="dxa"/><w:right w:w="160" w:type="dxa"/></w:tcMar></w:tcPr>"#
        ));
        result.push_str(&format!(
            r#"<w:p><w:pPr><w:spacing w:after="70"/></w:pPr><w:r><w:rPr><w:b/><w:color w:val="1F4E79"/><w:sz w:val="21"/></w:rPr><w:t xml:space="preserve">{}</w:t></w:r></w:p>"#,
            xml_escape(&title)
        ));

        for (col_index, heading) in header.iter().enumerate() {
            if heading.trim() == "#" {
                continue;
            }
            let value = row.get(col_index).map(String::as_str).unwrap_or("").trim();
            if value.is_empty() {
                continue;
            }
            let (runs, cell_links) = inline_runs(value);
            links.extend(cell_links);
            result.push_str(&format!(
                r#"<w:p><w:pPr><w:spacing w:after="55"/><w:ind w:left="160"/></w:pPr><w:r><w:rPr><w:b/><w:color w:val="3B4A5A"/><w:sz w:val="18"/></w:rPr><w:t xml:space="preserve">{} : </w:t></w:r>{}</w:p>"#,
                xml_escape(heading.trim()),
                runs
            ));
        }

        result.push_str("</w:tc></w:tr></w:tbl>");
        result.push_str(r#"<w:p><w:pPr><w:spacing w:after="100"/></w:pPr></w:p>"#);
    }

    (result, links)
}

fn table_card_title(header: &[String], row: &[String]) -> String {
    if header.first().is_some_and(|cell| cell.trim() == "#") {
        if let Some(number) = row
            .first()
            .map(|cell| cell.trim())
            .filter(|cell| !cell.is_empty())
        {
            return format!("Question {number}");
        }
    }

    row.iter()
        .find(|cell| !cell.trim().is_empty())
        .map(|cell| truncate_for_title(cell.trim(), 90))
        .unwrap_or_else(|| "Entrée".to_string())
}

fn truncate_for_title(value: &str, max_chars: usize) -> String {
    let mut iter = value.chars();
    let truncated: String = iter.by_ref().take(max_chars).collect();
    if iter.next().is_some() {
        format!("{truncated}…")
    } else {
        truncated
    }
}

fn parse_table_row(line: &str) -> Vec<String> {
    // Markdown pipe rows have a leading and trailing `|` that produce empty
    // tokens at the edges after `split('|')`. We must drop those edge-only
    // empties, but keep interior empty cells (e.g. `| a | | c |` — the
    // middle cell is intentionally blank and removing it would shift every
    // subsequent cell one column left, corrupting the rendered table).
    let raw: Vec<String> = line.split('|').map(|s| s.trim().to_string()).collect();
    // Drop a single leading empty (from the opening `|`) and a single
    // trailing empty (from the closing `|`).
    let start = if raw.first().is_some_and(|s| s.is_empty()) {
        1
    } else {
        0
    };
    let end = if raw.last().is_some_and(|s| s.is_empty()) {
        raw.len().saturating_sub(1)
    } else {
        raw.len()
    };
    if start >= end {
        return Vec::new();
    }
    raw[start..end].to_vec()
}

/// Handle inline markdown formatting: **bold**, *italic*, `code`, [text](url)
/// Returns (ooxml_string, vec_of_links) where links are (rId, url) pairs.
fn inline_runs(text: &str) -> (String, Vec<(String, String)>) {
    let mut result = String::new();
    let mut links = Vec::new();
    let mut rid_counter = 10; // Start from rId10 (rId1-2 reserved for styles/numbering)
    let chars: Vec<char> = text.chars().collect();
    let len = chars.len();
    let mut i = 0;

    while i < len {
        // Bold: **text**
        if i + 1 < len && chars[i] == '*' && chars[i + 1] == '*' {
            if let Some(end) = find_closing_double(&chars, i + 2, '*') {
                let inner: String = chars[i + 2..end].iter().collect();
                result.push_str(&format!(
                    r#"<w:r><w:rPr><w:b/></w:rPr><w:t xml:space="preserve">{}</w:t></w:r>"#,
                    xml_escape(&inner)
                ));
                i = end + 2;
                continue;
            }
        }

        // Inline code: `text`
        if chars[i] == '`' {
            if let Some(end) = chars[i + 1..].iter().position(|&c| c == '`') {
                let inner: String = chars[i + 1..i + 1 + end].iter().collect();
                result.push_str(&format!(
                    r#"<w:r><w:rPr><w:rFonts w:ascii="Consolas" w:hAnsi="Consolas"/><w:sz w:val="20"/><w:shd w:val="clear" w:color="auto" w:fill="F0F0F0"/><w:color w:val="C7254E"/></w:rPr><w:t xml:space="preserve">{}</w:t></w:r>"#,
                    xml_escape(&inner)
                ));
                i = i + 1 + end + 1;
                continue;
            }
        }

        // Link: [text](url)
        if chars[i] == '[' {
            if let Some(link) = parse_link(&chars, i) {
                let rid = format!("rId{}", rid_counter);
                rid_counter += 1;
                // Record the link
                links.push((rid.clone(), link.url.clone()));
                // Render link as clickable hyperlink element
                result.push_str(&format!(
                    r#"<w:hyperlink r:id="{rid}"><w:r><w:rPr><w:color w:val="2E5EA0"/><w:u w:val="single"/><w:rStyle w:val="Hyperlink"/></w:rPr><w:t xml:space="preserve">{}</w:t></w:r></w:hyperlink>"#,
                    xml_escape(&link.text)
                ));
                i = link.end_pos;
                continue;
            }
        }

        // Italic: *text* (but not **)
        if chars[i] == '*' && (i + 1 >= len || chars[i + 1] != '*') {
            if let Some(end) = chars[i + 1..].iter().position(|&c| c == '*') {
                // Make sure it's not part of ** bold
                if i + 1 + end + 1 >= len || chars[i + 1 + end + 1] != '*' {
                    let inner: String = chars[i + 1..i + 1 + end].iter().collect();
                    result.push_str(&format!(
                        r#"<w:r><w:rPr><w:i/></w:rPr><w:t xml:space="preserve">{}</w:t></w:r>"#,
                        xml_escape(&inner)
                    ));
                    i = i + 1 + end + 1;
                    continue;
                }
            }
        }

        // Regular text: accumulate until next special char
        let start = i;
        while i < len && chars[i] != '*' && chars[i] != '`' && chars[i] != '[' {
            i += 1;
        }
        if i > start {
            let span: String = chars[start..i].iter().collect();
            result.push_str(&format!(
                r#"<w:r><w:t xml:space="preserve">{}</w:t></w:r>"#,
                xml_escape(&span)
            ));
        } else if i < len {
            // Unmatched special char — emit as-is
            result.push_str(&format!(
                r#"<w:r><w:t xml:space="preserve">{}</w:t></w:r>"#,
                xml_escape(&chars[i].to_string())
            ));
            i += 1;
        }
    }

    (result, links)
}

struct ParsedLink {
    text: String,
    url: String,
    end_pos: usize,
}

/// Parse a Markdown link: [text](url) starting at position `start` where chars[start] == '['
fn parse_link(chars: &[char], start: usize) -> Option<ParsedLink> {
    // Find closing ]
    let mut j = start + 1;
    while j < chars.len() && chars[j] != ']' {
        j += 1;
    }
    if j >= chars.len() {
        return None;
    }
    let text: String = chars[start + 1..j].iter().collect();

    // Expect ( immediately after ]
    if j + 1 >= chars.len() || chars[j + 1] != '(' {
        return None;
    }

    // Find closing )
    let mut k = j + 2;
    while k < chars.len() && chars[k] != ')' {
        k += 1;
    }
    if k >= chars.len() {
        return None;
    }
    let url: String = chars[j + 2..k].iter().collect();

    Some(ParsedLink {
        text,
        url,
        end_pos: k + 1,
    })
}

fn find_closing_double(chars: &[char], start: usize, ch: char) -> Option<usize> {
    if chars.len() < 2 {
        return None;
    }
    (start..chars.len() - 1).find(|&i| chars[i] == ch && chars[i + 1] == ch)
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

// ─── Static OOXML Templates ─────────────────────────────────────────────

const CONTENT_TYPES_XML: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
  <Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
  <Default Extension="xml" ContentType="application/xml"/>
  <Default Extension="png" ContentType="image/png"/>
  <Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>
  <Override PartName="/word/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml"/>
  <Override PartName="/word/numbering.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.numbering+xml"/>
  <Override PartName="/word/header1.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.header+xml"/>
  <Override PartName="/word/footer1.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.footer+xml"/>
  <Override PartName="/docProps/core.xml" ContentType="application/vnd.openxmlformats-package.core-properties+xml"/>
  <Override PartName="/docProps/app.xml" ContentType="application/vnd.openxmlformats-officedocument.extended-properties+xml"/>
</Types>"#;

const RELS_XML: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/>
  <Relationship Id="rId2" Type="http://schemas.openxmlformats.org/package/2006/relationships/metadata/core-properties" Target="docProps/core.xml"/>
  <Relationship Id="rId3" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/extended-properties" Target="docProps/app.xml"/>
</Relationships>"#;

fn generate_document_rels(links: &[(String, String)], images: &[MermaidImage]) -> String {
    // rId1/rId2 stay on styles/numbering for backwards-compat (these were the
    // only ids before headers/footers were added). rId3/rId4 are the header
    // and footer references — `generate_document_xml`'s <w:sectPr> hardcodes
    // these same ids, so don't renumber without updating both sites.
    // Image rels use the dedicated `rIdImg<N>` namespace so they never
    // collide with hyperlink rIds (which use a different counter).
    let mut rels = String::from(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/>
  <Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/numbering" Target="numbering.xml"/>
  <Relationship Id="rId3" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/header" Target="header1.xml"/>
  <Relationship Id="rId4" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/footer" Target="footer1.xml"/>"#,
    );

    // Image relationships — must precede hyperlinks because <w:drawing>
    // resolves r:embed against this same rels file.
    for img in images {
        rels.push_str(&format!(
            r#"
  <Relationship Id="rIdImg{idx}" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image" Target="media/image{idx}.png"/>"#,
            idx = img.index
        ));
    }

    // Add hyperlink relationships
    for (rid, url) in links {
        rels.push_str(&format!(
            r#"
  <Relationship Id="{rid}" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/hyperlink" Target="{}" TargetMode="External"/>"#,
            xml_escape(url)
        ));
    }

    rels.push_str("\n</Relationships>");
    rels
}

// ─── Header / Footer / DocProps generators ────────────────────────────

// ─── Brand customisation (Jour 5a) ─────────────────────────────────────

/// Per-delivery branding overrides. Loaded from `~/.codeexplorer/brand.json`
/// (or `$CODE_EXPLORER_BRAND_FILE` if set). All fields are optional — a missing
/// file simply yields the legacy "agile-up.com" defaults so the binary
/// stays usable without any setup.
///
/// Example brand.json:
/// ```json
/// {
///   "client_name": "Acme Sample",
///   "company_name": "agile-up.com",
///   "footer_text": "agile-up.com — Confidentiel — Ne pas diffuser",
///   "document_title": "Documentation Technique et Fonctionnelle"
/// }
/// ```
#[derive(Debug, Clone, Default, serde::Deserialize)]
pub struct BrandConfig {
    pub client_name: Option<String>,
    pub company_name: Option<String>,
    pub footer_text: Option<String>,
    pub document_title: Option<String>,
}

impl BrandConfig {
    /// Effective company name — used in docProps and the default footer.
    fn company(&self) -> &str {
        self.company_name.as_deref().unwrap_or("agile-up.com")
    }
    /// Effective document subtitle, shown on the title page and in the header.
    fn document_title(&self) -> &str {
        self.document_title
            .as_deref()
            .unwrap_or("Documentation Technique et Fonctionnelle")
    }
    /// Effective footer text on the left side of every page.
    fn footer_text(&self) -> String {
        match &self.footer_text {
            Some(t) => t.clone(),
            None => format!("{} — Confidentiel", self.company()),
        }
    }
}

/// Load brand overrides from `$CODE_EXPLORER_BRAND_FILE` if set, otherwise from
/// `~/.codeexplorer/brand.json`. Missing or malformed file returns defaults.
fn load_brand_config() -> BrandConfig {
    let mut candidates: Vec<std::path::PathBuf> = Vec::new();
    if let Ok(env_path) = std::env::var("CODE_EXPLORER_BRAND_FILE") {
        candidates.push(std::path::PathBuf::from(env_path));
    }
    for var in ["USERPROFILE", "HOME"] {
        if let Ok(home) = std::env::var(var) {
            candidates.push(
                std::path::PathBuf::from(home)
                    .join(".codeexplorer")
                    .join("brand.json"),
            );
        }
    }
    for path in candidates {
        if path.exists() {
            if let Ok(raw) = std::fs::read_to_string(&path) {
                if let Ok(cfg) = serde_json::from_str::<BrandConfig>(&raw) {
                    return cfg;
                } else {
                    eprintln!(
                        "Warning: brand.json at {} could not be parsed — using defaults.",
                        path.display()
                    );
                }
            }
        }
    }
    BrandConfig::default()
}

/// Page header — references `rId3` defined in `generate_document_rels`.
/// Layout: client name (or project, fallback) on the left in italic gray,
/// document title (typically "Documentation Technique et Fonctionnelle")
/// on the right in bold blue. A thin bottom border separates the header
/// band from the body.
fn generate_header_xml(project_name: &str, brand: &BrandConfig) -> String {
    let left = brand.client_name.as_deref().unwrap_or(project_name);
    let right = brand.document_title();
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:hdr xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:p>
    <w:pPr>
      <w:tabs>
        <w:tab w:val="right" w:pos="9026"/>
      </w:tabs>
      <w:pBdr>
        <w:bottom w:val="single" w:sz="6" w:space="1" w:color="1B3A6B"/>
      </w:pBdr>
      <w:spacing w:after="60"/>
    </w:pPr>
    <w:r>
      <w:rPr><w:rFonts w:ascii="Segoe UI" w:hAnsi="Segoe UI"/><w:i/><w:sz w:val="18"/><w:color w:val="555555"/></w:rPr>
      <w:t>{left}</w:t>
    </w:r>
    <w:r><w:tab/></w:r>
    <w:r>
      <w:rPr><w:rFonts w:ascii="Segoe UI" w:hAnsi="Segoe UI"/><w:b/><w:sz w:val="18"/><w:color w:val="1B3A6B"/></w:rPr>
      <w:t>{right}</w:t>
    </w:r>
  </w:p>
</w:hdr>"#,
        left = xml_escape(left),
        right = xml_escape(right),
    )
}

/// Page footer — references `rId4` defined in `generate_document_rels`.
/// Layout: brand footer text (left, gray italic) ─── tab ───
/// "Page X / Y" using Word PAGE + NUMPAGES fields (right, gray).
fn generate_footer_xml(brand: &BrandConfig) -> String {
    let left = xml_escape(&brand.footer_text());
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:ftr xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:p>
    <w:pPr>
      <w:tabs>
        <w:tab w:val="right" w:pos="9026"/>
      </w:tabs>
      <w:pBdr>
        <w:top w:val="single" w:sz="4" w:space="1" w:color="BFBFBF"/>
      </w:pBdr>
      <w:spacing w:before="60"/>
    </w:pPr>
    <w:r>
      <w:rPr><w:rFonts w:ascii="Segoe UI" w:hAnsi="Segoe UI"/><w:i/><w:sz w:val="16"/><w:color w:val="888888"/></w:rPr>
      <w:t>{left}</w:t>
    </w:r>
    <w:r><w:tab/></w:r>
    <w:r>
      <w:rPr><w:rFonts w:ascii="Segoe UI" w:hAnsi="Segoe UI"/><w:sz w:val="16"/><w:color w:val="555555"/></w:rPr>
      <w:t xml:space="preserve">Page </w:t>
    </w:r>
    <w:r><w:rPr><w:sz w:val="16"/><w:color w:val="555555"/></w:rPr><w:fldChar w:fldCharType="begin"/></w:r>
    <w:r><w:rPr><w:sz w:val="16"/><w:color w:val="555555"/></w:rPr><w:instrText xml:space="preserve"> PAGE </w:instrText></w:r>
    <w:r><w:rPr><w:sz w:val="16"/><w:color w:val="555555"/></w:rPr><w:fldChar w:fldCharType="separate"/></w:r>
    <w:r><w:rPr><w:sz w:val="16"/><w:color w:val="555555"/></w:rPr><w:t>1</w:t></w:r>
    <w:r><w:rPr><w:sz w:val="16"/><w:color w:val="555555"/></w:rPr><w:fldChar w:fldCharType="end"/></w:r>
    <w:r>
      <w:rPr><w:rFonts w:ascii="Segoe UI" w:hAnsi="Segoe UI"/><w:sz w:val="16"/><w:color w:val="555555"/></w:rPr>
      <w:t xml:space="preserve"> / </w:t>
    </w:r>
    <w:r><w:rPr><w:sz w:val="16"/><w:color w:val="555555"/></w:rPr><w:fldChar w:fldCharType="begin"/></w:r>
    <w:r><w:rPr><w:sz w:val="16"/><w:color w:val="555555"/></w:rPr><w:instrText xml:space="preserve"> NUMPAGES </w:instrText></w:r>
    <w:r><w:rPr><w:sz w:val="16"/><w:color w:val="555555"/></w:rPr><w:fldChar w:fldCharType="separate"/></w:r>
    <w:r><w:rPr><w:sz w:val="16"/><w:color w:val="555555"/></w:rPr><w:t>1</w:t></w:r>
    <w:r><w:rPr><w:sz w:val="16"/><w:color w:val="555555"/></w:rPr><w:fldChar w:fldCharType="end"/></w:r>
  </w:p>
</w:ftr>"#
    )
}

/// Word "Fichier > Propriétés" core metadata. Visible in both Word and File
/// Explorer's right-click > Properties > Details panel.
fn generate_core_props_xml(project_name: &str, brand: &BrandConfig) -> String {
    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    let display_project = brand.client_name.as_deref().unwrap_or(project_name);
    let creator = format!("Code Explorer ({})", brand.company());
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<cp:coreProperties xmlns:cp="http://schemas.openxmlformats.org/package/2006/metadata/core-properties"
                   xmlns:dc="http://purl.org/dc/elements/1.1/"
                   xmlns:dcterms="http://purl.org/dc/terms/"
                   xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance">
  <dc:title>{project} — {subtitle}</dc:title>
  <dc:subject>Audit de code et documentation automatisée</dc:subject>
  <dc:creator>{creator}</dc:creator>
  <cp:keywords>documentation, audit, code intelligence, code-explorer, {company}</cp:keywords>
  <dc:description>Document généré par Code Explorer</dc:description>
  <cp:lastModifiedBy>Code Explorer</cp:lastModifiedBy>
  <cp:revision>1</cp:revision>
  <dcterms:created xsi:type="dcterms:W3CDTF">{now}</dcterms:created>
  <dcterms:modified xsi:type="dcterms:W3CDTF">{now}</dcterms:modified>
  <cp:category>Documentation</cp:category>
</cp:coreProperties>"#,
        project = xml_escape(display_project),
        subtitle = xml_escape(brand.document_title()),
        creator = xml_escape(&creator),
        company = xml_escape(brand.company()),
        now = now
    )
}

/// Extended properties — Application identifies the producer in
/// "Fichier > Propriétés > Détails > Application", Company branded
/// from `BrandConfig`.
fn generate_app_props_xml(brand: &BrandConfig) -> String {
    let company = xml_escape(brand.company());
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Properties xmlns="http://schemas.openxmlformats.org/officeDocument/2006/extended-properties"
            xmlns:vt="http://schemas.openxmlformats.org/officeDocument/2006/docPropsVTypes">
  <Template>Normal.dotm</Template>
  <TotalTime>0</TotalTime>
  <Pages>1</Pages>
  <Words>0</Words>
  <Characters>0</Characters>
  <Application>Code Explorer - Code Intelligence Engine</Application>
  <DocSecurity>0</DocSecurity>
  <Lines>0</Lines>
  <Paragraphs>0</Paragraphs>
  <ScaleCrop>false</ScaleCrop>
  <HeadingPairs>
    <vt:vector size="2" baseType="variant">
      <vt:variant><vt:lpstr>Title</vt:lpstr></vt:variant>
      <vt:variant><vt:i4>1</vt:i4></vt:variant>
    </vt:vector>
  </HeadingPairs>
  <TitlesOfParts>
    <vt:vector size="1" baseType="lpstr"><vt:lpstr>Document</vt:lpstr></vt:vector>
  </TitlesOfParts>
  <Manager/>
  <Company>{company}</Company>
  <LinksUpToDate>false</LinksUpToDate>
  <CharactersWithSpaces>0</CharactersWithSpaces>
  <SharedDoc>false</SharedDoc>
  <HyperlinkBase/>
  <HyperlinksChanged>false</HyperlinksChanged>
  <AppVersion>16.0000</AppVersion>
</Properties>"#,
        company = company
    )
}

fn generate_styles_xml() -> String {
    r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:styles xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:docDefaults>
    <w:rPrDefault>
      <w:rPr>
        <w:rFonts w:ascii="Georgia" w:hAnsi="Georgia" w:cs="Georgia"/>
        <w:sz w:val="22"/>
        <w:szCs w:val="22"/>
        <w:lang w:val="fr-FR"/>
      </w:rPr>
    </w:rPrDefault>
    <w:pPrDefault>
      <w:pPr>
        <w:spacing w:after="160" w:line="259" w:lineRule="auto"/>
      </w:pPr>
    </w:pPrDefault>
  </w:docDefaults>
  <w:style w:type="paragraph" w:styleId="Heading1">
    <w:name w:val="heading 1"/>
    <w:pPr><w:keepNext/><w:spacing w:before="480" w:after="200"/><w:outlineLvl w:val="0"/>
      <w:pBdr><w:bottom w:val="single" w:sz="4" w:space="4" w:color="1B3A6B"/></w:pBdr>
    </w:pPr>
    <w:rPr><w:rFonts w:ascii="Georgia" w:hAnsi="Georgia"/><w:b/><w:sz w:val="36"/><w:color w:val="1B3A6B"/></w:rPr>
  </w:style>
  <w:style w:type="paragraph" w:styleId="Heading2">
    <w:name w:val="heading 2"/>
    <w:pPr><w:keepNext/><w:spacing w:before="360" w:after="160"/><w:outlineLvl w:val="1"/></w:pPr>
    <w:rPr><w:rFonts w:ascii="Georgia" w:hAnsi="Georgia"/><w:b/><w:sz w:val="30"/><w:color w:val="2E5EA0"/></w:rPr>
  </w:style>
  <w:style w:type="paragraph" w:styleId="Heading3">
    <w:name w:val="heading 3"/>
    <w:pPr><w:keepNext/><w:spacing w:before="240" w:after="120"/><w:outlineLvl w:val="2"/></w:pPr>
    <w:rPr><w:rFonts w:ascii="Georgia" w:hAnsi="Georgia"/><w:b/><w:sz w:val="26"/><w:color w:val="4472C4"/></w:rPr>
  </w:style>
  <w:style w:type="paragraph" w:styleId="Heading4">
    <w:name w:val="heading 4"/>
    <w:pPr><w:keepNext/><w:spacing w:before="200" w:after="80"/><w:outlineLvl w:val="3"/></w:pPr>
    <w:rPr><w:rFonts w:ascii="Georgia" w:hAnsi="Georgia"/><w:b/><w:i/><w:sz w:val="24"/><w:color w:val="4472C4"/></w:rPr>
  </w:style>
  <w:style w:type="paragraph" w:styleId="Heading5">
    <w:name w:val="heading 5"/>
    <w:pPr><w:keepNext/><w:spacing w:before="160" w:after="60"/><w:outlineLvl w:val="4"/></w:pPr>
    <w:rPr><w:rFonts w:ascii="Georgia" w:hAnsi="Georgia"/><w:b/><w:sz w:val="22"/><w:color w:val="5B9BD5"/></w:rPr>
  </w:style>
  <w:style w:type="paragraph" w:styleId="Heading6">
    <w:name w:val="heading 6"/>
    <w:pPr><w:keepNext/><w:spacing w:before="120" w:after="40"/><w:outlineLvl w:val="5"/></w:pPr>
    <w:rPr><w:rFonts w:ascii="Georgia" w:hAnsi="Georgia"/><w:b/><w:i/><w:sz w:val="20"/><w:color w:val="5B9BD5"/></w:rPr>
  </w:style>
  <w:style w:type="paragraph" w:styleId="ListBullet">
    <w:name w:val="List Bullet"/>
    <w:pPr><w:spacing w:after="80"/><w:ind w:left="720" w:hanging="360"/></w:pPr>
  </w:style>
  <w:style w:type="paragraph" w:styleId="ListNumber">
    <w:name w:val="List Number"/>
    <w:pPr><w:spacing w:after="80"/><w:ind w:left="720" w:hanging="360"/></w:pPr>
  </w:style>
  <w:style w:type="table" w:styleId="TableGrid">
    <w:name w:val="Table Grid"/>
    <w:tblPr>
      <w:tblBorders>
        <w:top w:val="single" w:sz="4" w:space="0" w:color="BFBFBF"/>
        <w:left w:val="single" w:sz="4" w:space="0" w:color="BFBFBF"/>
        <w:bottom w:val="single" w:sz="4" w:space="0" w:color="BFBFBF"/>
        <w:right w:val="single" w:sz="4" w:space="0" w:color="BFBFBF"/>
        <w:insideH w:val="single" w:sz="4" w:space="0" w:color="BFBFBF"/>
        <w:insideV w:val="single" w:sz="4" w:space="0" w:color="BFBFBF"/>
      </w:tblBorders>
    </w:tblPr>
  </w:style>
</w:styles>"#
        .to_string()
}

const NUMBERING_XML: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:numbering xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:abstractNum w:abstractNumId="0">
    <w:lvl w:ilvl="0">
      <w:start w:val="1"/>
      <w:numFmt w:val="bullet"/>
      <w:lvlText w:val="&#x2022;"/>
      <w:lvlJc w:val="left"/>
      <w:pPr><w:ind w:left="720" w:hanging="360"/></w:pPr>
      <w:rPr><w:rFonts w:ascii="Symbol" w:hAnsi="Symbol" w:hint="default"/></w:rPr>
    </w:lvl>
    <w:lvl w:ilvl="1">
      <w:start w:val="1"/>
      <w:numFmt w:val="bullet"/>
      <w:lvlText w:val="&#x25E6;"/>
      <w:lvlJc w:val="left"/>
      <w:pPr><w:ind w:left="1080" w:hanging="360"/></w:pPr>
      <w:rPr><w:rFonts w:ascii="Courier New" w:hAnsi="Courier New" w:hint="default"/></w:rPr>
    </w:lvl>
  </w:abstractNum>
  <w:abstractNum w:abstractNumId="1">
    <w:lvl w:ilvl="0">
      <w:start w:val="1"/>
      <w:numFmt w:val="decimal"/>
      <w:lvlText w:val="%1."/>
      <w:lvlJc w:val="left"/>
      <w:pPr><w:ind w:left="720" w:hanging="360"/></w:pPr>
    </w:lvl>
  </w:abstractNum>
  <w:num w:numId="1"><w:abstractNumId w:val="0"/></w:num>
  <w:num w:numId="2"><w:abstractNumId w:val="1"/></w:num>
</w:numbering>"#;

#[cfg(test)]
mod tests {
    use std::io::Read;

    use zip::ZipArchive;

    use super::{code_block, export_markdown_as_docx, table_to_ooxml};

    #[test]
    fn export_markdown_as_docx_writes_valid_package() {
        let path = std::env::temp_dir().join(format!(
            "code-explorer-workdoc-docx-test-{}.docx",
            uuid::Uuid::new_v4()
        ));
        export_markdown_as_docx(
            "# Livrable\n\n| Métadonnée | Valeur |\n|---|---|\n| Questions extraites | 12 |\n| Questions répondues | 5 |\n| Statut du livrable | Prêt pour relecture finale |\n\n## Contrôle qualité documentaire\n\n| Contrôle | Valeur | Statut |\n|---|---:|---|\n| Fichiers sources cités | 9 | OK |\n\n## Question\n\n> [!WARNING]\n> Relire les sources citées avant diffusion.\n\nRéponse détaillée avec `code`.",
            &path,
            "Livrable Code Explorer",
        )
        .expect("DOCX generation should succeed");

        let file = std::fs::File::open(&path).expect("generated docx should be readable");
        let mut archive = ZipArchive::new(file).expect("docx should be a zip package");
        let mut document_xml = String::new();
        archive
            .by_name("word/document.xml")
            .expect("document.xml should exist")
            .read_to_string(&mut document_xml)
            .expect("document.xml should be utf-8 xml");
        let mut styles_xml = String::new();
        archive
            .by_name("word/styles.xml")
            .expect("styles.xml should exist")
            .read_to_string(&mut styles_xml)
            .expect("styles.xml should be utf-8 xml");

        assert!(document_xml.contains("CODE EXPLORER DOCUMENT DE TRAVAIL"));
        assert!(document_xml.contains("Questions extraites"));
        assert!(document_xml.contains("Réponses"));
        assert!(document_xml.contains("Prêt pour relecture finale"));
        assert!(document_xml.contains("Sources"));
        assert!(document_xml.contains("Livrable"));
        assert!(document_xml.contains("Réponse détaillée"));
        assert!(document_xml.contains("Point d&apos;attention"));
        assert!(document_xml.contains("Relire les sources"));
        assert!(styles_xml.contains(r#"w:ascii="Georgia""#));
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn export_markdown_as_docx_keeps_hyperlink_relationship_ids_unique() {
        let path = std::env::temp_dir().join(format!(
            "code-explorer-workdoc-links-test-{}.docx",
            uuid::Uuid::new_v4()
        ));
        export_markdown_as_docx(
            "# Liens\n\n[Premier](https://example.com/a)\n\n[Second](https://example.com/b)",
            &path,
            "Liens Code Explorer",
        )
        .expect("DOCX generation should succeed");

        let file = std::fs::File::open(&path).expect("generated docx should be readable");
        let mut archive = ZipArchive::new(file).expect("docx should be a zip package");
        let mut document_xml = String::new();
        archive
            .by_name("word/document.xml")
            .expect("document.xml should exist")
            .read_to_string(&mut document_xml)
            .expect("document.xml should be utf-8 xml");
        let mut rels_xml = String::new();
        archive
            .by_name("word/_rels/document.xml.rels")
            .expect("document rels should exist")
            .read_to_string(&mut rels_xml)
            .expect("rels should be utf-8 xml");

        assert!(document_xml.contains("r:id=\"rId10\""));
        assert!(document_xml.contains("r:id=\"rId11\""));
        assert_eq!(rels_xml.matches("Id=\"rId10\"").count(), 1);
        assert_eq!(rels_xml.matches("Id=\"rId11\"").count(), 1);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn export_markdown_as_docx_embeds_markdown_data_uri_images() {
        let path = std::env::temp_dir().join(format!(
            "code-explorer-workdoc-image-test-{}.docx",
            uuid::Uuid::new_v4()
        ));
        let tiny_png =
            "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==";
        export_markdown_as_docx(
            &format!(
                "# Analyse fonctionnelle\n\n![Capture fonctionnelle 1](data:image/png;base64,{tiny_png})"
            ),
            &path,
            "Images Code Explorer",
        )
        .expect("DOCX generation should succeed");

        let file = std::fs::File::open(&path).expect("generated docx should be readable");
        let mut archive = ZipArchive::new(file).expect("docx should be a zip package");
        let mut document_xml = String::new();
        archive
            .by_name("word/document.xml")
            .expect("document.xml should exist")
            .read_to_string(&mut document_xml)
            .expect("document.xml should be utf-8 xml");
        let mut rels_xml = String::new();
        archive
            .by_name("word/_rels/document.xml.rels")
            .expect("document rels should exist")
            .read_to_string(&mut rels_xml)
            .expect("rels should be utf-8 xml");
        let mut image_bytes = Vec::new();
        archive
            .by_name("word/media/image1.png")
            .expect("embedded png should exist")
            .read_to_end(&mut image_bytes)
            .expect("embedded png should be readable");

        assert!(document_xml.contains("r:embed=\"rIdImg1\""));
        assert!(document_xml.contains("Capture fonctionnelle 1"));
        assert!(rels_xml.contains("Target=\"media/image1.png\""));
        assert!(image_bytes.starts_with(b"\x89PNG\r\n\x1a\n"));
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn wide_narrative_tables_are_rendered_as_readable_cards() {
        let lines = [
            "| # | Question | État | Sources | Diagrammes |",
            "|---|---|---|---:|---:|",
            "| 1 | Mais le plus petit taux provient d’un barème ? Donc comme il n’y a jusqu’ici qu’un seul barème par groupe, cette règle de Min ne s’applique jamais ? | Répondue | 7 | 1 |",
        ];

        let (xml, _links) = table_to_ooxml(&lines);

        assert!(xml.contains(r#"<w:tblW w:w="9000" w:type="dxa"/>"#));
        assert!(xml.contains("Question 1"));
        assert!(xml.contains("Question :"));
        assert!(xml.contains("Sources :"));
        assert!(!xml.contains(r#"<w:gridCol w:w="1800"/>"#));
    }

    #[test]
    fn cartography_tables_with_code_paths_are_rendered_as_cards() {
        let lines = [
            "| Canal | Éléments trouvés | Commentaire |",
            "|---|---|---|",
            "| **Écran / vue** | `Acme.Sample.ihm/Views/Administration/VuesPartiellesGroupe/PlafondsGroupe.cshtml` | Affichage des plafonds groupe : validité, plafond max, plancher, période, majorations |",
        ];

        let (xml, _links) = table_to_ooxml(&lines);

        assert!(xml.contains(r#"<w:tblW w:w="9000" w:type="dxa"/>"#));
        assert!(xml.contains("Canal :"));
        assert!(xml.contains("Éléments trouvés :"));
        assert!(!xml.contains(r#"<w:gridCol w:w="3000"/>"#));
    }

    #[test]
    fn code_blocks_render_as_single_padded_panel() {
        let xml = code_block("public void Test()\\n{\\n    return;\\n}", "csharp");

        assert!(xml.contains(r#"<w:tblW w:w="9000" w:type="dxa"/>"#));
        assert!(xml.contains(r#"<w:shd w:val="clear" w:color="auto" w:fill="F6F8FA"/>"#));
        assert!(xml.contains("public void Test()"));
        assert!(xml.contains("    return;"));
    }
}

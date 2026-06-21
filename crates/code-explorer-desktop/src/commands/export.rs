//! Export commands for generating DOCX documentation from the desktop app.
//!
//! Uses _index.json to discover all documentation pages (including ASP.NET)
//! and converts them to a professional Word document with full formatting.

use std::path::PathBuf;

use tauri::State;

use crate::state::AppState;

/// Export documentation as a DOCX file.
/// Returns the path to the generated file.
#[tauri::command]
pub async fn export_docs_docx(state: State<'_, AppState>) -> Result<String, String> {
    let repo_path = get_active_repo_path(&state).await?;
    let docs_dir = repo_path.join(".codeexplorer").join("docs");

    if !docs_dir.exists() {
        return Err(
            "Documentation not generated yet. Run 'Analyze' first, then generate docs.".to_string(),
        );
    }

    let output_path = repo_path.join(".codeexplorer").join("documentation.docx");
    let repo_name = repo_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("Project");

    let docs_dir_clone = docs_dir.clone();
    let output_clone = output_path.clone();
    let name = repo_name.to_string();

    tokio::task::spawn_blocking(move || {
        generate_docx_from_docs(&docs_dir_clone, &output_clone, &name)
    })
    .await
    .map_err(|e| format!("Task error: {}", e))?
    .map_err(|e| format!("DOCX generation failed: {}", e))?;

    Ok(output_path.display().to_string())
}

/// Export documentation as an Obsidian Vault.
#[tauri::command]
pub async fn export_obsidian_vault(state: State<'_, AppState>) -> Result<String, String> {
    let repo_path = get_active_repo_path(&state).await?;
    let (graph, _, _, _) = state.get_repo(None).await?;
    let docs_dir = repo_path.join(".codeexplorer").join("docs");

    if !docs_dir.exists() {
        std::fs::create_dir_all(&docs_dir).map_err(|e| e.to_string())?;
    }

    use code_explorer_core::graph::types::{NodeLabel, RelationshipType};
    use code_explorer_output::obsidian::{generate_obsidian_vault, CommunityInfo};
    use std::collections::BTreeMap;

    let mut communities = BTreeMap::new();
    for node in graph.iter_nodes() {
        if node.label == NodeLabel::Community {
            let label = node
                .properties
                .heuristic_label
                .clone()
                .unwrap_or_else(|| node.properties.name.clone());
            communities.insert(
                node.id.clone(),
                CommunityInfo {
                    label,
                    description: node.properties.description.clone(),
                    member_ids: Vec::new(),
                },
            );
        }
    }

    for rel in graph.iter_relationships() {
        if rel.rel_type == RelationshipType::MemberOf {
            if let Some(info) = communities.get_mut(&rel.target_id) {
                info.member_ids.push(rel.source_id.clone());
            }
        }
    }

    let vault_path = tokio::task::spawn_blocking(move || {
        generate_obsidian_vault(&graph, &docs_dir, &communities)
    })
    .await
    .map_err(|e| format!("Task error: {}", e))?
    .map_err(|e| format!("Obsidian export failed: {}", e))?;

    Ok(vault_path)
}

/// Get ASP.NET specific stats for the UI dashboard.
#[tauri::command]
pub async fn get_aspnet_stats(state: State<'_, AppState>) -> Result<AspNetStats, String> {
    let (graph, _indexes, _fts_index, _repo_path) = state.get_repo(None).await?;

    use code_explorer_core::graph::types::NodeLabel;

    let mut stats = AspNetStats::default();
    for node in graph.iter_nodes() {
        match node.label {
            NodeLabel::Controller => stats.controllers += 1,
            NodeLabel::ControllerAction => stats.actions += 1,
            NodeLabel::ApiEndpoint => stats.api_endpoints += 1,
            NodeLabel::View => stats.views += 1,
            NodeLabel::DbEntity => stats.entities += 1,
            NodeLabel::DbContext => stats.db_contexts += 1,
            NodeLabel::Area => stats.areas += 1,
            _ => {}
        }
    }

    Ok(stats)
}

#[derive(Debug, Default, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AspNetStats {
    pub controllers: usize,
    pub actions: usize,
    pub api_endpoints: usize,
    pub views: usize,
    pub entities: usize,
    pub db_contexts: usize,
    pub areas: usize,
}

// ─── DOCX Generation Engine ──────────────────────────────────────────────
// Reads _index.json for page order, converts all Markdown to OOXML.

fn generate_docx_from_docs(
    docs_dir: &std::path::Path,
    output_path: &std::path::Path,
    project_name: &str,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    use std::io::Write;
    use zip::write::{FileOptions, SimpleFileOptions};
    use zip::ZipWriter;

    // Read _index.json for ordered page list and stats
    let index_path = docs_dir.join("_index.json");
    let (ordered_files, stats) = if index_path.exists() {
        let index_str = std::fs::read_to_string(&index_path)?;
        let index: serde_json::Value = serde_json::from_str(&index_str)?;
        let files = collect_pages_from_index(&index);
        let st = extract_stats(&index);
        (files, st)
    } else {
        (fallback_file_order(), DocStats::default())
    };

    // Read all markdown files in order (with path traversal protection)
    let docs_canonical = docs_dir.canonicalize().map_err(|e| {
        Box::<dyn std::error::Error + Send + Sync>::from(format!(
            "Cannot resolve docs directory: {e}"
        ))
    })?;
    let mut md_files: Vec<(String, String, String)> = Vec::new();
    for (id, title, filename) in &ordered_files {
        let path = docs_dir.join(filename);
        let canonical = match path.canonicalize() {
            Ok(p) => p,
            Err(_) => continue, // file doesn't exist, skip
        };
        if !canonical.starts_with(&docs_canonical) {
            continue; // skip paths outside docs directory
        }
        let content = std::fs::read_to_string(&canonical)?;
        md_files.push((id.clone(), title.clone(), content));
    }

    if md_files.is_empty() {
        return Err("No documentation files found".into());
    }

    let file = std::fs::File::create(output_path)?;
    let mut zip = ZipWriter::new(file);
    let options: SimpleFileOptions =
        FileOptions::default().compression_method(zip::CompressionMethod::Deflated);

    zip.start_file("[Content_Types].xml", options)?;
    zip.write_all(CONTENT_TYPES.as_bytes())?;
    zip.start_file("_rels/.rels", options)?;
    zip.write_all(RELS.as_bytes())?;
    zip.start_file("word/styles.xml", options)?;
    zip.write_all(STYLES.as_bytes())?;
    zip.start_file("word/numbering.xml", options)?;
    zip.write_all(NUMBERING.as_bytes())?;

    // Generate document body and collect hyperlinks
    let mut body = String::new();
    let mut links = Vec::new();
    let date = chrono::Local::now().format("%d/%m/%Y").to_string();

    // Title page with stats
    body.push_str(&format!(
        r#"<w:p><w:pPr><w:spacing w:before="3000"/><w:jc w:val="center"/></w:pPr>
      <w:r><w:rPr><w:b/><w:sz w:val="60"/><w:color w:val="1B3A6B"/></w:rPr>
        <w:t>{}</w:t></w:r></w:p>
    <w:p><w:pPr><w:spacing w:before="200"/><w:jc w:val="center"/></w:pPr>
      <w:r><w:rPr><w:sz w:val="32"/><w:color w:val="4472C4"/></w:rPr>
        <w:t>Documentation Technique et Fonctionnelle</w:t></w:r></w:p>
    <w:p><w:pPr><w:spacing w:before="120"/><w:jc w:val="center"/></w:pPr>
      <w:r><w:rPr><w:sz w:val="22"/><w:color w:val="888888"/></w:rPr>
        <w:t xml:space="preserve">Audit de code automatise — {}</w:t></w:r></w:p>"#,
        xml_escape(project_name),
        date
    ));

    // Stats table on title page
    if stats.files > 0 || stats.nodes > 0 {
        body.push_str(
            r#"<w:p><w:pPr><w:spacing w:before="600"/><w:jc w:val="center"/></w:pPr></w:p>"#,
        );
        body.push_str(r#"<w:tbl><w:tblPr><w:tblW w:w="7000" w:type="dxa"/><w:jc w:val="center"/><w:tblBorders>"#);
        body.push_str(r#"<w:top w:val="single" w:sz="4" w:space="0" w:color="D0D0D0"/><w:bottom w:val="single" w:sz="4" w:space="0" w:color="D0D0D0"/><w:insideH w:val="single" w:sz="4" w:space="0" w:color="D0D0D0"/><w:insideV w:val="single" w:sz="4" w:space="0" w:color="D0D0D0"/>"#);
        body.push_str(r#"</w:tblBorders></w:tblPr><w:tblGrid><w:gridCol w:w="1750"/><w:gridCol w:w="1750"/><w:gridCol w:w="1750"/><w:gridCol w:w="1750"/></w:tblGrid>"#);
        body.push_str("<w:tr>");
        for (label, value) in [
            ("Fichiers", stats.files),
            ("Noeuds", stats.nodes),
            ("Relations", stats.edges),
            ("Modules", stats.modules),
        ] {
            body.push_str(&format!(
                r#"<w:tc><w:tcPr><w:shd w:val="clear" w:color="auto" w:fill="F5F7FA"/><w:tcMar><w:top w:w="80" w:type="dxa"/><w:bottom w:w="80" w:type="dxa"/></w:tcMar></w:tcPr><w:p><w:pPr><w:jc w:val="center"/><w:spacing w:after="0"/></w:pPr><w:r><w:rPr><w:b/><w:sz w:val="28"/><w:color w:val="1B3A6B"/></w:rPr><w:t>{}</w:t></w:r></w:p><w:p><w:pPr><w:jc w:val="center"/><w:spacing w:after="0"/></w:pPr><w:r><w:rPr><w:sz w:val="16"/><w:color w:val="888888"/></w:rPr><w:t>{}</w:t></w:r></w:p></w:tc>"#,
                value, label
            ));
        }
        body.push_str("</w:tr></w:tbl>");
    }

    body.push_str(
        r#"<w:p><w:pPr><w:spacing w:before="800"/><w:jc w:val="center"/></w:pPr>
      <w:r><w:rPr><w:sz w:val="18"/><w:color w:val="AAAAAA"/></w:rPr>
        <w:t xml:space="preserve">Genere automatiquement par Code Explorer</w:t></w:r></w:p>"#,
    );

    body.push_str(PAGE_BREAK);

    // TOC
    body.push_str(r#"<w:p><w:pPr><w:pStyle w:val="Heading1"/></w:pPr><w:r><w:t>Table des matieres</w:t></w:r></w:p>"#);
    body.push_str(r#"<w:p><w:r><w:fldChar w:fldCharType="begin"/></w:r><w:r><w:instrText xml:space="preserve"> TOC \o "1-4" \h \z \u </w:instrText></w:r><w:r><w:fldChar w:fldCharType="separate"/></w:r><w:r><w:rPr><w:i/><w:color w:val="999999"/></w:rPr><w:t>Ctrl+A, F9 pour actualiser la table des matieres</w:t></w:r><w:r><w:fldChar w:fldCharType="end"/></w:r></w:p>"#);
    body.push_str(PAGE_BREAK);

    // Each markdown file
    let mut rid_counter: usize = 10; // rId1–9 reserved for core rels
    for (i, (_id, _title, content)) in md_files.iter().enumerate() {
        let (ooxml, doc_links) = md_to_ooxml(content, &mut rid_counter);
        body.push_str(&ooxml);
        links.extend(doc_links);
        if i < md_files.len() - 1 {
            body.push_str(PAGE_BREAK);
        }
    }

    let document = format!(
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
  <w:body>{body}
    <w:sectPr><w:pgSz w:w="11906" w:h="16838"/>
      <w:pgMar w:top="1440" w:right="1440" w:bottom="1440" w:left="1440"/></w:sectPr>
  </w:body></w:document>"#
    );

    zip.start_file("word/document.xml", options)?;
    zip.write_all(document.as_bytes())?;

    // Write dynamic document.xml.rels with hyperlinks
    let doc_rels = generate_doc_rels(&links);
    zip.start_file("word/_rels/document.xml.rels", options)?;
    zip.write_all(doc_rels.as_bytes())?;

    zip.finish()?;

    Ok(())
}

const PAGE_BREAK: &str = r#"<w:p><w:r><w:br w:type="page"/></w:r></w:p>"#;

// ─── _index.json Parsing ─────────────────────────────────────────────────

#[derive(Default)]
struct DocStats {
    files: usize,
    nodes: usize,
    edges: usize,
    modules: usize,
}

fn extract_stats(index: &serde_json::Value) -> DocStats {
    let s = &index["stats"];
    DocStats {
        files: s["files"].as_u64().unwrap_or(0) as usize,
        nodes: s["nodes"].as_u64().unwrap_or(0) as usize,
        edges: s["edges"].as_u64().unwrap_or(0) as usize,
        modules: s["modules"].as_u64().unwrap_or(0) as usize,
    }
}

fn collect_pages_from_index(index: &serde_json::Value) -> Vec<(String, String, String)> {
    let mut result = Vec::new();
    if let Some(pages) = index["pages"].as_array() {
        for page in pages {
            collect_page_recursive(page, &mut result);
        }
    }
    result
}

fn collect_page_recursive(page: &serde_json::Value, out: &mut Vec<(String, String, String)>) {
    let id = page["id"].as_str().unwrap_or("").to_string();
    let title = page["title"].as_str().unwrap_or("").to_string();
    if let Some(path) = page["path"].as_str() {
        out.push((id, title, path.to_string()));
    }
    if let Some(children) = page["children"].as_array() {
        for child in children {
            collect_page_recursive(child, out);
        }
    }
}

fn fallback_file_order() -> Vec<(String, String, String)> {
    [
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
    ]
    .iter()
    .map(|(id, title, path)| (id.to_string(), title.to_string(), path.to_string()))
    .collect()
}

// ─── Markdown to OOXML ──────────────────────────────────────────────────

fn md_to_ooxml(md: &str, rid_counter: &mut usize) -> (String, Vec<(String, String)>) {
    let mut out = String::new();
    let mut links = Vec::new();
    let lines: Vec<&str> = md.lines().collect();
    let mut i = 0;

    while i < lines.len() {
        let t = lines[i].trim();
        if t.is_empty() {
            i += 1;
            continue;
        }

        // Headings (check H6 first to avoid prefix conflicts)
        if let Some(rest) = t.strip_prefix("###### ") {
            let (ooxml, hdr_links) = heading(rest, 6, rid_counter);
            out.push_str(&ooxml);
            links.extend(hdr_links);
            i += 1;
            continue;
        }
        if let Some(rest) = t.strip_prefix("##### ") {
            let (ooxml, hdr_links) = heading(rest, 5, rid_counter);
            out.push_str(&ooxml);
            links.extend(hdr_links);
            i += 1;
            continue;
        }
        if let Some(rest) = t.strip_prefix("#### ") {
            let (ooxml, hdr_links) = heading(rest, 4, rid_counter);
            out.push_str(&ooxml);
            links.extend(hdr_links);
            i += 1;
            continue;
        }
        if let Some(rest) = t.strip_prefix("### ") {
            let (ooxml, hdr_links) = heading(rest, 3, rid_counter);
            out.push_str(&ooxml);
            links.extend(hdr_links);
            i += 1;
            continue;
        }
        if let Some(rest) = t.strip_prefix("## ") {
            let (ooxml, hdr_links) = heading(rest, 2, rid_counter);
            out.push_str(&ooxml);
            links.extend(hdr_links);
            i += 1;
            continue;
        }
        if let Some(rest) = t.strip_prefix("# ") {
            let (ooxml, hdr_links) = heading(rest, 1, rid_counter);
            out.push_str(&ooxml);
            links.extend(hdr_links);
            i += 1;
            continue;
        }

        // Code blocks
        if t.starts_with("```") {
            let lang = t.strip_prefix("```").unwrap_or("").trim();
            let mut code = Vec::new();
            i += 1;
            while i < lines.len() && !lines[i].trim().starts_with("```") {
                code.push(lines[i]);
                i += 1;
            }
            i += 1;
            if lang == "mermaid" {
                out.push_str(&mermaid_placeholder(&code.join("\n")));
            } else {
                out.push_str(&code_block(&code.join("\n"), lang));
            }
            continue;
        }

        // Tables
        if t.starts_with('|') && t.contains('|') {
            let mut rows = Vec::new();
            while i < lines.len() && lines[i].trim().starts_with('|') {
                rows.push(lines[i].trim());
                i += 1;
            }
            let (ooxml, tbl_links) = table_ooxml(&rows, rid_counter);
            out.push_str(&ooxml);
            links.extend(tbl_links);
            continue;
        }

        // Nested bullet list (indented 2+ spaces or tab + - or *)
        if (t.starts_with("- ") || t.starts_with("* "))
            && (lines[i].starts_with("  ") || lines[i].starts_with('\t'))
        {
            let content = &t[2..];
            let (runs, item_links) = inline_runs(content, rid_counter);
            links.extend(item_links);
            out.push_str(&format!(
                r#"<w:p><w:pPr><w:pStyle w:val="ListBullet"/><w:numPr><w:ilvl w:val="1"/><w:numId w:val="1"/></w:numPr><w:ind w:left="1080" w:hanging="360"/><w:spacing w:after="80"/></w:pPr>{}</w:p>"#,
                runs
            ));
            i += 1;
            continue;
        }

        // Bullets (level 0)
        if t.starts_with("- ") || t.starts_with("* ") {
            let (runs, item_links) = inline_runs(&t[2..], rid_counter);
            links.extend(item_links);
            out.push_str(&format!(
                r#"<w:p><w:pPr><w:pStyle w:val="ListBullet"/><w:numPr><w:ilvl w:val="0"/><w:numId w:val="1"/></w:numPr><w:spacing w:after="80"/></w:pPr>{}</w:p>"#,
                runs
            ));
            i += 1;
            continue;
        }

        // Numbered list
        if t.len() > 2 && t.chars().next().is_some_and(|c| c.is_ascii_digit()) && t.contains(". ") {
            let dot_pos = t.find(". ").unwrap_or(0);
            let (runs, item_links) = inline_runs(&t[dot_pos + 2..], rid_counter);
            links.extend(item_links);
            out.push_str(&format!(
                r#"<w:p><w:pPr><w:numPr><w:ilvl w:val="0"/><w:numId w:val="2"/></w:numPr><w:spacing w:after="80"/></w:pPr>{}</w:p>"#,
                runs
            ));
            i += 1;
            continue;
        }

        // Blockquote
        if let Some(rest) = t.strip_prefix("> ") {
            let (runs, quote_links) = inline_runs(rest, rid_counter);
            links.extend(quote_links);
            out.push_str(&format!(
                r#"<w:p><w:pPr><w:pBdr><w:left w:val="single" w:sz="18" w:space="8" w:color="4472C4"/></w:pBdr><w:ind w:left="360"/><w:shd w:val="clear" w:color="auto" w:fill="F0F4FA"/></w:pPr>{}</w:p>"#,
                runs
            ));
            i += 1;
            continue;
        }

        // Horizontal rule
        if t == "---" || t == "***" || t == "___" {
            out.push_str(r#"<w:p><w:pPr><w:pBdr><w:bottom w:val="single" w:sz="4" w:space="4" w:color="CCCCCC"/></w:pBdr><w:spacing w:before="200" w:after="200"/></w:pPr></w:p>"#);
            i += 1;
            continue;
        }

        // Regular paragraph with inline formatting
        let (runs, para_links) = inline_runs(t, rid_counter);
        links.extend(para_links);
        out.push_str(&format!(
            r#"<w:p><w:pPr><w:spacing w:after="120"/></w:pPr>{}</w:p>"#,
            runs
        ));
        i += 1;
    }

    (out, links)
}

fn heading(text: &str, level: u32, rid_counter: &mut usize) -> (String, Vec<(String, String)>) {
    let (runs, links) = inline_runs(text, rid_counter);
    (
        format!(
            r#"<w:p><w:pPr><w:pStyle w:val="Heading{level}"/></w:pPr>{}</w:p>"#,
            runs
        ),
        links,
    )
}

fn code_block(code: &str, lang: &str) -> String {
    let mut r = String::new();
    if !lang.is_empty() {
        r.push_str(&format!(
            r#"<w:p><w:pPr><w:shd w:val="clear" w:color="auto" w:fill="E8E8E8"/><w:spacing w:after="0"/></w:pPr><w:r><w:rPr><w:rFonts w:ascii="Consolas" w:hAnsi="Consolas"/><w:sz w:val="16"/><w:b/><w:color w:val="888888"/></w:rPr><w:t xml:space="preserve">{}</w:t></w:r></w:p>"#,
            xml_escape(lang)
        ));
    }
    for line in code.lines() {
        r.push_str(&format!(
            r#"<w:p><w:pPr><w:shd w:val="clear" w:color="auto" w:fill="F2F2F2"/><w:spacing w:after="0" w:line="276" w:lineRule="auto"/></w:pPr><w:r><w:rPr><w:rFonts w:ascii="Consolas" w:hAnsi="Consolas"/><w:sz w:val="18"/><w:color w:val="333333"/></w:rPr><w:t xml:space="preserve">{}</w:t></w:r></w:p>"#,
            xml_escape(line)
        ));
    }
    r.push_str(r#"<w:p><w:pPr><w:spacing w:after="80"/></w:pPr></w:p>"#);
    r
}

fn mermaid_placeholder(code: &str) -> String {
    let mut r = String::new();
    let diagram_type = if code.starts_with("sequenceDiagram") {
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
    };

    r.push_str(&format!(
        r#"<w:p><w:pPr><w:shd w:val="clear" w:color="auto" w:fill="E8F0FE"/><w:spacing w:after="60"/><w:pBdr><w:top w:val="single" w:sz="4" w:space="4" w:color="4472C4"/><w:bottom w:val="single" w:sz="4" w:space="4" w:color="4472C4"/></w:pBdr></w:pPr><w:r><w:rPr><w:b/><w:color w:val="1B3A6B"/><w:sz w:val="22"/></w:rPr><w:t xml:space="preserve">  {diagram_type}</w:t></w:r></w:p>"#,
    ));
    r.push_str(r#"<w:p><w:pPr><w:shd w:val="clear" w:color="auto" w:fill="E8F0FE"/><w:spacing w:after="60"/></w:pPr><w:r><w:rPr><w:i/><w:sz w:val="18"/><w:color w:val="666666"/></w:rPr><w:t xml:space="preserve">Copiez le code ci-dessous dans mermaid.live pour voir le rendu visuel.</w:t></w:r></w:p>"#);
    for line in code.lines() {
        r.push_str(&format!(
            r#"<w:p><w:pPr><w:shd w:val="clear" w:color="auto" w:fill="E8F0FE"/><w:spacing w:after="0"/></w:pPr><w:r><w:rPr><w:rFonts w:ascii="Consolas" w:hAnsi="Consolas"/><w:sz w:val="16"/><w:color w:val="3366AA"/></w:rPr><w:t xml:space="preserve">{}</w:t></w:r></w:p>"#,
            xml_escape(line)
        ));
    }
    r.push_str(r#"<w:p><w:pPr><w:spacing w:after="120"/></w:pPr></w:p>"#);
    r
}

fn table_ooxml(rows: &[&str], rid_counter: &mut usize) -> (String, Vec<(String, String)>) {
    if rows.is_empty() {
        return (String::new(), Vec::new());
    }
    let mut out = String::new();
    let mut links = Vec::new();
    let header = parse_table_row(rows[0]);
    let col_count = header.len();
    if col_count == 0 {
        return (String::new(), Vec::new());
    }
    let data_start = if rows.len() > 1 && rows[1].contains("---") {
        2
    } else {
        1
    };
    let col_w = 9000 / col_count;

    out.push_str(r#"<w:tbl><w:tblPr><w:tblStyle w:val="TableGrid"/><w:tblW w:w="0" w:type="auto"/><w:tblBorders>"#);
    out.push_str(r#"<w:top w:val="single" w:sz="4" w:space="0" w:color="BFBFBF"/><w:left w:val="single" w:sz="4" w:space="0" w:color="BFBFBF"/><w:bottom w:val="single" w:sz="4" w:space="0" w:color="BFBFBF"/><w:right w:val="single" w:sz="4" w:space="0" w:color="BFBFBF"/><w:insideH w:val="single" w:sz="4" w:space="0" w:color="BFBFBF"/><w:insideV w:val="single" w:sz="4" w:space="0" w:color="BFBFBF"/>"#);
    out.push_str(r#"</w:tblBorders></w:tblPr><w:tblGrid>"#);
    for _ in 0..col_count {
        out.push_str(&format!(r#"<w:gridCol w:w="{col_w}"/>"#));
    }
    out.push_str("</w:tblGrid>");

    // Header
    out.push_str("<w:tr>");
    for cell in &header {
        out.push_str(&format!(
            r#"<w:tc><w:tcPr><w:shd w:val="clear" w:color="auto" w:fill="1B3A6B"/><w:tcMar><w:top w:w="40" w:type="dxa"/><w:bottom w:w="40" w:type="dxa"/><w:left w:w="80" w:type="dxa"/></w:tcMar></w:tcPr><w:p><w:pPr><w:spacing w:after="0"/></w:pPr><w:r><w:rPr><w:b/><w:color w:val="FFFFFF"/><w:sz w:val="20"/></w:rPr><w:t xml:space="preserve">{}</w:t></w:r></w:p></w:tc>"#,
            xml_escape(cell)
        ));
    }
    out.push_str("</w:tr>");

    // Data rows
    for (i, row) in rows.iter().enumerate().skip(data_start) {
        let cells = parse_table_row(row);
        let bg = if (i - data_start) % 2 == 0 {
            "FFFFFF"
        } else {
            "F5F7FA"
        };
        out.push_str("<w:tr>");
        for (j, cell) in cells.iter().enumerate() {
            if j < col_count {
                let (runs, cell_links) = inline_runs(cell, rid_counter);
                links.extend(cell_links);
                out.push_str(&format!(
                    r#"<w:tc><w:tcPr><w:shd w:val="clear" w:color="auto" w:fill="{bg}"/><w:tcMar><w:top w:w="30" w:type="dxa"/><w:bottom w:w="30" w:type="dxa"/><w:left w:w="80" w:type="dxa"/></w:tcMar></w:tcPr><w:p><w:pPr><w:spacing w:after="0"/></w:pPr>{}</w:p></w:tc>"#,
                    runs
                ));
            }
        }
        for _ in cells.len()..col_count {
            out.push_str(&format!(r#"<w:tc><w:tcPr><w:shd w:val="clear" w:color="auto" w:fill="{bg}"/></w:tcPr><w:p></w:p></w:tc>"#));
        }
        out.push_str("</w:tr>");
    }

    out.push_str("</w:tbl>");
    out.push_str(r#"<w:p><w:pPr><w:spacing w:after="160"/></w:pPr></w:p>"#);
    (out, links)
}

/// Split a markdown pipe row into cells, dropping only the leading and
/// trailing empty tokens that come from the surrounding `|` sentinels.
/// Interior empty cells (e.g. `| a | | c |`) are preserved — naively
/// filtering all empties shifts every subsequent column one position left
/// and corrupts the rendered table, breaking alignment with the header
/// (which determines `col_count`).
fn parse_table_row(line: &str) -> Vec<String> {
    let raw: Vec<String> = line.split('|').map(|s| s.trim().to_string()).collect();
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

/// Handle inline markdown: **bold**, *italic*, `code`, [text](url)
/// Returns (ooxml_string, vec_of_links) where links are (rId, url) pairs.
fn inline_runs(text: &str, rid_counter: &mut usize) -> (String, Vec<(String, String)>) {
    let mut result = String::new();
    let mut links = Vec::new();
    let chars: Vec<char> = text.chars().collect();
    let len = chars.len();
    let mut i = 0;

    while i < len {
        // Bold **text**
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
        // Inline code `text`
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
        // Link [text](url)
        if chars[i] == '[' {
            if let Some((link_text, link_url, end_pos)) = parse_link(&chars, i) {
                let rid = format!("rId{}", *rid_counter);
                *rid_counter += 1;
                // Record the link
                links.push((rid.clone(), link_url));
                // Render link as clickable hyperlink element
                result.push_str(&format!(
                    r#"<w:hyperlink r:id="{rid}"><w:r><w:rPr><w:color w:val="2E5EA0"/><w:u w:val="single"/><w:rStyle w:val="Hyperlink"/></w:rPr><w:t xml:space="preserve">{}</w:t></w:r></w:hyperlink>"#,
                    xml_escape(&link_text)
                ));
                i = end_pos;
                continue;
            }
        }
        // Italic *text*
        if chars[i] == '*' && (i + 1 >= len || chars[i + 1] != '*') {
            if let Some(end) = chars[i + 1..].iter().position(|&c| c == '*') {
                let inner: String = chars[i + 1..i + 1 + end].iter().collect();
                result.push_str(&format!(
                    r#"<w:r><w:rPr><w:i/></w:rPr><w:t xml:space="preserve">{}</w:t></w:r>"#,
                    xml_escape(&inner)
                ));
                i = i + 1 + end + 1;
                continue;
            }
        }
        // Regular text
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
            result.push_str(&format!(
                r#"<w:r><w:t xml:space="preserve">{}</w:t></w:r>"#,
                xml_escape(&chars[i].to_string())
            ));
            i += 1;
        }
    }
    (result, links)
}

fn parse_link(chars: &[char], start: usize) -> Option<(String, String, usize)> {
    let mut j = start + 1;
    while j < chars.len() && chars[j] != ']' {
        j += 1;
    }
    if j >= chars.len() {
        return None;
    }
    let text: String = chars[start + 1..j].iter().collect();
    if j + 1 >= chars.len() || chars[j + 1] != '(' {
        return None;
    }
    let mut k = j + 2;
    while k < chars.len() && chars[k] != ')' {
        k += 1;
    }
    if k >= chars.len() {
        return None;
    }
    let url: String = chars[j + 2..k].iter().collect();
    Some((text, url, k + 1))
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

const CONTENT_TYPES: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
  <Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
  <Default Extension="xml" ContentType="application/xml"/>
  <Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>
  <Override PartName="/word/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml"/>
  <Override PartName="/word/numbering.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.numbering+xml"/>
</Types>"#;

const RELS: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/>
</Relationships>"#;

fn generate_doc_rels(links: &[(String, String)]) -> String {
    let mut rels = String::from(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/>
  <Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/numbering" Target="numbering.xml"/>"#,
    );

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

const STYLES: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:styles xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:docDefaults><w:rPrDefault><w:rPr><w:rFonts w:ascii="Segoe UI" w:hAnsi="Segoe UI" w:cs="Segoe UI"/><w:sz w:val="22"/><w:szCs w:val="22"/><w:lang w:val="fr-FR"/></w:rPr></w:rPrDefault>
    <w:pPrDefault><w:pPr><w:spacing w:after="160" w:line="259" w:lineRule="auto"/></w:pPr></w:pPrDefault></w:docDefaults>
  <w:style w:type="paragraph" w:styleId="Heading1"><w:name w:val="heading 1"/><w:pPr><w:keepNext/><w:spacing w:before="480" w:after="200"/><w:outlineLvl w:val="0"/><w:pBdr><w:bottom w:val="single" w:sz="4" w:space="4" w:color="1B3A6B"/></w:pBdr></w:pPr><w:rPr><w:b/><w:sz w:val="36"/><w:color w:val="1B3A6B"/></w:rPr></w:style>
  <w:style w:type="paragraph" w:styleId="Heading2"><w:name w:val="heading 2"/><w:pPr><w:keepNext/><w:spacing w:before="360" w:after="160"/><w:outlineLvl w:val="1"/></w:pPr><w:rPr><w:b/><w:sz w:val="30"/><w:color w:val="2E5EA0"/></w:rPr></w:style>
  <w:style w:type="paragraph" w:styleId="Heading3"><w:name w:val="heading 3"/><w:pPr><w:keepNext/><w:spacing w:before="240" w:after="120"/><w:outlineLvl w:val="2"/></w:pPr><w:rPr><w:b/><w:sz w:val="26"/><w:color w:val="4472C4"/></w:rPr></w:style>
  <w:style w:type="paragraph" w:styleId="Heading4"><w:name w:val="heading 4"/><w:pPr><w:keepNext/><w:spacing w:before="200" w:after="80"/><w:outlineLvl w:val="3"/></w:pPr><w:rPr><w:b/><w:i/><w:sz w:val="24"/><w:color w:val="4472C4"/></w:rPr></w:style>
  <w:style w:type="paragraph" w:styleId="Heading5"><w:name w:val="heading 5"/><w:pPr><w:keepNext/><w:spacing w:before="160" w:after="60"/><w:outlineLvl w:val="4"/></w:pPr><w:rPr><w:b/><w:sz w:val="22"/><w:color w:val="5B9BD5"/></w:rPr></w:style>
  <w:style w:type="paragraph" w:styleId="Heading6"><w:name w:val="heading 6"/><w:pPr><w:keepNext/><w:spacing w:before="120" w:after="40"/><w:outlineLvl w:val="5"/></w:pPr><w:rPr><w:b/><w:i/><w:sz w:val="20"/><w:color w:val="5B9BD5"/></w:rPr></w:style>
  <w:style w:type="paragraph" w:styleId="ListBullet"><w:name w:val="List Bullet"/><w:pPr><w:ind w:left="720" w:hanging="360"/></w:pPr></w:style>
  <w:style w:type="paragraph" w:styleId="ListNumber"><w:name w:val="List Number"/><w:pPr><w:ind w:left="720" w:hanging="360"/></w:pPr></w:style>
  <w:style w:type="table" w:styleId="TableGrid"><w:name w:val="Table Grid"/></w:style>
</w:styles>"#;

const NUMBERING: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:numbering xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:abstractNum w:abstractNumId="0">
    <w:lvl w:ilvl="0">
      <w:start w:val="1"/><w:numFmt w:val="bullet"/><w:lvlText w:val="&#x2022;"/><w:lvlJc w:val="left"/>
      <w:pPr><w:ind w:left="720" w:hanging="360"/></w:pPr>
      <w:rPr><w:rFonts w:ascii="Symbol" w:hAnsi="Symbol" w:hint="default"/></w:rPr>
    </w:lvl>
    <w:lvl w:ilvl="1">
      <w:start w:val="1"/><w:numFmt w:val="bullet"/><w:lvlText w:val="&#x25E6;"/><w:lvlJc w:val="left"/>
      <w:pPr><w:ind w:left="1080" w:hanging="360"/></w:pPr>
      <w:rPr><w:rFonts w:ascii="Courier New" w:hAnsi="Courier New" w:hint="default"/></w:rPr>
    </w:lvl>
  </w:abstractNum>
  <w:abstractNum w:abstractNumId="1">
    <w:lvl w:ilvl="0">
      <w:start w:val="1"/><w:numFmt w:val="decimal"/><w:lvlText w:val="%1."/><w:lvlJc w:val="left"/>
      <w:pPr><w:ind w:left="720" w:hanging="360"/></w:pPr>
    </w:lvl>
  </w:abstractNum>
  <w:num w:numId="1"><w:abstractNumId w:val="0"/></w:num>
  <w:num w:numId="2"><w:abstractNumId w:val="1"/></w:num>
</w:numbering>"#;

/// Helper to get the active repo path
async fn get_active_repo_path(state: &State<'_, AppState>) -> Result<PathBuf, String> {
    let name = state
        .active_repo_name()
        .await
        .ok_or_else(|| "No active repository".to_string())?;

    let registry = state.registry().await;
    let entry = registry
        .iter()
        .find(|e| e.name == name)
        .ok_or_else(|| format!("Repository '{}' not found", name))?;

    Ok(PathBuf::from(&entry.path))
}

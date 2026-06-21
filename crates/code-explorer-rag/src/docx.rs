//! Extractor for Microsoft Word .docx (Office Open XML / OOXML) files.
//!
//! Reads `word/document.xml` from the zip container and emits a markdown-like
//! string: `w:pStyle` values starting with "Heading"/"Titre"/"Title" become
//! `#`-prefixed headings, everything else is flattened to paragraphs. Tables
//! are walked into and each cell is emitted as its own paragraph (a pragmatic
//! choice — we're feeding a GraphRAG chunker, not a Word viewer).
//!
//! The produced markdown is then handed to [`crate::chunker::chunk_markdown`]
//! so we reuse the existing header-driven splitter.

use std::collections::HashMap;
use std::fs::File;
use std::io::Read;
use std::path::Path;

use anyhow::{Context, Result};
use base64::{engine::general_purpose, Engine as _};
use quick_xml::events::Event;
use quick_xml::Reader;

/// Visual color marker found on a Word run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocxColorMark {
    pub family: String,
    pub label: String,
    pub value: String,
}

/// Paragraph text flattened from `word/document.xml`, with its dominant visual
/// color when Word marks most of the paragraph in a non-default color.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocxColoredParagraph {
    pub text: String,
    pub color: Option<DocxColorMark>,
}

/// Extract the body of a `.docx` file as markdown.
pub fn docx_to_markdown(path: &Path) -> Result<String> {
    let file = File::open(path).with_context(|| format!("open docx: {}", path.display()))?;
    let mut archive = zip::ZipArchive::new(file)
        .with_context(|| format!("not a valid .docx (zip) file: {}", path.display()))?;

    let mut doc_xml = String::new();
    {
        let mut entry = archive
            .by_name("word/document.xml")
            .with_context(|| format!("missing word/document.xml in {}", path.display()))?;
        entry.read_to_string(&mut doc_xml)?;
    }

    parse_document_xml(&doc_xml)
}

/// Extract the body of a `.docx` file as markdown, preserving embedded
/// screenshots as Markdown data URI images.
///
/// This is intentionally separate from [`docx_to_markdown`]: RAG ingestion
/// should stay text-first, while the chat work-document flow needs the
/// original functional screenshots to survive into the final deliverable.
pub fn docx_to_markdown_with_images(path: &Path) -> Result<String> {
    let file = File::open(path).with_context(|| format!("open docx: {}", path.display()))?;
    let mut archive = zip::ZipArchive::new(file)
        .with_context(|| format!("not a valid .docx (zip) file: {}", path.display()))?;

    let mut doc_xml = String::new();
    {
        let mut entry = archive
            .by_name("word/document.xml")
            .with_context(|| format!("missing word/document.xml in {}", path.display()))?;
        entry.read_to_string(&mut doc_xml)?;
    }

    let images = read_document_images(&mut archive)?;
    parse_document_xml_with_images(&doc_xml, &images)
}

/// Extract flattened paragraphs with their dominant non-default text color or
/// highlight. This is used by the work-document flow to distinguish old/new
/// question groups in questionnaires that encode them visually.
pub fn docx_colored_paragraphs(path: &Path) -> Result<Vec<DocxColoredParagraph>> {
    let file = File::open(path).with_context(|| format!("open docx: {}", path.display()))?;
    let mut archive = zip::ZipArchive::new(file)
        .with_context(|| format!("not a valid .docx (zip) file: {}", path.display()))?;

    let mut doc_xml = String::new();
    {
        let mut entry = archive
            .by_name("word/document.xml")
            .with_context(|| format!("missing word/document.xml in {}", path.display()))?;
        entry.read_to_string(&mut doc_xml)?;
    }

    parse_colored_paragraphs_xml(&doc_xml)
}

/// Parse an OOXML `word/document.xml` body into a markdown string.
///
/// Separated from [`docx_to_markdown`] so it can be unit-tested without
/// constructing a real zip file.
fn parse_document_xml(xml: &str) -> Result<String> {
    let images = HashMap::new();
    parse_document_xml_with_images(xml, &images)
}

fn parse_document_xml_with_images(
    xml: &str,
    images: &HashMap<String, MarkdownImage>,
) -> Result<String> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(false);

    let mut out = String::new();
    let mut buf: Vec<u8> = Vec::new();

    let mut para_text = String::new();
    let mut para_images: Vec<String> = Vec::new();
    let mut heading_level: Option<u8> = None;
    let mut in_t = false;
    let mut in_body = false;
    let mut image_count = 0usize;

    loop {
        match reader.read_event_into(&mut buf) {
            Err(e) => {
                return Err(anyhow::anyhow!(
                    "xml parse error at byte {}: {}",
                    reader.buffer_position(),
                    e
                ))
            }
            Ok(Event::Eof) => break,

            Ok(Event::Start(e)) => {
                let name = e.name();
                match name.as_ref() {
                    b"w:body" => in_body = true,
                    b"w:t" if in_body => in_t = true,
                    b"w:pStyle" if in_body => {
                        if let Some(level) = heading_level_from_attrs(&e) {
                            heading_level = Some(level);
                        }
                    }
                    b"a:blip" if in_body => {
                        append_markdown_image_from_blip(
                            &e,
                            images,
                            &mut image_count,
                            &mut para_images,
                        );
                    }
                    _ => {}
                }
            }

            Ok(Event::Empty(e)) => {
                // Self-closing tags: w:pStyle, w:br, etc.
                let name = e.name();
                match name.as_ref() {
                    b"w:pStyle" if in_body => {
                        if let Some(level) = heading_level_from_attrs(&e) {
                            heading_level = Some(level);
                        }
                    }
                    b"w:br" if in_body => {
                        // Soft line break within a paragraph
                        para_text.push('\n');
                    }
                    b"w:tab" if in_body => {
                        para_text.push('\t');
                    }
                    b"a:blip" if in_body => {
                        append_markdown_image_from_blip(
                            &e,
                            images,
                            &mut image_count,
                            &mut para_images,
                        );
                    }
                    _ => {}
                }
            }

            Ok(Event::End(e)) => {
                let name = e.name();
                match name.as_ref() {
                    b"w:body" => in_body = false,
                    b"w:t" => in_t = false,
                    b"w:p" if in_body => {
                        flush_paragraph_with_images(
                            &mut out,
                            &mut para_text,
                            &mut heading_level,
                            &mut para_images,
                        );
                    }
                    // Also flush on cell boundary so a cell with no </w:p> doesn't leak.
                    // (In well-formed docx every cell has at least one w:p — but be defensive.)
                    _ => {}
                }
            }

            Ok(Event::Text(e)) => {
                if in_t && in_body {
                    if let Ok(s) = std::str::from_utf8(e.as_ref()) {
                        // Best-effort XML entity unescape (&amp; → &, &lt; → <, …).
                        // If quick_xml rejects the input, fall back to the raw text.
                        match quick_xml::escape::unescape(s) {
                            Ok(cow) => para_text.push_str(&cow),
                            Err(_) => para_text.push_str(s),
                        }
                    }
                }
            }

            Ok(Event::CData(e)) => {
                if in_t && in_body {
                    if let Ok(s) = std::str::from_utf8(e.as_ref()) {
                        para_text.push_str(s);
                    }
                }
            }

            Ok(_) => {}
        }
        buf.clear();
    }

    // Final flush in case the document ends without a closing body event
    // (shouldn't happen for valid OOXML, but be defensive).
    flush_paragraph_with_images(
        &mut out,
        &mut para_text,
        &mut heading_level,
        &mut para_images,
    );

    Ok(out)
}

fn parse_colored_paragraphs_xml(xml: &str) -> Result<Vec<DocxColoredParagraph>> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(false);

    let mut paragraphs = Vec::new();
    let mut buf: Vec<u8> = Vec::new();

    let mut in_body = false;
    let mut in_run = false;
    let mut in_t = false;
    let mut run_text = String::new();
    let mut run_color: Option<DocxColorMark> = None;
    let mut para_runs: Vec<(String, Option<DocxColorMark>)> = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Err(e) => {
                return Err(anyhow::anyhow!(
                    "xml parse error at byte {}: {}",
                    reader.buffer_position(),
                    e
                ))
            }
            Ok(Event::Eof) => break,
            Ok(Event::Start(e)) => match e.name().as_ref() {
                b"w:body" => in_body = true,
                b"w:r" if in_body => {
                    in_run = true;
                    run_text.clear();
                    run_color = None;
                }
                b"w:t" if in_body => in_t = true,
                b"w:color" if in_body && in_run => {
                    if let Some(color) = color_mark_from_attrs(&e) {
                        run_color = Some(color);
                    }
                }
                b"w:highlight" if in_body && in_run => {
                    if let Some(color) = highlight_mark_from_attrs(&e) {
                        run_color = Some(color);
                    }
                }
                _ => {}
            },
            Ok(Event::Empty(e)) => match e.name().as_ref() {
                b"w:color" if in_body && in_run => {
                    if let Some(color) = color_mark_from_attrs(&e) {
                        run_color = Some(color);
                    }
                }
                b"w:highlight" if in_body && in_run => {
                    if let Some(color) = highlight_mark_from_attrs(&e) {
                        run_color = Some(color);
                    }
                }
                b"w:tab" if in_body && in_run => run_text.push('\t'),
                b"w:br" if in_body && in_run => run_text.push('\n'),
                _ => {}
            },
            Ok(Event::End(e)) => match e.name().as_ref() {
                b"w:body" => in_body = false,
                b"w:t" => in_t = false,
                b"w:r" if in_body && in_run => {
                    if !run_text.is_empty() {
                        para_runs.push((std::mem::take(&mut run_text), run_color.clone()));
                    }
                    run_color = None;
                    in_run = false;
                }
                b"w:p" if in_body => {
                    flush_colored_paragraph(&mut paragraphs, &mut para_runs);
                }
                _ => {}
            },
            Ok(Event::Text(e)) => {
                if in_t && in_body {
                    if let Ok(s) = std::str::from_utf8(e.as_ref()) {
                        match quick_xml::escape::unescape(s) {
                            Ok(cow) => run_text.push_str(&cow),
                            Err(_) => run_text.push_str(s),
                        }
                    }
                }
            }
            Ok(Event::CData(e)) => {
                if in_t && in_body {
                    if let Ok(s) = std::str::from_utf8(e.as_ref()) {
                        run_text.push_str(s);
                    }
                }
            }
            Ok(_) => {}
        }
        buf.clear();
    }

    flush_colored_paragraph(&mut paragraphs, &mut para_runs);
    Ok(paragraphs)
}

fn flush_colored_paragraph(
    paragraphs: &mut Vec<DocxColoredParagraph>,
    para_runs: &mut Vec<(String, Option<DocxColorMark>)>,
) {
    let text = para_runs
        .iter()
        .map(|(run_text, _)| run_text.as_str())
        .collect::<String>()
        .trim()
        .to_string();
    if text.is_empty() {
        para_runs.clear();
        return;
    }

    paragraphs.push(DocxColoredParagraph {
        text,
        color: dominant_run_color(para_runs),
    });
    para_runs.clear();
}

fn dominant_run_color(runs: &[(String, Option<DocxColorMark>)]) -> Option<DocxColorMark> {
    let mut totals: Vec<(DocxColorMark, usize)> = Vec::new();
    for (run_text, color) in runs {
        let Some(color) = color else { continue };
        let weight = run_text.chars().filter(|c| !c.is_whitespace()).count();
        if weight == 0 {
            continue;
        }
        if let Some((_, total_weight)) = totals
            .iter_mut()
            .find(|(existing_color, _)| existing_color == color)
        {
            *total_weight += weight;
        } else {
            totals.push((color.clone(), weight));
        }
    }
    totals
        .into_iter()
        .max_by_key(|(_, total_weight)| *total_weight)
        .map(|(color, _)| color)
}

fn color_mark_from_attrs(e: &quick_xml::events::BytesStart) -> Option<DocxColorMark> {
    let value = attr_value(e, b"w:val")?;
    let normalized = value.trim().trim_start_matches('#').to_ascii_uppercase();
    if matches!(normalized.as_str(), "" | "AUTO" | "000000" | "FFFFFF") {
        return None;
    }
    let (family, label) = color_family_and_label(&normalized);
    Some(DocxColorMark {
        family,
        label,
        value: normalized,
    })
}

fn highlight_mark_from_attrs(e: &quick_xml::events::BytesStart) -> Option<DocxColorMark> {
    let value = attr_value(e, b"w:val")?;
    let normalized = value.trim().to_ascii_lowercase();
    if matches!(normalized.as_str(), "" | "none" | "clear") {
        return None;
    }
    let (family, label) = color_family_and_label(&normalized);
    Some(DocxColorMark {
        family,
        label,
        value: normalized,
    })
}

fn color_family_and_label(value: &str) -> (String, String) {
    let lower = value.to_ascii_lowercase();
    let family = if matches!(
        lower.as_str(),
        "blue" | "cyan" | "0070c0" | "0000ff" | "0563c1" | "1f4e79" | "00b0f0"
    ) {
        "blue"
    } else if matches!(
        lower.as_str(),
        "green" | "brightgreen" | "00b050" | "008000" | "70ad47" | "92d050" | "00ff00"
    ) {
        "green"
    } else if matches!(lower.as_str(), "red" | "ff0000" | "c00000") {
        "red"
    } else if matches!(lower.as_str(), "yellow" | "ffff00") {
        "yellow"
    } else if matches!(lower.as_str(), "magenta" | "purple" | "7030a0" | "800080") {
        "purple"
    } else if matches!(lower.as_str(), "orange" | "ed7d31" | "ffc000") {
        "orange"
    } else if matches!(lower.as_str(), "gray" | "grey" | "808080" | "a6a6a6") {
        "gray"
    } else {
        "custom"
    };
    let label = match family {
        "blue" => "Bleu".to_string(),
        "green" => "Vert".to_string(),
        "red" => "Rouge".to_string(),
        "yellow" => "Jaune".to_string(),
        "purple" => "Violet".to_string(),
        "orange" => "Orange".to_string(),
        "gray" => "Gris".to_string(),
        _ if value.len() == 6 && value.chars().all(|c| c.is_ascii_hexdigit()) => {
            format!("#{value}")
        }
        _ => value.to_string(),
    };
    (family.to_string(), label)
}

fn attr_value(e: &quick_xml::events::BytesStart, name: &[u8]) -> Option<String> {
    let bare_name = name.split(|byte| *byte == b':').next_back().unwrap_or(name);
    for attr_res in e.attributes() {
        let Ok(attr) = attr_res else { continue };
        let key = attr.key.as_ref();
        let key_bare = key.split(|byte| *byte == b':').next_back().unwrap_or(key);
        if key == name || key_bare == bare_name {
            return Some(String::from_utf8_lossy(attr.value.as_ref()).to_string());
        }
    }
    None
}

fn flush_paragraph_with_images(
    out: &mut String,
    para_text: &mut String,
    heading_level: &mut Option<u8>,
    para_images: &mut Vec<String>,
) {
    let trimmed = para_text.trim();
    if !trimmed.is_empty() {
        if let Some(level) = *heading_level {
            for _ in 0..level {
                out.push('#');
            }
            out.push(' ');
        }
        out.push_str(trimmed);
        out.push_str("\n\n");
    }
    for image in para_images.drain(..) {
        out.push_str(&image);
        out.push_str("\n\n");
    }
    para_text.clear();
    *heading_level = None;
}

#[derive(Clone, Debug)]
struct MarkdownImage {
    data_uri: String,
}

fn read_document_images(
    archive: &mut zip::ZipArchive<File>,
) -> Result<HashMap<String, MarkdownImage>> {
    let mut rels_xml = String::new();
    match archive.by_name("word/_rels/document.xml.rels") {
        Ok(mut entry) => entry.read_to_string(&mut rels_xml)?,
        Err(_) => return Ok(HashMap::new()),
    };

    let relationships = parse_image_relationships(&rels_xml)?;
    let mut images = HashMap::new();
    for (rid, target) in relationships {
        let Some(zip_path) = relationship_target_to_zip_path(&target) else {
            continue;
        };
        let Some(mime) = image_mime_for_path(&zip_path) else {
            continue;
        };
        let mut bytes = Vec::new();
        let Ok(mut entry) = archive.by_name(&zip_path) else {
            continue;
        };
        entry.read_to_end(&mut bytes)?;
        let encoded = general_purpose::STANDARD.encode(bytes);
        images.insert(
            rid,
            MarkdownImage {
                data_uri: format!("data:{mime};base64,{encoded}"),
            },
        );
    }
    Ok(images)
}

fn parse_image_relationships(xml: &str) -> Result<Vec<(String, String)>> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);
    let mut buf = Vec::new();
    let mut relationships = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Err(e) => {
                return Err(anyhow::anyhow!(
                    "relationship xml parse error at byte {}: {}",
                    reader.buffer_position(),
                    e
                ))
            }
            Ok(Event::Eof) => break,
            Ok(Event::Empty(e)) | Ok(Event::Start(e)) => {
                if e.name().as_ref() != b"Relationship" {
                    buf.clear();
                    continue;
                }
                let mut id = None;
                let mut target = None;
                let mut rel_type = None;
                for attr_res in e.attributes() {
                    let Ok(attr) = attr_res else { continue };
                    let value = String::from_utf8_lossy(attr.value.as_ref()).to_string();
                    match attr.key.as_ref() {
                        b"Id" => id = Some(value),
                        b"Target" => target = Some(value),
                        b"Type" => rel_type = Some(value),
                        _ => {}
                    }
                }
                let is_image = rel_type
                    .as_deref()
                    .is_some_and(|value| value.ends_with("/image"));
                if is_image {
                    if let (Some(id), Some(target)) = (id, target) {
                        relationships.push((id, target));
                    }
                }
            }
            Ok(_) => {}
        }
        buf.clear();
    }

    Ok(relationships)
}

fn relationship_target_to_zip_path(target: &str) -> Option<String> {
    let target = target.trim().trim_start_matches('/');
    if target.is_empty() || target.contains("..") || target.contains('\\') {
        return None;
    }
    let path = if target.starts_with("word/") {
        target.to_string()
    } else {
        format!("word/{target}")
    };
    path.starts_with("word/media/").then_some(path)
}

fn image_mime_for_path(path: &str) -> Option<&'static str> {
    let lower = path.to_ascii_lowercase();
    if lower.ends_with(".png") {
        Some("image/png")
    } else if lower.ends_with(".jpg") || lower.ends_with(".jpeg") {
        Some("image/jpeg")
    } else if lower.ends_with(".gif") {
        Some("image/gif")
    } else {
        None
    }
}

fn append_markdown_image_from_blip(
    e: &quick_xml::events::BytesStart,
    images: &HashMap<String, MarkdownImage>,
    image_count: &mut usize,
    para_images: &mut Vec<String>,
) {
    let mut rid = None;
    for attr_res in e.attributes() {
        let Ok(attr) = attr_res else { continue };
        if attr.key.as_ref() == b"r:embed" || attr.key.as_ref() == b"r:link" {
            rid = Some(String::from_utf8_lossy(attr.value.as_ref()).to_string());
            break;
        }
    }
    let Some(rid) = rid else { return };
    let Some(image) = images.get(&rid) else {
        return;
    };
    *image_count += 1;
    let alt = format!("Capture fonctionnelle {}", *image_count);
    para_images.push(format!("![{alt}]({})", image.data_uri));
}

/// Extract the heading level from a `<w:pStyle w:val="...">` element, if its
/// style name matches a known heading convention.
fn heading_level_from_attrs(e: &quick_xml::events::BytesStart) -> Option<u8> {
    for attr_res in e.attributes() {
        let Ok(attr) = attr_res else { continue };
        if attr.key.as_ref() == b"w:val" {
            let val = std::str::from_utf8(attr.value.as_ref()).ok()?;
            return detect_heading_level(val);
        }
    }
    None
}

fn detect_heading_level(style: &str) -> Option<u8> {
    let s = style.trim();
    let lower = s.to_ascii_lowercase();
    // English (Heading1, Heading 1, heading1), French (Titre1, Titre 1).
    let is_heading =
        lower.starts_with("heading") || lower.starts_with("titre") || lower.starts_with("title");
    if !is_heading {
        return None;
    }
    // Extract the first run of digits from the style name.
    let digits: String = s.chars().filter(|c| c.is_ascii_digit()).collect();
    if digits.is_empty() {
        // "Title" with no number → H1
        return Some(1);
    }
    digits.parse::<u8>().ok().map(|n| n.clamp(1, 6))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wrap_body(body: &str) -> String {
        format!(
            r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
<w:body>
{body}
</w:body>
</w:document>"#
        )
    }

    fn wrap_body_with_drawing_namespaces(body: &str) -> String {
        format!(
            r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"
            xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"
            xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships">
<w:body>
{body}
</w:body>
</w:document>"#
        )
    }

    #[test]
    fn extracts_dominant_colored_paragraphs_from_word_runs() {
        let xml = wrap_body(
            r#"
<w:p>
  <w:r><w:rPr><w:color w:val="00B050"/></w:rPr><w:t>Ancienne question verte ?</w:t></w:r>
</w:p>
<w:p>
  <w:r><w:rPr><w:color w:val="0070C0"/></w:rPr><w:t>Nouvelle question bleue ?</w:t></w:r>
</w:p>
<w:p>
  <w:r><w:rPr><w:highlight w:val="yellow"/></w:rPr><w:t>Point surligne jaune</w:t></w:r>
</w:p>
<w:p>
  <w:r><w:t>Texte normal</w:t></w:r>
</w:p>
"#,
        );

        let paragraphs = parse_colored_paragraphs_xml(&xml).unwrap();

        assert_eq!(paragraphs.len(), 4);
        assert_eq!(paragraphs[0].color.as_ref().unwrap().label, "Vert");
        assert_eq!(paragraphs[1].color.as_ref().unwrap().label, "Bleu");
        assert_eq!(paragraphs[2].color.as_ref().unwrap().label, "Jaune");
        assert!(paragraphs[3].color.is_none());
    }

    #[test]
    fn parses_simple_paragraph() {
        let body = r#"
<w:p><w:r><w:t>Hello world</w:t></w:r></w:p>
"#;
        let md = parse_document_xml(&wrap_body(body)).unwrap();
        assert_eq!(md.trim(), "Hello world");
    }

    #[test]
    fn parses_heading_and_body() {
        let body = r#"
<w:p><w:pPr><w:pStyle w:val="Heading1"/></w:pPr><w:r><w:t>Intro</w:t></w:r></w:p>
<w:p><w:r><w:t>Body text here.</w:t></w:r></w:p>
<w:p><w:pPr><w:pStyle w:val="Heading2"/></w:pPr><w:r><w:t>Sub</w:t></w:r></w:p>
<w:p><w:r><w:t>More content.</w:t></w:r></w:p>
"#;
        let md = parse_document_xml(&wrap_body(body)).unwrap();
        assert!(md.contains("# Intro"), "missing H1: {md}");
        assert!(md.contains("Body text here."));
        assert!(md.contains("## Sub"), "missing H2: {md}");
        assert!(md.contains("More content."));
    }

    #[test]
    fn parses_french_titre_style() {
        // French locale of Word uses "Titre1", "Titre 1", etc.
        let body = r#"
<w:p><w:pPr><w:pStyle w:val="Titre 2"/></w:pPr><w:r><w:t>Calcul des baremes</w:t></w:r></w:p>
"#;
        let md = parse_document_xml(&wrap_body(body)).unwrap();
        assert!(
            md.contains("## Calcul des baremes"),
            "expected French H2, got: {md}"
        );
    }

    #[test]
    fn walks_into_table_cells() {
        // First w:body element is a table (common pattern in Sample docs).
        // Each cell's paragraph must be emitted.
        let body = r#"
<w:tbl>
  <w:tr>
    <w:tc><w:p><w:r><w:t>Cell A</w:t></w:r></w:p></w:tc>
    <w:tc><w:p><w:r><w:t>Cell B</w:t></w:r></w:p></w:tc>
  </w:tr>
</w:tbl>
"#;
        let md = parse_document_xml(&wrap_body(body)).unwrap();
        assert!(md.contains("Cell A"), "missing cell A: {md}");
        assert!(md.contains("Cell B"), "missing cell B: {md}");
    }

    #[test]
    fn empty_paragraphs_dont_crash() {
        let body = r#"
<w:p></w:p>
<w:p><w:r></w:r></w:p>
<w:p><w:r><w:t>Not empty.</w:t></w:r></w:p>
"#;
        let md = parse_document_xml(&wrap_body(body)).unwrap();
        assert_eq!(md.trim(), "Not empty.");
    }

    #[test]
    fn concatenates_multiple_runs_in_paragraph() {
        // Word often splits a single logical sentence across many <w:r> runs
        // (e.g., when formatting changes mid-sentence).
        let body = r#"
<w:p>
  <w:r><w:t xml:space="preserve">The </w:t></w:r>
  <w:r><w:t xml:space="preserve">quick </w:t></w:r>
  <w:r><w:t>fox</w:t></w:r>
</w:p>
"#;
        let md = parse_document_xml(&wrap_body(body)).unwrap();
        assert!(md.contains("The quick fox"), "runs not concatenated: {md}");
    }

    #[test]
    fn self_closing_break_becomes_newline() {
        let body = r#"
<w:p><w:r><w:t>Line 1</w:t><w:br/><w:t>Line 2</w:t></w:r></w:p>
"#;
        let md = parse_document_xml(&wrap_body(body)).unwrap();
        assert!(md.contains("Line 1"));
        assert!(md.contains("Line 2"));
    }

    #[test]
    fn preserves_embedded_images_as_markdown_data_uri() {
        let body = r#"
<w:p><w:r><w:drawing><a:blip r:embed="rId5"/></w:drawing></w:r></w:p>
"#;
        let mut images = HashMap::new();
        images.insert(
            "rId5".to_string(),
            MarkdownImage {
                data_uri: "data:image/png;base64,abc123".to_string(),
            },
        );

        let md = parse_document_xml_with_images(&wrap_body_with_drawing_namespaces(body), &images)
            .unwrap();

        assert_eq!(
            md.trim(),
            "![Capture fonctionnelle 1](data:image/png;base64,abc123)"
        );
    }

    #[test]
    fn heading_level_detection() {
        assert_eq!(detect_heading_level("Heading1"), Some(1));
        assert_eq!(detect_heading_level("Heading 2"), Some(2));
        assert_eq!(detect_heading_level("heading3"), Some(3));
        assert_eq!(detect_heading_level("Titre 4"), Some(4));
        assert_eq!(detect_heading_level("Title"), Some(1));
        assert_eq!(detect_heading_level("BodyText"), None);
        assert_eq!(detect_heading_level("Normal"), None);
        // Clamped to 1..=6
        assert_eq!(detect_heading_level("Heading9"), Some(6));
    }
}

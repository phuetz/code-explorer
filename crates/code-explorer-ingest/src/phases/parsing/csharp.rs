use std::collections::HashMap;

use streaming_iterator::StreamingIterator;
use tree_sitter::{Parser, Query, QueryCursor};

use code_explorer_core::config::languages::SupportedLanguage;
use code_explorer_core::graph::types::{
    GraphNode, GraphRelationship, NodeLabel, NodeProperties, RelationshipType,
};
use code_explorer_core::graph::KnowledgeGraph;
use code_explorer_core::id::generate_id;

use crate::grammar;
use crate::phases::structure::FileEntry;

use super::{CallForm, ExtractedCall, ExtractedData, ExtractedHeritage, ExtractedImport};

/// Razor/C# specific post-processing: extract directives, script blocks,
/// and detect UI component library usage.
pub(super) fn process_razor_extras(
    file: &FileEntry,
    file_node_id: &str,
    nodes: &mut Vec<GraphNode>,
    relationships: &mut Vec<GraphRelationship>,
    extracted: &mut ExtractedData,
) {
    use code_explorer_lang::component_detection::{
        extract_html_helpers, extract_razor_directives, extract_script_blocks, ComponentDetector,
    };

    let directives = extract_razor_directives(&file.content);
    for directive in &directives {
        match directive.directive.as_str() {
            "page" => {
                let route_id = generate_id("Route", &format!("{}:{}", file.path, directive.value));
                let edge_id = format!("handles_route_{}_{}", file_node_id, route_id);
                nodes.push(GraphNode {
                    id: route_id.clone(),
                    label: NodeLabel::Route,
                    properties: NodeProperties {
                        name: directive.value.clone(),
                        file_path: file.path.clone(),
                        start_line: Some(directive.line as u32 + 1),
                        description: Some("Razor page route".to_string()),
                        ..Default::default()
                    },
                });
                relationships.push(GraphRelationship {
                    id: edge_id,
                    source_id: file_node_id.to_string(),
                    target_id: route_id,
                    rel_type: RelationshipType::HandlesRoute,
                    confidence: 1.0,
                    reason: "razor_page_directive".to_string(),
                    step: None,
                });
            }
            "model" => {
                extracted.imports.push(ExtractedImport {
                    file_path: file.path.clone(),
                    raw_import_path: directive.value.clone(),
                    binding_text: None,
                    language: "razor".to_string(),
                });
            }
            "inject" => {
                let parts: Vec<&str> = directive.value.split_whitespace().collect();
                if !parts.is_empty() {
                    extracted.imports.push(ExtractedImport {
                        file_path: file.path.clone(),
                        raw_import_path: format!("@inject {}", directive.value),
                        binding_text: None,
                        language: "razor".to_string(),
                    });
                }
            }
            "using" => {
                extracted.imports.push(ExtractedImport {
                    file_path: file.path.clone(),
                    raw_import_path: directive.value.clone(),
                    binding_text: None,
                    language: "razor".to_string(),
                });
            }
            "inherits" => {
                extracted.heritage.push(ExtractedHeritage {
                    file_path: file.path.clone(),
                    class_name: razor_filename(&file.path),
                    parent_name: directive.value.clone(),
                    kind: "extends".to_string(),
                });
            }
            "implements" => {
                extracted.heritage.push(ExtractedHeritage {
                    file_path: file.path.clone(),
                    class_name: razor_filename(&file.path),
                    parent_name: directive.value.clone(),
                    kind: "implements".to_string(),
                });
            }
            "layout" => {
                extracted.heritage.push(ExtractedHeritage {
                    file_path: file.path.clone(),
                    class_name: razor_filename(&file.path),
                    parent_name: directive.value.clone(),
                    kind: "extends".to_string(),
                });
                extracted.imports.push(ExtractedImport {
                    file_path: file.path.clone(),
                    raw_import_path: directive.value.clone(),
                    binding_text: None,
                    language: "razor".to_string(),
                });
            }
            "namespace" => {
                extracted.imports.push(ExtractedImport {
                    file_path: file.path.clone(),
                    raw_import_path: directive.value.clone(),
                    binding_text: None,
                    language: "razor".to_string(),
                });
            }
            _ => {}
        }
    }

    let script_blocks = extract_script_blocks(&file.content);
    if !script_blocks.is_empty() {
        parse_embedded_javascript(
            file,
            file_node_id,
            nodes,
            relationships,
            extracted,
            script_blocks,
        );
    }

    let helpers = extract_html_helpers(&file.content);
    for helper in &helpers {
        match helper.helper_type.as_str() {
            "Partial" | "RenderPartial" | "PartialAsync" | "RenderPartialAsync" => {
                extracted.calls.push(ExtractedCall {
                    file_path: file.path.clone(),
                    called_name: helper.target.clone(),
                    source_id: file_node_id.to_string(),
                    arg_count: None,
                    call_form: CallForm::Member,
                    receiver_name: Some("Html".to_string()),
                    receiver_type_name: Some("IHtmlHelper".to_string()),
                });
            }
            "ActionLink" | "Action" | "RenderAction" | "RouteUrl" => {
                let target_name = if let Some(ref controller) = helper.controller {
                    format!("{}.{}", controller, helper.target)
                } else {
                    helper.target.clone()
                };
                extracted.calls.push(ExtractedCall {
                    file_path: file.path.clone(),
                    called_name: target_name,
                    source_id: file_node_id.to_string(),
                    arg_count: None,
                    call_form: CallForm::Member,
                    receiver_name: helper.controller.clone(),
                    receiver_type_name: helper
                        .controller
                        .as_ref()
                        .map(|c| format!("{}Controller", c)),
                });
            }
            _ => {}
        }
    }

    let detector = ComponentDetector::shared();
    let detected = detector.detect_in_file(&file.content, &file.path);
    for component in &detected {
        let lib_id = generate_id("Library", &component.library_name);
        if !nodes.iter().any(|n| n.id == lib_id) {
            nodes.push(GraphNode {
                id: lib_id.clone(),
                label: NodeLabel::Library,
                properties: NodeProperties {
                    name: component.library_name.clone(),
                    file_path: String::new(),
                    description: Some(format!(
                        "{} — {} (detected via {:?})",
                        component.vendor, component.category, component.detected_by
                    )),
                    ..Default::default()
                },
            });
        }

        let edge_id = format!("uses_lib_{}_{}", file_node_id, lib_id);
        relationships.push(GraphRelationship {
            id: edge_id,
            source_id: file_node_id.to_string(),
            target_id: lib_id,
            rel_type: RelationshipType::Uses,
            confidence: component.confidence,
            reason: format!("{:?}", component.detected_by),
            step: None,
        });
    }
}

fn parse_embedded_javascript(
    file: &FileEntry,
    file_node_id: &str,
    nodes: &mut Vec<GraphNode>,
    relationships: &mut Vec<GraphRelationship>,
    extracted: &mut ExtractedData,
    script_blocks: Vec<(usize, String)>,
) {
    let js_lang = grammar::get_language(SupportedLanguage::JavaScript);
    let js_provider = code_explorer_lang::registry::get_provider(SupportedLanguage::JavaScript);
    let js_query_str = js_provider.tree_sitter_queries();

    let mut js_parser = Parser::new();
    if js_parser.set_language(&js_lang).is_err() {
        return;
    }

    let Ok(js_query) = Query::new(&js_lang, js_query_str) else {
        return;
    };

    for (block_idx, (_line_num, script_content)) in script_blocks.iter().enumerate() {
        let virtual_path = format!("{}#script-{}", file.path, block_idx);
        let virtual_file_id = generate_id("File", &virtual_path);

        if let Some(tree) = js_parser.parse(script_content, None) {
            let content_bytes = script_content.as_bytes();
            let capture_names = js_query.capture_names();
            let mut cursor = QueryCursor::new();
            let mut matches = cursor.matches(&js_query, tree.root_node(), content_bytes);

            while let Some(m) = matches.next() {
                let mut captures: HashMap<&str, (&str, tree_sitter::Node)> = HashMap::new();
                let mut multi_captures: HashMap<&str, Vec<&str>> = HashMap::new();
                for capture in m.captures {
                    let Some(name) = capture_names.get(capture.index as usize) else {
                        continue;
                    };
                    if let Ok(text) = capture.node.utf8_text(content_bytes) {
                        captures.insert(name, (text, capture.node));
                        multi_captures.entry(name).or_default().push(text);
                    }
                }

                let virtual_file = FileEntry {
                    path: virtual_path.clone(),
                    content: script_content.clone(),
                    language: Some(SupportedLanguage::JavaScript),
                    size: script_content.len(),
                };

                super::process_match(
                    &captures,
                    &multi_captures,
                    &virtual_file,
                    SupportedLanguage::JavaScript,
                    &virtual_file_id,
                    nodes,
                    relationships,
                    extracted,
                );
            }
        }

        let edge_id = format!("contains_script_{}_{}", file_node_id, virtual_file_id);
        relationships.push(GraphRelationship {
            id: edge_id,
            source_id: file_node_id.to_string(),
            target_id: virtual_file_id,
            rel_type: RelationshipType::Contains,
            confidence: 1.0,
            reason: "embedded_script_block".to_string(),
            step: None,
        });
    }
}

fn razor_filename(file_path: &str) -> String {
    std::path::Path::new(file_path)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("Unknown")
        .to_string()
}

/// Scan for .csproj files and detect component libraries from NuGet PackageReferences.
///
/// This provides higher-confidence library detection than source-level patterns because
/// .csproj files contain the definitive list of NuGet dependencies with exact versions.
pub fn detect_csproj_components(graph: &mut KnowledgeGraph, repo_path: &std::path::Path) {
    use code_explorer_lang::component_detection::ComponentDetector;
    use ignore::WalkBuilder;

    let detector = ComponentDetector::shared();

    let walker = WalkBuilder::new(repo_path)
        .hidden(true)
        .git_ignore(true)
        .git_global(false)
        .git_exclude(true)
        .max_depth(Some(8))
        .build();

    for result in walker.flatten() {
        if !result.file_type().is_some_and(|ft| ft.is_file()) {
            continue;
        }
        let path = result.path();
        let path_str = path.to_string_lossy();

        let is_project_file = path_str.ends_with(".csproj")
            || path_str.ends_with("packages.config")
            || path_str.ends_with("web.config");

        if !is_project_file {
            continue;
        }

        let content = match std::fs::read_to_string(path) {
            Ok(c) => c,
            Err(_) => continue,
        };

        let rel_path = path
            .strip_prefix(repo_path)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/");

        let detected = detector.detect_in_csproj(&content);
        if detected.is_empty() {
            continue;
        }

        let project_id = generate_id("File", &rel_path);

        for component in &detected {
            let lib_id = generate_id("Library", &component.library_name);

            if graph.get_node(&lib_id).is_none() {
                let mut desc = format!("{} — {}", component.vendor, component.category);
                if let Some(ref ver) = component.detected_version {
                    desc.push_str(&format!(" (v{})", ver));
                }
                graph.add_node(GraphNode {
                    id: lib_id.clone(),
                    label: NodeLabel::Library,
                    properties: NodeProperties {
                        name: component.library_name.clone(),
                        file_path: rel_path.clone(),
                        description: Some(desc),
                        ..Default::default()
                    },
                });
            }

            let edge_id = format!("uses_lib_{}_{}", project_id, lib_id);
            graph.add_relationship(GraphRelationship {
                id: edge_id,
                source_id: project_id.clone(),
                target_id: lib_id,
                rel_type: RelationshipType::Uses,
                confidence: component.confidence,
                reason: format!("csproj_{:?}", component.detected_by),
                step: None,
            });
        }
    }
}

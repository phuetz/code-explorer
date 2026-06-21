use std::collections::{HashMap, HashSet};

use rayon::prelude::*;
use streaming_iterator::StreamingIterator;
use tracing::warn;
use tree_sitter::{Parser, Query, QueryCursor};

use code_explorer_core::config::languages::SupportedLanguage;
use code_explorer_core::graph::types::*;
use code_explorer_core::graph::KnowledgeGraph;
use code_explorer_core::id::generate_id;
use code_explorer_core::symbol::{SymbolDefinition, SymbolTable};

use crate::grammar;
use crate::phases::structure::FileEntry;
use crate::pipeline::ProgressSender;

mod cpp;
mod csharp;
mod go;
mod kotlin;
mod python;
mod ruby;
mod rust;
mod typescript;

pub use csharp::detect_csproj_components;
pub(crate) use cpp::reconcile_out_of_class_methods;
pub(crate) use go::reconcile_cross_file_methods;

/// Data extracted from parsing phase (before resolution).
#[derive(Debug, Default)]
pub struct ExtractedData {
    pub imports: Vec<ExtractedImport>,
    pub calls: Vec<ExtractedCall>,
    pub assignments: Vec<ExtractedAssignment>,
    pub heritage: Vec<ExtractedHeritage>,
}

impl ExtractedData {
    fn merge(&mut self, other: ExtractedData) {
        self.imports.extend(other.imports);
        self.calls.extend(other.calls);
        self.assignments.extend(other.assignments);
        self.heritage.extend(other.heritage);
    }
}

#[derive(Debug, Clone)]
pub struct ExtractedImport {
    pub file_path: String,
    pub raw_import_path: String,
    pub binding_text: Option<String>,
    pub language: String,
}

#[derive(Debug, Clone)]
pub struct ExtractedCall {
    pub file_path: String,
    pub called_name: String,
    pub source_id: String,
    pub arg_count: Option<u32>,
    pub call_form: CallForm,
    pub receiver_name: Option<String>,
    pub receiver_type_name: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CallForm {
    Free,
    Member,
    Constructor,
}

#[derive(Debug, Clone)]
pub struct ExtractedAssignment {
    pub file_path: String,
    pub source_id: String,
    pub receiver_text: String,
    pub property_name: String,
    pub receiver_type_name: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ExtractedHeritage {
    pub file_path: String,
    pub class_name: String,
    pub parent_name: String,
    pub kind: String,
}

/// Result of parsing a single file (graph nodes + extracted data).
struct FileParsed {
    nodes: Vec<GraphNode>,
    relationships: Vec<GraphRelationship>,
    extracted: ExtractedData,
}

/// Parse all files and extract symbols, imports, calls, heritage.
pub fn parse_files(
    graph: &mut KnowledgeGraph,
    files: &[FileEntry],
    _progress_tx: Option<&ProgressSender>,
) -> Result<ExtractedData, crate::IngestError> {
    // Parse all files in parallel using rayon
    let results: Vec<FileParsed> = files
        .par_iter()
        .filter_map(|file| {
            let lang = file.language?;
            if !grammar::is_language_available(lang) {
                return None;
            }
            Some(parse_single_file(file, lang))
        })
        .collect();

    // Merge results into the graph (single-threaded for graph mutation)
    let mut extracted = ExtractedData::default();
    for result in results {
        for node in result.nodes {
            graph.add_node(node);
        }
        for rel in result.relationships {
            graph.add_relationship(rel);
        }
        extracted.merge(result.extracted);
    }

    Ok(extracted)
}

/// Parse a single file with tree-sitter and extract all symbols.
fn parse_single_file(file: &FileEntry, lang: SupportedLanguage) -> FileParsed {
    let ts_language = grammar::get_language_for_file(lang, &file.path);
    let provider = code_explorer_lang::registry::get_provider(lang);
    let query_str = provider.tree_sitter_queries();

    // Create parser and parse the content
    let mut parser = Parser::new();
    if parser.set_language(&ts_language).is_err() {
        warn!("Failed to set language for {}", file.path);
        return FileParsed {
            nodes: Vec::new(),
            relationships: Vec::new(),
            extracted: ExtractedData::default(),
        };
    }

    let tree = match parser.parse(&file.content, None) {
        Some(t) => t,
        None => {
            warn!("Failed to parse {}", file.path);
            return FileParsed {
                nodes: Vec::new(),
                relationships: Vec::new(),
                extracted: ExtractedData::default(),
            };
        }
    };

    // Compile query
    let query = match Query::new(&ts_language, query_str) {
        Ok(q) => q,
        Err(e) => {
            warn!(
                "Query compilation failed for {} ({}): {}",
                file.path,
                lang.as_str(),
                e
            );
            return FileParsed {
                nodes: Vec::new(),
                relationships: Vec::new(),
                extracted: ExtractedData::default(),
            };
        }
    };

    let content_bytes = file.content.as_bytes();
    let capture_names = query.capture_names();
    let file_node_id = generate_id("File", &file.path);

    let mut nodes: Vec<GraphNode> = Vec::new();
    let mut relationships: Vec<GraphRelationship> = Vec::new();
    let mut extracted = ExtractedData::default();

    // Build a capture index for fast lookup: capture_name -> index
    // Execute query
    let mut cursor = QueryCursor::new();
    let mut matches = cursor.matches(&query, tree.root_node(), content_bytes);

    while let Some(m) = matches.next() {
        // Collect captures for this match into a map: capture_name -> text.
        // We also collect multi-value captures separately for capture names
        // where one match legitimately produces several captures (e.g.
        // `class Foo implements IBar, IBaz` emits multiple
        // `@heritage.implements` captures in a single match — `HashMap::insert`
        // would silently keep only the last one).
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

        // Determine the pattern category from the outermost capture name
        // The pattern type is determined by which captures are present
        process_match(
            &captures,
            &multi_captures,
            file,
            lang,
            &file_node_id,
            &mut nodes,
            &mut relationships,
            &mut extracted,
        );
    }

    if typescript::is_script_language(lang) {
        typescript::post_parse_script(
            tree.root_node(),
            file,
            lang,
            &file_node_id,
            &mut nodes,
            &mut relationships,
            &mut extracted,
        );
    }

    // ── Razor-specific post-processing ─────────────────────────────────
    // Extract Razor directives, embedded JavaScript, and detect UI
    // component libraries from .cshtml/.razor files.
    if lang == SupportedLanguage::Razor {
        csharp::process_razor_extras(
            file,
            &file_node_id,
            &mut nodes,
            &mut relationships,
            &mut extracted,
        );
    }

    // ── Rust-specific post-processing ──────────────────────────────────
    // Nest `impl`/`trait` methods under their owning type (HasMethod). The
    // generic nesting path can't: Rust methods are `Function`-labeled and an
    // `impl_item` has no `name` field. See `rust::post_parse`.
    if lang == SupportedLanguage::Rust {
        rust::post_parse(tree.root_node(), file, &nodes, &mut relationships);
    }

    // ── Python-specific post-processing ────────────────────────────────
    // Nest class methods under their `Class` node (HasMethod). Like Rust,
    // Python methods are `Function`-labeled so the generic nesting guard
    // skips them. See `python::post_parse`.
    if lang == SupportedLanguage::Python {
        python::post_parse(tree.root_node(), file, &mut nodes, &mut relationships);
    }

    // ── Go-specific post-processing ────────────────────────────────────
    // Nest methods (by receiver type) and struct fields under their type.
    // Go methods are top-level `method_declaration`s and struct fields sit
    // under `type_spec`, neither reachable by the generic nesting path.
    // See `go::post_parse`.
    if lang == SupportedLanguage::Go {
        go::post_parse(tree.root_node(), file, &nodes, &mut relationships);
    }

    // ── Ruby-specific post-processing ──────────────────────────────────
    // Nest methods declared directly in a `module` block under their Module
    // node. `module` can't join the shared CONTAINER_KINDS (Python's root is
    // also `module`). See `ruby::post_parse`.
    if lang == SupportedLanguage::Ruby {
        ruby::post_parse(tree.root_node(), file, &nodes, &mut relationships);
    }

    // ── Kotlin post-processing ─────────────────────────────────────────
    // Methods are `Function`-labeled but lexically inside a class container,
    // so the generic guard skips them. See kotlin::post_parse.
    if lang == SupportedLanguage::Kotlin {
        kotlin::post_parse(tree.root_node(), file, &nodes, &mut relationships);
    }

    FileParsed {
        nodes,
        relationships,
        extracted,
    }
}

/// Process a single query match and extract nodes/edges/data.
#[allow(clippy::too_many_arguments)]
pub(super) fn process_match(
    captures: &HashMap<&str, (&str, tree_sitter::Node)>,
    multi_captures: &HashMap<&str, Vec<&str>>,
    file: &FileEntry,
    lang: SupportedLanguage,
    file_node_id: &str,
    nodes: &mut Vec<GraphNode>,
    relationships: &mut Vec<GraphRelationship>,
    extracted: &mut ExtractedData,
) {
    // --- Original TS capture pattern: @name + @definition.X ---
    // The original Code Explorer queries use @name for the symbol name and
    // @definition.class, @definition.function, etc. as the match pattern.
    if let Some((name, name_node)) = captures.get("name") {
        // Determine label from which @definition.X captures are present
        let label = if captures.contains_key("definition.class") {
            Some(NodeLabel::Class)
        } else if captures.contains_key("definition.function") {
            Some(NodeLabel::Function)
        } else if captures.contains_key("definition.method") {
            Some(NodeLabel::Method)
        } else if captures.contains_key("definition.interface") {
            Some(NodeLabel::Interface)
        } else if captures.contains_key("definition.struct") {
            Some(NodeLabel::Struct)
        } else if captures.contains_key("definition.enum") {
            Some(NodeLabel::Enum)
        } else if captures.contains_key("definition.property") {
            Some(NodeLabel::Property)
        } else if captures.contains_key("definition.constructor") {
            Some(NodeLabel::Constructor)
        } else if captures.contains_key("definition.trait") {
            Some(NodeLabel::Trait)
        } else if captures.contains_key("definition.impl") {
            Some(NodeLabel::Impl)
        } else if captures.contains_key("definition.module") {
            Some(NodeLabel::Module)
        } else if captures.contains_key("definition.namespace") {
            Some(NodeLabel::Namespace)
        } else if captures.contains_key("definition.type") {
            Some(NodeLabel::TypeAlias)
        } else if captures.contains_key("definition.const") {
            Some(NodeLabel::Const)
        } else if captures.contains_key("definition.static") {
            Some(NodeLabel::Static)
        } else if captures.contains_key("definition.macro") {
            Some(NodeLabel::Macro)
        } else if captures.contains_key("definition.typedef") {
            Some(NodeLabel::Typedef)
        } else if captures.contains_key("definition.union") {
            Some(NodeLabel::Union)
        } else if captures.contains_key("definition.record") {
            Some(NodeLabel::Record)
        } else if captures.contains_key("definition.delegate") {
            Some(NodeLabel::Delegate)
        } else if captures.contains_key("definition.annotation") {
            Some(NodeLabel::Annotation)
        } else if captures.contains_key("definition.template") {
            Some(NodeLabel::Template)
        } else {
            None
        };

        if let Some(label) = label {
            create_definition_node(
                label,
                name,
                name_node,
                None,
                file,
                lang,
                file_node_id,
                nodes,
                relationships,
            );
            return;
        }
        // Fall through if @name present but no @definition.X (could be import/call/heritage)
    }

    // --- Original TS: @import with @import.source ---
    if captures.contains_key("import") || captures.contains_key("import.source") {
        extract_import(captures, file, lang, extracted);
        return;
    }

    // --- Original TS: @call with @call.name ---
    if captures.contains_key("call") && captures.contains_key("call.name") {
        extract_call(captures, file, lang, file_node_id, extracted);
        return;
    }

    // --- Original TS: @heritage with @heritage.extends / @heritage.implements / @heritage.trait ---
    if captures.contains_key("heritage") || captures.contains_key("heritage.impl") {
        extract_heritage(captures, multi_captures, file, extracted);
        return;
    }

    // --- Original TS: @assignment with @assignment.receiver / @assignment.property ---
    if captures.contains_key("assignment") && captures.contains_key("assignment.property") {
        extract_assignment(captures, file, file_node_id, extracted);
        return;
    }

    // --- Fallback: agent-style capture names (class.name, function.name, etc.) ---
    // Functions
    if let Some((name, node)) = captures.get("function.name") {
        create_definition_node(
            NodeLabel::Function,
            name,
            node,
            captures.get("function.params").map(|(t, _)| *t),
            file,
            lang,
            file_node_id,
            nodes,
            relationships,
        );
    }
    // Variable functions (arrow / function expressions)
    else if let Some((name, node)) = captures.get("variable_function.name") {
        create_definition_node(
            NodeLabel::Function,
            name,
            node,
            captures.get("variable_function.params").map(|(t, _)| *t),
            file,
            lang,
            file_node_id,
            nodes,
            relationships,
        );
    }
    // Classes
    else if let Some((name, node)) = captures.get("class.name") {
        create_definition_node(
            NodeLabel::Class,
            name,
            node,
            None,
            file,
            lang,
            file_node_id,
            nodes,
            relationships,
        );
    }
    // Methods
    else if let Some((name, node)) = captures.get("method.name") {
        create_definition_node(
            NodeLabel::Method,
            name,
            node,
            captures.get("method.params").map(|(t, _)| *t),
            file,
            lang,
            file_node_id,
            nodes,
            relationships,
        );
    }
    // Interfaces
    else if let Some((name, node)) = captures.get("interface.name") {
        create_definition_node(
            NodeLabel::Interface,
            name,
            node,
            None,
            file,
            lang,
            file_node_id,
            nodes,
            relationships,
        );
    }
    // Structs
    else if let Some((name, node)) = captures.get("struct.name") {
        create_definition_node(
            NodeLabel::Struct,
            name,
            node,
            None,
            file,
            lang,
            file_node_id,
            nodes,
            relationships,
        );
    }
    // Enums
    else if let Some((name, node)) = captures.get("enum.name") {
        create_definition_node(
            NodeLabel::Enum,
            name,
            node,
            None,
            file,
            lang,
            file_node_id,
            nodes,
            relationships,
        );
    }
    // Traits
    else if let Some((name, node)) = captures.get("trait.name") {
        create_definition_node(
            NodeLabel::Trait,
            name,
            node,
            None,
            file,
            lang,
            file_node_id,
            nodes,
            relationships,
        );
    }
    // Constructors
    else if let Some((name, node)) = captures.get("constructor.name") {
        create_definition_node(
            NodeLabel::Constructor,
            name,
            node,
            captures.get("constructor.params").map(|(t, _)| *t),
            file,
            lang,
            file_node_id,
            nodes,
            relationships,
        );
    }
    // Type aliases
    else if let Some((name, node)) = captures.get("type_alias.name") {
        create_definition_node(
            NodeLabel::TypeAlias,
            name,
            node,
            None,
            file,
            lang,
            file_node_id,
            nodes,
            relationships,
        );
    }
    // Constants
    else if let Some((name, node)) = captures.get("const.name") {
        create_definition_node(
            NodeLabel::Const,
            name,
            node,
            None,
            file,
            lang,
            file_node_id,
            nodes,
            relationships,
        );
    }
    // Statics
    else if let Some((name, node)) = captures.get("static.name") {
        create_definition_node(
            NodeLabel::Static,
            name,
            node,
            None,
            file,
            lang,
            file_node_id,
            nodes,
            relationships,
        );
    }
    // Macros
    else if let Some((name, node)) = captures.get("macro.name") {
        create_definition_node(
            NodeLabel::Macro,
            name,
            node,
            None,
            file,
            lang,
            file_node_id,
            nodes,
            relationships,
        );
    }
    // Modules
    else if let Some((name, node)) = captures.get("module.name") {
        create_definition_node(
            NodeLabel::Module,
            name,
            node,
            None,
            file,
            lang,
            file_node_id,
            nodes,
            relationships,
        );
    }
    // Namespaces
    else if let Some((name, node)) = captures.get("namespace.name") {
        create_definition_node(
            NodeLabel::Namespace,
            name,
            node,
            None,
            file,
            lang,
            file_node_id,
            nodes,
            relationships,
        );
    }
    // Typedefs
    else if let Some((name, node)) = captures.get("typedef.name") {
        create_definition_node(
            NodeLabel::Typedef,
            name,
            node,
            None,
            file,
            lang,
            file_node_id,
            nodes,
            relationships,
        );
    }
    // Unions
    else if let Some((name, node)) = captures.get("union.name") {
        create_definition_node(
            NodeLabel::Union,
            name,
            node,
            None,
            file,
            lang,
            file_node_id,
            nodes,
            relationships,
        );
    }
    // Records
    else if let Some((name, node)) = captures.get("record.name") {
        create_definition_node(
            NodeLabel::Record,
            name,
            node,
            None,
            file,
            lang,
            file_node_id,
            nodes,
            relationships,
        );
    }
    // Annotation types
    else if let Some((name, node)) = captures.get("annotation_type.name") {
        create_definition_node(
            NodeLabel::Annotation,
            name,
            node,
            None,
            file,
            lang,
            file_node_id,
            nodes,
            relationships,
        );
    }
    // Delegates
    else if let Some((name, node)) = captures.get("delegate.name") {
        create_definition_node(
            NodeLabel::Delegate,
            name,
            node,
            None,
            file,
            lang,
            file_node_id,
            nodes,
            relationships,
        );
    }
    // Protocols (Swift - treated as Interface)
    else if let Some((name, node)) = captures.get("protocol.name") {
        create_definition_node(
            NodeLabel::Interface,
            name,
            node,
            None,
            file,
            lang,
            file_node_id,
            nodes,
            relationships,
        );
    }
    // Function signatures (TypeScript overloads)
    else if let Some((name, node)) = captures.get("function_signature.name") {
        create_definition_node(
            NodeLabel::Function,
            name,
            node,
            captures.get("function_signature.params").map(|(t, _)| *t),
            file,
            lang,
            file_node_id,
            nodes,
            relationships,
        );
    }
    // --- Imports ---
    else if captures.contains_key("import")
        || captures.contains_key("import.source")
        || captures.contains_key("import.path")
        || captures.contains_key("import.name")
    {
        extract_import(captures, file, lang, extracted);
    }
    // --- Function calls ---
    else if captures.contains_key("call.function") || captures.contains_key("call.method") {
        extract_call(captures, file, lang, file_node_id, extracted);
    }
    // --- Constructor calls (new expressions) ---
    else if captures.contains_key("new.constructor") || captures.contains_key("new.type") {
        extract_new_call(captures, file, file_node_id, extracted);
    }
    // --- Heritage ---
    else if captures.contains_key("heritage.extends")
        || captures.contains_key("heritage.implements")
        || captures.contains_key("heritage.trait")
        || captures.contains_key("heritage.embeds")
        || captures.contains_key("heritage.conforms")
        || captures.contains_key("heritage.protocol")
        || captures.contains_key("heritage.uses_trait")
    {
        extract_heritage(captures, multi_captures, file, extracted);
    }
    // --- Assignments (member/field) ---
    else if captures.contains_key("assignment.property") {
        extract_assignment(captures, file, file_node_id, extracted);
    }
    // --- Properties (field definitions) - create Property nodes ---
    else if let Some((name, node)) = captures.get("property.name") {
        create_definition_node(
            NodeLabel::Property,
            name,
            node,
            None,
            file,
            lang,
            file_node_id,
            nodes,
            relationships,
        );
    }
}

pub(super) fn is_default_export_statement(node: tree_sitter::Node, content: &[u8]) -> bool {
    node.kind() == "export_statement"
        && node
            .utf8_text(content)
            .map(|text| text.trim_start().starts_with("export default"))
            .unwrap_or(false)
}

/// Create a definition node and a DEFINES edge from the file to it.
#[allow(clippy::too_many_arguments)]
fn create_definition_node(
    label: NodeLabel,
    name: &str,
    node: &tree_sitter::Node,
    params_text: Option<&str>,
    file: &FileEntry,
    lang: SupportedLanguage,
    file_node_id: &str,
    nodes: &mut Vec<GraphNode>,
    relationships: &mut Vec<GraphRelationship>,
) {
    let qualified_name = format!("{}:{}", file.path, name);
    let node_id = generate_id(label.as_str(), &qualified_name);

    // Count parameters if we have params text
    let parameter_count = params_text.map(count_parameters);

    // Check export status using the language provider
    let provider = code_explorer_lang::registry::get_provider(lang);
    // Approximate ancestors check: look at the node's parent chain
    let parent_type = node
        .parent()
        .map(|p| p.kind().to_string())
        .unwrap_or_default();
    let grandparent_type = node
        .parent()
        .and_then(|p| p.parent())
        .map(|gp| gp.kind().to_string())
        .unwrap_or_default();

    let ancestors = [parent_type.as_str(), grandparent_type.as_str()];
    let is_exported = provider.check_export(name, node.kind(), &ancestors);

    let start_line = node.start_position().row as u32 + 1;
    let end_line = node
        .parent()
        .map(|p| p.end_position().row as u32 + 1)
        .unwrap_or(start_line);

    // Compute cyclomatic complexity for callable nodes
    let complexity = if matches!(
        label,
        NodeLabel::Method | NodeLabel::Function | NodeLabel::Constructor
    ) {
        // Walk up to the definition node (parent of the name node) to get the full body
        let def_node = node.parent().unwrap_or(*node);
        Some(compute_complexity(def_node, file.content.as_bytes()))
    } else {
        None
    };

    let graph_node = GraphNode {
        id: node_id.clone(),
        label,
        properties: NodeProperties {
            name: name.to_string(),
            file_path: file.path.clone(),
            start_line: Some(start_line),
            end_line: Some(end_line),
            language: Some(lang),
            is_exported: Some(is_exported),
            parameter_count,
            complexity,
            ..Default::default()
        },
    };
    nodes.push(graph_node);

    // Create nesting edges: Class -> Method/Property/Constructor
    if matches!(
        label,
        NodeLabel::Method | NodeLabel::Property | NodeLabel::Constructor
    ) {
        if let Some(class_node_id) =
            find_enclosing_class_id(node, &file.path, file.content.as_bytes())
        {
            let rel_type = if label == NodeLabel::Property {
                RelationshipType::HasProperty
            } else {
                RelationshipType::HasMethod
            };
            let nesting_edge_id = format!("{}_{}", rel_type.as_str().to_lowercase(), node_id);
            relationships.push(GraphRelationship {
                id: nesting_edge_id,
                source_id: class_node_id,
                target_id: node_id.clone(),
                rel_type,
                confidence: 1.0,
                reason: "ast_nesting".to_string(),
                step: None,
            });
        }
    }

    // Create DEFINES edge: File -> Symbol
    let edge_id = format!("defines_{}_{}", file_node_id, node_id);
    relationships.push(GraphRelationship {
        id: edge_id,
        source_id: file_node_id.to_string(),
        target_id: node_id,
        rel_type: RelationshipType::Defines,
        confidence: 1.0,
        reason: "ast".to_string(),
        step: None,
    });
}

#[allow(clippy::too_many_arguments)]
pub(super) fn create_synthetic_definition_node(
    label: NodeLabel,
    name: &str,
    node: &tree_sitter::Node,
    params_text: Option<&str>,
    file: &FileEntry,
    lang: SupportedLanguage,
    file_node_id: &str,
    nodes: &mut Vec<GraphNode>,
    relationships: &mut Vec<GraphRelationship>,
) {
    let qualified_name = format!("{}:{}", file.path, name);
    let node_id = generate_id(label.as_str(), &qualified_name);
    if nodes.iter().any(|existing| existing.id == node_id) {
        return;
    }

    let parameter_count = params_text.map(count_parameters);
    let complexity = if matches!(
        label,
        NodeLabel::Method | NodeLabel::Function | NodeLabel::Constructor
    ) {
        Some(compute_complexity(*node, file.content.as_bytes()))
    } else {
        None
    };

    nodes.push(GraphNode {
        id: node_id.clone(),
        label,
        properties: NodeProperties {
            name: name.to_string(),
            file_path: file.path.clone(),
            start_line: Some(node.start_position().row as u32 + 1),
            end_line: Some(node.end_position().row as u32 + 1),
            language: Some(lang),
            is_exported: Some(true),
            parameter_count,
            complexity,
            ..Default::default()
        },
    });

    let edge_id = format!("defines_{}_{}", file_node_id, node_id);
    relationships.push(GraphRelationship {
        id: edge_id,
        source_id: file_node_id.to_string(),
        target_id: node_id,
        rel_type: RelationshipType::Defines,
        confidence: 1.0,
        reason: "ast_synthetic_default".to_string(),
        step: None,
    });
}

/// Compute cyclomatic complexity (CC) for a tree-sitter AST node.
///
/// CC = 1 + number of decision points found in the subtree.
/// Decision points: if, for, foreach, while, do, case/switch-arm, catch,
/// ternary/conditional expressions, and `&&`/`||` binary operators.
fn compute_complexity(node: tree_sitter::Node, content: &[u8]) -> u32 {
    let mut cc = 1u32;
    let mut cursor = node.walk();
    walk_tree_for_complexity(&mut cursor, content, &mut cc);
    cc
}

/// Recursively walk the AST via TreeCursor counting decision points.
fn walk_tree_for_complexity(cursor: &mut tree_sitter::TreeCursor, content: &[u8], cc: &mut u32) {
    let kind = cursor.node().kind();
    match kind {
        // Branching
        "if_statement" | "if_expression" => *cc += 1,

        // Loops
        "for_statement"
        | "for_expression"
        | "foreach_statement"
        | "for_in_statement"
        | "for_each_statement"
        | "enhanced_for_statement" => *cc += 1,

        "while_statement" | "while_expression" => *cc += 1,

        "do_statement" => *cc += 1,

        // Case clauses (NOT the switch/match itself)
        "case_clause" | "switch_expression_arm" | "match_arm" => *cc += 1,

        // Exception handling
        "catch_clause" | "catch_declaration" => *cc += 1,

        // Ternary / conditional expressions
        "conditional_expression" | "ternary_expression" => *cc += 1,

        // Logical operators in binary expressions
        "binary_expression" | "logical_expression" => {
            // Check if the operator is && or ||
            if let Some(op_node) = cursor.node().child_by_field_name("operator") {
                if let Ok(op_text) = op_node.utf8_text(content) {
                    if op_text == "&&" || op_text == "||" || op_text == "and" || op_text == "or" {
                        *cc += 1;
                    }
                }
            }
        }

        _ => {}
    }

    // Recurse into children
    if cursor.goto_first_child() {
        loop {
            walk_tree_for_complexity(cursor, content, cc);
            if !cursor.goto_next_sibling() {
                break;
            }
        }
        cursor.goto_parent();
    }
}

/// Walk up the tree-sitter AST from `node` to find the nearest enclosing class/struct/interface
/// container, and return its graph node ID so we can create HasMethod/HasProperty edges.
fn find_enclosing_class_id(
    node: &tree_sitter::Node,
    file_path: &str,
    content: &[u8],
) -> Option<String> {
    // Container node kinds across all supported languages
    const CONTAINER_KINDS: &[&str] = &[
        // C#
        "class_declaration",
        "struct_declaration",
        "interface_declaration",
        "record_declaration",
        // Java
        "annotation_type_declaration",
        // Python
        "class_definition",
        // Rust
        "struct_item",
        "impl_item",
        "enum_item",
        "trait_item",
        // C / C++
        "class_specifier",
        "struct_specifier",
        // JavaScript / TypeScript anonymous default class expressions.
        "class",
        // PHP
        "trait_declaration",
        // Note: "class_declaration" / "interface_declaration" / "enum_declaration"
        // are shared across C#, Java, TS/JS, Kotlin, PHP, Swift — no duplicates needed
    ];

    let mut cursor = node.parent();
    while let Some(ancestor) = cursor {
        let kind = ancestor.kind();
        if CONTAINER_KINDS.contains(&kind) {
            // Extract the class/struct/interface name via the "name" field child
            let synthetic_default = matches!(kind, "class_declaration" | "class")
                && ancestor.child_by_field_name("name").is_none()
                && ancestor
                    .parent()
                    .is_some_and(|parent| is_default_export_statement(parent, content));
            if let Some(name_node) = ancestor.child_by_field_name("name") {
                let class_name = name_node.utf8_text(content).ok()?;
                let label_str = match kind {
                    k if k.contains("interface") => "Interface",
                    k if k.contains("struct") => "Struct",
                    k if k.contains("record") => "Record",
                    k if k.contains("trait") => "Trait",
                    k if k.contains("impl") => "Impl",
                    k if k.contains("enum") => "Enum",
                    _ => "Class",
                };
                let qualified = format!("{}:{}", file_path, class_name);
                return Some(generate_id(label_str, &qualified));
            } else if synthetic_default {
                let qualified = format!("{}:default", file_path);
                return Some(generate_id("Class", &qualified));
            }
        }
        cursor = ancestor.parent();
    }
    None
}

/// Walk up the tree-sitter AST from `node` to find the nearest enclosing method/function/constructor
/// and return its graph node ID. This enables Method→Method Calls edges instead of File→Method.
pub(super) fn find_enclosing_method_id(
    node: &tree_sitter::Node,
    file_path: &str,
    content: &[u8],
) -> Option<String> {
    const METHOD_KINDS: &[&str] = &[
        // C#
        "method_declaration",
        "constructor_declaration",
        "local_function_statement",
        // Java
        // (method_declaration, constructor_declaration already listed)
        // Python
        "function_definition",
        // Rust
        "function_item",
        // Ruby (have a `name` field; map to the Method label below)
        "method",
        "singleton_method",
        // JavaScript / TypeScript
        "method_definition",
        "function_declaration",
        // Anonymous JS/TS function-like nodes (named via parent variable_declarator
        // or property assignment).
        "arrow_function",
        "function_expression",
        // Kotlin / Swift / generic lambdas
        "lambda_expression",
        // C / C++
        // (function_definition already listed)
        // Generic
        "function",
    ];

    let mut cursor = node.parent();
    while let Some(ancestor) = cursor {
        let kind = ancestor.kind();
        if METHOD_KINDS.contains(&kind) {
            // 1. Direct `name` field — works for declarations and method_definition.
            // 2. Fallback for arrow_function / function_expression: walk up to a
            //    `variable_declarator` (e.g. `const foo = () => {}`) or `pair` /
            //    `property_assignment` (object literal `{ foo: () => {} }`) and
            //    grab its name.
            let name_node = find_function_like_name_node(&ancestor);
            if let Some(name_node) = name_node {
                let method_name = name_node.utf8_text(content).ok()?;
                let label_str = if kind.contains("constructor") {
                    "Constructor"
                } else if matches!(kind, "arrow_function" | "function_expression")
                    && function_like_has_class_field_parent(&ancestor)
                {
                    "Method"
                } else if kind == "function_declaration"
                    || kind == "function_definition"
                    || kind == "function_item"
                    || kind == "arrow_function"
                    || kind == "function_expression"
                    || kind == "lambda_expression"
                    // C# `local_function_statement` is registered as
                    // `@definition.function` by the C# query, which generates
                    // a Function node ID. If we classify it as "Method" here,
                    // every CALLS edge from inside a C# local function points
                    // to a phantom `Method:...` node and the call disappears
                    // from impact analysis.
                    || kind == "local_function_statement"
                {
                    "Function"
                } else {
                    "Method"
                };
                let qualified = format!("{}:{}", file_path, method_name);
                return Some(generate_id(label_str, &qualified));
            } else if kind == "function_definition" {
                // C/C++ functions have no `name` field (the name is in a declarator).
                // Resolve it best-effort; any miss is re-pointed to File later by
                // `repoint_orphan_call_sources`, so it never leaves an orphan source.
                if let Some((name, label)) = cpp::enclosing_cpp_fn(&ancestor, content) {
                    return Some(generate_id(label, &format!("{}:{}", file_path, name)));
                }
            } else if matches!(kind, "function_declaration" | "function_expression")
                && typescript::is_anonymous_default_export_declaration(&ancestor, content)
            {
                let qualified = format!("{}:default", file_path);
                return Some(generate_id("Function", &qualified));
            }
        }
        cursor = ancestor.parent();
    }
    None
}

/// Nest methods that are lexically inside a class-like container but captured as
/// `@definition.function` (→ `Function` label), so the generic nesting guard skips them.
/// Used by Kotlin (methods are `function_declaration` inside `class_declaration`/
/// `object_declaration`). For each `function_declaration` whose NEAREST enclosing
/// function-or-container is one of `container_kinds`, emit a `HasMethod` edge from that
/// container's type node (resolved by name) to the method. Walking to the nearest
/// *function-or-container* excludes local functions declared inside a method. Both
/// endpoints are resolved from existing nodes, so no dangling/phantom edges.
pub(super) fn nest_function_methods(
    root: tree_sitter::Node,
    file: &FileEntry,
    nodes: &[GraphNode],
    relationships: &mut Vec<GraphRelationship>,
    container_kinds: &[&str],
    reason: &'static str,
) {
    let content = file.content.as_bytes();
    let mut type_ids: HashMap<&str, &str> = HashMap::new();
    let mut fn_ids: HashSet<&str> = HashSet::new();
    for n in nodes {
        match n.label {
            NodeLabel::Class | NodeLabel::Struct | NodeLabel::Enum | NodeLabel::Interface => {
                type_ids
                    .entry(n.properties.name.as_str())
                    .or_insert(n.id.as_str());
            }
            NodeLabel::Function => {
                fn_ids.insert(n.id.as_str());
            }
            _ => {}
        }
    }
    if type_ids.is_empty() {
        return;
    }

    let mut emitted: HashSet<String> = HashSet::new();
    let mut stack = vec![root];
    while let Some(node) = stack.pop() {
        if node.kind() == "function_declaration" {
            let mut a = node.parent();
            while let Some(p) = a {
                if p.kind() == "function_declaration" {
                    break; // local function inside a method → not a class method
                }
                if container_kinds.contains(&p.kind()) {
                    if let (Some(cname), Some(mname)) =
                        (ts_like_decl_name(&p, content), ts_like_decl_name(&node, content))
                    {
                        if let Some(&owner_id) = type_ids.get(cname) {
                            let method_id =
                                generate_id("Function", &format!("{}:{}", file.path, mname));
                            if fn_ids.contains(method_id.as_str()) {
                                let edge_id = format!(
                                    "{}_{}_{}",
                                    RelationshipType::HasMethod.as_str().to_lowercase(),
                                    owner_id,
                                    method_id
                                );
                                if emitted.insert(edge_id.clone()) {
                                    relationships.push(GraphRelationship {
                                        id: edge_id,
                                        source_id: owner_id.to_string(),
                                        target_id: method_id,
                                        rel_type: RelationshipType::HasMethod,
                                        confidence: 1.0,
                                        reason: reason.to_string(),
                                        step: None,
                                    });
                                }
                            }
                        }
                    }
                    break;
                }
                a = p.parent();
            }
        }

        let mut cur = node.walk();
        for child in node.children(&mut cur) {
            stack.push(child);
        }
    }
}

/// Name of a Kotlin declaration: the `name` field (kotlin-ng) if present, else the first
/// identifier-like child.
fn ts_like_decl_name<'a>(node: &tree_sitter::Node, content: &'a [u8]) -> Option<&'a str> {
    if let Some(n) = node.child_by_field_name("name") {
        return n.utf8_text(content).ok();
    }
    let mut cur = node.walk();
    let found = node
        .children(&mut cur)
        .find(|ch| matches!(ch.kind(), "simple_identifier" | "type_identifier" | "identifier"));
    found.and_then(|ch| ch.utf8_text(content).ok())
}

fn function_like_has_class_field_parent(node: &tree_sitter::Node) -> bool {
    let mut cursor = node.parent();
    while let Some(parent) = cursor {
        match parent.kind() {
            "field_definition" | "public_field_definition" => return true,
            "variable_declarator" | "pair" | "property_assignment" | "method_definition" => {
                return false;
            }
            "arguments" | "call_expression" | "parenthesized_expression" | "await_expression" => {
                cursor = parent.parent();
            }
            _ => return false,
        }
    }
    false
}

fn find_function_like_name_node<'tree>(
    node: &tree_sitter::Node<'tree>,
) -> Option<tree_sitter::Node<'tree>> {
    if let Some(name) = node.child_by_field_name("name") {
        return Some(name);
    }

    let mut cursor = node.parent();
    while let Some(parent) = cursor {
        match parent.kind() {
            "variable_declarator"
            | "pair"
            | "property_assignment"
            | "field_definition"
            | "public_field_definition" => return function_like_container_name_node(&parent),
            "arguments" | "call_expression" | "parenthesized_expression" | "await_expression" => {
                cursor = parent.parent();
            }
            _ => return None,
        }
    }

    None
}

fn function_like_container_name_node<'tree>(
    node: &tree_sitter::Node<'tree>,
) -> Option<tree_sitter::Node<'tree>> {
    node.child_by_field_name("name")
        .or_else(|| node.child_by_field_name("property"))
        .or_else(|| {
            node.child_by_field_name("key")
                .map(function_like_key_name_node)
        })
}

fn function_like_key_name_node<'tree>(key: tree_sitter::Node<'tree>) -> tree_sitter::Node<'tree> {
    if key.kind() == "string" {
        for idx in 0..key.child_count() {
            let Some(child) = key.child(idx) else {
                continue;
            };
            if child.kind() == "string_fragment" {
                return child;
            }
        }
    }
    key
}

/// Count parameters from a params string like "(a, b, c)" or "(a: int, b: str)".
///
/// Splits on top-level commas and discards empty segments so trailing commas
/// (e.g. `"(a, b, )"`) and whitespace-only argument lists do not inflate the
/// arity. A pure empty list `"()"` returns 0.
fn count_parameters(params: &str) -> u32 {
    let trimmed = params.trim();
    // Remove surrounding parens
    let inner = if trimmed.starts_with('(') && trimmed.ends_with(')') {
        &trimmed[1..trimmed.len() - 1]
    } else {
        trimmed
    };
    let inner = inner.trim();
    if inner.is_empty() {
        return 0;
    }
    // Walk the string, splitting at top-level commas (not inside nested
    // parens/brackets/braces). Angle brackets are tracked separately because
    // `<` / `>` are ambiguous between comparison operators and generics.
    let mut depth = 0i32;
    let mut angle_depth = 0i32;
    let mut count = 0u32;
    let mut current_has_content = false;
    for ch in inner.chars() {
        match ch {
            '(' | '[' | '{' => {
                depth += 1;
                current_has_content = true;
            }
            ')' | ']' | '}' => {
                depth -= 1;
                current_has_content = true;
            }
            '<' => {
                angle_depth += 1;
                current_has_content = true;
            }
            '>' if angle_depth > 0 => {
                angle_depth -= 1;
                current_has_content = true;
            }
            ',' if depth == 0 && angle_depth == 0 => {
                if current_has_content {
                    count += 1;
                }
                current_has_content = false;
            }
            c if c.is_whitespace() => {}
            _ => {
                current_has_content = true;
            }
        }
    }
    if current_has_content {
        count += 1;
    }
    count
}

fn ts_enclosing_parameter_contains(
    call_node: tree_sitter::Node,
    called_name: &str,
    file: &FileEntry,
) -> bool {
    let content = file.content.as_bytes();
    let mut cursor = call_node.parent();

    while let Some(ancestor) = cursor {
        if is_ts_function_like_node(ancestor.kind())
            && ancestor
                .child_by_field_name("parameters")
                .and_then(|parameters| parameters.utf8_text(content).ok())
                .map(extract_ts_parameter_names)
                .is_some_and(|names| names.contains(called_name))
        {
            return true;
        }

        cursor = ancestor.parent();
    }

    false
}

fn is_ts_function_like_node(kind: &str) -> bool {
    matches!(
        kind,
        "method_definition" | "function_declaration" | "arrow_function" | "function_expression"
    )
}

fn extract_ts_parameter_names(parameters: &str) -> HashSet<String> {
    let trimmed = parameters.trim();
    let inner = if trimmed.starts_with('(') && trimmed.ends_with(')') {
        &trimmed[1..trimmed.len() - 1]
    } else {
        trimmed
    };

    split_top_level_commas(inner)
        .into_iter()
        .filter_map(extract_ts_parameter_name)
        .collect()
}

fn split_top_level_commas(input: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut depth = 0i32;
    let mut angle_depth = 0i32;
    let mut start = 0usize;

    for (idx, ch) in input.char_indices() {
        match ch {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth -= 1,
            '<' => angle_depth += 1,
            '>' if angle_depth > 0 => angle_depth -= 1,
            ',' if depth == 0 && angle_depth == 0 => {
                parts.push(&input[start..idx]);
                start = idx + ch.len_utf8();
            }
            _ => {}
        }
    }

    if start < input.len() {
        parts.push(&input[start..]);
    }

    parts
}

fn extract_ts_parameter_name(parameter: &str) -> Option<String> {
    let mut text = parameter.trim().trim_start_matches("...").trim_start();
    for modifier in ["public", "private", "protected", "readonly", "override"] {
        if let Some(rest) = text.strip_prefix(modifier) {
            text = rest.trim_start();
        }
    }

    let mut chars = text.char_indices();
    let (_, first) = chars.next()?;
    if !is_ts_identifier_start(first) {
        return None;
    }

    let mut end = first.len_utf8();
    for (idx, ch) in chars {
        if !is_ts_identifier_continue(ch) {
            break;
        }
        end = idx + ch.len_utf8();
    }

    Some(text[..end].trim_end_matches('?').to_string())
}

fn is_ts_identifier_start(ch: char) -> bool {
    ch == '_' || ch == '$' || ch.is_ascii_alphabetic()
}

fn is_ts_identifier_continue(ch: char) -> bool {
    is_ts_identifier_start(ch) || ch.is_ascii_digit()
}

/// Extract import information from match captures.
fn extract_import(
    captures: &HashMap<&str, (&str, tree_sitter::Node)>,
    file: &FileEntry,
    lang: SupportedLanguage,
    extracted: &mut ExtractedData,
) {
    // Try different capture names for the import path/source
    let raw_path = captures
        .get("import.source")
        .or_else(|| captures.get("import.path"))
        .or_else(|| captures.get("import.name"))
        .map(|(text, _)| *text);

    if let Some(path) = raw_path {
        // Clean quotes from import path
        let cleaned = path.trim_matches(|c| c == '"' || c == '\'' || c == '`');
        let binding_text = captures.get("import").and_then(|(text, node)| {
            if node.kind() == "call_expression" && typescript::is_script_language(lang) {
                typescript::dynamic_import_binding_text(*node, file)
            } else {
                Some((*text).to_string())
            }
        });
        extracted.imports.push(ExtractedImport {
            file_path: file.path.clone(),
            raw_import_path: cleaned.to_string(),
            binding_text,
            language: lang.as_str().to_string(),
        });
    }
}

/// Extract function call information from match captures.
fn extract_call(
    captures: &HashMap<&str, (&str, tree_sitter::Node)>,
    file: &FileEntry,
    lang: SupportedLanguage,
    file_node_id: &str,
    extracted: &mut ExtractedData,
) {
    // Determine call form and name
    // Original TS queries use @call.name for both free and member calls
    // Agent-style queries use @call.method + @call.object or @call.function
    let (called_name, call_form, receiver_name) =
        if let Some((call_name, _)) = captures.get("call.name") {
            // Original capture pattern - determine form from context
            // If there's a receiver/object capture, it's a member call
            let receiver = captures
                .get("call.object")
                .or_else(|| captures.get("assignment.receiver"))
                .map(|(t, _)| t.to_string());
            let form = if receiver.is_some() {
                CallForm::Member
            } else {
                CallForm::Free
            };
            (call_name.to_string(), form, receiver)
        } else if let Some((method_name, _)) = captures.get("call.method") {
            let receiver = captures.get("call.object").map(|(t, _)| t.to_string());
            (method_name.to_string(), CallForm::Member, receiver)
        } else if let Some((func_name, _)) = captures.get("call.function") {
            (func_name.to_string(), CallForm::Free, None)
        } else {
            return;
        };

    // Language-specific call routing. The Ruby provider redirects calls like
    // `require 'foo'`, `include Bar`, and `attr_accessor :baz` to imports,
    // heritage, and property declarations respectively. Without this hook,
    // every `require` in a Ruby project produced an unresolved Calls edge
    // and zero Imports edges existed for any Ruby file. The route is opt-in
    // — `route_call` returns `None` for languages that don't override it.
    let provider = code_explorer_lang::registry::get_provider(lang);
    let call_text = captures
        .get("call")
        .and_then(|(_, node)| node.utf8_text(file.content.as_bytes()).ok())
        .unwrap_or("");
    if let Some(routed) = provider.route_call(&called_name, call_text) {
        use code_explorer_lang::call_routing::CallRoutingResult;
        match routed {
            CallRoutingResult::Import {
                import_path,
                is_relative,
            } => {
                // The Ruby resolver treats a path as relative only if it
                // starts with `./` or `../`. `require_relative 'models/user'`
                // (without the dot prefix) is also relative in Ruby — anchor
                // it to the calling file by injecting `./` when the call
                // form is `require_relative` but the path is bare.
                let normalized = if is_relative
                    && !import_path.starts_with("./")
                    && !import_path.starts_with("../")
                {
                    format!("./{import_path}")
                } else {
                    import_path
                };
                extracted.imports.push(ExtractedImport {
                    file_path: file.path.clone(),
                    raw_import_path: normalized,
                    binding_text: None,
                    language: lang.as_str().to_string(),
                });
                return;
            }
            CallRoutingResult::Skip => return,
            // Heritage / Properties / Call routing not yet wired here — the
            // Ruby `include` / `attr_accessor` patterns currently still flow
            // through as plain calls so name-based resolution can pick them
            // up. Treat them like normal calls for now.
            _ => {}
        }
    }

    if typescript::is_script_language(lang)
        && matches!(call_form, CallForm::Free)
        && captures
            .get("call")
            .is_some_and(|(_, node)| ts_enclosing_parameter_contains(*node, &called_name, file))
    {
        return;
    }

    // Count args
    let arg_count = captures
        .get("call.args")
        .map(|(text, _)| count_parameters(text));

    // Resolve enclosing method as call source (fallback to file node)
    let source_id = captures
        .get("call")
        .or_else(|| captures.get("call.name"))
        .and_then(|(_, node)| find_enclosing_method_id(node, &file.path, file.content.as_bytes()))
        .unwrap_or_else(|| file_node_id.to_string());

    extracted.calls.push(ExtractedCall {
        file_path: file.path.clone(),
        called_name,
        source_id,
        arg_count,
        call_form,
        receiver_name,
        receiver_type_name: None,
    });
}

/// Extract constructor call (new expression) information.
fn extract_new_call(
    captures: &HashMap<&str, (&str, tree_sitter::Node)>,
    file: &FileEntry,
    file_node_id: &str,
    extracted: &mut ExtractedData,
) {
    let constructor_name = captures
        .get("new.constructor")
        .or_else(|| captures.get("new.type"))
        .map(|(text, _)| text.to_string());

    if let Some(name) = constructor_name {
        let arg_count = captures
            .get("new.args")
            .map(|(text, _)| count_parameters(text));

        let source_id = captures
            .get("new.constructor")
            .or_else(|| captures.get("new.type"))
            .and_then(|(_, node)| {
                find_enclosing_method_id(node, &file.path, file.content.as_bytes())
            })
            .unwrap_or_else(|| file_node_id.to_string());

        extracted.calls.push(ExtractedCall {
            file_path: file.path.clone(),
            called_name: name,
            source_id,
            arg_count,
            call_form: CallForm::Constructor,
            receiver_name: None,
            receiver_type_name: None,
        });
    }
}

/// Extract heritage (extends/implements/trait) information.
fn extract_heritage(
    captures: &HashMap<&str, (&str, tree_sitter::Node)>,
    multi_captures: &HashMap<&str, Vec<&str>>,
    file: &FileEntry,
    extracted: &mut ExtractedData,
) {
    let class_name = captures
        .get("heritage.class")
        .or_else(|| captures.get("heritage.type"))
        .or_else(|| captures.get("heritage.struct"))
        .or_else(|| captures.get("heritage.record"))
        .or_else(|| captures.get("heritage.extension"))
        .map(|(text, _)| text.to_string());

    let push_all = |key: &str, kind: &str, extracted: &mut ExtractedData| {
        let Some(ref cls) = class_name else { return };
        let Some(items) = multi_captures.get(key) else {
            return;
        };
        for item in items {
            extracted.heritage.push(ExtractedHeritage {
                file_path: file.path.clone(),
                class_name: cls.clone(),
                parent_name: (*item).to_string(),
                kind: kind.to_string(),
            });
        }
    };

    // For every heritage capture name, iterate over ALL matched values, not
    // just the last one stored in `captures`. The HashMap-based `captures`
    // silently overwrites repeats, so a class implementing multiple interfaces
    // (`class Foo : IBar, IBaz`) used to record only the last interface.
    push_all("heritage.extends", "extends", extracted);
    push_all("heritage.implements", "implements", extracted);
    push_all("heritage.trait", "implements", extracted);
    push_all("heritage.embeds", "extends", extracted);
    push_all("heritage.conforms", "implements", extracted);
    push_all("heritage.protocol", "implements", extracted);
    push_all("heritage.uses_trait", "uses", extracted);
}

/// Extract assignment (member/field) information.
fn extract_assignment(
    captures: &HashMap<&str, (&str, tree_sitter::Node)>,
    file: &FileEntry,
    file_node_id: &str,
    extracted: &mut ExtractedData,
) {
    let receiver = captures
        .get("assignment.object")
        .map(|(text, _)| text.to_string())
        .unwrap_or_default();

    let property = captures
        .get("assignment.property")
        .map(|(text, _)| text.to_string())
        .unwrap_or_default();

    if !property.is_empty() {
        extracted.assignments.push(ExtractedAssignment {
            file_path: file.path.clone(),
            source_id: file_node_id.to_string(),
            receiver_text: receiver,
            property_name: property,
            receiver_type_name: None,
        });
    }
}

/// Build symbol table from the current graph state.
pub fn build_symbol_table(graph: &KnowledgeGraph, table: &mut SymbolTable) {
    graph.for_each_node(|node| match node.label {
        NodeLabel::Function
        | NodeLabel::Method
        | NodeLabel::Constructor
        | NodeLabel::Class
        | NodeLabel::Interface
        | NodeLabel::Struct
        | NodeLabel::Trait
        | NodeLabel::Enum
        | NodeLabel::Variable
        | NodeLabel::Property
        | NodeLabel::TypeAlias
        | NodeLabel::Const
        | NodeLabel::Static
        | NodeLabel::Macro => {
            let def = SymbolDefinition {
                node_id: node.id.clone(),
                file_path: node.properties.file_path.clone(),
                symbol_type: node.label,
                parameter_count: node.properties.parameter_count,
                required_parameter_count: None,
                parameter_types: None,
                return_type: node.properties.return_type.clone(),
                declared_type: None,
                owner_id: None,
                is_exported: node.properties.is_exported.unwrap_or(false),
            };
            table.add(node.properties.name.clone(), def);
        }
        _ => {}
    });

    // Populate owner_id from HasMethod / HasProperty edges so that
    // call resolution can match methods to their containing class.
    for rel in graph.iter_relationships() {
        if !matches!(
            rel.rel_type,
            RelationshipType::HasMethod | RelationshipType::HasProperty
        ) {
            continue;
        }
        let owner_id = rel.source_id.clone();
        let target_id = &rel.target_id;
        table.set_owner_id(target_id, owner_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_count_parameters_empty() {
        assert_eq!(count_parameters("()"), 0);
        assert_eq!(count_parameters("(  )"), 0);
    }

    #[test]
    fn test_count_parameters_simple() {
        assert_eq!(count_parameters("(a, b, c)"), 3);
        assert_eq!(count_parameters("(x)"), 1);
    }

    #[test]
    fn test_count_parameters_with_types() {
        assert_eq!(count_parameters("(a: number, b: string)"), 2);
    }

    #[test]
    fn test_count_parameters_nested() {
        // Nested generics should not count extra commas
        assert_eq!(count_parameters("(a: Map<K, V>, b: int)"), 2);
        assert_eq!(count_parameters("(f: Fn(a, b) -> c, d: int)"), 2);
    }

    #[test]
    fn test_parse_javascript_function() {
        let file = FileEntry {
            path: "test.js".to_string(),
            content: "function greet(name) { return 'hello ' + name; }".to_string(),
            size: 49,
            language: Some(SupportedLanguage::JavaScript),
        };

        let mut graph = KnowledgeGraph::new();
        // Add file node first
        graph.add_node(GraphNode {
            id: "File:test.js".to_string(),
            label: NodeLabel::File,
            properties: NodeProperties {
                name: "test.js".to_string(),
                file_path: "test.js".to_string(),
                ..Default::default()
            },
        });

        let _extracted = parse_files(&mut graph, &[file], None).unwrap();

        // Should have created a Function node for greet
        let func_node = graph.get_node("Function:test.js:greet");
        assert!(func_node.is_some(), "Should create Function node for greet");
        let func = func_node.unwrap();
        assert_eq!(func.properties.name, "greet");
        // Note: original queries don't capture parameter count directly
        // Parameter count extraction happens via AST analysis in full implementation
    }

    #[test]
    fn test_parse_javascript_class() {
        let file = FileEntry {
            path: "test.js".to_string(),
            content: "class UserService { constructor() {} getUser(id) { } }".to_string(),
            size: 54,
            language: Some(SupportedLanguage::JavaScript),
        };

        let mut graph = KnowledgeGraph::new();
        graph.add_node(GraphNode {
            id: "File:test.js".to_string(),
            label: NodeLabel::File,
            properties: NodeProperties {
                name: "test.js".to_string(),
                file_path: "test.js".to_string(),
                ..Default::default()
            },
        });

        let _extracted = parse_files(&mut graph, &[file], None).unwrap();

        assert!(
            graph.get_node("Class:test.js:UserService").is_some(),
            "Should create Class node"
        );
    }

    #[test]
    fn test_parse_rust_impl_methods_nest_under_type() {
        let content = r#"
struct Foo { x: i32 }
impl Foo {
    fn bar(&self) -> i32 { self.x }
    pub fn baz() {}
}
impl<T> Foo<T> {
    fn generic_method(&self) {}
}
enum E { A, B }
impl E {
    fn variant_count() -> usize { 2 }
}
trait T {
    fn provided(&self) -> bool { true }
    fn required(&self) -> bool;
}
"#;
        let file = FileEntry {
            path: "lib.rs".to_string(),
            content: content.to_string(),
            size: content.len(),
            language: Some(SupportedLanguage::Rust),
        };

        let mut graph = KnowledgeGraph::new();
        graph.add_node(GraphNode {
            id: "File:lib.rs".to_string(),
            label: NodeLabel::File,
            properties: NodeProperties {
                name: "lib.rs".to_string(),
                file_path: "lib.rs".to_string(),
                ..Default::default()
            },
        });

        let _ = parse_files(&mut graph, &[file], None).unwrap();

        let has_method = |src: &str, tgt: &str| {
            graph.iter_relationships().any(|r| {
                r.rel_type == RelationshipType::HasMethod
                    && r.source_id == src
                    && r.target_id == tgt
            })
        };

        // Inherent impl methods nest under the Struct (not the Impl node).
        assert!(
            has_method("Struct:lib.rs:Foo", "Function:lib.rs:bar"),
            "inherent method `bar` should nest under Struct Foo"
        );
        assert!(
            has_method("Struct:lib.rs:Foo", "Function:lib.rs:baz"),
            "associated fn `baz` should nest under Struct Foo"
        );
        // Generic impl `impl<T> Foo<T>` resolves to base type `Foo`.
        assert!(
            has_method("Struct:lib.rs:Foo", "Function:lib.rs:generic_method"),
            "generic-impl method should nest under base Struct Foo"
        );
        // Enum impl methods nest under the Enum.
        assert!(
            has_method("Enum:lib.rs:E", "Function:lib.rs:variant_count"),
            "enum method `variant_count` should nest under Enum E"
        );
        // Trait default-bodied method nests under the Trait; bodyless one is not a node.
        assert!(
            has_method("Trait:lib.rs:T", "Function:lib.rs:provided"),
            "trait default method `provided` should nest under Trait T"
        );
        assert!(
            graph.get_node("Function:lib.rs:required").is_none(),
            "bodyless trait method `required` is not extracted as a Function node"
        );

        // Regression guard: the call graph must still attribute calls to the
        // enclosing function (not to the File). `bar` calls nothing, but ensure
        // no HasMethod edge was sourced from the orphan Impl node.
        assert!(
            !graph
                .iter_relationships()
                .any(|r| r.rel_type == RelationshipType::HasMethod
                    && r.source_id.starts_with("Impl:")),
            "HasMethod edges should hang off the type node, not the Impl node"
        );
    }

    #[test]
    fn test_parse_python_methods_nest_under_class() {
        let content = r#"
class Foo:
    x: int = 0

    def bar(self):
        def inner():
            return 1
        return inner()

    @property
    def prop(self):
        return self.x

    @staticmethod
    def helper():
        return 2

    async def fetch(self):
        return 3

class Bar(Foo):
    def baz(self):
        return 4

def free_function():
    return 5
"#;
        let file = FileEntry {
            path: "mod.py".to_string(),
            content: content.to_string(),
            size: content.len(),
            language: Some(SupportedLanguage::Python),
        };

        let mut graph = KnowledgeGraph::new();
        graph.add_node(GraphNode {
            id: "File:mod.py".to_string(),
            label: NodeLabel::File,
            properties: NodeProperties {
                name: "mod.py".to_string(),
                file_path: "mod.py".to_string(),
                ..Default::default()
            },
        });

        let _ = parse_files(&mut graph, &[file], None).unwrap();

        let has_method = |src: &str, tgt: &str| {
            graph.iter_relationships().any(|r| {
                r.rel_type == RelationshipType::HasMethod
                    && r.source_id == src
                    && r.target_id == tgt
            })
        };

        // Plain, @property, @staticmethod, and async methods all nest under the class.
        assert!(has_method("Class:mod.py:Foo", "Function:mod.py:bar"), "plain method nests");
        assert!(has_method("Class:mod.py:Foo", "Function:mod.py:prop"), "@property method nests");
        assert!(has_method("Class:mod.py:Foo", "Function:mod.py:helper"), "@staticmethod nests");
        assert!(has_method("Class:mod.py:Foo", "Function:mod.py:fetch"), "async method nests");
        assert!(has_method("Class:mod.py:Bar", "Function:mod.py:baz"), "subclass method nests");

        // A `def` nested inside a method must NOT be attached to the class.
        assert!(
            !has_method("Class:mod.py:Foo", "Function:mod.py:inner"),
            "nested def must not nest under the class"
        );
        // A module-level function must not nest under any class.
        assert!(
            !graph.iter_relationships().any(|r| r.rel_type == RelationshipType::HasMethod
                && r.target_id == "Function:mod.py:free_function"),
            "module-level function must not nest"
        );
    }

    #[test]
    fn test_parse_python_same_name_methods_distinct_edges() {
        // Two classes in one file each defining __init__: each must get its own
        // HasMethod edge (edge id keyed on the owner class), even though they share
        // the same target Function node.
        let content = r#"
class A:
    def __init__(self):
        self.a = 1

class B:
    def __init__(self):
        self.b = 2
"#;
        let file = FileEntry {
            path: "m.py".to_string(),
            content: content.to_string(),
            size: content.len(),
            language: Some(SupportedLanguage::Python),
        };
        let mut graph = KnowledgeGraph::new();
        graph.add_node(GraphNode {
            id: "File:m.py".to_string(),
            label: NodeLabel::File,
            properties: NodeProperties {
                name: "m.py".to_string(),
                file_path: "m.py".to_string(),
                ..Default::default()
            },
        });
        let _ = parse_files(&mut graph, &[file], None).unwrap();

        let has = |src: &str| {
            graph.iter_relationships().any(|r| {
                r.rel_type == RelationshipType::HasMethod
                    && r.source_id == src
                    && r.target_id == "Function:m.py:__init__"
            })
        };
        assert!(has("Class:m.py:A"), "A.__init__ must nest");
        assert!(
            has("Class:m.py:B"),
            "B.__init__ must nest too — must not collide with A's edge"
        );
    }

    #[test]
    fn test_parse_javascript_imports() {
        let file = FileEntry {
            path: "test.js".to_string(),
            content: r#"import { foo } from './utils';"#.to_string(),
            size: 30,
            language: Some(SupportedLanguage::JavaScript),
        };

        let mut graph = KnowledgeGraph::new();
        graph.add_node(GraphNode {
            id: "File:test.js".to_string(),
            label: NodeLabel::File,
            properties: NodeProperties {
                name: "test.js".to_string(),
                file_path: "test.js".to_string(),
                ..Default::default()
            },
        });

        let extracted = parse_files(&mut graph, &[file], None).unwrap();
        assert!(!extracted.imports.is_empty(), "Should extract import");
        assert_eq!(extracted.imports[0].raw_import_path, "./utils");
        assert_eq!(
            extracted.imports[0].binding_text.as_deref(),
            Some(r#"import { foo } from './utils';"#)
        );
    }

    #[test]
    fn test_parse_typescript_dynamic_import_bindings() {
        let file = FileEntry {
            path: "test.ts".to_string(),
            content: r#"export async function load() {
  const { foo, bar: baz } = await import("./utils.js");
  return foo() + baz();
}"#
            .to_string(),
            size: 111,
            language: Some(SupportedLanguage::TypeScript),
        };

        let mut graph = KnowledgeGraph::new();
        graph.add_node(GraphNode {
            id: "File:test.ts".to_string(),
            label: NodeLabel::File,
            properties: NodeProperties {
                name: "test.ts".to_string(),
                file_path: "test.ts".to_string(),
                ..Default::default()
            },
        });

        let extracted = parse_files(&mut graph, &[file], None).unwrap();
        let dynamic_import = extracted
            .imports
            .iter()
            .find(|import| import.raw_import_path == "./utils.js")
            .expect("Should extract dynamic import source");
        assert_eq!(
            dynamic_import.binding_text.as_deref(),
            Some("{ foo, bar: baz }")
        );
    }

    #[test]
    fn test_parse_typescript_dynamic_import_then_bindings() {
        let file = FileEntry {
            path: "test.ts".to_string(),
            content: r#"export function load() {
  import("./utils.js").then(({ foo, bar: baz }) => {
    foo();
    baz();
  });
}"#
            .to_string(),
            size: 112,
            language: Some(SupportedLanguage::TypeScript),
        };

        let mut graph = KnowledgeGraph::new();
        graph.add_node(GraphNode {
            id: "File:test.ts".to_string(),
            label: NodeLabel::File,
            properties: NodeProperties {
                name: "test.ts".to_string(),
                file_path: "test.ts".to_string(),
                ..Default::default()
            },
        });

        let extracted = parse_files(&mut graph, &[file], None).unwrap();
        let dynamic_import = extracted
            .imports
            .iter()
            .find(|import| import.raw_import_path == "./utils.js")
            .expect("Should extract dynamic import source");
        assert_eq!(
            dynamic_import.binding_text.as_deref(),
            Some("{ foo, bar: baz }")
        );
    }

    #[test]
    fn test_parse_typescript_imported_call_result_bindings() {
        let file = FileEntry {
            path: "test.ts".to_string(),
            content: r#"import { useTools } from "./tools.js";

export function load() {
  const { run, close: stop } = useTools();
  run();
  stop();
}"#
            .to_string(),
            size: 132,
            language: Some(SupportedLanguage::TypeScript),
        };

        let mut graph = KnowledgeGraph::new();
        graph.add_node(GraphNode {
            id: "File:test.ts".to_string(),
            label: NodeLabel::File,
            properties: NodeProperties {
                name: "test.ts".to_string(),
                file_path: "test.ts".to_string(),
                ..Default::default()
            },
        });

        let extracted = parse_files(&mut graph, &[file], None).unwrap();
        let result_import = extracted
            .imports
            .iter()
            .find(|import| {
                import.raw_import_path == "./tools.js"
                    && import.binding_text.as_deref() == Some("{ run, close: stop }")
            })
            .expect("Should extract bindings from imported call result destructuring");
        assert_eq!(result_import.language, "typescript");
    }

    #[test]
    fn test_parse_typescript_lazy_dynamic_import_factory_bindings() {
        let file = FileEntry {
            path: "test.ts".to_string(),
            content: r#"export async function load() {
  const lazyImport = {
    renderers: () => lazyLoad("renderers", () => import("./renderers/index.js")),
    settingsManager: () => lazyLoad("settingsManager", () => import("./utils/settings-manager.js").then(m => m.getSettingsManager)),
  };
  const { initializeRenderers, configureRenderContext } = await lazyImport.renderers();
  const getSettingsManager = await lazyImport.settingsManager();
  initializeRenderers();
  configureRenderContext();
  getSettingsManager();
}"#
            .to_string(),
            size: 433,
            language: Some(SupportedLanguage::TypeScript),
        };

        let mut graph = KnowledgeGraph::new();
        graph.add_node(GraphNode {
            id: "File:test.ts".to_string(),
            label: NodeLabel::File,
            properties: NodeProperties {
                name: "test.ts".to_string(),
                file_path: "test.ts".to_string(),
                ..Default::default()
            },
        });

        let extracted = parse_files(&mut graph, &[file], None).unwrap();
        let factory_import = extracted
            .imports
            .iter()
            .find(|import| {
                import.raw_import_path == "./renderers/index.js"
                    && import.binding_text.as_deref()
                        == Some("{ initializeRenderers, configureRenderContext }")
            })
            .expect("Should extract named bindings from lazy dynamic import factory usage");

        assert_eq!(factory_import.language, "typescript");
        let function_factory_import = extracted
            .imports
            .iter()
            .find(|import| {
                import.raw_import_path == "./utils/settings-manager.js"
                    && import.binding_text.as_deref() == Some("{ getSettingsManager }")
            })
            .expect("Should extract named export returned by lazy dynamic import factory");
        assert_eq!(function_factory_import.language, "typescript");
        assert!(
            extracted.imports.iter().all(|import| {
                !matches!(
                    import.binding_text.as_deref(),
                    Some(text) if text.contains("import * as lazyImport")
                )
            }),
            "nested import inside lazyImport object should not bind lazyImport as a namespace"
        );
    }

    #[test]
    fn test_parse_typescript_require_destructuring_bindings() {
        let file = FileEntry {
            path: "test.ts".to_string(),
            content: r#"export function load() {
  const { foo, bar: baz } = require("./utils.js");
  return foo() + baz();
}"#
            .to_string(),
            size: 103,
            language: Some(SupportedLanguage::TypeScript),
        };

        let mut graph = KnowledgeGraph::new();
        graph.add_node(GraphNode {
            id: "File:test.ts".to_string(),
            label: NodeLabel::File,
            properties: NodeProperties {
                name: "test.ts".to_string(),
                file_path: "test.ts".to_string(),
                ..Default::default()
            },
        });

        let extracted = parse_files(&mut graph, &[file], None).unwrap();
        let require_import = extracted
            .imports
            .iter()
            .find(|import| import.raw_import_path == "./utils.js")
            .expect("Should extract require() source");
        assert_eq!(
            require_import.binding_text.as_deref(),
            Some("{ foo, bar: baz }")
        );
    }

    #[test]
    fn test_parse_typescript_require_namespace_binding() {
        let file = FileEntry {
            path: "test.ts".to_string(),
            content: r#"export function load() {
  const utils = require("./utils.js");
  return utils.foo();
}"#
            .to_string(),
            size: 82,
            language: Some(SupportedLanguage::TypeScript),
        };

        let mut graph = KnowledgeGraph::new();
        graph.add_node(GraphNode {
            id: "File:test.ts".to_string(),
            label: NodeLabel::File,
            properties: NodeProperties {
                name: "test.ts".to_string(),
                file_path: "test.ts".to_string(),
                ..Default::default()
            },
        });

        let extracted = parse_files(&mut graph, &[file], None).unwrap();
        let require_import = extracted
            .imports
            .iter()
            .find(|import| import.raw_import_path == "./utils.js")
            .expect("Should extract require() source");
        assert_eq!(
            require_import.binding_text.as_deref(),
            Some(r#"import * as utils from "./utils.js""#)
        );
    }

    #[test]
    fn test_parse_typescript_require_member_alias_binding() {
        let file = FileEntry {
            path: "test.ts".to_string(),
            content: r#"export function load() {
  const execute = require("./utils.js").run;
  return execute();
}"#
            .to_string(),
            size: 93,
            language: Some(SupportedLanguage::TypeScript),
        };

        let mut graph = KnowledgeGraph::new();
        graph.add_node(GraphNode {
            id: "File:test.ts".to_string(),
            label: NodeLabel::File,
            properties: NodeProperties {
                name: "test.ts".to_string(),
                file_path: "test.ts".to_string(),
                ..Default::default()
            },
        });

        let extracted = parse_files(&mut graph, &[file], None).unwrap();
        let require_import = extracted
            .imports
            .iter()
            .find(|import| {
                import.raw_import_path == "./utils.js"
                    && import.binding_text.as_deref() == Some("{ run: execute }")
            })
            .expect("Should extract require() member alias binding");
        assert_eq!(require_import.language, "typescript");
    }

    #[test]
    fn test_parse_typescript_require_member_call_result_is_not_alias_binding() {
        let file = FileEntry {
            path: "test.ts".to_string(),
            content: r#"export function load() {
  const execute = require("./utils.js").factory();
  return execute();
}"#
            .to_string(),
            size: 97,
            language: Some(SupportedLanguage::TypeScript),
        };

        let mut graph = KnowledgeGraph::new();
        graph.add_node(GraphNode {
            id: "File:test.ts".to_string(),
            label: NodeLabel::File,
            properties: NodeProperties {
                name: "test.ts".to_string(),
                file_path: "test.ts".to_string(),
                ..Default::default()
            },
        });

        let extracted = parse_files(&mut graph, &[file], None).unwrap();
        assert!(
            extracted.imports.iter().all(|import| {
                !matches!(
                    import.binding_text.as_deref(),
                    Some("{ factory: execute }" | r#"import * as execute from "./utils.js""#)
                )
            }),
            "require() member call results should not become local alias bindings"
        );
    }

    #[test]
    fn test_parse_typescript_namespace_member_alias_binding() {
        let file = FileEntry {
            path: "test.ts".to_string(),
            content: r#"import * as utils from "./utils.js";
export function load() {
  const execute = utils.run;
  return execute();
}"#
            .to_string(),
            size: 108,
            language: Some(SupportedLanguage::TypeScript),
        };

        let mut graph = KnowledgeGraph::new();
        graph.add_node(GraphNode {
            id: "File:test.ts".to_string(),
            label: NodeLabel::File,
            properties: NodeProperties {
                name: "test.ts".to_string(),
                file_path: "test.ts".to_string(),
                ..Default::default()
            },
        });

        let extracted = parse_files(&mut graph, &[file], None).unwrap();
        let member_alias_import = extracted
            .imports
            .iter()
            .find(|import| {
                import.raw_import_path == "./utils.js"
                    && import.binding_text.as_deref() == Some("{ run: execute }")
            })
            .expect("Should extract namespace import member alias binding");
        assert_eq!(member_alias_import.language, "typescript");
    }

    #[test]
    fn test_parse_typescript_type_alias_and_enum() {
        let file = FileEntry {
            path: "types.ts".to_string(),
            content: r#"
export type RunMode = "auto" | "manual";
export enum ToolStatus { Ready, Running, Failed }
"#
            .to_string(),
            size: 88,
            language: Some(SupportedLanguage::TypeScript),
        };

        let mut graph = KnowledgeGraph::new();
        graph.add_node(GraphNode {
            id: "File:types.ts".to_string(),
            label: NodeLabel::File,
            properties: NodeProperties {
                name: "types.ts".to_string(),
                file_path: "types.ts".to_string(),
                ..Default::default()
            },
        });

        let _extracted = parse_files(&mut graph, &[file], None).unwrap();

        assert!(
            graph.iter_nodes().any(|node| {
                node.label == NodeLabel::TypeAlias && node.properties.name == "RunMode"
            }),
            "Should create TypeAlias node for RunMode"
        );
        assert!(
            graph.iter_nodes().any(|node| {
                node.label == NodeLabel::Enum && node.properties.name == "ToolStatus"
            }),
            "Should create Enum node for ToolStatus"
        );
    }

    #[test]
    fn test_parse_typescript_interface_members() {
        let file = FileEntry {
            path: "client.ts".to_string(),
            content: r#"
export interface CodeBuddyClient {
  readonly id: string;
  chat(prompt: string): Promise<string>;
}
"#
            .to_string(),
            size: 95,
            language: Some(SupportedLanguage::TypeScript),
        };

        let mut graph = KnowledgeGraph::new();
        graph.add_node(GraphNode {
            id: "File:client.ts".to_string(),
            label: NodeLabel::File,
            properties: NodeProperties {
                name: "client.ts".to_string(),
                file_path: "client.ts".to_string(),
                ..Default::default()
            },
        });

        let _extracted = parse_files(&mut graph, &[file], None).unwrap();

        let interface = graph
            .iter_nodes()
            .find(|node| {
                node.label == NodeLabel::Interface && node.properties.name == "CodeBuddyClient"
            })
            .expect("Should create Interface node");
        let chat = graph
            .iter_nodes()
            .find(|node| node.label == NodeLabel::Method && node.properties.name == "chat")
            .expect("Should create Method node for interface method signature");
        let id = graph
            .iter_nodes()
            .find(|node| node.label == NodeLabel::Property && node.properties.name == "id")
            .expect("Should create Property node for interface property signature");

        assert!(
            graph.iter_relationships().any(|rel| {
                rel.source_id == interface.id
                    && rel.target_id == chat.id
                    && rel.rel_type == RelationshipType::HasMethod
            }),
            "Should nest interface method signature under the interface"
        );
        assert!(
            graph.iter_relationships().any(|rel| {
                rel.source_id == interface.id
                    && rel.target_id == id.id
                    && rel.rel_type == RelationshipType::HasProperty
            }),
            "Should nest interface property signature under the interface"
        );
    }

    #[test]
    fn test_parse_typescript_react_memo_component() {
        let file = FileEntry {
            path: "panel.tsx".to_string(),
            content: r#"
import React from "react";

function helper() { return null; }

export const Panel = React.memo(function Panel() {
  return helper();
});
"#
            .to_string(),
            size: 135,
            language: Some(SupportedLanguage::TypeScript),
        };

        let mut graph = KnowledgeGraph::new();
        graph.add_node(GraphNode {
            id: "File:panel.tsx".to_string(),
            label: NodeLabel::File,
            properties: NodeProperties {
                name: "panel.tsx".to_string(),
                file_path: "panel.tsx".to_string(),
                ..Default::default()
            },
        });

        let _extracted = parse_files(&mut graph, &[file], None).unwrap();

        assert!(
            graph.iter_nodes().any(|node| {
                node.label == NodeLabel::Function && node.properties.name == "Panel"
            }),
            "Should create Function node for React.memo component"
        );
    }

    #[test]
    fn test_parse_typescript_react_memo_identifier_alias() {
        let file = FileEntry {
            path: "dialog.tsx".to_string(),
            content: r#"
import React from "react";

function DialogInner() { return null; }

const Dialog = React.memo(DialogInner);
export default Dialog;
"#
            .to_string(),
            size: 128,
            language: Some(SupportedLanguage::TypeScript),
        };

        let mut graph = KnowledgeGraph::new();
        graph.add_node(GraphNode {
            id: "File:dialog.tsx".to_string(),
            label: NodeLabel::File,
            properties: NodeProperties {
                name: "dialog.tsx".to_string(),
                file_path: "dialog.tsx".to_string(),
                ..Default::default()
            },
        });

        let _extracted = parse_files(&mut graph, &[file], None).unwrap();

        assert!(
            graph.iter_nodes().any(|node| {
                node.label == NodeLabel::Function && node.properties.name == "Dialog"
            }),
            "Should create Function node for React.memo identifier alias"
        );
    }

    #[test]
    fn test_parse_python_function() {
        let file = FileEntry {
            path: "test.py".to_string(),
            content: "def hello(name, age):\n    return name".to_string(),
            size: 38,
            language: Some(SupportedLanguage::Python),
        };

        let mut graph = KnowledgeGraph::new();
        graph.add_node(GraphNode {
            id: "File:test.py".to_string(),
            label: NodeLabel::File,
            properties: NodeProperties {
                name: "test.py".to_string(),
                file_path: "test.py".to_string(),
                ..Default::default()
            },
        });

        let _extracted = parse_files(&mut graph, &[file], None).unwrap();

        let func_node = graph.get_node("Function:test.py:hello");
        assert!(func_node.is_some(), "Should create Function node for hello");
        assert_eq!(func_node.unwrap().properties.name, "hello");
    }

    #[test]
    fn test_parse_empty_files() {
        let extracted = parse_files(&mut KnowledgeGraph::new(), &[], None).unwrap();
        assert!(extracted.imports.is_empty());
        assert!(extracted.calls.is_empty());
    }

    #[test]
    fn test_parse_unsupported_language_skipped() {
        let file = FileEntry {
            path: "test.kt".to_string(),
            content: "fun main() {}".to_string(),
            size: 14,
            language: Some(SupportedLanguage::Kotlin),
        };

        let mut graph = KnowledgeGraph::new();
        let extracted = parse_files(&mut graph, &[file], None).unwrap();
        // Kotlin uses fallback grammar, so it's skipped
        assert!(extracted.imports.is_empty());
    }

    #[test]
    fn test_build_symbol_table() {
        let mut graph = KnowledgeGraph::new();
        graph.add_node(GraphNode {
            id: "Function:src/main.ts:handleLogin".to_string(),
            label: NodeLabel::Function,
            properties: NodeProperties {
                name: "handleLogin".to_string(),
                file_path: "src/main.ts".to_string(),
                is_exported: Some(true),
                parameter_count: Some(2),
                ..Default::default()
            },
        });

        let mut table = SymbolTable::new();
        build_symbol_table(&graph, &mut table);

        let results = table.lookup_global("handleLogin");
        assert!(results.is_some());
        assert_eq!(results.unwrap().len(), 1);
    }
}

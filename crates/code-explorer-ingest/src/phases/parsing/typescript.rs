use std::collections::{HashMap, HashSet};

use code_explorer_core::config::languages::SupportedLanguage;
use tree_sitter::Node;

use crate::phases::structure::FileEntry;

use code_explorer_core::graph::types::{GraphNode, GraphRelationship, NodeLabel};

use super::{CallForm, ExtractedCall, ExtractedData, ExtractedImport};

/// JavaScript and TypeScript share the same import/call parsing hooks here.
pub(super) fn is_script_language(lang: SupportedLanguage) -> bool {
    matches!(
        lang,
        SupportedLanguage::JavaScript | SupportedLanguage::TypeScript
    )
}

/// Extract bindings from `const { foo } = await import("./module")` and
/// `const { foo } = require("./module")`.
///
/// The query captures the dynamic import / require call expression as `@import`,
/// while the named bindings live on the enclosing `variable_declarator`.
pub(super) fn dynamic_import_binding_text(import_node: Node, file: &FileEntry) -> Option<String> {
    let content = file.content.as_bytes();
    let mut node = import_node;
    while let Some(parent) = node.parent() {
        if parent.kind() == "call_expression" && is_then_call(parent, content) {
            if let Some(binding_text) = then_callback_destructured_binding(parent, content) {
                return Some(binding_text);
            }
        }

        if parent.kind() == "variable_declarator" {
            if !is_direct_dynamic_import_initializer(import_node, parent) {
                return None;
            }

            let name_node = parent.child_by_field_name("name")?;
            if name_node.kind() == "object_pattern" {
                return name_node
                    .utf8_text(file.content.as_bytes())
                    .ok()
                    .map(str::to_string);
            }
            if name_node.kind() == "identifier" {
                let alias = name_node.utf8_text(content).ok()?;
                let source = import_node
                    .child_by_field_name("arguments")
                    .and_then(|args| args.named_child(0))
                    .and_then(|source| source.utf8_text(content).ok())?;
                if let Some(exported) =
                    dynamic_import_member_export_name(import_node, parent, content)
                {
                    return Some(identifier_dynamic_import_binding_text(
                        alias,
                        source,
                        Some(&exported),
                    ));
                }
                if is_direct_module_namespace_initializer(import_node, parent) {
                    return Some(format!("import * as {alias} from {source}"));
                }
                return None;
            }
            return None;
        }

        if matches!(parent.kind(), "program" | "statement_block") {
            return None;
        }

        node = parent;
    }
    None
}

fn is_direct_module_namespace_initializer(import_node: Node, variable_declarator: Node) -> bool {
    let Some(value_node) = variable_declarator.child_by_field_name("value") else {
        return false;
    };
    let value_node = unwrap_expression(value_node);
    same_node(value_node, import_node)
}

fn dynamic_import_member_export_name(
    import_node: Node,
    variable_declarator: Node,
    content: &[u8],
) -> Option<String> {
    let value_node = variable_declarator.child_by_field_name("value")?;
    let value_node = unwrap_expression(value_node);
    if value_node.kind() != "member_expression" {
        return None;
    }

    let object_node = value_node.child_by_field_name("object")?;
    if !contains_node(object_node, import_node) {
        return None;
    }

    value_node
        .child_by_field_name("property")
        .and_then(|property| property_key_text(property, content))
}

fn then_callback_destructured_binding(then_call: Node, content: &[u8]) -> Option<String> {
    let callback = then_call
        .child_by_field_name("arguments")
        .and_then(|args| args.named_child(0))?;

    callback_first_parameter(callback)
        .filter(|parameter| parameter.kind() == "object_pattern")
        .and_then(|parameter| node_text(parameter, content))
}

fn callback_first_parameter(callback: Node) -> Option<Node> {
    let parameters = callback.child_by_field_name("parameters")?;
    if matches!(parameters.kind(), "object_pattern" | "identifier") {
        return Some(parameters);
    }

    if parameters.kind() != "formal_parameters" {
        return None;
    }

    for idx in 0..parameters.named_child_count() {
        let Some(parameter) = parameters.named_child(idx) else {
            continue;
        };
        if matches!(parameter.kind(), "object_pattern" | "identifier") {
            return Some(parameter);
        }
        if let Some(pattern) = parameter.child_by_field_name("pattern") {
            if matches!(pattern.kind(), "object_pattern" | "identifier") {
                return Some(pattern);
            }
        }
    }

    None
}

/// Recover bindings from destructuring parallel dynamic imports:
///
/// ```ts
/// const [{ getSettingsManager }, { buildLearningRetrospective }] =
///   await Promise.all([
///     import("./settings-manager.js"),
///     import("./learning-agent.js"),
///   ]);
/// ```
///
/// The import query sees both `import()` calls, but their local names are on the
/// array pattern. Matching positions in the pattern and the `Promise.all` array
/// lets the call resolver use `named-import` for the destructured functions.
pub(super) fn extract_promise_all_dynamic_import_bindings(
    root: Node,
    file: &FileEntry,
    lang: SupportedLanguage,
    extracted: &mut ExtractedData,
) {
    let mut seen: HashSet<(String, String)> = extracted
        .imports
        .iter()
        .filter_map(|import| {
            import
                .binding_text
                .as_ref()
                .map(|binding| (import.raw_import_path.clone(), binding.clone()))
        })
        .collect();

    collect_promise_all_dynamic_import_bindings(root, file, lang, &mut seen, extracted);
}

fn collect_promise_all_dynamic_import_bindings(
    node: Node,
    file: &FileEntry,
    lang: SupportedLanguage,
    seen: &mut HashSet<(String, String)>,
    extracted: &mut ExtractedData,
) {
    if node.kind() == "variable_declarator" {
        for (source, binding_text) in
            promise_all_dynamic_import_bindings(node, file.content.as_bytes())
        {
            if seen.insert((source.clone(), binding_text.clone())) {
                extracted.imports.push(ExtractedImport {
                    file_path: file.path.clone(),
                    raw_import_path: source,
                    binding_text: Some(binding_text),
                    language: lang.as_str().to_string(),
                });
            }
        }
    }

    for idx in 0..node.named_child_count() {
        if let Some(child) = node.named_child(idx) {
            collect_promise_all_dynamic_import_bindings(child, file, lang, seen, extracted);
        }
    }
}

fn promise_all_dynamic_import_bindings(declarator: Node, content: &[u8]) -> Vec<(String, String)> {
    let Some(name_node) = declarator.child_by_field_name("name") else {
        return Vec::new();
    };
    if name_node.kind() != "array_pattern" {
        return Vec::new();
    }

    let Some(value_node) = declarator.child_by_field_name("value") else {
        return Vec::new();
    };
    let call_node = unwrap_expression(value_node);
    if !is_promise_all_call(call_node, content) {
        return Vec::new();
    }

    let Some(imports_array) = call_node
        .child_by_field_name("arguments")
        .and_then(|args| args.named_child(0))
        .filter(|node| node.kind() == "array")
    else {
        return Vec::new();
    };

    let mut bindings = Vec::new();
    let pair_count = name_node
        .named_child_count()
        .min(imports_array.named_child_count());
    for idx in 0..pair_count {
        let Some(pattern_node) = name_node.named_child(idx) else {
            continue;
        };
        let Some(import_node) = imports_array.named_child(idx) else {
            continue;
        };
        let Some(module_ref) = find_dynamic_module_reference(import_node, content) else {
            continue;
        };
        let Some(binding_text) =
            promise_all_pattern_binding_text(pattern_node, &module_ref, content)
        else {
            continue;
        };
        bindings.push((module_ref.source, binding_text));
    }

    bindings
}

fn promise_all_pattern_binding_text(
    pattern_node: Node,
    module_ref: &DynamicModuleReference,
    content: &[u8],
) -> Option<String> {
    match pattern_node.kind() {
        "object_pattern" => node_text(pattern_node, content),
        "identifier" => {
            let alias = node_text(pattern_node, content)?;
            Some(identifier_dynamic_import_binding_text(
                &alias,
                &module_ref.source,
                module_ref.returned_export.as_deref(),
            ))
        }
        _ => None,
    }
}

fn is_promise_all_call(call_node: Node, content: &[u8]) -> bool {
    if call_node.kind() != "call_expression" {
        return false;
    }

    let Some(function_node) = call_node.child_by_field_name("function") else {
        return false;
    };
    if function_node.kind() != "member_expression" {
        return false;
    }

    let object_is_promise = function_node
        .child_by_field_name("object")
        .and_then(|object| node_text(object, content))
        .as_deref()
        == Some("Promise");
    let property_is_all = function_node
        .child_by_field_name("property")
        .and_then(|property| node_text(property, content))
        .as_deref()
        == Some("all");

    object_is_promise && property_is_all
}

/// Recover bindings from local helpers that return a dynamic import member:
///
/// ```ts
/// async function getEnqueueMessage() {
///   const mod = await import("./channels.js");
///   return mod.enqueueMessage;
/// }
/// const enqueueMessage = await getEnqueueMessage();
/// ```
///
/// Code Buddy uses the same shape with a cached module member. Treating the call
/// result as an imported binding keeps the edge precise and avoids fuzzy
/// import-scoped fallback.
pub(super) fn extract_dynamic_import_return_factory_bindings(
    root: Node,
    file: &FileEntry,
    lang: SupportedLanguage,
    extracted: &mut ExtractedData,
) {
    let content = file.content.as_bytes();
    let mut factories = Vec::new();
    collect_dynamic_import_return_factories(root, content, &mut factories);
    if factories.is_empty() {
        return;
    }

    let mut seen: HashSet<(String, String)> = extracted
        .imports
        .iter()
        .filter_map(|import| {
            import
                .binding_text
                .as_ref()
                .map(|binding| (import.raw_import_path.clone(), binding.clone()))
        })
        .collect();

    collect_dynamic_import_return_factory_usages(
        root, file, lang, &factories, &mut seen, extracted,
    );
}

#[derive(Debug, Clone)]
struct DynamicImportReturnFactory {
    function_name: String,
    source: String,
    returned_export: String,
    declaration_end: usize,
    scope_start: usize,
    scope_end: usize,
}

fn collect_dynamic_import_return_factories(
    node: Node,
    content: &[u8],
    factories: &mut Vec<DynamicImportReturnFactory>,
) {
    if node.kind() == "function_declaration" {
        collect_dynamic_import_return_factory(node, content, factories);
    }

    for idx in 0..node.named_child_count() {
        if let Some(child) = node.named_child(idx) {
            collect_dynamic_import_return_factories(child, content, factories);
        }
    }
}

fn collect_dynamic_import_return_factory(
    function_node: Node,
    content: &[u8],
    factories: &mut Vec<DynamicImportReturnFactory>,
) -> Option<()> {
    let function_name = function_node
        .child_by_field_name("name")
        .and_then(|name| node_text(name, content))?;
    let body = function_node.child_by_field_name("body")?;
    let module_ref = find_dynamic_import_returned_member(body, content)?;
    let (scope_start, scope_end) = enclosing_scope_bounds(function_node);

    factories.push(DynamicImportReturnFactory {
        function_name,
        source: module_ref.source,
        returned_export: module_ref.returned_export?,
        declaration_end: function_node.end_byte(),
        scope_start,
        scope_end,
    });

    Some(())
}

fn find_dynamic_import_returned_member(
    body: Node,
    content: &[u8],
) -> Option<DynamicModuleReference> {
    let mut module_aliases = HashMap::new();
    let mut binding_aliases = HashMap::new();
    let mut returned = None;
    scan_dynamic_import_return(
        body,
        content,
        &mut module_aliases,
        &mut binding_aliases,
        &mut returned,
    );
    returned
}

fn scan_dynamic_import_return(
    node: Node,
    content: &[u8],
    module_aliases: &mut HashMap<String, String>,
    binding_aliases: &mut HashMap<String, DynamicModuleReference>,
    returned: &mut Option<DynamicModuleReference>,
) {
    if returned.is_some() {
        return;
    }

    match node.kind() {
        "variable_declarator" => {
            collect_dynamic_import_return_variable(node, content, module_aliases, binding_aliases);
        }
        "assignment_expression" => {
            collect_dynamic_import_return_assignment(
                node,
                content,
                module_aliases,
                binding_aliases,
            );
        }
        "return_statement" => {
            *returned = return_statement_dynamic_module_reference(node, content, binding_aliases);
            if returned.is_some() {
                return;
            }
        }
        _ => {}
    }

    for idx in 0..node.named_child_count() {
        let Some(child) = node.named_child(idx) else {
            continue;
        };
        if child.kind() != "statement_block" && is_nested_dynamic_return_boundary(child) {
            continue;
        }
        scan_dynamic_import_return(child, content, module_aliases, binding_aliases, returned);
    }
}

fn collect_dynamic_import_return_variable(
    declarator: Node,
    content: &[u8],
    module_aliases: &mut HashMap<String, String>,
    binding_aliases: &mut HashMap<String, DynamicModuleReference>,
) -> Option<()> {
    let name_node = declarator.child_by_field_name("name")?;
    if name_node.kind() != "identifier" {
        return None;
    }
    let local_name = node_text(name_node, content)?;
    let value_node = declarator.child_by_field_name("value")?;
    let value_node = unwrap_expression(value_node);

    if value_node.kind() == "call_expression" && is_dynamic_module_call(value_node, content) {
        let source = value_node
            .child_by_field_name("arguments")
            .and_then(|args| args.named_child(0))
            .and_then(|source| string_literal_text(source, content))?;
        module_aliases.insert(local_name, source);
        return Some(());
    }

    if let Some(module_ref) = dynamic_module_member_reference(value_node, content, module_aliases) {
        binding_aliases.insert(local_name, module_ref);
    }

    Some(())
}

fn collect_dynamic_import_return_assignment(
    assignment: Node,
    content: &[u8],
    module_aliases: &HashMap<String, String>,
    binding_aliases: &mut HashMap<String, DynamicModuleReference>,
) -> Option<()> {
    let left_node = assignment.child_by_field_name("left")?;
    if left_node.kind() != "identifier" {
        return None;
    }
    let local_name = node_text(left_node, content)?;
    let right_node = assignment.child_by_field_name("right")?;
    let module_ref = dynamic_module_member_reference(right_node, content, module_aliases)?;
    binding_aliases.insert(local_name, module_ref);
    Some(())
}

fn return_statement_dynamic_module_reference(
    return_node: Node,
    content: &[u8],
    binding_aliases: &HashMap<String, DynamicModuleReference>,
) -> Option<DynamicModuleReference> {
    let argument = return_node
        .child_by_field_name("argument")
        .or_else(|| return_node.named_child(0))?;
    let argument = unwrap_expression(argument);

    if argument.kind() == "identifier" {
        let local_name = node_text(argument, content)?;
        return binding_aliases.get(&local_name).cloned();
    }

    None
}

fn dynamic_module_member_reference(
    node: Node,
    content: &[u8],
    module_aliases: &HashMap<String, String>,
) -> Option<DynamicModuleReference> {
    let node = unwrap_expression(node);
    if node.kind() != "member_expression" {
        return None;
    }

    let object_name = node
        .child_by_field_name("object")
        .and_then(|object| node_text(object, content))?;
    let source = module_aliases.get(&object_name)?;
    let returned_export = node
        .child_by_field_name("property")
        .and_then(|property| node_text(property, content))?;

    Some(DynamicModuleReference {
        source: source.clone(),
        returned_export: Some(returned_export),
    })
}

fn collect_dynamic_import_return_factory_usages(
    node: Node,
    file: &FileEntry,
    lang: SupportedLanguage,
    factories: &[DynamicImportReturnFactory],
    seen: &mut HashSet<(String, String)>,
    extracted: &mut ExtractedData,
) {
    if node.kind() == "variable_declarator" {
        if let Some((source, binding_text)) =
            dynamic_import_return_factory_usage_binding(node, file.content.as_bytes(), factories)
        {
            if seen.insert((source.clone(), binding_text.clone())) {
                extracted.imports.push(ExtractedImport {
                    file_path: file.path.clone(),
                    raw_import_path: source,
                    binding_text: Some(binding_text),
                    language: lang.as_str().to_string(),
                });
            }
        }
    }

    for idx in 0..node.named_child_count() {
        if let Some(child) = node.named_child(idx) {
            collect_dynamic_import_return_factory_usages(
                child, file, lang, factories, seen, extracted,
            );
        }
    }
}

fn dynamic_import_return_factory_usage_binding(
    declarator: Node,
    content: &[u8],
    factories: &[DynamicImportReturnFactory],
) -> Option<(String, String)> {
    let name_node = declarator.child_by_field_name("name")?;
    if name_node.kind() != "identifier" {
        return None;
    }
    let alias = node_text(name_node, content)?;

    let value_node = declarator.child_by_field_name("value")?;
    let call_node = unwrap_expression(value_node);
    if call_node.kind() != "call_expression" {
        return None;
    }

    let function_node = call_node.child_by_field_name("function")?;
    if function_node.kind() != "identifier" {
        return None;
    }
    let function_name = node_text(function_node, content)?;

    let factory = matching_return_factory(
        factories,
        &function_name,
        declarator.start_byte(),
        declarator.end_byte(),
    )?;
    let binding_text = identifier_dynamic_import_binding_text(
        &alias,
        &factory.source,
        Some(&factory.returned_export),
    );

    Some((factory.source.clone(), binding_text))
}

fn matching_return_factory<'a>(
    factories: &'a [DynamicImportReturnFactory],
    function_name: &str,
    usage_start: usize,
    usage_end: usize,
) -> Option<&'a DynamicImportReturnFactory> {
    factories
        .iter()
        .filter(|factory| {
            factory.function_name == function_name
                && factory.declaration_end <= usage_start
                && factory.scope_start <= usage_start
                && usage_end <= factory.scope_end
        })
        .max_by_key(|factory| factory.declaration_end)
}

fn is_nested_dynamic_return_boundary(node: Node) -> bool {
    matches!(
        node.kind(),
        "arrow_function"
            | "function_expression"
            | "function_declaration"
            | "method_definition"
            | "generator_function"
            | "class_declaration"
    )
}

/// Recover bindings for local lazy dynamic import factories:
///
/// ```ts
/// const lazyImport = {
///   renderers: () => lazyLoad("renderers", () => import("./renderers/index.js")),
/// };
/// const { initializeRenderers } = await lazyImport.renderers();
/// ```
///
/// The tree-sitter import query sees the nested `import()` call, but the named
/// bindings live at the call site of `lazyImport.renderers()`. This pass links
/// those two AST locations while staying scoped to local factory declarations.
pub(super) fn extract_dynamic_import_factory_bindings(
    root: Node,
    file: &FileEntry,
    lang: SupportedLanguage,
    extracted: &mut ExtractedData,
) {
    let content = file.content.as_bytes();
    let mut factories = Vec::new();
    collect_dynamic_import_factories(root, content, &mut factories);
    if factories.is_empty() {
        return;
    }

    let mut seen: HashSet<(String, String)> = extracted
        .imports
        .iter()
        .filter_map(|import| {
            import
                .binding_text
                .as_ref()
                .map(|binding| (import.raw_import_path.clone(), binding.clone()))
        })
        .collect();

    collect_dynamic_import_factory_usages(root, file, lang, &factories, &mut seen, extracted);
}

/// Recover bindings from destructuring a local imported factory/hook result:
///
/// ```ts
/// import { useEnhancedInput } from "./use-enhanced-input.js";
/// const { setInput, clearInput } = useEnhancedInput();
/// ```
///
/// The destructured names are runtime callbacks produced by the imported module,
/// so treating them as local bindings to that module lets the call resolver use
/// `named-import` instead of fuzzy import-scoped matching.
pub(super) fn extract_imported_call_result_bindings(
    root: Node,
    file: &FileEntry,
    lang: SupportedLanguage,
    extracted: &mut ExtractedData,
) {
    let imported_call_sources = imported_call_sources(file, lang, extracted);
    if imported_call_sources.is_empty() {
        return;
    }

    let mut seen: HashSet<(String, String)> = extracted
        .imports
        .iter()
        .filter_map(|import| {
            import
                .binding_text
                .as_ref()
                .map(|binding| (import.raw_import_path.clone(), binding.clone()))
        })
        .collect();

    collect_imported_call_result_bindings(
        root,
        file,
        lang,
        &imported_call_sources,
        &mut seen,
        extracted,
    );
}

/// Recover bindings from destructuring or member-aliasing a namespace import:
///
/// ```ts
/// import * as api from "./api.js";
/// const { run: execute } = api;
/// const executeAgain = api.run;
/// execute();
/// ```
///
/// This makes the local names (`execute`, `executeAgain`) resolve through the
/// namespace source (`api.ts:run`) instead of falling back to import/global
/// name matching.
pub(super) fn extract_namespace_import_member_bindings(
    root: Node,
    file: &FileEntry,
    lang: SupportedLanguage,
    extracted: &mut ExtractedData,
) {
    let namespace_sources = namespace_import_sources(file, lang, extracted);
    if namespace_sources.is_empty() {
        return;
    }

    let mut seen: HashSet<(String, String)> = extracted
        .imports
        .iter()
        .filter_map(|import| {
            import
                .binding_text
                .as_ref()
                .map(|binding| (import.raw_import_path.clone(), binding.clone()))
        })
        .collect();

    collect_namespace_import_member_bindings(
        root,
        file,
        lang,
        &namespace_sources,
        &mut seen,
        extracted,
    );
}

fn namespace_import_sources(
    file: &FileEntry,
    lang: SupportedLanguage,
    extracted: &ExtractedData,
) -> HashMap<String, String> {
    let provider = code_explorer_lang::registry::get_provider(lang);
    let mut sources = HashMap::new();

    for import in &extracted.imports {
        if import.file_path != file.path {
            continue;
        }

        let binding_text = import
            .binding_text
            .as_deref()
            .unwrap_or(&import.raw_import_path);
        if !binding_text.trim_start().starts_with("import ") {
            continue;
        }

        let Some(bindings) = provider.extract_named_bindings(binding_text) else {
            continue;
        };

        for binding in bindings {
            if binding.is_type_only || !binding.is_module_alias {
                continue;
            }
            sources
                .entry(binding.local)
                .or_insert_with(|| import.raw_import_path.clone());
        }
    }

    sources
}

fn collect_namespace_import_member_bindings(
    node: Node,
    file: &FileEntry,
    lang: SupportedLanguage,
    namespace_sources: &HashMap<String, String>,
    seen: &mut HashSet<(String, String)>,
    extracted: &mut ExtractedData,
) {
    if node.kind() == "variable_declarator" {
        if let Some((source, binding_text)) =
            namespace_import_member_binding(node, file.content.as_bytes(), namespace_sources)
        {
            if seen.insert((source.clone(), binding_text.clone())) {
                extracted.imports.push(ExtractedImport {
                    file_path: file.path.clone(),
                    raw_import_path: source,
                    binding_text: Some(binding_text),
                    language: lang.as_str().to_string(),
                });
            }
        }
    }

    for idx in 0..node.named_child_count() {
        if let Some(child) = node.named_child(idx) {
            collect_namespace_import_member_bindings(
                child,
                file,
                lang,
                namespace_sources,
                seen,
                extracted,
            );
        }
    }
}

fn namespace_import_member_binding(
    declarator: Node,
    content: &[u8],
    namespace_sources: &HashMap<String, String>,
) -> Option<(String, String)> {
    let name_node = declarator.child_by_field_name("name")?;
    let value_node = declarator.child_by_field_name("value")?;
    let value_node = unwrap_expression(value_node);

    match name_node.kind() {
        "object_pattern" => {
            if value_node.kind() != "identifier" {
                return None;
            }

            let namespace_name = node_text(value_node, content)?;
            let source = namespace_sources.get(&namespace_name)?;
            Some((source.clone(), node_text(name_node, content)?))
        }
        "identifier" => {
            let (source, exported) =
                namespace_member_source_and_export(value_node, content, namespace_sources)?;
            let alias = node_text(name_node, content)?;
            let binding_text =
                identifier_dynamic_import_binding_text(&alias, &source, Some(&exported));
            Some((source, binding_text))
        }
        _ => None,
    }
}

fn namespace_member_source_and_export(
    value_node: Node,
    content: &[u8],
    namespace_sources: &HashMap<String, String>,
) -> Option<(String, String)> {
    if value_node.kind() != "member_expression" {
        return None;
    }

    let object_node = value_node.child_by_field_name("object")?;
    if object_node.kind() != "identifier" {
        return None;
    }
    let namespace_name = node_text(object_node, content)?;
    let source = namespace_sources.get(&namespace_name)?.clone();
    let exported = value_node
        .child_by_field_name("property")
        .and_then(|property| property_key_text(property, content))?;

    Some((source, exported))
}

fn imported_call_sources(
    file: &FileEntry,
    lang: SupportedLanguage,
    extracted: &ExtractedData,
) -> std::collections::HashMap<String, String> {
    let provider = code_explorer_lang::registry::get_provider(lang);
    let mut sources = std::collections::HashMap::new();

    for import in &extracted.imports {
        if import.file_path != file.path || !is_local_import_path(&import.raw_import_path) {
            continue;
        }

        let binding_text = import
            .binding_text
            .as_deref()
            .unwrap_or(&import.raw_import_path);
        let Some(bindings) = provider.extract_named_bindings(binding_text) else {
            continue;
        };

        for binding in bindings {
            if binding.is_type_only || binding.is_module_alias {
                continue;
            }
            sources
                .entry(binding.local)
                .or_insert_with(|| import.raw_import_path.clone());
        }
    }

    sources
}

fn collect_imported_call_result_bindings(
    node: Node,
    file: &FileEntry,
    lang: SupportedLanguage,
    imported_call_sources: &std::collections::HashMap<String, String>,
    seen: &mut HashSet<(String, String)>,
    extracted: &mut ExtractedData,
) {
    if node.kind() == "variable_declarator" {
        if let Some((source, binding_text)) =
            imported_call_result_binding(node, file.content.as_bytes(), imported_call_sources)
        {
            if seen.insert((source.clone(), binding_text.clone())) {
                extracted.imports.push(ExtractedImport {
                    file_path: file.path.clone(),
                    raw_import_path: source,
                    binding_text: Some(binding_text),
                    language: lang.as_str().to_string(),
                });
            }
        }
    }

    for idx in 0..node.named_child_count() {
        if let Some(child) = node.named_child(idx) {
            collect_imported_call_result_bindings(
                child,
                file,
                lang,
                imported_call_sources,
                seen,
                extracted,
            );
        }
    }
}

fn imported_call_result_binding(
    declarator: Node,
    content: &[u8],
    imported_call_sources: &std::collections::HashMap<String, String>,
) -> Option<(String, String)> {
    let name_node = declarator.child_by_field_name("name")?;
    if name_node.kind() != "object_pattern" {
        return None;
    }

    let value_node = declarator.child_by_field_name("value")?;
    let call_node = unwrap_expression(value_node);
    if call_node.kind() != "call_expression" {
        return None;
    }

    let function_node = call_node.child_by_field_name("function")?;
    if function_node.kind() != "identifier" {
        return None;
    }

    let function_name = node_text(function_node, content)?;
    let source = imported_call_sources.get(&function_name)?;
    Some((source.clone(), node_text(name_node, content)?))
}

#[derive(Debug, Clone)]
struct DynamicImportFactory {
    object_name: String,
    property_name: String,
    source: String,
    returned_export: Option<String>,
    declaration_end: usize,
    scope_start: usize,
    scope_end: usize,
}

#[derive(Clone)]
struct DynamicModuleReference {
    source: String,
    returned_export: Option<String>,
}

fn collect_dynamic_import_factories(
    node: Node,
    content: &[u8],
    factories: &mut Vec<DynamicImportFactory>,
) {
    if node.kind() == "variable_declarator" {
        collect_dynamic_import_factory(node, content, factories);
    }

    for idx in 0..node.named_child_count() {
        if let Some(child) = node.named_child(idx) {
            collect_dynamic_import_factories(child, content, factories);
        }
    }
}

fn collect_dynamic_import_factory(
    declarator: Node,
    content: &[u8],
    factories: &mut Vec<DynamicImportFactory>,
) -> Option<()> {
    let name_node = declarator.child_by_field_name("name")?;
    if name_node.kind() != "identifier" {
        return None;
    }

    let object_name = node_text(name_node, content)?;
    let value_node = declarator.child_by_field_name("value")?;
    if value_node.kind() != "object" {
        return None;
    }

    let (scope_start, scope_end) = enclosing_scope_bounds(declarator);
    for idx in 0..value_node.named_child_count() {
        let Some(pair) = value_node.named_child(idx) else {
            continue;
        };
        if pair.kind() != "pair" {
            continue;
        }
        let Some(key_node) = pair.child_by_field_name("key") else {
            continue;
        };
        let Some(value_node) = pair.child_by_field_name("value") else {
            continue;
        };
        let Some(property_name) = property_key_text(key_node, content) else {
            continue;
        };
        let Some(module_ref) = find_dynamic_module_reference(value_node, content) else {
            continue;
        };

        factories.push(DynamicImportFactory {
            object_name: object_name.clone(),
            property_name,
            source: module_ref.source,
            returned_export: module_ref.returned_export,
            declaration_end: declarator.end_byte(),
            scope_start,
            scope_end,
        });
    }

    Some(())
}

fn collect_dynamic_import_factory_usages(
    node: Node,
    file: &FileEntry,
    lang: SupportedLanguage,
    factories: &[DynamicImportFactory],
    seen: &mut HashSet<(String, String)>,
    extracted: &mut ExtractedData,
) {
    if node.kind() == "variable_declarator" {
        if let Some((source, binding_text)) =
            dynamic_import_factory_usage_binding(node, file.content.as_bytes(), factories)
        {
            if seen.insert((source.clone(), binding_text.clone())) {
                extracted.imports.push(ExtractedImport {
                    file_path: file.path.clone(),
                    raw_import_path: source,
                    binding_text: Some(binding_text),
                    language: lang.as_str().to_string(),
                });
            }
        }
    }

    for idx in 0..node.named_child_count() {
        if let Some(child) = node.named_child(idx) {
            collect_dynamic_import_factory_usages(child, file, lang, factories, seen, extracted);
        }
    }
}

fn dynamic_import_factory_usage_binding(
    declarator: Node,
    content: &[u8],
    factories: &[DynamicImportFactory],
) -> Option<(String, String)> {
    let name_node = declarator.child_by_field_name("name")?;
    let value_node = declarator.child_by_field_name("value")?;
    let call_node = unwrap_expression(value_node);
    let (object_name, property_name) = factory_member_call_parts(call_node, content)?;
    let factory = matching_factory(
        factories,
        &object_name,
        &property_name,
        declarator.start_byte(),
        declarator.end_byte(),
    )?;

    let binding_text = match name_node.kind() {
        "object_pattern" => node_text(name_node, content)?,
        "identifier" => {
            let alias = node_text(name_node, content)?;
            factory_identifier_binding_text(&alias, factory)
        }
        _ => return None,
    };

    Some((factory.source.clone(), binding_text))
}

fn factory_identifier_binding_text(alias: &str, factory: &DynamicImportFactory) -> String {
    identifier_dynamic_import_binding_text(
        alias,
        &factory.source,
        factory.returned_export.as_deref(),
    )
}

fn identifier_dynamic_import_binding_text(
    alias: &str,
    source: &str,
    returned_export: Option<&str>,
) -> String {
    match returned_export {
        Some("default") => format!("import {alias} from \"{source}\""),
        Some(exported) if exported == alias => format!("{{ {alias} }}"),
        Some(exported) => format!("{{ {exported}: {alias} }}"),
        None => format!("import * as {alias} from \"{source}\""),
    }
}

fn matching_factory<'a>(
    factories: &'a [DynamicImportFactory],
    object_name: &str,
    property_name: &str,
    usage_start: usize,
    usage_end: usize,
) -> Option<&'a DynamicImportFactory> {
    factories
        .iter()
        .filter(|factory| {
            factory.object_name == object_name
                && factory.property_name == property_name
                && factory.declaration_end <= usage_start
                && factory.scope_start <= usage_start
                && usage_end <= factory.scope_end
        })
        .max_by_key(|factory| factory.declaration_end)
}

fn factory_member_call_parts(call_node: Node, content: &[u8]) -> Option<(String, String)> {
    if call_node.kind() != "call_expression" {
        return None;
    }

    let function_node = call_node.child_by_field_name("function")?;
    if function_node.kind() != "member_expression" {
        return None;
    }

    let object_node = function_node.child_by_field_name("object")?;
    if object_node.kind() != "identifier" {
        return None;
    }

    let property_node = function_node.child_by_field_name("property")?;
    if !matches!(property_node.kind(), "property_identifier" | "identifier") {
        return None;
    }

    Some((
        node_text(object_node, content)?,
        node_text(property_node, content)?,
    ))
}

fn find_dynamic_module_reference(node: Node, content: &[u8]) -> Option<DynamicModuleReference> {
    if node.kind() == "call_expression" && is_dynamic_module_call(node, content) {
        let source = node
            .child_by_field_name("arguments")
            .and_then(|args| args.named_child(0))
            .and_then(|source| string_literal_text(source, content))?;

        return Some(DynamicModuleReference {
            source,
            returned_export: returned_export_from_then_chain(node, content),
        });
    }

    for idx in 0..node.named_child_count() {
        if let Some(child) = node.named_child(idx) {
            if let Some(module_ref) = find_dynamic_module_reference(child, content) {
                return Some(module_ref);
            }
        }
    }

    None
}

fn returned_export_from_then_chain(import_call: Node, content: &[u8]) -> Option<String> {
    let mut node = import_call;
    while let Some(parent) = node.parent() {
        if parent.kind() == "call_expression" && is_then_call(parent, content) {
            return then_callback_returned_export(parent, content);
        }
        if is_binding_boundary(parent) {
            return None;
        }
        node = parent;
    }

    None
}

fn is_then_call(call_node: Node, content: &[u8]) -> bool {
    let Some(function_node) = call_node.child_by_field_name("function") else {
        return false;
    };
    if function_node.kind() != "member_expression" {
        return false;
    }

    function_node
        .child_by_field_name("property")
        .and_then(|property| node_text(property, content))
        .as_deref()
        == Some("then")
}

fn then_callback_returned_export(then_call: Node, content: &[u8]) -> Option<String> {
    let callback = then_call
        .child_by_field_name("arguments")
        .and_then(|args| args.named_child(0))?;

    returned_member_export(callback, callback, content)
}

fn returned_member_export(node: Node, callback: Node, content: &[u8]) -> Option<String> {
    if node.kind() == "member_expression" && is_returned_member_expression(node, callback) {
        return node
            .child_by_field_name("property")
            .and_then(|property| node_text(property, content));
    }

    for idx in 0..node.named_child_count() {
        if let Some(child) = node.named_child(idx) {
            if let Some(export_name) = returned_member_export(child, callback, content) {
                return Some(export_name);
            }
        }
    }

    None
}

fn is_returned_member_expression(member_node: Node, callback: Node) -> bool {
    let mut node = member_node;
    while let Some(parent) = node.parent() {
        if same_node(parent, callback) {
            return true;
        }
        if matches!(
            parent.kind(),
            "parenthesized_expression" | "return_statement"
        ) {
            node = parent;
            continue;
        }
        return false;
    }

    false
}

fn is_dynamic_module_call(node: Node, content: &[u8]) -> bool {
    let Some(function_node) = node.child_by_field_name("function") else {
        return false;
    };

    function_node.kind() == "import"
        || (function_node.kind() == "identifier"
            && node_text(function_node, content).as_deref() == Some("require"))
}

fn is_direct_dynamic_import_initializer(import_node: Node, variable_declarator: Node) -> bool {
    let Some(value_node) = variable_declarator.child_by_field_name("value") else {
        return false;
    };
    if !contains_node(value_node, import_node) {
        return false;
    }

    let mut node = import_node;
    while let Some(parent) = node.parent() {
        if same_node(parent, variable_declarator) {
            return true;
        }
        if is_binding_boundary(parent) {
            return false;
        }
        node = parent;
    }

    false
}

fn contains_node(root: Node, target: Node) -> bool {
    root.start_byte() <= target.start_byte() && target.end_byte() <= root.end_byte()
}

fn same_node(left: Node, right: Node) -> bool {
    left.kind() == right.kind()
        && left.start_byte() == right.start_byte()
        && left.end_byte() == right.end_byte()
}

fn is_binding_boundary(node: Node) -> bool {
    matches!(
        node.kind(),
        "arrow_function"
            | "function_expression"
            | "function_declaration"
            | "method_definition"
            | "generator_function"
            | "pair"
            | "object"
            | "class_declaration"
    )
}

fn unwrap_expression(mut node: Node) -> Node {
    while matches!(node.kind(), "await_expression" | "parenthesized_expression") {
        let Some(child) = node.named_child(0) else {
            break;
        };
        node = child;
    }
    node
}

fn enclosing_scope_bounds(mut node: Node) -> (usize, usize) {
    while let Some(parent) = node.parent() {
        if matches!(parent.kind(), "program" | "statement_block") {
            return (parent.start_byte(), parent.end_byte());
        }
        node = parent;
    }

    (node.start_byte(), node.end_byte())
}

fn property_key_text(node: Node, content: &[u8]) -> Option<String> {
    match node.kind() {
        "property_identifier" | "identifier" | "private_property_identifier" => {
            node_text(node, content)
        }
        "string" => string_literal_text(node, content),
        _ => None,
    }
}

fn is_local_import_path(path: &str) -> bool {
    path.starts_with("./") || path.starts_with("../")
}

fn string_literal_text(node: Node, content: &[u8]) -> Option<String> {
    let raw = node_text(node, content)?;
    Some(
        raw.trim()
            .trim_matches(|ch| matches!(ch, '"' | '\'' | '`'))
            .to_string(),
    )
}

fn node_text(node: Node, content: &[u8]) -> Option<String> {
    node.utf8_text(content).ok().map(str::to_string)
}

/// JS/TS script post-pass: run the binding extractors, JSX component-call
/// detection, and anonymous default-export synthesis for a parsed script file.
pub(super) fn post_parse_script(
    root: tree_sitter::Node,
    file: &FileEntry,
    lang: SupportedLanguage,
    file_node_id: &str,
    nodes: &mut Vec<GraphNode>,
    relationships: &mut Vec<GraphRelationship>,
    extracted: &mut ExtractedData,
) {
    extract_promise_all_dynamic_import_bindings(root, file, lang, extracted);
    extract_dynamic_import_return_factory_bindings(root, file, lang, extracted);
    extract_dynamic_import_factory_bindings(root, file, lang, extracted);
    extract_imported_call_result_bindings(root, file, lang, extracted);
    extract_namespace_import_member_bindings(root, file, lang, extracted);
    extract_jsx_component_calls(root, file, file_node_id, extracted);
    process_script_default_exports(root, file, lang, file_node_id, nodes, relationships);
}

fn extract_jsx_component_calls(
    node: tree_sitter::Node,
    file: &FileEntry,
    file_node_id: &str,
    extracted: &mut ExtractedData,
) {
    if !is_jsx_like_file(&file.path) {
        return;
    }

    collect_jsx_component_calls(node, file, file_node_id, extracted);
}

fn collect_jsx_component_calls(
    node: tree_sitter::Node,
    file: &FileEntry,
    file_node_id: &str,
    extracted: &mut ExtractedData,
) {
    if matches!(
        node.kind(),
        "jsx_self_closing_element" | "jsx_opening_element"
    ) {
        if let Some((called_name, call_form, receiver_name)) =
            jsx_component_call_parts(node, file.content.as_bytes())
        {
            let source_id =
                super::find_enclosing_method_id(&node, &file.path, file.content.as_bytes())
                    .unwrap_or_else(|| file_node_id.to_string());
            extracted.calls.push(ExtractedCall {
                file_path: file.path.clone(),
                called_name,
                source_id,
                arg_count: None,
                call_form,
                receiver_name,
                receiver_type_name: None,
            });
        }
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_jsx_component_calls(child, file, file_node_id, extracted);
    }
}

fn jsx_component_call_parts(
    element_node: tree_sitter::Node,
    content: &[u8],
) -> Option<(String, CallForm, Option<String>)> {
    let name_node = element_node.child_by_field_name("name")?;
    match name_node.kind() {
        "identifier" => {
            let name = name_node.utf8_text(content).ok()?;
            is_uppercase_jsx_component_name(name).then(|| (name.to_string(), CallForm::Free, None))
        }
        "member_expression" => {
            let property = name_node.child_by_field_name("property")?;
            let name = property.utf8_text(content).ok()?;
            if !is_uppercase_jsx_component_name(name) {
                return None;
            }
            let receiver = name_node
                .child_by_field_name("object")
                .and_then(|object| object.utf8_text(content).ok())
                .map(str::to_string)?;
            Some((name.to_string(), CallForm::Member, Some(receiver)))
        }
        _ => None,
    }
}

fn is_jsx_like_file(path: &str) -> bool {
    matches!(
        std::path::Path::new(path)
            .extension()
            .and_then(|ext| ext.to_str()),
        Some("tsx" | "jsx")
    )
}

fn is_uppercase_jsx_component_name(name: &str) -> bool {
    name.as_bytes()
        .first()
        .is_some_and(|first| first.is_ascii_uppercase())
}

fn process_script_default_exports(
    node: tree_sitter::Node,
    file: &FileEntry,
    lang: SupportedLanguage,
    file_node_id: &str,
    nodes: &mut Vec<GraphNode>,
    relationships: &mut Vec<GraphRelationship>,
) {
    match node.kind() {
        "function_declaration" | "function_expression"
            if is_anonymous_default_export_declaration(&node, file.content.as_bytes()) =>
        {
            let params_text = node
                .child_by_field_name("parameters")
                .and_then(|params| params.utf8_text(file.content.as_bytes()).ok());
            super::create_synthetic_definition_node(
                NodeLabel::Function,
                "default",
                &node,
                params_text,
                file,
                lang,
                file_node_id,
                nodes,
                relationships,
            );
        }
        "class_declaration" | "class"
            if is_anonymous_default_export_declaration(&node, file.content.as_bytes()) =>
        {
            super::create_synthetic_definition_node(
                NodeLabel::Class,
                "default",
                &node,
                None,
                file,
                lang,
                file_node_id,
                nodes,
                relationships,
            );
        }
        _ => {}
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        process_script_default_exports(child, file, lang, file_node_id, nodes, relationships);
    }
}

pub(super) fn is_anonymous_default_export_declaration(
    node: &tree_sitter::Node,
    content: &[u8],
) -> bool {
    node.child_by_field_name("name").is_none()
        && node
            .parent()
            .is_some_and(|parent| super::is_default_export_statement(parent, content))
}

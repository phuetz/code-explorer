use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use code_explorer_core::config::languages::SupportedLanguage;
use code_explorer_core::graph::types::*;
use code_explorer_core::graph::KnowledgeGraph;
use code_explorer_core::id::generate_id;
use code_explorer_core::resolution::types::*;
use code_explorer_core::symbol::SymbolTable;
use serde_json::Value;
use tree_sitter::Parser;

use code_explorer_lang::import_resolvers::types::{
    ImportConfigs, ImportResult, ResolveCtx, SuffixIndex,
};
use code_explorer_lang::registry::get_provider;

use crate::phases::parsing::ExtractedData;
use crate::phases::structure::FileEntry;
use crate::IngestError;

/// Resolve all imports and build dependency maps.
pub fn resolve_imports(
    graph: &mut KnowledgeGraph,
    repo_path: &Path,
    files: &[FileEntry],
    extracted: &ExtractedData,
    _symbol_table: &SymbolTable,
) -> Result<
    (
        ImportMap,
        NamedImportMap,
        ReExportMap,
        PackageMap,
        ModuleAliasMap,
    ),
    IngestError,
> {
    // Build file path sets and suffix index
    let all_paths: Vec<String> = files.iter().map(|f| f.path.clone()).collect();
    let all_set: HashSet<String> = all_paths.iter().cloned().collect();
    // Lower-cased copy used by case-insensitive resolvers (e.g. Windows where
    // `Controllers/HomeController.cs` may be referenced as `controllers/...`).
    // The SuffixIndex itself stores both case-sensitive and case-insensitive
    // entries internally, so it builds from `(all_paths, all_paths)` as before.
    // The ResolveCtx now exposes a properly lowercased `normalized_file_list`
    // so resolvers that key off it directly behave correctly.
    let normalized_paths: Vec<String> = all_paths.iter().map(|p| p.to_lowercase()).collect();
    let suffix_index = SuffixIndex::build(&all_paths, &all_paths);
    let configs = build_import_configs(repo_path);

    let ctx = ResolveCtx {
        all_file_paths: &all_set,
        all_file_list: &all_paths,
        normalized_file_list: &normalized_paths,
        suffix_index: &suffix_index,
        configs: &configs,
    };

    let mut import_map: ImportMap = HashMap::new();
    let mut named_import_map: NamedImportMap = HashMap::new();
    let mut re_export_map: ReExportMap = HashMap::new();
    let mut package_map: PackageMap = HashMap::new();
    let mut module_alias_map: ModuleAliasMap = HashMap::new();

    add_local_export_aliases(files, &mut re_export_map);

    // Process each extracted import
    for imp in &extracted.imports {
        let lang = SupportedLanguage::from_filename(&imp.file_path);
        let lang = match lang {
            Some(l) => l,
            None => continue,
        };
        let provider = get_provider(lang);

        // Resolve the import path
        let result = provider.resolve_import(&imp.raw_import_path, &imp.file_path, &ctx);

        match result {
            ImportResult::Files(resolved_files) => {
                for resolved in &resolved_files {
                    import_map
                        .entry(imp.file_path.clone())
                        .or_default()
                        .insert(resolved.clone());

                    // Create IMPORTS edge
                    let source_id = generate_id("File", &imp.file_path);
                    let target_id = generate_id("File", resolved);
                    let edge_id = format!("imports_{}_{}", source_id, target_id);
                    graph.add_relationship(GraphRelationship {
                        id: edge_id,
                        source_id,
                        target_id,
                        rel_type: RelationshipType::Imports,
                        confidence: 0.9,
                        reason: "resolved".to_string(),
                        step: None,
                    });
                }

                // Extract named bindings
                let binding_text = imp.binding_text.as_deref().unwrap_or(&imp.raw_import_path);
                let is_re_export = is_re_export_statement(binding_text);
                if let Some(bindings) = provider.extract_named_bindings(binding_text) {
                    for binding in bindings {
                        if let Some(first_file) = resolved_files.first() {
                            let local_name = binding.local.clone();
                            let exported_name = binding.exported.clone();
                            if binding.is_module_alias {
                                if binding.is_type_only {
                                    named_import_map
                                        .entry(imp.file_path.clone())
                                        .or_default()
                                        .insert(
                                            local_name.clone(),
                                            NamedImportBinding {
                                                source_path: first_file.clone(),
                                                exported_name: exported_name.clone(),
                                                is_type_only: true,
                                            },
                                        );

                                    if is_re_export {
                                        re_export_map
                                            .entry(imp.file_path.clone())
                                            .or_default()
                                            .push(ReExportBinding {
                                                source_path: first_file.clone(),
                                                local_name: Some(local_name),
                                                exported_name: Some(exported_name),
                                                is_type_only: true,
                                            });
                                    }
                                } else {
                                    module_alias_map
                                        .entry(imp.file_path.clone())
                                        .or_default()
                                        .insert(local_name, first_file.clone());
                                }
                                continue;
                            }

                            named_import_map
                                .entry(imp.file_path.clone())
                                .or_default()
                                .insert(
                                    local_name.clone(),
                                    NamedImportBinding {
                                        source_path: first_file.clone(),
                                        exported_name: exported_name.clone(),
                                        is_type_only: binding.is_type_only,
                                    },
                                );

                            if is_re_export {
                                re_export_map
                                    .entry(imp.file_path.clone())
                                    .or_default()
                                    .push(ReExportBinding {
                                        source_path: first_file.clone(),
                                        local_name: Some(local_name),
                                        exported_name: Some(exported_name),
                                        is_type_only: binding.is_type_only
                                            || is_type_only_re_export_statement(binding_text),
                                    });
                            }
                        }
                    }
                } else if is_re_export && is_wildcard_re_export_statement(binding_text) {
                    for resolved in &resolved_files {
                        re_export_map
                            .entry(imp.file_path.clone())
                            .or_default()
                            .push(ReExportBinding {
                                source_path: resolved.clone(),
                                local_name: None,
                                exported_name: None,
                                is_type_only: is_type_only_re_export_statement(binding_text),
                            });
                    }
                }
            }
            ImportResult::Package {
                files: pkg_files,
                dir_suffix,
            } => {
                for f in &pkg_files {
                    import_map
                        .entry(imp.file_path.clone())
                        .or_default()
                        .insert(f.clone());
                }
                package_map
                    .entry(imp.file_path.clone())
                    .or_default()
                    .insert(dir_suffix);
            }
            ImportResult::Unresolved => {
                // Skip unresolved imports
            }
        }
    }

    propagate_script_re_exported_module_aliases(
        &named_import_map,
        &re_export_map,
        &mut module_alias_map,
    );

    Ok((
        import_map,
        named_import_map,
        re_export_map,
        package_map,
        module_alias_map,
    ))
}

fn is_re_export_statement(text: &str) -> bool {
    let trimmed = text.trim_start();
    trimmed.starts_with("export ") && trimmed.contains(" from ")
}

fn is_wildcard_re_export_statement(text: &str) -> bool {
    let trimmed = text.trim_start();
    trimmed.starts_with("export *") || trimmed.starts_with("export type *")
}

fn is_type_only_re_export_statement(text: &str) -> bool {
    text.trim_start().starts_with("export type ")
}

fn propagate_script_re_exported_module_aliases(
    named_import_map: &NamedImportMap,
    re_export_map: &ReExportMap,
    module_alias_map: &mut ModuleAliasMap,
) {
    for _ in 0..16 {
        let snapshot = module_alias_map.clone();
        let mut additions = Vec::new();

        for (file_path, bindings) in named_import_map {
            if !is_script_language_file(file_path) {
                continue;
            }

            for (local_name, binding) in bindings {
                if let Some(target_file) = snapshot
                    .get(&binding.source_path)
                    .and_then(|aliases| aliases.get(&binding.exported_name))
                {
                    additions.push((file_path.clone(), local_name.clone(), target_file.clone()));
                }
            }
        }

        for (file_path, re_exports) in re_export_map {
            if !is_script_language_file(file_path) {
                continue;
            }

            for re_export in re_exports {
                let (Some(local_name), Some(exported_name)) =
                    (&re_export.local_name, &re_export.exported_name)
                else {
                    continue;
                };

                if let Some(target_file) = snapshot
                    .get(&re_export.source_path)
                    .and_then(|aliases| aliases.get(exported_name))
                {
                    additions.push((file_path.clone(), local_name.clone(), target_file.clone()));
                }
            }
        }

        let mut changed = false;
        for (file_path, alias, target_file) in additions {
            let aliases = module_alias_map.entry(file_path).or_default();
            if !aliases.contains_key(&alias) {
                aliases.insert(alias, target_file);
                changed = true;
            }
        }

        if !changed {
            break;
        }
    }
}

fn is_script_language_file(file_path: &str) -> bool {
    matches!(
        SupportedLanguage::from_filename(file_path),
        Some(SupportedLanguage::JavaScript | SupportedLanguage::TypeScript)
    )
}

fn add_local_export_aliases(files: &[FileEntry], re_export_map: &mut ReExportMap) {
    for file in files {
        let Some(lang @ (SupportedLanguage::JavaScript | SupportedLanguage::TypeScript)) =
            SupportedLanguage::from_filename(&file.path)
        else {
            continue;
        };

        let ts_language = crate::grammar::get_language_for_file(lang, &file.path);
        let mut parser = Parser::new();
        if parser.set_language(&ts_language).is_err() {
            continue;
        }
        let Some(tree) = parser.parse(&file.content, None) else {
            continue;
        };

        let mut local_object_aliases = HashMap::new();
        collect_local_object_member_aliases(
            tree.root_node(),
            file.content.as_bytes(),
            &mut local_object_aliases,
        );

        collect_local_export_aliases(
            tree.root_node(),
            file.content.as_bytes(),
            &file.path,
            re_export_map,
            &local_object_aliases,
        );
    }
}

fn collect_local_object_member_aliases(
    node: tree_sitter::Node,
    content: &[u8],
    aliases: &mut HashMap<String, Vec<(String, String)>>,
) {
    if node.kind() == "variable_declarator" && is_top_level_variable_declarator(node) {
        if let (Some(name_node), Some(value_node)) = (
            node.child_by_field_name("name"),
            node.child_by_field_name("value"),
        ) {
            if name_node.kind() == "identifier" && value_node.kind() == "object" {
                if let (Ok(name), Ok(value_text)) =
                    (name_node.utf8_text(content), value_node.utf8_text(content))
                {
                    if let Some(inner) = extract_balanced_brace_content(value_text.trim()) {
                        let members: Vec<(String, String)> = split_ts_top_level_commas(inner)
                            .into_iter()
                            .filter_map(extract_default_object_member_alias)
                            .collect();
                        if !members.is_empty() {
                            aliases.insert(name.to_string(), members);
                        }
                    }
                }
            }
        }
    }

    for idx in 0..node.child_count() {
        if let Some(child) = node.child(idx) {
            collect_local_object_member_aliases(child, content, aliases);
        }
    }
}

fn is_top_level_variable_declarator(node: tree_sitter::Node) -> bool {
    let mut current = node.parent();
    while let Some(parent) = current {
        match parent.kind() {
            "program" => return true,
            "statement_block" | "class_body" => return false,
            _ => current = parent.parent(),
        }
    }
    false
}

fn collect_local_export_aliases(
    node: tree_sitter::Node,
    content: &[u8],
    file_path: &str,
    re_export_map: &mut ReExportMap,
    local_object_aliases: &HashMap<String, Vec<(String, String)>>,
) {
    if node.kind() == "export_statement" {
        if let Ok(text) = node.utf8_text(content) {
            let export_aliases = extract_local_export_aliases(text);
            for (public_name, local_name) in &export_aliases {
                re_export_map
                    .entry(file_path.to_string())
                    .or_default()
                    .push(ReExportBinding {
                        source_path: file_path.to_string(),
                        local_name: Some(public_name.clone()),
                        exported_name: Some(local_name.clone()),
                        is_type_only: false,
                    });
            }

            for (public_name, local_name) in export_aliases {
                if let Some(members) = local_object_aliases.get(&local_name) {
                    for (member_public_name, member_local_name) in members {
                        re_export_map
                            .entry(file_path.to_string())
                            .or_default()
                            .push(ReExportBinding {
                                source_path: file_path.to_string(),
                                local_name: Some(format!("{public_name}.{member_public_name}")),
                                exported_name: Some(member_local_name.clone()),
                                is_type_only: false,
                            });
                    }
                }
            }
        }
    }

    for idx in 0..node.child_count() {
        if let Some(child) = node.child(idx) {
            collect_local_export_aliases(
                child,
                content,
                file_path,
                re_export_map,
                local_object_aliases,
            );
        }
    }
}

fn extract_local_export_aliases(text: &str) -> Vec<(String, String)> {
    let trimmed = text.trim();
    let mut aliases = Vec::new();

    if let Some(local_name) = extract_default_export_local_name(trimmed) {
        aliases.push(("default".to_string(), local_name));
    }

    for (public_name, local_name) in extract_default_object_export_aliases(trimmed) {
        aliases.push((format!("default.{public_name}"), local_name));
    }

    for (object_name, public_name, local_name) in extract_named_object_export_aliases(trimmed) {
        aliases.push((format!("{object_name}.{public_name}"), local_name));
    }

    if trimmed.starts_with("export {") && !trimmed.contains(" from ") {
        if let Some(bindings) = code_explorer_lang::named_bindings::typescript::extract(trimmed) {
            for binding in bindings {
                aliases.push((binding.local, binding.exported));
            }
        }
    }

    aliases
}

fn extract_named_object_export_aliases(text: &str) -> Vec<(String, String, String)> {
    let Some(rest) = text.strip_prefix("export") else {
        return Vec::new();
    };
    let rest = rest.trim_start();
    if rest.starts_with("default") || rest.starts_with("type ") {
        return Vec::new();
    }
    let rest = rest
        .strip_prefix("const ")
        .or_else(|| rest.strip_prefix("let "))
        .or_else(|| rest.strip_prefix("var "))
        .map(str::trim_start);
    let Some(rest) = rest else {
        return Vec::new();
    };

    let Some(object_name) = take_leading_identifier(rest) else {
        return Vec::new();
    };
    let after_name = rest[object_name.len()..].trim_start();
    let Some(equals_pos) = find_ts_top_level_equals(after_name) else {
        return Vec::new();
    };
    let after_equals = after_name[equals_pos + 1..].trim_start();
    if !after_equals.starts_with('{') {
        return Vec::new();
    }

    let Some(inner) = extract_balanced_brace_content(after_equals) else {
        return Vec::new();
    };

    split_ts_top_level_commas(inner)
        .into_iter()
        .filter_map(extract_default_object_member_alias)
        .map(|(public_name, local_name)| (object_name.clone(), public_name, local_name))
        .collect()
}

fn extract_default_object_export_aliases(text: &str) -> Vec<(String, String)> {
    let Some(rest) = text.strip_prefix("export") else {
        return Vec::new();
    };
    let Some(rest) = rest.trim_start().strip_prefix("default") else {
        return Vec::new();
    };
    let rest = strip_leading_ts_modifiers(rest);
    if !rest.starts_with('{') {
        return Vec::new();
    }

    let Some(inner) = extract_balanced_brace_content(rest) else {
        return Vec::new();
    };

    split_ts_top_level_commas(inner)
        .into_iter()
        .filter_map(extract_default_object_member_alias)
        .collect()
}

fn extract_balanced_brace_content(text: &str) -> Option<&str> {
    if !text.starts_with('{') {
        return None;
    }

    let mut depth = 0i32;
    let mut in_string: Option<char> = None;
    let mut escaped = false;

    for (idx, ch) in text.char_indices() {
        if let Some(quote) = in_string {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == quote {
                in_string = None;
            }
            continue;
        }

        match ch {
            '\'' | '"' | '`' => in_string = Some(ch),
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return text.get(1..idx);
                }
            }
            _ => {}
        }
    }

    None
}

fn split_ts_top_level_commas(text: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut start = 0usize;
    let mut depth = 0i32;
    let mut in_string: Option<char> = None;
    let mut escaped = false;

    for (idx, ch) in text.char_indices() {
        if let Some(quote) = in_string {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == quote {
                in_string = None;
            }
            continue;
        }

        match ch {
            '\'' | '"' | '`' => in_string = Some(ch),
            '{' | '(' | '[' | '<' => depth += 1,
            '}' | ')' | ']' | '>' => depth = (depth - 1).max(0),
            ',' if depth == 0 => {
                parts.push(&text[start..idx]);
                start = idx + 1;
            }
            _ => {}
        }
    }

    if start < text.len() {
        parts.push(&text[start..]);
    }

    parts
}

fn extract_default_object_member_alias(part: &str) -> Option<(String, String)> {
    let part = part.trim();
    if part.is_empty() || part.starts_with("...") {
        return None;
    }

    if let Some(colon_pos) = find_ts_top_level_colon(part) {
        let public_name = normalize_ts_object_property_key(part[..colon_pos].trim())?;
        let value = part[colon_pos + 1..].trim();
        if is_default_object_function_value(value) {
            return Some((public_name.clone(), public_name));
        }

        let local_name = take_leading_identifier(value)?;
        let trailing = value[local_name.len()..].trim();
        if trailing.is_empty() {
            return Some((public_name, local_name));
        }
        return None;
    }

    let method_part = part
        .strip_prefix("async ")
        .map(str::trim_start)
        .unwrap_or(part)
        .trim_start_matches('*')
        .trim_start();

    if let Some(paren_pos) = method_part.find('(') {
        let public_name = normalize_ts_object_property_key(method_part[..paren_pos].trim())?;
        return Some((public_name.clone(), public_name));
    }

    let local_name = take_leading_identifier(part)?;
    let trailing = part[local_name.len()..].trim();
    if trailing.is_empty() {
        Some((local_name.clone(), local_name))
    } else {
        None
    }
}

fn find_ts_top_level_colon(text: &str) -> Option<usize> {
    let mut depth = 0i32;
    let mut in_string: Option<char> = None;
    let mut escaped = false;

    for (idx, ch) in text.char_indices() {
        if let Some(quote) = in_string {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == quote {
                in_string = None;
            }
            continue;
        }

        match ch {
            '\'' | '"' | '`' => in_string = Some(ch),
            '{' | '(' | '[' | '<' => depth += 1,
            '}' | ')' | ']' | '>' => depth = (depth - 1).max(0),
            ':' if depth == 0 => return Some(idx),
            _ => {}
        }
    }

    None
}

fn find_ts_top_level_equals(text: &str) -> Option<usize> {
    let mut depth = 0i32;
    let mut in_string: Option<char> = None;
    let mut escaped = false;

    for (idx, ch) in text.char_indices() {
        if let Some(quote) = in_string {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == quote {
                in_string = None;
            }
            continue;
        }

        match ch {
            '\'' | '"' | '`' => in_string = Some(ch),
            '{' | '(' | '[' | '<' => depth += 1,
            '}' | ')' | ']' | '>' => depth = (depth - 1).max(0),
            '=' if depth == 0 && !text[idx + ch.len_utf8()..].starts_with('>') => {
                return Some(idx);
            }
            _ => {}
        }
    }

    None
}

fn normalize_ts_object_property_key(key: &str) -> Option<String> {
    let key = key.trim();
    let unquoted = key
        .strip_prefix('"')
        .and_then(|rest| rest.strip_suffix('"'))
        .or_else(|| {
            key.strip_prefix('\'')
                .and_then(|rest| rest.strip_suffix('\''))
        })
        .unwrap_or(key);
    let name = take_leading_identifier(unquoted)?;
    if name.len() == unquoted.trim().len() {
        Some(name)
    } else {
        None
    }
}

fn is_default_object_function_value(value: &str) -> bool {
    let value = strip_leading_ts_modifiers(value);
    value.starts_with("function") || value.contains("=>")
}

fn extract_default_export_local_name(text: &str) -> Option<String> {
    let rest = text.strip_prefix("export")?.trim_start();
    let rest = rest.strip_prefix("default")?.trim_start();
    if rest.starts_with('{') {
        return None;
    }

    let rest = strip_leading_ts_modifiers(rest);
    if let Some(after_function) = rest.strip_prefix("function") {
        return take_leading_identifier(after_function);
    }
    if let Some(after_class) = rest.strip_prefix("class") {
        return take_leading_identifier(after_class);
    }

    let local_name = take_leading_identifier(rest)?;
    let trailing = rest[local_name.len()..].trim();
    if trailing.is_empty() || trailing == ";" {
        Some(local_name)
    } else {
        None
    }
}

fn strip_leading_ts_modifiers(mut text: &str) -> &str {
    loop {
        let trimmed = text.trim_start();
        if let Some(rest) = trimmed.strip_prefix("async ") {
            text = rest;
        } else if let Some(rest) = trimmed.strip_prefix("abstract ") {
            text = rest;
        } else {
            return trimmed;
        }
    }
}

fn take_leading_identifier(text: &str) -> Option<String> {
    let text = text.trim_start().trim_start_matches('*').trim_start();
    let mut end = 0;
    let mut chars = text.char_indices();
    let (_, first) = chars.next()?;
    if !is_ts_identifier_start(first) {
        return None;
    }
    end += first.len_utf8();
    for (idx, ch) in chars {
        if !is_ts_identifier_continue(ch) {
            break;
        }
        end = idx + ch.len_utf8();
    }
    Some(text[..end].to_string())
}

fn is_ts_identifier_start(ch: char) -> bool {
    ch == '_' || ch == '$' || ch.is_ascii_alphabetic()
}

fn is_ts_identifier_continue(ch: char) -> bool {
    is_ts_identifier_start(ch) || ch.is_ascii_digit()
}

fn build_import_configs(repo_path: &Path) -> ImportConfigs {
    let mut configs = ImportConfigs::default();
    let mut ts_paths: HashMap<String, Vec<String>> = HashMap::new();

    for config_path in discover_ts_config_paths(repo_path) {
        let Some(value) = parse_json_like_file(&config_path) else {
            continue;
        };
        let Some(compiler_options) = value.get("compilerOptions").and_then(Value::as_object) else {
            continue;
        };

        let config_dir = config_path.parent().unwrap_or(repo_path);
        let base_url = compiler_options
            .get("baseUrl")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty());

        if configs.ts_base_url.is_none() {
            configs.ts_base_url = base_url
                .and_then(|base| normalize_config_path(repo_path, config_dir, None, base))
                .filter(|base| !base.is_empty() && base != ".");
        }

        let Some(paths) = compiler_options.get("paths").and_then(Value::as_object) else {
            continue;
        };

        for (pattern, targets) in paths {
            let Some(targets) = targets.as_array() else {
                continue;
            };
            let normalized_targets: Vec<String> = targets
                .iter()
                .filter_map(Value::as_str)
                .filter_map(|target| normalize_config_path(repo_path, config_dir, base_url, target))
                .collect();
            if !normalized_targets.is_empty() {
                ts_paths
                    .entry(pattern.clone())
                    .or_insert(normalized_targets);
            }
        }
    }

    if !ts_paths.is_empty() {
        configs.ts_paths = Some(ts_paths);
    }

    configs
}

fn discover_ts_config_paths(repo_path: &Path) -> Vec<PathBuf> {
    let mut configs = Vec::new();
    let mut current = Some(repo_path);
    let mut depth = 0;

    while let Some(dir) = current {
        if depth > 3 {
            break;
        }

        if let Ok(entries) = std::fs::read_dir(dir) {
            let mut dir_configs: Vec<PathBuf> = entries
                .filter_map(Result::ok)
                .map(|entry| entry.path())
                .filter(|path| {
                    path.file_name()
                        .and_then(|name| name.to_str())
                        .is_some_and(is_ts_config_name)
                })
                .collect();
            dir_configs.sort();
            configs.extend(dir_configs);
        }

        current = dir.parent();
        depth += 1;
    }

    configs
}

fn is_ts_config_name(name: &str) -> bool {
    (name == "tsconfig.json"
        || name == "jsconfig.json"
        || (name.starts_with("tsconfig.") && name.ends_with(".json"))
        || (name.starts_with("jsconfig.") && name.ends_with(".json")))
        && !name.ends_with(".tmp.json")
}

fn parse_json_like_file(path: &Path) -> Option<Value> {
    let content = std::fs::read_to_string(path).ok()?;
    let stripped = strip_json_comments(&content);
    let stripped = strip_trailing_commas(&stripped);
    serde_json::from_str(&stripped).ok()
}

fn strip_json_comments(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    let mut in_string = false;
    let mut escaped = false;

    while let Some(ch) = chars.next() {
        if in_string {
            out.push(ch);
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                in_string = false;
            }
            continue;
        }

        if ch == '"' {
            in_string = true;
            out.push(ch);
            continue;
        }

        if ch == '/' {
            match chars.peek().copied() {
                Some('/') => {
                    chars.next();
                    for next in chars.by_ref() {
                        if next == '\n' {
                            out.push('\n');
                            break;
                        }
                    }
                    continue;
                }
                Some('*') => {
                    chars.next();
                    let mut prev = '\0';
                    for next in chars.by_ref() {
                        if next == '\n' {
                            out.push('\n');
                        }
                        if prev == '*' && next == '/' {
                            break;
                        }
                        prev = next;
                    }
                    continue;
                }
                _ => {}
            }
        }

        out.push(ch);
    }

    out
}

fn strip_trailing_commas(input: &str) -> String {
    let chars: Vec<char> = input.chars().collect();
    let mut out = String::with_capacity(input.len());
    let mut i = 0;
    let mut in_string = false;
    let mut escaped = false;

    while i < chars.len() {
        let ch = chars[i];
        if in_string {
            out.push(ch);
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                in_string = false;
            }
            i += 1;
            continue;
        }

        if ch == '"' {
            in_string = true;
            out.push(ch);
            i += 1;
            continue;
        }

        if ch == ',' {
            let mut j = i + 1;
            while j < chars.len() && chars[j].is_whitespace() {
                j += 1;
            }
            if j < chars.len() && matches!(chars[j], '}' | ']') {
                i += 1;
                continue;
            }
        }

        out.push(ch);
        i += 1;
    }

    out
}

fn normalize_config_path(
    repo_path: &Path,
    config_dir: &Path,
    base_url: Option<&str>,
    target: &str,
) -> Option<String> {
    let target = target.trim();
    if target.is_empty() {
        return None;
    }

    let base = base_url.filter(|base| !base.is_empty()).unwrap_or(".");
    let abs = normalize_pathbuf(config_dir.join(base).join(target));
    let repo = normalize_pathbuf(repo_path);
    let rel = abs.strip_prefix(&repo).ok().unwrap_or(&abs);
    let normalized = rel.to_string_lossy().replace('\\', "/");
    Some(normalized.trim_start_matches("./").to_string())
}

fn normalize_pathbuf(path: impl AsRef<Path>) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.as_ref().components() {
        use std::path::Component;
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            other => normalized.push(other.as_os_str()),
        }
    }
    normalized
}

/// Build a reverse import map (imported_file -> set of files that import it).
#[allow(dead_code)]
pub fn build_reverse_import_map(import_map: &ImportMap) -> HashMap<String, HashSet<String>> {
    let mut reverse: HashMap<String, HashSet<String>> = HashMap::new();
    for (file, imports) in import_map {
        for imported in imports {
            reverse
                .entry(imported.clone())
                .or_default()
                .insert(file.clone());
        }
    }
    reverse
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_export_aliases_detect_default_function() {
        assert_eq!(
            extract_local_export_aliases("export default function runTask() { return 1; }"),
            vec![("default".to_string(), "runTask".to_string())]
        );
    }

    #[test]
    fn local_export_aliases_detect_default_class() {
        assert_eq!(
            extract_local_export_aliases("export default abstract class Service {}"),
            vec![("default".to_string(), "Service".to_string())]
        );
    }

    #[test]
    fn local_export_aliases_detect_default_identifier() {
        assert_eq!(
            extract_local_export_aliases("export default runTask;"),
            vec![("default".to_string(), "runTask".to_string())]
        );
    }

    #[test]
    fn local_export_aliases_ignore_default_expression() {
        assert!(extract_local_export_aliases("export default memo(Component);").is_empty());
    }

    #[test]
    fn local_export_aliases_detect_default_object_members() {
        assert_eq!(
            extract_local_export_aliases(
                r#"export default {
  runTask,
  task: runTask,
  inline: () => 1,
  "quoted": runTask,
  config: { nested: true },
  ...extra
};"#
            ),
            vec![
                ("default.runTask".to_string(), "runTask".to_string()),
                ("default.task".to_string(), "runTask".to_string()),
                ("default.inline".to_string(), "inline".to_string()),
                ("default.quoted".to_string(), "runTask".to_string())
            ]
        );
    }

    #[test]
    fn local_export_aliases_detect_named_object_members() {
        assert_eq!(
            extract_local_export_aliases(
                r#"export const logger = {
  debug: (message: string) => getLogger().debug(message),
  warn,
  child(source: string) { return getLogger().child(source); },
  config: { nested: true },
};"#
            ),
            vec![
                ("logger.debug".to_string(), "debug".to_string()),
                ("logger.warn".to_string(), "warn".to_string()),
                ("logger.child".to_string(), "child".to_string())
            ]
        );
    }

    #[test]
    fn local_export_aliases_detect_annotated_named_object_members() {
        assert_eq!(
            extract_local_export_aliases(
                r#"export const logger: Record<string, (message: string) => void> = {
  debug: (message: string) => getLogger().debug(message),
  warn,
};"#
            ),
            vec![
                ("logger.debug".to_string(), "debug".to_string()),
                ("logger.warn".to_string(), "warn".to_string())
            ]
        );
    }

    #[test]
    fn local_export_aliases_detect_named_alias() {
        assert_eq!(
            extract_local_export_aliases("export { runTask as task, helper };"),
            vec![
                ("task".to_string(), "runTask".to_string()),
                ("helper".to_string(), "helper".to_string())
            ]
        );
    }
}

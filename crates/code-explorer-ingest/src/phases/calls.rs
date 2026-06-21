use std::collections::{HashMap, HashSet};

use code_explorer_core::config::languages::SupportedLanguage;
use code_explorer_core::graph::types::*;
use code_explorer_core::graph::KnowledgeGraph;
use code_explorer_core::id::generate_id;
use code_explorer_core::resolution::context::ResolutionContext;
use code_explorer_core::resolution::types::*;
use code_explorer_core::symbol::SymbolTable;
use once_cell::sync::Lazy;
use regex::Regex;
use std::sync::Arc;
use tree_sitter::{Node, Parser};

use crate::phases::parsing::{CallForm, ExtractedCall, ExtractedData};
use crate::type_env::TypeEnvironment;
use crate::IngestError;

// Pattern 1: Field declarations like "CourriersService courriersService = null;"
static FIELD_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"(?:private|protected|public|internal)?\s*([A-Z]\w+(?:Service|Repository|Manager|Helper|Provider|Client|Handler))\s+(\w+)\s*[=;]"
    ).expect("FIELD_RE must compile")
});

// Pattern 2: Constructor DI params like "public Foo(ICourriersService courriersService)"
static CLASS_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?:public|internal|private)?\s*(?:partial\s+)?class\s+(\w+)")
        .expect("CLASS_RE must compile")
});

// Pattern 3: C# 'using' statement: using (var x = new CourriersService(...))
static USING_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"using\s*\(\s*(?:var|[A-Z]\w+)\s+(\w+)\s*=\s*new\s+([A-Z]\w+)\s*\(")
        .expect("USING_RE must compile")
});

// Pattern 4: Local variable instantiation: var x = new SomeService(...)
static LOCAL_NEW_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?:var|[A-Z]\w+)\s+(\w+)\s*=\s*new\s+([A-Z]\w+)\s*[\(\{]")
        .expect("LOCAL_NEW_RE must compile")
});

static TS_EXPLICIT_TYPE_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"\b(?:const|let|var)\s+([A-Za-z_$][\w$]*)\s*:\s*([A-Za-z_$][\w$]*(?:\.[A-Za-z_$][\w$]*)?)",
    )
    .expect("TS_EXPLICIT_TYPE_RE must compile")
});

static TS_CONSTRUCTOR_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"\b(?:const|let|var)\s+([A-Za-z_$][\w$]*)\s*=\s*new\s+([A-Za-z_$][\w$]*(?:\.[A-Za-z_$][\w$]*)?)",
    )
    .expect("TS_CONSTRUCTOR_RE must compile")
});

static TS_FIELD_TYPE_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"(?:^|[;\n\r{])\s*(?:(?:public|private|protected|readonly|static|declare|abstract|override)\s+)*([A-Za-z_$][\w$]*)\??\s*:\s*([A-Za-z_$][\w$]*(?:\.[A-Za-z_$][\w$]*)?)",
    )
    .expect("TS_FIELD_TYPE_RE must compile")
});

static TS_CONSTRUCTOR_PARAMS_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?s)\bconstructor\s*\((.*?)\)").expect("TS_CONSTRUCTOR_PARAMS_RE must compile")
});

static TS_EXTERNAL_FLUENT_ASSIGN_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"\b(?:const|let|var)\s+([A-Za-z_$][\w$]*)\s*=\s*(?:await\s+)?([A-Za-z_$][\w$]*(?:\.[A-Za-z_$][\w$]*)?)\s*\.",
    )
    .expect("TS_EXTERNAL_FLUENT_ASSIGN_RE must compile")
});

static TS_PARAM_TYPE_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"(?:(?:public|private|protected|readonly)\s+)*([A-Za-z_$][\w$]*)\??\s*:\s*([A-Za-z_$][\w$]*(?:\.[A-Za-z_$][\w$]*)?)",
    )
    .expect("TS_PARAM_TYPE_RE must compile")
});

static TS_NODE_FS_NAMESPACE_IMPORT_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"\bimport\s+\*\s+as\s+([A-Za-z_$][\w$]*)\s+from\s+['"](?:node:)?fs['"]"#)
        .expect("TS_NODE_FS_NAMESPACE_IMPORT_RE must compile")
});

static TS_NODE_FS_DEFAULT_IMPORT_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"\bimport\s+([A-Za-z_$][\w$]*)\s*(?:,\s*[^;]*?)?\s+from\s+['"](?:node:)?fs['"]"#)
        .expect("TS_NODE_FS_DEFAULT_IMPORT_RE must compile")
});

static TS_NODE_FS_PROMISES_IMPORT_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r#"\bimport\s*\{[^}]*\bpromises\s+as\s+([A-Za-z_$][\w$]*)[^}]*\}\s+from\s+['"](?:node:)?fs['"]"#,
    )
    .expect("TS_NODE_FS_PROMISES_IMPORT_RE must compile")
});

static TS_NODE_FS_REQUIRE_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"\b(?:const|let|var)\s+([A-Za-z_$][\w$]*)\s*=\s*require\s*\(\s*['"](?:node:)?fs['"]\s*\)"#)
        .expect("TS_NODE_FS_REQUIRE_RE must compile")
});

static TS_NODE_FS_STAT_ASSIGN_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"\b(?:const|let|var)\s+([A-Za-z_$][\w$]*)\s*=\s*(?:await\s+)?([A-Za-z_$][\w$]*)(?:\.promises)?\.(?:stat|lstat|statSync|lstatSync)\s*\(",
    )
    .expect("TS_NODE_FS_STAT_ASSIGN_RE must compile")
});

static TS_NODE_FS_READDIR_ASSIGN_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"\b(?:const|let|var)\s+([A-Za-z_$][\w$]*)\s*=\s*(?:await\s+)?([A-Za-z_$][\w$]*)(?:\.promises)?\.(?:readdir|readdirSync)\s*\(",
    )
    .expect("TS_NODE_FS_READDIR_ASSIGN_RE must compile")
});

static TS_ARRAY_METHOD_ASSIGN_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"\b(?:const|let|var)\s+([A-Za-z_$][\w$]*)\s*=\s*([A-Za-z_$][\w$]*)\s*\.\s*(?:sort|filter|slice|toSorted)\s*\(",
    )
    .expect("TS_ARRAY_METHOD_ASSIGN_RE must compile")
});

static TS_FOR_OF_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"\bfor\s*\(\s*(?:const|let|var)\s+([A-Za-z_$][\w$]*)\s+of\s+([A-Za-z_$][\w$]*)\s*\)",
    )
    .expect("TS_FOR_OF_RE must compile")
});

static TS_FOR_OF_LITERAL_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"\bfor\s*\(\s*(?:const|let|var)\s+([A-Za-z_$][\w$]*)\s+of\s*\[")
        .expect("TS_FOR_OF_LITERAL_RE must compile")
});

static TS_ARRAY_INDEX_ASSIGN_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"\b(?:const|let|var)\s+([A-Za-z_$][\w$]*)\s*=\s*([A-Za-z_$][\w$]*)\s*\[[^\]]+\]")
        .expect("TS_ARRAY_INDEX_ASSIGN_RE must compile")
});

static TS_ARRAY_CALLBACK_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"\b([A-Za-z_$][\w$]*)\s*\.\s*(?:sort|filter|find|some|every|forEach)\s*\(\s*(?:async\s*)?(?:\(\s*)?([A-Za-z_$][\w$]*)(?:\s*,\s*([A-Za-z_$][\w$]*))?",
    )
    .expect("TS_ARRAY_CALLBACK_RE must compile")
});

static TS_ARRAY_ASSIGN_CALLBACK_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"\b(?:const|let|var)\s+([A-Za-z_$][\w$]*)\s*=\s*([A-Za-z_$][\w$]*)\s*\.\s*(?:sort|filter|find|some|every|forEach)\s*\(\s*(?:async\s*)?(?:\(\s*)?([A-Za-z_$][\w$]*)(?:\s*,\s*([A-Za-z_$][\w$]*))?",
    )
    .expect("TS_ARRAY_ASSIGN_CALLBACK_RE must compile")
});

static TS_WITH_FILE_TYPES_TRUE_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"\bwithFileTypes\s*:\s*true\b").expect("TS_WITH_FILE_TYPES_TRUE_RE must compile")
});

static TS_FETCH_RESPONSE_ASSIGN_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"\b(?:const|let|var)\s+([A-Za-z_$][\w$]*)\s*=\s*(?:await\s+)?fetch\s*\(")
        .expect("TS_FETCH_RESPONSE_ASSIGN_RE must compile")
});

static TS_NODE_CHILD_PROCESS_ASSIGN_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"\b(?:const|let|var)\s+([A-Za-z_$][\w$]*)\s*=\s*(?:spawn|exec|execFile|fork)\s*\(")
        .expect("TS_NODE_CHILD_PROCESS_ASSIGN_RE must compile")
});

static TS_EVENT_EMITTER_ASSIGN_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"\b(?:const|let|var)\s+([A-Za-z_$][\w$]*)\s*=\s*new\s+(?:EventEmitter|Worker|WebSocket)\s*\(")
        .expect("TS_EVENT_EMITTER_ASSIGN_RE must compile")
});

static TS_BOUND_METHOD_ALIAS_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"\b(?:const|let|var)\s+([A-Za-z_$][\w$]*)\s*=\s*([A-Za-z_$][\w$]*(?:\.[A-Za-z_$][\w$]*)*)\s*\.\s*([A-Za-z_$][\w$]*)\??\s*\.\s*bind\s*\(",
    )
    .expect("TS_BOUND_METHOD_ALIAS_RE must compile")
});

static TS_EXPRESS_IMPORT_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"(?m)\bfrom\s+['"]express['"]|\brequire\s*\(\s*['"]express['"]\s*\)"#)
        .expect("TS_EXPRESS_IMPORT_RE must compile")
});

static TS_EXPRESS_ROUTER_CALLBACK_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"\b[A-Za-z_$][\w$]*\s*\.\s*(?:all|delete|get|patch|post|put|use)\s*\([^,]+,\s*(?:async\s*)?\(?\s*([A-Za-z_$][\w$]*)\s*,\s*([A-Za-z_$][\w$]*)",
    )
    .expect("TS_EXPRESS_ROUTER_CALLBACK_RE must compile")
});

const TS_EXTERNAL_NODE_FS_STATS: &str = "__external.node.fs.Stats";
const TS_EXTERNAL_NODE_FS_DIRENT: &str = "__external.node.fs.Dirent";
const TS_EXTERNAL_NODE_FS_DIRENT_ARRAY: &str = "__external.node.fs.Dirent[]";
const TS_EXTERNAL_WEB_RESPONSE: &str = "__external.web.Response";
const TS_EXTERNAL_EXPRESS_RESPONSE: &str = "__external.express.Response";
const TS_EXTERNAL_NODE_CHILD_PROCESS: &str = "__external.node.child_process.ChildProcess";
const TS_EXTERNAL_EVENT_EMITTER: &str = "__external.node.events.EventEmitter";

#[derive(Debug, Clone)]
struct TsScopeRange {
    start_byte: usize,
    end_byte: usize,
    source_id: String,
}

#[derive(Debug, Clone)]
struct TsBoundMethodAlias {
    receiver_name: String,
    member_name: String,
}

/// Build a map of (file_path, field_name) → interface_type from constructor parameters.
/// Scans .cs files for class constructors with DI-injected parameters.
fn build_field_type_map(
    file_entries: &[crate::phases::structure::FileEntry],
) -> HashMap<(String, String), String> {
    let mut map = HashMap::new();
    let mut class_count = 0u32;

    for file in file_entries {
        if !file.path.ends_with(".cs") {
            continue;
        }
        let fp = file.path.clone();

        // Extract field declarations (legacy ASP.NET pattern without DI)
        for cap in FIELD_RE.captures_iter(&file.content) {
            if let (Some(type_name), Some(field_name)) = (cap.get(1), cap.get(2)) {
                let type_name = type_name.as_str().to_string();
                let field_name = field_name.as_str().to_string();
                map.insert((fp.clone(), field_name.clone()), type_name.clone());
                // Also with _ prefix
                map.insert((fp.clone(), format!("_{}", field_name)), type_name.clone());
                class_count += 1;
            }
        }

        // Extract constructor DI params (modern pattern)
        for cap in CLASS_RE.captures_iter(&file.content) {
            if let Some(class_name) = cap.get(1) {
                let deps =
                    code_explorer_lang::route_extractors::csharp::extract_constructor_dependencies(
                        &file.content,
                        class_name.as_str(),
                    );
                for (iface_type, param_name) in deps {
                    map.insert((fp.clone(), param_name.clone()), iface_type.clone());
                    map.insert((fp.clone(), format!("_{}", param_name)), iface_type.clone());
                }
            }
        }

        // Extract 'using' pattern: using (var svc = new CourriersService(...))
        for cap in USING_RE.captures_iter(&file.content) {
            if let (Some(var_name), Some(type_name)) = (cap.get(1), cap.get(2)) {
                map.insert(
                    (fp.clone(), var_name.as_str().to_string()),
                    type_name.as_str().to_string(),
                );
            }
        }

        // Extract local instantiation: var x = new SomeService(...)
        for cap in LOCAL_NEW_RE.captures_iter(&file.content) {
            if let (Some(var_name), Some(type_name)) = (cap.get(1), cap.get(2)) {
                map.insert(
                    (fp.clone(), var_name.as_str().to_string()),
                    type_name.as_str().to_string(),
                );
            }
        }
    }

    tracing::debug!(
        "Built field type map: {} entries from {} classes with DI",
        map.len(),
        class_count
    );
    map
}

fn build_ts_type_envs(
    file_entries: &[crate::phases::structure::FileEntry],
    ts_external_imported_type_names: &HashMap<String, HashSet<String>>,
) -> HashMap<String, TypeEnvironment> {
    let mut envs = HashMap::new();

    for file in file_entries {
        if !is_ts_like_file(&file.path) {
            continue;
        }

        let mut env = TypeEnvironment::new();
        let scope_ranges = build_ts_scope_ranges(file);

        let Some(lang @ (SupportedLanguage::TypeScript | SupportedLanguage::JavaScript)) =
            file.language
        else {
            continue;
        };
        let ts_language = crate::grammar::get_language_for_file(lang, &file.path);
        let mut parser = Parser::new();
        if parser.set_language(&ts_language).is_ok() {
            if let Some(tree) = parser.parse(&file.content, None) {
                collect_ts_parameter_type_bindings(
                    tree.root_node(),
                    &file.path,
                    file.content.as_bytes(),
                    &mut env,
                );
            }
        }

        for cap in TS_EXPLICIT_TYPE_RE.captures_iter(&file.content) {
            if let (Some(var_name), Some(type_name)) = (cap.get(1), cap.get(2)) {
                if let Some(type_name) = normalize_ts_type_name(type_name.as_str()) {
                    let scope = ts_scope_at_byte(&scope_ranges, cap.get(0).unwrap().start());
                    env.bind(scope, var_name.as_str(), type_name);
                }
            }
        }

        for cap in TS_CONSTRUCTOR_RE.captures_iter(&file.content) {
            if let (Some(var_name), Some(type_name)) = (cap.get(1), cap.get(2)) {
                if let Some(type_name) = normalize_ts_type_name(type_name.as_str()) {
                    let scope = ts_scope_at_byte(&scope_ranges, cap.get(0).unwrap().start());
                    env.bind_constructor(scope, var_name.as_str(), type_name);
                }
            }
        }

        for cap in TS_FIELD_TYPE_RE.captures_iter(&file.content) {
            if let (Some(var_name), Some(type_name)) = (cap.get(1), cap.get(2)) {
                if let Some(type_name) = normalize_ts_type_name(type_name.as_str()) {
                    let scope = ts_scope_at_byte(&scope_ranges, cap.get(0).unwrap().start());
                    env.bind(scope, var_name.as_str(), type_name);
                }
            }
        }

        for cap in TS_CONSTRUCTOR_PARAMS_RE.captures_iter(&file.content) {
            let Some(params) = cap.get(1) else {
                continue;
            };
            for param in params.as_str().split(',') {
                if let Some(param_cap) = TS_PARAM_TYPE_RE.captures(param) {
                    if let (Some(var_name), Some(type_name)) = (param_cap.get(1), param_cap.get(2))
                    {
                        if let Some(type_name) = normalize_ts_type_name(type_name.as_str()) {
                            let scope =
                                ts_scope_at_byte(&scope_ranges, cap.get(0).unwrap().start());
                            env.bind(scope, var_name.as_str(), type_name);
                            if is_ts_parameter_property(param) {
                                env.bind("", var_name.as_str(), type_name);
                            }
                        }
                    }
                }
            }
        }

        if let Some(external_type_names) = ts_external_imported_type_names.get(&file.path) {
            bind_ts_external_fluent_assignments(file, &scope_ranges, external_type_names, &mut env);
        }

        envs.insert(file.path.clone(), env);
    }

    envs
}

fn build_ts_external_receiver_envs(
    file_entries: &[crate::phases::structure::FileEntry],
) -> HashMap<String, TypeEnvironment> {
    let mut envs = HashMap::new();

    for file in file_entries {
        if !is_ts_like_file(&file.path) {
            continue;
        }

        let fs_aliases = collect_ts_node_fs_aliases(&file.content);
        let mut env = TypeEnvironment::new();
        let scope_ranges = build_ts_scope_ranges(file);

        bind_ts_node_fs_stats_receivers(file, &scope_ranges, &fs_aliases, &mut env);
        bind_ts_node_fs_dirent_arrays(file, &scope_ranges, &fs_aliases, &mut env);
        bind_ts_external_array_derivatives(file, &scope_ranges, &mut env);
        bind_ts_external_array_loop_items(file, &scope_ranges, &mut env);
        bind_ts_external_array_index_items(file, &scope_ranges, &mut env);
        bind_ts_external_array_callback_params(file, &scope_ranges, &mut env);
        bind_ts_fetch_response_receivers(file, &scope_ranges, &mut env);
        bind_ts_external_response_parameters(file, &scope_ranges, &mut env);
        bind_ts_express_router_response_parameters(file, &scope_ranges, &mut env);
        bind_ts_node_event_receivers(file, &scope_ranges, &mut env);

        envs.insert(file.path.clone(), env);
    }

    envs
}

fn collect_ts_node_fs_aliases(content: &str) -> HashSet<String> {
    let mut aliases = HashSet::new();

    for cap in TS_NODE_FS_NAMESPACE_IMPORT_RE.captures_iter(content) {
        if let Some(alias) = cap.get(1) {
            aliases.insert(alias.as_str().to_string());
        }
    }

    for cap in TS_NODE_FS_DEFAULT_IMPORT_RE.captures_iter(content) {
        if let Some(alias) = cap.get(1) {
            aliases.insert(alias.as_str().to_string());
        }
    }

    for cap in TS_NODE_FS_PROMISES_IMPORT_RE.captures_iter(content) {
        if let Some(alias) = cap.get(1) {
            aliases.insert(alias.as_str().to_string());
        }
    }

    for cap in TS_NODE_FS_REQUIRE_RE.captures_iter(content) {
        if let Some(alias) = cap.get(1) {
            aliases.insert(alias.as_str().to_string());
        }
    }

    aliases
}

fn bind_ts_node_fs_stats_receivers(
    file: &crate::phases::structure::FileEntry,
    scope_ranges: &[TsScopeRange],
    fs_aliases: &HashSet<String>,
    env: &mut TypeEnvironment,
) {
    for cap in TS_NODE_FS_STAT_ASSIGN_RE.captures_iter(&file.content) {
        let (Some(var_name), Some(alias)) = (cap.get(1), cap.get(2)) else {
            continue;
        };
        if !fs_aliases.contains(alias.as_str()) {
            continue;
        }

        let scope = ts_scope_at_byte(scope_ranges, cap.get(0).unwrap().start());
        env.bind(scope, var_name.as_str(), TS_EXTERNAL_NODE_FS_STATS);
    }
}

fn bind_ts_node_fs_dirent_arrays(
    file: &crate::phases::structure::FileEntry,
    scope_ranges: &[TsScopeRange],
    fs_aliases: &HashSet<String>,
    env: &mut TypeEnvironment,
) {
    for cap in TS_NODE_FS_READDIR_ASSIGN_RE.captures_iter(&file.content) {
        let (Some(var_name), Some(alias), Some(call_prefix)) = (cap.get(1), cap.get(2), cap.get(0))
        else {
            continue;
        };
        if !fs_aliases.contains(alias.as_str()) {
            continue;
        }

        let Some(args) = extract_balanced_parenthesized(&file.content, call_prefix.end() - 1)
        else {
            continue;
        };
        if !TS_WITH_FILE_TYPES_TRUE_RE.is_match(args) {
            continue;
        }

        let scope = ts_scope_at_byte(scope_ranges, call_prefix.start());
        env.bind(scope, var_name.as_str(), TS_EXTERNAL_NODE_FS_DIRENT_ARRAY);
    }
}

fn bind_ts_external_array_derivatives(
    file: &crate::phases::structure::FileEntry,
    scope_ranges: &[TsScopeRange],
    env: &mut TypeEnvironment,
) {
    for cap in TS_ARRAY_METHOD_ASSIGN_RE.captures_iter(&file.content) {
        let (Some(var_name), Some(source_array), Some(full_match)) =
            (cap.get(1), cap.get(2), cap.get(0))
        else {
            continue;
        };

        let scope = ts_scope_at_byte(scope_ranges, full_match.start());
        if env
            .resolve(scope, source_array.as_str())
            .is_some_and(is_ts_external_dirent_array)
        {
            env.bind(scope, var_name.as_str(), TS_EXTERNAL_NODE_FS_DIRENT_ARRAY);
        }
    }
}

fn bind_ts_external_array_loop_items(
    file: &crate::phases::structure::FileEntry,
    scope_ranges: &[TsScopeRange],
    env: &mut TypeEnvironment,
) {
    for cap in TS_FOR_OF_RE.captures_iter(&file.content) {
        let (Some(item_name), Some(array_name), Some(full_match)) =
            (cap.get(1), cap.get(2), cap.get(0))
        else {
            continue;
        };

        let scope = ts_scope_at_byte(scope_ranges, full_match.start());
        if env
            .resolve(scope, array_name.as_str())
            .is_some_and(is_ts_external_dirent_array)
        {
            env.bind(scope, item_name.as_str(), TS_EXTERNAL_NODE_FS_DIRENT);
        }
    }
}

fn bind_ts_external_array_index_items(
    file: &crate::phases::structure::FileEntry,
    scope_ranges: &[TsScopeRange],
    env: &mut TypeEnvironment,
) {
    for cap in TS_ARRAY_INDEX_ASSIGN_RE.captures_iter(&file.content) {
        let (Some(item_name), Some(array_name), Some(full_match)) =
            (cap.get(1), cap.get(2), cap.get(0))
        else {
            continue;
        };

        let scope = ts_scope_at_byte(scope_ranges, full_match.start());
        if env
            .resolve(scope, array_name.as_str())
            .is_some_and(is_ts_external_dirent_array)
        {
            env.bind(scope, item_name.as_str(), TS_EXTERNAL_NODE_FS_DIRENT);
        }
    }
}

fn bind_ts_external_array_callback_params(
    file: &crate::phases::structure::FileEntry,
    scope_ranges: &[TsScopeRange],
    env: &mut TypeEnvironment,
) {
    for cap in TS_ARRAY_ASSIGN_CALLBACK_RE.captures_iter(&file.content) {
        let (Some(callback_name), Some(array_name), Some(first_param), Some(full_match)) =
            (cap.get(1), cap.get(2), cap.get(3), cap.get(0))
        else {
            continue;
        };

        let array_scope = ts_scope_at_byte(scope_ranges, array_name.start());
        if !env
            .resolve(array_scope, array_name.as_str())
            .is_some_and(is_ts_external_dirent_array)
        {
            continue;
        }

        let assignment_scope = ts_scope_at_byte(scope_ranges, full_match.start());
        let callback_scope = generate_id(
            "Function",
            &format!("{}:{}", file.path, callback_name.as_str()),
        );
        bind_ts_external_callback_param_scopes(
            env,
            [assignment_scope, callback_scope.as_str()],
            first_param.as_str(),
            cap.get(4).map(|param| param.as_str()),
        );
    }

    for cap in TS_ARRAY_CALLBACK_RE.captures_iter(&file.content) {
        let (Some(array_name), Some(first_param)) = (cap.get(1), cap.get(2)) else {
            continue;
        };

        let array_scope = ts_scope_at_byte(scope_ranges, array_name.start());
        if !env
            .resolve(array_scope, array_name.as_str())
            .is_some_and(is_ts_external_dirent_array)
        {
            continue;
        }

        let callback_scope = ts_scope_at_byte(scope_ranges, first_param.start());
        bind_ts_external_callback_param_scopes(
            env,
            [callback_scope],
            first_param.as_str(),
            cap.get(3).map(|param| param.as_str()),
        );
    }
}

fn bind_ts_external_callback_param_scopes<const N: usize>(
    env: &mut TypeEnvironment,
    scopes: [&str; N],
    first_param: &str,
    second_param: Option<&str>,
) {
    for scope in scopes {
        env.bind(scope, first_param, TS_EXTERNAL_NODE_FS_DIRENT);
        if let Some(second_param) = second_param {
            env.bind(scope, second_param, TS_EXTERNAL_NODE_FS_DIRENT);
        }
    }
}

fn bind_ts_fetch_response_receivers(
    file: &crate::phases::structure::FileEntry,
    scope_ranges: &[TsScopeRange],
    env: &mut TypeEnvironment,
) {
    for cap in TS_FETCH_RESPONSE_ASSIGN_RE.captures_iter(&file.content) {
        let (Some(var_name), Some(full_match)) = (cap.get(1), cap.get(0)) else {
            continue;
        };
        let scope = ts_scope_at_byte(scope_ranges, full_match.start());
        env.bind(scope, var_name.as_str(), TS_EXTERNAL_WEB_RESPONSE);
    }
}

fn bind_ts_external_response_parameters(
    file: &crate::phases::structure::FileEntry,
    scope_ranges: &[TsScopeRange],
    env: &mut TypeEnvironment,
) {
    for cap in TS_PARAM_TYPE_RE.captures_iter(&file.content) {
        let (Some(var_name), Some(type_name), Some(full_match)) =
            (cap.get(1), cap.get(2), cap.get(0))
        else {
            continue;
        };

        let Some(type_name) = normalize_ts_type_name(type_name.as_str()) else {
            continue;
        };

        let external_type = match type_name {
            "Response" | "express.Response" => TS_EXTERNAL_EXPRESS_RESPONSE,
            _ => continue,
        };

        let scope = ts_scope_at_byte(scope_ranges, full_match.start());
        env.bind(scope, var_name.as_str(), external_type);
    }
}

fn bind_ts_express_router_response_parameters(
    file: &crate::phases::structure::FileEntry,
    scope_ranges: &[TsScopeRange],
    env: &mut TypeEnvironment,
) {
    if !TS_EXPRESS_IMPORT_RE.is_match(&file.content) {
        return;
    }

    for cap in TS_EXPRESS_ROUTER_CALLBACK_RE.captures_iter(&file.content) {
        let Some(response_param) = cap.get(2) else {
            continue;
        };
        let scope = ts_scope_at_byte(scope_ranges, response_param.start());
        env.bind(scope, response_param.as_str(), TS_EXTERNAL_EXPRESS_RESPONSE);
    }
}

fn bind_ts_node_event_receivers(
    file: &crate::phases::structure::FileEntry,
    scope_ranges: &[TsScopeRange],
    env: &mut TypeEnvironment,
) {
    for cap in TS_NODE_CHILD_PROCESS_ASSIGN_RE.captures_iter(&file.content) {
        let (Some(var_name), Some(full_match)) = (cap.get(1), cap.get(0)) else {
            continue;
        };
        let scope = ts_scope_at_byte(scope_ranges, full_match.start());
        env.bind(scope, var_name.as_str(), TS_EXTERNAL_NODE_CHILD_PROCESS);
    }

    for cap in TS_EVENT_EMITTER_ASSIGN_RE.captures_iter(&file.content) {
        let (Some(var_name), Some(full_match)) = (cap.get(1), cap.get(0)) else {
            continue;
        };
        let scope = ts_scope_at_byte(scope_ranges, full_match.start());
        env.bind(scope, var_name.as_str(), TS_EXTERNAL_EVENT_EMITTER);
    }
}

fn extract_balanced_parenthesized(content: &str, open_paren_byte: usize) -> Option<&str> {
    if content.as_bytes().get(open_paren_byte) != Some(&b'(') {
        return None;
    }

    let mut depth = 0i32;
    let mut in_string: Option<char> = None;
    let mut escaped = false;

    for (idx, ch) in content[open_paren_byte..].char_indices() {
        let absolute_idx = open_paren_byte + idx;

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
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    return content.get(open_paren_byte + 1..absolute_idx);
                }
            }
            _ => {}
        }
    }

    None
}

fn build_ts_scope_ranges(file: &crate::phases::structure::FileEntry) -> Vec<TsScopeRange> {
    let Some(lang @ (SupportedLanguage::TypeScript | SupportedLanguage::JavaScript)) =
        file.language
    else {
        return Vec::new();
    };

    let ts_language = crate::grammar::get_language_for_file(lang, &file.path);
    let mut parser = Parser::new();
    if parser.set_language(&ts_language).is_err() {
        return Vec::new();
    }

    let Some(tree) = parser.parse(&file.content, None) else {
        return Vec::new();
    };

    let mut ranges = Vec::new();
    collect_ts_scope_ranges(
        tree.root_node(),
        &file.path,
        file.content.as_bytes(),
        &mut ranges,
    );
    ranges
}

fn collect_ts_scope_ranges(
    node: Node,
    file_path: &str,
    content: &[u8],
    ranges: &mut Vec<TsScopeRange>,
) {
    if let Some(source_id) = ts_scope_id_for_node(node, file_path, content) {
        ranges.push(TsScopeRange {
            start_byte: node.start_byte(),
            end_byte: node.end_byte(),
            source_id,
        });
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_ts_scope_ranges(child, file_path, content, ranges);
    }
}

fn ts_scope_id_for_node(node: Node, file_path: &str, content: &[u8]) -> Option<String> {
    let kind = node.kind();
    if !is_ts_function_like_node(node) {
        return None;
    }

    let name_node = ts_function_like_name_node(&node)?;
    let name = name_node.utf8_text(content).ok()?;
    let label = if matches!(
        kind,
        "function_declaration" | "arrow_function" | "function_expression"
    ) {
        "Function"
    } else {
        "Method"
    };
    Some(generate_id(label, &format!("{file_path}:{name}")))
}

fn is_ts_function_like_node(node: Node) -> bool {
    matches!(
        node.kind(),
        "method_definition" | "function_declaration" | "arrow_function" | "function_expression"
    )
}

fn ts_function_like_name_node<'tree>(node: &Node<'tree>) -> Option<Node<'tree>> {
    if let Some(name) = node.child_by_field_name("name") {
        return Some(name);
    }

    let parent = node.parent()?;
    match parent.kind() {
        "variable_declarator"
        | "pair"
        | "property_assignment"
        | "field_definition"
        | "public_field_definition" => parent
            .child_by_field_name("name")
            .or_else(|| parent.child_by_field_name("property"))
            .or_else(|| parent.child_by_field_name("key").map(ts_key_name_node)),
        _ => None,
    }
}

fn ts_key_name_node<'tree>(key: Node<'tree>) -> Node<'tree> {
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

fn ts_scope_at_byte(scopes: &[TsScopeRange], byte: usize) -> &str {
    scopes
        .iter()
        .filter(|scope| byte >= scope.start_byte && byte <= scope.end_byte)
        .min_by_key(|scope| scope.end_byte.saturating_sub(scope.start_byte))
        .map(|scope| scope.source_id.as_str())
        .unwrap_or("")
}

fn is_ts_parameter_property(param: &str) -> bool {
    let trimmed = param.trim_start();
    trimmed.starts_with("public ")
        || trimmed.starts_with("private ")
        || trimmed.starts_with("protected ")
        || trimmed.starts_with("readonly ")
}

/// Resolve all extracted calls and create CALLS edges.
///
/// Resolution tiers:
/// - 0: Receiver-aware: _service.Method() → resolve via DI type map (C# only)
/// - 1: Same-file exact match
/// - 2a: Named import binding chain
/// - 2b: Package-scoped fuzzy match
/// - 3: Global fuzzy match
///
/// Creates CALLS edges in the graph with confidence based on resolution tier.
#[allow(clippy::too_many_arguments)]
pub fn resolve_calls(
    graph: &mut KnowledgeGraph,
    extracted: &ExtractedData,
    symbol_table: &SymbolTable,
    import_map: &ImportMap,
    named_import_map: &NamedImportMap,
    re_export_map: &ReExportMap,
    package_map: &PackageMap,
    module_alias_map: &ModuleAliasMap,
    file_entries: &[crate::phases::structure::FileEntry],
) -> Result<(), IngestError> {
    let mut ctx = ResolutionContext::new(
        symbol_table,
        import_map,
        package_map,
        named_import_map,
        re_export_map,
        module_alias_map,
    );

    // Build field→type map for receiver-aware resolution (C# DI)
    let field_type_map = build_field_type_map(file_entries);
    let ts_external_imported_type_names = build_ts_external_imported_type_names(extracted);
    let ts_type_envs = build_ts_type_envs(file_entries, &ts_external_imported_type_names);
    let ts_external_receiver_envs = build_ts_external_receiver_envs(file_entries);
    let ts_global_fallback_blocked_names = build_ts_global_fallback_blocked_names(
        extracted,
        named_import_map,
        re_export_map,
        symbol_table,
        file_entries,
    );
    let ts_callable_parameter_names = build_ts_callable_parameter_names(file_entries);
    let ts_opaque_local_callable_names = build_ts_opaque_local_callable_names(file_entries);
    let ts_bound_method_aliases = build_ts_bound_method_aliases(file_entries);
    let ts_imported_binding_names = build_ts_imported_binding_names(extracted);
    let mut receiver_resolved = 0u32;

    let mut edge_count = 0;

    // Debug: count how many calls have receivers
    let with_receiver = extracted
        .calls
        .iter()
        .filter(|c| c.receiver_name.is_some())
        .count();
    let cs_calls = extracted
        .calls
        .iter()
        .filter(|c| c.file_path.ends_with(".cs"))
        .count();
    tracing::debug!(
        "Calls: {} total, {} C#, {} with receiver",
        extracted.calls.len(),
        cs_calls,
        with_receiver
    );

    for call in &extracted.calls {
        ctx.enable_cache(&call.file_path);

        // Tier 0: Field-type-aware resolution for C# files
        // If the call has a receiver (e.g., _courriersService.CreerCourrier()),
        // look up the receiver's type from the field map and resolve the method
        // in that specific service type's file.
        if call.file_path.ends_with(".cs") {
            if let Some(ref receiver) = call.receiver_name {
                // Look up the specific receiver's type from the field map
                if let Some(svc_type) =
                    field_type_map.get(&(call.file_path.clone(), receiver.clone()))
                {
                    if let Some(candidates) = symbol_table.lookup_global(&call.called_name) {
                        let impl_name = svc_type.strip_prefix('I').unwrap_or(svc_type);
                        let target = candidates.iter().find(|def| {
                            (def.symbol_type == NodeLabel::Method
                                || def.symbol_type == NodeLabel::Function)
                                && (def.file_path.contains(impl_name)
                                    || def
                                        .owner_id
                                        .as_deref()
                                        .map(|o| o.contains(impl_name))
                                        .unwrap_or(false))
                        });

                        if let Some(target_def) = target {
                            let edge_id =
                                format!("calls_di_{}_{}", call.source_id, target_def.node_id);
                            if graph.get_relationship(&edge_id).is_none() {
                                graph.add_relationship(GraphRelationship {
                                    id: edge_id,
                                    source_id: call.source_id.clone(),
                                    target_id: target_def.node_id.clone(),
                                    rel_type: RelationshipType::Calls,
                                    confidence: 0.85,
                                    reason: format!("field-type:{}:{}", receiver, call.called_name),
                                    step: None,
                                });
                                edge_count += 1;
                                receiver_resolved += 1;
                            }
                            continue;
                        }
                    }
                }
            }
        }

        // Tier 0.5: Static method calls — receiver is a class name, not a field/variable
        // e.g., RegleCourriers.TraitementGenerationCourrier(...)
        if call.file_path.ends_with(".cs") {
            if let Some(ref receiver) = call.receiver_name {
                // Check if the receiver matches a known Class/Struct name
                if let Some(class_defs) = symbol_table.lookup_global(receiver) {
                    let class_files: Vec<&str> = class_defs
                        .iter()
                        .filter(|d| matches!(d.symbol_type, NodeLabel::Class | NodeLabel::Struct))
                        .map(|d| d.file_path.as_str())
                        .collect();

                    if !class_files.is_empty() {
                        if let Some(method_defs) = symbol_table.lookup_global(&call.called_name) {
                            let target = method_defs.iter().find(|d| {
                                matches!(
                                    d.symbol_type,
                                    NodeLabel::Method
                                        | NodeLabel::Function
                                        | NodeLabel::Constructor
                                ) && class_files.contains(&d.file_path.as_str())
                            });
                            if let Some(target_def) = target {
                                let edge_id = format!(
                                    "calls_static_{}_{}",
                                    call.source_id, target_def.node_id
                                );
                                if graph.get_relationship(&edge_id).is_none() {
                                    graph.add_relationship(GraphRelationship {
                                        id: edge_id,
                                        source_id: call.source_id.clone(),
                                        target_id: target_def.node_id.clone(),
                                        rel_type: RelationshipType::Calls,
                                        confidence: 0.80,
                                        reason: format!(
                                            "static-call:{}::{}",
                                            receiver, call.called_name
                                        ),
                                        step: None,
                                    });
                                    edge_count += 1;
                                    receiver_resolved += 1;
                                }
                                continue;
                            }
                        }
                    }
                }
            }
        }

        if should_skip_ts_type_only_runtime_call(call, named_import_map, re_export_map) {
            continue;
        }

        // Tier 0.55: TypeScript/JavaScript exported object members.
        // `export const api = { run }` + `import { api } ...; api.run()`
        // should resolve to the exact member exposed by the exported object,
        // not by fuzzy import-scoped matching.
        if is_ts_like_file(&call.file_path) {
            if let Some((target_def, reason)) = resolve_ts_exported_object_member_call_target(
                call,
                &mut ctx,
                symbol_table,
                named_import_map,
                re_export_map,
                import_map,
                ts_type_envs.get(&call.file_path),
                ts_external_receiver_envs.get(&call.file_path),
                ts_imported_binding_names.get(&call.file_path),
                ts_external_imported_type_names.get(&call.file_path),
            ) {
                let edge_id = format!(
                    "calls_ts_default_object_{}_{}",
                    call.source_id, target_def.node_id
                );
                if graph.get_relationship(&edge_id).is_none() {
                    graph.add_relationship(GraphRelationship {
                        id: edge_id,
                        source_id: call.source_id.clone(),
                        target_id: target_def.node_id.clone(),
                        rel_type: RelationshipType::Calls,
                        confidence: 0.94,
                        reason,
                        step: None,
                    });
                    edge_count += 1;
                    receiver_resolved += 1;
                }
                continue;
            }
        }

        // Tier 0.6: TypeScript/JavaScript receiver type inference
        // `const svc = new Service(); svc.run()` should resolve to
        // `Service.run`, not a global `run` with the same name.
        if is_ts_like_file(&call.file_path) {
            if let Some(ref receiver) = call.receiver_name {
                let receiver_root = receiver_type_lookup_name(receiver);
                if let Some(type_name) = ts_type_envs
                    .get(&call.file_path)
                    .and_then(|env| env.resolve(&call.source_id, receiver_root))
                {
                    if let Some(target_def) = resolve_type_member_target(
                        symbol_table,
                        named_import_map,
                        re_export_map,
                        import_map,
                        &call.file_path,
                        type_name,
                        &call.called_name,
                    ) {
                        let edge_id = format!(
                            "calls_ts_receiver_{}_{}",
                            call.source_id, target_def.node_id
                        );
                        if graph.get_relationship(&edge_id).is_none() {
                            graph.add_relationship(GraphRelationship {
                                id: edge_id,
                                source_id: call.source_id.clone(),
                                target_id: target_def.node_id.clone(),
                                rel_type: RelationshipType::Calls,
                                confidence: 0.90,
                                reason: format!(
                                    "receiver-type:{}:{}:{}",
                                    receiver_root, type_name, call.called_name
                                ),
                                step: None,
                            });
                            edge_count += 1;
                            receiver_resolved += 1;
                        }
                        continue;
                    }
                }
            }
        }

        // Tier 0.65: TypeScript/JavaScript static class calls
        // `Service.create()` uses a class name as receiver. Resolve the member
        // against that class before falling through to global matching.
        if is_ts_like_file(&call.file_path) {
            if let Some(ref receiver) = call.receiver_name {
                let receiver_root = receiver_root(receiver);
                if receiver_root
                    .chars()
                    .next()
                    .is_some_and(|ch| ch.is_ascii_uppercase())
                {
                    if let Some(target_def) = resolve_type_member_target(
                        symbol_table,
                        named_import_map,
                        re_export_map,
                        import_map,
                        &call.file_path,
                        receiver_root,
                        &call.called_name,
                    ) {
                        let edge_id =
                            format!("calls_ts_static_{}_{}", call.source_id, target_def.node_id);
                        if graph.get_relationship(&edge_id).is_none() {
                            graph.add_relationship(GraphRelationship {
                                id: edge_id,
                                source_id: call.source_id.clone(),
                                target_id: target_def.node_id.clone(),
                                rel_type: RelationshipType::Calls,
                                confidence: 0.88,
                                reason: format!(
                                    "static-call-ts:{}::{}",
                                    receiver_root, call.called_name
                                ),
                                step: None,
                            });
                            edge_count += 1;
                            receiver_resolved += 1;
                        }
                        continue;
                    }
                }
            }
        }

        // Tier 0.7: TypeScript/JavaScript namespace imports
        // `import * as api from "./api"; api.run()` should resolve `run` inside
        // the imported module before generic same-file/import/global matching.
        if is_ts_like_file(&call.file_path) {
            if let Some(ref receiver) = call.receiver_name {
                let receiver_root = receiver_root(receiver);
                if let Some(source_file) = module_alias_map
                    .get(&call.file_path)
                    .and_then(|aliases| aliases.get(receiver_root))
                {
                    if let Some(defs) = symbol_table.lookup_in_file(source_file, &call.called_name)
                    {
                        if let Some(target_def) = defs
                            .iter()
                            .find(|def| {
                                matches!(
                                    def.symbol_type,
                                    NodeLabel::Method
                                        | NodeLabel::Function
                                        | NodeLabel::Constructor
                                        | NodeLabel::Class
                                )
                            })
                            .or(defs.first())
                        {
                            let edge_id = format!(
                                "calls_module_alias_{}_{}",
                                call.source_id, target_def.node_id
                            );
                            if graph.get_relationship(&edge_id).is_none() {
                                graph.add_relationship(GraphRelationship {
                                    id: edge_id,
                                    source_id: call.source_id.clone(),
                                    target_id: target_def.node_id.clone(),
                                    rel_type: RelationshipType::Calls,
                                    confidence: 0.93,
                                    reason: format!(
                                        "module-alias:{}:{}",
                                        receiver_root, call.called_name
                                    ),
                                    step: None,
                                });
                                edge_count += 1;
                            }
                            continue;
                        }
                    }
                }
            }
        }

        if should_skip_ts_parameter_call(call, &ts_callable_parameter_names) {
            continue;
        }

        if should_skip_ts_opaque_local_call(call, &ts_opaque_local_callable_names) {
            continue;
        }

        if let Some((target_def, reason)) = resolve_ts_bound_method_alias_call_target(
            call,
            &mut ctx,
            symbol_table,
            named_import_map,
            re_export_map,
            import_map,
            ts_bound_method_aliases.get(&call.source_id),
            ts_type_envs.get(&call.file_path),
            ts_external_receiver_envs.get(&call.file_path),
            ts_imported_binding_names.get(&call.file_path),
            ts_external_imported_type_names.get(&call.file_path),
        ) {
            let edge_id = format!(
                "calls_ts_bound_method_{}_{}",
                call.source_id, target_def.node_id
            );
            if graph.get_relationship(&edge_id).is_none() {
                graph.add_relationship(GraphRelationship {
                    id: edge_id,
                    source_id: call.source_id.clone(),
                    target_id: target_def.node_id.clone(),
                    rel_type: RelationshipType::Calls,
                    confidence: 0.89,
                    reason,
                    step: None,
                });
                edge_count += 1;
                receiver_resolved += 1;
            }
            continue;
        }

        // Tiers 1-3: Standard resolution
        if let Some(resolved) = ctx.resolve(&call.called_name, &call.file_path) {
            if should_skip_ts_global_fallback(call, &resolved, &ts_global_fallback_blocked_names) {
                continue;
            }

            let confidence = resolved.tier.confidence();
            let reason = resolved.tier.as_str().to_string();

            // Pick best candidate (first match, or arity-filtered)
            let target = select_call_target(
                call,
                &resolved,
                ts_type_envs.get(&call.file_path),
                ts_external_receiver_envs.get(&call.file_path),
                ts_imported_binding_names.get(&call.file_path),
                ts_external_imported_type_names.get(&call.file_path),
            );

            if let Some(target_def) = target {
                let edge_id = format!("calls_{}_{}", call.source_id, target_def.node_id);
                // Skip if this exact edge already exists
                if graph.get_relationship(&edge_id).is_none() {
                    graph.add_relationship(GraphRelationship {
                        id: edge_id,
                        source_id: call.source_id.clone(),
                        target_id: target_def.node_id.clone(),
                        rel_type: RelationshipType::Calls,
                        confidence,
                        reason,
                        step: None,
                    });
                    edge_count += 1;
                }
            }
        }
    }

    tracing::info!(
        "Resolved {} call edges ({} via receiver-aware tiers)",
        edge_count,
        receiver_resolved
    );
    Ok(())
}

/// Safety net: re-point any `CALLS` edge whose `source_id` node doesn't exist to the
/// File node of the source's file (the codebase's convention for unattributable calls),
/// dropping it only if even the File node is missing. The call source is computed during
/// parsing without graph access, so a best-effort attribution (e.g. C/C++ functions whose
/// name lives in a declarator) can occasionally name a node the query labeled differently;
/// this guarantees no orphan-source CALLS edges regardless. Returns the number re-pointed.
pub fn repoint_orphan_call_sources(graph: &mut KnowledgeGraph) -> usize {
    // Collect fixes while borrowing the graph immutably; mutate after.
    let fixes: Vec<(String, String, Option<String>)> = {
        let node_ids: HashSet<&str> = graph.iter_nodes().map(|n| n.id.as_str()).collect();
        graph
            .iter_relationships()
            .filter(|r| {
                r.rel_type == RelationshipType::Calls && !node_ids.contains(r.source_id.as_str())
            })
            .map(|r| {
                let file_id = file_id_from_node_id(&r.source_id)
                    .filter(|fid| node_ids.contains(fid.as_str()));
                (r.id.clone(), r.target_id.clone(), file_id)
            })
            .collect()
    };

    let mut fixed = 0;
    for (old_id, target, file_id) in fixes {
        graph.remove_relationship(&old_id);
        if let Some(fid) = file_id {
            let new_id = format!("calls_{fid}_{target}");
            if graph.get_relationship(&new_id).is_none() {
                graph.add_relationship(GraphRelationship {
                    id: new_id,
                    source_id: fid,
                    target_id: target,
                    rel_type: RelationshipType::Calls,
                    confidence: 1.0,
                    reason: "repointed_orphan_source".to_string(),
                    step: None,
                });
            }
        }
        fixed += 1;
    }
    fixed
}

/// `"{Label}:{filepath}:{name}"` -> `Some("File:{filepath}")` (filepath = everything
/// between the label and the last `:`). Returns None if the id doesn't fit the shape.
fn file_id_from_node_id(id: &str) -> Option<String> {
    let after_label = id.split_once(':')?.1;
    let filepath = after_label.rsplit_once(':')?.0;
    if filepath.is_empty() {
        return None;
    }
    Some(format!("File:{filepath}"))
}

#[allow(clippy::too_many_arguments)]
fn resolve_ts_bound_method_alias_call_target(
    call: &ExtractedCall,
    ctx: &mut ResolutionContext<'_>,
    symbol_table: &SymbolTable,
    named_import_map: &NamedImportMap,
    re_export_map: &ReExportMap,
    import_map: &ImportMap,
    aliases: Option<&HashMap<String, TsBoundMethodAlias>>,
    ts_type_env: Option<&TypeEnvironment>,
    ts_external_receiver_env: Option<&TypeEnvironment>,
    ts_imported_binding_names: Option<&HashSet<String>>,
    ts_external_imported_type_names: Option<&HashSet<String>>,
) -> Option<(Arc<code_explorer_core::symbol::SymbolDefinition>, String)> {
    if !is_ts_like_file(&call.file_path) || !matches!(call.call_form, CallForm::Free) {
        return None;
    }

    let alias = aliases?.get(&call.called_name)?;
    let receiver_root = receiver_type_lookup_name(&alias.receiver_name);

    if let Some(type_name) = ts_type_env.and_then(|env| env.resolve(&call.source_id, receiver_root))
    {
        if let Some(target_def) = resolve_type_member_target(
            symbol_table,
            named_import_map,
            re_export_map,
            import_map,
            &call.file_path,
            type_name,
            &alias.member_name,
        ) {
            return Some((
                target_def.clone(),
                format!(
                    "bound-method:{}:{}:{}",
                    alias.receiver_name, type_name, alias.member_name
                ),
            ));
        }
    }

    let resolved = ctx.resolve(&alias.member_name, &call.file_path)?;
    let mut member_call = call.clone();
    member_call.called_name = alias.member_name.clone();
    member_call.receiver_name = Some(alias.receiver_name.clone());
    member_call.receiver_type_name = None;

    select_call_target(
        &member_call,
        &resolved,
        ts_type_env,
        ts_external_receiver_env,
        ts_imported_binding_names,
        ts_external_imported_type_names,
    )
    .cloned()
    .map(|target_def| {
        (
            target_def,
            format!("bound-method:{}:{}", alias.receiver_name, alias.member_name),
        )
    })
}

#[allow(clippy::too_many_arguments)]
fn resolve_ts_exported_object_member_call_target(
    call: &ExtractedCall,
    ctx: &mut ResolutionContext<'_>,
    symbol_table: &SymbolTable,
    named_import_map: &NamedImportMap,
    re_export_map: &ReExportMap,
    import_map: &ImportMap,
    ts_type_env: Option<&TypeEnvironment>,
    ts_external_receiver_env: Option<&TypeEnvironment>,
    ts_imported_binding_names: Option<&HashSet<String>>,
    ts_external_imported_type_names: Option<&HashSet<String>>,
) -> Option<(Arc<code_explorer_core::symbol::SymbolDefinition>, String)> {
    if !matches!(call.call_form, CallForm::Member) {
        return None;
    }

    let receiver = call.receiver_name.as_deref()?;
    let receiver_root = receiver_root(receiver);
    let binding = named_import_map
        .get(&call.file_path)
        .and_then(|bindings| bindings.get(receiver_root))?;
    if binding.is_type_only {
        return None;
    }

    if let Some(type_member) = receiver_member_after_root(receiver) {
        let (source_file, local_type_name, via_default) =
            resolve_ts_exported_object_member_binding(
                named_import_map,
                re_export_map,
                &binding.source_path,
                &binding.exported_name,
                type_member,
                &mut HashSet::new(),
            )?;
        let target_def = resolve_type_member_target(
            symbol_table,
            named_import_map,
            re_export_map,
            import_map,
            &source_file,
            &local_type_name,
            &call.called_name,
        )?;
        let reason_prefix = if via_default {
            "default-object-static"
        } else {
            "named-object-static"
        };
        return Some((
            Arc::clone(target_def),
            format!(
                "{reason_prefix}:{}:{}::{}",
                receiver_root, type_member, call.called_name
            ),
        ));
    }

    let (source_file, local_name, via_default) = resolve_ts_exported_object_member_binding(
        named_import_map,
        re_export_map,
        &binding.source_path,
        &binding.exported_name,
        &call.called_name,
        &mut HashSet::new(),
    )?;
    let resolved = ctx.resolve(&local_name, &source_file)?;
    let target_def = select_call_target(
        call,
        &resolved,
        ts_type_env,
        ts_external_receiver_env,
        ts_imported_binding_names,
        ts_external_imported_type_names,
    )?;
    let reason_prefix = if via_default {
        "default-object"
    } else {
        "named-object"
    };
    Some((
        Arc::clone(target_def),
        format!("{reason_prefix}:{}:{}", receiver_root, call.called_name),
    ))
}

fn resolve_ts_exported_object_member_binding(
    named_import_map: &NamedImportMap,
    re_export_map: &ReExportMap,
    module_file: &str,
    exported_name: &str,
    member_name: &str,
    visited: &mut HashSet<(String, String, String)>,
) -> Option<(String, String, bool)> {
    if !visited.insert((
        module_file.to_string(),
        exported_name.to_string(),
        member_name.to_string(),
    )) {
        return None;
    }

    let object_member_name = format!("{exported_name}.{member_name}");
    if let Some(re_exports) = re_export_map.get(module_file) {
        for re_export in re_exports {
            if re_export.is_type_only {
                continue;
            }
            let (Some(local), Some(exported)) = (&re_export.local_name, &re_export.exported_name)
            else {
                continue;
            };
            if local == &object_member_name {
                return Some((
                    re_export.source_path.clone(),
                    exported.clone(),
                    exported_name == "default",
                ));
            }
        }

        if exported_name != "default" {
            for re_export in re_exports {
                if re_export.is_type_only {
                    continue;
                }
                if !matches!(
                    (&re_export.local_name, &re_export.exported_name),
                    (None, None)
                ) {
                    continue;
                }
                if let Some(result) = resolve_ts_exported_object_member_binding(
                    named_import_map,
                    re_export_map,
                    &re_export.source_path,
                    exported_name,
                    member_name,
                    visited,
                ) {
                    return Some(result);
                }
            }
        }
    }

    let re_exports = re_export_map.get(module_file)?;
    for re_export in re_exports {
        if re_export.is_type_only {
            continue;
        }

        let (Some(local), Some(exported)) = (&re_export.local_name, &re_export.exported_name)
        else {
            continue;
        };
        if local != exported_name {
            continue;
        }

        if re_export.source_path == module_file {
            if let Some(binding) = named_import_map
                .get(module_file)
                .and_then(|bindings| bindings.get(exported))
            {
                if let Some(result) = resolve_ts_exported_object_member_binding(
                    named_import_map,
                    re_export_map,
                    &binding.source_path,
                    &binding.exported_name,
                    member_name,
                    visited,
                ) {
                    return Some(result);
                }
            }
        }

        if let Some(result) = resolve_ts_exported_object_member_binding(
            named_import_map,
            re_export_map,
            &re_export.source_path,
            exported,
            member_name,
            visited,
        ) {
            return Some(result);
        }
    }

    None
}

fn select_call_target<'a>(
    call: &ExtractedCall,
    resolved: &'a code_explorer_core::resolution::types::TieredCandidates,
    ts_type_env: Option<&TypeEnvironment>,
    ts_external_receiver_env: Option<&TypeEnvironment>,
    ts_imported_binding_names: Option<&HashSet<String>>,
    ts_external_imported_type_names: Option<&HashSet<String>>,
) -> Option<&'a Arc<code_explorer_core::symbol::SymbolDefinition>> {
    let mut candidates: Vec<_> = resolved
        .candidates
        .iter()
        .filter(|candidate| {
            !should_skip_ts_call_candidate(
                call,
                resolved.tier,
                candidate,
                ts_type_env,
                ts_external_receiver_env,
                ts_imported_binding_names,
                ts_external_imported_type_names,
            )
        })
        .collect();

    if candidates.is_empty() {
        return None;
    }

    if is_ts_like_file(&call.file_path)
        && candidates
            .iter()
            .any(|candidate| is_valid_ts_fuzzy_call_target(call.call_form, &candidate.symbol_type))
    {
        candidates.retain(|candidate| {
            is_valid_ts_fuzzy_call_target(call.call_form, &candidate.symbol_type)
        });
    }

    if let Some(arg_count) = call.arg_count {
        candidates
            .iter()
            .copied()
            .find(|candidate| {
                let param_count = candidate.parameter_count.unwrap_or(0);
                let required = candidate.required_parameter_count.unwrap_or(0);
                arg_count >= required && arg_count <= param_count
            })
            .or_else(|| candidates.first().copied())
    } else {
        candidates.first().copied()
    }
}

fn should_skip_ts_call_candidate(
    call: &ExtractedCall,
    tier: ResolutionTier,
    candidate: &code_explorer_core::symbol::SymbolDefinition,
    ts_type_env: Option<&TypeEnvironment>,
    ts_external_receiver_env: Option<&TypeEnvironment>,
    ts_imported_binding_names: Option<&HashSet<String>>,
    ts_external_imported_type_names: Option<&HashSet<String>>,
) -> bool {
    if !is_ts_like_file(&call.file_path) {
        return false;
    }

    if matches!(tier, ResolutionTier::ImportScoped | ResolutionTier::Global)
        && matches!(call.call_form, CallForm::Member)
    {
        return true;
    }

    if matches!(tier, ResolutionTier::ImportScoped | ResolutionTier::Global)
        && !is_valid_ts_fuzzy_call_target(call.call_form, &candidate.symbol_type)
    {
        return true;
    }

    if matches!(tier, ResolutionTier::ImportScoped)
        && matches!(call.call_form, CallForm::Free)
        && ts_external_imported_type_names.is_some_and(|names| names.contains(&call.called_name))
    {
        return true;
    }

    if !matches!(tier, ResolutionTier::SameFile)
        || !matches!(candidate.symbol_type, NodeLabel::Property)
    {
        return false;
    }

    if matches!(call.call_form, CallForm::Free) {
        return true;
    }

    if !matches!(call.call_form, CallForm::Member) {
        return false;
    }

    if call
        .receiver_name
        .as_deref()
        .map(receiver_root)
        .is_some_and(is_ts_known_global_receiver)
    {
        return true;
    }

    if is_ts_imported_member_receiver(call, ts_imported_binding_names) {
        return true;
    }

    if is_ts_external_imported_type_member_receiver(
        call,
        ts_type_env,
        ts_external_imported_type_names,
    ) {
        return true;
    }

    if is_ts_builtin_member_call_on_non_this_receiver(call) {
        return true;
    }

    is_ts_external_member_receiver(call, ts_external_receiver_env)
}

fn is_ts_builtin_member_call_on_non_this_receiver(call: &ExtractedCall) -> bool {
    let Some(receiver) = call.receiver_name.as_deref() else {
        return false;
    };

    let root = receiver_root(receiver);
    if root == "super" {
        return false;
    }

    if root == "this" && !is_nested_this_receiver(receiver) {
        return false;
    }

    is_ts_known_builtin_member_name(&call.called_name)
}

fn is_nested_this_receiver(receiver: &str) -> bool {
    receiver
        .strip_prefix("this.")
        .is_some_and(|member| !member.is_empty())
}

fn is_ts_known_builtin_member_name(name: &str) -> bool {
    matches!(
        name,
        "at" | "concat"
            | "endsWith"
            | "entries"
            | "every"
            | "filter"
            | "find"
            | "findIndex"
            | "flat"
            | "flatMap"
            | "forEach"
            | "includes"
            | "indexOf"
            | "join"
            | "keys"
            | "lastIndexOf"
            | "map"
            | "match"
            | "pop"
            | "push"
            | "reduce"
            | "reduceRight"
            | "replace"
            | "reverse"
            | "shift"
            | "slice"
            | "some"
            | "sort"
            | "splice"
            | "split"
            | "startsWith"
            | "substring"
            | "toLowerCase"
            | "toSorted"
            | "toSpliced"
            | "toUpperCase"
            | "trim"
            | "trimEnd"
            | "trimStart"
            | "unshift"
            | "values"
    )
}

fn is_ts_imported_member_receiver(
    call: &ExtractedCall,
    ts_imported_binding_names: Option<&HashSet<String>>,
) -> bool {
    let Some(receiver) = call.receiver_name.as_deref() else {
        return false;
    };

    let Some(imported_names) = ts_imported_binding_names else {
        return false;
    };

    imported_names.contains(receiver_root(receiver))
}

fn is_ts_external_imported_type_member_receiver(
    call: &ExtractedCall,
    ts_type_env: Option<&TypeEnvironment>,
    ts_external_imported_type_names: Option<&HashSet<String>>,
) -> bool {
    let Some(receiver) = call.receiver_name.as_deref() else {
        return false;
    };
    let Some(env) = ts_type_env else {
        return false;
    };
    let Some(external_type_names) = ts_external_imported_type_names else {
        return false;
    };

    let receiver_root = receiver_type_lookup_name(receiver);
    env.resolve(&call.source_id, receiver_root)
        .is_some_and(|type_name| {
            ts_type_name_matches_imported_external(type_name, external_type_names)
        })
}

fn ts_type_name_matches_imported_external(
    type_name: &str,
    external_type_names: &HashSet<String>,
) -> bool {
    external_type_names.contains(type_name)
        || type_name
            .split('.')
            .next()
            .is_some_and(|root| external_type_names.contains(root))
        || type_name
            .rsplit('.')
            .next()
            .is_some_and(|leaf| external_type_names.contains(leaf))
}

fn is_ts_external_member_receiver(
    call: &ExtractedCall,
    ts_external_receiver_env: Option<&TypeEnvironment>,
) -> bool {
    let Some(receiver) = call.receiver_name.as_deref() else {
        return false;
    };

    let Some(env) = ts_external_receiver_env else {
        return false;
    };

    let receiver_root = receiver_type_lookup_name(receiver);
    env.resolve(&call.source_id, receiver_root)
        .is_some_and(is_ts_external_member_type)
}

fn is_ts_external_member_type(type_name: &str) -> bool {
    matches!(
        type_name,
        TS_EXTERNAL_NODE_FS_STATS
            | TS_EXTERNAL_NODE_FS_DIRENT
            | TS_EXTERNAL_WEB_RESPONSE
            | TS_EXTERNAL_EXPRESS_RESPONSE
            | TS_EXTERNAL_NODE_CHILD_PROCESS
            | TS_EXTERNAL_EVENT_EMITTER
    )
}

fn is_ts_external_dirent_array(type_name: &str) -> bool {
    type_name == TS_EXTERNAL_NODE_FS_DIRENT_ARRAY
}

fn is_valid_ts_fuzzy_call_target(call_form: CallForm, label: &NodeLabel) -> bool {
    match call_form {
        CallForm::Constructor => matches!(
            label,
            NodeLabel::Class
                | NodeLabel::Struct
                | NodeLabel::Function
                | NodeLabel::Method
                | NodeLabel::Constructor
        ),
        CallForm::Free | CallForm::Member => {
            matches!(
                label,
                NodeLabel::Function | NodeLabel::Method | NodeLabel::Constructor
            )
        }
    }
}

fn is_ts_known_global_receiver(receiver: &str) -> bool {
    matches!(
        receiver,
        "Array"
            | "Atomics"
            | "BigInt"
            | "Boolean"
            | "Buffer"
            | "Date"
            | "Error"
            | "Intl"
            | "JSON"
            | "Map"
            | "Math"
            | "Number"
            | "Object"
            | "Promise"
            | "Reflect"
            | "RegExp"
            | "Set"
            | "String"
            | "Symbol"
            | "URL"
            | "URLSearchParams"
            | "WeakMap"
            | "WeakSet"
            | "console"
            | "crypto"
            | "global"
            | "globalThis"
            | "performance"
            | "process"
    )
}

fn build_ts_callable_parameter_names(
    file_entries: &[crate::phases::structure::FileEntry],
) -> HashMap<String, HashSet<String>> {
    let mut names_by_source: HashMap<String, HashSet<String>> = HashMap::new();

    for file in file_entries {
        let Some(lang @ (SupportedLanguage::TypeScript | SupportedLanguage::JavaScript)) =
            file.language
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

        collect_ts_callable_parameter_names(
            tree.root_node(),
            &file.path,
            file.content.as_bytes(),
            &mut names_by_source,
        );
    }

    names_by_source
}

fn collect_ts_callable_parameter_names(
    node: Node,
    file_path: &str,
    content: &[u8],
    names_by_source: &mut HashMap<String, HashSet<String>>,
) {
    if let Some(source_id) = ts_scope_id_for_node(node, file_path, content) {
        if let Some(parameters) = node.child_by_field_name("parameters") {
            if let Ok(parameter_text) = parameters.utf8_text(content) {
                let names = extract_ts_parameter_names(parameter_text);
                if !names.is_empty() {
                    names_by_source.insert(source_id, names);
                }
            }
        }
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_ts_callable_parameter_names(child, file_path, content, names_by_source);
    }
}

fn build_ts_opaque_local_callable_names(
    file_entries: &[crate::phases::structure::FileEntry],
) -> HashMap<String, HashSet<String>> {
    let mut names_by_source: HashMap<String, HashSet<String>> = HashMap::new();

    for file in file_entries {
        let Some(lang @ (SupportedLanguage::TypeScript | SupportedLanguage::JavaScript)) =
            file.language
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

        collect_ts_opaque_local_callable_names(
            tree.root_node(),
            &file.path,
            file.content.as_bytes(),
            &mut names_by_source,
        );
    }

    names_by_source
}

fn collect_ts_opaque_local_callable_names(
    node: Node,
    file_path: &str,
    content: &[u8],
    names_by_source: &mut HashMap<String, HashSet<String>>,
) {
    if let Some(source_id) = ts_scope_id_for_node(node, file_path, content) {
        let mut names = HashSet::new();
        collect_ts_opaque_local_callable_names_in_scope(node, content, &mut names);
        if !names.is_empty() {
            names_by_source.entry(source_id).or_default().extend(names);
        }
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_ts_opaque_local_callable_names(child, file_path, content, names_by_source);
    }
}

fn collect_ts_opaque_local_callable_names_in_scope(
    node: Node,
    content: &[u8],
    names: &mut HashSet<String>,
) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if is_ts_function_like_node(child) {
            continue;
        }

        if matches!(child.kind(), "for_in_statement" | "for_statement") {
            if let Ok(text) = child.utf8_text(content) {
                for cap in TS_FOR_OF_LITERAL_RE.captures_iter(text) {
                    if let Some(name) = cap.get(1) {
                        names.insert(name.as_str().to_string());
                    }
                }
            }
        }

        collect_ts_opaque_local_callable_names_in_scope(child, content, names);
    }
}

fn build_ts_bound_method_aliases(
    file_entries: &[crate::phases::structure::FileEntry],
) -> HashMap<String, HashMap<String, TsBoundMethodAlias>> {
    let mut aliases_by_source: HashMap<String, HashMap<String, TsBoundMethodAlias>> =
        HashMap::new();

    for file in file_entries {
        let Some(SupportedLanguage::TypeScript | SupportedLanguage::JavaScript) = file.language
        else {
            continue;
        };

        let scope_ranges = build_ts_scope_ranges(file);
        for cap in TS_BOUND_METHOD_ALIAS_RE.captures_iter(&file.content) {
            let Some(full_match) = cap.get(0) else {
                continue;
            };
            let (Some(alias), Some(receiver), Some(member)) = (cap.get(1), cap.get(2), cap.get(3))
            else {
                continue;
            };

            let scope = ts_scope_at_byte(&scope_ranges, full_match.start());
            if scope.is_empty() {
                continue;
            }

            aliases_by_source
                .entry(scope.to_string())
                .or_default()
                .entry(alias.as_str().to_string())
                .or_insert_with(|| TsBoundMethodAlias {
                    receiver_name: receiver.as_str().to_string(),
                    member_name: member.as_str().to_string(),
                });
        }
    }

    aliases_by_source
}

fn collect_ts_parameter_type_bindings(
    node: Node,
    file_path: &str,
    content: &[u8],
    env: &mut TypeEnvironment,
) {
    if let Some(source_id) = ts_scope_id_for_node(node, file_path, content) {
        if let Some(parameters) = node.child_by_field_name("parameters") {
            if let Ok(parameter_text) = parameters.utf8_text(content) {
                for (param_name, type_name) in extract_ts_parameter_types(parameter_text) {
                    env.bind(&source_id, &param_name, &type_name);
                }
            }
        }
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_ts_parameter_type_bindings(child, file_path, content, env);
    }
}

fn bind_ts_external_fluent_assignments(
    file: &crate::phases::structure::FileEntry,
    scope_ranges: &[TsScopeRange],
    external_type_names: &HashSet<String>,
    env: &mut TypeEnvironment,
) {
    if external_type_names.is_empty() {
        return;
    }

    for _ in 0..4 {
        let mut bound_count = 0usize;

        for cap in TS_EXTERNAL_FLUENT_ASSIGN_RE.captures_iter(&file.content) {
            let (Some(var_name), Some(receiver), Some(full_match)) =
                (cap.get(1), cap.get(2), cap.get(0))
            else {
                continue;
            };

            let scope = ts_scope_at_byte(scope_ranges, full_match.start());
            if env.lookup(scope, var_name.as_str()).is_some()
                || env.lookup_constructor(scope, var_name.as_str()).is_some()
            {
                continue;
            }

            let receiver_name = receiver_type_lookup_name(receiver.as_str());
            let Some(type_name) = env.resolve(scope, receiver_name) else {
                continue;
            };
            if !ts_type_name_matches_imported_external(type_name, external_type_names) {
                continue;
            }

            let type_name = type_name.to_string();
            env.bind(scope, var_name.as_str(), &type_name);
            bound_count += 1;
        }

        if bound_count == 0 {
            break;
        }
    }
}

fn extract_ts_parameter_types(parameters: &str) -> Vec<(String, String)> {
    let trimmed = parameters.trim();
    let inner = if trimmed.starts_with('(') && trimmed.ends_with(')') {
        &trimmed[1..trimmed.len() - 1]
    } else {
        trimmed
    };

    split_ts_top_level_commas(inner)
        .into_iter()
        .filter_map(|param| {
            let param_cap = TS_PARAM_TYPE_RE.captures(param)?;
            let param_name = param_cap.get(1)?.as_str();
            let type_name = normalize_ts_type_name(param_cap.get(2)?.as_str())?;
            Some((param_name.to_string(), type_name.to_string()))
        })
        .collect()
}

fn extract_ts_parameter_names(parameters: &str) -> HashSet<String> {
    let trimmed = parameters.trim();
    let inner = if trimmed.starts_with('(') && trimmed.ends_with(')') {
        &trimmed[1..trimmed.len() - 1]
    } else {
        trimmed
    };

    split_ts_top_level_commas(inner)
        .into_iter()
        .filter_map(extract_ts_parameter_name)
        .collect()
}

fn split_ts_top_level_commas(input: &str) -> Vec<&str> {
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

fn should_skip_ts_parameter_call(
    call: &ExtractedCall,
    ts_callable_parameter_names: &HashMap<String, HashSet<String>>,
) -> bool {
    is_ts_like_file(&call.file_path)
        && matches!(call.call_form, CallForm::Free)
        && ts_callable_parameter_names
            .get(&call.source_id)
            .is_some_and(|names| names.contains(&call.called_name))
}

fn should_skip_ts_opaque_local_call(
    call: &ExtractedCall,
    ts_opaque_local_callable_names: &HashMap<String, HashSet<String>>,
) -> bool {
    is_ts_like_file(&call.file_path)
        && matches!(call.call_form, CallForm::Free)
        && ts_opaque_local_callable_names
            .get(&call.source_id)
            .is_some_and(|names| names.contains(&call.called_name))
}

fn should_skip_ts_type_only_runtime_call(
    call: &ExtractedCall,
    named_import_map: &NamedImportMap,
    re_export_map: &ReExportMap,
) -> bool {
    if !is_ts_like_file(&call.file_path) {
        return false;
    }

    let runtime_name = match call.call_form {
        CallForm::Free | CallForm::Constructor => call.called_name.as_str(),
        CallForm::Member => {
            let Some(receiver) = call.receiver_name.as_deref() else {
                return false;
            };
            receiver_root(receiver)
        }
    };

    is_ts_type_only_binding(
        &call.file_path,
        runtime_name,
        named_import_map,
        re_export_map,
        &mut HashSet::new(),
    )
}

fn is_ts_type_only_binding(
    from_file: &str,
    name: &str,
    named_import_map: &NamedImportMap,
    re_export_map: &ReExportMap,
    visited: &mut HashSet<(String, String)>,
) -> bool {
    if !visited.insert((from_file.to_string(), name.to_string())) {
        return false;
    }

    let Some(binding) = named_import_map
        .get(from_file)
        .and_then(|bindings| bindings.get(name))
    else {
        return false;
    };

    binding.is_type_only
        || is_ts_type_only_module_export(
            &binding.source_path,
            &binding.exported_name,
            named_import_map,
            re_export_map,
            visited,
        )
}

fn is_ts_type_only_module_export(
    module_file: &str,
    exported_name: &str,
    named_import_map: &NamedImportMap,
    re_export_map: &ReExportMap,
    visited: &mut HashSet<(String, String)>,
) -> bool {
    if !visited.insert((module_file.to_string(), exported_name.to_string())) {
        return false;
    }

    let mut saw_type_only = false;
    let mut saw_runtime = false;

    if let Some(bindings) = named_import_map.get(module_file) {
        if let Some(binding) = bindings.get(exported_name) {
            if binding.is_type_only {
                saw_type_only = true;
            } else if is_ts_type_only_module_export(
                &binding.source_path,
                &binding.exported_name,
                named_import_map,
                re_export_map,
                visited,
            ) {
                saw_type_only = true;
            } else {
                saw_runtime = true;
            }
        }
    }

    if let Some(re_exports) = re_export_map.get(module_file) {
        for re_export in re_exports {
            if !ts_re_export_matches_name(re_export, exported_name) {
                continue;
            }

            if re_export.is_type_only {
                saw_type_only = true;
            } else if let Some(source_name) = re_export.exported_name.as_deref() {
                if is_ts_type_only_binding(
                    &re_export.source_path,
                    source_name,
                    named_import_map,
                    re_export_map,
                    visited,
                ) || is_ts_type_only_module_export(
                    &re_export.source_path,
                    source_name,
                    named_import_map,
                    re_export_map,
                    visited,
                ) {
                    saw_type_only = true;
                } else {
                    saw_runtime = true;
                }
            } else {
                saw_runtime = true;
            }
        }
    }

    saw_type_only && !saw_runtime
}

fn ts_re_export_matches_name(re_export: &ReExportBinding, exported_name: &str) -> bool {
    match (&re_export.local_name, &re_export.exported_name) {
        (Some(local), _) if local == exported_name => true,
        (None, None) => true,
        _ => false,
    }
}

fn build_ts_imported_binding_names(extracted: &ExtractedData) -> HashMap<String, HashSet<String>> {
    let mut names_by_file: HashMap<String, HashSet<String>> = HashMap::new();

    for import in &extracted.imports {
        if !is_ts_like_file(&import.file_path) {
            continue;
        }

        let Some(lang) = SupportedLanguage::from_filename(&import.file_path) else {
            continue;
        };
        let provider = code_explorer_lang::registry::get_provider(lang);
        let binding_text = import
            .binding_text
            .as_deref()
            .unwrap_or(&import.raw_import_path);

        let Some(bindings) = provider.extract_named_bindings(binding_text) else {
            continue;
        };

        for binding in bindings {
            names_by_file
                .entry(import.file_path.clone())
                .or_default()
                .insert(binding.local);
        }
    }

    names_by_file
}

fn build_ts_external_imported_type_names(
    extracted: &ExtractedData,
) -> HashMap<String, HashSet<String>> {
    let mut names_by_file: HashMap<String, HashSet<String>> = HashMap::new();

    for import in &extracted.imports {
        if !is_ts_like_file(&import.file_path)
            || !is_external_ts_import_path(&import.raw_import_path)
        {
            continue;
        }

        let Some(lang) = SupportedLanguage::from_filename(&import.file_path) else {
            continue;
        };
        let provider = code_explorer_lang::registry::get_provider(lang);
        let binding_text = import
            .binding_text
            .as_deref()
            .unwrap_or(&import.raw_import_path);

        let Some(bindings) = provider.extract_named_bindings(binding_text) else {
            continue;
        };

        let names = names_by_file.entry(import.file_path.clone()).or_default();
        for binding in bindings {
            names.insert(binding.local);
        }
    }

    names_by_file
}

fn build_ts_global_fallback_blocked_names(
    extracted: &ExtractedData,
    named_import_map: &NamedImportMap,
    re_export_map: &ReExportMap,
    symbol_table: &SymbolTable,
    file_entries: &[crate::phases::structure::FileEntry],
) -> HashMap<String, HashSet<String>> {
    let mut names_by_file: HashMap<String, HashSet<String>> = HashMap::new();

    for import in &extracted.imports {
        if !is_ts_like_file(&import.file_path) {
            continue;
        }

        let Some(lang) = SupportedLanguage::from_filename(&import.file_path) else {
            continue;
        };
        let provider = code_explorer_lang::registry::get_provider(lang);
        let binding_text = import
            .binding_text
            .as_deref()
            .unwrap_or(&import.raw_import_path);

        let Some(bindings) = provider.extract_named_bindings(binding_text) else {
            continue;
        };

        for binding in bindings {
            if is_external_ts_import_path(&import.raw_import_path)
                || is_unresolved_ts_import_binding(
                    import,
                    &binding.local,
                    named_import_map,
                    re_export_map,
                    symbol_table,
                )
            {
                names_by_file
                    .entry(import.file_path.clone())
                    .or_default()
                    .insert(binding.local);
            }
        }
    }

    add_ts_external_namespace_destructured_names(file_entries, &mut names_by_file);

    names_by_file
}

fn is_unresolved_ts_import_binding(
    import: &crate::phases::parsing::ExtractedImport,
    local_name: &str,
    named_import_map: &NamedImportMap,
    re_export_map: &ReExportMap,
    symbol_table: &SymbolTable,
) -> bool {
    if is_external_ts_import_path(&import.raw_import_path) {
        return false;
    }

    let Some(binding) = named_import_map
        .get(&import.file_path)
        .and_then(|bindings| bindings.get(local_name))
    else {
        return true;
    };

    let mut visited = HashSet::new();
    !ts_module_export_exists(
        &binding.source_path,
        &binding.exported_name,
        symbol_table,
        named_import_map,
        re_export_map,
        &mut visited,
    )
}

fn ts_module_export_exists(
    module_file: &str,
    name: &str,
    symbol_table: &SymbolTable,
    named_import_map: &NamedImportMap,
    re_export_map: &ReExportMap,
    visited: &mut HashSet<(String, String)>,
) -> bool {
    if !visited.insert((module_file.to_string(), name.to_string())) {
        return false;
    }

    if symbol_table
        .lookup_in_file(module_file, name)
        .is_some_and(|defs| !defs.is_empty())
    {
        return true;
    }

    re_export_map.get(module_file).is_some_and(|re_exports| {
        re_exports.iter().any(
            |re_export| match (&re_export.local_name, &re_export.exported_name) {
                (Some(local), Some(exported)) if local == name => {
                    if re_export.source_path == module_file {
                        if let Some(binding) = named_import_map
                            .get(module_file)
                            .and_then(|bindings| bindings.get(exported))
                        {
                            return ts_module_export_exists(
                                &binding.source_path,
                                &binding.exported_name,
                                symbol_table,
                                named_import_map,
                                re_export_map,
                                visited,
                            );
                        }
                    }

                    ts_module_export_exists(
                        &re_export.source_path,
                        exported,
                        symbol_table,
                        named_import_map,
                        re_export_map,
                        visited,
                    )
                }
                (None, None) => ts_module_export_exists(
                    &re_export.source_path,
                    name,
                    symbol_table,
                    named_import_map,
                    re_export_map,
                    visited,
                ),
                _ => false,
            },
        )
    })
}

fn add_ts_external_namespace_destructured_names(
    file_entries: &[crate::phases::structure::FileEntry],
    names_by_file: &mut HashMap<String, HashSet<String>>,
) {
    for file in file_entries {
        let Some(lang @ (SupportedLanguage::TypeScript | SupportedLanguage::JavaScript)) =
            file.language
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

        let mut namespace_vars = HashSet::new();
        collect_ts_external_namespace_destructured_names(
            tree.root_node(),
            &file.path,
            file.content.as_bytes(),
            &mut namespace_vars,
            names_by_file,
        );
    }
}

fn collect_ts_external_namespace_destructured_names(
    node: Node,
    file_path: &str,
    content: &[u8],
    namespace_vars: &mut HashSet<String>,
    names_by_file: &mut HashMap<String, HashSet<String>>,
) {
    if node.kind() == "variable_declarator" {
        if let (Some(name), Some(value)) = (
            node.child_by_field_name("name"),
            node.child_by_field_name("value"),
        ) {
            if name.kind() == "identifier" {
                if contains_external_dynamic_import(value, content) {
                    if let Ok(name_text) = name.utf8_text(content) {
                        namespace_vars.insert(name_text.to_string());
                    }
                }
            } else if name.kind() == "object_pattern" {
                if let Ok(value_text) = value.utf8_text(content) {
                    if namespace_vars.contains(value_text.trim()) {
                        add_ts_binding_names_from_pattern(file_path, name, content, names_by_file);
                    }
                }
            }
        }
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_ts_external_namespace_destructured_names(
            child,
            file_path,
            content,
            namespace_vars,
            names_by_file,
        );
    }
}

fn contains_external_dynamic_import(node: Node, content: &[u8]) -> bool {
    if node.kind() == "call_expression"
        && node
            .child_by_field_name("function")
            .is_some_and(|function| function.kind() == "import")
        && node
            .child_by_field_name("arguments")
            .is_some_and(|arguments| {
                let mut cursor = arguments.walk();
                let has_external_import = arguments.children(&mut cursor).any(|child| {
                    if child.kind() != "string" {
                        return false;
                    }
                    child
                        .utf8_text(content)
                        .ok()
                        .map(|text| text.trim_matches(|c| c == '"' || c == '\'' || c == '`'))
                        .is_some_and(is_external_ts_import_path)
                });
                has_external_import
            })
    {
        return true;
    }

    let mut cursor = node.walk();
    let has_external_import = node
        .children(&mut cursor)
        .any(|child| contains_external_dynamic_import(child, content));
    has_external_import
}

fn add_ts_binding_names_from_pattern(
    file_path: &str,
    pattern: Node,
    content: &[u8],
    names_by_file: &mut HashMap<String, HashSet<String>>,
) {
    let Some(lang) = SupportedLanguage::from_filename(file_path) else {
        return;
    };
    let provider = code_explorer_lang::registry::get_provider(lang);
    let Ok(pattern_text) = pattern.utf8_text(content) else {
        return;
    };
    let Some(bindings) = provider.extract_named_bindings(pattern_text) else {
        return;
    };

    let names = names_by_file.entry(file_path.to_string()).or_default();
    for binding in bindings {
        names.insert(binding.local);
    }
}

fn is_external_ts_import_path(path: &str) -> bool {
    let trimmed = path.trim();
    !trimmed.is_empty()
        && !trimmed.starts_with('.')
        && !trimmed.starts_with('/')
        && !trimmed.starts_with('#')
}

fn should_skip_ts_global_fallback(
    call: &ExtractedCall,
    resolved: &code_explorer_core::resolution::types::TieredCandidates,
    ts_global_fallback_blocked_names: &HashMap<String, HashSet<String>>,
) -> bool {
    if !matches!(resolved.tier, ResolutionTier::Global) {
        return false;
    }

    if !resolved.candidates.is_empty()
        && resolved.candidates.iter().all(|candidate| {
            is_ts_like_file(&candidate.file_path)
                && is_ts_type_only_call_target(&candidate.symbol_type)
        })
    {
        return true;
    }

    if !is_ts_like_file(&call.file_path) {
        return false;
    }

    if matches!(call.call_form, CallForm::Member) && call.receiver_name.is_some() {
        return true;
    }

    ts_global_fallback_blocked_names
        .get(&call.file_path)
        .is_some_and(|names| names.contains(&call.called_name))
}

fn is_ts_type_only_call_target(label: &NodeLabel) -> bool {
    matches!(label, NodeLabel::Interface | NodeLabel::TypeAlias)
}

fn resolve_type_member_target<'a>(
    symbol_table: &'a SymbolTable,
    named_import_map: &NamedImportMap,
    re_export_map: &ReExportMap,
    import_map: &ImportMap,
    from_file: &str,
    type_name: &str,
    member_name: &str,
) -> Option<&'a Arc<code_explorer_core::symbol::SymbolDefinition>> {
    let (type_ids, type_files) = collect_type_candidates(
        symbol_table,
        named_import_map,
        re_export_map,
        import_map,
        from_file,
        type_name,
    );
    if type_ids.is_empty() && type_files.is_empty() {
        return None;
    }

    let member_defs = symbol_table.lookup_global(member_name)?;
    member_defs
        .iter()
        .find(|def| {
            matches!(
                def.symbol_type,
                NodeLabel::Method
                    | NodeLabel::Function
                    | NodeLabel::Constructor
                    | NodeLabel::Property
            ) && def
                .owner_id
                .as_deref()
                .is_some_and(|owner| type_ids.contains(owner))
        })
        .or_else(|| {
            member_defs.iter().find(|def| {
                matches!(def.symbol_type, NodeLabel::Method | NodeLabel::Property)
                    && type_files.contains(&def.file_path)
            })
        })
}

fn collect_type_candidates(
    symbol_table: &SymbolTable,
    named_import_map: &NamedImportMap,
    re_export_map: &ReExportMap,
    import_map: &ImportMap,
    from_file: &str,
    type_name: &str,
) -> (HashSet<String>, HashSet<String>) {
    let mut ids = HashSet::new();
    let mut files = HashSet::new();
    let short_name = type_name.rsplit('.').next().unwrap_or(type_name);

    if let Some(bindings) = named_import_map.get(from_file) {
        if let Some(binding) = bindings.get(short_name) {
            add_module_export_type_defs(
                symbol_table,
                named_import_map,
                re_export_map,
                &binding.source_path,
                &binding.exported_name,
                &mut HashSet::new(),
                &mut ids,
                &mut files,
            );
        }
    }

    if let Some(defs) = symbol_table.lookup_in_file(from_file, short_name) {
        add_type_defs(defs, &mut ids, &mut files);
    }

    if let Some(imported_files) = import_map.get(from_file) {
        for imported_file in imported_files {
            if let Some(defs) = symbol_table.lookup_in_file(imported_file, short_name) {
                add_type_defs(defs, &mut ids, &mut files);
            }
            add_module_export_type_defs(
                symbol_table,
                named_import_map,
                re_export_map,
                imported_file,
                short_name,
                &mut HashSet::new(),
                &mut ids,
                &mut files,
            );
        }
    }

    if ids.is_empty() {
        if let Some(defs) = symbol_table.lookup_global(short_name) {
            add_type_defs(defs, &mut ids, &mut files);
        }
    }

    (ids, files)
}

fn add_type_defs(
    defs: &[Arc<code_explorer_core::symbol::SymbolDefinition>],
    ids: &mut HashSet<String>,
    files: &mut HashSet<String>,
) {
    for def in defs {
        if matches!(
            def.symbol_type,
            NodeLabel::Class
                | NodeLabel::Interface
                | NodeLabel::Struct
                | NodeLabel::TypeAlias
                | NodeLabel::Enum
        ) {
            ids.insert(def.node_id.clone());
            files.insert(def.file_path.clone());
        }
    }
}

fn add_module_export_type_defs(
    symbol_table: &SymbolTable,
    named_import_map: &NamedImportMap,
    re_export_map: &ReExportMap,
    module_file: &str,
    type_name: &str,
    visited: &mut HashSet<(String, String)>,
    ids: &mut HashSet<String>,
    files: &mut HashSet<String>,
) {
    if !visited.insert((module_file.to_string(), type_name.to_string())) {
        return;
    }

    if let Some(defs) = symbol_table.lookup_in_file(module_file, type_name) {
        add_type_defs(defs, ids, files);
    }

    let Some(re_exports) = re_export_map.get(module_file) else {
        return;
    };
    for re_export in re_exports {
        match (&re_export.local_name, &re_export.exported_name) {
            (Some(local), Some(exported)) if local == type_name => {
                if re_export.source_path == module_file {
                    if let Some(binding) = named_import_map
                        .get(module_file)
                        .and_then(|bindings| bindings.get(exported))
                    {
                        add_module_export_type_defs(
                            symbol_table,
                            named_import_map,
                            re_export_map,
                            &binding.source_path,
                            &binding.exported_name,
                            visited,
                            ids,
                            files,
                        );
                        continue;
                    }
                }
                add_module_export_type_defs(
                    symbol_table,
                    named_import_map,
                    re_export_map,
                    &re_export.source_path,
                    exported,
                    visited,
                    ids,
                    files,
                );
            }
            (None, None) => {
                add_module_export_type_defs(
                    symbol_table,
                    named_import_map,
                    re_export_map,
                    &re_export.source_path,
                    type_name,
                    visited,
                    ids,
                    files,
                );
            }
            _ => {}
        }
    }
}

fn is_ts_like_file(file_path: &str) -> bool {
    let lower = file_path.to_lowercase();
    matches!(
        lower.rsplit('.').next(),
        Some("ts" | "tsx" | "mts" | "cts" | "js" | "jsx" | "mjs" | "cjs")
    )
}

fn receiver_root(receiver: &str) -> &str {
    receiver
        .split(|ch: char| !(ch == '_' || ch == '$' || ch == '.' || ch.is_ascii_alphanumeric()))
        .next()
        .unwrap_or(receiver)
        .split('.')
        .next()
        .unwrap_or(receiver)
}

fn receiver_member_after_root(receiver: &str) -> Option<&str> {
    let receiver = receiver
        .split(|ch: char| !(ch == '_' || ch == '$' || ch == '.' || ch.is_ascii_alphanumeric()))
        .next()
        .unwrap_or(receiver);
    let mut parts = receiver.split('.');
    parts.next()?;
    parts.next().filter(|part| !part.is_empty())
}

fn receiver_type_lookup_name(receiver: &str) -> &str {
    if let Some(field_receiver) = receiver.strip_prefix("this.") {
        receiver_root(field_receiver)
    } else {
        receiver_root(receiver)
    }
}

fn normalize_ts_type_name(type_name: &str) -> Option<&str> {
    let trimmed = type_name.trim();
    let trimmed = trimmed.trim_start_matches("readonly ").trim();
    let trimmed = trimmed.trim_start_matches("typeof ").trim();
    let trimmed = trimmed
        .split(['<', '[', '|', '&', '?', ' '])
        .next()
        .unwrap_or(trimmed)
        .trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    }
}

#[cfg(test)]
mod orphan_source_tests {
    use super::*;

    #[test]
    fn test_file_id_from_node_id() {
        assert_eq!(
            file_id_from_node_id("Method:crates/x.cpp:save").as_deref(),
            Some("File:crates/x.cpp")
        );
        assert_eq!(
            file_id_from_node_id("Function:a/b.go:foo").as_deref(),
            Some("File:a/b.go")
        );
        assert_eq!(file_id_from_node_id("File:only").as_deref(), None); // no name segment
        assert_eq!(file_id_from_node_id("garbage").as_deref(), None);
    }

    #[test]
    fn test_repoint_orphan_call_sources() {
        let mut graph = KnowledgeGraph::new();
        let mk = |id: &str, label: NodeLabel| GraphNode {
            id: id.to_string(),
            label,
            properties: NodeProperties {
                file_path: "x.cpp".to_string(),
                ..Default::default()
            },
        };
        graph.add_node(mk("File:x.cpp", NodeLabel::File));
        graph.add_node(mk("Function:x.cpp:callee", NodeLabel::Function));
        graph.add_node(mk("Function:x.cpp:caller", NodeLabel::Function));
        let call = |id: &str, src: &str, tgt: &str| GraphRelationship {
            id: id.to_string(),
            source_id: src.to_string(),
            target_id: tgt.to_string(),
            rel_type: RelationshipType::Calls,
            confidence: 1.0,
            reason: "test".to_string(),
            step: None,
        };
        // Orphan: source node `Method:x.cpp:save` does not exist.
        graph.add_relationship(call(
            "calls_Method:x.cpp:save_Function:x.cpp:callee",
            "Method:x.cpp:save",
            "Function:x.cpp:callee",
        ));
        // Valid: source exists, must be left untouched.
        graph.add_relationship(call(
            "calls_valid",
            "Function:x.cpp:caller",
            "Function:x.cpp:callee",
        ));

        let fixed = repoint_orphan_call_sources(&mut graph);
        assert_eq!(fixed, 1, "one orphan re-pointed");
        // Orphan edge gone; a File-sourced edge created in its place.
        assert!(graph
            .get_relationship("calls_Method:x.cpp:save_Function:x.cpp:callee")
            .is_none());
        assert!(graph.iter_relationships().any(|r| {
            r.rel_type == RelationshipType::Calls
                && r.source_id == "File:x.cpp"
                && r.target_id == "Function:x.cpp:callee"
        }));
        // No CALLS edge has a missing source anymore.
        let ids: std::collections::HashSet<&str> =
            graph.iter_nodes().map(|n| n.id.as_str()).collect();
        assert!(graph
            .iter_relationships()
            .filter(|r| r.rel_type == RelationshipType::Calls)
            .all(|r| ids.contains(r.source_id.as_str())));
        // The valid edge survives.
        assert!(graph.get_relationship("calls_valid").is_some());
    }
}

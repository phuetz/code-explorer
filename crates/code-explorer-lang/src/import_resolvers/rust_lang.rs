//! Rust import resolver.
//!
//! Handles Rust `use` paths:
//! - `crate::` prefix — resolve from the crate root.
//! - `super::` prefix — resolve relative to parent module.
//! - `self::` prefix — resolve relative to current module.
//! - External crate paths (no special prefix) — try suffix match.
//!
//! Rust modules map to either `module_name.rs` or `module_name/mod.rs`.

use super::types::{ImportResult, ResolveCtx};
use super::utils;

/// Resolve a Rust `use` path.
pub fn resolve(raw_path: &str, file_path: &str, ctx: &ResolveCtx<'_>) -> ImportResult {
    let cleaned = utils::normalize_import_path(raw_path);

    if cleaned.is_empty() {
        return ImportResult::Unresolved;
    }

    // Split on `::` to get module path segments
    let segments: Vec<&str> = cleaned.split("::").collect();

    if segments.is_empty() {
        return ImportResult::Unresolved;
    }

    match segments[0] {
        "crate" => resolve_crate_path(&segments[1..], file_path, ctx),
        "super" => resolve_super_path(&segments, file_path, ctx),
        "self" => resolve_self_path(&segments[1..], file_path, ctx),
        _ => {
            // External crate or ambiguous — try as suffix
            let path = segments.join("/");
            resolve_rust_module(&path, ctx)
        }
    }
}

/// Resolve a `crate::` prefixed path from the crate root.
///
/// The crate root is inferred from the file path by finding the nearest
/// `src/` directory or `lib.rs`/`main.rs`.
fn resolve_crate_path(segments: &[&str], file_path: &str, ctx: &ResolveCtx<'_>) -> ImportResult {
    let crate_root = find_crate_root(file_path);
    let module_path = if crate_root.is_empty() {
        segments.join("/")
    } else {
        format!("{}/{}", crate_root, segments.join("/"))
    };

    resolve_rust_module(&module_path, ctx)
}

/// Resolve `super::` paths by walking up from the current module.
fn resolve_super_path(segments: &[&str], file_path: &str, ctx: &ResolveCtx<'_>) -> ImportResult {
    let mut dir = module_dir(file_path);

    // Count consecutive `super` segments
    let mut i = 0;
    while i < segments.len() && segments[i] == "super" {
        if let Some(pos) = dir.rfind('/') {
            dir = dir[..pos].to_string();
        } else {
            dir = String::new();
        }
        i += 1;
    }

    let remaining: Vec<&str> = segments[i..].to_vec();
    let module_path = if remaining.is_empty() {
        dir
    } else if dir.is_empty() {
        remaining.join("/")
    } else {
        format!("{}/{}", dir, remaining.join("/"))
    };

    resolve_rust_module(&module_path, ctx)
}

/// Resolve `self::` paths relative to the current module.
fn resolve_self_path(segments: &[&str], file_path: &str, ctx: &ResolveCtx<'_>) -> ImportResult {
    let dir = module_dir(file_path);
    let module_path = if segments.is_empty() {
        dir
    } else if dir.is_empty() {
        segments.join("/")
    } else {
        format!("{}/{}", dir, segments.join("/"))
    };

    resolve_rust_module(&module_path, ctx)
}

/// Try to resolve a Rust module path, checking both `path.rs` and `path/mod.rs`.
///
/// Also tries stripping the last segment (it might be a symbol name, not a module).
fn resolve_rust_module(module_path: &str, ctx: &ResolveCtx<'_>) -> ImportResult {
    // Try as a direct module file
    for candidate in rust_module_candidates(module_path) {
        if let Some(full) = ctx.suffix_index.get(&candidate) {
            return ImportResult::Files(vec![full.to_string()]);
        }
    }

    // The last segment might be a symbol name, not a module path.
    // Strip it and retry.
    if let Some(pos) = module_path.rfind('/') {
        let parent = &module_path[..pos];
        for candidate in rust_module_candidates(parent) {
            if let Some(full) = ctx.suffix_index.get(&candidate) {
                return ImportResult::Files(vec![full.to_string()]);
            }
        }
    }

    ImportResult::Unresolved
}

/// Generate candidate file paths for a Rust module path.
fn rust_module_candidates(module_path: &str) -> Vec<String> {
    vec![
        format!("{module_path}.rs"),
        format!("{module_path}/mod.rs"),
        format!("{module_path}/lib.rs"),
    ]
}

/// Find the crate root (src/ directory) from a file path.
///
/// Given `crates/my-crate/src/models/user.rs`, returns `crates/my-crate/src`.
fn find_crate_root(file_path: &str) -> String {
    // Look for the nearest `src/` in the path
    if let Some(pos) = file_path.rfind("/src/") {
        file_path[..pos + 4].to_string() // include "/src"
    } else if file_path.starts_with("src/") {
        "src".to_string()
    } else {
        String::new()
    }
}

/// Get the module directory for a Rust file.
///
/// - `src/models/user.rs` -> `src/models`
/// - `src/models/mod.rs` -> `src/models`
/// - `src/lib.rs` -> `src`
fn module_dir(file_path: &str) -> String {
    let dir = utils::file_dir(file_path);

    // If the file is `mod.rs`, the module dir is the containing directory
    // (which is already what file_dir returns for `src/models/mod.rs` -> `src/models`)
    dir.to_string()
}

#[cfg(test)]
mod tests {
    use super::super::types::{ImportConfigs, SuffixIndex};
    use super::*;
    use std::collections::HashSet;

    fn make_ctx<'a>(
        files: &'a [String],
        suffix_index: &'a SuffixIndex,
        configs: &'a ImportConfigs,
    ) -> ResolveCtx<'a> {
        let all_set: HashSet<String> = files.iter().cloned().collect();
        let all_set = Box::leak(Box::new(all_set));
        ResolveCtx {
            all_file_paths: all_set,
            all_file_list: files,
            normalized_file_list: files,
            suffix_index,
            configs,
        }
    }

    #[test]
    fn test_crate_path() {
        let files = vec!["src/models/user.rs".to_string(), "src/lib.rs".to_string()];
        let index = SuffixIndex::build(&files, &files);
        let configs = ImportConfigs::default();
        let ctx = make_ctx(&files, &index, &configs);

        match resolve("crate::models::user", "src/main.rs", &ctx) {
            ImportResult::Files(f) => assert_eq!(f, vec!["src/models/user.rs"]),
            other => panic!("Expected Files, got {:?}", other),
        }
    }

    #[test]
    fn test_super_path() {
        let files = vec![
            "src/models/user.rs".to_string(),
            "src/models/types.rs".to_string(),
            "src/utils/helper.rs".to_string(),
        ];
        let index = SuffixIndex::build(&files, &files);
        let configs = ImportConfigs::default();
        let ctx = make_ctx(&files, &index, &configs);

        match resolve("super::utils::helper", "src/models/user.rs", &ctx) {
            ImportResult::Files(f) => assert_eq!(f, vec!["src/utils/helper.rs"]),
            other => panic!("Expected Files, got {:?}", other),
        }
    }

    #[test]
    fn test_self_path() {
        let files = vec![
            "src/models/user.rs".to_string(),
            "src/models/types.rs".to_string(),
        ];
        let index = SuffixIndex::build(&files, &files);
        let configs = ImportConfigs::default();
        let ctx = make_ctx(&files, &index, &configs);

        match resolve("self::types", "src/models/user.rs", &ctx) {
            ImportResult::Files(f) => assert_eq!(f, vec!["src/models/types.rs"]),
            other => panic!("Expected Files, got {:?}", other),
        }
    }

    #[test]
    fn test_mod_rs_resolution() {
        let files = vec!["src/models/mod.rs".to_string()];
        let index = SuffixIndex::build(&files, &files);
        let configs = ImportConfigs::default();
        let ctx = make_ctx(&files, &index, &configs);

        match resolve("crate::models", "src/main.rs", &ctx) {
            ImportResult::Files(f) => assert_eq!(f, vec!["src/models/mod.rs"]),
            other => panic!("Expected Files, got {:?}", other),
        }
    }

    #[test]
    fn test_symbol_stripping() {
        // `crate::models::user::User` should resolve to user.rs even though User is a symbol
        let files = vec!["src/models/user.rs".to_string()];
        let index = SuffixIndex::build(&files, &files);
        let configs = ImportConfigs::default();
        let ctx = make_ctx(&files, &index, &configs);

        match resolve("crate::models::user::User", "src/main.rs", &ctx) {
            ImportResult::Files(f) => assert_eq!(f, vec!["src/models/user.rs"]),
            other => panic!("Expected Files, got {:?}", other),
        }
    }
}

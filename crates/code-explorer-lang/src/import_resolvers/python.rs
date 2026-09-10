//! Python import resolver.
//!
//! Handles:
//! - Absolute imports: `import foo.bar` / `from foo.bar import baz`
//! - PEP 328 relative imports: `from . import sibling`, `from ..pkg import mod`
//! - Proximity-based resolution: when multiple files match, prefer the closest one.

use super::types::{ImportResult, ResolveCtx};
use super::utils;

/// Resolve a Python import path.
///
/// Python imports use dot-separated module paths. Relative imports start with
/// one or more dots. The resolver converts dots to slashes and tries suffix
/// matching with `.py` and `/__init__.py` extensions.
pub fn resolve(raw_path: &str, file_path: &str, ctx: &ResolveCtx<'_>) -> ImportResult {
    let cleaned = utils::normalize_import_path(raw_path);

    if cleaned.is_empty() {
        return ImportResult::Unresolved;
    }

    // ── Relative imports (PEP 328) ───────────────────────────────────────
    if cleaned.starts_with('.') {
        return resolve_relative_import(&cleaned, file_path, ctx);
    }

    // ── Absolute imports ─────────────────────────────────────────────────
    resolve_absolute_import(&cleaned, ctx)
}

/// Resolve a PEP 328 relative import.
///
/// Leading dots indicate parent traversal:
/// - `.` = current package
/// - `..` = parent package
/// - `...` = grandparent package, etc.
fn resolve_relative_import(cleaned: &str, file_path: &str, ctx: &ResolveCtx<'_>) -> ImportResult {
    // Count leading dots
    let dot_count = cleaned.chars().take_while(|c| *c == '.').count();
    let remainder = &cleaned[dot_count..];

    // Start from the importing file's directory
    let mut dir = utils::file_dir(file_path).to_string();

    // Each dot (after the first) goes up one directory
    for _ in 1..dot_count {
        if let Some(pos) = dir.rfind('/') {
            dir = dir[..pos].to_string();
        } else {
            dir = String::new();
            break;
        }
    }

    // Convert remaining dots-separated module path to slash-separated
    let module_path = if remainder.is_empty() {
        dir.clone()
    } else {
        let relative_part = remainder.replace('.', "/");
        if dir.is_empty() {
            relative_part
        } else {
            format!("{dir}/{relative_part}")
        }
    };

    utils::resolve_by_suffix(&module_path, ctx)
}

/// Resolve an absolute Python import (dot-separated to slash-separated).
fn resolve_absolute_import(cleaned: &str, ctx: &ResolveCtx<'_>) -> ImportResult {
    let path = cleaned.replace('.', "/");
    utils::resolve_by_suffix(&path, ctx)
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
    fn test_absolute_import() {
        let files = vec![
            "myapp/models/user.py".to_string(),
            "myapp/__init__.py".to_string(),
        ];
        let index = SuffixIndex::build(&files, &files);
        let configs = ImportConfigs::default();
        let ctx = make_ctx(&files, &index, &configs);

        match resolve("myapp.models.user", "main.py", &ctx) {
            ImportResult::Files(f) => assert_eq!(f, vec!["myapp/models/user.py"]),
            other => panic!("Expected Files, got {:?}", other),
        }
    }

    #[test]
    fn test_relative_import_single_dot() {
        let files = vec![
            "myapp/models/user.py".to_string(),
            "myapp/models/types.py".to_string(),
        ];
        let index = SuffixIndex::build(&files, &files);
        let configs = ImportConfigs::default();
        let ctx = make_ctx(&files, &index, &configs);

        match resolve(".types", "myapp/models/user.py", &ctx) {
            ImportResult::Files(f) => assert_eq!(f, vec!["myapp/models/types.py"]),
            other => panic!("Expected Files, got {:?}", other),
        }
    }

    #[test]
    fn test_relative_import_double_dot() {
        let files = vec![
            "myapp/utils/helpers.py".to_string(),
            "myapp/models/user.py".to_string(),
        ];
        let index = SuffixIndex::build(&files, &files);
        let configs = ImportConfigs::default();
        let ctx = make_ctx(&files, &index, &configs);

        match resolve("..utils.helpers", "myapp/models/user.py", &ctx) {
            ImportResult::Files(f) => assert_eq!(f, vec!["myapp/utils/helpers.py"]),
            other => panic!("Expected Files, got {:?}", other),
        }
    }

    #[test]
    fn test_init_py_resolution() {
        let files = vec!["myapp/models/__init__.py".to_string()];
        let index = SuffixIndex::build(&files, &files);
        let configs = ImportConfigs::default();
        let ctx = make_ctx(&files, &index, &configs);

        match resolve("myapp.models", "main.py", &ctx) {
            ImportResult::Files(f) => assert_eq!(f, vec!["myapp/models/__init__.py"]),
            other => panic!("Expected Files, got {:?}", other),
        }
    }
}

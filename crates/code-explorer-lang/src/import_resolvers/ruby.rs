//! Ruby import resolver.
//!
//! Handles:
//! - `require` with gem/library names (external -- ignored)
//! - `require_relative` with file paths (relative to the requiring file)
//! - `require` with project-local paths (resolved via suffix matching)
//!
//! Note: Ruby `require` and `require_relative` calls are extracted via
//! call routing (see `call_routing.rs`), not via tree-sitter import queries.

use super::types::{ImportResult, ResolveCtx};
use super::utils;

/// Resolve a Ruby require/require_relative path.
pub fn resolve(raw_path: &str, file_path: &str, ctx: &ResolveCtx<'_>) -> ImportResult {
    let cleaned = utils::normalize_import_path(raw_path);

    if cleaned.is_empty() {
        return ImportResult::Unresolved;
    }

    // require_relative paths are always relative to the current file
    if utils::is_relative_path(&cleaned) {
        if let Some(resolved) = utils::resolve_relative_opt(&cleaned, file_path) {
            return resolve_rb_exact(&resolved, ctx);
        }
        return ImportResult::Unresolved;
    }

    // Plain require: try as a suffix
    resolve_rb(&cleaned, ctx)
}

/// Resolve with Ruby extension preference (.rb).
fn resolve_rb(path: &str, ctx: &ResolveCtx<'_>) -> ImportResult {
    // Try with .rb extension first
    let with_ext = format!("{path}.rb");
    if let Some(full) = ctx.suffix_index.get(&with_ext) {
        return ImportResult::Files(vec![full.to_string()]);
    }

    // Try exact match (path might already include extension)
    if let Some(full) = ctx.suffix_index.get(path) {
        return ImportResult::Files(vec![full.to_string()]);
    }

    // Fallback to general suffix resolution
    utils::resolve_by_suffix(path, ctx)
}

/// Resolve exactly with Ruby extension preference (.rb).
fn resolve_rb_exact(path: &str, ctx: &ResolveCtx<'_>) -> ImportResult {
    // Try with .rb extension first (case sensitive)
    let with_ext = format!("{path}.rb");
    if ctx.all_file_paths.contains(&with_ext) {
        return ImportResult::Files(vec![with_ext]);
    }

    // Try exact match (case sensitive)
    if ctx.all_file_paths.contains(path) {
        return ImportResult::Files(vec![path.to_string()]);
    }

    // Try with .rb extension first (case insensitive)
    if let Some(full) = ctx.all_file_list.iter().find(|p| p.eq_ignore_ascii_case(&with_ext)) {
        return ImportResult::Files(vec![full.to_string()]);
    }

    // Try exact match (case insensitive)
    if let Some(full) = ctx.all_file_list.iter().find(|p| p.eq_ignore_ascii_case(path)) {
        return ImportResult::Files(vec![full.to_string()]);
    }

    // Fallback to general exact resolution
    utils::resolve_exact(path, ctx)
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
    fn test_require_relative() {
        let files = vec![
            "lib/models/user.rb".to_string(),
            "lib/models/base.rb".to_string(),
        ];
        let index = SuffixIndex::build(&files, &files);
        let configs = ImportConfigs::default();
        let ctx = make_ctx(&files, &index, &configs);

        match resolve("./base", "lib/models/user.rb", &ctx) {
            ImportResult::Files(f) => assert_eq!(f, vec!["lib/models/base.rb"]),
            other => panic!("Expected Files, got {:?}", other),
        }
    }

    #[test]
    fn test_require_plain() {
        let files = vec!["lib/helpers.rb".to_string()];
        let index = SuffixIndex::build(&files, &files);
        let configs = ImportConfigs::default();
        let ctx = make_ctx(&files, &index, &configs);

        match resolve("helpers", "app.rb", &ctx) {
            ImportResult::Files(f) => assert_eq!(f, vec!["lib/helpers.rb"]),
            other => panic!("Expected Files, got {:?}", other),
        }
    }
    #[test]
    fn test_relative_import_false_positive_root() {
        let files = vec!["pkg/helper.rb".to_string(), "main.rb".to_string()];
        let index = SuffixIndex::build(&files, &files);
        let configs = ImportConfigs::default();
        let ctx = make_ctx(&files, &index, &configs);
        assert!(matches!(
            resolve("./helper", "main.rb", &ctx),
            ImportResult::Unresolved
        ));
    }
}

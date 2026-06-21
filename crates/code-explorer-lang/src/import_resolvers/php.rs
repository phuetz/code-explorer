//! PHP import resolver.
//!
//! Handles:
//! - `use` statements: `use App\Models\User`
//! - PSR-4 autoloading conventions (namespace prefix -> directory mapping from composer.json)
//! - `require`/`include` with string paths

use super::types::{ImportResult, ResolveCtx};
use super::utils;

/// Resolve a PHP import (use statement or require path).
pub fn resolve(raw_path: &str, file_path: &str, ctx: &ResolveCtx<'_>) -> ImportResult {
    let raw_trimmed = raw_path
        .trim()
        .trim_matches(|c: char| c == '\'' || c == '"' || c == '`')
        .trim()
        .trim_end_matches(';');

    if raw_trimmed.is_empty() {
        return ImportResult::Unresolved;
    }

    // Determine if this is a file path (require/include) or a namespace (use).
    // Check on the raw (un-normalized) input so backslash namespace separators
    // don't get confused with forward-slash file path separators.
    let is_file_path =
        (raw_trimmed.contains('/') && !raw_trimmed.contains('\\')) || raw_trimmed.ends_with(".php");

    if is_file_path {
        let cleaned = raw_trimmed.replace('\\', "/");
        // File path -- resolve relative or absolute
        if utils::is_relative_path(&cleaned) {
            let resolved = utils::resolve_relative(&cleaned, file_path);
            return utils::resolve_by_suffix(&resolved, ctx);
        }
        return utils::resolve_by_suffix(&cleaned, ctx);
    }

    // ── PSR-4 namespace resolution ───────────────────────────────────────
    // Convert backslash-separated namespace to slash-separated path
    let namespace_path = raw_trimmed.replace('\\', "/");

    // Try PSR-4 autoload mappings from composer.json
    if let Some(autoload) = &ctx.configs.php_autoload {
        for (prefix, dir) in autoload {
            let prefix_normalized = prefix.replace('\\', "/").trim_end_matches('/').to_string();
            if let Some(rest) = namespace_path.strip_prefix(&prefix_normalized) {
                // PSR-4 prefixes must match on a namespace boundary, not as a
                // raw substring: prefix `App` should NOT match `AppExtended`.
                // After stripping, the next char must be `/` (separator) or
                // empty (the prefix matched the entire namespace), otherwise
                // we'd silently rewrite an unrelated file path.
                if !rest.is_empty() && !rest.starts_with('/') {
                    continue;
                }
                let rest = rest.trim_start_matches('/');
                let candidate = if dir.is_empty() {
                    format!("{rest}.php")
                } else {
                    format!("{}/{rest}.php", dir.trim_end_matches('/'))
                };
                if let Some(file) = ctx.suffix_index.get(&candidate) {
                    return ImportResult::Files(vec![file.to_string()]);
                }
            }
        }
    }

    // Direct suffix match
    let candidate = format!("{namespace_path}.php");
    if let Some(file) = ctx.suffix_index.get(&candidate) {
        return ImportResult::Files(vec![file.to_string()]);
    }

    // Try last segment as class file
    if let Some(last) = namespace_path.rsplit('/').next() {
        let candidate = format!("{last}.php");
        if let Some(file) = ctx.suffix_index.get(&candidate) {
            return ImportResult::Files(vec![file.to_string()]);
        }
    }

    ImportResult::Unresolved
}

#[cfg(test)]
mod tests {
    use super::super::types::{ImportConfigs, SuffixIndex};
    use super::*;
    use std::collections::{HashMap, HashSet};

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
    fn test_psr4_resolution() {
        let files = vec!["src/Models/User.php".to_string()];
        let index = SuffixIndex::build(&files, &files);
        let mut autoload = HashMap::new();
        autoload.insert("App".to_string(), "src".to_string());
        let configs = ImportConfigs {
            php_autoload: Some(autoload),
            ..Default::default()
        };
        let ctx = make_ctx(&files, &index, &configs);

        match resolve("App\\Models\\User", "index.php", &ctx) {
            ImportResult::Files(f) => assert_eq!(f, vec!["src/Models/User.php"]),
            other => panic!("Expected Files, got {:?}", other),
        }
    }
}

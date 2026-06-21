//! Shared utilities for import resolution across all languages.
//!
//! Provides suffix-based file resolution, path normalization, and common
//! resolution strategies shared by multiple language resolvers.

use super::types::{ImportResult, ResolveCtx, RESOLVE_EXTENSIONS};

/// Resolve an import path by trying various file extensions via suffix matching.
///
/// This is the core resolution strategy: given a cleaned import path, try
/// appending each extension from [`RESOLVE_EXTENSIONS`] and look up the result
/// in the suffix index.
pub fn resolve_by_suffix<'a>(cleaned_path: &str, ctx: &ResolveCtx<'a>) -> ImportResult {
    for ext in RESOLVE_EXTENSIONS {
        let candidate = format!("{cleaned_path}{ext}");
        if ctx.all_file_paths.contains(&candidate) {
            return ImportResult::Files(vec![candidate]);
        }
        if let Some(full) = ctx.suffix_index.get(&candidate) {
            return ImportResult::Files(vec![full.to_string()]);
        }
    }
    ImportResult::Unresolved
}

/// Resolve by suffix with case-insensitive matching as a fallback.
pub fn resolve_by_suffix_insensitive<'a>(cleaned_path: &str, ctx: &ResolveCtx<'a>) -> ImportResult {
    // Try case-sensitive first
    let result = resolve_by_suffix(cleaned_path, ctx);
    if !matches!(result, ImportResult::Unresolved) {
        return result;
    }

    // Fall back to case-insensitive
    for ext in RESOLVE_EXTENSIONS {
        let candidate = format!("{cleaned_path}{ext}");
        if let Some(full) = ctx
            .all_file_list
            .iter()
            .find(|path| path.eq_ignore_ascii_case(&candidate))
        {
            return ImportResult::Files(vec![full.to_string()]);
        }
        if let Some(full) = ctx.suffix_index.get_insensitive(&candidate) {
            return ImportResult::Files(vec![full.to_string()]);
        }
    }
    ImportResult::Unresolved
}

/// Normalize an import path: trim quotes, whitespace, and clean separators.
pub fn normalize_import_path(raw: &str) -> String {
    raw.trim()
        .trim_matches(|c: char| c == '\'' || c == '"' || c == '`')
        .trim()
        .replace('\\', "/")
        .to_string()
}

/// Resolve a relative import path against the importing file's directory.
///
/// Given `file_path = "src/models/user.ts"` and `relative = "./types"`,
/// returns `"src/models/types"`.
pub fn resolve_relative(relative: &str, file_path: &str) -> String {
    let dir = file_dir(file_path);
    let mut parts: Vec<&str> = if dir.is_empty() {
        Vec::new()
    } else {
        dir.split('/').collect()
    };

    let clean = relative.trim_start_matches("./");
    for segment in clean.split('/') {
        match segment {
            ".." => {
                parts.pop();
            }
            "." | "" => {}
            s => parts.push(s),
        }
    }

    parts.join("/")
}

/// Get the directory portion of a file path.
pub fn file_dir(file_path: &str) -> &str {
    match file_path.rfind('/') {
        Some(pos) => &file_path[..pos],
        None => "",
    }
}

/// Check if an import path looks like a relative path (starts with `.` or `..`).
pub fn is_relative_path(path: &str) -> bool {
    path.starts_with("./") || path.starts_with("../") || path == "." || path == ".."
}

/// Resolve using TypeScript/JavaScript path aliases from tsconfig.json.
///
/// Tries each alias pattern against the import path. Patterns use `*` as a
/// wildcard (e.g., `@/*` maps to `src/*`).
pub fn resolve_ts_path_alias<'a>(import_path: &str, ctx: &ResolveCtx<'a>) -> ImportResult {
    let ts_paths = match &ctx.configs.ts_paths {
        Some(paths) => paths,
        None => return ImportResult::Unresolved,
    };
    let base_url = ctx.configs.ts_base_url.as_deref().unwrap_or("");

    for (pattern, targets) in ts_paths {
        if let Some(wildcard_match) = match_ts_pattern(pattern, import_path) {
            for target in targets {
                let resolved = target.replace('*', wildcard_match);
                if !base_url.is_empty() {
                    let full = if resolved.is_empty() {
                        base_url.to_string()
                    } else {
                        format!("{base_url}/{resolved}")
                    };
                    let result = resolve_by_suffix(&full, ctx);
                    if !matches!(result, ImportResult::Unresolved) {
                        return result;
                    }
                }

                let result = resolve_by_suffix(&resolved, ctx);
                if !matches!(result, ImportResult::Unresolved) {
                    return result;
                }
            }
        }
    }

    ImportResult::Unresolved
}

/// Match a tsconfig path pattern (e.g., `@/*`) against an import path.
/// Returns the wildcard-matched portion if successful.
fn match_ts_pattern<'a>(pattern: &str, import_path: &'a str) -> Option<&'a str> {
    if let Some(star_pos) = pattern.find('*') {
        let prefix = &pattern[..star_pos];
        let suffix = &pattern[star_pos + 1..];
        if import_path.starts_with(prefix) && import_path.ends_with(suffix) {
            let end = import_path.len() - suffix.len();
            if prefix.len() <= end {
                return Some(&import_path[prefix.len()..end]);
            }
        }
    } else if pattern == import_path {
        return Some("");
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_import_path() {
        assert_eq!(normalize_import_path("  './foo'  "), "./foo");
        assert_eq!(normalize_import_path("\"../bar\""), "../bar");
        assert_eq!(
            normalize_import_path("src\\models\\user"),
            "src/models/user"
        );
    }

    #[test]
    fn test_resolve_relative() {
        assert_eq!(
            resolve_relative("./types", "src/models/user.ts"),
            "src/models/types"
        );
        assert_eq!(
            resolve_relative("../utils/helper", "src/models/user.ts"),
            "src/utils/helper"
        );
        assert_eq!(
            resolve_relative("./sub/deep", "src/index.ts"),
            "src/sub/deep"
        );
    }

    #[test]
    fn test_file_dir() {
        assert_eq!(file_dir("src/models/user.ts"), "src/models");
        assert_eq!(file_dir("main.ts"), "");
    }

    #[test]
    fn test_is_relative_path() {
        assert!(is_relative_path("./foo"));
        assert!(is_relative_path("../foo"));
        assert!(!is_relative_path("@/foo"));
        assert!(!is_relative_path("lodash"));
    }

    #[test]
    fn test_match_ts_pattern() {
        assert_eq!(
            match_ts_pattern("@/*", "@/components/Button"),
            Some("components/Button")
        );
        assert_eq!(match_ts_pattern("~/*", "~/utils"), Some("utils"));
        assert_eq!(match_ts_pattern("@/*", "lodash"), None);
        assert_eq!(match_ts_pattern("exact-match", "exact-match"), Some(""));
    }
}

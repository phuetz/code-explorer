//! Standard import resolver for TypeScript and JavaScript.
//!
//! Resolution order:
//! 1. Relative paths (`./`, `../`) — resolve against the importing file's directory.
//! 2. Path aliases from tsconfig.json (`@/`, `~/`, custom patterns).
//! 3. Bare specifiers — try suffix match (node_modules are excluded from the file list).

use super::types::{ImportResult, ResolveCtx};
use super::utils;

/// Resolve a TypeScript/JavaScript import path.
///
/// Handles relative imports, tsconfig path aliases, and bare specifiers.
/// Index file resolution (e.g., `./components` -> `./components/index.ts`)
/// is handled automatically by the suffix extension list.
pub fn resolve(raw_path: &str, file_path: &str, ctx: &ResolveCtx<'_>) -> ImportResult {
    let cleaned = utils::normalize_import_path(raw_path);

    if cleaned.is_empty() {
        return ImportResult::Unresolved;
    }

    // ── 1. Relative imports ──────────────────────────────────────────────
    if utils::is_relative_path(&cleaned) {
        let resolved = utils::resolve_relative(&cleaned, file_path);
        return resolve_standard_path(&resolved, ctx);
    }

    // ── 2. Path aliases (tsconfig.json) ──────────────────────────────────
    let alias_result = utils::resolve_ts_path_alias(&cleaned, ctx);
    if !matches!(alias_result, ImportResult::Unresolved) {
        return alias_result;
    }
    if let Some(stripped) = strip_runtime_js_extension(&cleaned) {
        let alias_result = utils::resolve_ts_path_alias(stripped, ctx);
        if !matches!(alias_result, ImportResult::Unresolved) {
            return alias_result;
        }
    }

    if is_node_builtin(&cleaned) {
        return ImportResult::Unresolved;
    }

    // ── 3. Bare specifier (suffix match) ─────────────────────────────────
    // Try the import path directly as a suffix (works for monorepo packages
    // and project-local absolute imports configured via baseUrl).
    if let Some(base_url) = &ctx.configs.ts_base_url {
        let with_base = format!("{base_url}/{cleaned}");
        let result = resolve_standard_path(&with_base, ctx);
        if !matches!(result, ImportResult::Unresolved) {
            return result;
        }
    }

    // Try direct suffix match as last resort
    resolve_standard_path(&cleaned, ctx)
}

fn resolve_standard_path(path: &str, ctx: &ResolveCtx<'_>) -> ImportResult {
    let exact = utils::resolve_by_suffix(path, ctx);
    if !matches!(exact, ImportResult::Unresolved) {
        return exact;
    }

    if let Some(stripped) = strip_runtime_js_extension(path) {
        return utils::resolve_by_suffix(stripped, ctx);
    }

    ImportResult::Unresolved
}

fn strip_runtime_js_extension(path: &str) -> Option<&str> {
    [".js", ".jsx", ".mjs", ".cjs"]
        .iter()
        .find_map(|ext| path.strip_suffix(ext))
}

fn is_node_builtin(path: &str) -> bool {
    let path = path.strip_prefix("node:").unwrap_or(path);
    let root = path.split('/').next().unwrap_or(path);
    matches!(
        root,
        "assert"
            | "async_hooks"
            | "buffer"
            | "child_process"
            | "cluster"
            | "console"
            | "constants"
            | "crypto"
            | "dgram"
            | "diagnostics_channel"
            | "dns"
            | "domain"
            | "events"
            | "fs"
            | "http"
            | "http2"
            | "https"
            | "inspector"
            | "module"
            | "net"
            | "os"
            | "path"
            | "perf_hooks"
            | "process"
            | "punycode"
            | "querystring"
            | "readline"
            | "repl"
            | "stream"
            | "string_decoder"
            | "sys"
            | "test"
            | "timers"
            | "tls"
            | "trace_events"
            | "tty"
            | "url"
            | "util"
            | "v8"
            | "vm"
            | "wasi"
            | "worker_threads"
            | "zlib"
    )
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
        // Leak is fine in tests
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
    fn test_relative_import() {
        let files = vec![
            "src/models/user.ts".to_string(),
            "src/models/types.ts".to_string(),
        ];
        let index = SuffixIndex::build(&files, &files);
        let configs = ImportConfigs::default();
        let ctx = make_ctx(&files, &index, &configs);

        match resolve("./types", "src/models/user.ts", &ctx) {
            ImportResult::Files(f) => assert_eq!(f, vec!["src/models/types.ts"]),
            other => panic!("Expected Files, got {:?}", other),
        }
    }

    #[test]
    fn test_relative_js_extension_resolves_typescript_source() {
        let files = vec!["src/models/a.ts".to_string()];
        let index = SuffixIndex::build(&files, &files);
        let configs = ImportConfigs::default();
        let ctx = make_ctx(&files, &index, &configs);

        match resolve("./a.js", "src/models/b.ts", &ctx) {
            ImportResult::Files(f) => assert_eq!(f, vec!["src/models/a.ts"]),
            other => panic!("Expected Files, got {:?}", other),
        }
    }

    #[test]
    fn test_relative_js_extension_prefers_exact_path_over_suffix_collision() {
        let files = vec![
            "src/agent/middleware/types.ts".to_string(),
            "src/middleware/types.ts".to_string(),
        ];
        let index = SuffixIndex::build(&files, &files);
        let configs = ImportConfigs::default();
        let ctx = make_ctx(&files, &index, &configs);

        match resolve("./types.js", "src/middleware/pipeline.ts", &ctx) {
            ImportResult::Files(f) => assert_eq!(f, vec!["src/middleware/types.ts"]),
            other => panic!("Expected Files, got {:?}", other),
        }
    }

    #[test]
    fn test_path_alias() {
        let files = vec!["src/components/Button.tsx".to_string()];
        let index = SuffixIndex::build(&files, &files);
        let mut ts_paths = HashMap::new();
        ts_paths.insert("@/*".to_string(), vec!["src/*".to_string()]);
        let configs = ImportConfigs {
            ts_paths: Some(ts_paths),
            ts_base_url: None,
            ..Default::default()
        };
        let ctx = make_ctx(&files, &index, &configs);

        match resolve("@/components/Button", "src/pages/Home.tsx", &ctx) {
            ImportResult::Files(f) => assert_eq!(f, vec!["src/components/Button.tsx"]),
            other => panic!("Expected Files, got {:?}", other),
        }
    }

    #[test]
    fn test_unresolved() {
        let files = vec!["src/index.ts".to_string()];
        let index = SuffixIndex::build(&files, &files);
        let configs = ImportConfigs::default();
        let ctx = make_ctx(&files, &index, &configs);

        assert!(matches!(
            resolve("nonexistent-package", "src/index.ts", &ctx),
            ImportResult::Unresolved
        ));
    }

    #[test]
    fn test_node_builtin_does_not_resolve_to_local_suffix() {
        let files = vec!["src/platform/os.ts".to_string()];
        let index = SuffixIndex::build(&files, &files);
        let configs = ImportConfigs::default();
        let ctx = make_ctx(&files, &index, &configs);

        assert!(matches!(
            resolve("os", "src/main.ts", &ctx),
            ImportResult::Unresolved
        ));
        assert!(matches!(
            resolve("node:fs/promises", "src/main.ts", &ctx),
            ImportResult::Unresolved
        ));
    }
}

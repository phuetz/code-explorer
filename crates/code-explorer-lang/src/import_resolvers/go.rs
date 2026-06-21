//! Go import resolver.
//!
//! Handles Go package imports:
//! - Standard library packages (ignored — not in the file list).
//! - Module-relative imports (strip the go.mod module path prefix).
//! - Package directory resolution: a Go import path maps to a *directory*
//!   containing `.go` files, not a single file.

use super::types::{ImportResult, ResolveCtx};
use super::utils;

/// Resolve a Go import path.
///
/// Go imports reference packages (directories), not individual files. The
/// resolver strips the module prefix (from `go.mod`) and finds all `.go` files
/// in the target directory.
pub fn resolve(raw_path: &str, _file_path: &str, ctx: &ResolveCtx<'_>) -> ImportResult {
    let cleaned = utils::normalize_import_path(raw_path);

    if cleaned.is_empty() {
        return ImportResult::Unresolved;
    }

    // ── Strip module prefix ──────────────────────────────────────────────
    let local_path = if let Some(module_path) = &ctx.configs.go_module {
        if let Some(stripped) = cleaned.strip_prefix(module_path.as_str()) {
            stripped.trim_start_matches('/').to_string()
        } else {
            // External package or stdlib — not in our file list
            return ImportResult::Unresolved;
        }
    } else {
        // No go.mod info available. Without a module prefix we cannot
        // distinguish stdlib (`"log"`, `"net/http"`), third-party
        // (`"github.com/foo/bar"`), and project-local imports. If we
        // optimistically suffix-match, a project that happens to contain a
        // `log/` directory will get a spurious `Imports` edge from every
        // file that imports the stdlib `"log"` package. Reject anything
        // that looks like stdlib (no `.` in the first segment) or external
        // (first segment contains `.`) and only accept paths that look
        // unambiguously project-local — which without a module path is
        // essentially impossible. The conservative choice is to bail out.
        let first_segment = cleaned.split('/').next().unwrap_or("");
        if first_segment.contains('.') {
            // Looks like a domain (github.com/...), definitely external
            return ImportResult::Unresolved;
        }
        // Pure-identifier first segment without `.` — almost certainly
        // a stdlib package (`log`, `net`, `strings`, `os/exec`...).
        return ImportResult::Unresolved;
    };

    if local_path.is_empty() {
        return ImportResult::Unresolved;
    }

    // ── Directory-based resolution ───────────────────────────────────────
    // Go imports are package-level: find all .go files in the directory.
    let go_files = ctx
        .suffix_index
        .get_files_in_dir_with_ext(&local_path, ".go");

    if !go_files.is_empty() {
        // Filter out test files
        let non_test_files: Vec<String> = go_files
            .into_iter()
            .filter(|f| !f.ends_with("_test.go"))
            .map(|f| f.to_string())
            .collect();

        if !non_test_files.is_empty() {
            return ImportResult::Package {
                files: non_test_files,
                dir_suffix: local_path,
            };
        }
    }

    // Try last path segment as directory name (fallback)
    if let Some(last_seg) = cleaned.rsplit('/').next() {
        if last_seg != local_path {
            let files = ctx.suffix_index.get_files_in_dir_with_ext(last_seg, ".go");
            if !files.is_empty() {
                let non_test: Vec<String> = files
                    .into_iter()
                    .filter(|f| !f.ends_with("_test.go"))
                    .map(|f| f.to_string())
                    .collect();
                if !non_test.is_empty() {
                    return ImportResult::Package {
                        files: non_test,
                        dir_suffix: last_seg.to_string(),
                    };
                }
            }
        }
    }

    // Try as a single-file match (rare, but handles edge cases)
    utils::resolve_by_suffix(&local_path, ctx)
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
    fn test_module_relative_import() {
        let files = vec![
            "internal/handler/user.go".to_string(),
            "internal/handler/auth.go".to_string(),
            "internal/handler/auth_test.go".to_string(),
        ];
        let index = SuffixIndex::build(&files, &files);
        let configs = ImportConfigs {
            go_module: Some("github.com/user/repo".to_string()),
            ..Default::default()
        };
        let ctx = make_ctx(&files, &index, &configs);

        match resolve("github.com/user/repo/internal/handler", "cmd/main.go", &ctx) {
            ImportResult::Package { files, dir_suffix } => {
                assert_eq!(dir_suffix, "internal/handler");
                assert_eq!(files.len(), 2); // test file excluded
                assert!(files.iter().all(|f| !f.ends_with("_test.go")));
            }
            other => panic!("Expected Package, got {:?}", other),
        }
    }

    #[test]
    fn test_external_package() {
        let files = vec!["main.go".to_string()];
        let index = SuffixIndex::build(&files, &files);
        let configs = ImportConfigs {
            go_module: Some("github.com/user/repo".to_string()),
            ..Default::default()
        };
        let ctx = make_ctx(&files, &index, &configs);

        assert!(matches!(
            resolve("fmt", "main.go", &ctx),
            ImportResult::Unresolved
        ));
    }
}

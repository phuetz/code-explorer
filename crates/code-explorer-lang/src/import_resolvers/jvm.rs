//! JVM import resolver for Java and Kotlin.
//!
//! Handles:
//! - Fully qualified class imports: `import com.example.User`
//! - Wildcard imports: `import com.example.*`
//! - Static imports: `import static com.example.Utils.helper`
//! - Kotlin: same as Java plus aliased imports (`import com.example.User as U`)

use super::types::{ImportResult, ResolveCtx};
use super::utils;

/// Resolve a Java import path.
pub fn resolve_java(raw_path: &str, _file_path: &str, ctx: &ResolveCtx<'_>) -> ImportResult {
    resolve_jvm(raw_path, ctx, ".java")
}

/// Resolve a Kotlin import path.
pub fn resolve_kotlin(raw_path: &str, _file_path: &str, ctx: &ResolveCtx<'_>) -> ImportResult {
    resolve_jvm(raw_path, ctx, ".kt")
}

fn resolve_jvm(raw_path: &str, ctx: &ResolveCtx<'_>, primary_ext: &str) -> ImportResult {
    let cleaned = utils::normalize_import_path(raw_path);
    if cleaned.is_empty() {
        return ImportResult::Unresolved;
    }

    // Strip `static ` prefix for static imports
    let path = cleaned.strip_prefix("static ").unwrap_or(&cleaned);

    // Strip Kotlin alias: `com.example.User as U` -> `com.example.User`
    let path = if let Some(pos) = path.find(" as ") {
        &path[..pos]
    } else {
        path
    };

    // Strip semicolons FIRST, then wildcard, so an input like `com.example.*;`
    // (wildcard with stray trailing semicolon) doesn't end up keeping the
    // wildcard suffix and producing an unresolvable `com/example/*` path.
    let path = path.trim_end_matches(';').trim_end_matches(".*");

    let as_path = path.replace('.', "/");

    // Try direct file match with the primary extension
    let candidate = format!("{as_path}{primary_ext}");
    if let Some(file) = ctx.suffix_index.get(&candidate) {
        return ImportResult::Files(vec![file.to_string()]);
    }

    // Try other JVM extensions
    for ext in &[".java", ".kt", ".kts"] {
        if *ext == primary_ext {
            continue;
        }
        let candidate = format!("{as_path}{ext}");
        if let Some(file) = ctx.suffix_index.get(&candidate) {
            return ImportResult::Files(vec![file.to_string()]);
        }
    }

    // For wildcard imports or last-resort: resolve as package directory
    if let Some(files) = ctx.suffix_index.get_files_in_dir(&as_path) {
        let jvm_files: Vec<String> = files
            .iter()
            .filter(|f| f.ends_with(".java") || f.ends_with(".kt"))
            .cloned()
            .collect();
        if !jvm_files.is_empty() {
            return ImportResult::Package {
                files: jvm_files,
                dir_suffix: as_path,
            };
        }
    }

    ImportResult::Unresolved
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
    fn test_java_import() {
        let files = vec!["com/example/User.java".to_string()];
        let index = SuffixIndex::build(&files, &files);
        let configs = ImportConfigs::default();
        let ctx = make_ctx(&files, &index, &configs);

        match resolve_java("com.example.User", "Main.java", &ctx) {
            ImportResult::Files(f) => assert_eq!(f, vec!["com/example/User.java"]),
            other => panic!("Expected Files, got {:?}", other),
        }
    }

    #[test]
    fn test_kotlin_import() {
        let files = vec!["com/example/UserService.kt".to_string()];
        let index = SuffixIndex::build(&files, &files);
        let configs = ImportConfigs::default();
        let ctx = make_ctx(&files, &index, &configs);

        match resolve_kotlin("com.example.UserService", "Main.kt", &ctx) {
            ImportResult::Files(f) => assert_eq!(f, vec!["com/example/UserService.kt"]),
            other => panic!("Expected Files, got {:?}", other),
        }
    }
}

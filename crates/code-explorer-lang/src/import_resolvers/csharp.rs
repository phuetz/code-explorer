//! C# import resolver.
//!
//! Handles:
//! - `using` directives: `using System.Collections.Generic`
//! - `using static` directives
//! - `using` aliases: `using Alias = Full.Namespace.Type`
//!
//! C# namespaces map to directory structures similar to Java, but with
//! less strict enforcement. Resolution converts dot-separated namespaces
//! to slash-separated paths and tries suffix matching.

use super::types::{ImportResult, ResolveCtx};
use super::utils;

/// Resolve a C# using directive to source files.
pub fn resolve(raw_path: &str, _file_path: &str, ctx: &ResolveCtx<'_>) -> ImportResult {
    let cleaned = utils::normalize_import_path(raw_path);

    if cleaned.is_empty() {
        return ImportResult::Unresolved;
    }

    // Strip `static ` prefix
    let path = cleaned.strip_prefix("static ").unwrap_or(&cleaned);

    // Handle alias: `Alias = Full.Namespace.Type` -> take the RHS
    let path = if let Some(pos) = path.find(" = ") {
        path[pos + 3..].trim()
    } else {
        path
    };

    // Strip trailing semicolons
    let path = path.trim_end_matches(';');

    // Convert dot-separated namespace to slash-separated path
    let as_path = path.replace('.', "/");

    // Try direct file match
    let candidate = format!("{as_path}.cs");
    if let Some(file) = ctx.suffix_index.get(&candidate) {
        return ImportResult::Files(vec![file.to_string()]);
    }

    // Try as namespace -> directory of .cs files
    if let Some(files) = ctx.suffix_index.get_files_in_dir(&as_path) {
        let cs_files: Vec<String> = files
            .iter()
            .filter(|f| f.ends_with(".cs"))
            .cloned()
            .collect();
        if !cs_files.is_empty() {
            return ImportResult::Package {
                files: cs_files,
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
    fn test_csharp_using() {
        let files = vec!["Models/User.cs".to_string()];
        let index = SuffixIndex::build(&files, &files);
        let configs = ImportConfigs::default();
        let ctx = make_ctx(&files, &index, &configs);

        match resolve("Models.User", "Program.cs", &ctx) {
            ImportResult::Files(f) => assert_eq!(f, vec!["Models/User.cs"]),
            other => panic!("Expected Files, got {:?}", other),
        }
    }
}

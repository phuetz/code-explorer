//! Swift import resolver.
//!
//! Handles:
//! - `import Module` -- module-level imports (Swift Package Manager targets)
//! - `import struct Module.Type` / `import func Module.function` -- symbol-level
//!
//! Swift module imports differ from file-level imports in most other languages.
//! A Swift module typically corresponds to a target in Package.swift, containing
//! all `.swift` files in a directory. Resolution finds all `.swift` files
//! belonging to the imported module/target.

use super::types::{ImportResult, ResolveCtx};
use super::utils;

/// Resolve a Swift import to source files.
///
/// Swift imports are module-level. We try to find a directory matching the
/// module name and return all `.swift` files in it.
pub fn resolve(raw_path: &str, _file_path: &str, ctx: &ResolveCtx<'_>) -> ImportResult {
    let cleaned = utils::normalize_import_path(raw_path);

    if cleaned.is_empty() {
        return ImportResult::Unresolved;
    }

    // Strip symbol-level import prefixes: `import struct`, `import func`, etc.
    let module_name = cleaned
        .strip_prefix("struct ")
        .or_else(|| cleaned.strip_prefix("func "))
        .or_else(|| cleaned.strip_prefix("class "))
        .or_else(|| cleaned.strip_prefix("enum "))
        .or_else(|| cleaned.strip_prefix("protocol "))
        .or_else(|| cleaned.strip_prefix("typealias "))
        .unwrap_or(&cleaned);

    // Extract the module name (before any `.` for symbol-level imports)
    let module = module_name.split('.').next().unwrap_or(module_name);

    // Try to find a directory matching the module name (SPM convention:
    // Sources/<ModuleName>/*.swift)
    let sources_dir = format!("Sources/{module}");
    let swift_files = ctx
        .suffix_index
        .get_files_in_dir_with_ext(&sources_dir, ".swift");

    if !swift_files.is_empty() {
        return ImportResult::Package {
            files: swift_files.into_iter().map(|f| f.to_string()).collect(),
            dir_suffix: sources_dir,
        };
    }

    // Try the module name as a direct directory
    let direct_files = ctx.suffix_index.get_files_in_dir_with_ext(module, ".swift");

    if !direct_files.is_empty() {
        return ImportResult::Package {
            files: direct_files.into_iter().map(|f| f.to_string()).collect(),
            dir_suffix: module.to_string(),
        };
    }

    // Try as a single file
    utils::resolve_by_suffix(module, ctx)
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
    fn test_spm_module_import() {
        let files = vec![
            "Sources/MyLib/Foo.swift".to_string(),
            "Sources/MyLib/Bar.swift".to_string(),
        ];
        let index = SuffixIndex::build(&files, &files);
        let configs = ImportConfigs::default();
        let ctx = make_ctx(&files, &index, &configs);

        match resolve("MyLib", "Sources/MyApp/main.swift", &ctx) {
            ImportResult::Package { files, dir_suffix } => {
                assert_eq!(dir_suffix, "Sources/MyLib");
                assert_eq!(files.len(), 2);
            }
            other => panic!("Expected Package, got {:?}", other),
        }
    }
}

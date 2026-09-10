//! Razor import resolver.
//!
//! Handles:
//! - `@using` directives: `@using Namespace.SubNamespace`
//! - Standard C# `using` directives found in `_ViewImports.cshtml` or `@code` blocks
//! - `@inject` directives (resolved as type references)
//!
//! Delegates the core namespace-to-path resolution to the C# resolver since
//! Razor uses the same namespace semantics.

use super::types::{ImportResult, ResolveCtx};
use super::utils;

/// Resolve a Razor import directive to source files.
///
/// Accepts both plain C# using paths (`System.Collections.Generic`)
/// and Razor-prefixed forms (`@using My.Namespace`).
pub fn resolve(raw_path: &str, _file_path: &str, ctx: &ResolveCtx<'_>) -> ImportResult {
    let cleaned = utils::normalize_import_path(raw_path);

    if cleaned.is_empty() {
        return ImportResult::Unresolved;
    }

    // Strip Razor-specific prefixes
    let path = cleaned
        .strip_prefix("@using ")
        .or_else(|| cleaned.strip_prefix("@inject "))
        .unwrap_or(&cleaned);

    // Strip `static ` prefix (same as C#)
    let path = path.strip_prefix("static ").unwrap_or(path);

    // Handle alias: `Alias = Full.Namespace.Type`
    let path = if let Some(pos) = path.find(" = ") {
        path[pos + 3..].trim()
    } else {
        path
    };

    // For @inject directives, extract just the type name (first token)
    // e.g., "@inject IMyService MyService" → "IMyService"
    let path = path.split_whitespace().next().unwrap_or(path);

    // Strip trailing semicolons
    let path = path.trim_end_matches(';');

    // Convert dot-separated namespace to slash-separated path
    let as_path = path.replace('.', "/");

    // Try direct file match (.cs files)
    let candidate_cs = format!("{as_path}.cs");
    if let Some(file) = ctx.suffix_index.get(&candidate_cs) {
        return ImportResult::Files(vec![file.to_string()]);
    }

    // Try .cshtml and .razor file matches
    for ext in &[".cshtml", ".razor"] {
        let candidate = format!("{as_path}{ext}");
        if let Some(file) = ctx.suffix_index.get(&candidate) {
            return ImportResult::Files(vec![file.to_string()]);
        }
    }

    // Try as namespace → directory of .cs/.cshtml/.razor files
    if let Some(files) = ctx.suffix_index.get_files_in_dir(&as_path) {
        let relevant_files: Vec<String> = files
            .iter()
            .filter(|f| f.ends_with(".cs") || f.ends_with(".cshtml") || f.ends_with(".razor"))
            .cloned()
            .collect();
        if !relevant_files.is_empty() {
            return ImportResult::Package {
                files: relevant_files,
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
    fn test_razor_using_resolves_to_cs() {
        let files = vec!["Models/User.cs".to_string()];
        let index = SuffixIndex::build(&files, &files);
        let configs = ImportConfigs::default();
        let ctx = make_ctx(&files, &index, &configs);

        match resolve("@using Models.User", "Views/Home/Index.cshtml", &ctx) {
            ImportResult::Files(f) => assert_eq!(f, vec!["Models/User.cs"]),
            other => panic!("Expected Files, got {:?}", other),
        }
    }

    #[test]
    fn test_razor_using_plain_namespace() {
        let files = vec!["ViewModels/HomeViewModel.cs".to_string()];
        let index = SuffixIndex::build(&files, &files);
        let configs = ImportConfigs::default();
        let ctx = make_ctx(&files, &index, &configs);

        match resolve("ViewModels.HomeViewModel", "Views/Home/Index.cshtml", &ctx) {
            ImportResult::Files(f) => assert_eq!(f, vec!["ViewModels/HomeViewModel.cs"]),
            other => panic!("Expected Files, got {:?}", other),
        }
    }

    #[test]
    fn test_razor_inject_directive() {
        let files = vec!["Services/IMyService.cs".to_string()];
        let index = SuffixIndex::build(&files, &files);
        let configs = ImportConfigs::default();
        let ctx = make_ctx(&files, &index, &configs);

        // @inject IMyService MyService → should resolve the type "Services.IMyService"
        match resolve(
            "@inject Services.IMyService MyService",
            "Pages/Index.razor",
            &ctx,
        ) {
            ImportResult::Files(f) => assert_eq!(f, vec!["Services/IMyService.cs"]),
            other => panic!("Expected Files, got {:?}", other),
        }
    }

    #[test]
    fn test_razor_resolves_to_razor_files() {
        let files = vec!["Shared/NavMenu.razor".to_string()];
        let index = SuffixIndex::build(&files, &files);
        let configs = ImportConfigs::default();
        let ctx = make_ctx(&files, &index, &configs);

        match resolve("Shared.NavMenu", "Pages/Index.razor", &ctx) {
            ImportResult::Files(f) => assert_eq!(f, vec!["Shared/NavMenu.razor"]),
            other => panic!("Expected Files, got {:?}", other),
        }
    }
}

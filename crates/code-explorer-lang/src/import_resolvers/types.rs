use std::collections::{HashMap, HashSet};

/// Result of import resolution.
#[derive(Debug, Clone)]
pub enum ImportResult {
    /// Resolved to specific files.
    Files(Vec<String>),
    /// Resolved to a package directory (Go/C# style).
    Package {
        files: Vec<String>,
        dir_suffix: String,
    },
    /// Could not resolve.
    Unresolved,
}

/// Context for import resolution providing access to the project's file list
/// and various configuration.
pub struct ResolveCtx<'a> {
    /// All file paths in the project (normalized with forward slashes).
    pub all_file_paths: &'a HashSet<String>,
    /// All file paths as a sorted list.
    pub all_file_list: &'a [String],
    /// Normalized file list (lowercase on case-insensitive systems).
    pub normalized_file_list: &'a [String],
    /// Suffix index for O(1) path suffix matching.
    pub suffix_index: &'a SuffixIndex,
    /// Language-specific import configurations.
    pub configs: &'a ImportConfigs,
}

/// Suffix index for fast import path resolution.
///
/// Maps path suffixes to full file paths for O(1) lookup.
#[derive(Debug, Default)]
pub struct SuffixIndex {
    /// suffix → original file path
    index: HashMap<String, String>,
    /// lowercase suffix → original file path (case-insensitive)
    insensitive_index: HashMap<String, String>,
    /// directory suffix → files in that directory
    dir_index: HashMap<String, Vec<String>>,
}

impl SuffixIndex {
    /// Build suffix index from file lists.
    pub fn build(normalized_files: &[String], original_files: &[String]) -> Self {
        let mut index = HashMap::new();
        let mut insensitive_index = HashMap::new();
        let mut dir_index: HashMap<String, Vec<String>> = HashMap::new();

        for (norm, orig) in normalized_files.iter().zip(original_files.iter()) {
            // Index all suffixes of the path
            let parts: Vec<&str> = norm.split('/').collect();
            for i in 0..parts.len() {
                let suffix = parts[i..].join("/");
                // Only store first match (deterministic)
                index.entry(suffix.clone()).or_insert_with(|| orig.clone());
                insensitive_index
                    .entry(suffix.to_lowercase())
                    .or_insert_with(|| orig.clone());
            }

            // Index directory membership
            if let Some(dir_pos) = norm.rfind('/') {
                let dir = &norm[..dir_pos];
                let dir_parts: Vec<&str> = dir.split('/').collect();
                for i in 0..dir_parts.len() {
                    let dir_suffix = dir_parts[i..].join("/");
                    dir_index.entry(dir_suffix).or_default().push(orig.clone());
                }
            }
        }

        Self {
            index,
            insensitive_index,
            dir_index,
        }
    }

    /// Lookup by exact suffix.
    pub fn get(&self, suffix: &str) -> Option<&str> {
        self.index.get(suffix).map(|s| s.as_str())
    }

    /// Lookup by case-insensitive suffix.
    pub fn get_insensitive(&self, suffix: &str) -> Option<&str> {
        self.insensitive_index
            .get(&suffix.to_lowercase())
            .map(|s| s.as_str())
    }

    /// Get all files in a directory matching a suffix.
    pub fn get_files_in_dir(&self, dir_suffix: &str) -> Option<&[String]> {
        self.dir_index.get(dir_suffix).map(|v| v.as_slice())
    }

    /// Get files in directory filtered by extension.
    pub fn get_files_in_dir_with_ext(&self, dir_suffix: &str, ext: &str) -> Vec<&str> {
        self.dir_index
            .get(dir_suffix)
            .map(|files| {
                files
                    .iter()
                    .filter(|f| f.ends_with(ext))
                    .map(|f| f.as_str())
                    .collect()
            })
            .unwrap_or_default()
    }
}

/// Language-specific import configurations.
#[derive(Debug, Default)]
pub struct ImportConfigs {
    /// TypeScript: tsconfig.json paths mapping
    pub ts_paths: Option<HashMap<String, Vec<String>>>,
    /// TypeScript: tsconfig.json baseUrl
    pub ts_base_url: Option<String>,
    /// Go: go.mod module path
    pub go_module: Option<String>,
    /// PHP: composer.json PSR-4 autoload mappings
    pub php_autoload: Option<HashMap<String, String>>,
}

/// Common file extensions to try when resolving imports.
pub const RESOLVE_EXTENSIONS: &[&str] = &[
    "",
    ".tsx",
    ".ts",
    ".mts",
    ".cts",
    ".jsx",
    ".js",
    ".mjs",
    ".cjs",
    "/index.tsx",
    "/index.ts",
    "/index.mts",
    "/index.cts",
    "/index.jsx",
    "/index.js",
    "/index.mjs",
    "/index.cjs",
    ".py",
    "/__init__.py",
    ".go",
    ".rs",
    "/mod.rs",
    "/lib.rs",
    ".java",
    ".kt",
    ".cs",
    ".php",
    ".rb",
    ".swift",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_suffix_index_build_and_lookup() {
        let files = vec![
            "src/models/user.ts".to_string(),
            "src/utils/helper.ts".to_string(),
        ];
        let index = SuffixIndex::build(&files, &files);

        assert_eq!(index.get("user.ts"), Some("src/models/user.ts"));
        assert_eq!(index.get("models/user.ts"), Some("src/models/user.ts"));
        assert_eq!(index.get("src/models/user.ts"), Some("src/models/user.ts"));
        assert_eq!(index.get("nonexistent.ts"), None);
    }

    #[test]
    fn test_suffix_index_case_insensitive() {
        let files = vec!["src/Models/User.ts".to_string()];
        let normalized = vec!["src/models/user.ts".to_string()];
        let index = SuffixIndex::build(&normalized, &files);

        assert_eq!(
            index.get_insensitive("models/user.ts"),
            Some("src/Models/User.ts")
        );
    }

    #[test]
    fn test_suffix_index_dir_files() {
        let files = vec![
            "src/models/user.ts".to_string(),
            "src/models/repo.ts".to_string(),
            "src/utils/helper.ts".to_string(),
        ];
        let index = SuffixIndex::build(&files, &files);

        let model_files = index.get_files_in_dir("models").unwrap();
        assert_eq!(model_files.len(), 2);
    }
}

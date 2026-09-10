use std::collections::{HashMap, HashSet};

use code_explorer_core::config::languages::SupportedLanguage;
use code_explorer_core::graph::types::RelationshipType;

use crate::export_detection;
use crate::import_resolvers::swift as swift_resolver;
use crate::import_resolvers::types::{ImportResult, ResolveCtx};
use crate::provider::{ImportSemantics, LanguageProvider, MroStrategy};
use crate::queries;

pub struct SwiftProvider;

impl LanguageProvider for SwiftProvider {
    fn id(&self) -> SupportedLanguage {
        SupportedLanguage::Swift
    }

    fn extensions(&self) -> &'static [&'static str] {
        &[".swift"]
    }

    fn tree_sitter_queries(&self) -> &'static str {
        queries::swift::QUERIES
    }

    fn check_export(&self, node_text: &str, node_type: &str, ancestors: &[&str]) -> bool {
        export_detection::check_swift_export(node_text, node_type, ancestors)
    }

    fn resolve_import<'a>(
        &self,
        raw_path: &str,
        file_path: &str,
        ctx: &ResolveCtx<'a>,
    ) -> ImportResult {
        swift_resolver::resolve(raw_path, file_path, ctx)
    }

    fn import_semantics(&self) -> ImportSemantics {
        ImportSemantics::Wildcard
    }

    fn heritage_default_edge(&self) -> RelationshipType {
        RelationshipType::Implements
    }

    fn mro_strategy(&self) -> MroStrategy {
        MroStrategy::FirstWins
    }

    fn wire_implicit_imports(
        &self,
        language_files: &[String],
        import_map: &HashMap<String, HashSet<String>>,
        add_edge: &mut dyn FnMut(&str, &str),
    ) {
        wire_swift_implicit_imports(language_files, import_map, add_edge);
    }
}

/// Wire implicit imports between Swift files in the same SPM target.
///
/// In Swift, all files within the same SPM target (or Xcode target) can see
/// each other without explicit import statements. This creates implicit edges.
fn wire_swift_implicit_imports(
    language_files: &[String],
    import_map: &HashMap<String, HashSet<String>>,
    add_edge: &mut dyn FnMut(&str, &str),
) {
    // Group files by directory (proxy for SPM target).
    //
    // Paths reaching this function are normally already normalized to forward
    // slashes by the ingestion pipeline, but we cannot assume that — Swift
    // projects on Windows that bypass `ResolveCtx` normalization (e.g.,
    // through a different code path or a future caller) would otherwise
    // collapse every file into a single empty-directory group, producing
    // a quadratic explosion of false-positive implicit edges between
    // unrelated files. Accept either separator defensively.
    let mut dir_groups: HashMap<String, Vec<&str>> = HashMap::new();
    for file in language_files {
        let last_sep = file.rfind('/').max(file.rfind('\\'));
        let dir = last_sep.map(|i| &file[..i]).unwrap_or("");
        dir_groups.entry(dir.to_string()).or_default().push(file);
    }

    // For each group, create implicit edges between all pairs
    for files in dir_groups.values() {
        if files.len() < 2 || files.len() > 500 {
            continue; // Skip trivially small or pathologically large targets
        }
        for i in 0..files.len() {
            let already_imported = import_map.get(files[i]);
            for j in 0..files.len() {
                if i == j {
                    continue;
                }
                // Skip if already connected
                if let Some(imports) = already_imported {
                    if imports.contains(files[j]) {
                        continue;
                    }
                }
                add_edge(files[i], files[j]);
            }
        }
    }
}

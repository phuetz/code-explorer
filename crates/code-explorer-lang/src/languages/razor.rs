use code_explorer_core::config::languages::SupportedLanguage;

use crate::export_detection;
use crate::import_resolvers::razor as razor_resolver;
use crate::import_resolvers::types::{ImportResult, ResolveCtx};
use crate::named_bindings::{razor as razor_bindings, types::NamedBinding};
use crate::provider::{ImportSemantics, LanguageProvider, MroStrategy};
use crate::queries;

use once_cell::sync::Lazy;
use regex::Regex;

static INTERFACE_PATTERN: Lazy<Regex> = Lazy::new(|| Regex::new(r"^I[A-Z]").unwrap());

/// Language provider for Razor files (.cshtml / .razor).
///
/// Razor files are mixed-content templates containing HTML, C#, and optionally
/// JavaScript. This provider focuses on extracting C# symbols using the C#
/// tree-sitter grammar and delegates Razor-specific directive extraction
/// (e.g., `@page`, `@model`, `@inject`) to the component detection module.
///
/// Design decision: reuse the C# grammar instead of requiring a separate
/// tree-sitter-razor grammar, since the C# portions of Razor files parse
/// correctly and we handle Razor directives via regex preprocessing.
pub struct RazorProvider;

impl LanguageProvider for RazorProvider {
    fn id(&self) -> SupportedLanguage {
        SupportedLanguage::Razor
    }

    fn extensions(&self) -> &'static [&'static str] {
        &[".cshtml", ".razor"]
    }

    fn tree_sitter_queries(&self) -> &'static str {
        queries::razor::QUERIES
    }

    fn check_export(&self, node_text: &str, node_type: &str, ancestors: &[&str]) -> bool {
        // Razor components: @page directive makes the component routable/public.
        // For C# symbols inside @code blocks, use the same rules as C#.
        // In Razor, types defined in @code blocks without access modifiers
        // are effectively internal to the component. We treat public/internal
        // the same as in C#.
        export_detection::check_razor_export(node_text, node_type, ancestors)
    }

    fn resolve_import<'a>(
        &self,
        raw_path: &str,
        file_path: &str,
        ctx: &ResolveCtx<'a>,
    ) -> ImportResult {
        razor_resolver::resolve(raw_path, file_path, ctx)
    }

    fn import_semantics(&self) -> ImportSemantics {
        ImportSemantics::Named
    }

    fn extract_named_bindings(&self, import_text: &str) -> Option<Vec<NamedBinding>> {
        razor_bindings::extract(import_text)
    }

    fn interface_name_pattern(&self) -> Option<&regex::Regex> {
        Some(&INTERFACE_PATTERN)
    }

    fn mro_strategy(&self) -> MroStrategy {
        MroStrategy::ImplementsSplit
    }

    fn is_route_file(&self, path: &str) -> bool {
        // Razor Pages (.cshtml in Pages/ directory) are route definitions
        let path_lower = path.to_lowercase();
        (path_lower.contains("/pages/") || path_lower.starts_with("pages/"))
            && path_lower.ends_with(".cshtml")
    }
}

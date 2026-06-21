use code_explorer_core::config::languages::SupportedLanguage;

use crate::export_detection;
use crate::import_resolvers::csharp as cs_resolver;
use crate::import_resolvers::types::{ImportResult, ResolveCtx};
use crate::named_bindings::{csharp as cs_bindings, types::NamedBinding};
use crate::provider::{ImportSemantics, LanguageProvider, MroStrategy};
use crate::queries;

use once_cell::sync::Lazy;
use regex::Regex;

static INTERFACE_PATTERN: Lazy<Regex> = Lazy::new(|| Regex::new(r"^I[A-Z]").unwrap());

pub struct CSharpProvider;

impl LanguageProvider for CSharpProvider {
    fn id(&self) -> SupportedLanguage {
        SupportedLanguage::CSharp
    }

    fn extensions(&self) -> &'static [&'static str] {
        &[".cs"]
    }

    fn tree_sitter_queries(&self) -> &'static str {
        queries::csharp::QUERIES
    }

    fn check_export(&self, node_text: &str, node_type: &str, ancestors: &[&str]) -> bool {
        export_detection::check_csharp_export(node_text, node_type, ancestors)
    }

    fn resolve_import<'a>(
        &self,
        raw_path: &str,
        file_path: &str,
        ctx: &ResolveCtx<'a>,
    ) -> ImportResult {
        cs_resolver::resolve(raw_path, file_path, ctx)
    }

    fn import_semantics(&self) -> ImportSemantics {
        ImportSemantics::Named
    }

    fn extract_named_bindings(&self, import_text: &str) -> Option<Vec<NamedBinding>> {
        cs_bindings::extract(import_text)
    }

    fn interface_name_pattern(&self) -> Option<&regex::Regex> {
        Some(&INTERFACE_PATTERN)
    }

    fn mro_strategy(&self) -> MroStrategy {
        MroStrategy::ImplementsSplit
    }
}

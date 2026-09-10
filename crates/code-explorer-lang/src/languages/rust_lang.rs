use code_explorer_core::config::languages::SupportedLanguage;

use crate::export_detection;
use crate::import_resolvers::rust_lang as rust_resolver;
use crate::import_resolvers::types::{ImportResult, ResolveCtx};
use crate::named_bindings::{rust_lang as rust_bindings, types::NamedBinding};
use crate::provider::{ImportSemantics, LanguageProvider, MroStrategy};
use crate::queries;

pub struct RustProvider;

impl LanguageProvider for RustProvider {
    fn id(&self) -> SupportedLanguage {
        SupportedLanguage::Rust
    }

    fn extensions(&self) -> &'static [&'static str] {
        &[".rs"]
    }

    fn tree_sitter_queries(&self) -> &'static str {
        queries::rust_lang::QUERIES
    }

    fn check_export(&self, node_text: &str, node_type: &str, ancestors: &[&str]) -> bool {
        export_detection::check_rust_export(node_text, node_type, ancestors)
    }

    fn resolve_import<'a>(
        &self,
        raw_path: &str,
        file_path: &str,
        ctx: &ResolveCtx<'a>,
    ) -> ImportResult {
        rust_resolver::resolve(raw_path, file_path, ctx)
    }

    fn import_semantics(&self) -> ImportSemantics {
        ImportSemantics::Named
    }

    fn extract_named_bindings(&self, import_text: &str) -> Option<Vec<NamedBinding>> {
        rust_bindings::extract(import_text)
    }

    fn mro_strategy(&self) -> MroStrategy {
        MroStrategy::QualifiedSyntax
    }
}

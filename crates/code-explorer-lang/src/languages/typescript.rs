use code_explorer_core::config::languages::SupportedLanguage;

use crate::export_detection;
use crate::import_resolvers::standard;
use crate::import_resolvers::types::{ImportResult, ResolveCtx};
use crate::named_bindings::{types::NamedBinding, typescript as ts_bindings};
use crate::provider::{ImportSemantics, LanguageProvider, MroStrategy};
use crate::queries;

pub struct TypeScriptProvider;

impl LanguageProvider for TypeScriptProvider {
    fn id(&self) -> SupportedLanguage {
        SupportedLanguage::TypeScript
    }

    fn extensions(&self) -> &'static [&'static str] {
        &[".ts", ".tsx", ".mts", ".cts"]
    }

    fn tree_sitter_queries(&self) -> &'static str {
        queries::typescript::QUERIES
    }

    fn check_export(&self, node_text: &str, node_type: &str, ancestors: &[&str]) -> bool {
        export_detection::check_ts_export(node_text, node_type, ancestors)
    }

    fn resolve_import<'a>(
        &self,
        raw_path: &str,
        file_path: &str,
        ctx: &ResolveCtx<'a>,
    ) -> ImportResult {
        standard::resolve(raw_path, file_path, ctx)
    }

    fn import_semantics(&self) -> ImportSemantics {
        ImportSemantics::Named
    }

    fn extract_named_bindings(&self, import_text: &str) -> Option<Vec<NamedBinding>> {
        ts_bindings::extract(import_text)
    }

    fn mro_strategy(&self) -> MroStrategy {
        MroStrategy::FirstWins
    }
}

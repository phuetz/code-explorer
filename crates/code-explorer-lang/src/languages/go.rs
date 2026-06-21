use code_explorer_core::config::languages::SupportedLanguage;

use crate::export_detection;
use crate::import_resolvers::go as go_resolver;
use crate::import_resolvers::types::{ImportResult, ResolveCtx};
use crate::provider::{ImportSemantics, LanguageProvider, MroStrategy};
use crate::queries;

pub struct GoProvider;

impl LanguageProvider for GoProvider {
    fn id(&self) -> SupportedLanguage {
        SupportedLanguage::Go
    }

    fn extensions(&self) -> &'static [&'static str] {
        &[".go"]
    }

    fn tree_sitter_queries(&self) -> &'static str {
        queries::go::QUERIES
    }

    fn check_export(&self, node_text: &str, node_type: &str, ancestors: &[&str]) -> bool {
        export_detection::check_go_export(node_text, node_type, ancestors)
    }

    fn resolve_import<'a>(
        &self,
        raw_path: &str,
        file_path: &str,
        ctx: &ResolveCtx<'a>,
    ) -> ImportResult {
        go_resolver::resolve(raw_path, file_path, ctx)
    }

    fn import_semantics(&self) -> ImportSemantics {
        ImportSemantics::Wildcard
    }

    fn mro_strategy(&self) -> MroStrategy {
        MroStrategy::FirstWins
    }
}

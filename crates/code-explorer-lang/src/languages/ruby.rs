use code_explorer_core::config::languages::SupportedLanguage;

use crate::call_routing::{self, CallRoutingResult};
use crate::export_detection;
use crate::import_resolvers::ruby as ruby_resolver;
use crate::import_resolvers::types::{ImportResult, ResolveCtx};
use crate::provider::{ImportSemantics, LanguageProvider, MroStrategy};
use crate::queries;

pub struct RubyProvider;

impl LanguageProvider for RubyProvider {
    fn id(&self) -> SupportedLanguage {
        SupportedLanguage::Ruby
    }

    fn extensions(&self) -> &'static [&'static str] {
        &[".rb", ".rake", ".gemspec"]
    }

    fn tree_sitter_queries(&self) -> &'static str {
        queries::ruby::QUERIES
    }

    fn check_export(&self, node_text: &str, node_type: &str, ancestors: &[&str]) -> bool {
        export_detection::check_ruby_export(node_text, node_type, ancestors)
    }

    fn resolve_import<'a>(
        &self,
        raw_path: &str,
        file_path: &str,
        ctx: &ResolveCtx<'a>,
    ) -> ImportResult {
        ruby_resolver::resolve(raw_path, file_path, ctx)
    }

    fn import_semantics(&self) -> ImportSemantics {
        ImportSemantics::Wildcard
    }

    fn route_call(&self, called_name: &str, call_text: &str) -> Option<CallRoutingResult> {
        call_routing::route_ruby_call(called_name, call_text)
    }

    fn mro_strategy(&self) -> MroStrategy {
        MroStrategy::FirstWins
    }
}

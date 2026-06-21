use code_explorer_core::config::languages::SupportedLanguage;

use crate::export_detection;
use crate::import_resolvers::jvm;
use crate::import_resolvers::types::{ImportResult, ResolveCtx};
use crate::named_bindings::{java as java_bindings, types::NamedBinding};
use crate::provider::{ImportSemantics, LanguageProvider, MroStrategy};
use crate::queries;

use once_cell::sync::Lazy;
use regex::Regex;

static INTERFACE_PATTERN: Lazy<Regex> = Lazy::new(|| Regex::new(r"^I[A-Z]").unwrap());

pub struct JavaProvider;

impl LanguageProvider for JavaProvider {
    fn id(&self) -> SupportedLanguage {
        SupportedLanguage::Java
    }

    fn extensions(&self) -> &'static [&'static str] {
        &[".java"]
    }

    fn tree_sitter_queries(&self) -> &'static str {
        queries::java::QUERIES
    }

    fn check_export(&self, node_text: &str, node_type: &str, ancestors: &[&str]) -> bool {
        export_detection::check_java_export(node_text, node_type, ancestors)
    }

    fn resolve_import<'a>(
        &self,
        raw_path: &str,
        file_path: &str,
        ctx: &ResolveCtx<'a>,
    ) -> ImportResult {
        jvm::resolve_java(raw_path, file_path, ctx)
    }

    fn import_semantics(&self) -> ImportSemantics {
        ImportSemantics::Named
    }

    fn extract_named_bindings(&self, import_text: &str) -> Option<Vec<NamedBinding>> {
        java_bindings::extract(import_text)
    }

    fn interface_name_pattern(&self) -> Option<&regex::Regex> {
        Some(&INTERFACE_PATTERN)
    }

    fn mro_strategy(&self) -> MroStrategy {
        MroStrategy::ImplementsSplit
    }
}

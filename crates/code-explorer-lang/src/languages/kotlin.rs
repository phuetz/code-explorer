use code_explorer_core::config::languages::SupportedLanguage;
use code_explorer_core::graph::types::NodeLabel;

use crate::export_detection;
use crate::import_resolvers::jvm;
use crate::import_resolvers::types::{ImportResult, ResolveCtx};
use crate::named_bindings::{kotlin as kt_bindings, types::NamedBinding};
use crate::provider::{ImportSemantics, LanguageProvider, MroStrategy};
use crate::queries;

pub struct KotlinProvider;

impl LanguageProvider for KotlinProvider {
    fn id(&self) -> SupportedLanguage {
        SupportedLanguage::Kotlin
    }

    fn extensions(&self) -> &'static [&'static str] {
        &[".kt", ".kts"]
    }

    fn tree_sitter_queries(&self) -> &'static str {
        queries::kotlin::QUERIES
    }

    fn check_export(&self, node_text: &str, node_type: &str, ancestors: &[&str]) -> bool {
        export_detection::check_kotlin_export(node_text, node_type, ancestors)
    }

    fn resolve_import<'a>(
        &self,
        raw_path: &str,
        file_path: &str,
        ctx: &ResolveCtx<'a>,
    ) -> ImportResult {
        jvm::resolve_kotlin(raw_path, file_path, ctx)
    }

    fn import_semantics(&self) -> ImportSemantics {
        ImportSemantics::Named
    }

    fn extract_named_bindings(&self, import_text: &str) -> Option<Vec<NamedBinding>> {
        kt_bindings::extract(import_text)
    }

    fn label_override(&self, _node_type: &str, default_label: NodeLabel) -> Option<NodeLabel> {
        // Kotlin: function_declaration inside class body should be Method, not Function.
        // Full check requires AST parent inspection (done in pipeline Phase 2).
        Some(default_label)
    }

    fn mro_strategy(&self) -> MroStrategy {
        MroStrategy::ImplementsSplit
    }
}

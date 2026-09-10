use code_explorer_core::config::languages::SupportedLanguage;
use code_explorer_core::graph::types::NodeLabel;

use crate::export_detection;
use crate::import_resolvers::standard;
use crate::import_resolvers::types::{ImportResult, ResolveCtx};
use crate::languages::c::cpp_label_override;
use crate::provider::{ImportSemantics, LanguageProvider, MroStrategy};
use crate::queries;

pub struct CppProvider;

impl LanguageProvider for CppProvider {
    fn id(&self) -> SupportedLanguage {
        SupportedLanguage::CPlusPlus
    }

    fn extensions(&self) -> &'static [&'static str] {
        &[".cpp", ".hpp", ".cc", ".hh", ".cxx", ".hxx"]
    }

    fn tree_sitter_queries(&self) -> &'static str {
        queries::cpp::QUERIES
    }

    fn check_export(&self, node_text: &str, node_type: &str, ancestors: &[&str]) -> bool {
        export_detection::check_c_cpp_export(node_text, node_type, ancestors)
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
        ImportSemantics::Wildcard
    }

    fn label_override(&self, node_type: &str, default_label: NodeLabel) -> Option<NodeLabel> {
        cpp_label_override(node_type, default_label)
    }

    fn mro_strategy(&self) -> MroStrategy {
        MroStrategy::LeftmostBase
    }
}

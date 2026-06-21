use code_explorer_core::config::languages::SupportedLanguage;

use crate::export_detection;
use crate::import_resolvers::python as py_resolver;
use crate::import_resolvers::types::{ImportResult, ResolveCtx};
use crate::named_bindings::{python as py_bindings, types::NamedBinding};
use crate::provider::{ImportSemantics, LanguageProvider, MroStrategy};
use crate::queries;

pub struct PythonProvider;

impl LanguageProvider for PythonProvider {
    fn id(&self) -> SupportedLanguage {
        SupportedLanguage::Python
    }

    fn extensions(&self) -> &'static [&'static str] {
        &[".py", ".pyi"]
    }

    fn tree_sitter_queries(&self) -> &'static str {
        queries::python::QUERIES
    }

    fn check_export(&self, node_text: &str, node_type: &str, ancestors: &[&str]) -> bool {
        export_detection::check_python_export(node_text, node_type, ancestors)
    }

    fn resolve_import<'a>(
        &self,
        raw_path: &str,
        file_path: &str,
        ctx: &ResolveCtx<'a>,
    ) -> ImportResult {
        py_resolver::resolve(raw_path, file_path, ctx)
    }

    fn import_semantics(&self) -> ImportSemantics {
        ImportSemantics::Namespace
    }

    fn extract_named_bindings(&self, import_text: &str) -> Option<Vec<NamedBinding>> {
        py_bindings::extract(import_text)
    }

    fn mro_strategy(&self) -> MroStrategy {
        MroStrategy::C3
    }
}

use code_explorer_core::config::languages::SupportedLanguage;
use code_explorer_core::graph::types::NodeLabel;

use crate::export_detection;
use crate::import_resolvers::php as php_resolver;
use crate::import_resolvers::types::{ImportResult, ResolveCtx};
use crate::named_bindings::{php as php_bindings, types::NamedBinding};
use crate::provider::{ImportSemantics, LanguageProvider, MroStrategy};
use crate::queries;

pub struct PhpProvider;

impl LanguageProvider for PhpProvider {
    fn id(&self) -> SupportedLanguage {
        SupportedLanguage::Php
    }

    fn extensions(&self) -> &'static [&'static str] {
        &[".php"]
    }

    fn tree_sitter_queries(&self) -> &'static str {
        queries::php::QUERIES
    }

    fn check_export(&self, node_text: &str, node_type: &str, ancestors: &[&str]) -> bool {
        export_detection::check_php_export(node_text, node_type, ancestors)
    }

    fn resolve_import<'a>(
        &self,
        raw_path: &str,
        file_path: &str,
        ctx: &ResolveCtx<'a>,
    ) -> ImportResult {
        php_resolver::resolve(raw_path, file_path, ctx)
    }

    fn import_semantics(&self) -> ImportSemantics {
        ImportSemantics::Named
    }

    fn extract_named_bindings(&self, import_text: &str) -> Option<Vec<NamedBinding>> {
        php_bindings::extract(import_text)
    }

    fn extract_description(&self, label: NodeLabel, name: &str, node_text: &str) -> Option<String> {
        extract_php_description(label, name, node_text)
    }

    fn is_route_file(&self, path: &str) -> bool {
        path.contains("/routes/") || path.starts_with("routes/")
    }

    fn mro_strategy(&self) -> MroStrategy {
        MroStrategy::FirstWins
    }
}

/// Extract PHP Eloquent model descriptions.
fn extract_php_description(label: NodeLabel, _name: &str, node_text: &str) -> Option<String> {
    if label != NodeLabel::Property {
        return None;
    }

    // Detect Eloquent array properties
    const ELOQUENT_PROPS: &[&str] = &["fillable", "casts", "hidden", "guarded", "with", "appends"];
    for prop in ELOQUENT_PROPS {
        if node_text.contains(prop) && node_text.contains('[') {
            return Some(format!("Eloquent ${prop}"));
        }
    }

    // Detect Eloquent relationship methods
    const RELATIONS: &[&str] = &[
        "hasMany",
        "hasOne",
        "belongsTo",
        "belongsToMany",
        "morphMany",
        "morphOne",
        "morphTo",
        "morphToMany",
    ];
    for rel in RELATIONS {
        if node_text.contains(rel) {
            return Some(format!("Eloquent {rel} relationship"));
        }
    }

    None
}

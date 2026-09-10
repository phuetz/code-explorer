use code_explorer_core::config::languages::SupportedLanguage;
use code_explorer_core::graph::types::{NodeLabel, RelationshipType};

use crate::call_routing::CallRoutingResult;
use crate::import_resolvers::types::{ImportResult, ResolveCtx};
use crate::named_bindings::types::NamedBinding;

/// Method Resolution Order strategy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MroStrategy {
    /// First match wins (TypeScript, JavaScript, Go, PHP, Ruby, C, Swift)
    FirstWins,
    /// Python C3 linearization
    C3,
    /// C++ left-to-right base class ordering
    LeftmostBase,
    /// Java/C#/Kotlin: class-over-interface split
    ImplementsSplit,
    /// Rust: trait qualification (no traditional MRO)
    QualifiedSyntax,
}

/// Import semantics for the language.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImportSemantics {
    /// Named imports: `import { X } from 'y'` (TS, JS, Java, C#, Rust, PHP, Kotlin)
    Named,
    /// Wildcard imports: all symbols visible (Go, C, C++, Ruby, Swift)
    Wildcard,
    /// Namespace imports: `import module` (Python)
    Namespace,
}

/// Language provider trait - defines all language-specific behavior.
///
/// Each of the 13 supported languages implements this trait.
/// Uses default methods for optional capabilities.
pub trait LanguageProvider: Send + Sync {
    // ── Identity ──────────────────────────────────────────────────
    fn id(&self) -> SupportedLanguage;
    fn extensions(&self) -> &'static [&'static str];

    // ── Tree-sitter ───────────────────────────────────────────────
    fn tree_sitter_queries(&self) -> &'static str;

    // ── Core (required behavior) ──────────────────────────────────

    /// Check if a symbol at the given AST node is exported.
    fn check_export(&self, node_text: &str, node_type: &str, ancestors: &[&str]) -> bool;

    /// Resolve an import path to file(s).
    fn resolve_import<'a>(
        &self,
        raw_path: &str,
        file_path: &str,
        ctx: &ResolveCtx<'a>,
    ) -> ImportResult;

    // ── Import semantics ──────────────────────────────────────────
    fn import_semantics(&self) -> ImportSemantics {
        ImportSemantics::Named
    }

    /// Preprocess import path before resolution (e.g., Kotlin wildcard appending).
    fn preprocess_import_path(&self, cleaned: &str) -> Option<String> {
        let _ = cleaned;
        None
    }

    // ── Named bindings ────────────────────────────────────────────

    /// Extract named import bindings from an import statement.
    /// Returns None if this language doesn't have named imports.
    fn extract_named_bindings(&self, _import_text: &str) -> Option<Vec<NamedBinding>> {
        None
    }

    // ── Call routing ──────────────────────────────────────────────

    /// Route a call expression to a specific action (import, heritage, property, skip).
    /// Only Ruby uses this in practice.
    fn route_call(&self, _called_name: &str, _call_text: &str) -> Option<CallRoutingResult> {
        None
    }

    // ── Labels ────────────────────────────────────────────────────

    /// Override the default label for a definition node.
    /// Return None to skip the node, Some(label) to use that label.
    fn label_override(&self, _node_type: &str, default_label: NodeLabel) -> Option<NodeLabel> {
        Some(default_label)
    }

    // ── Heritage & MRO ────────────────────────────────────────────

    /// Default relationship type for heritage edges (EXTENDS or IMPLEMENTS).
    fn heritage_default_edge(&self) -> RelationshipType {
        RelationshipType::Extends
    }

    /// Regex pattern for interface names (e.g., /^I[A-Z]/ for Java/C#).
    fn interface_name_pattern(&self) -> Option<&regex::Regex> {
        None
    }

    /// MRO strategy for this language.
    fn mro_strategy(&self) -> MroStrategy {
        MroStrategy::FirstWins
    }

    // ── Description extraction ────────────────────────────────────

    /// Extract description metadata from a node (e.g., PHP Eloquent properties).
    fn extract_description(
        &self,
        _label: NodeLabel,
        _name: &str,
        _node_text: &str,
    ) -> Option<String> {
        None
    }

    /// Check if a file path is a route definition file.
    fn is_route_file(&self, _path: &str) -> bool {
        false
    }

    // ── Implicit imports ──────────────────────────────────────────

    /// Wire implicit imports between files (e.g., Swift SPM targets).
    fn wire_implicit_imports(
        &self,
        _language_files: &[String],
        _import_map: &std::collections::HashMap<String, std::collections::HashSet<String>>,
        _add_edge: &mut dyn FnMut(&str, &str),
    ) {
        // Default: no implicit imports
    }
}

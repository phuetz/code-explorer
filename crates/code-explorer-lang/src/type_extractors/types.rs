//! Language-specific type extraction configuration.
//!
//! This trait provides the interface for extracting type information from AST nodes.
//! Full implementation in Phase 3 (code-explorer-ingest), where each language provider
//! will supply concrete implementations.

/// Language-specific type extraction configuration.
///
/// Used by the ingestion pipeline for type inference from AST nodes.
/// Implementors define which tree-sitter node types represent declarations,
/// how to extract return types, parameter types, and type annotations.
pub trait LanguageTypeConfig: Send + Sync {
    /// Tree-sitter node types that represent type declarations in this language.
    ///
    /// Examples: `["class_declaration", "interface_declaration"]` for TypeScript,
    /// `["struct_item", "enum_item"]` for Rust.
    fn declaration_node_types(&self) -> &[&str];

    /// Tree-sitter node types that represent function/method signatures.
    ///
    /// Default: empty (no type extraction for signatures).
    fn signature_node_types(&self) -> &[&str] {
        &[]
    }

    /// Extract the return type from a function/method node's text.
    ///
    /// Default: `None` (language does not have explicit return types or
    /// extraction is not yet implemented).
    fn extract_return_type(&self, _node_text: &str) -> Option<String> {
        None
    }

    /// Extract parameter count from a function/method node's text.
    ///
    /// Default: `None`.
    fn extract_parameter_count(&self, _node_text: &str) -> Option<u32> {
        None
    }

    /// Extract type annotation from a variable/property declaration.
    ///
    /// Default: `None`.
    fn extract_type_annotation(&self, _node_text: &str) -> Option<String> {
        None
    }
}

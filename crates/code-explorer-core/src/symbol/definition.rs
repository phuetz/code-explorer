use crate::graph::types::NodeLabel;

/// A symbol definition extracted from the codebase.
/// Used by the SymbolTable for name resolution across files.
#[derive(Debug, Clone)]
pub struct SymbolDefinition {
    /// Unique node ID in the knowledge graph (e.g., "Function:src/main.ts:handleLogin")
    pub node_id: String,
    /// File path where this symbol is defined
    pub file_path: String,
    /// Type of symbol (Function, Class, Method, etc.)
    pub symbol_type: NodeLabel,
    /// Total number of parameters (including optional)
    pub parameter_count: Option<u32>,
    /// Number of required parameters (for arity filtering)
    pub required_parameter_count: Option<u32>,
    /// Types of parameters (for overload disambiguation)
    pub parameter_types: Option<Vec<String>>,
    /// Return type of the symbol (for type inference)
    pub return_type: Option<String>,
    /// Declared type (for variables)
    pub declared_type: Option<String>,
    /// Owner node ID (for Method → Class, Property → Class)
    pub owner_id: Option<String>,
    /// Whether this symbol is exported from its module
    pub is_exported: bool,
}

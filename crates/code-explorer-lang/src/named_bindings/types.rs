/// A named import binding extracted from an import statement.
///
/// Tracks renames like `import { X as Y } from './module'`
/// where `local` = "Y" and `exported` = "X".
#[derive(Debug, Clone)]
pub struct NamedBinding {
    /// Local name in the importing file
    pub local: String,
    /// Name exported by the source module
    pub exported: String,
    /// Whether this is a module alias (Python: `import numpy as np`)
    pub is_module_alias: bool,
    /// Whether this binding only exists in the type namespace.
    pub is_type_only: bool,
}

impl NamedBinding {
    pub fn new(local: impl Into<String>, exported: impl Into<String>) -> Self {
        Self {
            local: local.into(),
            exported: exported.into(),
            is_module_alias: false,
            is_type_only: false,
        }
    }

    pub fn module_alias(local: impl Into<String>, exported: impl Into<String>) -> Self {
        Self {
            local: local.into(),
            exported: exported.into(),
            is_module_alias: true,
            is_type_only: false,
        }
    }

    pub fn type_only(mut self) -> Self {
        self.is_type_only = true;
        self
    }
}

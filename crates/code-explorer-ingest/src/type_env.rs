use std::collections::HashMap;

/// Scope-aware type environment for variable->type inference.
///
/// Three tiers:
/// 1. Explicit type annotations: `let x: User = ...`
/// 2. Constructor inference: `let x = new User()` -> x is User
/// 3. Assignment propagation: `let y = x` where x is known -> y has same type
pub struct TypeEnvironment {
    /// scope -> (variable_name -> type_name)
    /// scope="" for file-level, otherwise the source node id for a local scope
    scopes: HashMap<String, HashMap<String, String>>,
    /// Constructor type overrides: "scope\0varName" -> type
    constructor_types: HashMap<String, String>,
}

impl TypeEnvironment {
    pub fn new() -> Self {
        Self {
            scopes: HashMap::new(),
            constructor_types: HashMap::new(),
        }
    }

    /// Bind a variable to a type in the given scope.
    pub fn bind(&mut self, scope: &str, var_name: &str, type_name: &str) {
        self.scopes
            .entry(scope.to_string())
            .or_default()
            .insert(var_name.to_string(), type_name.to_string());
    }

    /// Lookup a variable's type in the given scope (exact scope only).
    pub fn lookup(&self, scope: &str, var_name: &str) -> Option<&str> {
        self.scopes
            .get(scope)
            .and_then(|vars| vars.get(var_name))
            .map(|s| s.as_str())
    }

    /// Bind a constructor-inferred type for a variable in the given scope.
    pub fn bind_constructor(&mut self, scope: &str, var_name: &str, type_name: &str) {
        let key = format!("{scope}\0{var_name}");
        self.constructor_types.insert(key, type_name.to_string());
    }

    /// Lookup a constructor-inferred type for a variable in the given scope.
    pub fn lookup_constructor(&self, scope: &str, var_name: &str) -> Option<&str> {
        let key = format!("{scope}\0{var_name}");
        self.constructor_types.get(&key).map(|s| s.as_str())
    }

    /// Lookup with scope fallback: try local scope first, then file scope.
    /// Also checks constructor types as a secondary source.
    pub fn resolve(&self, scope: &str, var_name: &str) -> Option<&str> {
        // 1. Try explicit type annotation in local scope
        if let Some(t) = self.lookup(scope, var_name) {
            return Some(t);
        }
        // 2. Try constructor inference in local scope
        if let Some(t) = self.lookup_constructor(scope, var_name) {
            return Some(t);
        }
        // 3. Fallback to file scope (empty string)
        if !scope.is_empty() {
            if let Some(t) = self.lookup("", var_name) {
                return Some(t);
            }
            if let Some(t) = self.lookup_constructor("", var_name) {
                return Some(t);
            }
        }
        None
    }

    /// Clear all bindings.
    pub fn clear(&mut self) {
        self.scopes.clear();
        self.constructor_types.clear();
    }
}

impl Default for TypeEnvironment {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bind_and_lookup() {
        let mut env = TypeEnvironment::new();
        env.bind("", "user", "User");
        assert_eq!(env.lookup("", "user"), Some("User"));
        assert_eq!(env.lookup("", "unknown"), None);
    }

    #[test]
    fn test_scoped_lookup() {
        let mut env = TypeEnvironment::new();
        env.bind("handleLogin@123", "req", "Request");
        assert_eq!(env.lookup("handleLogin@123", "req"), Some("Request"));
        assert_eq!(env.lookup("", "req"), None);
    }

    #[test]
    fn test_constructor_binding() {
        let mut env = TypeEnvironment::new();
        env.bind_constructor("main@1", "svc", "UserService");
        assert_eq!(env.lookup_constructor("main@1", "svc"), Some("UserService"));
        assert_eq!(env.lookup_constructor("main@1", "other"), None);
    }

    #[test]
    fn test_resolve_local_scope_first() {
        let mut env = TypeEnvironment::new();
        env.bind("", "x", "GlobalType");
        env.bind("func@1", "x", "LocalType");
        // Local scope should win
        assert_eq!(env.resolve("func@1", "x"), Some("LocalType"));
    }

    #[test]
    fn test_resolve_falls_back_to_file_scope() {
        let mut env = TypeEnvironment::new();
        env.bind("", "config", "AppConfig");
        // Looking up in a local scope that doesn't have it should fallback
        assert_eq!(env.resolve("func@1", "config"), Some("AppConfig"));
    }

    #[test]
    fn test_resolve_constructor_fallback() {
        let mut env = TypeEnvironment::new();
        env.bind_constructor("func@1", "db", "Database");
        assert_eq!(env.resolve("func@1", "db"), Some("Database"));
    }

    #[test]
    fn test_resolve_constructor_file_scope_fallback() {
        let mut env = TypeEnvironment::new();
        env.bind_constructor("", "app", "Application");
        assert_eq!(env.resolve("func@1", "app"), Some("Application"));
    }

    #[test]
    fn test_resolve_not_found() {
        let env = TypeEnvironment::new();
        assert_eq!(env.resolve("func@1", "nothing"), None);
        assert_eq!(env.resolve("", "nothing"), None);
    }

    #[test]
    fn test_clear() {
        let mut env = TypeEnvironment::new();
        env.bind("", "x", "Foo");
        env.bind_constructor("", "y", "Bar");
        env.clear();
        assert_eq!(env.lookup("", "x"), None);
        assert_eq!(env.lookup_constructor("", "y"), None);
    }

    #[test]
    fn test_explicit_type_wins_over_constructor() {
        let mut env = TypeEnvironment::new();
        env.bind("func@1", "x", "ExplicitType");
        env.bind_constructor("func@1", "x", "ConstructorType");
        // Explicit annotation should take priority
        assert_eq!(env.resolve("func@1", "x"), Some("ExplicitType"));
    }

    #[test]
    fn test_default_trait() {
        let env = TypeEnvironment::default();
        assert_eq!(env.resolve("", "anything"), None);
    }
}

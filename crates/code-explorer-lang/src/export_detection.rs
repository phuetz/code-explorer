//! Export detection functions for all 13 supported languages.
//!
//! Each function takes the node text, node type, and ancestor types to determine
//! if a symbol is exported (publicly visible outside its defining module/file).

// ── TypeScript / JavaScript ──────────────────────────────────────────────────

/// Check if a TypeScript/JavaScript symbol is exported.
///
/// Logic: Walk ancestors for `export_statement`, or text starts with `export `.
pub fn check_ts_export(node_text: &str, _node_type: &str, ancestors: &[&str]) -> bool {
    if node_text.starts_with("export ") {
        return true;
    }
    ancestors.contains(&"export_statement")
}

// ── Python ───────────────────────────────────────────────────────────────────

/// Python: all top-level definitions are considered public unless the name starts
/// with an underscore. Since we receive the node text (not the name alone), the
/// caller should also use [`check_python_export_by_name`] for the underscore
/// convention check.
pub fn check_python_export(_node_text: &str, _node_type: &str, _ancestors: &[&str]) -> bool {
    // Convention: leading underscore = private. But node_text may be the full
    // def/class statement, so we return true here and let the caller verify
    // the extracted *name* via check_python_export_by_name.
    true
}

/// Python name-level export check: names starting with `_` are private.
pub fn check_python_export_by_name(name: &str) -> bool {
    !name.starts_with('_')
}

// ── Java ─────────────────────────────────────────────────────────────────────

/// Java: a symbol is exported if it has the `public` access modifier.
pub fn check_java_export(node_text: &str, _node_type: &str, _ancestors: &[&str]) -> bool {
    node_text_has_modifier(node_text, "public")
}

// ── Go ───────────────────────────────────────────────────────────────────────

/// Go: a symbol is exported if its name begins with an uppercase letter.
///
/// This operates on the *name* rather than the full node text, similar to
/// Python's convention check.
pub fn check_go_export(_node_text: &str, _node_type: &str, _ancestors: &[&str]) -> bool {
    // Go export is name-based; delegate to check_go_export_by_name.
    true
}

/// Go name-level export check: names starting with an uppercase letter are exported.
pub fn check_go_export_by_name(name: &str) -> bool {
    name.chars().next().is_some_and(|c| c.is_uppercase())
}

// ── Rust ─────────────────────────────────────────────────────────────────────

/// Rust: a symbol is exported if it has a `pub` visibility modifier.
///
/// Matches `pub `, `pub(crate)`, `pub(super)`, `pub(in ...)` prefixes.
pub fn check_rust_export(node_text: &str, _node_type: &str, ancestors: &[&str]) -> bool {
    // Direct pub on the item
    if node_text.starts_with("pub ") || node_text.starts_with("pub(") {
        return true;
    }
    // Ancestor might be a visibility_modifier in some tree-sitter grammars
    ancestors.contains(&"visibility_modifier")
}

// ── C / C++ ──────────────────────────────────────────────────────────────────

/// C/C++: symbols have external linkage by default. A `static` qualifier
/// restricts the symbol to file scope (internal linkage).
pub fn check_c_cpp_export(node_text: &str, _node_type: &str, _ancestors: &[&str]) -> bool {
    !node_text_has_modifier(node_text, "static")
}

// ── C# ───────────────────────────────────────────────────────────────────────

/// C#: a symbol is exported if it has the `public` or `internal` access modifier.
/// By default (no explicit modifier), class members are `private`, but top-level
/// types default to `internal` (assembly-visible). We treat both `public` and
/// `internal` as exported for cross-file analysis.
pub fn check_csharp_export(node_text: &str, _node_type: &str, _ancestors: &[&str]) -> bool {
    node_text_has_modifier(node_text, "public") || node_text_has_modifier(node_text, "internal")
}

// ── Razor (.cshtml / .razor) ─────────────────────────────────────────

/// Razor: symbols inside `@code` blocks follow C# visibility rules.
/// Additionally, Razor components with `@page` directives are publicly
/// routable, and `@inject` fields are treated as exported dependencies.
///
/// Since the tree-sitter parsing sees C# code, we apply the same logic
/// as C# (public/internal = exported) with a Razor-specific relaxation:
/// in Blazor components, properties decorated with `[Parameter]` are
/// always considered exported (they're the component's public API).
pub fn check_razor_export(node_text: &str, node_type: &str, ancestors: &[&str]) -> bool {
    // [Parameter] attribute makes properties public API in Blazor
    if node_text.contains("[Parameter]") {
        return true;
    }
    // Fall back to C# rules: public or internal = exported
    check_csharp_export(node_text, node_type, ancestors)
}

// ── PHP ──────────────────────────────────────────────────────────────────────

/// PHP: symbols are exported if they have `public` visibility or are at the
/// top-level scope (functions, classes without explicit visibility are public by
/// default in PHP).
pub fn check_php_export(node_text: &str, node_type: &str, _ancestors: &[&str]) -> bool {
    // Top-level functions and classes are always accessible
    if matches!(
        node_type,
        "function_definition" | "class_declaration" | "interface_declaration" | "trait_declaration"
    ) {
        return true;
    }
    // Methods: check for explicit public or absence of private/protected
    if node_text_has_modifier(node_text, "public") {
        return true;
    }
    // If neither private nor protected is present, PHP defaults to public
    !node_text_has_modifier(node_text, "private") && !node_text_has_modifier(node_text, "protected")
}

// ── Ruby ─────────────────────────────────────────────────────────────────────

/// Ruby: all top-level definitions (methods, classes, modules) are public by
/// default. Private methods are declared via `private` method calls, which we
/// don't detect at the AST node level.
pub fn check_ruby_export(_node_text: &str, _node_type: &str, _ancestors: &[&str]) -> bool {
    true
}

// ── Kotlin ───────────────────────────────────────────────────────────────────

/// Kotlin: symbols are public by default (unlike Java). A symbol is *not*
/// exported only if it has `private`, `internal`, or `protected` modifiers.
pub fn check_kotlin_export(node_text: &str, _node_type: &str, _ancestors: &[&str]) -> bool {
    !node_text_has_modifier(node_text, "private")
        && !node_text_has_modifier(node_text, "internal")
        && !node_text_has_modifier(node_text, "protected")
}

// ── Swift ────────────────────────────────────────────────────────────────────

/// Swift: the default access level is `internal`, meaning the symbol is visible
/// within the same module/target. We treat `internal` (and `public`/`open`) as
/// exported, and `private`/`fileprivate` as not exported.
pub fn check_swift_export(node_text: &str, _node_type: &str, _ancestors: &[&str]) -> bool {
    !node_text_has_modifier(node_text, "private")
        && !node_text_has_modifier(node_text, "fileprivate")
}

// ── Helpers ──────────────────────────────────────────────────────────────────

/// Check if a node's text contains a given modifier keyword as a whole word.
///
/// Performs a simple word-boundary check: the modifier must appear either at the
/// start of the text or after a whitespace/punctuation character, and must be
/// followed by a whitespace/punctuation character or end-of-string.
fn node_text_has_modifier(node_text: &str, modifier: &str) -> bool {
    let bytes = node_text.as_bytes();
    let mod_bytes = modifier.as_bytes();
    let mod_len = mod_bytes.len();

    if bytes.len() < mod_len {
        return false;
    }

    let mut i = 0;
    while i + mod_len <= bytes.len() {
        if &bytes[i..i + mod_len] == mod_bytes {
            // Check left boundary: start of string or non-alphanumeric
            let left_ok = i == 0 || !bytes[i - 1].is_ascii_alphanumeric() && bytes[i - 1] != b'_';
            // Check right boundary: end of string or non-alphanumeric
            let right_ok = i + mod_len == bytes.len()
                || !bytes[i + mod_len].is_ascii_alphanumeric() && bytes[i + mod_len] != b'_';
            if left_ok && right_ok {
                return true;
            }
        }
        i += 1;
    }
    false
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ts_export_keyword() {
        assert!(check_ts_export(
            "export function foo() {}",
            "function_declaration",
            &[]
        ));
    }

    #[test]
    fn test_ts_export_ancestor() {
        assert!(check_ts_export(
            "function foo() {}",
            "function_declaration",
            &["export_statement"]
        ));
    }

    #[test]
    fn test_ts_not_exported() {
        assert!(!check_ts_export(
            "function foo() {}",
            "function_declaration",
            &[]
        ));
    }

    #[test]
    fn test_python_underscore_private() {
        assert!(check_python_export_by_name("public_func"));
        assert!(!check_python_export_by_name("_private_func"));
        assert!(!check_python_export_by_name("__dunder__"));
    }

    #[test]
    fn test_java_public() {
        assert!(check_java_export(
            "public class Foo {}",
            "class_declaration",
            &[]
        ));
        assert!(!check_java_export(
            "private class Foo {}",
            "class_declaration",
            &[]
        ));
    }

    #[test]
    fn test_go_export_by_name() {
        assert!(check_go_export_by_name("Handler"));
        assert!(!check_go_export_by_name("handler"));
    }

    #[test]
    fn test_rust_pub() {
        assert!(check_rust_export("pub fn foo() {}", "function_item", &[]));
        assert!(check_rust_export(
            "pub(crate) fn foo() {}",
            "function_item",
            &[]
        ));
        assert!(!check_rust_export("fn foo() {}", "function_item", &[]));
    }

    #[test]
    fn test_c_cpp_static() {
        assert!(check_c_cpp_export(
            "int foo() {}",
            "function_definition",
            &[]
        ));
        assert!(!check_c_cpp_export(
            "static int foo() {}",
            "function_definition",
            &[]
        ));
    }

    #[test]
    fn test_csharp_public_internal() {
        assert!(check_csharp_export(
            "public class Foo {}",
            "class_declaration",
            &[]
        ));
        assert!(check_csharp_export(
            "internal class Foo {}",
            "class_declaration",
            &[]
        ));
        assert!(!check_csharp_export(
            "private class Foo {}",
            "class_declaration",
            &[]
        ));
    }

    #[test]
    fn test_kotlin_default_public() {
        assert!(check_kotlin_export(
            "fun foo() {}",
            "function_declaration",
            &[]
        ));
        assert!(!check_kotlin_export(
            "private fun foo() {}",
            "function_declaration",
            &[]
        ));
        assert!(!check_kotlin_export(
            "internal fun foo() {}",
            "function_declaration",
            &[]
        ));
    }

    #[test]
    fn test_swift_private_fileprivate() {
        assert!(check_swift_export(
            "func foo() {}",
            "function_declaration",
            &[]
        ));
        assert!(check_swift_export(
            "public func foo() {}",
            "function_declaration",
            &[]
        ));
        assert!(!check_swift_export(
            "private func foo() {}",
            "function_declaration",
            &[]
        ));
        assert!(!check_swift_export(
            "fileprivate func foo() {}",
            "function_declaration",
            &[]
        ));
    }

    #[test]
    fn test_ruby_always_public() {
        assert!(check_ruby_export("def foo; end", "method", &[]));
    }

    #[test]
    fn test_php_top_level_function() {
        assert!(check_php_export(
            "function foo() {}",
            "function_definition",
            &[]
        ));
    }

    #[test]
    fn test_modifier_word_boundary() {
        // "static" inside "ecstatic" should not match
        assert!(!node_text_has_modifier("ecstatic int foo()", "static"));
        // "public" at the start
        assert!(node_text_has_modifier("public void foo()", "public"));
    }
}

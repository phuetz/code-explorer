use super::types::NamedBinding;

/// Split a string on top-level commas only, ignoring commas nested inside
/// `<...>` (generics), `{...}` (object/template type literals), `(...)`,
/// or `[...]`. Used so that an import like
/// `import { foo: Map<K, V>, bar } from './x'` is not split inside the generic.
fn split_top_level_commas(s: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut depth: i32 = 0;
    let mut start = 0;
    for (i, ch) in s.char_indices() {
        match ch {
            '<' | '{' | '(' | '[' => depth += 1,
            '>' | '}' | ')' | ']' => depth = (depth - 1).max(0),
            ',' if depth == 0 => {
                parts.push(&s[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    if start < s.len() {
        parts.push(&s[start..]);
    }
    parts
}

fn find_top_level_colon(s: &str) -> Option<usize> {
    let mut depth: i32 = 0;
    for (i, ch) in s.char_indices() {
        match ch {
            '<' | '{' | '(' | '[' => depth += 1,
            '>' | '}' | ')' | ']' => depth = (depth - 1).max(0),
            ':' if depth == 0 => return Some(i),
            _ => {}
        }
    }
    None
}

fn strip_default_value(s: &str) -> &str {
    s.split('=').next().unwrap_or(s).trim()
}

fn strip_binding_comments(s: &str) -> String {
    let mut cleaned = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch == '/' {
            match chars.peek().copied() {
                Some('/') => {
                    chars.next();
                    for next in chars.by_ref() {
                        if next == '\n' {
                            cleaned.push(' ');
                            break;
                        }
                    }
                    continue;
                }
                Some('*') => {
                    chars.next();
                    let mut previous = '\0';
                    for next in chars.by_ref() {
                        if previous == '*' && next == '/' {
                            break;
                        }
                        previous = next;
                    }
                    cleaned.push(' ');
                    continue;
                }
                _ => {}
            }
        }
        cleaned.push(ch);
    }

    cleaned
}

fn is_identifier_like(s: &str) -> bool {
    let mut chars = s.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    (first == '_' || first == '$' || first.is_ascii_alphabetic())
        && chars.all(|ch| ch == '_' || ch == '$' || ch.is_ascii_alphanumeric())
}

fn import_clause(text: &str) -> Option<&str> {
    let trimmed = text.trim();
    let rest = trimmed.strip_prefix("import ")?;
    let rest = rest.strip_prefix("type ").unwrap_or(rest).trim_start();
    let from_pos = rest.rfind(" from ")?;
    Some(rest[..from_pos].trim())
}

fn export_namespace_alias(text: &str) -> Option<&str> {
    let trimmed = text.trim();
    let rest = trimmed.strip_prefix("export ")?.trim_start();
    let rest = rest.strip_prefix("type ").unwrap_or(rest).trim_start();
    let rest = rest.strip_prefix('*')?.trim_start();
    let rest = rest.strip_prefix("as ")?.trim_start();
    let from_pos = rest.rfind(" from ")?;
    Some(rest[..from_pos].trim())
}

fn is_statement_type_only(text: &str) -> bool {
    let trimmed = text.trim_start();
    trimmed.starts_with("import type ") || trimmed.starts_with("export type ")
}

fn named_binding(
    local: impl Into<String>,
    exported: impl Into<String>,
    is_type_only: bool,
) -> NamedBinding {
    let binding = NamedBinding::new(local, exported);
    if is_type_only {
        binding.type_only()
    } else {
        binding
    }
}

fn module_alias_binding(
    local: impl Into<String>,
    exported: impl Into<String>,
    is_type_only: bool,
) -> NamedBinding {
    let binding = NamedBinding::module_alias(local, exported);
    if is_type_only {
        binding.type_only()
    } else {
        binding
    }
}

/// Extract named import bindings from a TypeScript/JavaScript import statement.
///
/// Handles:
/// - `import Foo from './module'`
/// - `import Foo, { Bar } from './module'`
/// - `import * as Api from './module'`
/// - `import { Foo, Bar as Baz } from './module'`
/// - `export { X } from './y'`
/// - `export * as Api from './module'`
/// - dynamic destructuring text like `{ foo, bar: baz }` from
///   `const { foo, bar: baz } = await import('./module')`
pub fn extract(import_text: &str) -> Option<Vec<NamedBinding>> {
    let text = import_text.trim();
    let mut bindings = Vec::new();
    let statement_type_only = is_statement_type_only(text);

    if let Some(alias) = export_namespace_alias(text) {
        if is_identifier_like(alias) {
            bindings.push(module_alias_binding(alias, "*", statement_type_only));
        }
    }

    if let Some(clause) = import_clause(text) {
        let clause = clause.trim();
        if let Some(alias) = clause.strip_prefix("* as ").map(str::trim) {
            if is_identifier_like(alias) {
                bindings.push(module_alias_binding(alias, "*", statement_type_only));
            }
        } else {
            let default_part = clause
                .split_once(',')
                .map(|(default_part, _)| default_part)
                .unwrap_or(clause)
                .trim();
            if !default_part.starts_with('{')
                && !default_part.starts_with('*')
                && is_identifier_like(default_part)
            {
                bindings.push(named_binding(default_part, "default", statement_type_only));
            }
        }
    }

    // Find the braces containing named imports/exports.
    // Use `rfind('}')` so a nested `}` (e.g. from inline type annotations or
    // template-literal types like `import { type Foo<{ bar }>, Baz } from ...`)
    // does not truncate the binding list.
    let Some(open) = text.find('{') else {
        return if bindings.is_empty() {
            None
        } else {
            Some(bindings)
        };
    };
    let Some(close) = text.rfind('}') else {
        return if bindings.is_empty() {
            None
        } else {
            Some(bindings)
        };
    };
    if close <= open {
        return if bindings.is_empty() {
            None
        } else {
            Some(bindings)
        };
    }

    let inner = &text[open + 1..close];

    // Split on top-level commas only — commas inside nested generics like
    // `Map<K, V>` or template-literal types must not break a binding apart.
    for part in split_top_level_commas(inner) {
        let cleaned_part = strip_binding_comments(part);
        let part = cleaned_part.trim();
        if part.is_empty() {
            continue;
        }
        // Strip optional `type` modifier in TS type-only imports.
        let (part, binding_type_only) =
            if let Some(type_only_part) = part.strip_prefix("type ").map(str::trim_start) {
                (type_only_part, true)
            } else {
                (part, statement_type_only)
            };
        let part = strip_default_value(part);

        if part.starts_with("...") {
            continue;
        }

        // Check for "X as Y" pattern
        if let Some(as_pos) = part.find(" as ") {
            let exported = part[..as_pos].trim();
            let local = strip_default_value(part[as_pos + 4..].trim());
            if !exported.is_empty() && !local.is_empty() {
                bindings.push(named_binding(local, exported, binding_type_only));
            }
        } else if let Some(colon_pos) = find_top_level_colon(part) {
            let exported = part[..colon_pos].trim();
            let local = strip_default_value(part[colon_pos + 1..].trim());
            if !exported.is_empty()
                && !local.is_empty()
                && !matches!(local.chars().next(), Some('{' | '['))
            {
                bindings.push(named_binding(local, exported, binding_type_only));
            }
        } else {
            // Simple import: local == exported
            if !part.is_empty() {
                bindings.push(named_binding(part, part, binding_type_only));
            }
        }
    }

    if bindings.is_empty() {
        None
    } else {
        Some(bindings)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_imports() {
        let bindings = extract("import { User, Repo } from './models'").unwrap();
        assert_eq!(bindings.len(), 2);
        assert_eq!(bindings[0].local, "User");
        assert_eq!(bindings[0].exported, "User");
    }

    #[test]
    fn test_aliased_import() {
        let bindings = extract("import { User as U, Repo } from './models'").unwrap();
        assert_eq!(bindings.len(), 2);
        assert_eq!(bindings[0].local, "U");
        assert_eq!(bindings[0].exported, "User");
        assert_eq!(bindings[1].local, "Repo");
    }

    #[test]
    fn test_export_from() {
        let bindings = extract("export { handler } from './api'").unwrap();
        assert_eq!(bindings.len(), 1);
        assert_eq!(bindings[0].local, "handler");
    }

    #[test]
    fn test_export_from_with_comments() {
        let bindings = extract(
            r#"export {
  // Legacy validator
  ConfigValidator,
  /* Zod validator */ getZodConfigValidator,
  // Command handler
  handleConfigValidateCommand,
} from './validators'"#,
        )
        .unwrap();
        assert_eq!(bindings.len(), 3);
        assert_eq!(bindings[0].local, "ConfigValidator");
        assert_eq!(bindings[1].local, "getZodConfigValidator");
        assert_eq!(bindings[2].local, "handleConfigValidateCommand");
    }

    #[test]
    fn test_default_import() {
        let bindings = extract("import ApiClient from './api'").unwrap();
        assert_eq!(bindings.len(), 1);
        assert_eq!(bindings[0].local, "ApiClient");
        assert_eq!(bindings[0].exported, "default");
    }

    #[test]
    fn test_mixed_default_and_named_imports() {
        let bindings = extract("import ApiClient, { connect as open } from './api'").unwrap();
        assert_eq!(bindings.len(), 2);
        assert_eq!(bindings[0].local, "ApiClient");
        assert_eq!(bindings[0].exported, "default");
        assert_eq!(bindings[1].local, "open");
        assert_eq!(bindings[1].exported, "connect");
    }

    #[test]
    fn test_import_type_marks_bindings_type_only() {
        let bindings = extract("import type ApiClient, { User as U, Repo } from './api'").unwrap();
        assert_eq!(bindings.len(), 3);
        assert!(bindings.iter().all(|binding| binding.is_type_only));
        assert_eq!(bindings[0].local, "ApiClient");
        assert_eq!(bindings[0].exported, "default");
        assert_eq!(bindings[1].local, "U");
        assert_eq!(bindings[1].exported, "User");
        assert_eq!(bindings[2].local, "Repo");
        assert_eq!(bindings[2].exported, "Repo");
    }

    #[test]
    fn test_mixed_type_named_import_marks_only_type_bindings() {
        let bindings = extract("import { type User, connect as open } from './api'").unwrap();
        assert_eq!(bindings.len(), 2);
        assert_eq!(bindings[0].local, "User");
        assert!(bindings[0].is_type_only);
        assert_eq!(bindings[1].local, "open");
        assert!(!bindings[1].is_type_only);
    }

    #[test]
    fn test_namespace_import() {
        let bindings = extract("import * as api from './api'").unwrap();
        assert_eq!(bindings.len(), 1);
        assert_eq!(bindings[0].local, "api");
        assert_eq!(bindings[0].exported, "*");
        assert!(bindings[0].is_module_alias);
    }

    #[test]
    fn test_import_type_namespace_marks_binding_type_only() {
        let bindings = extract("import type * as ApiTypes from './api'").unwrap();
        assert_eq!(bindings.len(), 1);
        assert_eq!(bindings[0].local, "ApiTypes");
        assert_eq!(bindings[0].exported, "*");
        assert!(bindings[0].is_module_alias);
        assert!(bindings[0].is_type_only);
    }

    #[test]
    fn test_export_namespace_alias() {
        let bindings = extract("export * as api from './api'").unwrap();
        assert_eq!(bindings.len(), 1);
        assert_eq!(bindings[0].local, "api");
        assert_eq!(bindings[0].exported, "*");
        assert!(bindings[0].is_module_alias);
    }

    #[test]
    fn test_type_export_namespace_alias_is_not_runtime_binding() {
        let bindings = extract("export type * as ApiTypes from './api'").unwrap();
        assert_eq!(bindings.len(), 1);
        assert_eq!(bindings[0].local, "ApiTypes");
        assert_eq!(bindings[0].exported, "*");
        assert!(bindings[0].is_module_alias);
        assert!(bindings[0].is_type_only);
    }

    #[test]
    fn test_export_type_marks_bindings_type_only() {
        let bindings = extract("export type { User, Repo as Repository } from './api'").unwrap();
        assert_eq!(bindings.len(), 2);
        assert!(bindings.iter().all(|binding| binding.is_type_only));
        assert_eq!(bindings[0].local, "User");
        assert_eq!(bindings[1].local, "Repository");
    }

    #[test]
    fn test_no_braces() {
        assert!(extract("import './setup'").is_none());
    }

    #[test]
    fn test_dynamic_import_destructuring() {
        let bindings = extract("{ foo, bar: baz, qux = fallback }").unwrap();
        assert_eq!(bindings.len(), 3);
        assert_eq!(bindings[0].local, "foo");
        assert_eq!(bindings[0].exported, "foo");
        assert_eq!(bindings[1].local, "baz");
        assert_eq!(bindings[1].exported, "bar");
        assert_eq!(bindings[2].local, "qux");
        assert_eq!(bindings[2].exported, "qux");
    }
}

use super::types::NamedBinding;

/// Extract named bindings from a Python import statement.
///
/// Handles:
/// - `from x import User, Repo as R`
/// - `import numpy as np` (module alias)
pub fn extract(import_text: &str) -> Option<Vec<NamedBinding>> {
    let text = import_text.trim();

    // `from x import Y, Z as W` (also tolerates parenthesized / multi-line forms
    // such as `from x import (\n    Y,\n    Z as W,\n)` and trailing comments).
    //
    // Use `rfind` so a stray `" import "` inside the module names portion
    // (unusual but possible when tree-sitter error-recovery captures extra
    // text) does not split the statement at the wrong position. In valid
    // Python there is exactly one `" import "` boundary — the LAST one.
    if let Some(import_pos) = text.rfind(" import ") {
        let names_part = &text[import_pos + 8..];
        // Strip a wrapping `(...)` if present, drop comments, normalize whitespace.
        let cleaned: String = names_part
            .lines()
            .map(|line| line.split('#').next().unwrap_or("").trim())
            .collect::<Vec<_>>()
            .join(" ");
        let cleaned = cleaned.trim();
        let cleaned = cleaned.strip_prefix('(').unwrap_or(cleaned);
        let cleaned = cleaned.strip_suffix(')').unwrap_or(cleaned);
        let mut bindings = Vec::new();

        for part in cleaned.split(',') {
            let part = part.trim().trim_matches(|c: char| c == '(' || c == ')');
            let part = part.trim();
            if part.is_empty() || part == "*" {
                continue;
            }

            if let Some(as_pos) = part.find(" as ") {
                let exported = part[..as_pos].trim();
                let local = part[as_pos + 4..].trim();
                if !exported.is_empty() && !local.is_empty() {
                    bindings.push(NamedBinding::new(local, exported));
                }
            } else {
                let name = part.trim();
                if !name.is_empty() && !name.contains(' ') {
                    bindings.push(NamedBinding::new(name, name));
                }
            }
        }

        return if bindings.is_empty() {
            None
        } else {
            Some(bindings)
        };
    }

    // `import numpy as np` (module alias)
    if let Some(rest) = text.strip_prefix("import ") {
        if let Some(as_pos) = rest.find(" as ") {
            let module = rest[..as_pos].trim();
            let alias = rest[as_pos + 4..].trim();
            if !module.is_empty() && !alias.is_empty() {
                return Some(vec![NamedBinding::module_alias(alias, module)]);
            }
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_from_import() {
        let bindings = extract("from models import User, Repo").unwrap();
        assert_eq!(bindings.len(), 2);
        assert_eq!(bindings[0].local, "User");
    }

    #[test]
    fn test_from_import_alias() {
        let bindings = extract("from models import User as U").unwrap();
        assert_eq!(bindings.len(), 1);
        assert_eq!(bindings[0].local, "U");
        assert_eq!(bindings[0].exported, "User");
    }

    #[test]
    fn test_module_alias() {
        let bindings = extract("import numpy as np").unwrap();
        assert_eq!(bindings.len(), 1);
        assert_eq!(bindings[0].local, "np");
        assert_eq!(bindings[0].exported, "numpy");
        assert!(bindings[0].is_module_alias);
    }
}

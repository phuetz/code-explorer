use super::types::NamedBinding;

/// Extract named bindings from a C# using directive.
pub fn extract(import_text: &str) -> Option<Vec<NamedBinding>> {
    let text = import_text.trim().trim_end_matches(';');
    // using Alias = Namespace.Type  (or `using Alias = SimpleType` with no dot)
    if let Some(eq_pos) = text.find('=') {
        let alias = text[..eq_pos].trim().trim_start_matches("using").trim();
        let target = text[eq_pos + 1..].trim();
        // If the target is dotted, use the last segment; otherwise the whole
        // target is the type name. Without this fallback, `using A = Foo;`
        // silently dropped on the floor.
        let type_name = target.rfind('.').map_or(target, |p| &target[p + 1..]);
        if !alias.is_empty() && !type_name.is_empty() {
            return Some(vec![NamedBinding::new(alias, type_name)]);
        }
    }
    // using Namespace → last segment is the namespace "alias"
    if let Some(last_dot) = text.rfind('.') {
        let name = &text[last_dot + 1..];
        let name = name.trim();
        if !name.is_empty() {
            return Some(vec![NamedBinding::new(name, name)]);
        }
    }
    None
}

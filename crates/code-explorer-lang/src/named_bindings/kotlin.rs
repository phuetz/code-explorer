use super::types::NamedBinding;

/// Extract named bindings from a Kotlin import statement.
pub fn extract(import_text: &str) -> Option<Vec<NamedBinding>> {
    let text = import_text.trim();
    // import com.example.User as UserAlias
    if let Some(as_pos) = text.rfind(" as ") {
        let before = &text[..as_pos];
        let alias = text[as_pos + 4..].trim();
        if let Some(last_dot) = before.rfind('.') {
            let exported = &before[last_dot + 1..];
            return Some(vec![NamedBinding::new(alias, exported.trim())]);
        }
    }
    // import com.example.User → local = "User"
    if let Some(last_dot) = text.rfind('.') {
        let name = &text[last_dot + 1..];
        let name = name.trim();
        if !name.is_empty() && name != "*" {
            return Some(vec![NamedBinding::new(name, name)]);
        }
    }
    None
}

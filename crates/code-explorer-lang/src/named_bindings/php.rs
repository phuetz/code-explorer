use super::types::NamedBinding;

/// Extract named bindings from a PHP use statement.
pub fn extract(import_text: &str) -> Option<Vec<NamedBinding>> {
    let text = import_text.trim().trim_end_matches(';');
    // use App\Models\User as UserModel
    if let Some(as_pos) = text.rfind(" as ") {
        let before = &text[..as_pos];
        let alias = text[as_pos + 4..].trim();
        if let Some(last_sep) = before.rfind('\\') {
            let exported = &before[last_sep + 1..];
            return Some(vec![NamedBinding::new(alias, exported.trim())]);
        }
    }
    // use App\Models\User → local = "User"
    if let Some(last_sep) = text.rfind('\\') {
        let name = &text[last_sep + 1..];
        let name = name.trim();
        if !name.is_empty() {
            return Some(vec![NamedBinding::new(name, name)]);
        }
    }
    None
}

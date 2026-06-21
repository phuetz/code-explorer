use super::types::NamedBinding;

/// Extract named bindings from a Java import statement.
pub fn extract(import_text: &str) -> Option<Vec<NamedBinding>> {
    let text = import_text.trim().trim_end_matches(';');
    // Java imports are fully qualified: import com.example.User
    // The local name is the last segment
    if let Some(last_dot) = text.rfind('.') {
        let class_name = &text[last_dot + 1..];
        let class_name = class_name.trim();
        if !class_name.is_empty() && class_name != "*" {
            return Some(vec![NamedBinding::new(class_name, class_name)]);
        }
    }
    None
}

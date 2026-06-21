/// Generate a deterministic node ID from label and qualified name.
pub fn generate_id(label: &str, name: &str) -> String {
    format!("{label}:{name}")
}

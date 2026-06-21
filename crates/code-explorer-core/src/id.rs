/// Generate a deterministic node ID from label and qualified name.
///
/// Matches the TypeScript implementation: `${label}:${name}`
pub fn generate_id(label: &str, name: &str) -> String {
    format!("{label}:{name}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_id() {
        assert_eq!(
            generate_id("Function", "src/main.ts:handleLogin"),
            "Function:src/main.ts:handleLogin"
        );
        assert_eq!(generate_id("File", "src/index.ts"), "File:src/index.ts");
    }
}

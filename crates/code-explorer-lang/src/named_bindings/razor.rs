use super::types::NamedBinding;

/// Extract named bindings from a Razor import directive.
///
/// Handles:
/// - `@using Alias = Namespace.Type` → alias binding
/// - `@using Namespace.Type` → last segment binding (same as C#)
/// - `@inject IServiceType fieldName` → field name binding to the type
/// - Plain C# `using` directives (delegated to the same logic)
pub fn extract(import_text: &str) -> Option<Vec<NamedBinding>> {
    let text = import_text.trim().trim_end_matches(';');

    // Strip @using / @inject prefix if present
    let text = text
        .strip_prefix("@using ")
        .or_else(|| text.strip_prefix("@inject "))
        .unwrap_or(text);

    // Strip plain using prefix
    let text = text.strip_prefix("using ").unwrap_or(text).trim();

    // Handle @inject pattern: "IServiceType fieldName"
    // The second token is the field name, the first is the type
    if import_text.trim().starts_with("@inject") {
        let parts: Vec<&str> = text.split_whitespace().collect();
        if parts.len() >= 2 {
            let type_name = parts[0];
            let field_name = parts[1];
            // Extract just the last segment of the type name
            let short_type = type_name
                .rfind('.')
                .map(|i| &type_name[i + 1..])
                .unwrap_or(type_name);
            return Some(vec![
                NamedBinding::new(field_name, short_type),
                NamedBinding::new(short_type, short_type),
            ]);
        }
    }

    // Handle alias: Alias = Namespace.Type
    if let Some(eq_pos) = text.find('=') {
        let alias = text[..eq_pos].trim().trim_start_matches("static").trim();
        let target = text[eq_pos + 1..].trim();
        if let Some(last_dot) = target.rfind('.') {
            let type_name = &target[last_dot + 1..];
            if !alias.is_empty() && !type_name.is_empty() {
                return Some(vec![NamedBinding::new(alias, type_name)]);
            }
        }
    }

    // Standard namespace import → last segment
    if let Some(last_dot) = text.rfind('.') {
        let name = text[last_dot + 1..].trim();
        if !name.is_empty() {
            return Some(vec![NamedBinding::new(name, name)]);
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_razor_using_namespace() {
        let bindings = extract("@using MyApp.Models").unwrap();
        assert_eq!(bindings.len(), 1);
        assert_eq!(bindings[0].local, "Models");
    }

    #[test]
    fn test_razor_using_alias() {
        let bindings = extract("@using VM = MyApp.ViewModels.HomeViewModel").unwrap();
        assert_eq!(bindings.len(), 1);
        assert_eq!(bindings[0].local, "VM");
        assert_eq!(bindings[0].exported, "HomeViewModel");
    }

    #[test]
    fn test_razor_inject() {
        let bindings = extract("@inject IWeatherService Weather").unwrap();
        assert_eq!(bindings.len(), 2);
        assert_eq!(bindings[0].local, "Weather");
        assert_eq!(bindings[0].exported, "IWeatherService");
        assert_eq!(bindings[1].local, "IWeatherService");
    }

    #[test]
    fn test_razor_inject_qualified() {
        let bindings = extract("@inject MyApp.Services.IAuthService Auth").unwrap();
        assert_eq!(bindings.len(), 2);
        assert_eq!(bindings[0].local, "Auth");
        assert_eq!(bindings[0].exported, "IAuthService");
    }
}

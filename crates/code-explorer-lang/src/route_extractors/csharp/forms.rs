//! Razor form action and partial view reference extraction.

use once_cell::sync::Lazy;
use regex::Regex;

use super::types::{FormActionInfo, PartialReference};

/// Html.BeginForm("Action", "Controller", FormMethod.Post)
static RE_BEGIN_FORM: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"Html\.BeginForm\s*\(\s*"(\w+)"\s*,\s*"(\w+)"(?:\s*,\s*FormMethod\.(\w+))?"#)
        .expect("RE_BEGIN_FORM regex must compile")
});

/// Html.Partial("Name"), Html.RenderPartial("Name"), Html.Action("Name", "Controller"),
/// Html.RenderAction("Name", "Controller")
static RE_PARTIAL: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r#"Html\.\s*(Partial|RenderPartial|Action|RenderAction)\s*\(\s*"(\w+)"(?:\s*,\s*"(\w+)")?"#,
    )
    .expect("RE_PARTIAL regex must compile")
});

/// Extract Html.BeginForm() calls from Razor view source.
pub fn extract_form_actions(source: &str) -> Vec<FormActionInfo> {
    let mut results = Vec::new();

    for (line_idx, line) in source.lines().enumerate() {
        if let Some(cap) = RE_BEGIN_FORM.captures(line) {
            let action_name = cap
                .get(1)
                .map(|m| m.as_str().to_string())
                .unwrap_or_default();
            let controller_name = cap
                .get(2)
                .map(|m| m.as_str().to_string())
                .unwrap_or_default();
            let http_method = cap
                .get(3)
                .map(|m| m.as_str().to_uppercase())
                .unwrap_or_else(|| "POST".to_string());

            results.push(FormActionInfo {
                action_name,
                controller_name,
                http_method,
                line_number: (line_idx + 1) as u32,
            });
        }
    }

    results
}

/// Extract `@Html.Partial(...)`, `@Html.RenderPartial(...)`, `@Html.Action(...)`,
/// and `@Html.RenderAction(...)` references from Razor view source.
pub fn extract_partial_references(source: &str) -> Vec<PartialReference> {
    let mut results = Vec::new();

    for (line_idx, line) in source.lines().enumerate() {
        for cap in RE_PARTIAL.captures_iter(line) {
            let helper_type = cap
                .get(1)
                .map(|m| m.as_str().to_string())
                .unwrap_or_default();
            let partial_name = cap
                .get(2)
                .map(|m| m.as_str().to_string())
                .unwrap_or_default();
            let controller_name = cap.get(3).map(|m| m.as_str().to_string());

            results.push(PartialReference {
                partial_name,
                controller_name,
                helper_type,
                line_number: (line_idx + 1) as u32,
            });
        }
    }

    results
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_form_action() {
        let source = r#"
@using (Html.BeginForm("LogOff", "Home", FormMethod.Post, new { id = "logoutForm" }))
{
    <button type="submit">Log off</button>
}

@using (Html.BeginForm("Search", "Dossiers"))
{
    <input type="text" name="q" />
}
"#;
        let forms = extract_form_actions(source);
        assert_eq!(forms.len(), 2);

        let logoff = &forms[0];
        assert_eq!(logoff.action_name, "LogOff");
        assert_eq!(logoff.controller_name, "Home");
        assert_eq!(logoff.http_method, "POST");

        let search = &forms[1];
        assert_eq!(search.action_name, "Search");
        assert_eq!(search.controller_name, "Dossiers");
        assert_eq!(search.http_method, "POST"); // default when FormMethod not specified
    }

    #[test]
    fn test_extract_partial_references() {
        let source = r#"
@Html.Partial("_VuePrestationGrpAide")
@{ Html.RenderPartial("_VueListeDossier"); }
@Html.Action("GetDetails", "Parametrage")
"#;
        let refs = extract_partial_references(source);
        assert_eq!(refs.len(), 3);
        assert_eq!(refs[0].partial_name, "_VuePrestationGrpAide");
        assert_eq!(refs[0].helper_type, "Partial");
        assert_eq!(refs[2].controller_name.as_deref(), Some("Parametrage"));
    }
}

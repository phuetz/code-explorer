//! AJAX / fetch call extraction from C# / Razor / JavaScript source.

use once_cell::sync::Lazy;
use regex::Regex;

use super::types::AjaxCallInfo;

// ─── Compiled regexes for AJAX patterns ─────────────────────────────────

/// $.ajax({...type: "POST"...url: '/Controller/Action'...}) or similar
static RE_AJAX_CALL: Lazy<Regex> = Lazy::new(|| Regex::new(r#"\$\.\s*ajax\s*\("#).unwrap());

/// type/method inside $.ajax options: type: "POST" or method: "GET"
static RE_AJAX_TYPE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r#"(?:type|method)\s*:\s*["'](\w+)["']"#).unwrap());

/// url inside $.ajax options: url: '/Controller/Action' or url: "/Controller/Action"
static RE_AJAX_URL: Lazy<Regex> =
    Lazy::new(|| Regex::new(r#"url\s*:\s*["']([^"']+)["']"#).unwrap());

/// $.post('/Controller/Action', ...)
static RE_JQUERY_POST: Lazy<Regex> =
    Lazy::new(|| Regex::new(r#"\$\.\s*post\s*\(\s*["']([^"']+)["']"#).unwrap());

/// $.get('/Controller/Action', ...)
static RE_JQUERY_GET: Lazy<Regex> =
    Lazy::new(|| Regex::new(r#"\$\.\s*get\s*\(\s*["']([^"']+)["']"#).unwrap());

/// @Url.Action("Action", "Controller")
static RE_URL_ACTION: Lazy<Regex> =
    Lazy::new(|| Regex::new(r#"@?Url\.Action\s*\(\s*"(\w+)"\s*,\s*"(\w+)""#).unwrap());

/// fetch('/Controller/Action') or fetch("/Controller/Action")
static RE_FETCH: Lazy<Regex> =
    Lazy::new(|| Regex::new(r#"fetch\s*\(\s*["']([^"']+)["']"#).unwrap());

/// $.getJSON('/Controller/Action', ...)
static RE_GETJSON: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"\$\.getJSON\s*\(\s*['"]/?(\w+)/(\w+)['"]"#)
        .expect("RE_GETJSON regex must compile")
});

/// $(...).load('/Controller/Action', ...)
static RE_LOAD: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"\$\([^)]+\)\.load\s*\(\s*['"]/?(\w+)/(\w+)['"]"#)
        .expect("RE_LOAD regex must compile")
});

/// Helper: split a URL like "/Controller/Action" or "/api/Controller/Action" into
/// (Option<controller>, Option<action>).
fn parse_url_segments(url: &str) -> (Option<String>, Option<String>) {
    let trimmed = url.trim_start_matches('/');
    let segments: Vec<&str> = trimmed.split('/').filter(|s| !s.is_empty()).collect();

    match segments.len() {
        0 => (None, None),
        1 => (Some(segments[0].to_string()), None),
        2 => (Some(segments[0].to_string()), Some(segments[1].to_string())),
        _ => {
            // Skip leading segments like "api" -- take last two meaningful segments
            // e.g. /api/Products/GetAll -> controller=Products, action=GetAll
            let controller = segments[segments.len() - 2].to_string();
            let action = segments[segments.len() - 1].to_string();
            (Some(controller), Some(action))
        }
    }
}

/// Extract AJAX / fetch calls from C# / Razor / JavaScript source that target controller actions.
pub fn extract_ajax_calls(source: &str) -> Vec<AjaxCallInfo> {
    let mut results = Vec::new();

    for (line_idx, line) in source.lines().enumerate() {
        let line_number = (line_idx + 1) as u32;

        // --- $.ajax({...}) ---
        if RE_AJAX_CALL.is_match(line) {
            // Gather context: look ahead up to 15 lines for url and type within the ajax block
            let context = gather_context(source, line_idx, 15);

            let method = RE_AJAX_TYPE
                .captures(&context)
                .and_then(|c| c.get(1))
                .map(|m| m.as_str().to_uppercase())
                .unwrap_or_else(|| "GET".to_string());

            if let Some(url_cap) = RE_AJAX_URL.captures(&context) {
                let url = url_cap.get(1).map(|m| m.as_str()).unwrap_or("");
                let (controller, action) = parse_url_segments(url);
                results.push(AjaxCallInfo {
                    method,
                    controller_name: controller,
                    action_name: action,
                    url_pattern: url.to_string(),
                    line_number,
                });
            }
            continue;
        }

        // --- $.post(...) ---
        if let Some(cap) = RE_JQUERY_POST.captures(line) {
            let url = cap.get(1).map(|m| m.as_str()).unwrap_or("");
            let (controller, action) = parse_url_segments(url);
            results.push(AjaxCallInfo {
                method: "POST".to_string(),
                controller_name: controller,
                action_name: action,
                url_pattern: url.to_string(),
                line_number,
            });
            continue;
        }

        // --- $.get(...) ---
        if let Some(cap) = RE_JQUERY_GET.captures(line) {
            let url = cap.get(1).map(|m| m.as_str()).unwrap_or("");
            let (controller, action) = parse_url_segments(url);
            results.push(AjaxCallInfo {
                method: "GET".to_string(),
                controller_name: controller,
                action_name: action,
                url_pattern: url.to_string(),
                line_number,
            });
            continue;
        }

        // --- @Url.Action("Action", "Controller") ---
        if let Some(cap) = RE_URL_ACTION.captures(line) {
            let action_name = cap.get(1).map(|m| m.as_str().to_string());
            let controller_name = cap.get(2).map(|m| m.as_str().to_string());
            let url = format!(
                "/{}/{}",
                controller_name.as_deref().unwrap_or(""),
                action_name.as_deref().unwrap_or("")
            );
            results.push(AjaxCallInfo {
                method: "GET".to_string(),
                controller_name,
                action_name,
                url_pattern: url,
                line_number,
            });
            continue;
        }

        // --- fetch('/Controller/Action') ---
        if let Some(cap) = RE_FETCH.captures(line) {
            let url = cap.get(1).map(|m| m.as_str()).unwrap_or("");
            let (controller, action) = parse_url_segments(url);
            results.push(AjaxCallInfo {
                method: "GET".to_string(),
                controller_name: controller,
                action_name: action,
                url_pattern: url.to_string(),
                line_number,
            });
            continue;
        }

        // --- $.getJSON('/Controller/Action', ...) ---
        if let Some(cap) = RE_GETJSON.captures(line) {
            let controller = cap.get(1).map(|m| m.as_str().to_string());
            let action = cap.get(2).map(|m| m.as_str().to_string());
            let url = format!(
                "/{}/{}",
                controller.as_deref().unwrap_or(""),
                action.as_deref().unwrap_or("")
            );
            results.push(AjaxCallInfo {
                method: "GET".to_string(),
                controller_name: controller,
                action_name: action,
                url_pattern: url,
                line_number,
            });
            continue;
        }

        // --- $(...).load('/Controller/Action', ...) ---
        if let Some(cap) = RE_LOAD.captures(line) {
            let controller = cap.get(1).map(|m| m.as_str().to_string());
            let action = cap.get(2).map(|m| m.as_str().to_string());
            let url = format!(
                "/{}/{}",
                controller.as_deref().unwrap_or(""),
                action.as_deref().unwrap_or("")
            );
            results.push(AjaxCallInfo {
                method: "GET".to_string(),
                controller_name: controller,
                action_name: action,
                url_pattern: url,
                line_number,
            });
        }
    }

    results
}

/// Gather a context window of lines starting at `start` for up to `lookahead` additional lines.
fn gather_context(source: &str, start: usize, lookahead: usize) -> String {
    source
        .lines()
        .skip(start)
        .take(lookahead + 1)
        .collect::<Vec<&str>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_ajax_calls_jquery() {
        let source = r#"
<script>
    $.ajax({
        url: '/Products/GetAll',
        type: 'GET',
        success: function(data) { }
    });
</script>
"#;
        let calls = extract_ajax_calls(source);
        assert_eq!(calls.len(), 1);
        let call = &calls[0];
        assert_eq!(call.method, "GET");
        assert_eq!(call.controller_name.as_deref(), Some("Products"));
        assert_eq!(call.action_name.as_deref(), Some("GetAll"));
        assert_eq!(call.url_pattern, "/Products/GetAll");
    }

    #[test]
    fn test_extract_ajax_calls_post_get() {
        let source = r#"
$.post('/Orders/Create', data, function(result) { });
$.get('/Orders/Details', { id: 5 }, function(result) { });
"#;
        let calls = extract_ajax_calls(source);
        assert_eq!(calls.len(), 2);

        let post = calls.iter().find(|c| c.method == "POST").unwrap();
        assert_eq!(post.controller_name.as_deref(), Some("Orders"));
        assert_eq!(post.action_name.as_deref(), Some("Create"));

        let get = calls.iter().find(|c| c.method == "GET").unwrap();
        assert_eq!(get.controller_name.as_deref(), Some("Orders"));
        assert_eq!(get.action_name.as_deref(), Some("Details"));
    }

    #[test]
    fn test_extract_ajax_calls_url_action() {
        let source = r#"
var url = @Url.Action("Delete", "Products");
"#;
        let calls = extract_ajax_calls(source);
        assert_eq!(calls.len(), 1);
        let call = &calls[0];
        assert_eq!(call.controller_name.as_deref(), Some("Products"));
        assert_eq!(call.action_name.as_deref(), Some("Delete"));
    }

    #[test]
    fn test_extract_ajax_calls_fetch() {
        let source = r#"
fetch('/api/Products/Search')
    .then(r => r.json());
"#;
        let calls = extract_ajax_calls(source);
        assert_eq!(calls.len(), 1);
        let call = &calls[0];
        assert_eq!(call.controller_name.as_deref(), Some("Products"));
        assert_eq!(call.action_name.as_deref(), Some("Search"));
    }

    #[test]
    fn test_extract_ajax_getjson() {
        let source = r#"
$.getJSON('/Dossiers/AfficherAides', { id: dossierId }, function(data) {
    $('#aides-container').html(data);
});
"#;
        let calls = extract_ajax_calls(source);
        assert_eq!(calls.len(), 1);

        let call = &calls[0];
        assert_eq!(call.method, "GET");
        assert_eq!(call.controller_name.as_deref(), Some("Dossiers"));
        assert_eq!(call.action_name.as_deref(), Some("AfficherAides"));
    }

    #[test]
    fn test_extract_ajax_load() {
        let source = r#"
$('#details-panel').load('/Parametrage/GetDetails', { id: paramId });
"#;
        let calls = extract_ajax_calls(source);
        assert_eq!(calls.len(), 1);

        let call = &calls[0];
        assert_eq!(call.method, "GET");
        assert_eq!(call.controller_name.as_deref(), Some("Parametrage"));
        assert_eq!(call.action_name.as_deref(), Some("GetDetails"));
    }
}

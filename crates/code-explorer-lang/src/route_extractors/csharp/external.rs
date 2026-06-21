//! External service call detection (WebAPI clients, WCF service references).

use once_cell::sync::Lazy;
use regex::Regex;

use super::types::ExternalServiceCall;

/// new XxxClient(httpClient) -- WebAPI client instantiation
static RE_WEBAPI_CLIENT: Lazy<Regex> =
    Lazy::new(|| Regex::new(r#"new\s+(\w+Client)\s*\("#).expect("regex"));

/// client.XxxAsync(...).GetAwaiter -- async WebAPI method call with GetAwaiter pattern
static RE_CLIENT_METHOD_GETAWAITER: Lazy<Regex> =
    Lazy::new(|| Regex::new(r#"(\w+)\.\s*(\w+Async)\s*\([^)]*\)\s*\.GetAwaiter"#).expect("regex"));

/// WCF service reference calls: new XxxSvc() or new XxxService() or new XxxWS()
static RE_WCF_CALL: Lazy<Regex> =
    Lazy::new(|| Regex::new(r#"new\s+(\w+(?:Svc|WS))\s*\("#).expect("regex"));

/// Detect external service calls (WebAPI auto-generated clients, WCF service references)
/// from C# source code.
pub fn extract_external_service_calls(source: &str) -> Vec<ExternalServiceCall> {
    let mut results = Vec::new();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();

    for (line_idx, line) in source.lines().enumerate() {
        let line_number = (line_idx + 1) as u32;

        // --- WebAPI client instantiation: new AcmeClient(httpClient) ---
        for cap in RE_WEBAPI_CLIENT.captures_iter(line) {
            let client_class = cap
                .get(1)
                .map(|m| m.as_str().to_string())
                .unwrap_or_default();
            // Skip generic "HttpClient" -- that's the transport, not an API client
            if client_class == "HttpClient" || client_class == "WebClient" {
                continue;
            }
            let key = format!("WebAPI:{}:{}", client_class, line_number);
            if seen.insert(key) {
                results.push(ExternalServiceCall {
                    service_type: "WebAPI".to_string(),
                    client_class,
                    method_name: None,
                    line_number,
                });
            }
        }

        // --- Async method call with GetAwaiter: variable.XxxAsync(...).GetAwaiter ---
        for cap in RE_CLIENT_METHOD_GETAWAITER.captures_iter(line) {
            let _variable = cap.get(1).map(|m| m.as_str()).unwrap_or_default();
            let method_name = cap
                .get(2)
                .map(|m| m.as_str().to_string())
                .unwrap_or_default();
            // Try to find the client class from the variable name by looking backwards
            // (or infer from the method name pattern)
            let key = format!("WebAPI_call:{}:{}", method_name, line_number);
            if seen.insert(key) {
                // Try to find a client class from a `new XxxClient` in nearby context
                let search_start = line_idx.saturating_sub(30);
                let context: String = source
                    .lines()
                    .skip(search_start)
                    .take(line_idx - search_start + 1)
                    .collect::<Vec<&str>>()
                    .join("\n");

                let client_class = RE_WEBAPI_CLIENT
                    .captures_iter(&context)
                    .filter_map(|c| c.get(1).map(|m| m.as_str().to_string()))
                    .filter(|name| name != "HttpClient" && name != "WebClient")
                    .last()
                    .unwrap_or_else(|| "UnknownClient".to_string());

                results.push(ExternalServiceCall {
                    service_type: "WebAPI".to_string(),
                    client_class,
                    method_name: Some(method_name),
                    line_number,
                });
            }
        }

        // --- WCF service calls: new BarnabeSvc(), new ExploitationWS() ---
        for cap in RE_WCF_CALL.captures_iter(line) {
            let client_class = cap
                .get(1)
                .map(|m| m.as_str().to_string())
                .unwrap_or_default();
            let key = format!("WCF:{}:{}", client_class, line_number);
            if seen.insert(key) {
                results.push(ExternalServiceCall {
                    service_type: "WCF".to_string(),
                    client_class,
                    method_name: None,
                    line_number,
                });
            }
        }
    }

    results
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_external_service_calls() {
        let source = r#"
var cmcasClient = new AcmeClient(httpClient);
var result = cmcasClient.OuvrantsDroitGetAsync(cmcas, nia, token).GetAwaiter().GetResult();
var partnerClient = new PartnerClient(httpClient);
var foyer = partnerClient.MembersGetAsync(nia).GetAwaiter().GetResult();
"#;
        let calls = extract_external_service_calls(source);
        assert!(
            calls.len() >= 2,
            "Expected at least 2 external service calls, got {}",
            calls.len()
        );
        assert!(calls.iter().any(|c| c.client_class == "AcmeClient"));
        assert!(calls.iter().any(|c| c.client_class == "PartnerClient"));
    }

    #[test]
    fn test_extract_wcf_service_calls() {
        let source = r#"
var svc = new BarnabeSvc();
var ws = new ExploitationWS();
"#;
        let calls = extract_external_service_calls(source);
        assert_eq!(calls.len(), 2);
        assert!(calls
            .iter()
            .any(|c| c.client_class == "BarnabeSvc" && c.service_type == "WCF"));
        assert!(calls
            .iter()
            .any(|c| c.client_class == "ExploitationWS" && c.service_type == "WCF"));
    }

    #[test]
    fn test_extract_no_external_calls() {
        let source = r#"
public class PlainService {
    private readonly HttpClient _httpClient;
    public void DoWork() {
        var client = new HttpClient();
    }
}
"#;
        let calls = extract_external_service_calls(source);
        // HttpClient itself should NOT be detected as an external service
        assert!(calls.iter().all(|c| c.client_class != "HttpClient"));
    }
}

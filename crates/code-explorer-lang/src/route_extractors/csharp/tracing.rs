//! TraceLogger tracing/logging instrumentation detection.

use once_cell::sync::Lazy;
use regex::Regex;

use super::types::TracingInfo;

/// TraceLogger.BeginMethodScope() -- marks a fully traced method
static RE_TRACELOGGER_SCOPE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r#"TraceLogger\.BeginMethodScope\s*\("#).expect("regex"));

/// TraceLogger.Info/Error/Warning/TraceMethod/LogVariables/PrintParam/Log/DumpStackTrace
static RE_TRACELOGGER_CALL: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"TraceLogger\.\s*(Info|Error|Warning|TraceMethod|LogVariables|PrintParam|Log|DumpStackTrace)\s*\("#)
        .expect("regex")
});

/// Method declaration pattern for looking backwards from a BeginMethodScope line
static RE_METHOD_DECL: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"(?:public|private|protected|internal)\s+(?:static\s+|virtual\s+|override\s+|async\s+|sealed\s+)*\w+(?:<[^>]*>)?\s+(\w+)\s*\("#)
        .expect("regex")
});

/// Extract tracing/logging instrumentation info from a C# source file.
///
/// Scans for TraceLogger calls, identifies which methods have `BeginMethodScope`
/// (fully traced), and counts logging call sites.
pub fn extract_tracing_info(source: &str) -> TracingInfo {
    let lines: Vec<&str> = source.lines().collect();
    let mut call_count: u32 = 0;
    let mut traced_methods: Vec<String> = Vec::new();
    let mut log_levels: Vec<String> = Vec::new();
    let mut seen_levels: std::collections::HashSet<String> = std::collections::HashSet::new();

    for (line_idx, &line) in lines.iter().enumerate() {
        // Count TraceLogger.Info/Error/Warning/etc. calls
        for cap in RE_TRACELOGGER_CALL.captures_iter(line) {
            call_count += 1;
            let level = cap
                .get(1)
                .map(|m| m.as_str().to_string())
                .unwrap_or_default();
            if seen_levels.insert(level.clone()) {
                log_levels.push(level);
            }
        }

        // Detect BeginMethodScope and look backwards for method name
        if RE_TRACELOGGER_SCOPE.is_match(line) {
            // Look backwards for the nearest method declaration
            let search_start = line_idx.saturating_sub(15);
            for j in (search_start..line_idx).rev() {
                if let Some(cap) = RE_METHOD_DECL.captures(lines[j]) {
                    let method_name = cap
                        .get(1)
                        .map(|m| m.as_str().to_string())
                        .unwrap_or_default();
                    if !method_name.is_empty() && !traced_methods.contains(&method_name) {
                        traced_methods.push(method_name);
                    }
                    break;
                }
            }
        }
    }

    let is_traced = call_count > 0 || !traced_methods.is_empty();

    TracingInfo {
        is_traced,
        call_count,
        traced_methods,
        log_levels_used: log_levels,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_tracing_info() {
        let source = r#"
public class CaseService {
    public void ProcessDossier(int id) {
        using (TraceLogger.BeginMethodScope()) {
            TraceLogger.Info("Processing dossier");
            TraceLogger.TraceMethod();
            TraceLogger.LogVariables(new VariableInfo("id", id));
            // business logic
        }
    }
    public void SimpleMethod() {
        // no tracing
    }
}
"#;
        let info = extract_tracing_info(source);
        assert!(info.is_traced);
        assert_eq!(info.call_count, 3); // Info, TraceMethod, LogVariables
        assert_eq!(info.traced_methods.len(), 1); // ProcessDossier
        assert_eq!(info.traced_methods[0], "ProcessDossier");
        assert!(info.log_levels_used.contains(&"Info".to_string()));
        assert!(info.log_levels_used.contains(&"TraceMethod".to_string()));
        assert!(info.log_levels_used.contains(&"LogVariables".to_string()));
    }

    #[test]
    fn test_extract_tracing_info_untraced() {
        let source = r#"
public class PlainService {
    public void DoWork() {
        // no TraceLogger at all
    }
}
"#;
        let info = extract_tracing_info(source);
        assert!(!info.is_traced);
        assert_eq!(info.call_count, 0);
        assert!(info.traced_methods.is_empty());
        assert!(info.log_levels_used.is_empty());
    }

    #[test]
    fn test_extract_tracing_info_error_level() {
        let source = r#"
public class ErrorHandler {
    public void HandleError(Exception ex) {
        using (TraceLogger.BeginMethodScope()) {
            TraceLogger.Error("Something failed");
            TraceLogger.DumpStackTrace();
        }
    }
}
"#;
        let info = extract_tracing_info(source);
        assert!(info.is_traced);
        assert_eq!(info.call_count, 2); // Error, DumpStackTrace
        assert_eq!(info.traced_methods.len(), 1);
        assert_eq!(info.traced_methods[0], "HandleError");
        assert!(info.log_levels_used.contains(&"Error".to_string()));
        assert!(info.log_levels_used.contains(&"DumpStackTrace".to_string()));
    }
}

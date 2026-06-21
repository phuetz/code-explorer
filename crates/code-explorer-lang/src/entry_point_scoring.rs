//! Entry point scoring heuristics.
//!
//! Assigns a score (0.0 – 1.0) to symbol names based on how likely they are to
//! be application entry points vs. internal utility helpers. The score is stored
//! on graph nodes and used by the community/process detection pipeline.

use code_explorer_core::config::languages::SupportedLanguage;

/// Result of entry point scoring.
#[derive(Debug, Clone)]
pub struct EntryPointScore {
    /// Score from 0.0 (utility) to 1.0 (definite entry point).
    pub score: f64,
    /// Human-readable reason for the score.
    pub reason: String,
}

// ── Universal name patterns ──────────────────────────────────────────────────

/// Universal entry point name patterns that apply across all languages.
const ENTRY_POINT_PATTERNS: &[(&str, f64, &str)] = &[
    ("main", 1.0, "main entry point"),
    ("init", 0.8, "initialization function"),
    ("bootstrap", 0.9, "bootstrap function"),
    ("start", 0.85, "start function"),
    ("run", 0.8, "run function"),
    ("setup", 0.7, "setup function"),
    ("configure", 0.7, "configuration function"),
    ("register", 0.6, "registration function"),
    ("mount", 0.6, "mount function"),
    ("listen", 0.7, "server listen call"),
    ("serve", 0.7, "server serve call"),
    ("execute", 0.6, "execute function"),
    ("launch", 0.8, "launch function"),
    ("create_app", 0.9, "application factory"),
    ("create_server", 0.9, "server factory"),
];

/// Prefixes that indicate event handler or callback entry points.
const HANDLER_PREFIXES: &[(&str, f64, &str)] = &[
    ("handle", 0.7, "event handler"),
    ("on_", 0.65, "event callback"),
    ("on", 0.5, "event callback (camelCase)"),
    ("process", 0.6, "processing function"),
    ("dispatch", 0.6, "dispatch function"),
    ("route", 0.6, "routing function"),
    ("middleware", 0.5, "middleware function"),
];

/// Prefixes that indicate utility/helper functions (lower score).
const UTILITY_PREFIXES: &[(&str, f64, &str)] = &[
    ("is_", 0.1, "predicate utility"),
    ("is", 0.1, "predicate utility"),
    ("has_", 0.1, "predicate utility"),
    ("has", 0.1, "predicate utility"),
    ("get_", 0.15, "getter utility"),
    ("get", 0.15, "getter utility"),
    ("set_", 0.15, "setter utility"),
    ("set", 0.15, "setter utility"),
    ("to_", 0.1, "conversion utility"),
    ("to", 0.1, "conversion utility"),
    ("from_", 0.15, "conversion factory"),
    ("from", 0.15, "conversion factory"),
    ("parse", 0.15, "parsing utility"),
    ("format", 0.1, "formatting utility"),
    ("validate", 0.15, "validation utility"),
    ("normalize", 0.1, "normalization utility"),
    ("convert", 0.1, "conversion utility"),
    ("transform", 0.1, "transformation utility"),
    ("map_", 0.1, "mapping utility"),
    ("filter_", 0.1, "filtering utility"),
    ("sort_", 0.1, "sorting utility"),
    ("find_", 0.1, "search utility"),
    ("create_", 0.2, "factory utility"),
    ("build_", 0.2, "builder utility"),
    ("make_", 0.2, "factory utility"),
    ("new_", 0.2, "constructor utility"),
    ("_test", 0.05, "test function"),
    ("test_", 0.05, "test function"),
];

/// Score a symbol name using universal heuristics.
///
/// Returns `Some(EntryPointScore)` if the name matches a known pattern,
/// `None` if no pattern matched (caller should use a default score).
pub fn score_name(name: &str) -> Option<EntryPointScore> {
    let name_lower = name.to_lowercase();

    // Exact matches first (highest priority)
    for &(pattern, score, reason) in ENTRY_POINT_PATTERNS {
        if name_lower == pattern {
            return Some(EntryPointScore {
                score,
                reason: reason.into(),
            });
        }
    }

    // Handler prefixes
    for &(prefix, score, reason) in HANDLER_PREFIXES {
        if name_lower.starts_with(prefix) && name_lower.len() > prefix.len() {
            // Verify the character after the prefix is uppercase (camelCase) or _
            let rest = &name[prefix.len()..];
            if rest.starts_with('_') || rest.chars().next().is_some_and(|c| c.is_uppercase()) {
                return Some(EntryPointScore {
                    score,
                    reason: reason.into(),
                });
            }
        }
    }

    // Utility prefixes (low score)
    for &(prefix, score, reason) in UTILITY_PREFIXES {
        if name_lower.starts_with(prefix) && name_lower.len() > prefix.len() {
            let rest = &name[prefix.len()..];
            if rest.starts_with('_') || rest.chars().next().is_some_and(|c| c.is_uppercase()) {
                return Some(EntryPointScore {
                    score,
                    reason: reason.into(),
                });
            }
        }
        // Also check suffix patterns like _test
        if name_lower.ends_with(prefix) && prefix.starts_with('_') {
            return Some(EntryPointScore {
                score,
                reason: reason.into(),
            });
        }
    }

    None
}

/// Score a symbol with language-specific patterns applied on top of the universal ones.
pub fn score_name_for_language(name: &str, language: SupportedLanguage) -> Option<EntryPointScore> {
    // Try language-specific patterns first
    let lang_score = match language {
        SupportedLanguage::Python => score_python_name(name),
        SupportedLanguage::Go => score_go_name(name),
        SupportedLanguage::Rust => score_rust_name(name),
        SupportedLanguage::Java | SupportedLanguage::Kotlin => score_jvm_name(name),
        SupportedLanguage::Ruby => score_ruby_name(name),
        SupportedLanguage::Php => score_php_name(name),
        SupportedLanguage::Swift => score_swift_name(name),
        SupportedLanguage::CSharp => score_csharp_name(name),
        SupportedLanguage::Razor => score_razor_name(name),
        _ => None,
    };

    // Prefer language-specific score, fall back to universal
    lang_score.or_else(|| score_name(name))
}

// ── Python ───────────────────────────────────────────────────────────────────

fn score_python_name(name: &str) -> Option<EntryPointScore> {
    let name_lower = name.to_lowercase();
    // Python-specific patterns
    if name_lower == "__main__" || name_lower == "__init__" {
        return Some(EntryPointScore {
            score: 0.9,
            reason: "Python module entry point".into(),
        });
    }
    if name_lower == "app" || name_lower == "application" {
        return Some(EntryPointScore {
            score: 0.85,
            reason: "Python WSGI/ASGI application object".into(),
        });
    }
    if name_lower == "cli" || name_lower == "command" {
        return Some(EntryPointScore {
            score: 0.7,
            reason: "Python CLI entry point".into(),
        });
    }
    None
}

// ── Go ───────────────────────────────────────────────────────────────────────

fn score_go_name(name: &str) -> Option<EntryPointScore> {
    match name {
        "main" => Some(EntryPointScore {
            score: 1.0,
            reason: "Go main function".into(),
        }),
        "init" => Some(EntryPointScore {
            score: 0.9,
            reason: "Go init function (runs before main)".into(),
        }),
        "Run" | "Execute" | "Start" => Some(EntryPointScore {
            score: 0.8,
            reason: "Go exported entry point".into(),
        }),
        _ if name.starts_with("New") => Some(EntryPointScore {
            score: 0.3,
            reason: "Go constructor".into(),
        }),
        _ if name.starts_with("Test") => Some(EntryPointScore {
            score: 0.05,
            reason: "Go test function".into(),
        }),
        _ if name.starts_with("Benchmark") => Some(EntryPointScore {
            score: 0.05,
            reason: "Go benchmark function".into(),
        }),
        _ => None,
    }
}

// ── Rust ─────────────────────────────────────────────────────────────────────

fn score_rust_name(name: &str) -> Option<EntryPointScore> {
    match name {
        "main" => Some(EntryPointScore {
            score: 1.0,
            reason: "Rust main function".into(),
        }),
        "new" => Some(EntryPointScore {
            score: 0.3,
            reason: "Rust constructor".into(),
        }),
        "build" | "builder" => Some(EntryPointScore {
            score: 0.3,
            reason: "Rust builder pattern".into(),
        }),
        "run" | "start" | "serve" => Some(EntryPointScore {
            score: 0.8,
            reason: "Rust application entry point".into(),
        }),
        _ if name.starts_with("test_") => Some(EntryPointScore {
            score: 0.05,
            reason: "Rust test function".into(),
        }),
        _ => None,
    }
}

// ── JVM (Java/Kotlin) ────────────────────────────────────────────────────────

fn score_jvm_name(name: &str) -> Option<EntryPointScore> {
    match name {
        "main" => Some(EntryPointScore {
            score: 1.0,
            reason: "JVM main method".into(),
        }),
        "run" => Some(EntryPointScore {
            score: 0.8,
            reason: "Runnable/Thread entry point".into(),
        }),
        "call" => Some(EntryPointScore {
            score: 0.7,
            reason: "Callable entry point".into(),
        }),
        "onStart" | "onCreate" | "onResume" => Some(EntryPointScore {
            score: 0.8,
            reason: "Android lifecycle callback".into(),
        }),
        _ => None,
    }
}

// ── Ruby ─────────────────────────────────────────────────────────────────────

fn score_ruby_name(name: &str) -> Option<EntryPointScore> {
    match name {
        "call" => Some(EntryPointScore {
            score: 0.7,
            reason: "Ruby callable (proc/lambda/service object)".into(),
        }),
        "perform" => Some(EntryPointScore {
            score: 0.8,
            reason: "Ruby background job entry point".into(),
        }),
        "initialize" => Some(EntryPointScore {
            score: 0.3,
            reason: "Ruby constructor".into(),
        }),
        _ => None,
    }
}

// ── PHP ──────────────────────────────────────────────────────────────────────

fn score_php_name(name: &str) -> Option<EntryPointScore> {
    match name {
        "__construct" => Some(EntryPointScore {
            score: 0.3,
            reason: "PHP constructor".into(),
        }),
        "handle" => Some(EntryPointScore {
            score: 0.8,
            reason: "PHP command/job handler".into(),
        }),
        "invoke" | "__invoke" => Some(EntryPointScore {
            score: 0.7,
            reason: "PHP invokable class".into(),
        }),
        "boot" | "register" => Some(EntryPointScore {
            score: 0.7,
            reason: "PHP service provider lifecycle".into(),
        }),
        _ => None,
    }
}

// ── Swift ────────────────────────────────────────────────────────────────────

fn score_swift_name(name: &str) -> Option<EntryPointScore> {
    match name {
        "main" => Some(EntryPointScore {
            score: 1.0,
            reason: "Swift main entry point".into(),
        }),
        "body" => Some(EntryPointScore {
            score: 0.8,
            reason: "SwiftUI body property".into(),
        }),
        "application" => Some(EntryPointScore {
            score: 0.9,
            reason: "UIKit application delegate".into(),
        }),
        "viewDidLoad" | "viewWillAppear" => Some(EntryPointScore {
            score: 0.7,
            reason: "UIKit lifecycle callback".into(),
        }),
        _ => None,
    }
}

// ── Razor / Blazor ──────────────────────────────────────────────────────────

fn score_razor_name(name: &str) -> Option<EntryPointScore> {
    match name {
        // Razor Pages handler methods
        "OnGet" | "OnGetAsync" => Some(EntryPointScore {
            score: 0.95,
            reason: "Razor Page GET handler".into(),
        }),
        "OnPost" | "OnPostAsync" => Some(EntryPointScore {
            score: 0.95,
            reason: "Razor Page POST handler".into(),
        }),
        "OnPut" | "OnPutAsync" => Some(EntryPointScore {
            score: 0.9,
            reason: "Razor Page PUT handler".into(),
        }),
        "OnDelete" | "OnDeleteAsync" => Some(EntryPointScore {
            score: 0.9,
            reason: "Razor Page DELETE handler".into(),
        }),
        "OnPatch" | "OnPatchAsync" => Some(EntryPointScore {
            score: 0.9,
            reason: "Razor Page PATCH handler".into(),
        }),
        // Blazor component lifecycle
        "OnInitialized" | "OnInitializedAsync" => Some(EntryPointScore {
            score: 0.85,
            reason: "Blazor component initialization".into(),
        }),
        "OnParametersSet" | "OnParametersSetAsync" => Some(EntryPointScore {
            score: 0.7,
            reason: "Blazor parameter lifecycle callback".into(),
        }),
        "OnAfterRender" | "OnAfterRenderAsync" => Some(EntryPointScore {
            score: 0.6,
            reason: "Blazor render lifecycle callback".into(),
        }),
        "BuildRenderTree" => Some(EntryPointScore {
            score: 0.5,
            reason: "Blazor render tree builder".into(),
        }),
        // Fall back to C# scoring for other names
        _ => score_csharp_name(name),
    }
}

// ── C# ───────────────────────────────────────────────────────────────────────

fn score_csharp_name(name: &str) -> Option<EntryPointScore> {
    match name {
        "Main" => Some(EntryPointScore {
            score: 1.0,
            reason: "C# Main entry point".into(),
        }),
        "ConfigureServices" | "Configure" => Some(EntryPointScore {
            score: 0.8,
            reason: "ASP.NET Startup configuration".into(),
        }),
        "CreateHostBuilder" | "CreateWebHostBuilder" => Some(EntryPointScore {
            score: 0.9,
            reason: "ASP.NET host builder".into(),
        }),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_main_is_entry_point() {
        let score = score_name("main").unwrap();
        assert!((score.score - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_handler_prefix() {
        let score = score_name("handleRequest").unwrap();
        assert!(score.score >= 0.5);
        assert!(score.reason.contains("handler"));
    }

    #[test]
    fn test_getter_is_utility() {
        let score = score_name("getName").unwrap();
        assert!(score.score <= 0.2);
        assert!(score.reason.contains("getter"));
    }

    #[test]
    fn test_predicate_is_utility() {
        let score = score_name("isValid").unwrap();
        assert!(score.score <= 0.2);
    }

    #[test]
    fn test_unknown_name() {
        assert!(score_name("frobnicateData").is_none());
    }

    #[test]
    fn test_go_specific() {
        let score = score_name_for_language("init", SupportedLanguage::Go).unwrap();
        assert!(score.score >= 0.8);
        assert!(score.reason.contains("Go"));
    }

    #[test]
    fn test_python_specific() {
        let score = score_name_for_language("__main__", SupportedLanguage::Python).unwrap();
        assert!(score.score >= 0.8);
    }

    #[test]
    fn test_rust_test_function() {
        let score = score_name_for_language("test_something", SupportedLanguage::Rust).unwrap();
        assert!(score.score <= 0.1);
    }

    #[test]
    fn test_csharp_main() {
        let score = score_name_for_language("Main", SupportedLanguage::CSharp).unwrap();
        assert!((score.score - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_language_fallback_to_universal() {
        // "bootstrap" is universal, not Go-specific, but should still match
        let score = score_name_for_language("bootstrap", SupportedLanguage::Go).unwrap();
        assert!(score.score >= 0.8);
    }
}

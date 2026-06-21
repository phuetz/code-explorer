//! Framework detection based on file path patterns.
//!
//! Detects common web framework conventions by analyzing file paths, returning
//! an importance multiplier and a human-readable reason string. This information
//! is stored as `ast_framework_multiplier` / `ast_framework_reason` on graph nodes.

/// Result of framework detection for a file path.
#[derive(Debug, Clone)]
pub struct FrameworkHint {
    /// Multiplier applied to the entry-point score (e.g., 1.5 = 50% boost).
    pub multiplier: f64,
    /// Human-readable reason for the multiplier (e.g., "Next.js page route").
    pub reason: String,
}

/// Detect framework conventions from a normalized file path (forward slashes).
///
/// Returns `Some(FrameworkHint)` if the path matches a known pattern, `None`
/// otherwise.
pub fn detect_framework_from_path(path: &str) -> Option<FrameworkHint> {
    let path_lower = path.to_lowercase();
    let segments: Vec<&str> = path_lower.split('/').collect();

    // ── Next.js ──────────────────────────────────────────────────────────
    // App Router: app/**/page.tsx, app/**/layout.tsx, app/**/route.ts
    if segments.contains(&"app") {
        let filename = segments.last().unwrap_or(&"");
        if filename.starts_with("page.") {
            return Some(FrameworkHint {
                multiplier: 2.0,
                reason: "Next.js App Router page".into(),
            });
        }
        if filename.starts_with("layout.") {
            return Some(FrameworkHint {
                multiplier: 1.8,
                reason: "Next.js App Router layout".into(),
            });
        }
        if filename.starts_with("route.") {
            return Some(FrameworkHint {
                multiplier: 2.0,
                reason: "Next.js App Router API route".into(),
            });
        }
        if filename.starts_with("loading.") || filename.starts_with("error.") {
            return Some(FrameworkHint {
                multiplier: 1.3,
                reason: "Next.js App Router boundary".into(),
            });
        }
    }

    // Pages Router: pages/**/*.tsx (but not pages/_app, pages/_document)
    if segments.contains(&"pages") {
        let filename = segments.last().unwrap_or(&"");
        if filename.starts_with("_app.") || filename.starts_with("_document.") {
            return Some(FrameworkHint {
                multiplier: 1.5,
                reason: "Next.js Pages Router shell".into(),
            });
        }
        if has_code_extension(filename) {
            return Some(FrameworkHint {
                multiplier: 1.8,
                reason: "Next.js Pages Router page".into(),
            });
        }
    }

    // ── Express / Node.js ────────────────────────────────────────────────
    //
    // Gate Express detection on a JS/TS extension. Without the extension
    // guard, every file inside a `controllers/` directory matched here first
    // and short-circuited the language-specific checks below — so a Spring
    // `controllers/UserController.java`, a Laravel `app/Http/Controllers/
    // UserController.php`, or a Rails `controllers/users_controller.rb`
    // was misclassified as "Express/Node.js route or controller".
    let is_js_like = path_lower.ends_with(".js")
        || path_lower.ends_with(".mjs")
        || path_lower.ends_with(".cjs")
        || path_lower.ends_with(".ts")
        || path_lower.ends_with(".tsx")
        || path_lower.ends_with(".jsx");
    if is_js_like && (segments.contains(&"routes") || segments.contains(&"controllers")) {
        return Some(FrameworkHint {
            multiplier: 1.5,
            reason: "Express/Node.js route or controller".into(),
        });
    }
    if is_js_like && (segments.contains(&"middleware") || segments.contains(&"middlewares")) {
        return Some(FrameworkHint {
            multiplier: 1.3,
            reason: "Express/Node.js middleware".into(),
        });
    }

    // ── Django ────────────────────────────────────────────────────────────
    let filename: &str = segments.last().unwrap_or(&"");
    if filename == "views.py" || filename == "urls.py" {
        return Some(FrameworkHint {
            multiplier: 1.8,
            reason: "Django view/URL configuration".into(),
        });
    }
    if filename == "models.py" {
        return Some(FrameworkHint {
            multiplier: 1.5,
            reason: "Django model definition".into(),
        });
    }
    if filename == "serializers.py" {
        return Some(FrameworkHint {
            multiplier: 1.3,
            reason: "Django REST serializer".into(),
        });
    }

    // ── Flask / FastAPI ──────────────────────────────────────────────────
    if filename == "app.py" || filename == "main.py" {
        return Some(FrameworkHint {
            multiplier: 1.8,
            reason: "Python application entry point".into(),
        });
    }

    // ── Spring Boot (Java/Kotlin) ────────────────────────────────────────
    if (segments.contains(&"controller") || segments.contains(&"controllers"))
        && (path_lower.ends_with(".java") || path_lower.ends_with(".kt"))
    {
        return Some(FrameworkHint {
            multiplier: 1.8,
            reason: "Spring controller".into(),
        });
    }
    if (segments.contains(&"service") || segments.contains(&"services"))
        && (path_lower.ends_with(".java") || path_lower.ends_with(".kt"))
    {
        return Some(FrameworkHint {
            multiplier: 1.3,
            reason: "Spring service".into(),
        });
    }
    if (segments.contains(&"repository") || segments.contains(&"repositories"))
        && (path_lower.ends_with(".java") || path_lower.ends_with(".kt"))
    {
        return Some(FrameworkHint {
            multiplier: 1.2,
            reason: "Spring repository".into(),
        });
    }

    // ── Laravel (PHP) ────────────────────────────────────────────────────
    if segments.iter().any(|s| *s == "http" || *s == "controllers") && path_lower.ends_with(".php")
    {
        return Some(FrameworkHint {
            multiplier: 1.8,
            reason: "Laravel controller".into(),
        });
    }
    if filename == "web.php" || filename == "api.php" {
        return Some(FrameworkHint {
            multiplier: 2.0,
            reason: "Laravel route definition".into(),
        });
    }

    // ── Rails (Ruby) ─────────────────────────────────────────────────────
    if segments.contains(&"controllers") && path_lower.ends_with(".rb") {
        return Some(FrameworkHint {
            multiplier: 1.8,
            reason: "Rails controller".into(),
        });
    }
    if segments.contains(&"models") && path_lower.ends_with(".rb") {
        return Some(FrameworkHint {
            multiplier: 1.3,
            reason: "Rails model".into(),
        });
    }

    // ── ASP.NET (C#) ─────────────────────────────────────────────────────
    if path_lower.ends_with("controller.cs") {
        return Some(FrameworkHint {
            multiplier: 1.8,
            reason: "ASP.NET controller".into(),
        });
    }
    if filename == "startup.cs" || filename == "program.cs" {
        return Some(FrameworkHint {
            multiplier: 1.5,
            reason: "ASP.NET application entry point".into(),
        });
    }

    // ── ASP.NET MVC Razor Views (.cshtml) ────────────────────────────────
    if path_lower.ends_with(".cshtml") {
        // Shared layouts are high-priority structural files
        if segments.contains(&"shared") {
            if filename.starts_with("_layout")
                || filename.starts_with("_viewstart")
                || filename.starts_with("_viewimports")
            {
                return Some(FrameworkHint {
                    multiplier: 1.8,
                    reason: "ASP.NET MVC shared layout/configuration".into(),
                });
            }
            // Partial views in Shared/
            return Some(FrameworkHint {
                multiplier: 1.5,
                reason: "ASP.NET MVC shared partial view".into(),
            });
        }
        // Views in the standard Views/ directory
        if segments.contains(&"views") {
            return Some(FrameworkHint {
                multiplier: 1.6,
                reason: "ASP.NET MVC view (Razor)".into(),
            });
        }
        // Razor Pages in Pages/ directory
        if segments.contains(&"pages") {
            return Some(FrameworkHint {
                multiplier: 2.0,
                reason: "ASP.NET Razor Page".into(),
            });
        }
        // Areas/ (modular ASP.NET MVC structure)
        if segments.contains(&"areas") {
            return Some(FrameworkHint {
                multiplier: 1.6,
                reason: "ASP.NET MVC area view".into(),
            });
        }
        // Generic .cshtml file
        return Some(FrameworkHint {
            multiplier: 1.3,
            reason: "Razor template".into(),
        });
    }

    // ── Blazor Components (.razor) ───────────────────────────────────────
    if path_lower.ends_with(".razor") {
        if segments.contains(&"shared") || segments.contains(&"layout") {
            return Some(FrameworkHint {
                multiplier: 1.8,
                reason: "Blazor shared/layout component".into(),
            });
        }
        if segments.contains(&"pages") {
            return Some(FrameworkHint {
                multiplier: 2.0,
                reason: "Blazor routable page component".into(),
            });
        }
        if segments.contains(&"components") {
            return Some(FrameworkHint {
                multiplier: 1.5,
                reason: "Blazor UI component".into(),
            });
        }
        return Some(FrameworkHint {
            multiplier: 1.4,
            reason: "Blazor component".into(),
        });
    }

    // ── Go HTTP handlers ─────────────────────────────────────────────────
    if (segments.contains(&"handlers") || segments.contains(&"handler"))
        && path_lower.ends_with(".go")
    {
        return Some(FrameworkHint {
            multiplier: 1.5,
            reason: "Go HTTP handler".into(),
        });
    }

    // ── Swift / iOS ──────────────────────────────────────────────────────
    if path_lower.ends_with("viewcontroller.swift") {
        return Some(FrameworkHint {
            multiplier: 1.5,
            reason: "UIKit view controller".into(),
        });
    }
    if path_lower.ends_with("view.swift") && segments.contains(&"views") {
        return Some(FrameworkHint {
            multiplier: 1.3,
            reason: "SwiftUI view".into(),
        });
    }

    // ── Rust web frameworks (Actix/Axum) ─────────────────────────────────
    if (segments.contains(&"handlers") || segments.contains(&"routes"))
        && path_lower.ends_with(".rs")
    {
        return Some(FrameworkHint {
            multiplier: 1.5,
            reason: "Rust web handler/route".into(),
        });
    }

    None
}

/// Check if a filename has a typical source code extension.
fn has_code_extension(filename: &str) -> bool {
    let code_exts = [
        ".ts", ".tsx", ".js", ".jsx", ".py", ".java", ".go", ".rs", ".rb", ".php", ".cs", ".kt",
        ".swift", ".c", ".cpp", ".h", ".hpp", ".cshtml", ".razor",
    ];
    code_exts.iter().any(|ext| filename.ends_with(ext))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_nextjs_app_router_page() {
        let hint = detect_framework_from_path("src/app/dashboard/page.tsx").unwrap();
        assert!((hint.multiplier - 2.0).abs() < f64::EPSILON);
        assert!(hint.reason.contains("Next.js"));
    }

    #[test]
    fn test_nextjs_pages_router() {
        let hint = detect_framework_from_path("pages/index.tsx").unwrap();
        assert!(hint.multiplier >= 1.5);
    }

    #[test]
    fn test_django_views() {
        let hint = detect_framework_from_path("myapp/views.py").unwrap();
        assert!(hint.multiplier >= 1.5);
        assert!(hint.reason.contains("Django"));
    }

    #[test]
    fn test_spring_controller() {
        let hint =
            detect_framework_from_path("src/main/java/com/app/controller/UserController.java")
                .unwrap();
        assert!(hint.multiplier >= 1.5);
        assert!(hint.reason.contains("Spring"));
    }

    #[test]
    fn test_laravel_route() {
        let hint = detect_framework_from_path("routes/api.php").unwrap();
        assert!(hint.multiplier >= 1.5);
    }

    #[test]
    fn test_no_framework_detected() {
        assert!(detect_framework_from_path("src/utils/helper.ts").is_none());
    }
}

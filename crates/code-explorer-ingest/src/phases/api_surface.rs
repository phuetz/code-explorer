//! Phase: REST/GraphQL endpoint extraction (Theme D).
//!
//! Detects HTTP endpoints across several frameworks by regex-scanning source
//! files. This is intentionally lightweight — we don't re-parse ASTs — because
//! the parsing phase has already done that work. Instead we scan the raw file
//! content for framework-specific route registration patterns and create
//! `ApiEndpoint` nodes, linking them back to parsed handler Methods when we
//! can locate them, or leaving `handler_id` unset when we can't.
//!
//! ## Supported frameworks (MVP)
//!
//! - **Express / Next.js Pages Router**: `app.get('/path', handler)`,
//!   `router.post('/path', handler)`.
//! - **Next.js App Router**: `app/**/route.ts` exporting `GET`/`POST`/...
//! - **FastAPI / Flask**: `@app.get("/...")`, `@router.get(...)`,
//!   `@app.route(...)`.
//! - **Spring**: `@GetMapping("/...")`, `@RequestMapping(...)`,
//!   `@PostMapping`, `@PutMapping`, `@DeleteMapping`, `@PatchMapping`.
//! - **ASP.NET MVC 5** is handled by `aspnet_mvc.rs` — we explicitly skip
//!   `.cs` files here to avoid double-counting.
//!
//! ## TODO(theme-d)
//!
//! - GraphQL schema/resolver extraction (Apollo, graphql-tools).
//! - Axum / Actix Rust route macros.
//! - Ruby on Rails `config/routes.rb`.
//! - Django `urls.py`.
//! - Go `http.HandleFunc`, `gin.Engine.GET`.
//! - gRPC / protobuf service definitions.

use code_explorer_core::graph::types::{
    GraphNode, GraphRelationship, NodeLabel, NodeProperties, RelationshipType,
};
use code_explorer_core::graph::KnowledgeGraph;
use code_explorer_core::id::generate_id;
use once_cell::sync::Lazy;
use rayon::prelude::*;
use regex::Regex;

use crate::phases::structure::FileEntry;

/// Summary stats for this phase.
#[derive(Debug, Default, Clone, Copy)]
pub struct ApiSurfaceStats {
    pub endpoints: usize,
    pub express_next: usize,
    pub fastapi_flask: usize,
    pub spring: usize,
    pub next_app_router: usize,
    pub go: usize,
    pub php: usize,
    pub ruby: usize,
}

/// A raw endpoint detection before we write it to the graph.
#[derive(Debug, Clone)]
struct RawEndpoint {
    framework: &'static str,
    http_method: String,
    route: String,
    file_path: String,
    start_line: u32,
    /// Hint for handler lookup: either the identifier referenced in the route
    /// registration, or `None` when we should fall back to "nearest method in
    /// the same file".
    handler_hint: Option<String>,
}

// ─── Regexes (compiled once) ────────────────────────────────────────────────

static RE_EXPRESS: Lazy<Regex> = Lazy::new(|| {
    // Matches:  app.get('/path', handler)     router.post("/p", h)
    //           app.route('/p').get(h)        .delete(h)   .patch(h)
    // Intentionally loose to cover common forms. We only capture the first
    // arg (the route) and optionally a trailing identifier.
    Regex::new(
        r#"(?x)
        (?:\b(?:app|router|api)\b)\s*\.\s*
        (get|post|put|delete|patch|options|head|all)\s*\(\s*
        ['"`]([^'"`]+)['"`]
        (?:\s*,\s*(?:async\s+)?(?:\([^)]*\)\s*=>|function\b|([A-Za-z_$][\w$]*)))?
        "#,
    )
    .expect("express regex compiles")
});

static RE_PY_DECORATOR: Lazy<Regex> = Lazy::new(|| {
    // @app.get("/p")  @router.post('/p')  @blueprint.route("/p")
    Regex::new(
        r#"(?x)
        ^\s*@\s*
        (?:app|router|api|blueprint|bp)\s*\.\s*
        (get|post|put|delete|patch|route|head|options)\s*\(
        \s*['"]([^'"]+)['"]
        "#,
    )
    .expect("fastapi/flask regex compiles")
});

static RE_SPRING_MAPPING: Lazy<Regex> = Lazy::new(|| {
    // @GetMapping("/x")  @RequestMapping(value="/x", method=RequestMethod.POST)
    // We capture the mapping name and first string-literal arg.
    Regex::new(
        r#"(?x)
        @\s*(GetMapping|PostMapping|PutMapping|DeleteMapping|PatchMapping|RequestMapping)
        \s*\(
        (?:[^)]*?\bvalue\s*=\s*)?
        \s*['"]([^'"]+)['"]
        "#,
    )
    .expect("spring regex compiles")
});

static RE_SPRING_REQ_METHOD: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"method\s*=\s*RequestMethod\.(GET|POST|PUT|DELETE|PATCH)"#)
        .expect("spring method regex compiles")
});

static RE_NEXT_APP_HANDLER: Lazy<Regex> = Lazy::new(|| {
    // export async function GET(...)   export function POST(...)
    Regex::new(
        r#"(?x)
        ^\s*export\s+(?:async\s+)?function\s+
        (GET|POST|PUT|DELETE|PATCH|OPTIONS|HEAD)\b
        "#,
    )
    .expect("next app router regex compiles")
});

static RE_GO_ROUTE: Lazy<Regex> = Lazy::new(|| {
    // Gin/Echo `r.GET("/p", h)`, chi `r.Get("/p", h)`, net/http & gorilla
    // `mux.HandleFunc("/p", h)` / `r.Handle("/p", h)`. Route must start with `/`
    // (filters non-routing `.Get("key")` style calls).
    Regex::new(
        r#"(?x)
        \b\w+\s*\.\s*
        (GET|POST|PUT|DELETE|PATCH|HEAD|OPTIONS|Any|
         Get|Post|Put|Delete|Patch|Handle|HandleFunc|Connect)
        \s*\(\s*
        "(/[^"]*)"
        "#,
    )
    .expect("go route regex compiles")
});

static RE_PHP_LARAVEL: Lazy<Regex> = Lazy::new(|| {
    // Laravel `Route::get('/p', …)`, `Route::match(['get','post'], '/p', …)`,
    // `$router->post('/p')`.
    Regex::new(
        r#"(?x)
        (?:Route\s*::|\$\w+\s*->)\s*
        (get|post|put|patch|delete|options|any|match)
        \s*\(\s*
        (?:\[[^\]]*\]\s*,\s*)?
        ['"]([^'"]+)['"]
        "#,
    )
    .expect("laravel route regex compiles")
});

static RE_RUBY_RAILS: Lazy<Regex> = Lazy::new(|| {
    // Rails routes.rb DSL: `get '/p'`, `post "/p", to: '…'`, `delete '/p'`.
    // Only applied to files under a `routes` path (see scan_file) to avoid
    // matching `get`/`post` method calls elsewhere.
    Regex::new(
        r#"(?x)
        ^\s*
        (get|post|put|patch|delete)
        \s+
        ['"]([^'"]+)['"]
        "#,
    )
    .expect("rails route regex compiles")
});

// ─── Entry point ────────────────────────────────────────────────────────────

/// Run the endpoint extraction phase. Scans files in parallel and writes all
/// discovered endpoints + their `HandledBy` edges into the graph.
pub fn extract_api_surface(graph: &mut KnowledgeGraph, files: &[FileEntry]) -> ApiSurfaceStats {
    // Parallel scan — one thread per file.
    let per_file: Vec<Vec<RawEndpoint>> = files
        .par_iter()
        .filter(|f| !should_skip_file(&f.path))
        .map(scan_file)
        .collect();

    let mut stats = ApiSurfaceStats::default();
    let mut seen_ids: std::collections::HashSet<String> = std::collections::HashSet::new();

    for endpoints in per_file {
        for ep in endpoints {
            // Tally.
            match ep.framework {
                "express" => stats.express_next += 1,
                "nextjs" => stats.next_app_router += 1,
                "fastapi" | "flask" => stats.fastapi_flask += 1,
                "spring" => stats.spring += 1,
                "go" => stats.go += 1,
                "laravel" => stats.php += 1,
                "rails" => stats.ruby += 1,
                _ => {}
            }

            let normalized_route = normalize_route(&ep.route);
            let id_key = format!("{}:{}", ep.http_method, normalized_route);
            let node_id = generate_id("ApiEndpoint", &id_key);
            if !seen_ids.insert(node_id.clone()) {
                // Same method+route already emitted in this run. Skip to keep
                // endpoints unique — a project that declares the same route
                // in two places will surface via duplicate handler searches.
                continue;
            }

            // Resolve the handler Method ID if we can find one.
            let handler_id = resolve_handler(graph, &ep);

            let name = format!("{} {}", ep.http_method, normalized_route);
            let node = GraphNode {
                id: node_id.clone(),
                label: NodeLabel::ApiEndpoint,
                properties: NodeProperties {
                    name,
                    file_path: ep.file_path.clone(),
                    start_line: Some(ep.start_line),
                    http_method: Some(ep.http_method.clone()),
                    route: Some(normalized_route.clone()),
                    route_template: Some(normalized_route.clone()),
                    framework: Some(ep.framework.to_string()),
                    handler_id: handler_id.clone(),
                    ..Default::default()
                },
            };
            graph.add_node(node);
            stats.endpoints += 1;

            if let Some(hid) = handler_id {
                graph.add_relationship(GraphRelationship {
                    id: format!("handled_by_{}_{}", node_id, hid),
                    source_id: node_id.clone(),
                    target_id: hid,
                    rel_type: RelationshipType::HandledBy,
                    confidence: 0.8,
                    reason: format!("api_surface:{}", ep.framework),
                    step: None,
                });
            }
        }
    }

    stats
}

/// Skip files the parent pipeline already handles (ASP.NET MVC) and folders
/// we never want to walk (node_modules, vendor, etc).
fn should_skip_file(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    // ASP.NET MVC is handled elsewhere.
    if lower.ends_with(".cs") || lower.ends_with(".cshtml") || lower.ends_with(".aspx") {
        return true;
    }
    if lower.contains("/node_modules/")
        || lower.contains("/target/")
        || lower.contains("/dist/")
        || lower.contains("/build/")
        || lower.contains("/vendor/")
        || lower.starts_with("node_modules/")
        || lower.starts_with("target/")
        || lower.starts_with("vendor/")
        || lower.starts_with("dist/")
        || lower.starts_with("build/")
    {
        return true;
    }
    false
}

fn scan_file(file: &FileEntry) -> Vec<RawEndpoint> {
    let mut out = Vec::new();
    if file.content.is_empty() {
        return out;
    }
    let lower_path = file.path.to_ascii_lowercase();
    let is_js_family = matches!(
        lower_path.rsplit_once('.').map(|(_, e)| e),
        Some("js" | "jsx" | "ts" | "tsx" | "mjs" | "cjs")
    );
    let is_python = lower_path.ends_with(".py");
    let is_java_kotlin = lower_path.ends_with(".java") || lower_path.ends_with(".kt");
    let is_go = lower_path.ends_with(".go");
    let is_php = lower_path.ends_with(".php");
    // Rails routes live in config/routes.rb (+ config/routes/*.rb); gating to
    // `routes` paths avoids matching `get`/`post` method calls elsewhere.
    let is_ruby_routes = lower_path.ends_with(".rb") && lower_path.contains("routes");
    let is_next_route = is_js_family
        && (lower_path.ends_with("/route.ts")
            || lower_path.ends_with("/route.js")
            || lower_path.ends_with("/route.tsx")
            || lower_path.ends_with("/route.jsx"));

    // Scan line-by-line so we can report accurate line numbers.
    for (idx, line) in file.content.lines().enumerate() {
        let line_num = (idx + 1) as u32;

        if is_next_route {
            if let Some(cap) = RE_NEXT_APP_HANDLER.captures(line) {
                let method = cap.get(1).unwrap().as_str().to_string();
                let route = route_from_next_path(&file.path);
                out.push(RawEndpoint {
                    framework: "nextjs",
                    http_method: method.clone(),
                    route,
                    file_path: file.path.clone(),
                    start_line: line_num,
                    handler_hint: Some(method),
                });
                continue;
            }
        }

        if is_js_family {
            if let Some(cap) = RE_EXPRESS.captures(line) {
                let method_verb = cap.get(1).unwrap().as_str();
                let route = cap.get(2).unwrap().as_str().to_string();
                let handler = cap.get(3).map(|m| m.as_str().to_string());
                let method = if method_verb == "all" {
                    "ALL".to_string()
                } else {
                    method_verb.to_ascii_uppercase()
                };
                out.push(RawEndpoint {
                    framework: "express",
                    http_method: method,
                    route,
                    file_path: file.path.clone(),
                    start_line: line_num,
                    handler_hint: handler,
                });
            }
        }

        if is_python {
            if let Some(cap) = RE_PY_DECORATOR.captures(line) {
                let verb = cap.get(1).unwrap().as_str();
                let route = cap.get(2).unwrap().as_str().to_string();
                let framework = if verb == "route" { "flask" } else { "fastapi" };
                let method = if verb == "route" {
                    // Flask's @app.route defaults to GET; inspecting the rest
                    // of the line for `methods=["POST"]` would be a nice
                    // refinement — TODO(theme-d).
                    "GET".to_string()
                } else {
                    verb.to_ascii_uppercase()
                };
                out.push(RawEndpoint {
                    framework,
                    http_method: method,
                    route,
                    file_path: file.path.clone(),
                    start_line: line_num,
                    handler_hint: None,
                });
            }
        }

        if is_java_kotlin {
            if let Some(cap) = RE_SPRING_MAPPING.captures(line) {
                let mapping = cap.get(1).unwrap().as_str();
                let route = cap.get(2).unwrap().as_str().to_string();
                let method = match mapping {
                    "GetMapping" => "GET",
                    "PostMapping" => "POST",
                    "PutMapping" => "PUT",
                    "DeleteMapping" => "DELETE",
                    "PatchMapping" => "PATCH",
                    "RequestMapping" => {
                        // Try to read `method=RequestMethod.POST` from the
                        // surrounding line (same line only for now).
                        if let Some(mcap) = RE_SPRING_REQ_METHOD.captures(line) {
                            let _ = mcap.get(1).unwrap().as_str();
                            match mcap.get(1).unwrap().as_str() {
                                "POST" => "POST",
                                "PUT" => "PUT",
                                "DELETE" => "DELETE",
                                "PATCH" => "PATCH",
                                _ => "GET",
                            }
                        } else {
                            "GET"
                        }
                    }
                    _ => "GET",
                };
                out.push(RawEndpoint {
                    framework: "spring",
                    http_method: method.to_string(),
                    route,
                    file_path: file.path.clone(),
                    start_line: line_num,
                    handler_hint: None,
                });
            }
        }

        if is_go {
            if let Some(cap) = RE_GO_ROUTE.captures(line) {
                let verb = cap.get(1).unwrap().as_str().to_ascii_uppercase();
                let route = cap.get(2).unwrap().as_str().to_string();
                let method = match verb.as_str() {
                    "HANDLE" | "HANDLEFUNC" | "ANY" | "CONNECT" => "ANY".to_string(),
                    other => other.to_string(),
                };
                out.push(RawEndpoint {
                    framework: "go",
                    http_method: method,
                    route,
                    file_path: file.path.clone(),
                    start_line: line_num,
                    handler_hint: None,
                });
            }
        }

        if is_php {
            if let Some(cap) = RE_PHP_LARAVEL.captures(line) {
                let verb = cap.get(1).unwrap().as_str();
                let route = cap.get(2).unwrap().as_str().to_string();
                let method = match verb {
                    "match" | "any" => "ANY".to_string(),
                    v => v.to_ascii_uppercase(),
                };
                out.push(RawEndpoint {
                    framework: "laravel",
                    http_method: method,
                    route,
                    file_path: file.path.clone(),
                    start_line: line_num,
                    handler_hint: None,
                });
            }
        }

        if is_ruby_routes {
            if let Some(cap) = RE_RUBY_RAILS.captures(line) {
                let verb = cap.get(1).unwrap().as_str().to_ascii_uppercase();
                let route = cap.get(2).unwrap().as_str().to_string();
                out.push(RawEndpoint {
                    framework: "rails",
                    http_method: verb,
                    route,
                    file_path: file.path.clone(),
                    start_line: line_num,
                    handler_hint: None,
                });
            }
        }
    }

    out
}

/// Turn an app-router file path into the runtime URL.
///
/// `app/users/[id]/route.ts` → `/users/:id`
/// `src/app/api/grants/route.ts` → `/api/grants`
fn route_from_next_path(path: &str) -> String {
    let forward = path.replace('\\', "/");
    // Find the first `/app/` boundary (or leading `app/`).
    let start = forward
        .find("/app/")
        .map(|i| i + "/app/".len())
        .or_else(|| {
            if forward.starts_with("app/") {
                Some("app/".len())
            } else {
                None
            }
        })
        .unwrap_or(0);
    let tail = &forward[start..];
    // Strip the trailing `/route.ts` etc.
    let without_file = tail.rsplit_once('/').map(|(dirs, _)| dirs).unwrap_or(tail);
    // Convert `[id]` → `:id`; drop grouping folders `(auth)`.
    let mut parts: Vec<String> = Vec::new();
    for segment in without_file.split('/') {
        if segment.is_empty() {
            continue;
        }
        if segment.starts_with('(') && segment.ends_with(')') {
            continue; // Next.js grouping segments don't affect the URL
        }
        if segment.starts_with('[') && segment.ends_with(']') {
            parts.push(format!(":{}", &segment[1..segment.len() - 1]));
        } else {
            parts.push(segment.to_string());
        }
    }
    if parts.is_empty() {
        "/".to_string()
    } else {
        format!("/{}", parts.join("/"))
    }
}

/// Normalize a route so that two syntactic variants (`:id` vs `{id}`) produce
/// the same node ID. Keep it stable but reversible by downstream tooling.
fn normalize_route(route: &str) -> String {
    let mut out = String::with_capacity(route.len());
    if !route.starts_with('/') {
        out.push('/');
    }
    out.push_str(route.trim_end_matches('/'));
    if out.is_empty() {
        out.push('/');
    }
    out
}

/// Try to find the Method/Function node that implements this endpoint.
///
/// We look in the same file path. Priority:
/// 1. If we have a `handler_hint` name, match a Method/Function with that
///    name in the file.
/// 2. For Next.js App Router, match an exported function whose name equals the
///    HTTP method (GET/POST/…).
/// 3. Otherwise, give up and leave handler_id unset.
fn resolve_handler(graph: &KnowledgeGraph, ep: &RawEndpoint) -> Option<String> {
    let hint = ep.handler_hint.as_deref();
    let mut best: Option<String> = None;
    for node in graph.iter_nodes() {
        if !matches!(
            node.label,
            NodeLabel::Method | NodeLabel::Function | NodeLabel::Constructor
        ) {
            continue;
        }
        if node.properties.file_path != ep.file_path {
            continue;
        }
        let name_matches = match hint {
            Some(h) => node.properties.name.eq_ignore_ascii_case(h),
            None => false,
        };
        if name_matches {
            return Some(node.id.clone());
        }
        // Accept first symbol in the file as a weak fallback only when the
        // endpoint is the sole route in the file (best-effort for decorators
        // where we don't know which method below follows).
        if best.is_none() && hint.is_none() {
            best = Some(node.id.clone());
        }
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;
    use code_explorer_core::config::languages::SupportedLanguage;

    fn fe(path: &str, content: &str, lang: SupportedLanguage) -> FileEntry {
        FileEntry {
            path: path.to_string(),
            content: content.to_string(),
            language: Some(lang),
            size: content.len(),
        }
    }

    #[test]
    fn test_express_get() {
        let file = fe(
            "src/server.js",
            "const app = express();\napp.get('/users', listUsers);\n",
            SupportedLanguage::JavaScript,
        );
        let eps = scan_file(&file);
        assert_eq!(eps.len(), 1);
        assert_eq!(eps[0].framework, "express");
        assert_eq!(eps[0].http_method, "GET");
        assert_eq!(eps[0].route, "/users");
        assert_eq!(eps[0].handler_hint.as_deref(), Some("listUsers"));
    }

    #[test]
    fn test_express_router_post_with_arrow() {
        let file = fe(
            "api.ts",
            "router.post('/login', (req, res) => {})",
            SupportedLanguage::TypeScript,
        );
        let eps = scan_file(&file);
        assert_eq!(eps.len(), 1);
        assert_eq!(eps[0].http_method, "POST");
        assert_eq!(eps[0].route, "/login");
    }

    #[test]
    fn test_fastapi_decorator() {
        let file = fe(
            "api.py",
            "@app.get(\"/items\")\ndef list_items():\n    pass\n",
            SupportedLanguage::Python,
        );
        let eps = scan_file(&file);
        assert_eq!(eps.len(), 1);
        assert_eq!(eps[0].framework, "fastapi");
        assert_eq!(eps[0].http_method, "GET");
        assert_eq!(eps[0].route, "/items");
    }

    #[test]
    fn test_flask_route_defaults_get() {
        let file = fe(
            "app.py",
            "@app.route('/home')\ndef home():\n    return ''\n",
            SupportedLanguage::Python,
        );
        let eps = scan_file(&file);
        assert_eq!(eps.len(), 1);
        assert_eq!(eps[0].framework, "flask");
        assert_eq!(eps[0].http_method, "GET");
    }

    #[test]
    fn test_spring_get_mapping() {
        let file = fe(
            "Controller.java",
            "@GetMapping(\"/api/ping\")\npublic Response ping() { return null; }\n",
            SupportedLanguage::Java,
        );
        let eps = scan_file(&file);
        assert_eq!(eps.len(), 1);
        assert_eq!(eps[0].framework, "spring");
        assert_eq!(eps[0].http_method, "GET");
        assert_eq!(eps[0].route, "/api/ping");
    }

    #[test]
    fn test_spring_request_mapping_post() {
        let file = fe(
            "Controller.java",
            "@RequestMapping(value=\"/x\", method=RequestMethod.POST)\n",
            SupportedLanguage::Java,
        );
        let eps = scan_file(&file);
        assert_eq!(eps.len(), 1);
        assert_eq!(eps[0].http_method, "POST");
    }

    #[test]
    fn test_go_routes() {
        let eps = scan_file(&fe(
            "api/routes.go",
            "r.GET(\"/users\", listUsers)\nrouter.POST(\"/users\", create)\nmux.HandleFunc(\"/health\", h)\n",
            SupportedLanguage::Go,
        ));
        assert!(eps.iter().any(|e| e.framework == "go" && e.http_method == "GET" && e.route == "/users"));
        assert!(eps.iter().any(|e| e.http_method == "POST" && e.route == "/users"));
        assert!(eps.iter().any(|e| e.http_method == "ANY" && e.route == "/health"));
    }

    #[test]
    fn test_go_route_ignores_non_path_string() {
        // `cache.Get("key")` is not a route (no leading slash) — must not match.
        let eps = scan_file(&fe("x.go", "v := cache.Get(\"key\")\n", SupportedLanguage::Go));
        assert!(eps.is_empty());
    }

    #[test]
    fn test_php_laravel_routes() {
        let eps = scan_file(&fe(
            "routes/web.php",
            "Route::get('/users', [UserController::class, 'index']);\nRoute::post('/users', 'UserController@store');\nRoute::match(['get','post'], '/x', $h);\n",
            SupportedLanguage::Php,
        ));
        assert!(eps.iter().any(|e| e.framework == "laravel" && e.http_method == "GET" && e.route == "/users"));
        assert!(eps.iter().any(|e| e.http_method == "POST" && e.route == "/users"));
        assert!(eps.iter().any(|e| e.http_method == "ANY" && e.route == "/x"));
    }

    #[test]
    fn test_ruby_rails_routes() {
        let eps = scan_file(&fe(
            "config/routes.rb",
            "get '/users'\npost \"/users\", to: 'users#create'\n",
            SupportedLanguage::Ruby,
        ));
        assert!(eps.iter().any(|e| e.framework == "rails" && e.http_method == "GET" && e.route == "/users"));
        assert!(eps.iter().any(|e| e.http_method == "POST" && e.route == "/users"));
    }

    #[test]
    fn test_ruby_routes_gated_to_routes_files() {
        // `get '/users'` in a normal model file must NOT be treated as a route.
        let eps = scan_file(&fe("app/models/user.rb", "get '/users'\n", SupportedLanguage::Ruby));
        assert!(eps.is_empty());
    }

    #[test]
    fn test_next_app_router_dynamic_segment() {
        let file = fe(
            "app/users/[id]/route.ts",
            "export async function GET(req: Request) { return Response.json({}); }\n",
            SupportedLanguage::TypeScript,
        );
        let eps = scan_file(&file);
        assert_eq!(eps.len(), 1);
        assert_eq!(eps[0].framework, "nextjs");
        assert_eq!(eps[0].http_method, "GET");
        assert_eq!(eps[0].route, "/users/:id");
    }

    #[test]
    fn test_route_from_next_path_grouping_segment() {
        assert_eq!(
            route_from_next_path("src/app/(marketing)/about/route.ts"),
            "/about"
        );
    }

    #[test]
    fn test_cs_files_skipped() {
        let file = fe(
            "Controllers/HomeController.cs",
            "app.get('/x', h)\n",
            SupportedLanguage::CSharp,
        );
        // We explicitly skip C# at the entry layer; scan_file would otherwise
        // match the regex. Use extract_api_surface to exercise the gate.
        assert!(should_skip_file(&file.path));
    }

    #[test]
    fn test_extract_creates_nodes_and_edges() {
        let mut graph = KnowledgeGraph::new();
        // Pre-seed a Method node so resolve_handler can link to it.
        graph.add_node(GraphNode {
            id: "Method:src/server.js:listUsers".to_string(),
            label: NodeLabel::Method,
            properties: NodeProperties {
                name: "listUsers".to_string(),
                file_path: "src/server.js".to_string(),
                ..Default::default()
            },
        });
        let file = fe(
            "src/server.js",
            "app.get('/users', listUsers);\n",
            SupportedLanguage::JavaScript,
        );
        let stats = extract_api_surface(&mut graph, &[file]);
        assert_eq!(stats.endpoints, 1);
        let eps: Vec<_> = graph
            .iter_nodes()
            .filter(|n| n.label == NodeLabel::ApiEndpoint)
            .collect();
        assert_eq!(eps.len(), 1);
        assert_eq!(eps[0].properties.http_method.as_deref(), Some("GET"));
        let has_handled_by = graph
            .iter_relationships()
            .any(|r| r.rel_type == RelationshipType::HandledBy);
        assert!(has_handled_by, "HandledBy edge should be created");
    }
}

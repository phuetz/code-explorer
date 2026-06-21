//! Configuration-driven UI component library detection system.
//!
//! Detects UI component libraries (Telerik, DevExpress, Syncfusion, Kendo UI,
//! Infragistics, MudBlazor, Radzen, etc.) from source files using configurable
//! pattern matching. Detection patterns are defined in TOML configuration files,
//! enabling new libraries to be supported without code changes.
//!
//! # Architecture
//!
//! Each component library is described by a TOML file containing:
//! - Library metadata (name, vendor, category)
//! - Version ranges (historical coverage from 2012 to present)
//! - Detection patterns (multiple strategies for identifying the library)
//!
//! Pattern types include:
//! - NuGet package names (from .csproj files)
//! - HTML tag prefixes (from .cshtml/.razor files)
//! - C# namespace patterns (from using directives)
//! - JavaScript imports (from script blocks)
//! - HTML attributes (data-* attributes, component-specific attrs)
//! - File path patterns (vendor-specific file structures)
//! - Project references (from .csproj PackageReference elements)

use once_cell::sync::Lazy;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ── Configuration Schema ────────────────────────────────────────────────────

/// Root configuration for a component library config file.
/// Each TOML file can define multiple related libraries (e.g., Telerik MVC + Telerik Blazor).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComponentConfigFile {
    /// List of libraries defined in this config file.
    pub libraries: Vec<ComponentLibraryConfig>,
}

/// Configuration for a single component library.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComponentLibraryConfig {
    /// Display name (e.g., "Telerik UI for ASP.NET MVC")
    pub name: String,
    /// Vendor name (e.g., "Progress", "DevExpress", "Syncfusion")
    pub vendor: String,
    /// Category (e.g., "ASP.NET MVC UI Components", "Blazor UI Components")
    pub category: String,
    /// Supported version ranges
    #[serde(default)]
    pub versions: Vec<VersionRange>,
    /// Detection patterns to identify this library
    pub detection_patterns: Vec<DetectionPattern>,
}

/// Version range for a library.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionRange {
    /// First supported version (e.g., "2012.1")
    pub from: String,
    /// Last supported version (None = still actively developed)
    pub to: Option<String>,
    /// Optional notes about this version range
    pub notes: Option<String>,
}

/// A single detection pattern for identifying a component library.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetectionPattern {
    /// Type of pattern matching to apply
    pub pattern_type: PatternType,
    /// The pattern value (interpreted based on pattern_type)
    pub value: String,
    /// Optional confidence boost/reduction (default: 1.0)
    #[serde(default = "default_confidence")]
    pub confidence: f64,
    /// Optional description for this detection rule
    pub description: Option<String>,
}

fn default_confidence() -> f64 {
    1.0
}

/// Types of detection patterns supported.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PatternType {
    /// NuGet package name (matched against .csproj PackageReference)
    NugetPackage,
    /// HTML tag prefix (e.g., "telerik-", "dx-", "ejs-")
    HtmlTagPrefix,
    /// Exact HTML tag name (e.g., "TelerikRootComponent")
    HtmlTag,
    /// C# namespace pattern with glob support (e.g., "Telerik.Blazor.*")
    Namespace,
    /// JavaScript import path pattern
    JsImport,
    /// HTML attribute name (e.g., "data-role", "k-widget")
    HtmlAttribute,
    /// CSS class prefix (e.g., "k-", "dx-", "e-")
    CssClassPrefix,
    /// File path pattern (glob, e.g., "**/kendo/**")
    FilePath,
    /// .csproj ProjectReference or PackageReference content
    ProjectReference,
    /// JavaScript global variable or namespace (e.g., "kendo", "DevExpress")
    JsGlobal,
    /// NuGet packages.config entry (legacy format pre-2017)
    PackagesConfig,
    /// Web.config or app.config assembly binding
    AssemblyBinding,
}

// ── Detection Results ───────────────────────────────────────────────────────

/// Result of detecting a component library in a file or project.
#[derive(Debug, Clone)]
pub struct DetectedComponent {
    /// Library name
    pub library_name: String,
    /// Vendor name
    pub vendor: String,
    /// Category
    pub category: String,
    /// Pattern type that triggered the detection
    pub detected_by: PatternType,
    /// The specific value that matched
    pub matched_value: String,
    /// Confidence score (0.0 - 1.0)
    pub confidence: f64,
    /// Detected version (if extractable from the match)
    pub detected_version: Option<String>,
}

// ── Component Detector ──────────────────────────────────────────────────────

/// Main component detection engine.
///
/// Loads component library configs and provides detection methods
/// for analyzing source files.
pub struct ComponentDetector {
    /// All loaded library configurations
    libraries: Vec<ComponentLibraryConfig>,
    /// Compiled regex cache for namespace patterns
    namespace_regexes: HashMap<String, Regex>,
}

/// Default component library configurations, embedded at compile time.
static DEFAULT_CONFIGS: Lazy<Vec<ComponentLibraryConfig>> = Lazy::new(|| {
    let mut libs = Vec::new();

    // Parse each embedded TOML config
    let configs: &[&str] = &[
        include_str!("../data/components/telerik.toml"),
        include_str!("../data/components/devexpress.toml"),
        include_str!("../data/components/syncfusion.toml"),
        include_str!("../data/components/kendo.toml"),
        include_str!("../data/components/infragistics.toml"),
        include_str!("../data/components/blazor_community.toml"),
    ];

    for config_str in configs {
        match toml::from_str::<ComponentConfigFile>(config_str) {
            Ok(config) => libs.extend(config.libraries),
            Err(e) => {
                eprintln!("Warning: Failed to parse component config: {e}");
            }
        }
    }

    libs
});

impl ComponentDetector {
    /// Create a detector with the default embedded configurations.
    pub fn new() -> Self {
        Self::from_configs(DEFAULT_CONFIGS.clone())
    }

    /// Create a detector from a custom set of library configs.
    pub fn from_configs(libraries: Vec<ComponentLibraryConfig>) -> Self {
        let mut namespace_regexes = HashMap::new();

        // Pre-compile regex patterns for namespace matching
        for lib in &libraries {
            for pattern in &lib.detection_patterns {
                if pattern.pattern_type == PatternType::Namespace {
                    let regex_str = format!(
                        "^{}$",
                        regex::escape(&pattern.value).replace(r"\*", r"[\w.]*")
                    );
                    if let Ok(re) = Regex::new(&regex_str) {
                        namespace_regexes.insert(pattern.value.clone(), re);
                    }
                }
            }
        }

        Self {
            libraries,
            namespace_regexes,
        }
    }

    /// Detect component libraries used in a Razor file (.cshtml / .razor).
    ///
    /// Analyzes file content for:
    /// - HTML tags matching known component prefixes
    /// - C#/Razor using directives matching known namespaces
    /// - HTML attributes matching known patterns
    /// - JavaScript imports/globals in `<script>` blocks
    /// - CSS class prefixes
    pub fn detect_in_file(&self, content: &str, file_path: &str) -> Vec<DetectedComponent> {
        let mut detected = Vec::new();
        let content_lower = content.to_lowercase();

        for lib in &self.libraries {
            for pattern in &lib.detection_patterns {
                if let Some(matched) =
                    self.check_pattern(content, &content_lower, file_path, pattern)
                {
                    detected.push(DetectedComponent {
                        library_name: lib.name.clone(),
                        vendor: lib.vendor.clone(),
                        category: lib.category.clone(),
                        detected_by: pattern.pattern_type,
                        matched_value: matched,
                        confidence: pattern.confidence.clamp(0.0, 1.0),
                        detected_version: None,
                    });
                }
            }
        }

        // Deduplicate: keep the highest-confidence detection per library
        deduplicate_detections(&mut detected);
        detected
    }

    /// Detect component libraries from a .csproj file content.
    ///
    /// Extracts PackageReference elements and matches against known
    /// NuGet package patterns. Also detects versions from Version attributes.
    pub fn detect_in_csproj(&self, content: &str) -> Vec<DetectedComponent> {
        let mut detected = Vec::new();

        for lib in &self.libraries {
            for pattern in &lib.detection_patterns {
                match pattern.pattern_type {
                    PatternType::NugetPackage => {
                        if let Some(version) = extract_package_version(content, &pattern.value) {
                            detected.push(DetectedComponent {
                                library_name: lib.name.clone(),
                                vendor: lib.vendor.clone(),
                                category: lib.category.clone(),
                                detected_by: pattern.pattern_type,
                                matched_value: pattern.value.clone(),
                                confidence: pattern.confidence,
                                detected_version: Some(version),
                            });
                        } else if content.contains(&pattern.value) {
                            detected.push(DetectedComponent {
                                library_name: lib.name.clone(),
                                vendor: lib.vendor.clone(),
                                category: lib.category.clone(),
                                detected_by: pattern.pattern_type,
                                matched_value: pattern.value.clone(),
                                confidence: pattern.confidence * 0.8,
                                detected_version: None,
                            });
                        }
                    }
                    PatternType::ProjectReference => {
                        if content.contains(&pattern.value) {
                            detected.push(DetectedComponent {
                                library_name: lib.name.clone(),
                                vendor: lib.vendor.clone(),
                                category: lib.category.clone(),
                                detected_by: pattern.pattern_type,
                                matched_value: pattern.value.clone(),
                                confidence: pattern.confidence,
                                detected_version: None,
                            });
                        }
                    }
                    PatternType::PackagesConfig => {
                        if content.contains(&pattern.value) {
                            detected.push(DetectedComponent {
                                library_name: lib.name.clone(),
                                vendor: lib.vendor.clone(),
                                category: lib.category.clone(),
                                detected_by: pattern.pattern_type,
                                matched_value: pattern.value.clone(),
                                confidence: pattern.confidence,
                                detected_version: None,
                            });
                        }
                    }
                    _ => {} // Other pattern types don't apply to .csproj
                }
            }
        }

        deduplicate_detections(&mut detected);
        detected
    }

    /// Check a single detection pattern against file content.
    fn check_pattern(
        &self,
        content: &str,
        content_lower: &str,
        file_path: &str,
        pattern: &DetectionPattern,
    ) -> Option<String> {
        match pattern.pattern_type {
            PatternType::HtmlTagPrefix => {
                // Look for <prefix-something or <PrefixSomething
                let tag_pattern = format!("<{}", pattern.value);
                if content_lower.contains(&tag_pattern.to_lowercase()) {
                    return Some(pattern.value.clone());
                }
                // Also check PascalCase variant
                if content.contains(&format!("<{}", pattern.value)) {
                    return Some(pattern.value.clone());
                }
                None
            }
            PatternType::HtmlTag => {
                let tag_open = format!("<{}", pattern.value);
                if content.contains(&tag_open) {
                    return Some(pattern.value.clone());
                }
                None
            }
            PatternType::Namespace => {
                // Check @using directives and using statements
                if let Some(re) = self.namespace_regexes.get(&pattern.value) {
                    for line in content.lines() {
                        let trimmed = line.trim();
                        // Extract the namespace from @using or using directives
                        let ns = trimmed
                            .strip_prefix("@using ")
                            .or_else(|| trimmed.strip_prefix("using "))
                            .map(|s| s.trim_end_matches(';').trim());
                        if let Some(ns) = ns {
                            if re.is_match(ns) {
                                return Some(ns.to_string());
                            }
                        }
                    }
                }
                None
            }
            PatternType::JsImport => {
                // Check for import/require statements
                let patterns = [
                    format!("from '{}'", pattern.value),
                    format!("from \"{}\"", pattern.value),
                    format!("require('{}')", pattern.value),
                    format!("require(\"{}\")", pattern.value),
                ];
                for p in &patterns {
                    if content.contains(p.as_str()) {
                        return Some(pattern.value.clone());
                    }
                }
                None
            }
            PatternType::HtmlAttribute => {
                if content_lower.contains(&format!("{}=", pattern.value.to_lowercase())) {
                    return Some(pattern.value.clone());
                }
                None
            }
            PatternType::CssClassPrefix => {
                // Check for CSS class prefix anywhere in class attributes.
                // Handles: class="k-widget", class="other k-grid", class='k-button'
                let prefix_lower = pattern.value.to_lowercase();
                // Direct search for the prefix in the content (covers class attrs and beyond)
                if content_lower.contains(&prefix_lower) {
                    // Verify it appears in a plausible class or CSS context
                    let in_class_attr = content_lower
                        .contains(&format!("class=\"{}", prefix_lower))
                        || content_lower.contains(&format!("class='{}", prefix_lower))
                        || content_lower.contains(&format!(" {}", prefix_lower));
                    let in_css_rule = content_lower.contains(&format!(".{}", prefix_lower));
                    if in_class_attr || in_css_rule {
                        return Some(pattern.value.clone());
                    }
                }
                None
            }
            PatternType::JsGlobal => {
                // Check for JavaScript global references like "kendo." or "DevExpress."
                let global_dot = format!("{}.", pattern.value);
                if content.contains(&global_dot) {
                    return Some(pattern.value.clone());
                }
                None
            }
            PatternType::FilePath => {
                let path_lower = file_path.to_lowercase();
                let pattern_lower = pattern.value.to_lowercase();
                // Simple glob matching: ** = any path, * = any segment
                if path_lower.contains(&pattern_lower.replace("**", "").replace('*', "")) {
                    return Some(file_path.to_string());
                }
                None
            }
            PatternType::AssemblyBinding => {
                if content.contains(&pattern.value) {
                    return Some(pattern.value.clone());
                }
                None
            }
            // NugetPackage, ProjectReference, PackagesConfig handled in detect_in_csproj
            _ => None,
        }
    }

    /// Get all loaded library configurations.
    pub fn libraries(&self) -> &[ComponentLibraryConfig] {
        &self.libraries
    }
}

impl Default for ComponentDetector {
    fn default() -> Self {
        Self::new()
    }
}

/// Global cached detector instance for use in hot paths (e.g., per-file pipeline).
/// Avoids re-parsing TOML configs and re-compiling regexes on every call.
static DEFAULT_DETECTOR: Lazy<ComponentDetector> = Lazy::new(ComponentDetector::new);

impl ComponentDetector {
    /// Get the shared default detector instance.
    /// Prefer this over `ComponentDetector::new()` in hot paths to avoid
    /// repeated config parsing and regex compilation.
    pub fn shared() -> &'static ComponentDetector {
        &DEFAULT_DETECTOR
    }
}

// ── Helpers ─────────────────────────────────────────────────────────────────

/// Extract version from a PackageReference element.
///
/// Matches patterns like:
/// - `<PackageReference Include="Package.Name" Version="1.2.3" />`
/// - `<PackageReference Include="Package.Name">\n  <Version>1.2.3</Version>`
fn extract_package_version(csproj_content: &str, package_name: &str) -> Option<String> {
    // Pattern 1: inline Version attribute
    static VERSION_INLINE: Lazy<Regex> = Lazy::new(|| {
        Regex::new(r#"PackageReference\s+Include="([^"]+)"\s+Version="([^"]+)""#).unwrap()
    });
    // Pattern 2: nested Version element
    static VERSION_ELEMENT: Lazy<Regex> = Lazy::new(|| {
        Regex::new(
            r#"PackageReference\s+Include="([^"]+)"[^>]*>[\s\S]*?<Version>([^<]+)</Version>"#,
        )
        .unwrap()
    });

    for re in [&*VERSION_INLINE, &*VERSION_ELEMENT] {
        for cap in re.captures_iter(csproj_content) {
            if let (Some(name), Some(version)) = (cap.get(1), cap.get(2)) {
                if name.as_str() == package_name {
                    return Some(version.as_str().to_string());
                }
            }
        }
    }
    None
}

/// Deduplicate detections: keep only the highest-confidence detection per library.
fn deduplicate_detections(detections: &mut Vec<DetectedComponent>) {
    let mut best: HashMap<String, usize> = HashMap::new();

    for (i, d) in detections.iter().enumerate() {
        let key = d.library_name.clone();
        if let Some(&existing_idx) = best.get(&key) {
            if d.confidence > detections[existing_idx].confidence {
                best.insert(key, i);
            }
        } else {
            best.insert(key, i);
        }
    }

    let keep_indices: std::collections::HashSet<usize> = best.values().copied().collect();
    let mut i = 0;
    detections.retain(|_| {
        let keep = keep_indices.contains(&i);
        i += 1;
        keep
    });
}

// ── Razor Directive Extraction ──────────────────────────────────────────────

/// Extracted Razor directive from a .cshtml/.razor file.
#[derive(Debug, Clone)]
pub struct RazorDirective {
    /// Directive type (page, model, inject, using, inherits, implements, etc.)
    pub directive: String,
    /// Directive value/argument
    pub value: String,
    /// Line number (0-indexed)
    pub line: usize,
}

/// Extract Razor-specific directives from file content.
///
/// Extracts: @page, @model, @inject, @using, @inherits, @implements,
/// @namespace, @attribute, @layout, @section, @preservewhitespace.
pub fn extract_razor_directives(content: &str) -> Vec<RazorDirective> {
    static DIRECTIVE_RE: Lazy<Regex> = Lazy::new(|| {
        Regex::new(
            r#"(?m)^[ \t]*@(page|model|inject|using|inherits|implements|namespace|attribute|layout|section|preservewhitespace|typeparam|rendermode)\s+(.+?)$"#,
        )
        .unwrap()
    });

    let mut directives = Vec::new();

    for (line_num, line) in content.lines().enumerate() {
        if let Some(cap) = DIRECTIVE_RE.captures(line) {
            let directive = cap.get(1).unwrap().as_str().to_string();
            let value = cap.get(2).unwrap().as_str().trim().to_string();
            // Strip trailing quotes from @page directives
            let value = value.trim_matches('"').to_string();
            directives.push(RazorDirective {
                directive,
                value,
                line: line_num,
            });
        }
    }

    directives
}

/// Extract `<script>` block contents from a Razor file.
///
/// Returns a list of (start_line, script_content) tuples.
/// Handles both inline and multi-line script blocks.
pub fn extract_script_blocks(content: &str) -> Vec<(usize, String)> {
    static SCRIPT_RE: Lazy<Regex> =
        Lazy::new(|| Regex::new(r"(?si)<script[^>]*>(.*?)</script>").unwrap());

    let mut blocks = Vec::new();

    for cap in SCRIPT_RE.captures_iter(content) {
        if let Some(script_match) = cap.get(1) {
            let script_content = script_match.as_str().trim().to_string();
            if !script_content.is_empty() {
                // Calculate line number from byte offset
                let line_num = content[..script_match.start()]
                    .chars()
                    .filter(|c| *c == '\n')
                    .count();
                blocks.push((line_num, script_content));
            }
        }
    }

    blocks
}

/// Extract C# code from `@code { }`, `@functions { }`, and `@{ }` blocks in Razor files.
///
/// Returns the concatenated C# code suitable for tree-sitter parsing.
/// Uses brace-counting instead of regex to handle arbitrarily nested braces.
///
/// Operates on byte indices throughout so the function is safe on Razor files
/// containing multi-byte UTF-8 characters (accented identifiers, Unicode quotes,
/// emoji in comments, etc.). All sentinels we look for (`@`, `{`, `}`, ASCII
/// whitespace) are single-byte ASCII, so byte comparison cannot misfire on a
/// UTF-8 continuation byte.
pub fn extract_csharp_blocks(content: &str) -> String {
    let mut code = String::new();
    let bytes = content.as_bytes();
    let len = bytes.len();
    let mut i = 0;

    while i < len {
        // Look for @code, @functions, or standalone @{
        if bytes[i] == b'@' && i + 1 < len {
            let rest = &content[i..];

            let is_code_block = rest.starts_with("@code") && {
                let after = i + 5;
                after < len && (bytes[after].is_ascii_whitespace() || bytes[after] == b'{')
            };
            let is_functions_block = rest.starts_with("@functions") && {
                let after = i + 10;
                after < len && (bytes[after].is_ascii_whitespace() || bytes[after] == b'{')
            };
            let is_inline_block = bytes[i + 1] == b'{';

            if is_code_block || is_functions_block || is_inline_block {
                // Find the opening brace
                let search_from = if is_code_block {
                    i + 5
                } else if is_functions_block {
                    i + 10
                } else {
                    i + 1
                };

                if let Some(brace_offset) = bytes[search_from..].iter().position(|&c| c == b'{') {
                    let brace_start = search_from + brace_offset;
                    // Count braces to find matching close
                    let mut depth = 1;
                    let mut j = brace_start + 1;
                    while j < len && depth > 0 {
                        match bytes[j] {
                            b'{' => depth += 1,
                            b'}' => depth -= 1,
                            _ => {}
                        }
                        j += 1;
                    }
                    if depth == 0 {
                        // Extract content between braces (exclusive). `brace_start`
                        // and `j - 1` both point at ASCII `{`/`}`, so the surrounding
                        // byte slice is guaranteed to land on UTF-8 boundaries.
                        code.push_str(&content[brace_start + 1..j - 1]);
                        code.push('\n');
                        i = j;
                        continue;
                    }
                }
            }
        }
        i += 1;
    }

    code
}

// ── MVC HtmlHelper Extraction ────────────────────────────────────────────────

/// Extracted MVC HtmlHelper call from a Razor view.
#[derive(Debug, Clone)]
pub struct HtmlHelperCall {
    /// Helper type: "Partial", "RenderPartial", "ActionLink", "Action", "RenderAction", etc.
    pub helper_type: String,
    /// First argument (typically view name, action name, or controller name)
    pub target: String,
    /// Optional second argument (controller for ActionLink/Action)
    pub controller: Option<String>,
    /// Line number (0-indexed)
    pub line: usize,
}

/// Extract MVC HtmlHelper calls from Razor view content.
///
/// Detects patterns like:
/// - `@Html.Partial("_PartialName")`
/// - `@Html.RenderPartial("_PartialName")`
/// - `@Html.ActionLink("Text", "Action", "Controller")`
/// - `@Url.Action("Action", "Controller")`
/// - `@Html.RenderAction("Action", "Controller")`
/// - `@await Html.PartialAsync("_PartialName")`
/// - `<partial name="_PartialName" />`
pub fn extract_html_helpers(content: &str) -> Vec<HtmlHelperCall> {
    static HTML_HELPER_RE: Lazy<Regex> = Lazy::new(|| {
        Regex::new(
            r#"(?:@(?:await\s+)?Html\.(Partial|RenderPartial|PartialAsync|RenderPartialAsync|ActionLink|RenderAction|Action)\s*\(\s*"([^"]+)"(?:\s*,\s*"([^"]+)")?(?:\s*,\s*"([^"]+)")?)|(?:@Url\.(Action|RouteUrl)\s*\(\s*"([^"]+)"(?:\s*,\s*"([^"]+)")?)"#,
        )
        .unwrap()
    });

    static PARTIAL_TAG_RE: Lazy<Regex> =
        Lazy::new(|| Regex::new(r#"<partial\s+name="([^"]+)""#).unwrap());

    let mut helpers = Vec::new();

    for (line_num, line) in content.lines().enumerate() {
        // Standard Html.* helpers
        for cap in HTML_HELPER_RE.captures_iter(line) {
            if let Some(helper_match) = cap.get(1) {
                // Html.* pattern
                let helper_type = helper_match.as_str().to_string();
                let arg1 = cap
                    .get(2)
                    .map(|m| m.as_str().to_string())
                    .unwrap_or_default();
                let arg2 = cap.get(3).map(|m| m.as_str().to_string());
                let arg3 = cap.get(4).map(|m| m.as_str().to_string());

                // Argument semantics differ per helper type:
                //  - Partial / RenderPartial / *Async: (viewName, ...)
                //  - Action / RenderAction:            (actionName, controllerName?, ...)
                //  - ActionLink:                       (linkText, actionName, controllerName?, ...)
                //
                // Previously this code always used arg1 as `target` and arg2
                // as `controller`, which worked for Partial/Action but
                // mangled ActionLink: `@Html.ActionLink("Go Home", "Index",
                // "Home")` produced target="Go Home", controller="Index", so
                // the downstream consumer in `aspnet_mvc.rs::process_razor_
                // extras` built `called_name = "Index.Go Home"` — a string
                // that never resolved to any real method, silently losing
                // every `ActionLink` reference from the call graph.
                let (target, controller) = if helper_type == "ActionLink" {
                    // Discard the cosmetic link text (arg1); use action as
                    // target and controller as the receiver.
                    let action = arg2.unwrap_or_default();
                    (action, arg3)
                } else {
                    // Partial / Action / RenderAction / PartialAsync / RenderPartialAsync
                    (arg1, arg2)
                };

                helpers.push(HtmlHelperCall {
                    helper_type,
                    target,
                    controller,
                    line: line_num,
                });
            } else if let Some(url_helper) = cap.get(5) {
                // Url.* pattern
                let helper_type = url_helper.as_str().to_string();
                let target = cap.get(6).map_or("", |m| m.as_str()).to_string();
                let controller = cap.get(7).map(|m| m.as_str().to_string());

                helpers.push(HtmlHelperCall {
                    helper_type,
                    target,
                    controller,
                    line: line_num,
                });
            }
        }

        // <partial name="..." /> tag helper
        for cap in PARTIAL_TAG_RE.captures_iter(line) {
            if let Some(name_match) = cap.get(1) {
                helpers.push(HtmlHelperCall {
                    helper_type: "Partial".to_string(),
                    target: name_match.as_str().to_string(),
                    controller: None,
                    line: line_num,
                });
            }
        }
    }

    helpers
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_razor_directives() {
        let content = r#"
@page "/counter"
@model IndexModel
@using MyApp.Models
@inject IWeatherService Weather
@inherits ComponentBase
@implements IDisposable
"#;
        let directives = extract_razor_directives(content);
        assert_eq!(directives.len(), 6);
        assert_eq!(directives[0].directive, "page");
        assert_eq!(directives[0].value, "/counter");
        assert_eq!(directives[1].directive, "model");
        assert_eq!(directives[1].value, "IndexModel");
        assert_eq!(directives[3].directive, "inject");
        assert!(directives[3].value.contains("IWeatherService"));
    }

    #[test]
    fn test_extract_script_blocks() {
        let content = r#"
<div>Hello</div>
<script>
    function init() {
        console.log("hello");
    }
</script>
<p>More HTML</p>
<script type="text/javascript">
    var x = 42;
</script>
"#;
        let blocks = extract_script_blocks(content);
        assert_eq!(blocks.len(), 2);
        assert!(blocks[0].1.contains("function init()"));
        assert!(blocks[1].1.contains("var x = 42"));
    }

    #[test]
    fn test_extract_csharp_blocks() {
        let content = r#"
@page "/counter"
@code {
    private int count = 0;
    private void Increment() {
        count++;
    }
}
"#;
        let code = extract_csharp_blocks(content);
        assert!(code.contains("private int count = 0;"));
        assert!(code.contains("private void Increment()"));
    }

    #[test]
    fn test_default_configs_load() {
        let detector = ComponentDetector::new();
        assert!(
            !detector.libraries().is_empty(),
            "Should load at least one library config"
        );
    }

    #[test]
    fn test_detect_telerik_html_tag() {
        let detector = ComponentDetector::new();
        let content = r#"
@using Telerik.Blazor
<TelerikGrid Data="@GridData">
    <GridColumns>
        <GridColumn Field="Name" />
    </GridColumns>
</TelerikGrid>
"#;
        let results = detector.detect_in_file(content, "Pages/Index.razor");
        assert!(
            results.iter().any(|r| r.vendor == "Progress"),
            "Should detect Telerik/Progress component"
        );
    }

    #[test]
    fn test_detect_from_csproj() {
        let detector = ComponentDetector::new();
        let csproj = r#"
<Project Sdk="Microsoft.NET.Sdk.Web">
  <ItemGroup>
    <PackageReference Include="Telerik.UI.for.Blazor" Version="4.6.0" />
  </ItemGroup>
</Project>
"#;
        let results = detector.detect_in_csproj(csproj);
        assert!(!results.is_empty(), "Should detect Telerik from csproj");
        assert_eq!(results[0].detected_version.as_deref(), Some("4.6.0"));
    }

    #[test]
    fn test_extract_package_version() {
        let csproj = r#"<PackageReference Include="MyLib" Version="3.2.1" />"#;
        assert_eq!(
            extract_package_version(csproj, "MyLib"),
            Some("3.2.1".to_string())
        );
    }

    #[test]
    fn test_detect_kendo_data_role() {
        let detector = ComponentDetector::new();
        let content = r#"
<div id="grid" data-role="grid" data-bind="source: products"></div>
<script>
    kendo.init(document.body);
</script>
"#;
        let results = detector.detect_in_file(content, "Views/Home/Index.cshtml");
        assert!(
            results.iter().any(|r| r.library_name.contains("Kendo")),
            "Should detect Kendo UI"
        );
    }

    #[test]
    fn test_extract_csharp_blocks_deeply_nested() {
        // Regression test: the old regex-based approach only handled 1 level of nesting.
        // The brace-counting approach should handle arbitrary nesting.
        let content = r#"
@code {
    private void Process() {
        if (condition) {
            for (int i = 0; i < 10; i++) {
                DoSomething(i);
            }
        }
    }
}
"#;
        let code = extract_csharp_blocks(content);
        assert!(
            code.contains("private void Process()"),
            "Should extract method"
        );
        assert!(
            code.contains("DoSomething(i);"),
            "Should capture deeply nested code"
        );
        assert!(code.contains("for (int i = 0;"), "Should capture for loop");
    }

    #[test]
    fn test_extract_csharp_blocks_functions_directive() {
        let content = r#"
@functions {
    public string FormatDate(DateTime dt) {
        return dt.ToString("yyyy-MM-dd");
    }
}
"#;
        let code = extract_csharp_blocks(content);
        assert!(
            code.contains("FormatDate"),
            "Should extract @functions block"
        );
    }

    #[test]
    fn test_extract_csharp_blocks_multibyte_utf8() {
        // Regression: previously the function mixed char indices with byte
        // indices when slicing `content[i..]`, which panicked on any Razor
        // file containing multi-byte UTF-8 characters before a directive.
        let content = r#"
<!-- Bonjour les amis: éàü 🎉 -->
@code {
    private string greeting = "héllo";
}
"#;
        let code = extract_csharp_blocks(content);
        assert!(
            code.contains("greeting"),
            "Should extract @code block in presence of multi-byte UTF-8"
        );
        assert!(code.contains("héllo"), "Should preserve UTF-8 content");
    }

    #[test]
    fn test_extract_csharp_blocks_inline() {
        let content = r#"
<div>
@{
    var message = "Hello";
    ViewData["Title"] = message;
}
</div>
"#;
        let code = extract_csharp_blocks(content);
        assert!(
            code.contains("var message"),
            "Should extract @{{}} inline block"
        );
    }

    #[test]
    fn test_shared_detector_returns_same_instance() {
        let d1 = ComponentDetector::shared();
        let d2 = ComponentDetector::shared();
        assert_eq!(
            d1.libraries().len(),
            d2.libraries().len(),
            "Shared detector should return consistent data"
        );
    }

    #[test]
    fn test_css_class_prefix_mid_attribute() {
        let detector = ComponentDetector::new();
        // CSS class prefix should be detected even when not at the start of class attribute
        let content = r#"
<div class="container k-widget k-grid"></div>
"#;
        let results = detector.detect_in_file(content, "Views/Index.cshtml");
        assert!(
            results
                .iter()
                .any(|r| r.library_name.contains("Kendo") || r.vendor == "Progress"),
            "Should detect Kendo CSS prefix in mid-attribute position"
        );
    }

    #[test]
    fn test_extract_html_helpers_partial() {
        let content = r#"
<div>
    @Html.Partial("_Header")
    @Html.RenderPartial("_Sidebar")
    @await Html.PartialAsync("_Footer")
</div>
"#;
        let helpers = extract_html_helpers(content);
        assert_eq!(helpers.len(), 3);
        assert_eq!(helpers[0].helper_type, "Partial");
        assert_eq!(helpers[0].target, "_Header");
        assert_eq!(helpers[1].helper_type, "RenderPartial");
        assert_eq!(helpers[1].target, "_Sidebar");
        assert_eq!(helpers[2].helper_type, "PartialAsync");
        assert_eq!(helpers[2].target, "_Footer");
    }

    #[test]
    fn test_extract_html_helpers_action_link() {
        let content = r#"
@Html.ActionLink("Go Home", "Index", "Home")
@Url.Action("Details", "Products")
"#;
        let helpers = extract_html_helpers(content);
        assert_eq!(helpers.len(), 2);
        assert_eq!(helpers[0].helper_type, "ActionLink");
        // ActionLink(linkText, actionName, controllerName) — the first arg
        // is cosmetic link text and must be discarded. The action name
        // becomes the target and the controller name is the receiver.
        assert_eq!(helpers[0].target, "Index");
        assert_eq!(helpers[0].controller.as_deref(), Some("Home"));
        assert_eq!(helpers[1].helper_type, "Action");
        assert_eq!(helpers[1].target, "Details");
        assert_eq!(helpers[1].controller.as_deref(), Some("Products"));
    }

    #[test]
    fn test_extract_html_helpers_partial_tag() {
        let content = r#"<partial name="_LoginPartial" />"#;
        let helpers = extract_html_helpers(content);
        assert_eq!(helpers.len(), 1);
        assert_eq!(helpers[0].helper_type, "Partial");
        assert_eq!(helpers[0].target, "_LoginPartial");
    }

    #[test]
    fn test_extract_razor_directives_layout_and_namespace() {
        let content = r#"
@layout MainLayout
@namespace MyApp.Pages
@rendermode InteractiveServer
"#;
        let directives = extract_razor_directives(content);
        assert_eq!(directives.len(), 3);
        assert_eq!(directives[0].directive, "layout");
        assert_eq!(directives[0].value, "MainLayout");
        assert_eq!(directives[1].directive, "namespace");
        assert_eq!(directives[1].value, "MyApp.Pages");
        assert_eq!(directives[2].directive, "rendermode");
        assert_eq!(directives[2].value, "InteractiveServer");
    }

    #[test]
    fn test_razor_entry_point_scoring() {
        use crate::entry_point_scoring::score_name_for_language;
        use code_explorer_core::config::languages::SupportedLanguage;

        // Razor Page handlers
        let score = score_name_for_language("OnGet", SupportedLanguage::Razor).unwrap();
        assert!(
            score.score >= 0.9,
            "OnGet should be high-scoring entry point"
        );

        let score = score_name_for_language("OnPostAsync", SupportedLanguage::Razor).unwrap();
        assert!(score.score >= 0.9, "OnPostAsync should be high-scoring");

        // Blazor lifecycle
        let score =
            score_name_for_language("OnInitializedAsync", SupportedLanguage::Razor).unwrap();
        assert!(score.score >= 0.8, "OnInitializedAsync should score well");

        // Fallback to C#
        let score = score_name_for_language("Main", SupportedLanguage::Razor).unwrap();
        assert!(
            (score.score - 1.0).abs() < f64::EPSILON,
            "Main should fall back to C# scoring"
        );
    }
}

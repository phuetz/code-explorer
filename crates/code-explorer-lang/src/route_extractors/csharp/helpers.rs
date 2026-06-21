//! Internal helper functions and constants for C# route extraction.

use super::types::EntitySetInfo;

// ─── Constants ───────────────────────────────────────────────────────────

/// Base classes that identify a class as an MVC controller.
pub(super) const CONTROLLER_BASE_CLASSES: &[&str] = &[
    "Controller",
    "AsyncController",
    "ApiController",
    "ControllerBase",
    "ODataController",
];

/// Data annotation attributes commonly used on EF entities/ViewModels.
pub(super) const DATA_ANNOTATIONS: &[&str] = &[
    "Required",
    "MaxLength",
    "MinLength",
    "StringLength",
    "Range",
    "RegularExpression",
    "Compare",
    "EmailAddress",
    "Phone",
    "Url",
    "CreditCard",
    "DataType",
    "Display",
    "DisplayName",
    "DisplayFormat",
    "Key",
    "ForeignKey",
    "InverseProperty",
    "NotMapped",
    "Column",
    "Table",
    "Index",
    "Timestamp",
    "ConcurrencyCheck",
    "DatabaseGenerated",
    "ScaffoldColumn",
];

/// HTTP method attributes in ASP.NET MVC / Web API.
pub(super) const HTTP_ATTRIBUTES: &[(&str, &str)] = &[
    ("HttpGet", "GET"),
    ("HttpPost", "POST"),
    ("HttpPut", "PUT"),
    ("HttpDelete", "DELETE"),
    ("HttpPatch", "PATCH"),
    ("HttpHead", "HEAD"),
    ("HttpOptions", "OPTIONS"),
    ("AcceptVerbs", "MIXED"),
    ("GridAction", "GET"),
];

/// Standard filter attribute names recognized at high confidence.
pub(super) const STANDARD_FILTERS: &[&str] = &[
    "Authorize",
    "ValidateAntiForgeryToken",
    "OutputCache",
    "HandleError",
    "AllowAnonymous",
    "RequireHttps",
    "ActionFilter",
    "ExceptionFilter",
    "ResultFilter",
];

// ─── Internal Helpers ────────────────────────────────────────────────────

/// Parsed class declaration info.
pub(super) struct ClassMatch {
    pub(super) name: String,
    pub(super) base_classes: Vec<String>,
    pub(super) attributes: Vec<String>,
    pub(super) body_start_line: usize,
    pub(super) body_end_line: Option<usize>,
}

/// Find a class declaration starting from line `start`.
pub(super) fn find_class_declaration(lines: &[&str], start: usize) -> Option<ClassMatch> {
    // Collect attributes before the class
    let mut attributes = Vec::new();
    let mut _attr_start = start;

    // Look backwards from current position for attributes
    if start > 0 {
        let mut j = start;
        loop {
            if j == 0 {
                break;
            }
            j -= 1;
            let trimmed = lines[j].trim();
            if trimmed.starts_with('[') && trimmed.ends_with(']') {
                attributes.push(
                    trimmed
                        .get(1..trimmed.len() - 1)
                        .unwrap_or_default()
                        .to_string(),
                );
                _attr_start = j;
            } else if trimmed.is_empty() || trimmed.starts_with("//") {
                continue;
            } else {
                break;
            }
        }
    }

    // Check current line for class declaration
    let line = lines[start].trim();

    // Match pattern: [public|internal|...] [abstract|sealed|partial|static]* class ClassName [: BaseClass, IInterface]
    let class_keyword_idx = line.find(" class ")?;
    let after_class = line.get(class_keyword_idx + 7..)?;

    // Extract class name (before any : or { or < or where)
    let name_end = after_class
        .find([':', '{', '<', ' '])
        .unwrap_or(after_class.len());
    let name = after_class
        .get(..name_end)
        .unwrap_or_default()
        .trim()
        .to_string();

    if name.is_empty() {
        return None;
    }

    // Extract base classes (after :)
    //
    // Cut off any generic constraint clause (`where T : IBar`) before
    // looking for the inheritance colon. Otherwise:
    //
    //   class Foo<T> where T : IBar
    //
    // …has its FIRST `:` as the constraint colon, and we'd then push
    // `IBar` into `base_classes`, polluting the inheritance graph with a
    // phantom `Foo extends IBar` edge. Likewise:
    //
    //   class Foo<T> : Base where T : IBaz
    //
    // …without trimming `where`, the bases substring (`" Base where T : IBaz"`)
    // gets pushed verbatim as a single bogus base name. Truncating at the
    // first ` where ` makes both cases behave correctly.
    let after_class_no_where = if let Some(where_idx) = after_class.find(" where ") {
        after_class.get(..where_idx).unwrap_or(after_class)
    } else {
        after_class
    };

    let mut base_classes = Vec::new();
    if let Some(colon_idx) = after_class_no_where.find(':') {
        let bases_str = after_class_no_where
            .get(colon_idx + 1..)
            .unwrap_or_default();
        let bases_end = bases_str.find(['{', '\n']).unwrap_or(bases_str.len());
        for base in bases_str.get(..bases_end).unwrap_or_default().split(',') {
            let base_name = base.trim();
            // Handle generic base: Controller<T> -> Controller
            let clean = if let Some(lt) = base_name.find('<') {
                base_name.get(..lt).unwrap_or_default()
            } else {
                base_name
            };
            if !clean.is_empty() {
                base_classes.push(clean.trim().to_string());
            }
        }
    }

    // Also check if line itself has attributes
    if line.starts_with('[') {
        if let Some(attr_end) = line.find(']') {
            attributes.push(line.get(1..attr_end).unwrap_or_default().to_string());
        }
    }

    // Find body bounds (brace matching)
    let (body_start, body_end) = find_brace_bounds(lines, start);

    Some(ClassMatch {
        name,
        base_classes,
        attributes,
        body_start_line: body_start,
        body_end_line: body_end,
    })
}

/// Find matching braces for a class/method body starting from `start_line`.
///
/// Stray `}` characters in string literals or comments before the opening
/// brace must not underflow the depth counter, otherwise the function gets
/// stuck oscillating between 0 and -1 and never finds the real body end.
pub(super) fn find_brace_bounds(lines: &[&str], start_line: usize) -> (usize, Option<usize>) {
    let mut depth: i32 = 0;
    let mut found_first = false;
    let mut body_start = start_line;

    for (i, &line) in lines.iter().enumerate().skip(start_line) {
        for ch in line.chars() {
            if ch == '{' {
                if !found_first {
                    found_first = true;
                    body_start = i;
                }
                depth += 1;
            } else if ch == '}' {
                // Only count `}` after we've seen the opening `{`. This skips
                // stray closing braces in pre-body string literals/comments.
                if !found_first {
                    continue;
                }
                if depth > 0 {
                    depth -= 1;
                }
                if depth == 0 {
                    return (body_start, Some(i));
                }
            }
        }
    }

    (body_start, None)
}

/// Check if any base class is a known controller base.
pub(super) fn is_controller_class(base_classes: &[String]) -> bool {
    base_classes.iter().any(|b| {
        CONTROLLER_BASE_CLASSES
            .iter()
            .any(|cb| b == *cb || b.ends_with(cb))
    })
}

/// Extract a simple attribute value: [Attr("value")] -> Some("value")
pub(super) fn extract_attribute_value(attributes: &[String], attr_name: &str) -> Option<String> {
    for attr in attributes {
        let trimmed = attr.trim();
        if trimmed.starts_with(attr_name) {
            // Require a name boundary after `attr_name`. Without this, looking
            // up `"Route"` would match `RoutePrefix(...)` and return its value,
            // attributing the wrong attribute. Valid boundary chars are `(`,
            // `,`, ` `, `]`, or end-of-string.
            let next = trimmed.as_bytes().get(attr_name.len()).copied();
            let boundary_ok = matches!(
                next,
                None | Some(b'(') | Some(b',') | Some(b' ') | Some(b']')
            );
            if !boundary_ok {
                continue;
            }
            // Check for parenthesized value
            if let Some(paren_start) = trimmed.find('(') {
                let inner = trimmed.get(paren_start + 1..).unwrap_or_default();
                if let Some(paren_end) = inner.find(')') {
                    let value = inner.get(..paren_end).unwrap_or_default();
                    // Strip quotes
                    let clean = value.trim().trim_matches('"').trim_matches('\'');
                    return Some(clean.to_string());
                }
            }
            // Attribute without value: [Authorize] -> Some("")
            return Some(String::new());
        }
    }
    None
}

/// Check if a type name is a primitive or common simple type.
pub(super) fn is_primitive_type(name: &str) -> bool {
    matches!(
        name,
        "int"
            | "long"
            | "string"
            | "bool"
            | "float"
            | "double"
            | "decimal"
            | "byte"
            | "char"
            | "short"
            | "uint"
            | "ulong"
            | "ushort"
            | "DateTime"
            | "Guid"
            | "int?"
            | "long?"
            | "bool?"
            | "Nullable"
            | "CancellationToken"
            | "FormCollection"
            | "HttpPostedFileBase"
    )
}

/// Extract DbSet<T> from a line.
pub(super) fn extract_dbset(line: &str) -> Option<EntitySetInfo> {
    let trimmed = line.trim();
    // Pattern: public DbSet<EntityType> PropertyName { get; set; }
    // or: public virtual DbSet<EntityType> PropertyName { get; set; }
    // or: public IDbSet<EntityType> PropertyName { get; set; }
    let dbset_markers = ["DbSet<", "IDbSet<", "ObjectSet<"];
    for marker in dbset_markers {
        if let Some(idx) = trimmed.find(marker) {
            let after = trimmed.get(idx + marker.len()..)?;
            let type_end = after.find('>')?;
            let entity_type = after.get(..type_end).unwrap_or_default().trim().to_string();

            // Property name is after > and before {
            let after_type = after.get(type_end + 1..)?;
            let prop_name = after_type.split('{').next()?.trim().to_string();

            if !entity_type.is_empty() && !prop_name.is_empty() {
                return Some(EntitySetInfo {
                    entity_type,
                    property_name: prop_name,
                });
            }
        }
    }
    None
}

/// Extract connection string name from a constructor line.
pub(super) fn extract_connection_string(line: &str) -> Option<String> {
    let trimmed = line.trim();
    // Pattern: : base("connectionStringName") or "name=ConnectionStringName"
    if trimmed.contains("base(") || trimmed.contains("nameOrConnectionString") {
        if let Some(q1) = trimmed.find('"') {
            let after_q1 = trimmed.get(q1 + 1..)?;
            if let Some(q2) = after_q1.find('"') {
                let value = after_q1.get(..q2).unwrap_or_default();
                // Handle "name=X" format
                if let Some(rest) = value.strip_prefix("name=") {
                    return Some(rest.to_string());
                }
                return Some(value.to_string());
            }
        }
    }
    None
}

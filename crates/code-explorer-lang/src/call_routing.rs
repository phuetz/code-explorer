/// Result of routing a call expression through a language-specific router.
/// Only Ruby uses this in practice.
#[derive(Debug, Clone)]
pub enum CallRoutingResult {
    /// Route to import resolution (Ruby require/require_relative)
    Import {
        import_path: String,
        is_relative: bool,
    },
    /// Route to heritage processing (Ruby include/extend/prepend)
    Heritage { items: Vec<HeritageItem> },
    /// Route to property definition (Ruby attr_accessor/attr_reader/attr_writer)
    Properties { items: Vec<PropertyItem> },
    /// Treat as a regular call expression
    Call,
    /// Skip this call entirely
    Skip,
}

#[derive(Debug, Clone)]
pub struct HeritageItem {
    pub name: String,
    pub kind: HeritageKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeritageKind {
    Include,
    Extend,
    Prepend,
}

#[derive(Debug, Clone)]
pub struct PropertyItem {
    pub name: String,
    pub access: PropertyAccess,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PropertyAccess {
    ReadWrite,
    ReadOnly,
    WriteOnly,
}

/// Route a Ruby call to the appropriate action.
pub fn route_ruby_call(called_name: &str, call_text: &str) -> Option<CallRoutingResult> {
    match called_name {
        "require" | "require_relative" => {
            let is_relative = called_name == "require_relative";
            // Extract the string argument
            if let Some(path) = extract_string_arg(call_text) {
                if path.len() > 1024 || path.contains('\0') || path.chars().any(|c| c.is_control())
                {
                    return Some(CallRoutingResult::Skip);
                }
                Some(CallRoutingResult::Import {
                    import_path: path,
                    is_relative,
                })
            } else {
                Some(CallRoutingResult::Skip)
            }
        }
        "include" | "extend" | "prepend" => {
            let kind = match called_name {
                "include" => HeritageKind::Include,
                "extend" => HeritageKind::Extend,
                "prepend" => HeritageKind::Prepend,
                _ => unreachable!(),
            };
            let items = extract_constant_args(call_text)
                .into_iter()
                .map(|name| HeritageItem { name, kind })
                .collect();
            Some(CallRoutingResult::Heritage { items })
        }
        "attr_accessor" | "attr_reader" | "attr_writer" => {
            let access = match called_name {
                "attr_accessor" => PropertyAccess::ReadWrite,
                "attr_reader" => PropertyAccess::ReadOnly,
                "attr_writer" => PropertyAccess::WriteOnly,
                _ => unreachable!(),
            };
            let items = extract_symbol_args(call_text)
                .into_iter()
                .map(|name| PropertyItem { name, access })
                .collect();
            Some(CallRoutingResult::Properties { items })
        }
        _ => None, // Regular call
    }
}

/// Extract a string literal argument from a call expression text.
fn extract_string_arg(text: &str) -> Option<String> {
    // Match patterns like: require 'foo' or require("foo") or require_relative './bar'
    let text = text.trim();
    for quote in ['"', '\''] {
        if let Some(start) = text.find(quote) {
            if let Some(end) = text[start + 1..].find(quote) {
                return Some(text[start + 1..start + 1 + end].to_string());
            }
        }
    }
    None
}

/// Extract constant name arguments (for include/extend/prepend).
fn extract_constant_args(text: &str) -> Vec<String> {
    let mut results = Vec::new();
    // Match uppercase identifiers after the method name
    let parts: Vec<&str> = text.split_whitespace().collect();
    for part in parts.iter().skip(1) {
        let name = part.trim_matches(|c: char| !c.is_alphanumeric() && c != ':');
        if !name.is_empty() && name.chars().next().is_some_and(|c| c.is_uppercase()) {
            results.push(name.to_string());
        }
    }
    results
}

/// Extract symbol arguments (for attr_accessor/reader/writer).
fn extract_symbol_args(text: &str) -> Vec<String> {
    let mut results = Vec::new();
    // Match :symbol patterns
    for part in text.split(',') {
        let part = part.trim();
        if let Some(sym) = part.strip_prefix(':') {
            let name = sym
                .trim()
                .trim_matches(|c: char| !c.is_alphanumeric() && c != '_');
            if !name.is_empty() {
                results.push(name.to_string());
            }
        } else if part.contains(':') {
            // Could be in format `attr_accessor :name, :age`
            for sub in part.split(':') {
                let name = sub
                    .trim()
                    .trim_matches(|c: char| !c.is_alphanumeric() && c != '_');
                if !name.is_empty()
                    && name.chars().next().is_some_and(|c| c.is_lowercase())
                    && !["attr_accessor", "attr_reader", "attr_writer"].contains(&name)
                {
                    results.push(name.to_string());
                }
            }
        }
    }
    results
}

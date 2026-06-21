//! Razor view information extraction.

use super::types::ViewInfo;

/// Extract view information from a Razor file path and its source content.
pub fn extract_view_info(file_path: &str, source: &str) -> ViewInfo {
    let path_lower = file_path.to_lowercase().replace('\\', "/");
    let filename = path_lower.rsplit('/').next().unwrap_or("");

    // Extract @model directive
    let model_type = source.lines().find_map(|line| {
        let trimmed = line.trim();
        trimmed
            .strip_prefix("@model ")
            .map(|rest| rest.trim().to_string())
    });

    // Extract @{ Layout = "..."; } or @Layout = "..."
    let layout_path = source.lines().find_map(|line| {
        let trimmed = line.trim();
        if let Some(idx) = trimmed.find("Layout") {
            let after = trimmed.get(idx..)?;
            // Look for quoted string after Layout =
            if let Some(q1) = after.find('"') {
                let after_q1 = after.get(q1 + 1..)?;
                if let Some(q2) = after_q1.find('"') {
                    return Some(after_q1.get(..q2).unwrap_or_default().to_string());
                }
            }
        }
        None
    });

    // Infer area from path: Areas/<name>/Views/...
    //
    // Walk normalized path segments rather than reusing a byte offset from
    // `path_lower`. Unicode case folding can change byte length (Turkish
    // `İ` U+0130 → `i` U+0069), so a byte offset computed in `path_lower`
    // is not safe to apply to the original `file_path` and would silently
    // return `None` (or panic on a non-char-boundary in some Rust versions).
    let area_name = {
        let normalized = file_path.replace('\\', "/");
        let segments: Vec<&str> = normalized.split('/').collect();
        segments.iter().enumerate().find_map(|(i, seg)| {
            if seg.eq_ignore_ascii_case("areas") && i + 1 < segments.len() {
                Some(segments[i + 1].to_string())
            } else {
                None
            }
        })
    };

    let is_partial = filename.starts_with('_');

    ViewInfo {
        file_path: file_path.to_string(),
        model_type,
        layout_path,
        area_name,
        is_partial,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_view_info() {
        let info = extract_view_info(
            "Areas/Admin/Views/Products/Index.cshtml",
            "@model IEnumerable<MyApp.Models.Product>\n@{ Layout = \"~/Views/Shared/_Layout.cshtml\"; }",
        );
        assert_eq!(
            info.model_type.as_deref(),
            Some("IEnumerable<MyApp.Models.Product>")
        );
        assert_eq!(
            info.layout_path.as_deref(),
            Some("~/Views/Shared/_Layout.cshtml")
        );
        assert_eq!(info.area_name.as_deref(), Some("Admin"));
        assert!(!info.is_partial);
    }
}

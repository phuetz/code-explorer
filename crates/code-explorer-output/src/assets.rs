//! Pinned, locally bundled browser dependencies (see THIRD-PARTY-NOTICES.md).
pub const MERMAID_JS: &str = include_str!("../../../assets/vendor/mermaid.min.js");
pub const LUCIDE_JS: &str = include_str!("../../../assets/vendor/lucide.min.js");
pub const GRAPHOLOGY_JS: &str = include_str!("../../../assets/vendor/graphology.min.js");
pub const FORCEATLAS2_JS: &str = include_str!("../../../assets/vendor/forceatlas2.min.js");
pub const SIGMA_JS: &str = include_str!("../../../assets/vendor/sigma.min.js");

/// Prevent a library string from closing its enclosing HTML script element.
pub fn inline_script(source: &str) -> String {
    let mut result = String::with_capacity(source.len());
    let mut offset = 0;
    let lower = source.to_ascii_lowercase();
    while let Some(index) = lower[offset..].find("</script") {
        let index = offset + index;
        result.push_str(&source[offset..index]);
        result.push_str("<\\/");
        offset = index + 2;
    }
    result.push_str(&source[offset..]);
    result
}

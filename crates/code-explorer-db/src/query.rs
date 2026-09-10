//! Query utilities for Cypher string handling and result formatting.

use serde_json::Value;

/// Escape a string for safe embedding in a Cypher query.
///
/// Prevents Cypher injection by escaping single quotes, double quotes,
/// backslashes, and newlines.
pub fn escape_cypher_string(s: &str) -> String {
    let mut escaped = String::with_capacity(s.len() + 8);
    for ch in s.chars() {
        match ch {
            '\'' => escaped.push_str("\\'"),
            '"' => escaped.push_str("\\\""),
            '\\' => escaped.push_str("\\\\"),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            '\0' => {} // strip null bytes
            _ => escaped.push(ch),
        }
    }
    escaped
}

/// Strip single-line (`//`) and block (`/* */`) comments from a Cypher query.
///
/// String literals are tracked so that `//` or `/* */` appearing INSIDE a
/// quoted string are NOT mistaken for comment markers. Without this, a query
/// like `MATCH (n) WHERE n.text = "// CREATE foo" CREATE (m)...` would have
/// the in-string `//` strip everything to end-of-line including the real
/// `CREATE` that follows, bypassing `is_write_query`'s safety check.
fn strip_cypher_comments(query: &str) -> String {
    let mut result = String::with_capacity(query.len());
    let chars: Vec<char> = query.chars().collect();
    let mut i = 0;
    let mut in_string: Option<char> = None; // active string quote, if any
    while i < chars.len() {
        let c = chars[i];
        if let Some(quote) = in_string {
            // Inside a string literal: emit verbatim. Honor `\X` escape
            // sequences so a `\"` inside a double-quoted string is not
            // mis-interpreted as the closing quote, and likewise for `\'`.
            if c == '\\' && i + 1 < chars.len() {
                result.push(c);
                result.push(chars[i + 1]);
                i += 2;
                continue;
            }
            result.push(c);
            if c == quote {
                in_string = None;
            }
            i += 1;
        } else if c == '\'' || c == '"' {
            // Enter string literal — emit the opening quote.
            in_string = Some(c);
            result.push(c);
            i += 1;
        } else if i + 1 < chars.len() && c == '/' && chars[i + 1] == '/' {
            // Skip line comment
            while i < chars.len() && chars[i] != '\n' {
                i += 1;
            }
        } else if i + 1 < chars.len() && c == '/' && chars[i + 1] == '*' {
            // Skip block comment
            i += 2;
            while i + 1 < chars.len() && !(chars[i] == '*' && chars[i + 1] == '/') {
                i += 1;
            }
            i += 2; // skip */
        } else {
            result.push(c);
            i += 1;
        }
    }
    result
}

/// Check if a Cypher query is a write/mutation query.
///
/// Returns `true` if the query contains any write operations:
/// CREATE, DELETE, SET, MERGE, DROP, REMOVE, DETACH, ALTER.
/// Uses case-insensitive word-boundary matching to avoid false positives
/// (e.g., matching "CREATE" inside a string literal is acceptable for
/// safety since we err on the side of caution).
/// Comments are stripped before checking to prevent bypass via comment injection.
pub fn is_write_query(query: &str) -> bool {
    let cleaned = strip_cypher_comments(query);
    let upper = cleaned.to_uppercase();
    // Check for write keywords as whole words (preceded by whitespace or start-of-string)
    let write_patterns = [
        "CREATE", "DELETE", "SET", "MERGE", "DROP", "REMOVE", "DETACH", "ALTER",
    ];

    for pattern in &write_patterns {
        // Check ALL occurrences of the pattern, not just the first
        let pattern_len = pattern.len();
        let bytes = upper.as_bytes();
        let mut search_start = 0;
        while let Some(offset) = upper[search_start..].find(pattern) {
            let pos = search_start + offset;
            search_start = pos + 1;

            // Left boundary: start of string or preceded by whitespace/paren/brace/semicolon
            let left_ok =
                pos == 0 || matches!(bytes[pos - 1], b' ' | b'\n' | b'\t' | b'(' | b'{' | b';');
            if !left_ok {
                continue;
            }

            // Right boundary: end of string or followed by whitespace/paren/brace/semicolon
            let end_pos = pos + pattern_len;
            let right_ok = end_pos >= bytes.len()
                || matches!(
                    bytes[end_pos],
                    b' ' | b'\n' | b'\t' | b'(' | b')' | b'{' | b'}' | b';'
                );
            if right_ok {
                return true;
            }
        }
    }

    false
}

/// Format query result rows as a JSON array string.
pub fn format_query_result(rows: &[Value]) -> String {
    let arr = Value::Array(rows.to_vec());
    serde_json::to_string_pretty(&arr).unwrap_or_else(|_| "[]".to_string())
}

/// Format query result rows as a compact JSON array string.
pub fn format_query_result_compact(rows: &[Value]) -> String {
    let arr = Value::Array(rows.to_vec());
    serde_json::to_string(&arr).unwrap_or_else(|_| "[]".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_escape_cypher_string() {
        assert_eq!(escape_cypher_string("hello"), "hello");
        assert_eq!(escape_cypher_string("it's"), "it\\'s");
        assert_eq!(escape_cypher_string("line\nbreak"), "line\\nbreak");
        assert_eq!(escape_cypher_string(r#"say "hi""#), r#"say \"hi\""#);
        assert_eq!(escape_cypher_string("back\\slash"), "back\\\\slash");
        assert_eq!(escape_cypher_string("null\x00byte"), "nullbyte");
    }

    #[test]
    fn test_is_write_query() {
        // Write queries
        assert!(is_write_query("CREATE (n:File {id: '1'})"));
        assert!(is_write_query("MATCH (n) DELETE n"));
        assert!(is_write_query("MATCH (n) SET n.name = 'x'"));
        assert!(is_write_query("MERGE (n:File {id: '1'})"));
        assert!(is_write_query("DROP TABLE File"));
        assert!(is_write_query("MATCH (n) REMOVE n.name"));
        assert!(is_write_query("MATCH (n) DETACH DELETE n"));

        // Read queries
        assert!(!is_write_query("MATCH (n:File) RETURN n"));
        assert!(!is_write_query("MATCH (n)-[r]->(m) RETURN n, r, m"));
        assert!(!is_write_query(
            "MATCH (n:File) WHERE n.name = 'test' RETURN n"
        ));
    }

    #[test]
    fn test_is_write_query_case_insensitive() {
        assert!(is_write_query("create (n:File {id: '1'})"));
        assert!(is_write_query("match (n) delete n"));
    }

    #[test]
    fn test_is_write_query_string_literal_comment_bypass() {
        // Regression: a `//` inside a string literal must NOT be treated as
        // a line comment, otherwise stripping it would also discard a real
        // CREATE / DELETE / SET that follows on the same line and bypass
        // the write check entirely.
        assert!(is_write_query(
            "MATCH (n) WHERE n.text = \"// foo\" CREATE (m:File {id: 'x'}) RETURN m"
        ));
        assert!(is_write_query(
            "MATCH (n) WHERE n.text = '/* x */' DELETE n"
        ));
        // Comments OUTSIDE strings still get stripped, so a write keyword
        // hidden in a real comment is not flagged.
        assert!(!is_write_query("// CREATE (n:File)\nMATCH (n) RETURN n"));
    }

    #[test]
    fn test_format_query_result() {
        let rows = vec![
            json!({"name": "main", "type": "Function"}),
            json!({"name": "User", "type": "Class"}),
        ];
        let formatted = format_query_result(&rows);
        assert!(formatted.contains("main"));
        assert!(formatted.contains("User"));

        let compact = format_query_result_compact(&rows);
        assert!(!compact.contains('\n'));
    }

    #[test]
    fn test_format_empty_result() {
        let rows: Vec<Value> = vec![];
        assert_eq!(format_query_result(&rows), "[]");
    }
}

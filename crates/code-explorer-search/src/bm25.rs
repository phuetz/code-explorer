//! BM25 full-text search via Cypher FTS queries.
//!
//! Executes FTS queries across 17 searchable tables and merges results
//! by file_path with summed scores.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use code_explorer_db::adapter::DbAdapter;
use code_explorer_db::error::DbError;
use code_explorer_db::query::escape_cypher_string;

/// Tables that have FTS indexes.
/// Matches the 17 tables defined in code-explorer-db schema::fts_queries().
const FTS_TABLES: &[&str] = &[
    "File",
    "Function",
    "Class",
    "Method",
    "Interface",
    "Controller",
    "ControllerAction",
    "ApiEndpoint",
    "View",
    "ViewModel",
    "DbEntity",
    "DbContext",
    "ScriptFile",
    "UiComponent",
    "Service",
    "Repository",
    "ExternalService",
];

/// A single BM25 search result.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BM25SearchResult {
    pub file_path: String,
    pub score: f64,
    pub rank: usize,
    pub node_id: String,
    pub name: String,
    pub label: String,
    pub start_line: Option<u32>,
    pub end_line: Option<u32>,
}

/// Execute a BM25 full-text search across all FTS-indexed tables.
///
/// Queries each table's FTS index, collects results, merges by file_path
/// (summing scores), and returns sorted results.
pub fn search_fts(
    adapter: &DbAdapter,
    query_text: &str,
    limit: usize,
) -> std::result::Result<Vec<BM25SearchResult>, DbError> {
    let escaped = escape_cypher_string(query_text);
    let mut raw_results: Vec<BM25SearchResult> = Vec::new();

    for table in FTS_TABLES {
        let cypher = build_fts_query(table, &escaped, limit);
        match adapter.execute_query(&cypher) {
            Ok(rows) => {
                for row in rows {
                    if let Some(result) = parse_fts_row(&row, table) {
                        raw_results.push(result);
                    }
                }
            }
            Err(e) => {
                tracing::warn!("FTS query failed for {table}: {e}");
                // Continue with other tables
            }
        }
    }

    // Merge results by file_path, summing scores
    let merged = merge_by_file_path(raw_results);

    // Sort by score descending and assign ranks
    let mut sorted: Vec<BM25SearchResult> = merged.into_values().collect();
    sorted.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    sorted.truncate(limit);

    for (i, result) in sorted.iter_mut().enumerate() {
        result.rank = i + 1;
    }

    Ok(sorted)
}

/// Build a Cypher FTS query for a specific table.
///
/// The in-memory Cypher executor's `parse_call` only reads `funcname(args...)`
/// and stops at the closing `)`; YIELD/WITH/ORDER BY/LIMIT/RETURN clauses are
/// silently ignored, and `execute_call` hardcodes its own per-table limit.
/// Result keys come from `fts_result_to_json`, not from RETURN aliases.
///
/// We deliberately emit only the bits the parser actually consumes — adding
/// fake clauses here just misleads future readers into thinking they take effect.
fn build_fts_query(table: &str, escaped_query: &str, _limit: usize) -> String {
    format!("CALL QUERY_FTS_INDEX('fts_{table}', '{escaped_query}')")
}

/// Parse a JSON row from an FTS query result into a BM25SearchResult.
fn parse_fts_row(row: &Value, table: &str) -> Option<BM25SearchResult> {
    let file_path = row.get("filePath")?.as_str()?.to_string();
    let score = row.get("score")?.as_f64()?;
    let node_id = row
        .get("nodeId")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let name = row
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let start_line = row
        .get("startLine")
        .and_then(|v| v.as_u64())
        .map(|v| v as u32);
    let end_line = row
        .get("endLine")
        .and_then(|v| v.as_u64())
        .map(|v| v as u32);

    Some(BM25SearchResult {
        file_path,
        score,
        rank: 0, // Assigned after sorting
        node_id,
        name,
        label: table.to_string(),
        start_line,
        end_line,
    })
}

/// Merge results by file_path, keeping the **best** single-node score (MAX)
/// for each file along with that node's metadata.
///
/// Previously this summed per-file scores, which inflated files with many
/// small matches (e.g. minified JavaScript bundles where every query token
/// happens to occur in a function name). Taking the max keeps ranking driven
/// by the strongest individual match, which aligns with "best symbol in the
/// file" — the mental model the chat UI ends up surfacing to users.
fn merge_by_file_path(results: Vec<BM25SearchResult>) -> HashMap<String, BM25SearchResult> {
    let mut merged: HashMap<String, BM25SearchResult> = HashMap::new();

    for result in results {
        let file_path = result.file_path.clone();
        merged
            .entry(file_path)
            .and_modify(|existing| {
                if result.score > existing.score {
                    existing.score = result.score;
                    existing.node_id = result.node_id.clone();
                    existing.name = result.name.clone();
                    existing.label = result.label.clone();
                    existing.start_line = result.start_line;
                    existing.end_line = result.end_line;
                }
            })
            .or_insert(result);
    }

    merged
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_fts_query() {
        let q = build_fts_query("Function", "handleLogin", 10);
        assert!(q.contains("fts_Function"));
        assert!(q.contains("handleLogin"));
        // The in-memory parser doesn't honor YIELD/LIMIT/RETURN clauses on
        // CALL queries, so build_fts_query intentionally emits only the bare
        // CALL — see the doc-comment on the function.
        assert!(q.starts_with("CALL QUERY_FTS_INDEX"));
    }

    #[test]
    fn test_merge_by_file_path() {
        let results = vec![
            BM25SearchResult {
                file_path: "src/a.ts".into(),
                score: 2.0,
                rank: 0,
                node_id: "f1".into(),
                name: "foo".into(),
                label: "Function".into(),
                start_line: Some(1),
                end_line: Some(10),
            },
            BM25SearchResult {
                file_path: "src/a.ts".into(),
                score: 1.0,
                rank: 0,
                node_id: "f2".into(),
                name: "bar".into(),
                label: "Method".into(),
                start_line: Some(20),
                end_line: Some(30),
            },
            BM25SearchResult {
                file_path: "src/b.ts".into(),
                score: 3.0,
                rank: 0,
                node_id: "f3".into(),
                name: "baz".into(),
                label: "Class".into(),
                start_line: Some(1),
                end_line: Some(50),
            },
        ];

        let merged = merge_by_file_path(results);
        assert_eq!(merged.len(), 2);
        // MAX-merge: best single-symbol score wins per file.
        // src/a.ts had 2.0 (foo) and 1.0 (bar) → keeps 2.0 + foo's metadata.
        assert!((merged["src/a.ts"].score - 2.0).abs() < f64::EPSILON);
        assert_eq!(merged["src/a.ts"].node_id, "f1");
        assert_eq!(merged["src/a.ts"].name, "foo");
        // src/b.ts only had one result at 3.0 → unchanged.
        assert!((merged["src/b.ts"].score - 3.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_parse_fts_row() {
        let row = serde_json::json!({
            "nodeId": "Function:src/main.ts:handleLogin",
            "name": "handleLogin",
            "filePath": "src/main.ts",
            "startLine": 10,
            "endLine": 25,
            "score": 4.5
        });

        let result = parse_fts_row(&row, "Function").unwrap();
        assert_eq!(result.file_path, "src/main.ts");
        assert!((result.score - 4.5).abs() < f64::EPSILON);
        assert_eq!(result.label, "Function");
        assert_eq!(result.start_line, Some(10));
    }

    #[test]
    fn test_parse_fts_row_missing_fields() {
        let row = serde_json::json!({"name": "test"});
        assert!(parse_fts_row(&row, "File").is_none());
    }
}

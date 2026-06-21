use tauri::State;

use code_explorer_db::inmemory::cypher;

use crate::state::AppState;

/// Execute a raw Cypher query (read-only).
#[tauri::command]
pub async fn execute_cypher(
    state: State<'_, AppState>,
    query: String,
) -> Result<Vec<serde_json::Value>, String> {
    let (graph, indexes, fts_index, _repo_path) = state.get_repo(None).await?;

    // Read-only is enforced by the parser: it only accepts MATCH and CALL
    // statements, rejecting CREATE/DELETE/MERGE/SET/DROP/REMOVE at parse time.
    let stmt = cypher::parse(&query).map_err(|e| format!("Parse error: {}", e))?;
    cypher::execute(&stmt, &graph, &indexes, &fts_index).map_err(|e| format!("Query error: {}", e))
}

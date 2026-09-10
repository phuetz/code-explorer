//! Workflow editor — orchestrates typed steps in sequence.
//!
//! A workflow is a linear pipeline of `WorkflowStep`s. Each step has a kind
//! (`search` / `cypher` / `read_file` / `impact`) plus a JSON params object.
//! Step output is captured (text + JSON when applicable) and made available
//! to subsequent steps via `{{step_N.field}}` template interpolation, which
//! happens *just before* the step runs.
//!
//! Persistence: `<.codeexplorer>/workflows/<id>.json`, one file per workflow so
//! they're easy to share or git-version. Run produces a per-step result
//! list with timing and any error.

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Instant;

use serde::{Deserialize, Serialize};
use tauri::State;
use uuid::Uuid;

use code_explorer_core::graph::types::*;
use code_explorer_core::llm::PROMPT_CONTEXT_SAFETY;

use crate::commands::chat;
use crate::state::AppState;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowStep {
    pub id: String,
    /// "search" | "cypher" | "impact" | "read_file" | "llm"
    pub kind: String,
    /// Free-text label shown in the UI.
    pub label: String,
    /// Step-specific parameters; values may contain `{{step_<N>.field}}`
    /// placeholders that will be expanded against earlier outputs.
    pub params: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Workflow {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    pub steps: Vec<WorkflowStep>,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowSummary {
    pub id: String,
    pub name: String,
    pub step_count: u32,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StepRun {
    pub step_id: String,
    pub label: String,
    pub kind: String,
    pub status: String, // "ok" | "error" | "skipped"
    pub duration_ms: u64,
    /// Human-readable rendering used as input by later steps when they
    /// reference `{{step_N.text}}`.
    pub text: String,
    /// Structured JSON output (when the kind produces one) for later
    /// `{{step_N.json.path}}` references.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub json: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowRunResult {
    pub workflow_id: String,
    pub steps: Vec<StepRun>,
    pub total_ms: u64,
}

// ─── Persistence ────────────────────────────────────────────────────

fn workflows_dir(storage: &str) -> PathBuf {
    PathBuf::from(storage).join("workflows")
}

fn workflow_path(storage: &str, id: &str) -> Result<PathBuf, String> {
    let safe: String = id
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-')
        .collect();
    if safe.is_empty() {
        return Err("Invalid id: must contain at least one alphanumeric character".into());
    }
    Ok(workflows_dir(storage).join(format!("{safe}.json")))
}

#[tauri::command]
pub async fn workflow_list(state: State<'_, AppState>) -> Result<Vec<WorkflowSummary>, String> {
    let storage = state.active_storage_path().await?;
    let dir = workflows_dir(&storage);
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    for entry in std::fs::read_dir(&dir).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let s = match std::fs::read_to_string(&path) {
            Ok(s) => s,
            Err(_) => continue,
        };
        let wf: Workflow = match serde_json::from_str(&s) {
            Ok(w) => w,
            Err(_) => continue,
        };
        out.push(WorkflowSummary {
            id: wf.id,
            name: wf.name,
            step_count: wf.steps.len() as u32,
            updated_at: wf.updated_at,
        });
    }
    out.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    Ok(out)
}

#[tauri::command]
pub async fn workflow_load(state: State<'_, AppState>, id: String) -> Result<Workflow, String> {
    let storage = state.active_storage_path().await?;
    let path = workflow_path(&storage, &id)?;
    let s =
        std::fs::read_to_string(&path).map_err(|e| format!("Workflow '{id}' not found: {e}"))?;
    serde_json::from_str(&s).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn workflow_save(
    state: State<'_, AppState>,
    workflow: Workflow,
) -> Result<WorkflowSummary, String> {
    let storage = state.active_storage_path().await?;
    std::fs::create_dir_all(workflows_dir(&storage)).map_err(|e| e.to_string())?;
    let mut wf = workflow;
    if wf.id.is_empty() {
        wf.id = format!("wf_{}", Uuid::new_v4().simple());
    }
    wf.updated_at = chrono::Utc::now().timestamp_millis();
    let path = workflow_path(&storage, &wf.id)?;
    let s = serde_json::to_string_pretty(&wf).map_err(|e| e.to_string())?;
    std::fs::write(&path, s).map_err(|e| e.to_string())?;
    Ok(WorkflowSummary {
        id: wf.id,
        name: wf.name,
        step_count: wf.steps.len() as u32,
        updated_at: wf.updated_at,
    })
}

#[tauri::command]
pub async fn workflow_delete(state: State<'_, AppState>, id: String) -> Result<(), String> {
    let storage = state.active_storage_path().await?;
    let path = workflow_path(&storage, &id)?;
    if path.exists() {
        std::fs::remove_file(&path).map_err(|e| e.to_string())?;
    }
    Ok(())
}

// ─── Execution ──────────────────────────────────────────────────────

#[tauri::command]
pub async fn workflow_run(
    state: State<'_, AppState>,
    workflow: Workflow,
) -> Result<WorkflowRunResult, String> {
    let t_start = Instant::now();
    let mut runs: Vec<StepRun> = Vec::new();
    let (graph, indexes, _fts, repo_path_str) = state.get_repo(None).await?;
    let repo_path = PathBuf::from(&repo_path_str);
    let chat_config = chat::load_config_pub(&state).await;

    for (idx, step) in workflow.steps.iter().enumerate() {
        let t0 = Instant::now();
        // Interpolate placeholders against earlier step results.
        let resolved_params = interpolate_params(&step.params, &runs);
        let (text, json, error) = match step.kind.as_str() {
            "cypher" => run_cypher(&state, &resolved_params).await,
            "search" => run_search(&graph, &indexes, &resolved_params),
            "impact" => run_impact(&graph, &indexes, &resolved_params),
            "read_file" => run_read_file(&repo_path, &resolved_params),
            "llm" => run_llm(&chat_config, &resolved_params, &runs).await,
            other => (
                String::new(),
                None,
                Some(format!("Unknown step kind: {other}")),
            ),
        };
        let status = if error.is_some() { "error" } else { "ok" };
        runs.push(StepRun {
            step_id: step.id.clone(),
            label: format!("step_{}: {}", idx + 1, step.label),
            kind: step.kind.clone(),
            status: status.into(),
            duration_ms: t0.elapsed().as_millis() as u64,
            text,
            json,
            error,
        });
        // If a step errors, stop the pipeline — downstream steps probably
        // depend on its output.
        if status == "error" {
            break;
        }
    }

    Ok(WorkflowRunResult {
        workflow_id: workflow.id,
        total_ms: t_start.elapsed().as_millis() as u64,
        steps: runs,
    })
}

// ─── Step runners ───────────────────────────────────────────────────

async fn run_cypher(
    state: &State<'_, AppState>,
    params: &serde_json::Value,
) -> (String, Option<serde_json::Value>, Option<String>) {
    let q = match params.get("query").and_then(|v| v.as_str()) {
        Some(s) => s.to_string(),
        None => return (String::new(), None, Some("Missing 'query'".into())),
    };
    let entry = match crate::commands::cypher::execute_cypher(state.clone(), q).await {
        Ok(rows) => rows,
        Err(e) => return (String::new(), None, Some(e)),
    };
    let json = serde_json::to_value(&entry).unwrap_or(serde_json::json!(null));
    let text = format!("{} row(s)", entry.len());
    (text, Some(json), None)
}

fn run_search(
    graph: &code_explorer_core::graph::KnowledgeGraph,
    _indexes: &code_explorer_db::inmemory::cypher::GraphIndexes,
    params: &serde_json::Value,
) -> (String, Option<serde_json::Value>, Option<String>) {
    let q = match params.get("query").and_then(|v| v.as_str()) {
        Some(s) => s.to_string(),
        None => return (String::new(), None, Some("Missing 'query'".into())),
    };
    // Lightweight name-substring search to avoid pulling extra deps in
    // this module — the chat panel has the full FTS path.
    let lower = q.to_lowercase();
    let mut hits: Vec<serde_json::Value> = Vec::new();
    for n in graph.iter_nodes() {
        if n.properties.name.to_lowercase().contains(&lower) {
            hits.push(serde_json::json!({
                "id": n.id,
                "name": n.properties.name,
                "label": n.label.as_str(),
                "filePath": n.properties.file_path,
            }));
            if hits.len() >= 25 {
                break;
            }
        }
    }
    let text = format!("{} hit(s)", hits.len());
    (text, Some(serde_json::Value::Array(hits)), None)
}

fn run_impact(
    graph: &code_explorer_core::graph::KnowledgeGraph,
    _indexes: &code_explorer_db::inmemory::cypher::GraphIndexes,
    params: &serde_json::Value,
) -> (String, Option<serde_json::Value>, Option<String>) {
    let target = match params.get("target").and_then(|v| v.as_str()) {
        Some(s) => s.to_string(),
        None => return (String::new(), None, Some("Missing 'target'".into())),
    };
    let max_depth = params.get("maxDepth").and_then(|v| v.as_u64()).unwrap_or(3) as usize;

    let target_lower = target.to_lowercase();
    let target_ids: std::collections::HashSet<String> = graph
        .iter_nodes()
        .filter(|n| n.id == target || n.properties.name.to_lowercase() == target_lower)
        .map(|n| n.id.clone())
        .collect();
    if target_ids.is_empty() {
        return (
            String::new(),
            None,
            Some(format!("Symbol '{target}' not found")),
        );
    }
    // Reverse adjacency over causal edges (mirrors tool_impact).
    let want = |rt: RelationshipType| {
        matches!(
            rt,
            RelationshipType::Calls
                | RelationshipType::Uses
                | RelationshipType::Imports
                | RelationshipType::Inherits
                | RelationshipType::Implements
                | RelationshipType::Extends
                | RelationshipType::Overrides
                | RelationshipType::CallsAction
                | RelationshipType::CallsService
        )
    };
    let mut reverse: HashMap<String, Vec<String>> = HashMap::new();
    for rel in graph.iter_relationships() {
        if want(rel.rel_type) {
            reverse
                .entry(rel.target_id.clone())
                .or_default()
                .push(rel.source_id.clone());
        }
    }
    let mut affected: std::collections::HashSet<String> = target_ids.clone();
    let mut q: std::collections::VecDeque<(String, usize)> =
        target_ids.iter().map(|id| (id.clone(), 0usize)).collect();
    while let Some((node, depth)) = q.pop_front() {
        if depth >= max_depth {
            continue;
        }
        if let Some(ns) = reverse.get(&node) {
            for n in ns {
                if affected.insert(n.clone()) {
                    q.push_back((n.clone(), depth + 1));
                }
            }
        }
    }
    let count = affected.len().saturating_sub(target_ids.len()) as u32;
    let mut sample: Vec<serde_json::Value> = Vec::new();
    for id in affected
        .iter()
        .filter(|i| !target_ids.contains(*i))
        .take(20)
    {
        if let Some(n) = graph.get_node(id) {
            sample.push(serde_json::json!({
                "id": id,
                "name": n.properties.name,
                "label": n.label.as_str(),
                "filePath": n.properties.file_path,
            }));
        }
    }
    let text = format!("{count} upstream node(s) within depth {max_depth}");
    (
        text,
        Some(serde_json::json!({"count": count, "sample": sample})),
        None,
    )
}

fn run_read_file(
    repo_path: &std::path::Path,
    params: &serde_json::Value,
) -> (String, Option<serde_json::Value>, Option<String>) {
    let rel = match params.get("path").and_then(|v| v.as_str()) {
        Some(s) => s.to_string(),
        None => return (String::new(), None, Some("Missing 'path'".into())),
    };
    let full = repo_path.join(&rel);
    let canonical_repo = match repo_path.canonicalize() {
        Ok(p) => p,
        Err(e) => return (String::new(), None, Some(e.to_string())),
    };
    let canonical_file = match full.canonicalize() {
        Ok(p) => p,
        Err(e) => return (String::new(), None, Some(e.to_string())),
    };
    if !canonical_file.starts_with(&canonical_repo) {
        return (String::new(), None, Some("Path escapes repo".into()));
    }
    match std::fs::read_to_string(&canonical_file) {
        Ok(content) => {
            let limit = params
                .get("maxBytes")
                .and_then(|v| v.as_u64())
                .unwrap_or(8000) as usize;
            let text = if content.len() > limit {
                let boundary = content
                    .char_indices()
                    .take_while(|(i, _)| *i < limit)
                    .last()
                    .map(|(i, c)| i + c.len_utf8())
                    .unwrap_or(0);
                format!("{}…\n[truncated to {limit} bytes]", &content[..boundary])
            } else {
                content
            };
            (text, None, None)
        }
        Err(e) => (String::new(), None, Some(e.to_string())),
    }
}

async fn run_llm(
    config: &crate::types::ChatConfig,
    params: &serde_json::Value,
    runs: &[StepRun],
) -> (String, Option<serde_json::Value>, Option<String>) {
    let prompt = match params.get("prompt").and_then(|v| v.as_str()) {
        Some(s) => s.to_string(),
        None => return (String::new(), None, Some("Missing 'prompt'".into())),
    };
    let system = params
        .get("system")
        .and_then(|v| v.as_str())
        .unwrap_or("You are a helpful assistant. Respond concisely.")
        .to_string();
    let guarded_system = format!("{}\n\n{}", PROMPT_CONTEXT_SAFETY, system);
    // Append the prior step outputs so the LLM has automatic context
    // even without explicit `{{step_N.text}}` interpolation in the prompt.
    let mut user = prompt;
    if !runs.is_empty() {
        user.push_str("\n\n## Prior step outputs\n");
        for r in runs {
            user.push_str(&format!(
                "\n### {} ({}ms)\n{}\n",
                r.label, r.duration_ms, r.text
            ));
        }
    }
    let messages = vec![
        serde_json::json!({"role": "system", "content": guarded_system}),
        serde_json::json!({"role": "user", "content": user}),
    ];
    match chat::call_llm_pub(config, &messages).await {
        Ok(text) => (text, None, None),
        Err(e) => (String::new(), None, Some(e)),
    }
}

// ─── Template interpolation ─────────────────────────────────────────

/// Replace `{{step_N.text}}` and `{{step_N.json.path.to.field}}` placeholders
/// inside every string value of the params object. JSON paths use `.` as a
/// separator and support array indexing via `.[N]`.
fn interpolate_params(params: &serde_json::Value, runs: &[StepRun]) -> serde_json::Value {
    fn replace_str(s: &str, runs: &[StepRun]) -> String {
        let mut out = String::with_capacity(s.len());
        let mut rest = s;
        while !rest.is_empty() {
            if let Some(pos) = rest.find("{{") {
                out.push_str(&rest[..pos]);
                let after_open = &rest[pos + 2..];
                if let Some(end) = after_open.find("}}") {
                    let token = &after_open[..end];
                    out.push_str(&resolve_token(token.trim(), runs));
                    rest = &after_open[end + 2..];
                } else {
                    out.push_str("{{");
                    rest = after_open;
                }
            } else {
                out.push_str(rest);
                break;
            }
        }
        out
    }
    fn resolve_token(token: &str, runs: &[StepRun]) -> String {
        // Expected: "step_N.text" or "step_N.json.<path>"
        let mut parts = token.splitn(2, '.');
        let step_part = parts.next().unwrap_or("");
        let rest = parts.next().unwrap_or("");
        let n: usize = step_part
            .strip_prefix("step_")
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);
        if n == 0 || n > runs.len() {
            return format!("{{{{{token}}}}}"); // leave unresolved
        }
        let r = &runs[n - 1];
        if rest == "text" || rest.is_empty() {
            return r.text.clone();
        }
        if let Some(json_path) =
            rest.strip_prefix("json.")
                .or_else(|| if rest == "json" { Some("") } else { None })
        {
            let mut cursor = match &r.json {
                Some(v) => v.clone(),
                None => return String::new(),
            };
            if !json_path.is_empty() {
                for segment in json_path.split('.') {
                    if let Some(idx_str) =
                        segment.strip_prefix('[').and_then(|s| s.strip_suffix(']'))
                    {
                        if let Ok(idx) = idx_str.parse::<usize>() {
                            cursor = cursor
                                .as_array()
                                .and_then(|a| a.get(idx).cloned())
                                .unwrap_or(serde_json::Value::Null);
                            continue;
                        }
                    }
                    cursor = cursor
                        .get(segment)
                        .cloned()
                        .unwrap_or(serde_json::Value::Null);
                }
            }
            return match cursor {
                serde_json::Value::String(s) => s,
                other => other.to_string(),
            };
        }
        String::new()
    }
    fn walk(v: &serde_json::Value, runs: &[StepRun]) -> serde_json::Value {
        match v {
            serde_json::Value::String(s) => serde_json::Value::String(replace_str(s, runs)),
            serde_json::Value::Array(arr) => {
                serde_json::Value::Array(arr.iter().map(|x| walk(x, runs)).collect())
            }
            serde_json::Value::Object(map) => {
                let mut out = serde_json::Map::with_capacity(map.len());
                for (k, v) in map {
                    out.insert(k.clone(), walk(v, runs));
                }
                serde_json::Value::Object(out)
            }
            _ => v.clone(),
        }
    }
    walk(params, runs)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_workflow_path_strips_unsafe_chars() {
        let p = workflow_path("/tmp/x", "../../etc/passwd").unwrap();
        assert!(p.to_string_lossy().ends_with("etcpasswd.json"));
    }

    #[test]
    fn test_workflow_path_rejects_all_unsafe() {
        assert!(workflow_path("/tmp/x", "@@@@").is_err());
    }

    #[test]
    fn test_interpolate_text_and_json() {
        let runs = vec![StepRun {
            step_id: "s1".into(),
            label: "step_1".into(),
            kind: "cypher".into(),
            status: "ok".into(),
            duration_ms: 1,
            text: "5 row(s)".into(),
            json: Some(serde_json::json!([{"name": "foo"}, {"name": "bar"}])),
            error: None,
        }];
        let params = serde_json::json!({
            "query": "Refer to {{step_1.text}}; first name = {{step_1.json.[0].name}}",
        });
        let resolved = interpolate_params(&params, &runs);
        assert_eq!(
            resolved["query"].as_str().unwrap(),
            "Refer to 5 row(s); first name = foo"
        );
    }

    #[test]
    fn test_interpolate_unresolved_token_kept() {
        let runs: Vec<StepRun> = Vec::new();
        let params = serde_json::json!({"query": "missing: {{step_42.text}}"});
        let resolved = interpolate_params(&params, &runs);
        assert!(resolved["query"]
            .as_str()
            .unwrap()
            .contains("{{step_42.text}}"));
    }
}

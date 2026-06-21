//! Schema & API inventory commands (Theme D).
//!
//! Surfaces the nodes produced by the Theme D ingest phases:
//! - `list_endpoints` — ApiEndpoint nodes (REST/GraphQL handlers).
//! - `list_db_tables` — DbEntity nodes with column + FK counts.
//! - `list_env_vars` — EnvVar nodes with declared/referenced flags.
//! - `get_endpoint_handler` — resolves the handler Method for a route +
//!   returns its first-degree call neighborhood.

use serde::Serialize;
use tauri::State;

use code_explorer_core::graph::types::{NodeLabel, RelationshipType};

use crate::state::AppState;

// ─── Data shapes exposed to the frontend ────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiEndpointSummary {
    pub node_id: String,
    pub http_method: String,
    pub route: String,
    pub framework: Option<String>,
    pub file_path: String,
    pub start_line: Option<u32>,
    pub handler_id: Option<String>,
    pub handler_name: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DbTableSummary {
    pub node_id: String,
    pub name: String,
    pub file_path: String,
    pub column_count: u32,
    pub fk_count: u32,
    pub columns: Vec<DbColumnSummary>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DbColumnSummary {
    pub node_id: String,
    pub name: String,
    pub column_type: Option<String>,
    pub is_primary_key: bool,
    pub is_nullable: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EnvVarSummary {
    pub node_id: String,
    pub name: String,
    pub declared_in: Option<String>,
    pub used_in_count: u32,
    pub unused: bool,
    pub undeclared: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HandlerNeighbor {
    pub node_id: String,
    pub name: String,
    pub label: String,
    pub rel_type: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EndpointHandlerDetails {
    pub endpoint: ApiEndpointSummary,
    pub handler: Option<HandlerInfo>,
    pub callers: Vec<HandlerNeighbor>,
    pub callees: Vec<HandlerNeighbor>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HandlerInfo {
    pub node_id: String,
    pub name: String,
    pub label: String,
    pub file_path: String,
    pub start_line: Option<u32>,
    pub end_line: Option<u32>,
}

// ─── Commands ──────────────────────────────────────────────────────────────

#[tauri::command]
pub async fn list_endpoints(
    state: State<'_, AppState>,
    method: Option<String>,
    pattern: Option<String>,
) -> Result<Vec<ApiEndpointSummary>, String> {
    let (graph, _, _, _) = state.get_repo(None).await?;
    let want_method = method.map(|m| m.to_ascii_uppercase());
    let pattern_lower = pattern.map(|p| p.to_ascii_lowercase());

    let mut out: Vec<ApiEndpointSummary> = Vec::new();
    for node in graph.iter_nodes() {
        if node.label != NodeLabel::ApiEndpoint {
            continue;
        }
        let http = node.properties.http_method.clone().unwrap_or_default();
        let route = node
            .properties
            .route
            .clone()
            .or_else(|| node.properties.route_template.clone())
            .unwrap_or_default();

        if let Some(want) = &want_method {
            if !http.eq_ignore_ascii_case(want) {
                continue;
            }
        }
        if let Some(pat) = &pattern_lower {
            if !route.to_ascii_lowercase().contains(pat) {
                continue;
            }
        }

        let handler_id = node.properties.handler_id.clone();
        let handler_name = handler_id
            .as_ref()
            .and_then(|hid| graph.get_node(hid).map(|n| n.properties.name.clone()));

        out.push(ApiEndpointSummary {
            node_id: node.id.clone(),
            http_method: http,
            route,
            framework: node.properties.framework.clone(),
            file_path: node.properties.file_path.clone(),
            start_line: node.properties.start_line,
            handler_id,
            handler_name,
        });
    }
    out.sort_by(|a, b| {
        a.framework
            .clone()
            .unwrap_or_default()
            .cmp(&b.framework.clone().unwrap_or_default())
            .then(a.route.cmp(&b.route))
            .then(a.http_method.cmp(&b.http_method))
    });
    Ok(out)
}

#[tauri::command]
pub async fn list_db_tables(state: State<'_, AppState>) -> Result<Vec<DbTableSummary>, String> {
    let (graph, _, _, _) = state.get_repo(None).await?;

    // Pre-index HasColumn targets by source (DbEntity).
    let mut cols_by_entity: std::collections::HashMap<String, Vec<String>> =
        std::collections::HashMap::new();
    let mut fk_count_by_entity: std::collections::HashMap<String, u32> =
        std::collections::HashMap::new();
    for rel in graph.iter_relationships() {
        match rel.rel_type {
            RelationshipType::HasColumn => {
                cols_by_entity
                    .entry(rel.source_id.clone())
                    .or_default()
                    .push(rel.target_id.clone());
            }
            RelationshipType::ReferencesTable => {
                *fk_count_by_entity.entry(rel.source_id.clone()).or_insert(0) += 1;
            }
            _ => {}
        }
    }

    let mut out: Vec<DbTableSummary> = Vec::new();
    for node in graph.iter_nodes() {
        if node.label != NodeLabel::DbEntity {
            continue;
        }
        let col_ids = cols_by_entity.get(&node.id).cloned().unwrap_or_default();
        let mut columns: Vec<DbColumnSummary> = Vec::new();
        for cid in &col_ids {
            if let Some(col) = graph.get_node(cid) {
                columns.push(DbColumnSummary {
                    node_id: col.id.clone(),
                    name: col.properties.name.clone(),
                    column_type: col.properties.column_type.clone(),
                    is_primary_key: col.properties.is_primary_key.unwrap_or(false),
                    is_nullable: col.properties.is_nullable.unwrap_or(true),
                });
            }
        }
        columns.sort_by(|a, b| {
            b.is_primary_key
                .cmp(&a.is_primary_key)
                .then(a.name.cmp(&b.name))
        });
        out.push(DbTableSummary {
            node_id: node.id.clone(),
            name: node.properties.name.clone(),
            file_path: node.properties.file_path.clone(),
            column_count: col_ids.len() as u32,
            fk_count: fk_count_by_entity.get(&node.id).copied().unwrap_or(0),
            columns,
        });
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(out)
}

#[tauri::command]
pub async fn list_env_vars(
    state: State<'_, AppState>,
    unused_only: Option<bool>,
) -> Result<Vec<EnvVarSummary>, String> {
    let (graph, _, _, _) = state.get_repo(None).await?;
    let filter = unused_only.unwrap_or(false);

    let mut out: Vec<EnvVarSummary> = Vec::new();
    for node in graph.iter_nodes() {
        if node.label != NodeLabel::EnvVar {
            continue;
        }
        let unused = node.properties.unused.unwrap_or(false);
        let undeclared = node.properties.undeclared.unwrap_or(false);
        if filter && !unused {
            continue;
        }
        out.push(EnvVarSummary {
            node_id: node.id.clone(),
            name: node.properties.name.clone(),
            declared_in: node.properties.declared_in.clone(),
            used_in_count: node.properties.used_in_count.unwrap_or(0),
            unused,
            undeclared,
        });
    }
    // Unused first (most actionable), then undeclared, then by name.
    out.sort_by(|a, b| {
        b.unused
            .cmp(&a.unused)
            .then(b.undeclared.cmp(&a.undeclared))
            .then(a.name.cmp(&b.name))
    });
    Ok(out)
}

#[tauri::command]
pub async fn get_endpoint_handler(
    state: State<'_, AppState>,
    route: String,
    method: String,
) -> Result<EndpointHandlerDetails, String> {
    let (graph, _, _, _) = state.get_repo(None).await?;
    let wanted_method = method.to_ascii_uppercase();

    // Find the matching endpoint.
    let endpoint_node = graph
        .iter_nodes()
        .find(|n| {
            n.label == NodeLabel::ApiEndpoint
                && n.properties
                    .http_method
                    .as_deref()
                    .map(|m| m.eq_ignore_ascii_case(&wanted_method))
                    .unwrap_or(false)
                && (n.properties.route.as_deref() == Some(&route)
                    || n.properties.route_template.as_deref() == Some(&route))
        })
        .ok_or_else(|| format!("No endpoint found for {wanted_method} {route}"))?;

    let endpoint_summary = ApiEndpointSummary {
        node_id: endpoint_node.id.clone(),
        http_method: endpoint_node
            .properties
            .http_method
            .clone()
            .unwrap_or_default(),
        route: endpoint_node
            .properties
            .route
            .clone()
            .or_else(|| endpoint_node.properties.route_template.clone())
            .unwrap_or_default(),
        framework: endpoint_node.properties.framework.clone(),
        file_path: endpoint_node.properties.file_path.clone(),
        start_line: endpoint_node.properties.start_line,
        handler_id: endpoint_node.properties.handler_id.clone(),
        handler_name: None,
    };

    // Resolve handler by HandledBy edge.
    let handler_id = graph
        .iter_relationships()
        .find(|r| {
            matches!(r.rel_type, RelationshipType::HandledBy) && r.source_id == endpoint_node.id
        })
        .map(|r| r.target_id.clone())
        .or_else(|| endpoint_node.properties.handler_id.clone());

    let mut result = EndpointHandlerDetails {
        endpoint: endpoint_summary,
        handler: None,
        callers: Vec::new(),
        callees: Vec::new(),
    };

    let Some(handler_id) = handler_id else {
        return Ok(result);
    };

    let Some(handler) = graph.get_node(&handler_id) else {
        return Ok(result);
    };

    result.handler = Some(HandlerInfo {
        node_id: handler.id.clone(),
        name: handler.properties.name.clone(),
        label: handler.label.as_str().to_string(),
        file_path: handler.properties.file_path.clone(),
        start_line: handler.properties.start_line,
        end_line: handler.properties.end_line,
    });

    // Walk Calls edges: depth 1 incoming (callers) and outgoing (callees).
    for rel in graph.iter_relationships() {
        if !matches!(rel.rel_type, RelationshipType::Calls) {
            continue;
        }
        if rel.target_id == handler_id {
            if let Some(src) = graph.get_node(&rel.source_id) {
                result.callers.push(HandlerNeighbor {
                    node_id: src.id.clone(),
                    name: src.properties.name.clone(),
                    label: src.label.as_str().to_string(),
                    rel_type: rel.rel_type.as_str().to_string(),
                });
            }
        }
        if rel.source_id == handler_id {
            if let Some(tgt) = graph.get_node(&rel.target_id) {
                result.callees.push(HandlerNeighbor {
                    node_id: tgt.id.clone(),
                    name: tgt.properties.name.clone(),
                    label: tgt.label.as_str().to_string(),
                    rel_type: rel.rel_type.as_str().to_string(),
                });
            }
        }
    }

    result.callers.sort_by(|a, b| a.name.cmp(&b.name));
    result.callees.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(result)
}

use std::collections::HashMap;

use serde::Serialize;
use tauri::State;

use code_explorer_core::graph::types::{NodeLabel, RelationshipType};

use crate::state::AppState;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CoverageStats {
    pub total_methods: usize,
    pub traced_methods: usize,
    pub dead_code_candidates: usize,
    pub coverage_pct: f64,
    pub dead_methods: Vec<DeadMethod>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeadMethod {
    pub name: String,
    pub file_path: String,
    pub class_name: Option<String>,
    pub node_id: String,
}

#[tauri::command]
pub async fn get_coverage_stats(state: State<'_, AppState>) -> Result<CoverageStats, String> {
    let (graph, _, _, _) = state.get_repo(None).await?;

    // Build a method → owning-class index so we can show class names alongside
    // dead methods. We no longer need an incoming-calls index because dead-code
    // detection is delegated to the `is_dead_candidate` flag set by the pipeline.
    let mut method_class: HashMap<String, String> = HashMap::new();
    for rel in graph.iter_relationships() {
        if matches!(rel.rel_type, RelationshipType::HasMethod) {
            method_class.insert(rel.target_id.clone(), rel.source_id.clone());
        }
    }

    let all_methods: Vec<_> = graph
        .iter_nodes()
        .filter(|n| n.label == NodeLabel::Method || n.label == NodeLabel::Function)
        .collect();

    let total = all_methods.len();
    let traced = all_methods
        .iter()
        .filter(|n| n.properties.is_traced == Some(true))
        .count();

    // Trust the pipeline's `is_dead_candidate` flag, which is computed in
    // dead_code.rs after applying all the proper exclusions (test files,
    // interface methods, controller actions, view scripts, entry points,
    // constructors). Re-deriving from incoming_calls here would mark all
    // those legitimate methods as dead.
    let mut dead_methods = Vec::new();
    for method in &all_methods {
        if method.properties.is_dead_candidate == Some(true) {
            let class_name = method_class
                .get(&method.id)
                .and_then(|cid| graph.get_node(cid))
                .map(|n| n.properties.name.clone());
            dead_methods.push(DeadMethod {
                name: method.properties.name.clone(),
                file_path: method.properties.file_path.clone(),
                class_name,
                node_id: method.id.clone(),
            });
        }
    }

    // Sort dead methods by file path for consistent output
    dead_methods.sort_by(|a, b| a.file_path.cmp(&b.file_path));
    // Capture the true total before truncating the displayed list, otherwise
    // dead_code_candidates would silently report only the cap.
    let dead_code_total = dead_methods.len();
    // Limit to top 50 for the visible list
    dead_methods.truncate(50);

    Ok(CoverageStats {
        total_methods: total,
        traced_methods: traced,
        dead_code_candidates: dead_code_total,
        coverage_pct: if total > 0 {
            (traced as f64 / total as f64 * 100.0).round()
        } else {
            0.0
        },
        dead_methods,
    })
}

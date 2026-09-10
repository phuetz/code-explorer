use serde::Serialize;
use tauri::State;

use crate::state::AppState;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CodeHealth {
    pub overall_score: f64,
    pub grade: String,
    pub hotspot_score: f64,
    pub cohesion_score: f64,
    pub tracing_coverage: f64,
    pub ownership_score: f64,
    pub file_count: u32,
    pub node_count: u32,
    pub edge_count: u32,
    pub avg_complexity: f64,
    pub max_complexity: u32,
}

#[tauri::command]
pub async fn get_code_health(state: State<'_, AppState>) -> Result<CodeHealth, String> {
    let (graph, _, _, repo_path_str) = state.get_repo(None).await?;
    let repo_path = std::path::PathBuf::from(repo_path_str);

    let node_count = graph.iter_nodes().count() as u32;
    let edge_count = graph.iter_relationships().count() as u32;
    let file_count = graph
        .iter_nodes()
        .filter(|n| n.label == code_explorer_core::graph::types::NodeLabel::File)
        .count() as u32;

    // Cohesion: average cohesion of Community nodes
    let community_nodes: Vec<_> = graph
        .iter_nodes()
        .filter(|n| n.label == code_explorer_core::graph::types::NodeLabel::Community)
        .collect();
    let cohesion_score = if community_nodes.is_empty() {
        0.5
    } else {
        let sum: f64 = community_nodes
            .iter()
            .filter_map(|n| n.properties.cohesion)
            .sum();
        let count = community_nodes
            .iter()
            .filter(|n| n.properties.cohesion.is_some())
            .count();
        if count > 0 {
            sum / count as f64
        } else {
            0.5
        }
    };

    // Tracing coverage: ratio of traced methods to total methods.
    // Restrict the numerator to the same label set as the denominator —
    // `is_traced` is also set on File nodes by `extract_tracing_info`, so
    // an unfiltered count would mix methods + files and the ratio could
    // exceed 1.0 (which then displays as >100% coverage).
    let is_method_like = |label: &code_explorer_core::graph::types::NodeLabel| {
        matches!(
            label,
            code_explorer_core::graph::types::NodeLabel::Method
                | code_explorer_core::graph::types::NodeLabel::Function
                | code_explorer_core::graph::types::NodeLabel::ControllerAction
        )
    };
    let total_methods = graph
        .iter_nodes()
        .filter(|n| is_method_like(&n.label))
        .count() as f64;
    let traced_methods = graph
        .iter_nodes()
        .filter(|n| is_method_like(&n.label) && n.properties.is_traced == Some(true))
        .count() as f64;
    let tracing_coverage = if total_methods > 0.0 {
        traced_methods / total_methods
    } else {
        0.0
    };

    // Git-based scores (run in blocking task)
    let rp = repo_path.clone();
    let git_result = tokio::task::spawn_blocking(move || {
        let hotspots = code_explorer_git::hotspots::analyze_hotspots(&rp, 90).unwrap_or_default();
        let ownerships = code_explorer_git::ownership::analyze_ownership(&rp).unwrap_or_default();
        (hotspots, ownerships)
    })
    .await
    .map_err(|e| format!("Task error: {}", e))?;

    let (hotspots, ownerships) = git_result;

    // Hotspot score: lower is better (inverted — fewer high-score files = healthy)
    let hotspot_score = if hotspots.is_empty() {
        0.5
    } else {
        let high_risk = hotspots.iter().filter(|h| h.score > 0.7).count() as f64;
        let ratio = high_risk / hotspots.len() as f64;
        1.0 - ratio.min(1.0)
    };

    // Ownership score: higher ownership = healthier
    let ownership_score = if ownerships.is_empty() {
        0.5
    } else {
        let avg_pct: f64 =
            ownerships.iter().map(|o| o.ownership_pct).sum::<f64>() / ownerships.len() as f64;
        (avg_pct / 100.0).min(1.0)
    };

    // Cyclomatic complexity metrics
    let complexity_values: Vec<u32> = graph
        .iter_nodes()
        .filter(|n| {
            matches!(
                n.label,
                code_explorer_core::graph::types::NodeLabel::Method
                    | code_explorer_core::graph::types::NodeLabel::Function
                    | code_explorer_core::graph::types::NodeLabel::Constructor
            )
        })
        .filter_map(|n| n.properties.complexity)
        .collect();
    let max_complexity = complexity_values.iter().copied().max().unwrap_or(0);
    let avg_complexity = if complexity_values.is_empty() {
        0.0
    } else {
        complexity_values.iter().map(|&v| v as f64).sum::<f64>() / complexity_values.len() as f64
    };

    // Overall = weighted average (all 0.0-1.0)
    let overall = hotspot_score * 0.30
        + cohesion_score * 0.25
        + tracing_coverage * 0.20
        + ownership_score * 0.25;

    let overall_100 = (overall * 100.0).clamp(0.0, 100.0);

    // Derive the grade from the same rounded value that is surfaced to the
    // UI so the displayed score and letter grade can't disagree. Previously
    // the grade came from `overall_100 as u32` (truncation) while the
    // displayed score rounded to one decimal, so an `overall_100 = 89.95`
    // reported `"90.0 / B"` — the score crossed the A boundary after
    // rounding but the grade did not.
    let overall_score = (overall_100 * 10.0).round() / 10.0;
    let grade = match overall_score as u32 {
        90..=100 => "A",
        75..=89 => "B",
        60..=74 => "C",
        40..=59 => "D",
        _ => "E",
    };

    Ok(CodeHealth {
        overall_score,
        grade: grade.to_string(),
        hotspot_score: (hotspot_score * 100.0).round() / 100.0,
        cohesion_score: (cohesion_score * 100.0).round() / 100.0,
        tracing_coverage: (tracing_coverage * 100.0).round() / 100.0,
        ownership_score: (ownership_score * 100.0).round() / 100.0,
        file_count,
        node_count,
        edge_count,
        avg_complexity: (avg_complexity * 10.0).round() / 10.0,
        max_complexity,
    })
}

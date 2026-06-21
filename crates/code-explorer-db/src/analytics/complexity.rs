//! Cyclomatic complexity reporting.
//!
//! Complexity is computed at parse time and stored in
//! `NodeProperties.complexity` (see `code-explorer-ingest/src/phases/parsing.rs`).
//! This module slices & dices that data for the UI & MCP tool.

use code_explorer_core::graph::types::NodeLabel;
use code_explorer_core::graph::KnowledgeGraph;
use serde::{Deserialize, Serialize};

/// A single complex function/method.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ComplexSymbol {
    pub node_id: String,
    pub name: String,
    pub file_path: String,
    pub label: String,
    pub complexity: u32,
    pub start_line: Option<u32>,
    pub end_line: Option<u32>,
    /// Coarse severity: "low" (1–5), "medium" (6–10), "high" (11–20), "critical" (>20).
    pub severity: String,
}

/// Complexity summary for the whole repo.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ComplexityReport {
    pub total_symbols: usize,
    pub measured_symbols: usize,
    pub avg_complexity: f64,
    pub max_complexity: u32,
    pub p50: u32,
    pub p90: u32,
    pub p99: u32,
    /// Top-N most complex symbols, sorted descending by complexity.
    pub top_symbols: Vec<ComplexSymbol>,
    /// Count of symbols in each severity bucket.
    pub severity_counts: SeverityCounts,
    /// Per-module averages (key: module/community name or file prefix).
    pub by_module: Vec<ModuleComplexity>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SeverityCounts {
    pub low: usize,
    pub medium: usize,
    pub high: usize,
    pub critical: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModuleComplexity {
    pub module: String,
    pub symbol_count: usize,
    pub avg_complexity: f64,
    pub max_complexity: u32,
}

pub fn severity_for(cc: u32) -> &'static str {
    match cc {
        0..=5 => "low",
        6..=10 => "medium",
        11..=20 => "high",
        _ => "critical",
    }
}

/// Options for `get_complexity`.
#[derive(Debug, Clone, Copy)]
pub struct ComplexityOptions {
    /// Only include symbols with complexity >= this value. Default 0 = all.
    pub threshold: u32,
    /// Top-N list cap. Default 50.
    pub top_n: usize,
}

impl Default for ComplexityOptions {
    fn default() -> Self {
        Self {
            threshold: 0,
            top_n: 50,
        }
    }
}

/// Compute the repo's complexity report.
pub fn get_complexity(graph: &KnowledgeGraph, opts: ComplexityOptions) -> ComplexityReport {
    // 1. Collect every Method/Function/Constructor with a measured CC.
    let all: Vec<&code_explorer_core::graph::types::GraphNode> = graph
        .iter_nodes()
        .filter(|n| {
            matches!(
                n.label,
                NodeLabel::Method | NodeLabel::Function | NodeLabel::Constructor
            )
        })
        .collect();

    let total_symbols = all.len();

    let mut measured: Vec<(u32, &code_explorer_core::graph::types::GraphNode)> = all
        .iter()
        .filter_map(|n| n.properties.complexity.map(|cc| (cc, *n)))
        .collect();

    let measured_symbols = measured.len();

    if measured_symbols == 0 {
        return ComplexityReport {
            total_symbols,
            measured_symbols: 0,
            avg_complexity: 0.0,
            max_complexity: 0,
            p50: 0,
            p90: 0,
            p99: 0,
            top_symbols: Vec::new(),
            severity_counts: SeverityCounts::default(),
            by_module: Vec::new(),
        };
    }

    // 2. Aggregate stats.
    let sum: u64 = measured.iter().map(|(cc, _)| *cc as u64).sum();
    let avg = sum as f64 / measured_symbols as f64;

    let mut ccs: Vec<u32> = measured.iter().map(|(cc, _)| *cc).collect();
    ccs.sort_unstable();
    let max_cc = *ccs.last().unwrap_or(&0);
    let p50 = percentile(&ccs, 0.50);
    let p90 = percentile(&ccs, 0.90);
    let p99 = percentile(&ccs, 0.99);

    // 3. Severity bucket counts.
    let mut sev = SeverityCounts::default();
    for (cc, _) in &measured {
        match severity_for(*cc) {
            "low" => sev.low += 1,
            "medium" => sev.medium += 1,
            "high" => sev.high += 1,
            "critical" => sev.critical += 1,
            _ => {}
        }
    }

    // 4. Top-N most complex, above threshold.
    measured.sort_by(|a, b| b.0.cmp(&a.0));
    let top_symbols: Vec<ComplexSymbol> = measured
        .iter()
        .filter(|(cc, _)| *cc >= opts.threshold)
        .take(opts.top_n)
        .map(|(cc, n)| ComplexSymbol {
            node_id: n.id.clone(),
            name: n.properties.name.clone(),
            file_path: n.properties.file_path.clone(),
            label: n.label.as_str().to_string(),
            complexity: *cc,
            start_line: n.properties.start_line,
            end_line: n.properties.end_line,
            severity: severity_for(*cc).to_string(),
        })
        .collect();

    // 5. Per-module aggregation — use the first path segment (coarse but
    //    free and correlates well with logical modules).
    let mut by_mod: std::collections::HashMap<String, (u64, u32, usize)> =
        std::collections::HashMap::new();
    for (cc, n) in &measured {
        let module = module_from_path(&n.properties.file_path);
        let entry = by_mod.entry(module).or_insert((0, 0, 0));
        entry.0 += *cc as u64;
        entry.1 = entry.1.max(*cc);
        entry.2 += 1;
    }
    let mut by_module: Vec<ModuleComplexity> = by_mod
        .into_iter()
        .map(|(module, (sum, max, count))| ModuleComplexity {
            module,
            symbol_count: count,
            avg_complexity: sum as f64 / count as f64,
            max_complexity: max,
        })
        .collect();
    by_module.sort_by(|a, b| {
        b.avg_complexity
            .partial_cmp(&a.avg_complexity)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    ComplexityReport {
        total_symbols,
        measured_symbols,
        avg_complexity: avg,
        max_complexity: max_cc,
        p50,
        p90,
        p99,
        top_symbols,
        severity_counts: sev,
        by_module,
    }
}

fn percentile(sorted: &[u32], q: f64) -> u32 {
    if sorted.is_empty() {
        return 0;
    }
    let idx = ((sorted.len() as f64 - 1.0) * q).round() as usize;
    sorted[idx.min(sorted.len() - 1)]
}

fn module_from_path(path: &str) -> String {
    // Take the first 1–2 path segments as the module identifier.
    let normalized = path.replace('\\', "/");
    let mut parts = normalized.split('/').filter(|p| !p.is_empty());
    let first = parts.next().unwrap_or("(root)");
    match parts.next() {
        Some(second) if !second.contains('.') => format!("{first}/{second}"),
        _ => first.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use code_explorer_core::graph::types::{GraphNode, NodeProperties};

    fn make_fn(name: &str, path: &str, cc: Option<u32>) -> GraphNode {
        GraphNode {
            id: format!("Function:{path}:{name}"),
            label: NodeLabel::Function,
            properties: NodeProperties {
                name: name.to_string(),
                file_path: path.to_string(),
                complexity: cc,
                ..Default::default()
            },
        }
    }

    #[test]
    fn test_empty_graph() {
        let g = KnowledgeGraph::new();
        let r = get_complexity(&g, ComplexityOptions::default());
        assert_eq!(r.measured_symbols, 0);
        assert_eq!(r.max_complexity, 0);
        assert!(r.top_symbols.is_empty());
    }

    #[test]
    fn test_basic_report() {
        let mut g = KnowledgeGraph::new();
        g.add_node(make_fn("a", "src/a.rs", Some(3)));
        g.add_node(make_fn("b", "src/b.rs", Some(7)));
        g.add_node(make_fn("c", "src/c.rs", Some(25)));
        g.add_node(make_fn("d", "src/d.rs", None)); // unmeasured

        let r = get_complexity(&g, ComplexityOptions::default());
        assert_eq!(r.total_symbols, 4);
        assert_eq!(r.measured_symbols, 3);
        assert_eq!(r.max_complexity, 25);
        assert_eq!(r.severity_counts.low, 1);
        assert_eq!(r.severity_counts.medium, 1);
        assert_eq!(r.severity_counts.critical, 1);
        assert_eq!(r.top_symbols.len(), 3);
        assert_eq!(r.top_symbols[0].name, "c");
    }

    #[test]
    fn test_severity_buckets() {
        assert_eq!(severity_for(1), "low");
        assert_eq!(severity_for(5), "low");
        assert_eq!(severity_for(6), "medium");
        assert_eq!(severity_for(10), "medium");
        assert_eq!(severity_for(11), "high");
        assert_eq!(severity_for(20), "high");
        assert_eq!(severity_for(21), "critical");
    }

    #[test]
    fn test_threshold_filter() {
        let mut g = KnowledgeGraph::new();
        g.add_node(make_fn("a", "src/a.rs", Some(2)));
        g.add_node(make_fn("b", "src/b.rs", Some(15)));
        let r = get_complexity(
            &g,
            ComplexityOptions {
                threshold: 10,
                top_n: 50,
            },
        );
        assert_eq!(r.top_symbols.len(), 1);
        assert_eq!(r.top_symbols[0].name, "b");
    }
}

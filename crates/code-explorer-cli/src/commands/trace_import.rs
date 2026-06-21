//! The `trace-import` command: import execution traces to enrich the knowledge graph.

use anyhow::Result;
use colored::Colorize;
use std::collections::HashMap;

use code_explorer_core::trace;
use code_explorer_db::snapshot;

pub fn run(log_file: &str, path: Option<&str>) -> Result<()> {
    let repo_path = if let Some(p) = path {
        std::path::PathBuf::from(p)
    } else {
        std::env::current_dir()?
    };

    let snap_path = repo_path.join(".codeexplorer").join("graph.bin");
    if !snap_path.exists() {
        println!(
            "{} No index found. Run 'code-explorer analyze' first.",
            "ERROR".red()
        );
        return Ok(());
    }

    let log_path = std::path::Path::new(log_file);
    if !log_path.exists() {
        println!("{} Log file not found: {}", "ERROR".red(), log_file);
        return Ok(());
    }

    println!("{} Loading graph...", "->".cyan());
    let mut graph = snapshot::load_snapshot(&snap_path)
        .map_err(|e| anyhow::anyhow!("Failed to load graph: {}", e))?;

    println!("{} Parsing log file: {}", "->".cyan(), log_file);
    let log_content = std::fs::read_to_string(log_path)?;

    // Extract ClassName.MethodName patterns from log lines
    let mut trace_counts: HashMap<String, u32> = HashMap::new();
    let method_pattern = regex::Regex::new(
        r"(?:^|[\s|:])([A-Z]\w+(?:Service|Controller|Repository|Manager|Helper|Provider|Factory|Client|Handler|Processor))\.(\w+)",
    )?;

    // Also match simpler patterns: ClassName.MethodName with timing
    let simple_pattern = regex::Regex::new(r"([A-Z]\w{2,})\.([A-Z]\w+)\s*(?:\(|[\s|]|\d+ms)")?;

    for line in log_content.lines() {
        // Try structured pattern first
        let mut matched_line = false;
        for cap in method_pattern.captures_iter(line) {
            let class_name = cap.get(1).unwrap().as_str();
            let method_name = cap.get(2).unwrap().as_str();
            let key = format!("{}.{}", class_name, method_name);
            *trace_counts.entry(key).or_insert(0) += 1;
            matched_line = true;
        }

        // Try simple pattern only if structured pattern didn't match
        if !matched_line {
            for cap in simple_pattern.captures_iter(line) {
                let class_name = cap.get(1).unwrap().as_str();
                let method_name = cap.get(2).unwrap().as_str();
                let key = format!("{}.{}", class_name, method_name);
                *trace_counts.entry(key).or_insert(0) += 1;
            }
        }
    }

    if trace_counts.is_empty() {
        println!("{} No method calls found in log file.", "WARN".yellow());
        println!("  Expected patterns:");
        println!("    ClassName.MethodName");
        println!("    [INFO] 2024-01-15 | CourrierService.GenererCourrier | 234ms");
        println!("    timestamp,class.method,duration");
        return Ok(());
    }

    println!("  Found {} unique method calls in logs", trace_counts.len());

    // Match against graph nodes
    let mut matched = 0u32;
    let mut unmatched = Vec::new();
    let mut updated_nodes = 0u32;

    let name_to_ids = trace::build_name_index(&graph);

    for (key, count) in &trace_counts {
        let found = trace::resolve_method_node(&graph, &name_to_ids, key);

        if let Some(node_id) = found {
            matched += 1;
            // Update the node
            if let Some(node) = graph.get_node_mut(&node_id) {
                node.properties.is_traced = Some(true);
                let current = node.properties.trace_call_count.unwrap_or(0);
                // saturating_add to avoid panic on extremely high-volume
                // traces (millions of invocations across multiple imports).
                node.properties.trace_call_count = Some(current.saturating_add(*count));
                updated_nodes += 1;
            }
        } else {
            unmatched.push(key.clone());
        }
    }

    // Save updated graph
    if updated_nodes > 0 {
        println!("{} Saving updated graph...", "->".cyan());
        snapshot::save_snapshot(&graph, &snap_path)
            .map_err(|e| anyhow::anyhow!("Failed to save graph: {}", e))?;
    }

    // Summary
    println!();
    println!("{} Trace import complete", "OK".green());
    println!("  Log entries parsed:  {}", log_content.lines().count());
    println!("  Unique methods:      {}", trace_counts.len());
    println!(
        "  Matched in graph:    {} ({:.0}%)",
        matched,
        if trace_counts.is_empty() {
            0.0
        } else {
            matched as f64 / trace_counts.len() as f64 * 100.0
        }
    );
    println!("  Nodes updated:       {}", updated_nodes);
    println!("  Unmatched:           {}", unmatched.len());

    if !unmatched.is_empty() && unmatched.len() <= 20 {
        println!();
        println!("  {} Unmatched methods:", "WARN".yellow());
        for u in &unmatched {
            println!("    - {}", u);
        }
    }

    Ok(())
}

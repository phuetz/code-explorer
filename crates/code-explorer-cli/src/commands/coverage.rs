//! The `coverage` command: tracing coverage analysis and dead code detection.

use anyhow::Result;
use colored::Colorize;
use std::collections::HashMap;

use code_explorer_core::graph::types::{NodeLabel, RelationshipType};
use code_explorer_db::snapshot;

pub fn run(target: Option<&str>, path: Option<&str>, json: bool, trace: bool) -> Result<()> {
    let repo_path = if let Some(p) = path {
        std::path::PathBuf::from(p)
    } else {
        std::env::current_dir()?
    };

    let storage = code_explorer_core::storage::repo_manager::get_storage_paths(&repo_path);
    let snap_path =
        code_explorer_db::snapshot::snapshot_path(std::path::Path::new(&storage.storage_path));
    if !snap_path.exists() {
        println!(
            "{} No index found. Run 'code-explorer analyze' first.",
            "ERROR".red()
        );
        return Ok(());
    }

    let graph = snapshot::load_snapshot(&snap_path)
        .map_err(|e| anyhow::anyhow!("Failed to load graph: {}", e))?;

    // Build incoming Calls index: method_id -> set of caller IDs
    let mut incoming_calls: HashMap<String, Vec<String>> = HashMap::new();
    // Build HasMethod index: class_id -> vec of method_ids
    let mut class_methods: HashMap<String, Vec<String>> = HashMap::new();
    // Build method->class reverse index
    let mut method_class: HashMap<String, String> = HashMap::new();

    for rel in graph.iter_relationships() {
        match rel.rel_type {
            RelationshipType::Calls | RelationshipType::CallsAction => {
                incoming_calls
                    .entry(rel.target_id.clone())
                    .or_default()
                    .push(rel.source_id.clone());
            }
            RelationshipType::HasMethod => {
                class_methods
                    .entry(rel.source_id.clone())
                    .or_default()
                    .push(rel.target_id.clone());
                method_class.insert(rel.target_id.clone(), rel.source_id.clone());
            }
            _ => {}
        }
    }

    if let Some(target_name) = target {
        if trace {
            // Flow trace mode: follow call chain and show coverage
            run_flow_trace(&graph, target_name, &class_methods, &method_class)
        } else {
            // Single class mode
            run_single_class(
                &graph,
                target_name,
                &incoming_calls,
                &class_methods,
                &method_class,
                json,
            )
        }
    } else {
        // Global mode
        run_global(&graph, &incoming_calls, &class_methods, &method_class, json)
    }
}

fn run_single_class(
    graph: &code_explorer_core::graph::KnowledgeGraph,
    target_name: &str,
    incoming_calls: &HashMap<String, Vec<String>>,
    class_methods: &HashMap<String, Vec<String>>,
    method_class: &HashMap<String, String>,
    json: bool,
) -> Result<()> {
    let target_lower = target_name.to_lowercase();

    // Find the Class/Service node
    let mut candidates: Vec<_> = graph
        .iter_nodes()
        .filter(|n| {
            n.properties.name.to_lowercase() == target_lower
                && matches!(
                    n.label,
                    NodeLabel::Class | NodeLabel::Service | NodeLabel::Controller
                )
        })
        .collect();
    candidates.sort_by_key(|n| match n.label {
        NodeLabel::Controller => 0,
        NodeLabel::Class => 1,
        NodeLabel::Service => 2,
        _ => 10,
    });

    let class_node = match candidates.first() {
        Some(n) => *n,
        None => {
            println!("{} Class '{}' not found.", "ERROR".red(), target_name);
            return Ok(());
        }
    };

    let method_ids = match class_methods.get(&class_node.id) {
        Some(ids) => ids,
        None => {
            println!(
                "{} No methods found for '{}'.",
                "WARN".yellow(),
                target_name
            );
            return Ok(());
        }
    };

    let mut methods: Vec<MethodInfo> = Vec::new();

    for method_id in method_ids {
        if let Some(method_node) = graph.get_node(method_id) {
            // Skip constructors named same as class for cleaner output
            let is_traced = method_node.properties.is_traced.unwrap_or(false);

            // Count external callers (exclude same-class methods)
            let external_callers: usize = incoming_calls
                .get(method_id)
                .map(|callers| {
                    callers
                        .iter()
                        .filter(|caller_id| {
                            // External = caller's parent class differs from this class
                            method_class
                                .get(*caller_id)
                                .map(|cls| cls != &class_node.id)
                                .unwrap_or(true) // If no parent class, consider external
                        })
                        .count()
                })
                .unwrap_or(0);

            let is_entry_point = matches!(method_node.label, NodeLabel::ControllerAction);

            methods.push(MethodInfo {
                name: method_node.properties.name.clone(),
                label: method_node.label.as_str().to_string(),
                is_traced,
                external_callers,
                is_entry_point,
                line: method_node.properties.start_line,
            });
        }
    }

    methods.sort_by(|a, b| a.name.cmp(&b.name));

    if json {
        print_json_single(class_node, &methods);
    } else {
        print_text_single(class_node, &methods);
    }

    Ok(())
}

fn run_global(
    graph: &code_explorer_core::graph::KnowledgeGraph,
    _incoming_calls: &HashMap<String, Vec<String>>,
    class_methods: &HashMap<String, Vec<String>>,
    _method_class: &HashMap<String, String>,
    json: bool,
) -> Result<()> {
    // Collect all classes with their coverage stats
    let mut class_stats: Vec<ClassStats> = Vec::new();
    let mut total_methods = 0usize;
    let mut total_traced = 0usize;
    let mut total_dead = 0usize;

    for (class_id, method_ids) in class_methods {
        let class_node = match graph.get_node(class_id) {
            Some(n) => n,
            None => continue,
        };

        // Only show Classes/Services (not interfaces, enums, etc.)
        if !matches!(
            class_node.label,
            NodeLabel::Class | NodeLabel::Service | NodeLabel::Controller
        ) {
            continue;
        }

        let mut traced = 0usize;
        let mut dead = 0usize;
        // Count only live nodes so per-class denominators match the global tally.
        // Stale IDs in `method_ids` (deleted nodes after indexing) are skipped.
        let mut method_count = 0usize;

        for method_id in method_ids {
            if let Some(method_node) = graph.get_node(method_id) {
                method_count += 1;
                total_methods += 1;
                if method_node.properties.is_traced.unwrap_or(false) {
                    traced += 1;
                    total_traced += 1;
                }
                // Use the pre-computed is_dead_candidate flag from the pipeline
                if method_node.properties.is_dead_candidate.unwrap_or(false) {
                    dead += 1;
                    total_dead += 1;
                }
            }
        }

        if method_count > 0 {
            class_stats.push(ClassStats {
                name: class_node.properties.name.clone(),
                file_path: class_node.properties.file_path.clone(),
                method_count,
                traced_count: traced,
                dead_count: dead,
            });
        }
    }

    class_stats.sort_by(|a, b| {
        let pct_a = if a.method_count > 0 {
            a.traced_count as f64 / a.method_count as f64
        } else {
            0.0
        };
        let pct_b = if b.method_count > 0 {
            b.traced_count as f64 / b.method_count as f64
        } else {
            0.0
        };
        pct_a.total_cmp(&pct_b)
    });

    if json {
        let out = serde_json::json!({
            "totalMethods": total_methods,
            "totalTraced": total_traced,
            "totalDead": total_dead,
            "coveragePct": if total_methods > 0 { (total_traced as f64 / total_methods as f64) * 100.0 } else { 0.0 },
            "classes": class_stats.iter().map(|c| serde_json::json!({
                "name": c.name,
                "filePath": c.file_path,
                "methods": c.method_count,
                "traced": c.traced_count,
                "dead": c.dead_count,
                "coveragePct": if c.method_count > 0 { (c.traced_count as f64 / c.method_count as f64) * 100.0 } else { 0.0 },
            })).collect::<Vec<_>>(),
        });
        println!("{}", serde_json::to_string_pretty(&out)?);
    } else {
        println!();
        println!("{}", "Tracing Coverage & Dead Code Report".cyan().bold());
        println!("{}", "=".repeat(60).cyan());
        println!();

        let global_pct = if total_methods > 0 {
            (total_traced as f64 / total_methods as f64) * 100.0
        } else {
            0.0
        };
        println!(
            "  Global: {}/{} methods traced ({:.1}%), {} dead code candidates",
            total_traced, total_methods, global_pct, total_dead
        );
        println!();

        // Show classes with worst coverage first (those with methods but 0% traced)
        let worst: Vec<_> = class_stats
            .iter()
            .filter(|c| c.traced_count == 0 && c.method_count > 3)
            .collect();
        if !worst.is_empty() {
            println!(
                "  {} Classes with 0% trace coverage (>3 methods):",
                "!!".red()
            );
            for c in worst.iter().take(15) {
                println!(
                    "    {} ({} methods, {} dead)",
                    c.name, c.method_count, c.dead_count
                );
            }
            println!();
        }

        // Show classes with partial coverage
        let partial: Vec<_> = class_stats
            .iter()
            .filter(|c| c.traced_count > 0 && c.traced_count < c.method_count)
            .collect();
        if !partial.is_empty() {
            println!("  {} Classes with partial trace coverage:", "~".yellow());
            for c in partial.iter().take(15) {
                let pct = (c.traced_count as f64 / c.method_count as f64) * 100.0;
                println!(
                    "    {}: {}/{} ({:.0}%) traced, {} dead",
                    c.name, c.traced_count, c.method_count, pct, c.dead_count
                );
            }
            println!();
        }

        // Show top dead code
        let mut dead_classes: Vec<_> = class_stats.iter().filter(|c| c.dead_count > 0).collect();
        dead_classes.sort_by(|a, b| b.dead_count.cmp(&a.dead_count));
        if !dead_classes.is_empty() {
            println!("  {} Top dead code candidates:", "?".purple());
            for c in dead_classes.iter().take(10) {
                println!(
                    "    {}: {} dead methods / {} total",
                    c.name, c.dead_count, c.method_count
                );
            }
        }

        println!();
    }

    Ok(())
}

struct MethodInfo {
    name: String,
    label: String,
    is_traced: bool,
    external_callers: usize,
    is_entry_point: bool,
    line: Option<u32>,
}

struct ClassStats {
    name: String,
    file_path: String,
    method_count: usize,
    traced_count: usize,
    dead_count: usize,
}

fn print_text_single(class_node: &code_explorer_core::graph::types::GraphNode, methods: &[MethodInfo]) {
    let traced_count = methods.iter().filter(|m| m.is_traced).count();
    let dead_count = methods
        .iter()
        .filter(|m| m.external_callers == 0 && !m.is_entry_point)
        .count();
    let total = methods.len();
    let pct = if total > 0 {
        (traced_count as f64 / total as f64) * 100.0
    } else {
        0.0
    };

    println!();
    println!(
        "{} {} ({})",
        "Tracing Coverage for".cyan(),
        class_node.properties.name.bold(),
        class_node.label.as_str()
    );
    println!("{}", "=".repeat(60).cyan());

    // Traced methods
    for m in methods {
        let marker = if m.is_traced {
            "V".green().to_string()
        } else {
            "X".red().to_string()
        };
        let callers = if m.external_callers == 0 && !m.is_entry_point {
            " (0 callers)".dimmed().to_string()
        } else {
            String::new()
        };
        let traced_label = if m.is_traced { " (traced)" } else { "" };
        let line_info = m.line.map(|l| format!(":{}", l)).unwrap_or_default();
        println!(
            "  {} {}{}{}{} ",
            marker,
            m.name,
            line_info.dimmed(),
            traced_label.green(),
            callers
        );
    }

    println!();
    println!(
        "  Coverage: {}/{} methods ({:.1}%)",
        traced_count, total, pct
    );

    if dead_count > 0 {
        println!(
            "  Dead code candidates: {} methods with 0 external callers",
            dead_count
        );
    }
    println!();
}

fn print_json_single(class_node: &code_explorer_core::graph::types::GraphNode, methods: &[MethodInfo]) {
    let traced_count = methods.iter().filter(|m| m.is_traced).count();
    let total = methods.len();

    let out = serde_json::json!({
        "class": class_node.properties.name,
        "label": class_node.label.as_str(),
        "filePath": class_node.properties.file_path,
        "totalMethods": total,
        "tracedMethods": traced_count,
        "coveragePct": if total > 0 { (traced_count as f64 / total as f64) * 100.0 } else { 0.0 },
        "methods": methods.iter().map(|m| serde_json::json!({
            "name": m.name,
            "label": m.label,
            "isTraced": m.is_traced,
            "externalCallers": m.external_callers,
            "isEntryPoint": m.is_entry_point,
            "line": m.line,
        })).collect::<Vec<_>>(),
    });

    println!("{}", serde_json::to_string_pretty(&out).unwrap_or_default());
}

/// Trace the full call flow from a class and show tracing coverage along the chain.
fn run_flow_trace(
    graph: &code_explorer_core::graph::KnowledgeGraph,
    target_name: &str,
    _class_methods: &HashMap<String, Vec<String>>,
    method_class: &HashMap<String, String>,
) -> Result<()> {
    use std::collections::{BTreeMap, HashSet, VecDeque};

    let target_lower = target_name.to_lowercase();

    // Find the Class/Service/Controller node
    let mut candidates: Vec<_> = graph
        .iter_nodes()
        .filter(|n| {
            n.properties.name.to_lowercase() == target_lower
                && matches!(
                    n.label,
                    NodeLabel::Class | NodeLabel::Service | NodeLabel::Controller
                )
        })
        .collect();
    candidates.sort_by_key(|n| match n.label {
        NodeLabel::Controller => 0,
        NodeLabel::Class => 1,
        NodeLabel::Service => 2,
        _ => 10,
    });

    let start_node = match candidates.first() {
        Some(n) => *n,
        None => {
            println!("{} Class '{}' not found.", "ERROR".red(), target_name);
            return Ok(());
        }
    };

    // BFS: seed with child methods, follow Calls edges
    let mut visited = HashSet::new();
    let mut queue = VecDeque::new();
    visited.insert(start_node.id.clone());

    // Seed source IDs (include sibling Class for Controllers)
    let mut seed_source_ids = vec![start_node.id.clone()];
    if start_node.label == NodeLabel::Controller {
        for n in graph.iter_nodes() {
            if n.label == NodeLabel::Class
                && n.properties.name == start_node.properties.name
                && n.properties.file_path == start_node.properties.file_path
            {
                seed_source_ids.push(n.id.clone());
            }
        }
    }

    // Seed child methods
    for rel in graph.iter_relationships() {
        if seed_source_ids.contains(&rel.source_id)
            && matches!(
                rel.rel_type,
                RelationshipType::HasMethod
                    | RelationshipType::HasProperty
                    | RelationshipType::HasAction
            )
            && visited.insert(rel.target_id.clone())
        {
            queue.push_back((rel.target_id.clone(), 0usize));
        }
    }

    // BFS following Calls edges, collecting all traversed methods
    let mut flow_methods: Vec<(String, String)> = Vec::new(); // (method_id, parent_class_name)
    let max_depth = 3;

    // Add seed methods
    for mid in visited.iter() {
        if let Some(node) = graph.get_node(mid) {
            if matches!(
                node.label,
                NodeLabel::Method | NodeLabel::Constructor | NodeLabel::ControllerAction
            ) {
                let parent_class = method_class
                    .get(mid)
                    .and_then(|cid| graph.get_node(cid))
                    .map(|n| n.properties.name.clone())
                    .unwrap_or_else(|| {
                        node.properties
                            .file_path
                            .rsplit('/')
                            .next()
                            .unwrap_or("?")
                            .trim_end_matches(".cs")
                            .to_string()
                    });
                flow_methods.push((mid.clone(), parent_class));
            }
        }
    }

    // Pre-build outgoing adjacency map for O(1) lookups instead of O(E) per BFS step
    let mut outgoing_calls: HashMap<String, Vec<(String, RelationshipType)>> = HashMap::new();
    for rel in graph.iter_relationships() {
        if matches!(
            rel.rel_type,
            RelationshipType::Calls
                | RelationshipType::CallsAction
                | RelationshipType::CallsService
        ) {
            outgoing_calls
                .entry(rel.source_id.clone())
                .or_default()
                .push((rel.target_id.clone(), rel.rel_type));
        }
    }

    while let Some((node_id, depth)) = queue.pop_front() {
        if depth >= max_depth {
            continue;
        }
        for (target_id, _rel_type) in outgoing_calls.get(&node_id).unwrap_or(&Vec::new()) {
            if visited.insert(target_id.clone()) {
                if let Some(target) = graph.get_node(target_id) {
                    if matches!(
                        target.label,
                        NodeLabel::Method | NodeLabel::Constructor | NodeLabel::ControllerAction
                    ) {
                        if target.properties.file_path.contains("StackLogger") {
                            continue;
                        }
                        let parent_class = method_class
                            .get(target_id)
                            .and_then(|cid| graph.get_node(cid))
                            .map(|n| n.properties.name.clone())
                            .unwrap_or_else(|| {
                                target
                                    .properties
                                    .file_path
                                    .rsplit('/')
                                    .next()
                                    .unwrap_or("?")
                                    .trim_end_matches(".cs")
                                    .to_string()
                            });
                        flow_methods.push((target_id.clone(), parent_class));
                        queue.push_back((target_id.clone(), depth + 1));
                    }
                }
            }
        }
    }

    // Group by parent class
    #[allow(clippy::type_complexity)]
    let mut grouped: BTreeMap<String, Vec<(String, bool, bool, Option<u32>)>> = BTreeMap::new();
    for (method_id, parent_class) in &flow_methods {
        if let Some(node) = graph.get_node(method_id) {
            let is_traced = node.properties.is_traced.unwrap_or(false);
            let is_dead = node.properties.is_dead_candidate.unwrap_or(false);
            grouped.entry(parent_class.clone()).or_default().push((
                node.properties.name.clone(),
                is_traced,
                is_dead,
                node.properties.start_line,
            ));
        }
    }

    // Print results
    println!();
    println!(
        "{} {} (depth {})",
        "Flow Coverage from".cyan(),
        start_node.properties.name.bold(),
        max_depth
    );
    println!("{}", "=".repeat(60).cyan());

    let mut total_methods = 0usize;
    let mut total_traced = 0usize;
    let mut total_dead = 0usize;

    for (class_name, methods) in &grouped {
        let traced = methods.iter().filter(|m| m.1).count();
        let dead = methods.iter().filter(|m| m.2).count();
        let count = methods.len();
        total_methods += count;
        total_traced += traced;
        total_dead += dead;

        let pct = if count > 0 {
            (traced as f64 / count as f64) * 100.0
        } else {
            0.0
        };
        println!();
        println!(
            "  {} ({}/{} traced, {:.0}%)",
            class_name.bold(),
            traced,
            count,
            pct
        );
        for (name, is_traced, is_dead, line) in methods {
            let marker = if *is_traced {
                "V".green().to_string()
            } else {
                "X".red().to_string()
            };
            let dead_marker = if *is_dead {
                " [dead]".dimmed().to_string()
            } else {
                String::new()
            };
            let line_info = line.map(|l| format!(":{}", l)).unwrap_or_default();
            println!(
                "    {} {}{}{}",
                marker,
                name,
                line_info.dimmed(),
                dead_marker
            );
        }
    }

    let total_pct = if total_methods > 0 {
        (total_traced as f64 / total_methods as f64) * 100.0
    } else {
        0.0
    };

    println!();
    println!("{}", "-".repeat(60));
    println!(
        "  Flow total: {}/{} methods traced ({:.1}%)",
        total_traced, total_methods, total_pct
    );
    println!("  Untraced in flow: {}", total_methods - total_traced);
    if total_dead > 0 {
        println!("  Dead code in flow: {}", total_dead);
    }
    println!();

    Ok(())
}

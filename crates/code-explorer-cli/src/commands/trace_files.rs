//! The `trace-files` command: list all source files involved in a feature.

use anyhow::Result;
use colored::Colorize;
use std::collections::{BTreeMap, HashSet, VecDeque};

use code_explorer_core::graph::types::{NodeLabel, RelationshipType};
use code_explorer_db::snapshot;

pub fn run(target: &str, path: Option<&str>, depth: usize, json: bool) -> Result<()> {
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

    // Find the target symbol — prefer Class/Controller over Constructor/Method
    let target_lower = target.to_lowercase();
    let mut candidates: Vec<_> = graph
        .iter_nodes()
        .filter(|n| n.properties.name.to_lowercase() == target_lower)
        .collect();
    candidates.sort_by_key(|n| match n.label {
        code_explorer_core::graph::types::NodeLabel::Controller => 0,
        code_explorer_core::graph::types::NodeLabel::Class => 1,
        code_explorer_core::graph::types::NodeLabel::Service => 2,
        _ => 10,
    });
    let start_node = candidates.first().copied();

    let start_node = match start_node {
        Some(n) => n,
        None => {
            println!(
                "{} Symbol '{}' not found in the graph.",
                "ERROR".red(),
                target
            );
            return Ok(());
        }
    };

    println!(
        "{} Tracing files from {} ({})",
        "->".cyan(),
        start_node.properties.name,
        start_node.label.as_str()
    );

    // BFS: follow outgoing relationships, seeding with child methods for Class/Service nodes
    let mut visited = HashSet::new();
    let mut queue = VecDeque::new();
    visited.insert(start_node.id.clone());
    queue.push_back((start_node.id.clone(), 0usize));

    // Collect: file_path -> (labels, depth)
    let mut files: BTreeMap<String, (Vec<String>, usize)> = BTreeMap::new();

    // Helper: check if a file_path should be included (filter out obj/ build artifacts)
    let is_valid_path =
        |p: &str| -> bool { !p.is_empty() && !p.contains("/obj/") && !p.contains("\\obj\\") };

    // Record start node's file
    if is_valid_path(&start_node.properties.file_path) {
        files
            .entry(start_node.properties.file_path.clone())
            .or_insert_with(|| (Vec::new(), 0))
            .0
            .push(format!(
                "{} ({})",
                start_node.properties.name,
                start_node.label.as_str()
            ));
    }

    // Seed: include child methods via HasMethod so BFS can follow their Calls edges.
    // For Controllers, also seed from the sibling Class node (which carries HasMethod edges).
    if matches!(
        start_node.label,
        NodeLabel::Class
            | NodeLabel::Service
            | NodeLabel::Interface
            | NodeLabel::Struct
            | NodeLabel::Controller
    ) {
        // For Controllers, find the sibling Class node (same name, same file)
        let seed_source_ids: Vec<String> = if start_node.label == NodeLabel::Controller {
            let mut ids = vec![start_node.id.clone()];
            for n in graph.iter_nodes() {
                if n.label == NodeLabel::Class
                    && n.properties.name == start_node.properties.name
                    && n.properties.file_path == start_node.properties.file_path
                {
                    ids.push(n.id.clone());
                }
            }
            ids
        } else {
            vec![start_node.id.clone()]
        };

        for rel in graph.iter_relationships() {
            if seed_source_ids.contains(&rel.source_id)
                && matches!(
                    rel.rel_type,
                    RelationshipType::HasMethod
                        | RelationshipType::HasProperty
                        | RelationshipType::HasAction
                )
                && !visited.contains(&rel.target_id)
            {
                visited.insert(rel.target_id.clone());
                queue.push_back((rel.target_id.clone(), 1));
                if let Some(member) = graph.get_node(&rel.target_id) {
                    if is_valid_path(&member.properties.file_path) {
                        let entry = files
                            .entry(member.properties.file_path.clone())
                            .or_insert_with(|| (Vec::new(), 1));
                        entry.0.push(format!(
                            "{} ({})",
                            member.properties.name,
                            member.label.as_str()
                        ));
                        entry.1 = entry.1.min(1);
                    }
                }
            }
        }
    }

    while let Some((node_id, d)) = queue.pop_front() {
        if d >= depth {
            continue;
        }

        // Follow OUTGOING relationships (source → target)
        for rel in graph.iter_relationships() {
            if rel.source_id != node_id {
                continue;
            }
            // Skip already-seeded HasMethod/HasProperty edges to avoid double-counting
            if matches!(
                rel.rel_type,
                RelationshipType::HasMethod | RelationshipType::HasProperty
            ) {
                continue;
            }
            let neighbor_id = &rel.target_id;

            if visited.contains(neighbor_id) {
                continue;
            }
            visited.insert(neighbor_id.clone());

            if let Some(neighbor) = graph.get_node(neighbor_id) {
                // Record this file
                if is_valid_path(&neighbor.properties.file_path) {
                    let entry = files
                        .entry(neighbor.properties.file_path.clone())
                        .or_insert_with(|| (Vec::new(), d + 1));
                    entry.0.push(format!(
                        "{} ({})",
                        neighbor.properties.name,
                        neighbor.label.as_str()
                    ));
                    entry.1 = entry.1.min(d + 1);
                }

                queue.push_back((neighbor_id.clone(), d + 1));
            }
        }
    }

    // Deduplicate symbols per file (by name, keeping the more specific label)
    for (_path, (symbols, _depth)) in files.iter_mut() {
        symbols.sort();
        symbols.dedup_by(|a, b| {
            let name_a = a.split(" (").next().unwrap_or(a);
            let name_b = b.split(" (").next().unwrap_or(b);
            name_a == name_b
        });
    }

    if json {
        let json_files: Vec<serde_json::Value> = files
            .iter()
            .map(|(path, (symbols, d))| {
                serde_json::json!({
                    "path": path.replace('\\', "/"),
                    "depth": d,
                    "symbols": symbols,
                })
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&json_files)?);
        return Ok(());
    }

    // Group files by category
    let mut controllers = Vec::new();
    let mut services = Vec::new();
    let mut views = Vec::new();
    let mut entities = Vec::new();
    let mut scripts = Vec::new();
    let mut other = Vec::new();

    for (path, (symbols, _d)) in &files {
        let path_lower = path.to_lowercase();
        let has_label = |lbl: &str| symbols.iter().any(|s| s.contains(lbl));

        if has_label("Controller") {
            controllers.push((path, symbols));
        } else if has_label("Service") || has_label("Repository") {
            services.push((path, symbols));
        } else if has_label("View") || path_lower.ends_with(".cshtml") {
            views.push((path, symbols));
        } else if has_label("DbEntity") || has_label("Entity") {
            entities.push((path, symbols));
        } else if path_lower.ends_with(".js")
            || path_lower.ends_with(".ts")
            || has_label("ScriptFile")
            || has_label("Function")
        {
            scripts.push((path, symbols));
        } else {
            other.push((path, symbols));
        }
    }

    let total = files.len();
    println!();
    println!("{} {} files trouvés pour '{}'", "OK".green(), total, target);
    println!();

    fn print_section(title: &str, items: &[(&String, &Vec<String>)], color: &str) {
        if items.is_empty() {
            return;
        }
        let colored_title = match color {
            "blue" => title.blue(),
            "green" => title.green(),
            "purple" => title.purple(),
            "yellow" => title.yellow(),
            "cyan" => title.cyan(),
            _ => title.white(),
        };
        println!("  {} ({})", colored_title, items.len());
        for (path, symbols) in items.iter().take(15) {
            let short_path = path.replace('\\', "/");
            let sym_preview: String = symbols
                .iter()
                .take(3)
                .cloned()
                .collect::<Vec<_>>()
                .join(", ");
            println!("    {} {}", short_path, sym_preview.dimmed());
        }
        if items.len() > 15 {
            println!("    ... +{} fichiers", items.len() - 15);
        }
        println!();
    }

    print_section("Controllers", &controllers, "blue");
    print_section("Services & Repositories", &services, "green");
    print_section("Views & Templates", &views, "purple");
    print_section("Entities (EF6)", &entities, "yellow");
    print_section("Scripts (JS/TS)", &scripts, "cyan");
    print_section("Autres", &other, "white");

    Ok(())
}

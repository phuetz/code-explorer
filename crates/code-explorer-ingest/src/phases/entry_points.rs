//! Registrations and dispatch arms whose handlers are inline rather than named AST functions.

use std::collections::HashMap;

use code_explorer_core::graph::types::{
    GraphNode, GraphRelationship, NodeLabel, NodeProperties, RelationshipType,
};
use code_explorer_core::graph::KnowledgeGraph;
use code_explorer_core::id::generate_id;
use once_cell::sync::Lazy;
use regex::Regex;

use super::structure::FileEntry;

#[derive(Debug, Default)]
pub struct EntryPointStats {
    pub routes: usize,
    pub registrations: usize,
    pub commands: usize,
}

static CSHARP_ROUTE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r#"\bMap(Get|Post|Put|Delete|Patch)\s*\(\s*"([^"]+)""#).unwrap());
static AXUM_ROUTE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r#"\.route\s*\(\s*"([^"]+)"\s*,\s*(get|post|put|delete|patch)\s*\(\s*([A-Za-z_]\w*)"#,
    )
    .unwrap()
});
static DI: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
    r"\b(AddScoped|AddSingleton|AddTransient)\s*<\s*([A-Za-z_]\w*)(?:\s*,\s*([A-Za-z_]\w*))?\s*>\s*\(").unwrap()
});
/// Open-generic MS DI: AddScoped(typeof(IReadRepository<>), typeof(EfRepository<>))
static DI_TYPEOF: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"\b(AddScoped|AddSingleton|AddTransient)\(\s*typeof\(([A-Za-z_]\w*(?:<>)?)\),\s*typeof\(([A-Za-z_]\w*(?:<>)?)\)",
    )
    .unwrap()
});
static CLAP_ARM: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"\bCommands::([A-Za-z_]\w*)\s*(?:\{|\()?").unwrap());
static COMMANDER: Lazy<Regex> =
    Lazy::new(|| Regex::new(r#"\.command\s*\(\s*["'`]([^"'`]+)["'`]"#).unwrap());
static CALL: Lazy<Regex> = Lazy::new(|| Regex::new(r"\b([A-Za-z_]\w*)\s*\(").unwrap());

pub fn extract_entry_points(graph: &mut KnowledgeGraph, files: &[FileEntry]) -> EntryPointStats {
    let mut stats = EntryPointStats::default();
    // Build once: matching every inline body against every graph node is costly
    // on large repositories.
    let mut symbols: HashMap<String, Vec<String>> = HashMap::new();
    for node in graph.iter_nodes() {
        if matches!(
            node.label,
            NodeLabel::Function
                | NodeLabel::Method
                | NodeLabel::Class
                | NodeLabel::Service
                | NodeLabel::Struct
        ) {
            symbols
                .entry(node.properties.name.clone())
                .or_default()
                .push(node.id.clone());
        }
    }

    for file in files {
        let path = file.path.as_str();
        let lines: Vec<&str> = file.content.lines().collect();
        if path.ends_with(".cs") {
            for (index, line) in lines.iter().enumerate() {
                for cap in DI.captures_iter(line) {
                    let service = cap.get(2).unwrap().as_str();
                    let name = format!("{}<{}>", &cap[1], service);
                    let id = add_node(
                        graph,
                        NodeLabel::CodeElement,
                        path,
                        index,
                        index,
                        &name,
                        None,
                        None,
                        "aspnet-di",
                    );
                    for type_name in [Some(service), cap.get(3).map(|m| m.as_str())]
                        .into_iter()
                        .flatten()
                    {
                        link_named(
                            graph,
                            &id,
                            type_name,
                            &symbols,
                            RelationshipType::DependsOn,
                            "DI registration",
                        );
                    }
                    stats.registrations += 1;
                }
                for cap in DI_TYPEOF.captures_iter(line) {
                    let service = cap.get(2).unwrap().as_str();
                    let impl_type = cap.get(3).unwrap().as_str();
                    let name = format!("{}<{}>", &cap[1], service);
                    let id = add_node(
                        graph,
                        NodeLabel::CodeElement,
                        path,
                        index,
                        index,
                        &name,
                        None,
                        None,
                        "aspnet-di",
                    );
                    for type_name in [service, impl_type] {
                        let bare = type_name.trim_end_matches("<>");
                        link_named(
                            graph,
                            &id,
                            bare,
                            &symbols,
                            RelationshipType::DependsOn,
                            "DI typeof registration",
                        );
                    }
                    stats.registrations += 1;
                }
                if let Some(cap) = CSHARP_ROUTE.captures(line) {
                    let method = cap[1].to_ascii_uppercase();
                    let route = &cap[2];
                    let end = body_end(&lines, index);
                    let id = add_node(
                        graph,
                        NodeLabel::ApiEndpoint,
                        path,
                        index,
                        end,
                        &format!("{method} {route}"),
                        Some(&method),
                        Some(route),
                        "aspnet-minimal",
                    );
                    link_calls(graph, &id, &lines[index..=end], &symbols);
                    // Parameter types identify services injected into the inline handler.
                    for registration in graph
                        .iter_nodes()
                        .filter(|n| {
                            n.label == NodeLabel::CodeElement
                                && n.properties.framework.as_deref() == Some("aspnet-di")
                        })
                        .filter(|n| {
                            lines[index..=end].join("\n").contains(
                                &n.properties.name.split(['<', '>']).nth(1).unwrap_or("\0"),
                            )
                        })
                        .map(|n| n.id.clone())
                        .collect::<Vec<_>>()
                    {
                        edge(
                            graph,
                            &id,
                            &registration,
                            RelationshipType::DependsOn,
                            "injected service",
                        );
                    }
                    stats.routes += 1;
                }
            }
        } else if path.ends_with(".rs") {
            for (index, line) in lines.iter().enumerate() {
                if let Some(cap) = AXUM_ROUTE.captures(line) {
                    let method = cap[2].to_ascii_uppercase();
                    let route = &cap[1];
                    let id = add_node(
                        graph,
                        NodeLabel::ApiEndpoint,
                        path,
                        index,
                        index,
                        &format!("{method} {route}"),
                        Some(&method),
                        Some(route),
                        "axum",
                    );
                    link_named(
                        graph,
                        &id,
                        &cap[3],
                        &symbols,
                        RelationshipType::HandledBy,
                        "axum handler",
                    );
                    stats.routes += 1;
                }
                if let Some(cap) = CLAP_ARM.captures(line) {
                    // A match arm starts with the variant and reaches `=>`
                    // within its pattern. Other references are not entries.
                    if !line.trim_start().starts_with("Commands::")
                        || !lines[index..(index + 16).min(lines.len())]
                            .join("\n")
                            .contains("=>")
                    {
                        continue;
                    }
                    let end = body_end(&lines, index);
                    if !lines[index..=end].join("\n").contains("=>") {
                        continue;
                    }
                    let name = format!("Commands::{}", &cap[1]);
                    let id = add_node(
                        graph,
                        NodeLabel::CodeElement,
                        path,
                        index,
                        end,
                        &name,
                        None,
                        None,
                        "clap",
                    );
                    link_calls(graph, &id, &lines[index..=end], &symbols);
                    stats.commands += 1;
                }
            }
        } else if matches!(
            path.rsplit('.').next(),
            Some("js" | "jsx" | "ts" | "tsx" | "mjs" | "cjs")
        ) {
            for (index, line) in lines.iter().enumerate() {
                if let Some(cap) = COMMANDER.captures(line) {
                    let command = cap[1].split_whitespace().next().unwrap_or(&cap[1]);
                    let end = (index + 100).min(lines.len().saturating_sub(1));
                    let end = ((index + 1)..=end)
                        .find(|&i| COMMANDER.is_match(lines[i]))
                        .map(|i| i - 1)
                        .unwrap_or(end);
                    let id = add_node(
                        graph,
                        NodeLabel::CodeElement,
                        path,
                        index,
                        end,
                        command,
                        None,
                        None,
                        "commander",
                    );
                    link_calls(graph, &id, &lines[index..=end], &symbols);
                    stats.commands += 1;
                }
            }
        }
    }
    stats
}

fn add_node(
    graph: &mut KnowledgeGraph,
    label: NodeLabel,
    path: &str,
    start: usize,
    end: usize,
    name: &str,
    method: Option<&str>,
    route: Option<&str>,
    framework: &str,
) -> String {
    let id = generate_id(label.as_str(), &format!("{path}:{}:{name}", start + 1));
    graph.add_node(GraphNode {
        id: id.clone(),
        label,
        properties: NodeProperties {
            name: name.into(),
            file_path: path.into(),
            start_line: Some((start + 1) as u32),
            end_line: Some((end + 1) as u32),
            http_method: method.map(str::to_string),
            route: route.map(str::to_string),
            route_template: route.map(str::to_string),
            framework: Some(framework.into()),
            ..Default::default()
        },
    });
    let file_id = generate_id("File", path);
    if graph.get_node(&file_id).is_some() {
        edge(
            graph,
            &file_id,
            &id,
            RelationshipType::Defines,
            "entry point declaration",
        );
    }
    id
}

fn edge(
    graph: &mut KnowledgeGraph,
    source: &str,
    target: &str,
    kind: RelationshipType,
    reason: &str,
) {
    if source == target {
        return;
    }
    graph.add_relationship(GraphRelationship {
        id: format!("entry:{source}:{target}:{}", kind.as_str()),
        source_id: source.into(),
        target_id: target.into(),
        rel_type: kind,
        confidence: 0.8,
        reason: reason.into(),
        step: None,
    });
}

fn link_named(
    graph: &mut KnowledgeGraph,
    source: &str,
    name: &str,
    symbols: &HashMap<String, Vec<String>>,
    kind: RelationshipType,
    reason: &str,
) {
    if let Some(targets) = symbols.get(name) {
        // An untyped inline call may have homonyms. Record a link only when
        // the name is reasonably specific; avoid a fan-out to every `run`.
        if targets.len() <= 8 {
            for target in targets {
                edge(graph, source, target, kind, reason);
            }
        }
    }
}

fn link_calls(
    graph: &mut KnowledgeGraph,
    source: &str,
    body: &[&str],
    symbols: &HashMap<String, Vec<String>>,
) {
    let Some(node) = graph.get_node(source) else {
        return;
    };
    let path = node.properties.file_path.clone();
    let start_line = node.properties.start_line.unwrap_or(1);
    let mut names = HashMap::new();
    for (offset, line) in body.iter().enumerate() {
        for cap in CALL.captures_iter(line) {
            names
                .entry(cap[1].to_string())
                .or_insert(start_line + offset as u32);
        }
    }
    for (name, line) in names {
        link_named(
            graph,
            source,
            &name,
            symbols,
            RelationshipType::Calls,
            &format!("inline handler call @ {path}:{line}"),
        );
    }
}

/// Find the closing brace of a C# lambda or Rust match arm, including the
/// argument/variant lines preceding `=> {`.
fn body_end(lines: &[&str], start: usize) -> usize {
    let mut opened = false;
    let mut saw_arrow = false;
    let mut depth = 0i32;
    for (index, line) in lines.iter().enumerate().skip(start).take(500) {
        let text = if !saw_arrow {
            if let Some(pos) = line.find("=>") {
                saw_arrow = true;
                &line[pos + 2..]
            } else {
                continue;
            }
        } else {
            line
        };
        for ch in text.chars() {
            if ch == '{' {
                opened = true;
                depth += 1;
            }
            if ch == '}' {
                depth -= 1;
            }
        }
        if opened && depth <= 0 {
            return index;
        }
        if saw_arrow && !opened && text.trim_end().ends_with(';') {
            return index;
        }
    }
    start
}

#[cfg(test)]
mod tests {
    use super::*;
    use code_explorer_core::config::languages::SupportedLanguage;

    fn file(path: &str, content: &str, lang: SupportedLanguage) -> FileEntry {
        FileEntry {
            path: path.into(),
            content: content.into(),
            language: Some(lang),
            size: content.len(),
        }
    }
    fn symbol(graph: &mut KnowledgeGraph, id: &str, label: NodeLabel, name: &str, path: &str) {
        graph.add_node(GraphNode {
            id: id.into(),
            label,
            properties: NodeProperties {
                name: name.into(),
                file_path: path.into(),
                ..Default::default()
            },
        });
    }

    #[test]
    fn minimal_api_routes_and_di_link_to_import_methods() {
        let mut graph = KnowledgeGraph::new();
        symbol(
            &mut graph,
            "class:import",
            NodeLabel::Class,
            "FolderImportService",
            "FolderImportService.cs",
        );
        symbol(
            &mut graph,
            "method:resolve",
            NodeLabel::Method,
            "TryResolveFolder",
            "FolderImportService.cs",
        );
        symbol(
            &mut graph,
            "method:import",
            NodeLabel::Method,
            "ImportAsync",
            "FolderImportService.cs",
        );
        let source = "builder.Services.AddScoped<FolderImportService>();\napp.MapGet(\"/api/admin/import/browse\", (FolderImportService importer) =>\n{\n    importer.TryResolveFolder(folder);\n});\napp.MapPost(\"/api/admin/import/folder\", async (FolderImportService importer) =>\n{\n    await importer.ImportAsync();\n});\n";
        let stats = extract_entry_points(
            &mut graph,
            &[file("Program.cs", source, SupportedLanguage::CSharp)],
        );
        assert_eq!((stats.routes, stats.registrations), (2, 1));
        let browse = graph
            .iter_nodes()
            .find(|n| n.properties.name == "GET /api/admin/import/browse")
            .unwrap();
        assert_eq!(browse.properties.start_line, Some(2));
        assert!(graph.iter_relationships().any(|r| r.source_id == browse.id
            && r.target_id == "method:resolve"
            && r.reason.contains("Program.cs:4")
            && r.confidence > 0.0));
        assert!(graph
            .iter_relationships()
            .any(|r| r.source_id == browse.id && r.rel_type == RelationshipType::DependsOn));
        assert!(graph
            .iter_nodes()
            .any(|n| n.properties.name == "AddScoped<FolderImportService>"));
    }

    #[test]
    fn clap_arm_and_axum_route_link_to_handlers() {
        let mut graph = KnowledgeGraph::new();
        symbol(
            &mut graph,
            "fn:open",
            NodeLabel::Function,
            "open_store",
            "main.rs",
        );
        symbol(
            &mut graph,
            "fn:compress",
            NodeLabel::Function,
            "compress_text_with_pipeline",
            "main.rs",
        );
        symbol(
            &mut graph,
            "fn:health",
            NodeLabel::Function,
            "health",
            "main.rs",
        );
        let source = "Router::new().route(\"/health\", get(health));\nmatch cli.command {\n Commands::Compress { input } => {\n  let store = open_store(input)?;\n  compress_text_with_pipeline(store);\n }\n}\n";
        let stats = extract_entry_points(
            &mut graph,
            &[file("main.rs", source, SupportedLanguage::Rust)],
        );
        assert_eq!((stats.routes, stats.commands), (1, 1));
        let arm = graph
            .iter_nodes()
            .find(|n| n.properties.name == "Commands::Compress")
            .unwrap();
        assert_eq!(arm.properties.start_line, Some(3));
        assert!(graph
            .iter_relationships()
            .any(|r| r.source_id == arm.id && r.target_id == "fn:open"));
        assert!(graph
            .iter_relationships()
            .any(|r| r.source_id == arm.id && r.target_id == "fn:compress"));
        assert!(graph
            .iter_relationships()
            .any(|r| r.rel_type == RelationshipType::HandledBy && r.target_id == "fn:health"));
    }

    #[test]
    fn commander_command_links_to_its_action() {
        let mut graph = KnowledgeGraph::new();
        symbol(
            &mut graph,
            "fn:serve",
            NodeLabel::Function,
            "serveMCP",
            "src/commands/mcp.ts",
        );
        let source = "program\n  .command(\"mcp-server\")\n  .action(async () => {\n    await serveMCP();\n  });\n";
        let stats = extract_entry_points(
            &mut graph,
            &[file("src/index.ts", source, SupportedLanguage::TypeScript)],
        );
        assert_eq!(stats.commands, 1);
        let command = graph
            .iter_nodes()
            .find(|n| n.properties.name == "mcp-server")
            .unwrap();
        assert_eq!(command.properties.start_line, Some(2));
        assert!(graph
            .iter_relationships()
            .any(|r| r.source_id == command.id && r.target_id == "fn:serve"));
    }
}

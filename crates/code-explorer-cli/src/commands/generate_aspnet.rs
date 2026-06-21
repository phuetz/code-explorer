//! ASP.NET MVC 5 / EF6 documentation generators.
//!
//! Generates documentation pages specific to ASP.NET projects:
//! - **controllers.md** — All controllers with routes, actions, model bindings
//! - **entities.md** — Entity model diagram with properties, associations, cardinality
//! - **views.md** — View inventory with model bindings, layouts, areas
//! - **routes.md** — API route table (sorted by path)
//! - **areas.md** — MVC Areas breakdown
//! - **data-model.md** — Full entity relationship diagram (Mermaid ER diagram)
//! - **seq-http.md** — Sequence diagrams: HTTP request lifecycle per controller
//! - **seq-data.md** — Sequence diagrams: Data access flow (Controller → DbContext → Entity)

use std::collections::{BTreeMap, HashSet};
use std::io::Write;
use std::path::Path;

use anyhow::Result;
use colored::Colorize;
use code_explorer_core::graph::types::*;
use code_explorer_core::graph::KnowledgeGraph;

/// Check if the graph contains any ASP.NET-specific nodes.
pub fn has_aspnet_content(graph: &KnowledgeGraph) -> bool {
    graph.iter_nodes().any(|n| {
        matches!(
            n.label,
            NodeLabel::Controller
                | NodeLabel::ControllerAction
                | NodeLabel::ApiEndpoint
                | NodeLabel::DbContext
                | NodeLabel::DbEntity
                | NodeLabel::View
                | NodeLabel::Area
        )
    })
}

/// Generate all ASP.NET documentation pages.
pub fn generate_aspnet_docs(
    graph: &KnowledgeGraph,
    docs_dir: &Path,
) -> Result<Vec<(String, String, String)>> {
    // Returns: Vec<(id, title, filename)> for _index.json integration
    let mut pages = Vec::new();

    // Collect ASP.NET nodes
    let controllers = collect_by_label(graph, NodeLabel::Controller);
    let actions = collect_by_label(graph, NodeLabel::ControllerAction);
    let api_endpoints = collect_by_label(graph, NodeLabel::ApiEndpoint);
    let views = collect_by_label(graph, NodeLabel::View);
    let entities = collect_by_label(graph, NodeLabel::DbEntity);
    let db_contexts = collect_by_label(graph, NodeLabel::DbContext);
    let areas = collect_by_label(graph, NodeLabel::Area);

    // 1. Controllers documentation
    if !controllers.is_empty() {
        generate_controllers_doc(docs_dir, &controllers, &actions, &api_endpoints, graph)?;
        pages.push((
            "aspnet-controllers".to_string(),
            "Contrôleurs & Actions".to_string(),
            "aspnet-controllers.md".to_string(),
        ));
    }

    // 2. Routes table
    if !actions.is_empty() || !api_endpoints.is_empty() {
        generate_routes_doc(docs_dir, &controllers, &actions, &api_endpoints)?;
        pages.push((
            "aspnet-routes".to_string(),
            "Table des Routes API".to_string(),
            "aspnet-routes.md".to_string(),
        ));
    }

    // 3. Entity model
    if !entities.is_empty() {
        generate_entities_doc(docs_dir, &entities, &db_contexts, graph)?;
        pages.push((
            "aspnet-entities".to_string(),
            "Modèle de Données".to_string(),
            "aspnet-entities.md".to_string(),
        ));
    }

    // 4. Views
    if !views.is_empty() {
        generate_views_doc(docs_dir, &views, &controllers, graph)?;
        pages.push((
            "aspnet-views".to_string(),
            "Vues & Templates".to_string(),
            "aspnet-views.md".to_string(),
        ));
    }

    // 5. Areas
    if !areas.is_empty() {
        generate_areas_doc(docs_dir, &areas, &controllers, &views)?;
        pages.push((
            "aspnet-areas".to_string(),
            "MVC Areas".to_string(),
            "aspnet-areas.md".to_string(),
        ));
    }

    // 6. Data model ER diagram
    if !entities.is_empty() {
        generate_er_diagram_doc(docs_dir, &entities, graph)?;
        pages.push((
            "aspnet-data-model".to_string(),
            "Diagramme Entités-Relations".to_string(),
            "aspnet-data-model.md".to_string(),
        ));
    }

    // 7. Sequence diagrams — HTTP request flow (per controller)
    if !controllers.is_empty() && (!actions.is_empty() || !views.is_empty()) {
        generate_sequence_http_doc(
            docs_dir,
            &controllers,
            &actions,
            &views,
            &db_contexts,
            graph,
        )?;
        pages.push((
            "aspnet-seq-http".to_string(),
            "Sequence: HTTP Request Flow".to_string(),
            "aspnet-seq-http.md".to_string(),
        ));
    }

    // 8. Sequence diagrams — Data access flow (DbContext → Entities)
    if !db_contexts.is_empty() && !entities.is_empty() {
        generate_sequence_data_doc(docs_dir, &controllers, &db_contexts, &entities, graph)?;
        pages.push((
            "aspnet-seq-data".to_string(),
            "Flux d'Accès aux Données".to_string(),
            "aspnet-seq-data.md".to_string(),
        ));
    }

    // 9. Services detail — methods, dependencies, callers
    let services: Vec<&GraphNode> = graph
        .iter_nodes()
        .filter(|n| n.label == NodeLabel::Service || n.label == NodeLabel::Repository)
        .collect();
    if !services.is_empty() {
        generate_services_detail_doc(docs_dir, &services, &controllers, graph)?;
        pages.push((
            "aspnet-services".to_string(),
            "Services & Repositories".to_string(),
            "aspnet-services.md".to_string(),
        ));
    }

    // 10. External services detail
    let ext_services: Vec<&GraphNode> = graph
        .iter_nodes()
        .filter(|n| n.label == NodeLabel::ExternalService)
        .collect();
    if !ext_services.is_empty() {
        generate_external_services_doc(docs_dir, &ext_services, &controllers, graph)?;
        pages.push((
            "aspnet-external".to_string(),
            "Services Externes".to_string(),
            "aspnet-external.md".to_string(),
        ));
    }

    // 11. Entity detail with properties
    if !entities.is_empty() {
        generate_entities_detail_doc(docs_dir, &entities, graph)?;
        pages.push((
            "aspnet-entities-detail".to_string(),
            "Détail des Entités".to_string(),
            "aspnet-entities-detail.md".to_string(),
        ));
    }

    Ok(pages)
}

// ─── External Services Documentation ────────────────────────────────────

fn generate_external_services_doc(
    docs_dir: &Path,
    ext_services: &[&GraphNode],
    controllers: &[&GraphNode],
    graph: &KnowledgeGraph,
) -> Result<()> {
    let out_path = docs_dir.join("aspnet-external.md");
    let mut f = std::fs::File::create(&out_path)?;

    writeln!(f, "# Services Externes")?;
    writeln!(f, "<!-- GNX:LEAD -->")?;
    writeln!(f)?;
    writeln!(
        f,
        "> Documentation des {} services externes (WebAPI, WCF, REST) appelés par l'application.",
        ext_services.len()
    )?;
    writeln!(f)?;

    // Summary table (only show columns that have data)
    writeln!(f, "## Vue d'ensemble")?;
    writeln!(f)?;
    let has_urls = ext_services
        .iter()
        .any(|s| s.properties.description.is_some());
    let has_files = ext_services
        .iter()
        .any(|s| !s.properties.file_path.is_empty());

    if has_urls && has_files {
        writeln!(f, "| Service | Type | Endpoint | Fichier |")?;
        writeln!(f, "|---------|------|----------|---------|")?;
    } else if has_urls {
        writeln!(f, "| Service | Type | Endpoint |")?;
        writeln!(f, "|---------|------|----------|")?;
    } else {
        writeln!(f, "| Service | Type |")?;
        writeln!(f, "|---------|------|")?;
    }
    for svc in ext_services {
        let svc_type = svc.properties.component_type.as_deref().unwrap_or("API");
        if has_urls && has_files {
            writeln!(
                f,
                "| **{}** | {} | {} | `{}` |",
                svc.properties.name,
                svc_type,
                svc.properties.description.as_deref().unwrap_or("-"),
                svc.properties.file_path.replace('\\', "/")
            )?;
        } else if has_urls {
            writeln!(
                f,
                "| **{}** | {} | {} |",
                svc.properties.name,
                svc_type,
                svc.properties.description.as_deref().unwrap_or("-")
            )?;
        } else {
            writeln!(f, "| **{}** | {} |", svc.properties.name, svc_type)?;
        }
    }
    writeln!(f)?;

    // Detail per service (skip if no useful content)
    for svc in ext_services {
        let methods: Vec<&GraphNode> = graph
            .iter_nodes()
            .filter(|n| {
                n.label == NodeLabel::Method
                    && n.properties.file_path == svc.properties.file_path
                    && !n.properties.name.starts_with("get_")
                    && !n.properties.name.starts_with("set_")
                    && !n.properties.name.starts_with("PrepareRequest")
                    && !n.properties.name.starts_with("ProcessResponse")
            })
            .collect();

        let callers_exist = controllers.iter().any(|c| {
            graph
                .iter_relationships()
                .any(|r| r.source_id == c.id && r.target_id == svc.id)
        });

        // Only write the section if there's content
        if methods.is_empty() && !callers_exist && svc.properties.description.is_none() {
            continue;
        }

        writeln!(f, "---")?;
        writeln!(f, "## {}", svc.properties.name)?;
        writeln!(f)?;

        if let Some(desc) = &svc.properties.description {
            writeln!(f, "**Endpoint :** `{}`", desc)?;
            writeln!(f)?;
        }

        if !methods.is_empty() {
            writeln!(f, "### Méthodes disponibles ({})", methods.len())?;
            writeln!(f)?;
            writeln!(f, "| Méthode | Type retour |")?;
            writeln!(f, "|---------|-------------|")?;
            for m in methods.iter().take(25) {
                let ret = m.properties.return_type.as_deref().unwrap_or("-");
                writeln!(f, "| `{}` | {} |", m.properties.name, ret)?;
            }
            if methods.len() > 25 {
                writeln!(f, "| ... | +{} méthodes |", methods.len() - 25)?;
            }
            writeln!(f)?;
        }

        // Which controllers call this service
        let callers: Vec<String> = controllers
            .iter()
            .filter(|c| {
                graph
                    .iter_relationships()
                    .any(|r| r.source_id == c.id && r.target_id == svc.id)
            })
            .map(|c| format!("`{}`", c.properties.name))
            .collect();

        if !callers.is_empty() {
            writeln!(f, "**Appelé par :** {}", callers.join(", "))?;
            writeln!(f)?;
        }
    }

    println!(
        "  {} aspnet-external.md ({} services)",
        "OK".green(),
        ext_services.len()
    );
    Ok(())
}

// ─── Entity Detail Documentation ────────────────────────────────────────

fn generate_entities_detail_doc(
    docs_dir: &Path,
    entities: &[&GraphNode],
    graph: &KnowledgeGraph,
) -> Result<()> {
    let out_path = docs_dir.join("aspnet-entities-detail.md");
    let mut f = std::fs::File::create(&out_path)?;

    writeln!(f, "# Détail des Entités")?;
    writeln!(f, "<!-- GNX:LEAD -->")?;
    writeln!(f)?;
    writeln!(
        f,
        "> Propriétés, types et relations de chaque entité Entity Framework ({} entités).",
        entities.len()
    )?;
    writeln!(f)?;

    for entity in entities {
        writeln!(f, "---")?;
        writeln!(f, "## {}", entity.properties.name)?;
        writeln!(f)?;
        writeln!(
            f,
            "**Fichier :** `{}`",
            entity.properties.file_path.replace('\\', "/")
        )?;
        writeln!(f)?;

        // Collect properties (Property nodes in the same file or linked)
        let props: Vec<&GraphNode> = graph
            .iter_nodes()
            .filter(|n| {
                n.label == NodeLabel::Property
                    && n.properties.file_path == entity.properties.file_path
            })
            .collect();

        if !props.is_empty() {
            writeln!(f, "### Propriétés ({})", props.len())?;
            writeln!(f)?;
            writeln!(f, "| Propriété | Type |")?;
            writeln!(f, "|-----------|------|")?;
            for p in props.iter().take(50) {
                let ptype = p.properties.return_type.as_deref().unwrap_or("-");
                writeln!(f, "| `{}` | {} |", p.properties.name, ptype)?;
            }
            if props.len() > 50 {
                writeln!(f, "| ... | +{} propriétés |", props.len() - 50)?;
            }
            writeln!(f)?;
        }

        // Navigation properties / associations
        let nav_rels: Vec<String> = graph
            .iter_relationships()
            .filter(|r| {
                r.source_id == entity.id
                    && matches!(
                        r.rel_type,
                        code_explorer_core::graph::types::RelationshipType::AssociatesWith
                    )
            })
            .filter_map(|r| {
                graph
                    .get_node(&r.target_id)
                    .map(|target| format!("`{}`", target.properties.name))
            })
            .collect();

        if !nav_rels.is_empty() {
            let unique: HashSet<String> = nav_rels.into_iter().collect();
            let mut sorted: Vec<String> = unique.into_iter().collect();
            sorted.sort();
            writeln!(f, "**Relations :** {}", sorted.join(", "))?;
            writeln!(f)?;
        }
    }

    println!(
        "  {} aspnet-entities-detail.md ({} entités)",
        "OK".green(),
        entities.len()
    );
    Ok(())
}

// ─── Services Detail Documentation ──────────────────────────────────────

fn generate_services_detail_doc(
    docs_dir: &Path,
    services: &[&GraphNode],
    controllers: &[&GraphNode],
    graph: &KnowledgeGraph,
) -> Result<()> {
    let out_path = docs_dir.join("aspnet-services.md");
    let mut f = std::fs::File::create(&out_path)?;

    writeln!(f, "# Services & Repositories")?;
    writeln!(f, "<!-- GNX:LEAD -->")?;
    writeln!(f)?;
    writeln!(
        f,
        "> Documentation des {} services et repositories de la couche métier.",
        services.len()
    )?;
    writeln!(f)?;

    // Summary table
    writeln!(f, "## Vue d'ensemble")?;
    writeln!(f)?;
    writeln!(f, "| Service | Type | Fichier | Méthodes |")?;
    writeln!(f, "|---------|------|---------|----------|")?;

    for svc in services {
        let svc_type = if svc.label == NodeLabel::Service {
            "Service"
        } else {
            "Repository"
        };

        // Count methods belonging to this service (same file)
        let method_count = graph
            .iter_nodes()
            .filter(|n| {
                n.label == NodeLabel::Method && n.properties.file_path == svc.properties.file_path
            })
            .count();

        let short_path: String = svc
            .properties
            .file_path
            .replace('\\', "/")
            .chars()
            .collect();
        writeln!(
            f,
            "| **{}** | {} | `{}` | {} |",
            svc.properties.name, svc_type, short_path, method_count
        )?;
    }
    writeln!(f)?;

    // Detail per service
    for svc in services {
        writeln!(f, "---")?;
        writeln!(f, "## {}", svc.properties.name)?;
        writeln!(f)?;

        let svc_type = if svc.label == NodeLabel::Service {
            "Service"
        } else {
            "Repository"
        };
        writeln!(
            f,
            "**Type :** {} | **Fichier :** `{}`",
            svc_type,
            svc.properties.file_path.replace('\\', "/")
        )?;
        writeln!(f)?;

        // Constructor-injected dependencies. Read the `DependsOn` edges
        // emitted by aspnet_mvc.rs Pass 10, which already resolved each
        // constructor parameter to an interface/service node and recorded
        // the reason as `constructor_injection:{iface_type}`. The previous
        // implementation listed Constructor nodes filtered by file_path —
        // which just printed the service's own class name back at the user
        // because in C# the constructor's `name` equals the class name.
        let mut deps: Vec<String> = graph
            .iter_relationships()
            .filter(|r| {
                r.source_id == svc.id
                    && r.rel_type == RelationshipType::DependsOn
                    && r.reason.starts_with("constructor_injection")
            })
            .filter_map(|r| {
                graph
                    .get_node(&r.target_id)
                    .map(|n| n.properties.name.clone())
            })
            .collect();
        deps.sort();
        deps.dedup();
        if !deps.is_empty() {
            writeln!(f, "**Dépendances injectées :**")?;
            for dep in &deps {
                writeln!(f, "- `{}`", dep)?;
            }
            writeln!(f)?;
        }

        // Methods
        let methods: Vec<&GraphNode> = graph
            .iter_nodes()
            .filter(|n| {
                n.label == NodeLabel::Method
                    && n.properties.file_path == svc.properties.file_path
                    && !n.properties.name.starts_with("get_")
                    && !n.properties.name.starts_with("set_")
            })
            .collect();

        if !methods.is_empty() {
            writeln!(f, "### Méthodes ({})", methods.len())?;
            writeln!(f)?;
            writeln!(f, "| Méthode | Paramètres | Type retour |")?;
            writeln!(f, "|---------|-----------|-------------|")?;
            for m in methods.iter().take(30) {
                let params = m
                    .properties
                    .parameter_count
                    .map(|c| format!("{} params", c))
                    .unwrap_or_else(|| "-".to_string());
                let ret = m.properties.return_type.as_deref().unwrap_or("-");
                writeln!(f, "| `{}` | {} | {} |", m.properties.name, params, ret)?;
            }
            if methods.len() > 30 {
                writeln!(f, "| ... | +{} méthodes | |", methods.len() - 30)?;
            }
            writeln!(f)?;
        }

        // Which controllers use this service. Use exact ID matches only —
        // the previous substring fallback (`source_id.contains(&class_name)`)
        // matched any node ID containing the controller's name as a substring,
        // producing false positives like OrderController matching OrderDetail.
        let callers: Vec<String> = controllers
            .iter()
            .filter(|c| {
                graph
                    .iter_relationships()
                    .any(|r| r.source_id == c.id && r.target_id == svc.id)
            })
            .map(|c| c.properties.name.clone())
            .collect();

        if !callers.is_empty() {
            writeln!(f, "**Utilisé par :** {}", callers.join(", "))?;
            writeln!(f)?;
        }
    }

    println!(
        "  {} aspnet-services.md ({} services)",
        "OK".green(),
        services.len()
    );
    Ok(())
}

// ─── Node Collection ─────────────────────────────────────────────────────

fn collect_by_label(graph: &KnowledgeGraph, label: NodeLabel) -> Vec<&GraphNode> {
    let mut nodes: Vec<&GraphNode> = graph.iter_nodes().filter(|n| n.label == label).collect();
    nodes.sort_by(|a, b| a.properties.name.cmp(&b.properties.name));
    nodes
}

// ─── Controllers Documentation ──────────────────────────────────────────

fn generate_controllers_doc(
    docs_dir: &Path,
    controllers: &[&GraphNode],
    actions: &[&GraphNode],
    api_endpoints: &[&GraphNode],
    graph: &KnowledgeGraph,
) -> Result<()> {
    let path = docs_dir.join("aspnet-controllers.md");
    let mut f = std::fs::File::create(path)?;

    writeln!(f, "# Controllers & Actions")?;
    writeln!(f)?;
    writeln!(
        f,
        "This project contains **{} controllers** with **{} MVC actions** and **{} API endpoints**.",
        controllers.len(),
        actions.len(),
        api_endpoints.len()
    )?;
    writeln!(f)?;

    for ctrl in controllers {
        let area = ctrl.properties.area_name.as_deref().unwrap_or("");
        writeln!(f, "## {}", ctrl.properties.name)?;
        writeln!(f)?;
        writeln!(f, "- **Fichier :** `{}`", ctrl.properties.file_path)?;
        if !area.is_empty() {
            writeln!(f, "- **Area :** {}", area)?;
        }
        if let Some(route) = ctrl.properties.route_template.as_deref() {
            if !route.is_empty() && route != "(convention)" {
                writeln!(f, "- **Route :** `{}`", route)?;
            }
        }
        writeln!(f)?;

        // Find actions for this controller
        let ctrl_actions: Vec<&&GraphNode> = actions
            .iter()
            .chain(api_endpoints.iter())
            .filter(|a| a.properties.file_path == ctrl.properties.file_path)
            .collect();

        if !ctrl_actions.is_empty() {
            writeln!(f, "### Actions")?;
            writeln!(f)?;
            writeln!(f, "| Méthode | Action | Route | Modèle | Type retour |")?;
            writeln!(f, "|--------|--------|-------|-------|-------------|")?;

            for action in ctrl_actions {
                let http = action.properties.http_method.as_deref().unwrap_or("GET");
                let route = action.properties.route_template.as_deref().unwrap_or("-");
                let model = action.properties.model_type.as_deref().unwrap_or("-");
                let ret = action.properties.return_type.as_deref().unwrap_or("-");

                writeln!(
                    f,
                    "| `{}` | **{}** | `{}` | {} | {} |",
                    http, action.properties.name, route, model, ret
                )?;
            }
            writeln!(f)?;
        }

        // Controller dependency diagram
        let rels: Vec<&GraphRelationship> = graph
            .iter_relationships()
            .filter(|r| r.source_id == ctrl.id || r.target_id == ctrl.id)
            .collect();

        // Only show high-level dependencies (Services, DbContexts, ExternalServices)
        // Skip HAS_ACTION, RENDERS_VIEW, etc. to keep diagrams readable
        let dep_rels: Vec<&&GraphRelationship> = rels
            .iter()
            .filter(|r| {
                matches!(
                    r.rel_type,
                    RelationshipType::DependsOn
                        | RelationshipType::CallsService
                        | RelationshipType::Calls
                )
            })
            .filter(|r| {
                let other_id = if r.source_id == ctrl.id {
                    &r.target_id
                } else {
                    &r.source_id
                };
                graph.get_node(other_id).is_some_and(|n| {
                    matches!(
                        n.label,
                        NodeLabel::Service | NodeLabel::DbContext | NodeLabel::ExternalService
                    )
                })
            })
            .collect();

        if !dep_rels.is_empty() {
            writeln!(f, "### Dependencies")?;
            writeln!(f)?;
            writeln!(f, "```mermaid")?;
            writeln!(f, "graph LR")?;
            writeln!(
                f,
                "  {}[\"{}\"]",
                sanitize_mermaid(&ctrl.properties.name),
                escape_mermaid_label(&ctrl.properties.name)
            )?;
            let mut seen_targets: HashSet<String> = HashSet::new();
            for rel in &dep_rels {
                let other_id = if rel.source_id == ctrl.id {
                    &rel.target_id
                } else {
                    &rel.source_id
                };
                if let Some(other) = graph.get_node(other_id) {
                    if seen_targets.insert(other.properties.name.clone()) {
                        writeln!(
                            f,
                            "  {} --> {}[\"{}\"]",
                            sanitize_mermaid(&ctrl.properties.name),
                            sanitize_mermaid(&other.properties.name),
                            escape_mermaid_label(&other.properties.name)
                        )?;
                    }
                }
            }
            writeln!(f, "```")?;
            writeln!(f)?;
        }
    }

    Ok(())
}

// ─── Routes Documentation ───────────────────────────────────────────────

fn generate_routes_doc(
    docs_dir: &Path,
    controllers: &[&GraphNode],
    actions: &[&GraphNode],
    api_endpoints: &[&GraphNode],
) -> Result<()> {
    let path = docs_dir.join("aspnet-routes.md");
    let mut f = std::fs::File::create(path)?;

    writeln!(f, "# API & Route Table")?;
    writeln!(f)?;

    // Collect all routes
    let mut routes: Vec<(&str, &str, &str, &str, &str)> = Vec::new(); // (method, route, action, controller, type)

    for action in actions.iter().chain(api_endpoints.iter()) {
        let method = action.properties.http_method.as_deref().unwrap_or("GET");
        let route = action.properties.route_template.as_deref().unwrap_or("-");
        let controller = controllers
            .iter()
            .find(|c| c.properties.file_path == action.properties.file_path)
            .map(|c| c.properties.name.as_str())
            .unwrap_or("-");
        let kind = if action.label == NodeLabel::ApiEndpoint {
            "API"
        } else {
            "MVC"
        };
        routes.push((method, route, &action.properties.name, controller, kind));
    }

    routes.sort_by(|a, b| a.1.cmp(b.1));

    writeln!(f, "| Méthode | Route | Action | Contrôleur | Type |")?;
    writeln!(f, "|--------|-------|--------|------------|------|")?;
    for (method, route, action, controller, kind) in &routes {
        writeln!(
            f,
            "| `{}` | `{}` | {} | {} | {} |",
            method, route, action, controller, kind
        )?;
    }
    writeln!(f)?;

    // Summary by HTTP method
    writeln!(f, "## Summary")?;
    writeln!(f)?;
    let mut method_counts: BTreeMap<&str, usize> = BTreeMap::new();
    for (method, _, _, _, _) in &routes {
        *method_counts.entry(method).or_default() += 1;
    }
    for (method, count) in &method_counts {
        writeln!(f, "- **{}**: {} routes", method, count)?;
    }

    Ok(())
}

// ─── Entities Documentation ─────────────────────────────────────────────

fn generate_entities_doc(
    docs_dir: &Path,
    entities: &[&GraphNode],
    db_contexts: &[&GraphNode],
    graph: &KnowledgeGraph,
) -> Result<()> {
    let path = docs_dir.join("aspnet-entities.md");
    let mut f = std::fs::File::create(path)?;

    writeln!(f, "# Entity Data Model")?;
    writeln!(f)?;
    writeln!(
        f,
        "This project defines **{} entities** across **{} DbContext(s)**.",
        entities.len(),
        db_contexts.len()
    )?;
    writeln!(f)?;

    // DbContexts
    for ctx in db_contexts {
        writeln!(f, "## {}", ctx.properties.name)?;
        writeln!(f)?;
        writeln!(f, "- **File:** `{}`", ctx.properties.file_path)?;
        if let Some(cs) = &ctx.properties.connection_string_name {
            writeln!(f, "- **Connection String:** `{}`", cs)?;
        }
        writeln!(f)?;

        // Find entities mapped through this context
        let mapped: Vec<&GraphRelationship> = graph
            .iter_relationships()
            .filter(|r| r.source_id == ctx.id && r.rel_type == RelationshipType::MapsToEntity)
            .collect();

        if !mapped.is_empty() {
            writeln!(f, "### Entity Sets")?;
            writeln!(f)?;
            writeln!(f, "| Entity | DbSet Property |")?;
            writeln!(f, "|--------|---------------|")?;
            for rel in &mapped {
                let entity_name = rel.reason.split(':').nth(1).unwrap_or(&rel.target_id);
                let dbset_info = &rel.reason;
                writeln!(f, "| {} | `{}` |", entity_name, dbset_info)?;
            }
            writeln!(f)?;
        }
    }

    // Individual entities
    for entity in entities {
        writeln!(f, "## {}", entity.properties.name)?;
        writeln!(f)?;
        writeln!(f, "- **File:** `{}`", entity.properties.file_path)?;
        if let Some(table) = &entity.properties.db_table_name {
            writeln!(f, "- **Table:** `{}`", table)?;
        }
        writeln!(f)?;

        // Data annotations
        if let Some(annotations) = &entity.properties.data_annotations {
            if !annotations.is_empty() {
                writeln!(f, "**Data Annotations:** {}", annotations.join(", "))?;
                writeln!(f)?;
            }
        }

        // Associations
        let associations: Vec<&GraphRelationship> = graph
            .iter_relationships()
            .filter(|r| r.source_id == entity.id && r.rel_type == RelationshipType::AssociatesWith)
            .collect();

        if !associations.is_empty() {
            writeln!(f, "### Relationships")?;
            writeln!(f)?;
            writeln!(f, "| Target | Cardinality | Source |")?;
            writeln!(f, "|--------|------------|--------|")?;
            for rel in &associations {
                let target_name = rel.target_id.rsplit(':').next().unwrap_or(&rel.target_id);
                let source_name = rel.source_id.rsplit(':').next().unwrap_or(&rel.source_id);
                let card = &rel.reason;
                writeln!(f, "| {} | {} | {} |", target_name, card, source_name)?;
            }
            writeln!(f)?;
        }
    }

    Ok(())
}

// ─── Views Documentation ────────────────────────────────────────────────

fn generate_views_doc(
    docs_dir: &Path,
    views: &[&GraphNode],
    _controllers: &[&GraphNode],
    graph: &KnowledgeGraph,
) -> Result<()> {
    let path = docs_dir.join("aspnet-views.md");
    let mut f = std::fs::File::create(path)?;

    // Filter out PackageTmp/obj duplicates — keep only source views
    let source_views: Vec<&&GraphNode> = views
        .iter()
        .filter(|v| {
            let p = &v.properties.file_path;
            !p.contains("PackageTmp") && !p.contains("/obj/") && !p.contains("\\obj\\")
        })
        .collect();

    // Build sets for cross-referencing
    let mut views_with_grid: HashSet<String> = HashSet::new();
    let mut views_with_form: HashSet<String> = HashSet::new();

    // Find views that have Telerik grids (via RendersComponent relationships)
    for rel in graph.iter_relationships() {
        if rel.rel_type == RelationshipType::RendersComponent {
            if let Some(src) = graph.get_node(&rel.source_id) {
                if src.label == NodeLabel::View || src.label == NodeLabel::PartialView {
                    views_with_grid.insert(src.properties.file_path.clone());
                }
            }
        }
        // Find views that have forms (via CallsAction with reason containing "beginform")
        if rel.rel_type == RelationshipType::CallsAction {
            if let Some(src) = graph.get_node(&rel.source_id) {
                if (src.label == NodeLabel::View || src.label == NodeLabel::PartialView)
                    && rel.reason.contains("beginform")
                {
                    views_with_form.insert(src.properties.file_path.clone());
                }
            }
        }
    }

    // Classify views
    let partial_views: Vec<&&GraphNode> = source_views
        .iter()
        .filter(|v| {
            v.label == NodeLabel::PartialView
                || v.properties
                    .file_path
                    .rsplit(['/', '\\'])
                    .next()
                    .unwrap_or("")
                    .starts_with('_')
        })
        .copied()
        .collect();
    let typed_views: Vec<&&GraphNode> = source_views
        .iter()
        .filter(|v| v.properties.model_type.is_some())
        .copied()
        .collect();
    let grid_views: Vec<&&GraphNode> = source_views
        .iter()
        .filter(|v| views_with_grid.contains(&v.properties.file_path))
        .copied()
        .collect();
    let form_views: Vec<&&GraphNode> = source_views
        .iter()
        .filter(|v| views_with_form.contains(&v.properties.file_path))
        .copied()
        .collect();

    writeln!(f, "# Views & Templates")?;
    writeln!(f)?;
    writeln!(
        f,
        "Total: **{} vues** (hors copies de déploiement)",
        source_views.len()
    )?;
    writeln!(f)?;

    // Summary stats
    writeln!(f, "| Catégorie | Nombre |")?;
    writeln!(f, "|-----------|--------|")?;
    writeln!(f, "| Vues avec **grille Telerik** | {} |", grid_views.len())?;
    writeln!(
        f,
        "| Vues avec **formulaire** (Html.BeginForm) | {} |",
        form_views.len()
    )?;
    writeln!(
        f,
        "| Vues avec **modèle typé** (@model) | {} |",
        typed_views.len()
    )?;
    writeln!(
        f,
        "| **Vues partielles** (_*.cshtml) | {} |",
        partial_views.len()
    )?;
    writeln!(f)?;

    // Section 1: Views with Telerik Grids (most important)
    if !grid_views.is_empty() {
        writeln!(f, "## Vues avec grille Telerik ({}) ⚡", grid_views.len())?;
        writeln!(f)?;
        writeln!(f, "> Ces vues contiennent des grilles de données Telerik — les écrans les plus complexes.")?;
        writeln!(f)?;
        writeln!(f, "| Vue | Écran | Modèle |")?;
        writeln!(f, "|-----|-------|--------|")?;
        for view in &grid_views {
            let filename = view
                .properties
                .file_path
                .rsplit(['/', '\\'])
                .next()
                .unwrap_or("");
            let folder = extract_view_folder(&view.properties.file_path);
            let model = view.properties.model_type.as_deref().unwrap_or("-");
            writeln!(f, "| `{}` | {} | {} |", filename, folder, model)?;
        }
        writeln!(f)?;
    }

    // Section 2: Views with Forms
    if !form_views.is_empty() {
        writeln!(f, "## Vues avec formulaire ({}) 📝", form_views.len())?;
        writeln!(f)?;
        writeln!(
            f,
            "> Ces vues contiennent des formulaires de saisie qui envoient des données au serveur."
        )?;
        writeln!(f)?;
        writeln!(f, "| Vue | Écran | Modèle |")?;
        writeln!(f, "|-----|-------|--------|")?;
        for view in &form_views {
            let filename = view
                .properties
                .file_path
                .rsplit(['/', '\\'])
                .next()
                .unwrap_or("");
            let folder = extract_view_folder(&view.properties.file_path);
            let model = view.properties.model_type.as_deref().unwrap_or("-");
            writeln!(f, "| `{}` | {} | {} |", filename, folder, model)?;
        }
        writeln!(f)?;
    }

    // Section 3: All views grouped by screen/controller
    writeln!(f, "## Toutes les vues par écran")?;
    writeln!(f)?;

    let mut grouped: BTreeMap<String, Vec<&&&GraphNode>> = BTreeMap::new();
    for view in &source_views {
        let folder = extract_view_folder(&view.properties.file_path);
        grouped.entry(folder).or_default().push(view);
    }

    for (folder, folder_views) in &grouped {
        writeln!(f, "### {} ({} vues)", folder, folder_views.len())?;
        writeln!(f)?;
        writeln!(f, "| Vue | Type | Modèle | Layout |")?;
        writeln!(f, "|-----|------|--------|--------|")?;

        for view in folder_views {
            let filename = view
                .properties
                .file_path
                .rsplit(['/', '\\'])
                .next()
                .unwrap_or("");
            let model = view.properties.model_type.as_deref().unwrap_or("-");
            let layout = view.properties.layout_path.as_deref().unwrap_or("-");
            let is_partial = filename.starts_with('_');
            let has_grid = views_with_grid.contains(&view.properties.file_path);
            let has_form = views_with_form.contains(&view.properties.file_path);

            let type_str = if is_partial && has_grid {
                "Partielle ⚡"
            } else if is_partial {
                "Partielle"
            } else if has_grid && has_form {
                "⚡📝"
            } else if has_grid {
                "⚡ Grille"
            } else if has_form {
                "📝 Formulaire"
            } else {
                "Vue"
            };

            writeln!(
                f,
                "| `{}` | {} | {} | {} |",
                filename, type_str, model, layout
            )?;
        }
        writeln!(f)?;
    }

    Ok(())
}

fn extract_view_folder(file_path: &str) -> String {
    // Walk path components and look for "Views" case-insensitively. Lowercasing
    // the whole path and indexing back into the original is unsafe — characters
    // like Turkish 'İ' lowercase to 3 bytes (vs 2 in the original), so a byte
    // index found in the lowered string can land in the middle of a multi-byte
    // char in the original and panic on slicing
    // (`extract_view_folder("Aİa/views/Ωfoo")` previously panicked with
    // `byte index 12 is not a char boundary` on the `&file_path[idx + 6..]`
    // line). The framework convention always spells the folder "Views" with
    // ASCII characters, so ASCII case folding is sufficient here.
    let mut parts = file_path.split(['/', '\\']);
    while let Some(part) = parts.next() {
        if part.eq_ignore_ascii_case("Views") {
            return parts.next().unwrap_or("Shared").to_string();
        }
    }
    "Autres".to_string()
}

// ─── Areas Documentation ────────────────────────────────────────────────

fn generate_areas_doc(
    docs_dir: &Path,
    areas: &[&GraphNode],
    controllers: &[&GraphNode],
    views: &[&GraphNode],
) -> Result<()> {
    let path = docs_dir.join("aspnet-areas.md");
    let mut f = std::fs::File::create(path)?;

    writeln!(f, "# MVC Areas")?;
    writeln!(f)?;
    writeln!(
        f,
        "This project is organized into **{} areas**.",
        areas.len()
    )?;
    writeln!(f)?;

    for area in areas {
        let area_name = area
            .properties
            .area_name
            .as_deref()
            .unwrap_or(&area.properties.name);
        writeln!(f, "## Area: {}", area_name)?;
        writeln!(f)?;

        // Controllers in this area
        let area_controllers: Vec<&&GraphNode> = controllers
            .iter()
            .filter(|c| c.properties.area_name.as_deref() == Some(area_name))
            .collect();

        if !area_controllers.is_empty() {
            writeln!(f, "### Controllers ({})", area_controllers.len())?;
            writeln!(f)?;
            for ctrl in &area_controllers {
                writeln!(
                    f,
                    "- **{}** (`{}`)",
                    ctrl.properties.name, ctrl.properties.file_path
                )?;
            }
            writeln!(f)?;
        }

        // Views in this area
        let area_views: Vec<&&GraphNode> = views
            .iter()
            .filter(|v| v.properties.area_name.as_deref() == Some(area_name))
            .collect();

        if !area_views.is_empty() {
            writeln!(f, "### Views ({})", area_views.len())?;
            writeln!(f)?;
            for view in &area_views {
                writeln!(f, "- `{}`", view.properties.file_path)?;
            }
            writeln!(f)?;
        }
    }

    Ok(())
}

// ─── ER Diagram Documentation ───────────────────────────────────────────

fn generate_er_diagram_doc(
    docs_dir: &Path,
    entities: &[&GraphNode],
    graph: &KnowledgeGraph,
) -> Result<()> {
    let path = docs_dir.join("aspnet-data-model.md");
    let mut f = std::fs::File::create(path)?;

    writeln!(f, "# Modèle de Données")?;
    writeln!(f)?;
    writeln!(
        f,
        "> {} entités détectées dans le modèle Entity Framework.",
        entities.len()
    )?;
    writeln!(f)?;

    // Build adjacency map for entities
    let entity_names: HashSet<String> =
        entities.iter().map(|e| e.properties.name.clone()).collect();

    let mut adjacency: BTreeMap<String, Vec<(String, String)>> = BTreeMap::new(); // entity -> [(related, cardinality)]
    for rel in graph.iter_relationships() {
        if rel.rel_type == RelationshipType::AssociatesWith {
            let source = rel
                .source_id
                .rsplit(':')
                .next()
                .unwrap_or(&rel.source_id)
                .to_string();
            let target = rel
                .target_id
                .rsplit(':')
                .next()
                .unwrap_or(&rel.target_id)
                .to_string();
            let card = if rel.reason.contains("1:*") {
                "1:N"
            } else if rel.reason.contains("*:1") {
                "N:1"
            } else if rel.reason.contains("*:*") {
                "N:N"
            } else {
                "1:1"
            };
            adjacency
                .entry(source.clone())
                .or_default()
                .push((target.clone(), card.to_string()));
            adjacency
                .entry(target)
                .or_default()
                .push((source, card.to_string()));
        }
    }

    // Group entities by business domain (heuristic based on name)
    let domains: Vec<(&str, Vec<&str>)> = vec![
        (
            "Aides & Barèmes",
            vec![
                "AIDEFINANCE",
                "GROUPEAIDE",
                "BAREME",
                "TRANCHEBAREME",
                "TARIFBASE_AIDE",
                "PARAMAIDE",
                "PARAMAIDJUST",
                "MAJOAIDE",
                "PLAFOND",
                "PLAFOND_DOSSIER",
                "REF_UNITETAR",
                "REF_UNITEMAJ",
                "REFFOND",
                "BUDGET",
            ],
        ),
        (
            "Dossiers & Prestations",
            vec![
                "DOSSIERPRESTA",
                "DOSSIERELIGIBLE",
                "ODDEMANDEUR",
                "BENEFPRESTA",
                "DSTRGTPOSSIBLE",
                "JUSTIFICATIFS",
                "STATDOSSIER",
            ],
        ),
        (
            "Paiements & Factures",
            vec!["REGLEMENT", "REGLEMENTLIGNE", "REGULLIGNE", "REF_STATREG"],
        ),
        (
            "Utilisateurs & Habilitations",
            vec![
                "UTILISATEUR",
                "PROFILS",
                "HABILITATION",
                "AUTORISATION",
                "AcmeUTILPROF",
                "Acme",
                "AcmePARAM",
                "PARAMAcme",
                "REF_FONCTION",
            ],
        ),
        (
            "Référentiels",
            vec![
                "REFTYPEBENEF",
                "REFTYPEDST",
                "REFTYPEODAD",
                "REFTYPEPENSION",
                "REFTYPEMDLCOUR",
                "REFPUBLIC",
                "PARAMPUBLIC",
                "REFFORMAT",
                "REFMESSAGE",
                "REF_UNITE",
                "REF_CONFIG",
                "REF_NUM",
                "REF_STATUTS",
            ],
        ),
        (
            "Courriers & Documents",
            vec![
                "MODELECOURRIER",
                "MODCOURAIDE",
                "MODCOURGRP",
                "DOCUMENT",
                "EXPORT",
            ],
        ),
        ("Audit", vec!["AUDIT", "Audit", "Auditligne"]),
    ];

    // Entity table with file paths
    writeln!(f, "## Liste des entités\n")?;
    writeln!(f, "| Entité | Fichier | Relations |")?;
    writeln!(f, "|--------|---------|-----------|")?;
    for entity in entities {
        let rel_count = adjacency
            .get(&entity.properties.name)
            .map_or(0, |v| v.len());
        writeln!(
            f,
            "| **{}** | `{}` | {} |",
            entity.properties.name, entity.properties.file_path, rel_count
        )?;
    }
    writeln!(f)?;

    // Per-domain diagrams
    for (domain_name, domain_entities) in &domains {
        // Filter to entities that actually exist in the graph
        let existing: Vec<&&str> = domain_entities
            .iter()
            .filter(|name| entity_names.contains(**name))
            .collect();

        if existing.is_empty() {
            continue;
        }

        writeln!(f, "## {}\n", domain_name)?;

        // Domain relationship diagram (graph LR for Mermaid 11.x compatibility)
        writeln!(f, "```mermaid")?;
        writeln!(f, "graph LR")?;

        let domain_set: HashSet<&str> = existing.iter().map(|n| **n).collect();

        // Entity nodes
        for name in &existing {
            writeln!(f, "    {}[\"{}\"]", sanitize_mermaid(name), name)?;
        }

        // Relationships within this domain
        let mut seen_rels: HashSet<String> = HashSet::new();
        for name in &existing {
            if let Some(rels) = adjacency.get(**name) {
                for (target, card) in rels {
                    if domain_set.contains(target.as_str()) {
                        let key = if **name < target.as_str() {
                            format!("{}:{}", name, target)
                        } else {
                            format!("{}:{}", target, name)
                        };
                        if seen_rels.insert(key) {
                            let arrow = match card.as_str() {
                                "1:N" => "-->|1:N|",
                                "N:1" => "-->|N:1|",
                                "N:N" => "-->|N:N|",
                                _ => "---",
                            };
                            writeln!(
                                f,
                                "    {} {} {}",
                                sanitize_mermaid(name),
                                arrow,
                                sanitize_mermaid(target)
                            )?;
                        }
                    }
                }
            }
        }

        writeln!(f, "```\n")?;
    }

    // Entities not in any domain
    let all_domain_entities: HashSet<&str> = domains
        .iter()
        .flat_map(|(_, ents)| ents.iter().copied())
        .collect();
    let unclassified: Vec<&&GraphNode> = entities
        .iter()
        .filter(|e| !all_domain_entities.contains(e.properties.name.as_str()))
        .collect();

    if !unclassified.is_empty() {
        writeln!(f, "## Autres entités\n")?;
        for entity in &unclassified {
            writeln!(
                f,
                "- **{}** (`{}`)",
                entity.properties.name, entity.properties.file_path
            )?;
        }
        writeln!(f)?;
    }

    writeln!(
        f,
        "> This diagram is auto-generated from the knowledge graph. Entity properties are extracted from data annotations and .edmx models."
    )?;

    Ok(())
}

// ─── Sequence Diagram: HTTP Request Flow ─────────────────────────────────

/// Generate sequence diagrams showing the HTTP request lifecycle per controller.
/// Client → Router → Controller → Action → View (→ ViewModel)
fn generate_sequence_http_doc(
    docs_dir: &Path,
    controllers: &[&GraphNode],
    actions: &[&GraphNode],
    views: &[&GraphNode],
    db_contexts: &[&GraphNode],
    graph: &KnowledgeGraph,
) -> Result<()> {
    let path = docs_dir.join("aspnet-seq-http.md");
    let mut f = std::fs::File::create(path)?;

    writeln!(f, "# Sequence Diagrams: HTTP Request Flow")?;
    writeln!(f)?;
    writeln!(
        f,
        "These diagrams show the lifecycle of HTTP requests through the ASP.NET MVC pipeline, from client to response."
    )?;
    writeln!(f)?;

    // Build lookup: controller ID → actions
    let mut ctrl_actions: BTreeMap<String, Vec<&GraphNode>> = BTreeMap::new();
    for rel in graph.iter_relationships() {
        if rel.rel_type == RelationshipType::HasAction {
            for action in actions.iter() {
                if action.id == rel.target_id && rel.source_id.contains("Controller") {
                    ctrl_actions
                        .entry(rel.source_id.clone())
                        .or_default()
                        .push(action);
                }
            }
        }
    }

    // Build lookup: action ID → rendered views
    let mut action_views: BTreeMap<String, Vec<&GraphNode>> = BTreeMap::new();
    for rel in graph.iter_relationships() {
        if rel.rel_type == RelationshipType::RendersView {
            for view in views.iter() {
                if view.id == rel.target_id {
                    action_views
                        .entry(rel.source_id.clone())
                        .or_default()
                        .push(view);
                }
            }
        }
    }

    // Build lookup: action ID → bound model
    let mut action_models: BTreeMap<String, String> = BTreeMap::new();
    for rel in graph.iter_relationships() {
        if rel.rel_type == RelationshipType::BindsModel {
            if let Some(target) = graph.get_node(&rel.target_id) {
                action_models.insert(rel.source_id.clone(), target.properties.name.clone());
            }
        }
    }

    // Find if any DbContext is used (for data access participants)
    let has_db = !db_contexts.is_empty();
    let db_name = db_contexts
        .first()
        .map(|d| d.properties.name.as_str())
        .unwrap_or("DbContext");

    // Generate diagrams for top 5 controllers (by action count) to keep docs readable
    let mut sorted_ctrls: Vec<&&GraphNode> = controllers.iter().collect();
    sorted_ctrls.sort_by(|a, b| {
        let a_count = ctrl_actions.get(&a.id).map_or(0, |v| v.len());
        let b_count = ctrl_actions.get(&b.id).map_or(0, |v| v.len());
        b_count.cmp(&a_count)
    });
    let top_ctrls: Vec<&&GraphNode> = sorted_ctrls.into_iter().take(5).collect();

    if controllers.len() > 5 {
        writeln!(f, "> Showing request flows for the **5 most active controllers** (out of {}). Each diagram shows the typical HTTP request lifecycle.\n", controllers.len())?;
    }

    for ctrl in top_ctrls {
        let ctrl_name = &ctrl.properties.name;
        let area = ctrl.properties.area_name.as_deref().unwrap_or("");
        let area_prefix = if area.is_empty() {
            String::new()
        } else {
            format!(" (Area: {})", area)
        };

        writeln!(f, "## {}{}", ctrl_name, area_prefix)?;
        writeln!(f)?;

        let Some(my_actions) = ctrl_actions.get(&ctrl.id).filter(|a| !a.is_empty()) else {
            writeln!(f, "_No actions detected for this controller._")?;
            writeln!(f)?;
            continue;
        };

        writeln!(f, "```mermaid")?;
        writeln!(f, "sequenceDiagram")?;
        writeln!(f, "    participant Client")?;
        writeln!(f, "    participant Router as ASP.NET Router")?;
        writeln!(
            f,
            "    participant Ctrl as {}",
            escape_mermaid_label(ctrl_name)
        )?;

        // Show max 3 representative actions to keep diagram readable
        let sample_actions: Vec<&&GraphNode> = my_actions.iter().take(3).collect();
        if my_actions.len() > 3 {
            writeln!(
                f,
                "    Note over Client: Showing {}/{} actions",
                sample_actions.len(),
                my_actions.len()
            )?;
        }

        // Collect unique views and models for sampled actions
        let mut view_participants: BTreeMap<String, bool> = BTreeMap::new();
        let mut model_participants: BTreeMap<String, bool> = BTreeMap::new();

        for action in &sample_actions {
            if let Some(views_list) = action_views.get(&action.id) {
                for v in views_list {
                    view_participants.insert(v.properties.name.clone(), true);
                }
            }
            if let Some(model) = action_models.get(&action.id) {
                model_participants.insert(model.clone(), true);
            }
        }

        // Declare view participants
        for view_name in view_participants.keys() {
            writeln!(
                f,
                "    participant {} as {}",
                sanitize_mermaid(view_name),
                escape_mermaid_label(view_name)
            )?;
        }

        // Declare model participants if any
        for model_name in model_participants.keys() {
            writeln!(
                f,
                "    participant {} as {}",
                sanitize_mermaid(model_name),
                escape_mermaid_label(model_name)
            )?;
        }

        // Declare DbContext if used
        if has_db {
            writeln!(f, "    participant DB as {}", escape_mermaid_label(db_name))?;
        }

        writeln!(f)?;

        // Generate interactions for sampled actions only
        for action in &sample_actions {
            let action_name = &action.properties.name;
            let http_method = action.properties.http_method.as_deref().unwrap_or("GET");
            let route = action.properties.route_template.as_deref().unwrap_or("/?");

            // Client → Router
            writeln!(
                f,
                "    Client->>Router: {} {}",
                http_method,
                escape_mermaid_label(route)
            )?;

            // Router → Controller
            writeln!(
                f,
                "    Router->>Ctrl: {}()",
                escape_mermaid_label(action_name)
            )?;

            // If action binds a model, show model binding
            if let Some(model_name) = action_models.get(&action.id) {
                writeln!(
                    f,
                    "    Note right of Ctrl: Model binding: {}",
                    escape_mermaid_label(model_name)
                )?;
            }

            // If action uses DbContext (heuristic: action has BindsModel to a DbEntity)
            if has_db {
                if let Some(model_name) = action_models.get(&action.id) {
                    // Check if the model is a DbEntity
                    let is_entity = graph.iter_nodes().any(|n| {
                        n.label == NodeLabel::DbEntity && n.properties.name == *model_name
                    });
                    if is_entity {
                        writeln!(
                            f,
                            "    Ctrl->>DB: Query {}",
                            escape_mermaid_label(model_name)
                        )?;
                        writeln!(f, "    DB-->>Ctrl: {}[]", escape_mermaid_label(model_name))?;
                    }
                }
            }

            // Controller → View
            if let Some(views_list) = action_views.get(&action.id) {
                for v in views_list {
                    let vn = &v.properties.name;
                    writeln!(f, "    Ctrl->>{}: Render", sanitize_mermaid(vn))?;
                    writeln!(f, "    {}-->>Client: HTML Response", sanitize_mermaid(vn))?;
                }
            } else {
                // No view: likely returns JSON or redirect
                writeln!(f, "    Ctrl-->>Client: Response (JSON/Redirect)")?;
            }

            writeln!(f)?;
        }

        writeln!(f, "```")?;
        writeln!(f)?;
    }

    Ok(())
}

// ─── Sequence Diagram: Data Access Flow ──────────────────────────────────

/// Generate sequence diagrams showing the data access patterns.
/// Controller → DbContext → Entity (showing which controllers access which entities through which DbContexts)
fn generate_sequence_data_doc(
    docs_dir: &Path,
    controllers: &[&GraphNode],
    db_contexts: &[&GraphNode],
    _entities: &[&GraphNode],
    graph: &KnowledgeGraph,
) -> Result<()> {
    let path = docs_dir.join("aspnet-seq-data.md");
    let mut f = std::fs::File::create(path)?;

    writeln!(f, "# Sequence Diagrams: Data Access Flow")?;
    writeln!(f)?;
    writeln!(
        f,
        "These diagrams show how controllers access data through Entity Framework DbContexts and entities."
    )?;
    writeln!(f)?;

    // Build: DbContext ID → entity names it exposes (via MapsToEntity)
    let mut ctx_entities: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for rel in graph.iter_relationships() {
        if rel.rel_type == RelationshipType::MapsToEntity {
            if let Some(entity) = graph.get_node(&rel.target_id) {
                ctx_entities
                    .entry(rel.source_id.clone())
                    .or_default()
                    .push(entity.properties.name.clone());
            }
        }
    }

    // Build: Controller → set of entity names it touches (via BindsModel on its actions)
    let mut ctrl_entities: BTreeMap<String, BTreeMap<String, Vec<String>>> = BTreeMap::new();
    // ctrl_id → { entity_name → [action_names] }
    for rel in graph.iter_relationships() {
        if rel.rel_type == RelationshipType::HasAction {
            let ctrl_id = &rel.source_id;
            let action_id = &rel.target_id;

            // Find what this action binds to
            for bind_rel in graph.iter_relationships() {
                if bind_rel.rel_type == RelationshipType::BindsModel
                    && bind_rel.source_id == *action_id
                {
                    if let Some(target) = graph.get_node(&bind_rel.target_id) {
                        // Only include if target is a DbEntity
                        if target.label == NodeLabel::DbEntity {
                            if let Some(action_node) = graph.get_node(action_id) {
                                ctrl_entities
                                    .entry(ctrl_id.clone())
                                    .or_default()
                                    .entry(target.properties.name.clone())
                                    .or_default()
                                    .push(action_node.properties.name.clone());
                            }
                        }
                    }
                }
            }
        }
    }

    // Overview diagram: all DbContexts with their entity sets
    writeln!(f, "## DbContext Overview")?;
    writeln!(f)?;
    writeln!(f, "```mermaid")?;
    writeln!(f, "sequenceDiagram")?;
    writeln!(f, "    participant App as Application")?;

    for ctx in db_contexts {
        writeln!(
            f,
            "    participant {} as {}",
            sanitize_mermaid(&ctx.properties.name),
            escape_mermaid_label(&ctx.properties.name)
        )?;
    }

    writeln!(f, "    participant DB as SQL Server")?;
    writeln!(f)?;

    for ctx in db_contexts {
        let ctx_safe = sanitize_mermaid(&ctx.properties.name);
        let ctx_label = escape_mermaid_label(&ctx.properties.name);

        if let Some(entity_names) = ctx_entities.get(&ctx.id) {
            writeln!(f, "    App->>{}: Open connection", ctx_safe)?;

            for entity_name in entity_names {
                writeln!(
                    f,
                    "    {}-->>DB: DbSet<{}>",
                    ctx_safe,
                    escape_mermaid_label(entity_name)
                )?;
            }

            writeln!(
                f,
                "    Note over {},DB: {} entities managed",
                ctx_label,
                entity_names.len()
            )?;
            writeln!(f)?;
        }
    }

    writeln!(f, "```")?;
    writeln!(f)?;

    // Per-controller data flow diagrams
    for ctrl in controllers {
        let ctrl_name = &ctrl.properties.name;
        let Some(entity_map) = ctrl_entities.get(&ctrl.id).filter(|m| !m.is_empty()) else {
            continue;
        };

        writeln!(f, "## {} — Data Access", ctrl_name)?;
        writeln!(f)?;

        // Find which DbContext serves these entities
        let mut relevant_ctx: Option<&GraphNode> = None;
        for ctx in db_contexts {
            if let Some(ctx_ents) = ctx_entities.get(&ctx.id) {
                if entity_map.keys().any(|e| ctx_ents.iter().any(|ce| ce == e)) {
                    relevant_ctx = Some(ctx);
                    break;
                }
            }
        }

        let ctx_name = relevant_ctx
            .map(|c| c.properties.name.as_str())
            .unwrap_or("DbContext");

        writeln!(f, "```mermaid")?;
        writeln!(f, "sequenceDiagram")?;
        writeln!(
            f,
            "    participant Ctrl as {}",
            escape_mermaid_label(ctrl_name)
        )?;
        writeln!(
            f,
            "    participant Ctx as {}",
            escape_mermaid_label(ctx_name)
        )?;

        // Declare entity participants
        for entity_name in entity_map.keys() {
            writeln!(
                f,
                "    participant {} as {}",
                sanitize_mermaid(entity_name),
                escape_mermaid_label(entity_name)
            )?;
        }

        writeln!(f, "    participant DB as SQL Server")?;
        writeln!(f)?;

        for (entity_name, action_names) in entity_map {
            let ent_safe = sanitize_mermaid(entity_name);

            // Show which actions trigger this entity access
            let actions_str = action_names.join(", ");
            writeln!(
                f,
                "    Note right of Ctrl: {}()",
                escape_mermaid_label(&actions_str)
            )?;

            writeln!(
                f,
                "    Ctrl->>Ctx: Query {}",
                escape_mermaid_label(entity_name)
            )?;
            writeln!(
                f,
                "    Ctx->>DB: SELECT FROM {}",
                escape_mermaid_label(entity_name)
            )?;
            writeln!(f, "    DB-->>Ctx: ResultSet")?;
            writeln!(f, "    Ctx-->>{}: Materialize", ent_safe)?;
            writeln!(f, "    {}-->>Ctrl: Entity data", ent_safe)?;
            writeln!(f)?;
        }

        writeln!(f, "```")?;
        writeln!(f)?;
    }

    Ok(())
}

// ─── Helpers ─────────────────────────────────────────────────────────────

/// Sanitize a name for use as a Mermaid node ID.
fn sanitize_mermaid(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

/// Escape a label for safe use inside Mermaid `["..."]` quoted strings.
/// Uses HTML entity form (`&amp;`, `&lt;`, etc.) — consistent with the rest
/// of the codebase and Mermaid's documented escape syntax. The previous
/// `#amp;` / `#quot;` shortcuts worked only in live rendering and leaked
/// literal `#amp;` strings into HTML/DOCX exports and search index text.
fn escape_mermaid_label(label: &str) -> String {
    label
        .replace('&', "&amp;")
        .replace('"', "&quot;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('[', "&#91;")
        .replace(']', "&#93;")
        .replace('\n', " ")
        .replace('\r', "")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_view_folder_basic() {
        assert_eq!(
            extract_view_folder("/MyApp/Views/Home/Index.cshtml"),
            "Home"
        );
        assert_eq!(
            extract_view_folder("MyApp\\Views\\Account\\Login.cshtml"),
            "Account"
        );
        assert_eq!(
            extract_view_folder("MyApp/Controllers/HomeController.cs"),
            "Autres"
        );
    }

    #[test]
    fn extract_view_folder_unicode_does_not_panic() {
        // Path with Turkish 'İ' (2 bytes) lowercases to 'i' + combining dot
        // (3 bytes), which used to shift byte indices and panic when slicing
        // the original. The walking-by-component implementation must handle
        // this safely without panicking.
        let _ = extract_view_folder("Aİa/views/Ωfoo");
        let _ = extract_view_folder("/SrcΩ/Views/Foo/Bar.cshtml");
        let _ = extract_view_folder("İVIEWS/Home/Index.cshtml");
        // Real Turkish project layout
        assert_eq!(
            extract_view_folder("/proje/Views/Müşteri/Index.cshtml"),
            "Müşteri"
        );
    }

    #[test]
    fn extract_view_folder_views_at_end() {
        // "Views" with no folder after — falls back to "Shared".
        assert_eq!(extract_view_folder("/MyApp/Views"), "Shared");
    }
}

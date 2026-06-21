//! Phase: ASP.NET MVC 5 / EF6 enrichment.
//!
//! This phase runs after Heritage and before Communities. It:
//! 1. Scans all C# files for ASP.NET controllers and promotes Class → Controller
//! 2. Extracts action methods and creates ControllerAction/ApiEndpoint nodes
//! 3. Detects DbContext classes and promotes them to DbContext nodes
//! 4. Identifies entity classes referenced in DbSets → DbEntity nodes
//! 5. Parses .edmx files for entity model details (properties, associations)
//! 6. Processes .cshtml files as View nodes with @model/@layout metadata
//! 7. Creates ASP.NET-specific relationships (RendersView, BelongsToArea, etc.)

use code_explorer_core::graph::types::{
    GraphNode, GraphRelationship, NodeLabel, NodeProperties, RelationshipType,
};
use code_explorer_core::graph::KnowledgeGraph;
use code_explorer_lang::route_extractors::csharp::{
    self, extract_ajax_calls, extract_constructor_dependencies, extract_external_service_calls,
    extract_form_actions, extract_partial_references, extract_services_and_repositories,
    extract_telerik_components, extract_tracing_info, ControllerInfo, DbContextInfo,
};
use code_explorer_lang::route_extractors::edmx;

use super::structure::FileEntry;
use once_cell::sync::Lazy;
use regex::Regex;
use std::collections::{HashMap, HashSet};

/// Result statistics from the ASP.NET MVC enrichment phase.
#[derive(Debug, Default)]
pub struct AspNetMvcStats {
    pub controllers: usize,
    pub actions: usize,
    pub api_endpoints: usize,
    pub views: usize,
    pub db_contexts: usize,
    pub db_entities: usize,
    pub areas: usize,
    pub edmx_models: usize,
    pub filters: usize,
    pub webconfigs: usize,
    pub partial_views: usize,
    pub ajax_calls: usize,
    pub script_files: usize,
    pub ui_components: usize,
    pub services: usize,
    pub repositories: usize,
    pub traced_files: usize,
    pub trace_calls: usize,
    pub external_services: usize,
}

/// Run the ASP.NET MVC enrichment phase.
///
/// Returns the count of ASP.NET-specific nodes created.
pub fn enrich_aspnet_mvc(
    graph: &mut KnowledgeGraph,
    file_entries: &[FileEntry],
) -> Result<AspNetMvcStats, crate::IngestError> {
    // Quick check: does this project have any C# files?
    let has_csharp = file_entries.iter().any(|f| {
        f.path.ends_with(".cs") || f.path.ends_with(".cshtml") || f.path.ends_with(".edmx")
    });

    if !has_csharp {
        return Ok(AspNetMvcStats::default());
    }

    let mut stats = AspNetMvcStats::default();
    let mut known_entity_types: HashSet<String> = HashSet::new();
    let mut area_nodes: HashMap<String, String> = HashMap::new(); // area name → node ID

    // ──────────────────────────────────────────────────────────────────────
    // Pass 1: Parse .edmx files for entity model information
    // ──────────────────────────────────────────────────────────────────────
    let edmx_files: Vec<&FileEntry> = file_entries
        .iter()
        .filter(|f| f.path.ends_with(".edmx"))
        .collect();

    let mut edmx_models: Vec<edmx::EdmxModel> = Vec::new();
    for entry in &edmx_files {
        let model = edmx::parse_edmx(&entry.content);
        // Collect known entity types
        for et in &model.entity_types {
            known_entity_types.insert(et.name.clone());
        }
        for es in &model.entity_sets {
            let clean = edmx::clean_entity_type_name(&es.entity_type);
            known_entity_types.insert(clean.to_string());
        }
        edmx_models.push(model);
        stats.edmx_models += 1;
    }

    // ──────────────────────────────────────────────────────────────────────
    // Pass 1b: Parse Web.config files
    // ──────────────────────────────────────────────────────────────────────
    let re_auth =
        Regex::new(r#"<authentication\s+mode="([^"]+)""#).expect("regex pattern must compile");
    let re_conn = Regex::new(r#"<add\s+name="[^"]+"\s+connectionString="#)
        .expect("regex pattern must compile");
    let re_appsettings =
        Regex::new(r#"<appSettings>[\s\S]*?<add\s+key="#).expect("regex pattern must compile");
    let re_binding =
        Regex::new(r#"<bindingRedirect\s+oldVersion="#).expect("regex pattern must compile");

    for entry in file_entries {
        if entry.path.ends_with("Web.config") || entry.path.ends_with("web.config") {
            let webconfig_id = format!("WebConfig:{}", entry.path);

            let mut description_parts = Vec::new();

            if let Some(auth_match) = re_auth.captures(&entry.content) {
                description_parts.push(format!("auth: {}", &auth_match[1]));
            }

            let conn_string_count = re_conn.find_iter(&entry.content).count();
            if conn_string_count > 0 {
                description_parts.push(format!("{} connection strings", conn_string_count));
            }

            let app_settings_count = re_appsettings.find_iter(&entry.content).count();
            if app_settings_count > 0 {
                description_parts.push(format!("{} app settings", app_settings_count));
            }

            let binding_redirects = re_binding.find_iter(&entry.content).count();
            if binding_redirects > 0 {
                description_parts.push(format!("{} binding redirects", binding_redirects));
            }

            let description = if description_parts.is_empty() {
                None
            } else {
                Some(description_parts.join("; "))
            };

            graph.add_node(GraphNode {
                id: webconfig_id,
                label: NodeLabel::WebConfig,
                properties: NodeProperties {
                    name: "Web.config".to_string(),
                    file_path: entry.path.clone(),
                    description,
                    ..Default::default()
                },
            });

            stats.webconfigs += 1;
        }
    }

    // ──────────────────────────────────────────────────────────────────────
    // Pass 2: Scan C# files for controllers and DbContexts
    // ──────────────────────────────────────────────────────────────────────
    let cs_files: Vec<&FileEntry> = file_entries
        .iter()
        .filter(|f| f.path.ends_with(".cs"))
        .collect();

    let mut all_controllers: Vec<(String, ControllerInfo)> = Vec::new(); // (file_path, info)
    let mut all_db_contexts: Vec<(String, DbContextInfo)> = Vec::new();

    for entry in &cs_files {
        // Extract controllers
        let controllers = csharp::extract_controllers(&entry.content);
        for ctrl in controllers {
            all_controllers.push((entry.path.clone(), ctrl));
        }

        // Extract DbContexts
        let contexts = csharp::extract_db_contexts(&entry.content);
        for ctx in contexts {
            // Add entity types from DbSet<T> to known types
            for es in &ctx.entity_sets {
                known_entity_types.insert(es.entity_type.clone());
            }
            all_db_contexts.push((entry.path.clone(), ctx));
        }
    }

    // ──────────────────────────────────────────────────────────────────────
    // Pass 3: Extract entities from C# files (using known types from DbContext + .edmx)
    // ──────────────────────────────────────────────────────────────────────
    let known_types: Vec<String> = known_entity_types.iter().cloned().collect();
    let mut entity_file_map: HashMap<String, String> = HashMap::new(); // entity name → node ID

    for entry in &cs_files {
        let entities = csharp::extract_entities(&entry.content, &known_types);
        for entity in entities {
            let node_id = format!("DbEntity:{}:{}", entry.path, entity.class_name);

            // Create or promote the node
            let annotations: Vec<String> = entity
                .property_annotations
                .values()
                .flat_map(|v| v.iter().cloned())
                .collect();

            graph.add_node(GraphNode {
                id: node_id.clone(),
                label: NodeLabel::DbEntity,
                properties: NodeProperties {
                    name: entity.class_name.clone(),
                    file_path: entry.path.clone(),
                    db_table_name: entity.table_name,
                    data_annotations: if annotations.is_empty() {
                        None
                    } else {
                        Some(annotations)
                    },
                    ..Default::default()
                },
            });

            entity_file_map.insert(entity.class_name.clone(), node_id.clone());

            // Create navigation property relationships
            for nav in &entity.navigation_properties {
                // These will be linked in a later pass when all entities are known
                // For now, record them
                let target_id = format!("DbEntity:*:{}", nav.target_type);
                // Scope by source file to avoid ID collisions when two files
                // both define an entity with the same class name (e.g., a
                // generated EF class and a domain model in another namespace).
                let rel_id = format!(
                    "assoc:{}:{}:{}:{}",
                    entry.path, entity.class_name, nav.name, nav.target_type
                );
                let cardinality = if nav.is_collection { "1:*" } else { "*:1" };

                graph.add_relationship(GraphRelationship {
                    id: rel_id,
                    source_id: node_id.clone(),
                    target_id,
                    rel_type: RelationshipType::AssociatesWith,
                    confidence: 0.8,
                    reason: format!("navigation_property:{}", cardinality),
                    step: None,
                });
            }

            stats.db_entities += 1;
        }
    }

    // ──────────────────────────────────────────────────────────────────────
    // Pass 4: Create DbContext nodes and MapsToEntity relationships
    // ──────────────────────────────────────────────────────────────────────
    for (file_path, ctx) in &all_db_contexts {
        let ctx_id = format!("DbContext:{}:{}", file_path, ctx.class_name);

        graph.add_node(GraphNode {
            id: ctx_id.clone(),
            label: NodeLabel::DbContext,
            properties: NodeProperties {
                name: ctx.class_name.clone(),
                file_path: file_path.clone(),
                connection_string_name: ctx.connection_string_name.clone(),
                ..Default::default()
            },
        });

        // Create MAPS_TO_ENTITY relationships
        for es in &ctx.entity_sets {
            let entity_node_id = entity_file_map
                .get(&es.entity_type)
                .cloned()
                .unwrap_or_else(|| format!("DbEntity:*:{}", es.entity_type));

            graph.add_relationship(GraphRelationship {
                // Scope by file_path so partial-class DbContexts (multiple
                // files defining the same DbContext class) don't collide on
                // the same `maps:Class:Entity` ID.
                id: format!("maps:{}:{}:{}", file_path, ctx.class_name, es.entity_type),
                source_id: ctx_id.clone(),
                target_id: entity_node_id,
                rel_type: RelationshipType::MapsToEntity,
                confidence: 1.0,
                reason: format!("DbSet<{}>:{}", es.entity_type, es.property_name),
                step: None,
            });
        }

        stats.db_contexts += 1;
    }

    // ──────────────────────────────────────────────────────────────────────
    // Pass 5: Create Controller nodes, Action nodes, and relationships
    // ──────────────────────────────────────────────────────────────────────
    for (file_path, ctrl) in &all_controllers {
        let ctrl_id = format!("Controller:{}:{}", file_path, ctrl.class_name);

        // Create Area node if needed
        if let Some(area_name) = &ctrl.area_name {
            if !area_nodes.contains_key(area_name) {
                let area_id = format!("Area:{}", area_name);
                graph.add_node(GraphNode {
                    id: area_id.clone(),
                    label: NodeLabel::Area,
                    properties: NodeProperties {
                        name: area_name.clone(),
                        file_path: String::new(),
                        area_name: Some(area_name.clone()),
                        ..Default::default()
                    },
                });
                area_nodes.insert(area_name.clone(), area_id);
                stats.areas += 1;
            }
        }

        // Also detect area from path: Areas/<Name>/Controllers/...
        let inferred_area = infer_area_from_path(file_path);
        let effective_area = ctrl.area_name.as_ref().or(inferred_area.as_ref());

        graph.add_node(GraphNode {
            id: ctrl_id.clone(),
            label: NodeLabel::Controller,
            properties: NodeProperties {
                name: ctrl.class_name.clone(),
                file_path: file_path.clone(),
                area_name: effective_area.cloned(),
                route_template: ctrl.route_prefix.clone(),
                ..Default::default()
            },
        });

        // BELONGS_TO_AREA relationship
        if let Some(area_name) = effective_area {
            if !area_nodes.contains_key(area_name) {
                let area_id = format!("Area:{}", area_name);
                graph.add_node(GraphNode {
                    id: area_id.clone(),
                    label: NodeLabel::Area,
                    properties: NodeProperties {
                        name: area_name.clone(),
                        file_path: String::new(),
                        area_name: Some(area_name.clone()),
                        ..Default::default()
                    },
                });
                area_nodes.insert(area_name.clone(), area_id);
                stats.areas += 1;
            }

            if let Some(area_id) = area_nodes.get(area_name) {
                graph.add_relationship(GraphRelationship {
                    id: format!("area:{}:{}:{}", file_path, ctrl.class_name, area_name),
                    source_id: ctrl_id.clone(),
                    target_id: area_id.clone(),
                    rel_type: RelationshipType::BelongsToArea,
                    confidence: 1.0,
                    reason: "area_attribute".to_string(),
                    step: None,
                });
            }
        }

        // Create action nodes
        for action in &ctrl.actions {
            let label = if ctrl.is_api_controller {
                NodeLabel::ApiEndpoint
            } else {
                NodeLabel::ControllerAction
            };

            let action_id = format!(
                "{}:{}:{}:{}",
                if ctrl.is_api_controller {
                    "ApiEndpoint"
                } else {
                    "ControllerAction"
                },
                file_path,
                ctrl.class_name,
                action.name
            );

            // Build route template by combining controller prefix + action route
            let full_route = build_full_route(
                ctrl.route_prefix.as_deref(),
                action.route_template.as_deref(),
                &ctrl.class_name,
                &action.name,
            );

            graph.add_node(GraphNode {
                id: action_id.clone(),
                label,
                properties: NodeProperties {
                    name: action.name.clone(),
                    file_path: file_path.clone(),
                    start_line: action.start_line,
                    http_method: Some(action.http_method.clone()),
                    route_template: Some(full_route),
                    model_type: action.model_type.clone(),
                    return_type: action.return_type.clone(),
                    description: action.parameters.clone(),
                    ..Default::default()
                },
            });

            // HAS_ACTION relationship — scope edge ID by file_path so two
            // controllers with the same class name in different files don't
            // silently clobber each other.
            graph.add_relationship(GraphRelationship {
                id: format!("action:{}:{}:{}", file_path, ctrl.class_name, action.name),
                source_id: ctrl_id.clone(),
                target_id: action_id.clone(),
                rel_type: RelationshipType::HasAction,
                confidence: 1.0,
                reason: action.http_method.clone(),
                step: None,
            });

            // BINDS_MODEL relationship
            if let Some(model_type) = &action.model_type {
                let target_id = entity_file_map
                    .get(model_type)
                    .cloned()
                    .unwrap_or_else(|| format!("ViewModel:*:{}", model_type));

                graph.add_relationship(GraphRelationship {
                    id: format!(
                        "binds:{}:{}:{}:{}",
                        file_path, ctrl.class_name, action.name, model_type
                    ),
                    source_id: action_id.clone(),
                    target_id,
                    rel_type: RelationshipType::BindsModel,
                    confidence: 0.9,
                    reason: "parameter_binding".to_string(),
                    step: None,
                });
            }

            if ctrl.is_api_controller {
                stats.api_endpoints += 1;
            } else {
                stats.actions += 1;
            }
        }

        stats.controllers += 1;
    }

    // ──────────────────────────────────────────────────────────────────────
    // Pass 5b: Extract filter attributes from controllers
    // ──────────────────────────────────────────────────────────────────────

    // Standard well-known ASP.NET MVC filter attributes
    let filter_regex = Regex::new(
        r"\[(?:(?:System\.Web\.Mvc\.)?)(Authorize|ValidateAntiForgeryToken|OutputCache|HandleError|AllowAnonymous|RequireHttps|ActionFilter|ExceptionFilter|ResultFilter)(?:\(([^)]*)\))?\]"
    ).expect("regex pattern must compile");

    // Match any [SomethingAttribute] or [SomethingFilter] or [SomethingAction] pattern
    // to catch custom attributes like [AuthorizeADAttribute], [VerifActionFilter], [GridAction]
    static RE_CUSTOM_FILTER: Lazy<Regex> = Lazy::new(|| {
        Regex::new(r#"\[(\w+(?:Attribute|Filter|Action))\s*(?:\([^)]*\))?\]"#)
            .expect("RE_CUSTOM_FILTER regex must compile")
    });

    // Standard filter names for confidence check
    let standard_filters: HashSet<&str> = [
        "Authorize",
        "ValidateAntiForgeryToken",
        "OutputCache",
        "HandleError",
        "AllowAnonymous",
        "RequireHttps",
        "ActionFilter",
        "ExceptionFilter",
        "ResultFilter",
    ]
    .iter()
    .copied()
    .collect();

    for (file_path, ctrl) in &all_controllers {
        let content = file_entries
            .iter()
            .find(|f| f.path == *file_path)
            .map(|f| f.content.as_str())
            .unwrap_or("");
        if !content.is_empty() {
            let mut seen_filters: HashSet<String> = HashSet::new();

            // First pass: standard filters (high confidence 0.95)
            for cap in filter_regex.captures_iter(content) {
                let filter_name = &cap[1];
                let filter_params = cap.get(2).map_or("", |m| m.as_str());

                let filter_key = if filter_params.is_empty() {
                    filter_name.to_string()
                } else {
                    format!("{}({})", filter_name, filter_params)
                };

                if !seen_filters.contains(&filter_key) {
                    let filter_id = format!("Filter:{}", filter_name);

                    if graph.get_node(&filter_id).is_none() {
                        let mut props = NodeProperties {
                            name: filter_name.to_string(),
                            file_path: String::new(),
                            ..Default::default()
                        };
                        if !filter_params.is_empty() {
                            props.description = Some(filter_params.to_string());
                        }
                        graph.add_node(GraphNode {
                            id: filter_id.clone(),
                            label: NodeLabel::Filter,
                            properties: props,
                        });
                    }

                    let ctrl_id = format!("Controller:{}:{}", file_path, ctrl.class_name);
                    // Scope edge ID by `filter_key` (which includes params) so
                    // a controller carrying the same filter type with different
                    // params produces distinct edges instead of silently
                    // overwriting earlier ones via add_relationship.
                    graph.add_relationship(GraphRelationship {
                        id: format!("filter:{}:{}:{}", file_path, ctrl.class_name, filter_key),
                        source_id: ctrl_id,
                        target_id: filter_id,
                        rel_type: RelationshipType::HasFilter,
                        confidence: 0.95,
                        reason: if filter_params.is_empty() {
                            "attribute".to_string()
                        } else {
                            format!("attribute:{}", filter_params)
                        },
                        step: None,
                    });

                    seen_filters.insert(filter_key);
                    stats.filters += 1;
                }
            }

            // Second pass: custom filters matching *Attribute, *Filter, *Action patterns
            // with lower confidence (0.7) for those not in the standard list
            for cap in RE_CUSTOM_FILTER.captures_iter(content) {
                let filter_name = &cap[1];

                // Skip if already detected as a standard filter
                if standard_filters.contains(filter_name) || seen_filters.contains(filter_name) {
                    continue;
                }

                let filter_id = format!("Filter:{}", filter_name);

                if graph.get_node(&filter_id).is_none() {
                    graph.add_node(GraphNode {
                        id: filter_id.clone(),
                        label: NodeLabel::Filter,
                        properties: NodeProperties {
                            name: filter_name.to_string(),
                            file_path: String::new(),
                            description: Some("custom_filter".to_string()),
                            ..Default::default()
                        },
                    });
                }

                let ctrl_id = format!("Controller:{}:{}", file_path, ctrl.class_name);
                graph.add_relationship(GraphRelationship {
                    id: format!("filter:{}:{}:{}", file_path, ctrl.class_name, filter_name),
                    source_id: ctrl_id,
                    target_id: filter_id,
                    rel_type: RelationshipType::HasFilter,
                    confidence: 0.7,
                    reason: "custom_attribute".to_string(),
                    step: None,
                });

                seen_filters.insert(filter_name.to_string());
                stats.filters += 1;
            }
        }
    }

    // ──────────────────────────────────────────────────────────────────────
    // Pass 5c: Base controller inheritance tracking
    // ──────────────────────────────────────────────────────────────────────
    for (file_path, ctrl) in &all_controllers {
        if let Some(base_name) = &ctrl.base_controller {
            let ctrl_id = format!("Controller:{}:{}", file_path, ctrl.class_name);

            // Create or reference the base controller node
            let base_ctrl_id = format!("Controller:*:{}", base_name);

            // Check if the base controller already exists as a known controller
            let resolved_base_id = all_controllers
                .iter()
                .find(|(_, c)| c.class_name == *base_name)
                .map(|(fp, c)| format!("Controller:{}:{}", fp, c.class_name))
                .unwrap_or_else(|| {
                    // Create a placeholder node for the base controller if not found
                    if graph.get_node(&base_ctrl_id).is_none() {
                        graph.add_node(GraphNode {
                            id: base_ctrl_id.clone(),
                            label: NodeLabel::Controller,
                            properties: NodeProperties {
                                name: base_name.clone(),
                                file_path: String::new(),
                                description: Some("base_controller".to_string()),
                                ..Default::default()
                            },
                        });
                    }
                    base_ctrl_id.clone()
                });

            // Scope edge ID by source file_path so two derived controllers
            // with the same class name in different areas/folders don't
            // collide and silently overwrite each other.
            graph.add_relationship(GraphRelationship {
                id: format!("inherits:{}:{}:{}", file_path, ctrl.class_name, base_name),
                source_id: ctrl_id,
                target_id: resolved_base_id,
                rel_type: RelationshipType::Inherits,
                confidence: 1.0,
                reason: format!("extends:{}", base_name),
                step: None,
            });
        }
    }

    // ──────────────────────────────────────────────────────────────────────
    // Pass 6: Process Razor views (.cshtml)
    // ──────────────────────────────────────────────────────────────────────
    let view_files: Vec<&FileEntry> = file_entries
        .iter()
        .filter(|f| f.path.ends_with(".cshtml"))
        .collect();

    for entry in &view_files {
        let view_info = csharp::extract_view_info(&entry.path, &entry.content);
        let view_id = format!("View:{}", entry.path);

        graph.add_node(GraphNode {
            id: view_id.clone(),
            label: NodeLabel::View,
            properties: NodeProperties {
                name: entry
                    .path
                    .rsplit('/')
                    .next()
                    .unwrap_or(&entry.path)
                    .to_string(),
                file_path: entry.path.clone(),
                model_type: view_info.model_type.clone(),
                layout_path: view_info.layout_path,
                area_name: view_info.area_name.clone(),
                view_engine: Some(if view_info.is_partial {
                    "partial".to_string()
                } else {
                    "razor".to_string()
                }),
                ..Default::default()
            },
        });

        // Try to link view to controller via convention:
        // Views/<ControllerName>/<ActionName>.cshtml → Controller:*:<ControllerName>Controller
        if let Some((ctrl_name, _action_name)) = infer_controller_from_view_path(&entry.path) {
            let ctrl_class = format!("{}Controller", ctrl_name);
            // Find matching controller
            for (_, ctrl) in &all_controllers {
                if ctrl.class_name == ctrl_class {
                    let ctrl_id = all_controllers
                        .iter()
                        .find(|(_, c)| c.class_name == ctrl_class)
                        .map(|(fp, c)| format!("Controller:{}:{}", fp, c.class_name));

                    if let Some(ctrl_id) = ctrl_id {
                        graph.add_relationship(GraphRelationship {
                            id: format!("renders:{}", entry.path),
                            source_id: ctrl_id,
                            target_id: view_id.clone(),
                            rel_type: RelationshipType::RendersView,
                            confidence: 0.85,
                            reason: "convention_based".to_string(),
                            step: None,
                        });
                    }
                    break;
                }
            }
        }

        // Link to Area if in Areas/ directory
        if let Some(area_name) = &view_info.area_name {
            if let Some(area_id) = area_nodes.get(area_name) {
                graph.add_relationship(GraphRelationship {
                    id: format!("viewarea:{}", entry.path),
                    source_id: view_id.clone(),
                    target_id: area_id.clone(),
                    rel_type: RelationshipType::BelongsToArea,
                    confidence: 1.0,
                    reason: "path_convention".to_string(),
                    step: None,
                });
            }
        }

        stats.views += 1;
    }

    // ──────────────────────────────────────────────────────────────────────
    // Pass 6b: Detect @Html.Partial / @Html.RenderPartial calls in views
    //
    // This pass exists to create `PartialView:{name}` nodes that Pass 13 and
    // other passes reference by ID without materialising the node themselves.
    // `@Html.RenderAction(...)` used to be matched here too, but RenderAction
    // invokes a *controller action*, not a partial view — its first argument
    // is the action name, not a view name. Routing it through this pass
    // created a spurious `PartialView:{action_name}` node and a semantically
    // wrong `UsesPartial` edge. Pass 13 (below) already handles RenderAction
    // correctly via `extract_partial_references`, emitting a real
    // `CallsAction` edge, so we must not shadow it here.
    // ──────────────────────────────────────────────────────────────────────
    let partial_regex = Regex::new(r#"@?\s*Html\.(Partial|RenderPartial)\("([^"]+)""#)
        .expect("regex pattern must compile");

    for entry in &view_files {
        let view_id = format!("View:{}", entry.path);

        let mut seen_partials: HashSet<String> = HashSet::new();

        for cap in partial_regex.captures_iter(&entry.content) {
            let method = &cap[1];
            let partial_name = &cap[2];

            if !seen_partials.contains(partial_name) {
                let partial_view_id = format!("PartialView:{}", partial_name);

                if graph.get_node(&partial_view_id).is_none() {
                    graph.add_node(GraphNode {
                        id: partial_view_id.clone(),
                        label: NodeLabel::PartialView,
                        properties: NodeProperties {
                            name: partial_name.to_string(),
                            file_path: String::new(),
                            view_engine: Some("partial".to_string()),
                            ..Default::default()
                        },
                    });
                }

                graph.add_relationship(GraphRelationship {
                    id: format!("partial:{}:{}", entry.path, partial_name),
                    source_id: view_id.clone(),
                    target_id: partial_view_id,
                    rel_type: RelationshipType::UsesPartial,
                    confidence: 0.95,
                    reason: method.to_string(),
                    step: None,
                });

                seen_partials.insert(partial_name.to_string());
                stats.partial_views += 1;
            }
        }
    }

    // ──────────────────────────────────────────────────────────────────────
    // Pass 7: Enrich from .edmx models (associations with cardinality)
    // ──────────────────────────────────────────────────────────────────────
    for model in &edmx_models {
        for assoc in &model.associations {
            let entity1 = edmx::clean_entity_type_name(&assoc.end1.entity_type);
            let entity2 = edmx::clean_entity_type_name(&assoc.end2.entity_type);

            let source_id = entity_file_map
                .get(entity1)
                .cloned()
                .unwrap_or_else(|| format!("DbEntity:*:{}", entity1));
            let target_id = entity_file_map
                .get(entity2)
                .cloned()
                .unwrap_or_else(|| format!("DbEntity:*:{}", entity2));

            let cardinality = edmx::cardinality_str(assoc);

            graph.add_relationship(GraphRelationship {
                id: format!("edmx_assoc:{}", assoc.name),
                source_id,
                target_id,
                rel_type: RelationshipType::AssociatesWith,
                confidence: 1.0,
                reason: format!("edmx:{}", cardinality),
                step: None,
            });
        }

        // Pass 7b: Create Inherits relationships from EDMX BaseType attributes
        for et in &model.entity_types {
            if let Some(base_type_ref) = &et.base_type {
                let child_name = &et.name;
                let parent_name = edmx::clean_entity_type_name(base_type_ref);

                let child_id = entity_file_map
                    .get(child_name)
                    .cloned()
                    .unwrap_or_else(|| format!("DbEntity:*:{}", child_name));
                let parent_id = entity_file_map
                    .get(parent_name)
                    .cloned()
                    .unwrap_or_else(|| format!("DbEntity:*:{}", parent_name));

                graph.add_relationship(GraphRelationship {
                    id: format!("edmx_inherits:{}:{}", child_name, parent_name),
                    source_id: child_id,
                    target_id: parent_id,
                    rel_type: RelationshipType::Inherits,
                    confidence: 1.0,
                    reason: format!("edmx_base_type:{}", base_type_ref),
                    step: None,
                });
            }
        }
    }

    // ──────────────────────────────────────────────────────────────────────
    // Pass 8: jQuery/AJAX → Controller Action mapping
    // ──────────────────────────────────────────────────────────────────────

    // Regex for extracting <script src="..."> references
    static RE_SCRIPT_SRC: Lazy<Regex> =
        Lazy::new(|| Regex::new(r#"(?i)<script[^>]+src\s*=\s*["']([^"']+)["']"#).unwrap());

    // 8a: Process standalone .js files
    let js_files: Vec<&FileEntry> = file_entries
        .iter()
        .filter(|f| f.path.ends_with(".js"))
        .collect();

    for entry in &js_files {
        let script_id = format!("ScriptFile:{}", entry.path);

        graph.add_node(GraphNode {
            id: script_id.clone(),
            label: NodeLabel::ScriptFile,
            properties: NodeProperties {
                name: entry
                    .path
                    .rsplit('/')
                    .next()
                    .unwrap_or(&entry.path)
                    .to_string(),
                file_path: entry.path.clone(),
                ..Default::default()
            },
        });

        stats.script_files += 1;

        let ajax_calls = extract_ajax_calls(&entry.content);
        for call in &ajax_calls {
            let ajax_id = format!(
                "AjaxCall:{}:{}:{}",
                entry.path, call.line_number, call.url_pattern
            );

            graph.add_node(GraphNode {
                id: ajax_id.clone(),
                label: NodeLabel::AjaxCall,
                properties: NodeProperties {
                    name: format!(
                        "{} {}",
                        call.method,
                        call.action_name.as_deref().unwrap_or(&call.url_pattern)
                    ),
                    file_path: entry.path.clone(),
                    start_line: Some(call.line_number),
                    http_method: Some(call.method.clone()),
                    ajax_url: Some(call.url_pattern.clone()),
                    ajax_method: Some(call.method.clone()),
                    ..Default::default()
                },
            });

            // Link ScriptFile → AjaxCall
            graph.add_relationship(GraphRelationship {
                id: format!("script_ajax:{}:{}", entry.path, call.line_number),
                source_id: script_id.clone(),
                target_id: ajax_id.clone(),
                rel_type: RelationshipType::CallsAction,
                confidence: 0.7,
                reason: "script_ajax_call".to_string(),
                step: None,
            });

            // Try to link AjaxCall → ControllerAction
            if let (Some(ctrl_name), Some(action_name)) = (&call.controller_name, &call.action_name)
            {
                if let Some((action_node_id, confidence)) =
                    resolve_action_node_id(&all_controllers, ctrl_name, action_name)
                {
                    graph.add_relationship(GraphRelationship {
                        id: format!("ajax_action:{}:{}:{}", entry.path, ctrl_name, action_name),
                        source_id: ajax_id.clone(),
                        target_id: action_node_id,
                        rel_type: RelationshipType::CallsAction,
                        confidence,
                        reason: format!("ajax_url:{}", call.url_pattern),
                        step: None,
                    });
                }
            }

            stats.ajax_calls += 1;
        }
    }

    // Regex for @Url.Action("Action", "Controller") patterns in Razor views
    static RE_URL_ACTION: Lazy<Regex> = Lazy::new(|| {
        Regex::new(r#"Url\.Action\s*\(\s*"(\w+)"\s*,\s*"(\w+)""#)
            .expect("RE_URL_ACTION regex must compile")
    });

    // 8b: Process .cshtml view files — scan FULL content (not just <script> blocks)
    // This catches AJAX calls in inline event handlers, Razor blocks, etc.
    for entry in &view_files {
        let view_id = format!("View:{}", entry.path);

        // Scan the full file content for AJAX calls
        let ajax_calls = extract_ajax_calls(&entry.content);
        let mut seen_lines: HashSet<u32> = HashSet::new();

        for call in &ajax_calls {
            // Deduplicate by line number
            if !seen_lines.insert(call.line_number) {
                continue;
            }

            let ajax_id = format!(
                "AjaxCall:{}:inline:{}:{}",
                entry.path, call.line_number, call.url_pattern
            );

            graph.add_node(GraphNode {
                id: ajax_id.clone(),
                label: NodeLabel::AjaxCall,
                properties: NodeProperties {
                    name: format!(
                        "{} {}",
                        call.method,
                        call.action_name.as_deref().unwrap_or(&call.url_pattern)
                    ),
                    file_path: entry.path.clone(),
                    start_line: Some(call.line_number),
                    http_method: Some(call.method.clone()),
                    ajax_url: Some(call.url_pattern.clone()),
                    ajax_method: Some(call.method.clone()),
                    ..Default::default()
                },
            });

            // Link View → AjaxCall
            graph.add_relationship(GraphRelationship {
                id: format!(
                    "view_ajax:{}:{}:{}",
                    entry.path, call.line_number, call.url_pattern
                ),
                source_id: view_id.clone(),
                target_id: ajax_id.clone(),
                rel_type: RelationshipType::CallsAction,
                confidence: 0.8,
                reason: "inline_script_ajax".to_string(),
                step: None,
            });

            // Try to link AjaxCall → ControllerAction
            if let (Some(ctrl_name), Some(action_name)) = (&call.controller_name, &call.action_name)
            {
                if let Some((action_node_id, confidence)) =
                    resolve_action_node_id(&all_controllers, ctrl_name, action_name)
                {
                    graph.add_relationship(GraphRelationship {
                        id: format!("ajax_action:{}:{}:{}", entry.path, ctrl_name, action_name),
                        source_id: ajax_id.clone(),
                        target_id: action_node_id,
                        rel_type: RelationshipType::CallsAction,
                        confidence,
                        reason: format!("inline_ajax_url:{}", call.url_pattern),
                        step: None,
                    });
                }
            }

            stats.ajax_calls += 1;
        }

        // 8b-ii: Detect @Url.Action("Action", "Controller") in Razor views
        for cap in RE_URL_ACTION.captures_iter(&entry.content) {
            let action_name = &cap[1];
            let controller_name = &cap[2];

            if let Some((action_node_id, confidence)) =
                resolve_action_node_id(&all_controllers, controller_name, action_name)
            {
                graph.add_relationship(GraphRelationship {
                    id: format!(
                        "url_action:{}:{}:{}",
                        entry.path, controller_name, action_name
                    ),
                    source_id: view_id.clone(),
                    target_id: action_node_id,
                    rel_type: RelationshipType::CallsAction,
                    confidence: confidence * 0.95, // slightly lower than direct AJAX
                    reason: format!("Url.Action(\"{}\", \"{}\")", action_name, controller_name),
                    step: None,
                });
            }
        }

        // 8c: Scan for <script src="..."> references → IncludesScript relationship
        for src_cap in RE_SCRIPT_SRC.captures_iter(&entry.content) {
            let script_src = src_cap.get(1).map(|m| m.as_str()).unwrap_or("");

            // Try to resolve the src path to a known ScriptFile node
            let script_node_id = js_files
                .iter()
                .find(|f| {
                    // Match on filename or relative path suffix
                    f.path
                        .ends_with(script_src.trim_start_matches('~').trim_start_matches('/'))
                        || f.path
                            .rsplit('/')
                            .next()
                            .is_some_and(|name| script_src.ends_with(name))
                })
                .map(|f| format!("ScriptFile:{}", f.path));

            let target_id =
                script_node_id.unwrap_or_else(|| format!("ScriptFile:external:{}", script_src));

            graph.add_relationship(GraphRelationship {
                id: format!("includes_script:{}:{}", entry.path, script_src),
                source_id: view_id.clone(),
                target_id,
                rel_type: RelationshipType::IncludesScript,
                confidence: 0.9,
                reason: format!("script_src:{}", script_src),
                step: None,
            });
        }
    }

    // ──────────────────────────────────────────────────────────────────────
    // Pass 9: Telerik/Kendo UI components
    // ──────────────────────────────────────────────────────────────────────
    run_pass_9_telerik_components(graph, &mut stats, &view_files, &js_files, &all_controllers);

    // ──────────────────────────────────────────────────────────────────────
    // Pass 10: Service/Repository layer detection
    // ──────────────────────────────────────────────────────────────────────
    run_pass_10_services(graph, &mut stats, &cs_files, file_entries, &all_controllers);

    // ──────────────────────────────────────────────────────────────────────
    // Pass 11: StackLogger tracing coverage detection
    // ──────────────────────────────────────────────────────────────────────
    run_pass_11_tracing(graph, &mut stats, &cs_files);

    // ──────────────────────────────────────────────────────────────────────
    // Pass 12: External service calls (WebAPI + WCF)
    // ──────────────────────────────────────────────────────────────────────

    let mut service_nodes: HashMap<String, String> = HashMap::new(); // client_class → node ID

    for entry in &cs_files {
        let ext_calls = extract_external_service_calls(&entry.content);

        for call in &ext_calls {
            // Create or reuse ExternalService node
            let svc_id = format!("ExternalService:{}", call.client_class);

            if !service_nodes.contains_key(&call.client_class) {
                graph.add_node(GraphNode {
                    id: svc_id.clone(),
                    label: NodeLabel::ExternalService,
                    properties: NodeProperties {
                        name: call.client_class.clone(),
                        file_path: String::new(),
                        service_type: Some(call.service_type.clone()),
                        ..Default::default()
                    },
                });
                service_nodes.insert(call.client_class.clone(), svc_id.clone());
                stats.external_services += 1;
            }

            let target_id = service_nodes
                .get(&call.client_class)
                .cloned()
                .unwrap_or(svc_id);

            // Create CallsService relationship from the file to the service
            let source_id = format!("File:{}", entry.path);
            let rel_id = format!(
                "calls_svc:{}:{}:{}",
                entry.path, call.client_class, call.line_number
            );

            graph.add_relationship(GraphRelationship {
                id: rel_id,
                source_id,
                target_id,
                rel_type: RelationshipType::CallsService,
                confidence: 0.9,
                reason: format!(
                    "{}:{}",
                    call.service_type,
                    call.method_name.as_deref().unwrap_or("instantiation")
                ),
                step: None,
            });
        }
    }

    // ─── Pass 13: @Html.Partial / @Html.RenderPartial / @Html.Action references ───
    let mut partial_refs_count: usize = 0;
    for entry in &view_files {
        let refs = extract_partial_references(&entry.content);
        let view_id = format!("View:{}", entry.path);
        for pref in &refs {
            let is_partial_helper =
                pref.helper_type == "Partial" || pref.helper_type == "RenderPartial";
            // Resolve the target up front so we can skip the edge when
            // there is nothing real to point at — previously the
            // Action/RenderAction branch constructed a wildcard target
            // `ControllerAction:*:{ctrl}Controller:{action}` that never
            // matched the real node IDs (`ControllerAction:{file_path}:
            // {class_name}:{action_name}`) because the `*` segment stands
            // in for `file_path`, so every `@Html.Action` reference
            // produced a dangling edge. Use `resolve_action_node_id`,
            // mirroring Pass 8b/14, so we either create a real edge or
            // skip the reference entirely.
            let (target_id, confidence, rel_type) = if is_partial_helper {
                // Link to partial view node — Pass 6b creates these as
                // `PartialView:{partial_name}` (no source-path prefix), so
                // we must match that exact format or every edge here would
                // dangle and silently overwrite Pass 6b's UsesPartial edges.
                (
                    format!("PartialView:{}", pref.partial_name),
                    0.85,
                    RelationshipType::UsesPartial,
                )
            } else if let Some(ctrl) = &pref.controller_name {
                // Html.Action/RenderAction → controller action
                match resolve_action_node_id(&all_controllers, ctrl, &pref.partial_name) {
                    Some((action_id, conf)) => (action_id, conf, RelationshipType::CallsAction),
                    None => continue,
                }
            } else {
                continue;
            };
            graph.add_relationship(GraphRelationship {
                id: format!(
                    "partial_ref:{}:{}:{}",
                    entry.path, pref.partial_name, pref.line_number
                ),
                source_id: view_id.clone(),
                target_id,
                rel_type,
                confidence,
                reason: format!("razor_{}", pref.helper_type.to_lowercase()),
                step: None,
            });
            partial_refs_count += 1;
        }
    }
    tracing::info!(
        partial_refs = partial_refs_count,
        "Pass 13: Partial/RenderAction references"
    );

    // ─── Pass 14: @Html.BeginForm → Controller action links ─────────────
    let mut form_refs_count: usize = 0;
    for entry in &view_files {
        let forms = extract_form_actions(&entry.content);
        let view_id = format!("View:{}", entry.path);
        for form in &forms {
            let target_id =
                resolve_action_node_id(&all_controllers, &form.controller_name, &form.action_name);
            if let Some((action_id, confidence)) = target_id {
                graph.add_relationship(GraphRelationship {
                    id: format!(
                        "form_action:{}:{}:{}",
                        entry.path, form.action_name, form.line_number
                    ),
                    source_id: view_id.clone(),
                    target_id: action_id,
                    rel_type: RelationshipType::CallsAction,
                    confidence,
                    reason: format!("html_beginform_{}", form.http_method.to_lowercase()),
                    step: None,
                });
                form_refs_count += 1;
            }
        }
    }
    tracing::info!(
        form_refs = form_refs_count,
        "Pass 14: Html.BeginForm references"
    );

    tracing::info!(
        controllers = stats.controllers,
        actions = stats.actions,
        api_endpoints = stats.api_endpoints,
        views = stats.views,
        db_contexts = stats.db_contexts,
        db_entities = stats.db_entities,
        areas = stats.areas,
        edmx_models = stats.edmx_models,
        filters = stats.filters,
        webconfigs = stats.webconfigs,
        partial_views = stats.partial_views,
        ajax_calls = stats.ajax_calls,
        script_files = stats.script_files,
        ui_components = stats.ui_components,
        services = stats.services,
        repositories = stats.repositories,
        traced_files = stats.traced_files,
        trace_calls = stats.trace_calls,
        external_services = stats.external_services,
        "ASP.NET MVC enrichment complete"
    );

    Ok(stats)
}

// ─── Extracted Pass Functions ────────────────────────────────────────────

/// Pass 9: Telerik/Kendo UI component extraction from views and JS files.
fn run_pass_9_telerik_components(
    graph: &mut KnowledgeGraph,
    stats: &mut AspNetMvcStats,
    view_files: &[&FileEntry],
    js_files: &[&FileEntry],
    all_controllers: &[(String, ControllerInfo)],
) {
    // 9a: Extract from .cshtml view files
    for entry in view_files {
        let view_id = format!("View:{}", entry.path);
        let components = extract_telerik_components(&entry.content);

        for comp in &components {
            let comp_id = format!(
                "UiComponent:{}:{}:{}",
                entry.path, comp.component_type, comp.line_number
            );

            let col_desc = if comp.columns.is_empty() {
                None
            } else {
                Some(
                    comp.columns
                        .iter()
                        .map(|c| {
                            let title = c.title.as_deref().unwrap_or(&c.property_name);
                            if c.has_client_template {
                                format!("{}*", title)
                            } else {
                                title.to_string()
                            }
                        })
                        .collect::<Vec<_>>()
                        .join(", "),
                )
            };

            graph.add_node(GraphNode {
                id: comp_id.clone(),
                label: NodeLabel::UiComponent,
                properties: NodeProperties {
                    name: format!("{}.{}", comp.vendor, comp.component_type),
                    file_path: entry.path.clone(),
                    start_line: Some(comp.line_number),
                    component_type: Some(format!("{}.{}", comp.vendor, comp.component_type)),
                    bound_model: comp.model_type.clone(),
                    description: col_desc,
                    ..Default::default()
                },
            });

            graph.add_relationship(GraphRelationship {
                id: format!(
                    "renders_comp:{}:{}:{}",
                    entry.path, comp.component_type, comp.line_number
                ),
                source_id: view_id.clone(),
                target_id: comp_id.clone(),
                rel_type: RelationshipType::RendersComponent,
                confidence: 1.0,
                reason: format!("{}_{}", comp.vendor.to_lowercase(), comp.component_type),
                step: None,
            });

            for ds_action in &comp.data_source_actions {
                if let Some((action_node_id, confidence)) = resolve_action_node_id(
                    all_controllers,
                    &ds_action.controller_name,
                    &ds_action.action_name,
                ) {
                    graph.add_relationship(GraphRelationship {
                        id: format!(
                            "ds_action:{}:{}:{}:{}",
                            entry.path,
                            comp.component_type,
                            ds_action.controller_name,
                            ds_action.action_name
                        ),
                        source_id: comp_id.clone(),
                        target_id: action_node_id,
                        rel_type: RelationshipType::CallsAction,
                        confidence,
                        reason: format!("datasource_{}", ds_action.operation.to_lowercase()),
                        step: None,
                    });
                }
            }

            stats.ui_components += 1;
        }
    }

    // 9b: Extract Kendo jQuery widgets from .js files
    for entry in js_files {
        let script_id = format!("ScriptFile:{}", entry.path);
        let components = extract_telerik_components(&entry.content);

        for comp in &components {
            let comp_id = format!(
                "UiComponent:{}:{}:{}",
                entry.path, comp.component_type, comp.line_number
            );

            graph.add_node(GraphNode {
                id: comp_id.clone(),
                label: NodeLabel::UiComponent,
                properties: NodeProperties {
                    name: format!("{}.{}", comp.vendor, comp.component_type),
                    file_path: entry.path.clone(),
                    start_line: Some(comp.line_number),
                    component_type: Some(format!("{}.{}", comp.vendor, comp.component_type)),
                    bound_model: comp.model_type.clone(),
                    ..Default::default()
                },
            });

            graph.add_relationship(GraphRelationship {
                id: format!(
                    "renders_comp:{}:{}:{}",
                    entry.path, comp.component_type, comp.line_number
                ),
                source_id: script_id.clone(),
                target_id: comp_id.clone(),
                rel_type: RelationshipType::RendersComponent,
                confidence: 1.0,
                reason: format!("jquery_{}", comp.component_type.to_lowercase()),
                step: None,
            });

            for ds_action in &comp.data_source_actions {
                if let Some((action_node_id, confidence)) = resolve_action_node_id(
                    all_controllers,
                    &ds_action.controller_name,
                    &ds_action.action_name,
                ) {
                    graph.add_relationship(GraphRelationship {
                        id: format!(
                            "ds_action:{}:{}:{}:{}",
                            entry.path,
                            comp.component_type,
                            ds_action.controller_name,
                            ds_action.action_name
                        ),
                        source_id: comp_id.clone(),
                        target_id: action_node_id,
                        rel_type: RelationshipType::CallsAction,
                        confidence,
                        reason: format!("datasource_{}", ds_action.operation.to_lowercase()),
                        step: None,
                    });
                }
            }

            stats.ui_components += 1;
        }
    }
}

/// Pass 10: Service/Repository layer detection and constructor dependency injection.
fn run_pass_10_services(
    graph: &mut KnowledgeGraph,
    stats: &mut AspNetMvcStats,
    cs_files: &[&FileEntry],
    file_entries: &[FileEntry],
    all_controllers: &[(String, ControllerInfo)],
) {
    let mut service_map: HashMap<String, String> = HashMap::new();
    let mut interface_to_service: HashMap<String, String> = HashMap::new();

    // 10a: Extract services and repositories from all .cs files
    for entry in cs_files {
        let services = extract_services_and_repositories(&entry.content);

        for svc in &services {
            let label = if svc.layer_type == "Repository" {
                NodeLabel::Repository
            } else {
                NodeLabel::Service
            };

            let svc_id = format!("{}:{}:{}", svc.layer_type, entry.path, svc.class_name);

            graph.add_node(GraphNode {
                id: svc_id.clone(),
                label,
                properties: NodeProperties {
                    name: svc.class_name.clone(),
                    file_path: entry.path.clone(),
                    layer_type: Some(svc.layer_type.clone()),
                    implements_interface: svc.implements_interface.clone(),
                    ..Default::default()
                },
            });

            service_map.insert(svc.class_name.clone(), svc_id.clone());

            if let Some(iface_name) = &svc.implements_interface {
                let iface_id = format!("Interface:{}:{}", entry.path, iface_name);

                if graph.get_node(&iface_id).is_none() {
                    graph.add_node(GraphNode {
                        id: iface_id.clone(),
                        label: NodeLabel::Interface,
                        properties: NodeProperties {
                            name: iface_name.clone(),
                            file_path: entry.path.clone(),
                            ..Default::default()
                        },
                    });
                }

                graph.add_relationship(GraphRelationship {
                    // Scope by file_path so two different files defining
                    // services with the same class name (e.g., area-scoped
                    // duplicates) don't collide on the same edge ID.
                    id: format!(
                        "implements:{}:{}:{}",
                        entry.path, svc.class_name, iface_name
                    ),
                    source_id: svc_id.clone(),
                    target_id: iface_id.clone(),
                    rel_type: RelationshipType::Implements,
                    confidence: 1.0,
                    reason: "class_implements".to_string(),
                    step: None,
                });

                interface_to_service.insert(iface_name.clone(), svc_id.clone());
            }

            if svc.layer_type == "Repository" {
                stats.repositories += 1;
            } else {
                stats.services += 1;
            }
        }
    }

    // 10b: Detect constructor dependencies in controllers → DependsOn services/repositories
    for (file_path, ctrl) in all_controllers {
        let ctrl_id = format!("Controller:{}:{}", file_path, ctrl.class_name);

        let content = file_entries
            .iter()
            .find(|f| f.path == *file_path)
            .map(|f| f.content.as_str())
            .unwrap_or("");
        let deps = extract_constructor_dependencies(content, &ctrl.class_name);

        for (iface_type, _param_name) in &deps {
            let target_id = interface_to_service.get(iface_type).cloned().or_else(|| {
                let concrete_name = iface_type.strip_prefix('I').unwrap_or(iface_type);
                service_map.get(concrete_name).cloned()
            });

            if let Some(target_id) = target_id {
                graph.add_relationship(GraphRelationship {
                    // Include the controller's file_path so area-scoped
                    // controllers (Areas/Admin/HomeController vs
                    // Controllers/HomeController) don't collide on the
                    // same `depends:HomeController:IFoo:...` edge ID.
                    id: format!(
                        "depends:{}:{}:{}:{}",
                        file_path, ctrl.class_name, iface_type, target_id
                    ),
                    source_id: ctrl_id.clone(),
                    target_id,
                    rel_type: RelationshipType::DependsOn,
                    confidence: 0.95,
                    reason: format!("constructor_injection:{}", iface_type),
                    step: None,
                });
            }
        }
    }
}

/// Pass 11: StackLogger tracing coverage detection and propagation.
fn run_pass_11_tracing(
    graph: &mut KnowledgeGraph,
    stats: &mut AspNetMvcStats,
    cs_files: &[&FileEntry],
) {
    let mut total_cs_files: usize = 0;

    for entry in cs_files {
        total_cs_files += 1;
        let tracing_info = extract_tracing_info(&entry.content);

        if tracing_info.is_traced {
            stats.traced_files += 1;
            stats.trace_calls += tracing_info.call_count as usize;

            let file_node_id = format!("File:{}", entry.path);
            if let Some(node) = graph.get_node_mut(&file_node_id) {
                node.properties.is_traced = Some(true);
                node.properties.trace_call_count = Some(tracing_info.call_count);
            }

            for method_name in &tracing_info.traced_methods {
                let method_id = code_explorer_core::id::generate_id(
                    "Method",
                    &format!("{}:{}", entry.path, method_name),
                );
                if let Some(method_node) = graph.get_node_mut(&method_id) {
                    method_node.properties.is_traced = Some(true);
                }
                let ctor_id = code_explorer_core::id::generate_id(
                    "Constructor",
                    &format!("{}:{}", entry.path, method_name),
                );
                if let Some(ctor_node) = graph.get_node_mut(&ctor_id) {
                    ctor_node.properties.is_traced = Some(true);
                }
            }
        }
    }

    if total_cs_files > 0 && stats.traced_files > 0 {
        let coverage_pct = (stats.traced_files as f64 / total_cs_files as f64) * 100.0;
        tracing::info!(
            traced_files = stats.traced_files,
            total_files = total_cs_files,
            trace_calls = stats.trace_calls,
            coverage_pct = format!("{:.1}%", coverage_pct),
            "StackLogger tracing coverage"
        );
    }
}

// ─── Helpers ─────────────────────────────────────────────────────────────

/// Infer area name from file path: Areas/<Name>/Controllers/...
///
/// Splits on path separators and case-insensitively matches the "Areas" segment.
/// Operates on segments rather than byte offsets to avoid panics on paths
/// containing multi-byte UTF-8 characters before the area marker.
fn infer_area_from_path(path: &str) -> Option<String> {
    let normalized = path.replace('\\', "/");
    let segments: Vec<&str> = normalized.split('/').collect();
    for (i, seg) in segments.iter().enumerate() {
        if seg.eq_ignore_ascii_case("areas") && i + 1 < segments.len() {
            return Some(segments[i + 1].to_string());
        }
    }
    None
}

/// Infer controller and action names from view path.
/// Views/<Controller>/<Action>.cshtml → (Controller, Action)
fn infer_controller_from_view_path(path: &str) -> Option<(String, String)> {
    let normalized = path.replace('\\', "/");
    let segments: Vec<&str> = normalized.split('/').collect();

    // Look for "Views" directory
    for (i, seg) in segments.iter().enumerate() {
        if seg.eq_ignore_ascii_case("Views") && i + 2 < segments.len() {
            let controller = segments[i + 1].to_string();
            let action = segments[i + 2]
                .strip_suffix(".cshtml")
                .unwrap_or(segments[i + 2])
                .to_string();

            // Skip shared views
            if controller.eq_ignore_ascii_case("Shared") {
                return None;
            }

            return Some((controller, action));
        }
    }
    None
}

/// Resolve a controller/action pair to the corresponding graph node ID.
///
/// Searches `all_controllers` for a class whose name matches `controller_name`
/// (with or without the "Controller" suffix) and an action whose name matches
/// `action_name`. Returns `(node_id, confidence)`.
fn resolve_action_node_id(
    all_controllers: &[(String, ControllerInfo)],
    controller_name: &str,
    action_name: &str,
) -> Option<(String, f64)> {
    // Normalize: accept both "Products" and "ProductsController"
    let with_suffix = if controller_name.ends_with("Controller") {
        controller_name.to_string()
    } else {
        format!("{}Controller", controller_name)
    };
    let without_suffix = with_suffix
        .strip_suffix("Controller")
        .unwrap_or(&with_suffix)
        .to_string();

    for (file_path, ctrl) in all_controllers {
        let ctrl_base = ctrl
            .class_name
            .strip_suffix("Controller")
            .unwrap_or(&ctrl.class_name);

        let name_matches = ctrl.class_name.eq_ignore_ascii_case(&with_suffix)
            || ctrl_base.eq_ignore_ascii_case(&without_suffix);

        if !name_matches {
            continue;
        }

        for action in &ctrl.actions {
            if action.name.eq_ignore_ascii_case(action_name) {
                let prefix = if ctrl.is_api_controller {
                    "ApiEndpoint"
                } else {
                    "ControllerAction"
                };
                let node_id = format!(
                    "{}:{}:{}:{}",
                    prefix, file_path, ctrl.class_name, action.name
                );

                // Exact match (case matches) → 0.95; case-insensitive match → 0.85
                let confidence = if ctrl_base == without_suffix && action.name == action_name {
                    0.95
                } else {
                    0.85
                };

                return Some((node_id, confidence));
            }
        }
    }

    None
}

/// Build full route template by combining controller prefix + action route.
fn build_full_route(
    prefix: Option<&str>,
    action_route: Option<&str>,
    controller_name: &str,
    action_name: &str,
) -> String {
    match (prefix, action_route) {
        (Some(p), Some(a)) => format!("{}/{}", p.trim_end_matches('/'), a.trim_start_matches('/')),
        (Some(p), None) => format!("{}/{}", p.trim_end_matches('/'), action_name),
        (None, Some(a)) => a.to_string(),
        (None, None) => {
            // Convention-based: /{Controller}/{Action}
            let ctrl = controller_name
                .strip_suffix("Controller")
                .unwrap_or(controller_name);
            format!("{}/{}", ctrl, action_name)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_infer_area_from_path() {
        assert_eq!(
            infer_area_from_path("Areas/Admin/Controllers/ProductsController.cs"),
            Some("Admin".to_string())
        );
        assert_eq!(
            infer_area_from_path("src/Controllers/HomeController.cs"),
            None
        );
    }

    #[test]
    fn test_infer_controller_from_view_path() {
        let (ctrl, action) =
            infer_controller_from_view_path("Views/Products/Index.cshtml").unwrap();
        assert_eq!(ctrl, "Products");
        assert_eq!(action, "Index");

        assert!(infer_controller_from_view_path("Views/Shared/_Layout.cshtml").is_none());
    }

    #[test]
    fn test_build_full_route() {
        assert_eq!(
            build_full_route(
                Some("api/products"),
                Some("{id}"),
                "ProductsController",
                "Get"
            ),
            "api/products/{id}"
        );
        assert_eq!(
            build_full_route(Some("api/products"), None, "ProductsController", "GetAll"),
            "api/products/GetAll"
        );
        assert_eq!(
            build_full_route(None, None, "ProductsController", "Index"),
            "Products/Index"
        );
    }
}

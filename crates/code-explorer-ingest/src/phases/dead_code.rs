//! Dead code detection phase: marks methods with no incoming Calls edges.

use std::collections::HashSet;

use code_explorer_core::graph::types::{NodeLabel, RelationshipType};
use code_explorer_core::graph::KnowledgeGraph;

/// Mark methods with no incoming Calls edges as dead code candidates.
///
/// Excluded from dead code detection:
/// - Entry points: ControllerAction, ApiEndpoint, ASP.NET lifecycle methods
/// - Constructors (typically instantiated via DI/reflection)
/// - Test methods (invoked by test frameworks, not by application code)
/// - Interface method declarations (abstract, no implementation)
/// - JS functions in views/scripts (event handlers bound in markup)
/// - Methods that correspond to a ControllerAction (dual-node pattern)
/// - Code in compiled output directories (obj/, bin/)
pub fn mark_dead_code(graph: &mut KnowledgeGraph) {
    // Build set of method IDs that have at least one incoming Calls edge
    let mut has_incoming_call: HashSet<String> = HashSet::new();
    for rel in graph.iter_relationships() {
        if matches!(
            rel.rel_type,
            RelationshipType::Calls
                | RelationshipType::CallsAction
                | RelationshipType::CallsService
        ) {
            has_incoming_call.insert(rel.target_id.clone());
        }
    }

    // Build set of Interface node IDs to exclude their method declarations
    let interface_ids: HashSet<String> = graph
        .iter_nodes()
        .filter(|n| n.label == NodeLabel::Interface)
        .map(|n| n.id.clone())
        .collect();

    // Build set of method IDs that belong to interfaces (via HasMethod edges)
    let mut interface_method_ids: HashSet<String> = HashSet::new();
    for rel in graph.iter_relationships() {
        if rel.rel_type == RelationshipType::HasMethod && interface_ids.contains(&rel.source_id) {
            interface_method_ids.insert(rel.target_id.clone());
        }
    }

    // Build set of (file_path, method_name) for ControllerAction/ApiEndpoint nodes.
    // When a controller method is detected, the pipeline creates BOTH a Method node
    // (from parsing) and a ControllerAction node (from ASP.NET enrichment).
    // The ControllerAction receives CallsAction edges from views, but the Method
    // doesn't — so we must exclude Methods that have a matching ControllerAction.
    let action_signatures: HashSet<(String, String)> = graph
        .iter_nodes()
        .filter(|n| {
            matches!(
                n.label,
                NodeLabel::ControllerAction | NodeLabel::ApiEndpoint
            )
        })
        .map(|n| (n.properties.file_path.clone(), n.properties.name.clone()))
        .collect();

    // Entry point method names that should never be flagged as dead
    let entry_point_names: HashSet<&str> = [
        // Standard entry points
        "Main",
        "main",
        "Dispose",
        // ASP.NET Application lifecycle
        "Application_Start",
        "Application_End",
        "Application_Error",
        "Application_BeginRequest",
        "Application_EndRequest",
        "Application_AuthenticateRequest",
        "Session_Start",
        "Session_End",
        // ASP.NET Core startup
        "Configuration",
        "ConfigureServices",
        "Configure",
        // ASP.NET MVC registration
        "RegisterRoutes",
        "RegisterBundles",
        "RegisterGlobalFilters",
        "RegisterAreas",
        // ASP.NET MVC filter overrides
        "OnAuthorization",
        "OnActionExecuting",
        "OnActionExecuted",
        "OnResultExecuting",
        "OnResultExecuted",
        "OnException",
    ]
    .into_iter()
    .collect();

    let is_test_file = |path: &str| -> bool {
        let lower = path.to_lowercase();
        // Match test directories and test file patterns, not substrings
        lower.contains("/test/")
            || lower.contains("\\test\\")
            || lower.contains("/tests/")
            || lower.contains("\\tests\\")
            || lower.contains(".test/")
            || lower.contains(".test\\")
            || lower.contains(".tests/")
            || lower.contains(".tests\\")
            || lower.contains(".test.")
            || lower.contains("_test.")
            || lower.ends_with("/test.cs")
            || lower.ends_with("\\test.cs")
            || lower.ends_with("_test.cs")
            || lower.ends_with(".test.cs")
            || lower.ends_with("/test.js")
            || lower.ends_with("\\test.js")
            || lower.ends_with("_test.js")
            || lower.ends_with(".test.js")
            || lower.ends_with("/test.ts")
            || lower.ends_with("\\test.ts")
            || lower.ends_with("_test.ts")
            || lower.ends_with(".test.ts")
            || lower.contains(".spec.")
            || lower.contains("_spec.")
    };

    // Exclude JS functions in views, scripts, and compiled output.
    // Uses .contains() instead of .ends_with() because inline scripts
    // have paths like "Views/Foo.cshtml#script-0".
    let is_script_or_view = |path: &str| -> bool {
        let lower = path.to_lowercase();
        // Don't exclude code-behind files (.cshtml.cs, .razor.cs)
        if lower.ends_with(".cs") || lower.ends_with(".ts") {
            return false;
        }
        lower.contains(".cshtml") || lower.contains(".razor")
            || lower.contains("#script-") // virtual embedded JS from Razor
            || lower.ends_with(".jsx")
            || lower.ends_with(".vue")
    };

    let is_compiled_output = |path: &str| -> bool {
        let lower = path.to_lowercase();
        lower.contains("/obj/")
            || lower.contains("\\obj\\")
            || lower.contains("/bin/")
            || lower.contains("\\bin\\")
    };

    // Collect method IDs to evaluate — exclude Constructors entirely since
    // they are typically instantiated via DI containers or reflection.
    let method_ids: Vec<(String, bool)> = graph
        .iter_nodes()
        .filter(|n| matches!(n.label, NodeLabel::Method | NodeLabel::Function))
        .map(|n| {
            let is_entry = entry_point_names.contains(n.properties.name.as_str());
            let is_test = is_test_file(&n.properties.file_path);
            let is_interface_method = interface_method_ids.contains(&n.id);
            let is_view_script = n.label == NodeLabel::Function
                && (is_script_or_view(&n.properties.file_path)
                    || is_compiled_output(&n.properties.file_path));
            // Methods in controllers that have a matching ControllerAction node
            let has_action = n.label == NodeLabel::Method
                && action_signatures
                    .contains(&(n.properties.file_path.clone(), n.properties.name.clone()));

            (
                n.id.clone(),
                is_entry || is_test || is_interface_method || is_view_script || has_action,
            )
        })
        .collect();

    let mut dead_count = 0u32;
    let mut live_count = 0u32;
    let mut excluded_count = 0u32;

    for (method_id, is_excluded) in &method_ids {
        if *is_excluded {
            excluded_count += 1;
            continue;
        }
        if has_incoming_call.contains(method_id) {
            live_count += 1;
        } else if let Some(node) = graph.get_node_mut(method_id) {
            node.properties.is_dead_candidate = Some(true);
            dead_count += 1;
        }
    }

    tracing::info!(
        dead_candidates = dead_count,
        live_methods = live_count,
        excluded_methods = excluded_count,
        total_methods = method_ids.len(),
        "Dead code detection complete"
    );
}

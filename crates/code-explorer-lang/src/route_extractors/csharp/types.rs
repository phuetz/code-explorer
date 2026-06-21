//! Result types for C# ASP.NET MVC route extraction.

use std::collections::HashMap;

/// Information extracted from a C# class that may be an ASP.NET controller.
#[derive(Debug, Clone)]
pub struct ControllerInfo {
    /// Class name (e.g., "ProductsController")
    pub class_name: String,
    /// Area name from [Area("...")] attribute, if any
    pub area_name: Option<String>,
    /// Route prefix from [Route("...")] or [RoutePrefix("...")] on the class
    pub route_prefix: Option<String>,
    /// Whether this is an API controller (inherits ApiController or has [ApiController])
    pub is_api_controller: bool,
    /// The [Authorize] attribute roles/policies, if any
    pub authorize: Option<String>,
    /// Actions discovered inside this controller
    pub actions: Vec<ActionInfo>,
    /// Custom base controller name if not one of the standard base classes
    /// (Controller, ApiController, AsyncController, ControllerBase, ODataController)
    pub base_controller: Option<String>,
}

/// Information about a single controller action method.
#[derive(Debug, Clone)]
pub struct ActionInfo {
    /// Method name
    pub name: String,
    /// HTTP method: GET, POST, PUT, DELETE, PATCH (default GET for MVC)
    pub http_method: String,
    /// Route template from [Route("...")] on the method, or inferred from convention
    pub route_template: Option<String>,
    /// Parameter type for model binding (e.g., "ProductViewModel")
    pub model_type: Option<String>,
    /// Return type (e.g., "ActionResult", "JsonResult", "IHttpActionResult")
    pub return_type: Option<String>,
    /// Whether the action has [Authorize]
    pub requires_auth: bool,
    /// Start line in source (1-indexed)
    pub start_line: Option<u32>,
    /// Filter/attribute names applied to this action (e.g., "GridAction", "ValidateAntiForgeryToken")
    pub filters: Vec<String>,
    /// Method parameter signature (e.g., "string id, int page")
    pub parameters: Option<String>,
}

/// Information about an Entity Framework DbContext.
#[derive(Debug, Clone)]
pub struct DbContextInfo {
    /// Class name (e.g., "ApplicationDbContext")
    pub class_name: String,
    /// Connection string name from constructor or attribute
    pub connection_string_name: Option<String>,
    /// DbSet<T> properties (entity type name -> property name)
    pub entity_sets: Vec<EntitySetInfo>,
}

/// A DbSet<T> property inside a DbContext.
#[derive(Debug, Clone)]
pub struct EntitySetInfo {
    /// The entity type (e.g., "Product")
    pub entity_type: String,
    /// The property name (e.g., "Products")
    pub property_name: String,
}

/// Information about an Entity Framework entity / model class.
#[derive(Debug, Clone)]
pub struct EntityInfo {
    /// Class name
    pub class_name: String,
    /// [Table("...")] attribute value, if any
    pub table_name: Option<String>,
    /// Data annotations on properties: property name -> list of annotations
    pub property_annotations: HashMap<String, Vec<String>>,
    /// Navigation property names (references to other entities)
    pub navigation_properties: Vec<NavigationProperty>,
}

/// A navigation property on an EF entity.
#[derive(Debug, Clone)]
pub struct NavigationProperty {
    /// Property name (e.g., "Orders")
    pub name: String,
    /// Target entity type (e.g., "Order")
    pub target_type: String,
    /// Whether this is a collection navigation (ICollection<T>, List<T>, etc.)
    pub is_collection: bool,
}

/// Information extracted from a Razor view file.
#[derive(Debug, Clone)]
pub struct ViewInfo {
    /// File path
    pub file_path: String,
    /// @model directive type, if any
    pub model_type: Option<String>,
    /// @Layout directive, if any
    pub layout_path: Option<String>,
    /// Area name inferred from path (Areas/<name>/Views/...)
    pub area_name: Option<String>,
    /// Whether this is a partial view (filename starts with _)
    pub is_partial: bool,
}

/// AJAX call targeting a controller action.
#[derive(Debug, Clone)]
pub struct AjaxCallInfo {
    /// HTTP method (GET, POST, etc.)
    pub method: String,
    /// Controller name extracted from the URL, if any
    pub controller_name: Option<String>,
    /// Action name extracted from the URL, if any
    pub action_name: Option<String>,
    /// The raw URL pattern matched
    pub url_pattern: String,
    /// Line number where the call was found (1-indexed)
    pub line_number: u32,
}

/// An action in a DataSource transport configuration.
#[derive(Debug, Clone)]
pub struct DataSourceAction {
    /// CRUD operation: "Read", "Create", "Update", "Destroy"
    pub operation: String,
    /// Controller name
    pub controller_name: String,
    /// Action name
    pub action_name: String,
}

/// A column binding extracted from a Telerik Grid `.Columns(...)` block.
#[derive(Debug, Clone)]
pub struct GridColumnInfo {
    /// The property name bound via `columns.Bound(e => e.Property)`
    pub property_name: String,
    /// The display title set via `.Title("...")`
    pub title: Option<String>,
    /// Whether the column uses `.ClientTemplate(...)`
    pub has_client_template: bool,
}

/// Telerik or Kendo UI component extracted from a Razor view.
#[derive(Debug, Clone)]
pub struct TelerikComponentInfo {
    /// Component type (e.g., "Grid", "ComboBox", "DropDownList")
    pub component_type: String,
    /// Vendor identifier: "Kendo" or "Telerik"
    pub vendor: String,
    /// Generic model type, if any (e.g., "ProductViewModel")
    pub model_type: Option<String>,
    /// DataSource transport actions found nearby
    pub data_source_actions: Vec<DataSourceAction>,
    /// Client-side events: (event_name, js_function_name)
    pub client_events: Vec<(String, String)>,
    /// Grid column bindings extracted from `.Columns(...)` block
    pub columns: Vec<GridColumnInfo>,
    /// Line number where the component declaration starts (1-indexed)
    pub line_number: u32,
}

/// A service or repository class detected via naming conventions and DI patterns.
#[derive(Debug, Clone)]
pub struct ServiceInfo {
    /// Class name (e.g., "ProductService", "OrderRepository")
    pub class_name: String,
    /// Detected layer type: "Service", "Repository", "Manager", "Provider", "UnitOfWork", "Factory", or "Facade"
    pub layer_type: String,
    /// Interface implemented (e.g., "IProductService")
    pub implements_interface: Option<String>,
    /// Constructor-injected dependencies: (interface_type, parameter_name)
    pub dependencies: Vec<(String, String)>,
}

/// A DI container registration extracted from C# source.
#[derive(Debug, Clone)]
pub struct DiRegistration {
    /// The concrete implementation type (e.g., "ProductService")
    pub implementation_type: String,
    /// The service/interface type (e.g., "IProductService")
    pub service_type: String,
    /// The DI framework: "Autofac", "Unity", "Ninject", "Microsoft"
    pub framework: String,
    /// Lifetime scope, if detected: "Singleton", "Transient", "Scoped", "PerRequest", etc.
    pub lifetime: Option<String>,
}

/// Information about an Html.BeginForm() call in a Razor view.
#[derive(Debug, Clone)]
pub struct FormActionInfo {
    /// The action name (first argument)
    pub action_name: String,
    /// The controller name (second argument)
    pub controller_name: String,
    /// HTTP method from FormMethod enum (defaults to "POST")
    pub http_method: String,
    /// Line number where the form was found (1-indexed)
    pub line_number: u32,
}

/// A reference to a partial view or child action in a Razor view.
#[derive(Debug, Clone)]
pub struct PartialReference {
    /// The partial view or action name (e.g., "_VuePrestationGrpAide")
    pub partial_name: String,
    /// The controller name (for `Html.Action` / `Html.RenderAction`), if specified
    pub controller_name: Option<String>,
    /// The helper method type: "Partial", "RenderPartial", "Action", or "RenderAction"
    pub helper_type: String,
    /// Line number where the reference was found (1-indexed)
    pub line_number: u32,
}

/// Information about tracing/logging instrumentation in a C# file.
#[derive(Debug, Clone)]
pub struct TracingInfo {
    /// Whether this file has any StackLogger/tracing calls
    pub is_traced: bool,
    /// Number of StackLogger method calls (Info, Error, Warning, etc. -- NOT BeginMethodScope)
    pub call_count: u32,
    /// Methods with BeginMethodScope (fully traced methods)
    pub traced_methods: Vec<String>,
    /// Tracing methods used (Info, Error, Warning, etc.)
    pub log_levels_used: Vec<String>,
}

/// External API/service call site detected in C# source.
#[derive(Debug, Clone)]
pub struct ExternalServiceCall {
    /// Service type: "WebAPI", "WCF", "HttpClient"
    pub service_type: String,
    /// Client class name (e.g., "AcmeClient", "PartnerClient")
    pub client_class: String,
    /// Method called (e.g., "OuvrantsDroitGetAsync")
    pub method_name: Option<String>,
    /// Line number (1-indexed)
    pub line_number: u32,
}

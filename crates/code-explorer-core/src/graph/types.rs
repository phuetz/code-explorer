use serde::{Deserialize, Serialize};

use crate::config::languages::SupportedLanguage;

// ─── Node Labels ─────────────────────────────────────────────────────────

/// All possible node types in the knowledge graph.
/// Matches the TypeScript `NodeLabel` union type exactly (38 variants).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum NodeLabel {
    Project,
    Package,
    Module,
    Folder,
    File,
    Class,
    Function,
    Method,
    Variable,
    Interface,
    Enum,
    Decorator,
    Import,
    Type,
    CodeElement,
    Community,
    Process,
    // Multi-language node types
    Struct,
    Macro,
    Typedef,
    Union,
    Namespace,
    Trait,
    Impl,
    TypeAlias,
    Const,
    Static,
    Property,
    Record,
    Delegate,
    Annotation,
    Constructor,
    Template,
    Section,
    /// API route endpoint (e.g., /api/grants)
    Route,
    /// MCP tool definition
    Tool,
    /// External library / UI component library
    Library,
    // ── ASP.NET MVC 5 / EF6 node types ───────────────────────────
    /// ASP.NET MVC Controller class (inherits Controller / ApiController)
    Controller,
    /// Action method inside a Controller
    ControllerAction,
    /// Web API endpoint (ApiController-based)
    ApiEndpoint,
    /// Razor view (.cshtml) or ASPX view (.aspx)
    View,
    /// ViewModel / DTO class used for model binding
    ViewModel,
    /// Entity Framework entity (mapped to a DB table)
    DbEntity,
    /// Entity Framework DbContext / ObjectContext class
    DbContext,
    /// ASP.NET MVC Area (logical grouping of controllers/views)
    Area,
    /// ASP.NET MVC filter attribute (Authorize, ValidateAntiForgeryToken, etc.)
    Filter,
    /// Web.config configuration file
    WebConfig,
    /// Partial view referenced by @Html.Partial or @Html.RenderPartial
    PartialView,
    /// JavaScript file included in views or layouts
    ScriptFile,
    /// AJAX call site ($.ajax, $.post, $.get, fetch) targeting a controller action
    AjaxCall,
    /// UI component instance (Telerik Grid, Kendo DatePicker, etc.)
    UiComponent,
    /// Business logic service class
    Service,
    /// Data access repository class
    Repository,
    /// External service/API endpoint (REST, SOAP, WCF)
    ExternalService,
    // ── GraphRAG node types ──────────────────────────────────────
    /// External documentation document
    Document,
    /// Chunk of a document for semantic search
    DocChunk,
    // ── Code Quality node types ─────────────────────────────────
    /// A TODO/FIXME/HACK/XXX marker extracted from source code.
    /// Anchored to File/Method/Function via BelongsTo edges.
    TodoMarker,
    // ── Schema & API Inventory (Theme D) ───────────────────────
    /// A single column of a database table. Child of DbEntity via HasColumn.
    DbColumn,
    /// An environment variable declared in config files or referenced in code.
    EnvVar,
}

impl NodeLabel {
    /// Returns the string representation matching the TypeScript enum.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Project => "Project",
            Self::Package => "Package",
            Self::Module => "Module",
            Self::Folder => "Folder",
            Self::File => "File",
            Self::Class => "Class",
            Self::Function => "Function",
            Self::Method => "Method",
            Self::Variable => "Variable",
            Self::Interface => "Interface",
            Self::Enum => "Enum",
            Self::Decorator => "Decorator",
            Self::Import => "Import",
            Self::Type => "Type",
            Self::CodeElement => "CodeElement",
            Self::Community => "Community",
            Self::Process => "Process",
            Self::Struct => "Struct",
            Self::Macro => "Macro",
            Self::Typedef => "Typedef",
            Self::Union => "Union",
            Self::Namespace => "Namespace",
            Self::Trait => "Trait",
            Self::Impl => "Impl",
            Self::TypeAlias => "TypeAlias",
            Self::Const => "Const",
            Self::Static => "Static",
            Self::Property => "Property",
            Self::Record => "Record",
            Self::Delegate => "Delegate",
            Self::Annotation => "Annotation",
            Self::Constructor => "Constructor",
            Self::Template => "Template",
            Self::Section => "Section",
            Self::Route => "Route",
            Self::Tool => "Tool",
            Self::Library => "Library",
            Self::Controller => "Controller",
            Self::ControllerAction => "ControllerAction",
            Self::ApiEndpoint => "ApiEndpoint",
            Self::View => "View",
            Self::ViewModel => "ViewModel",
            Self::DbEntity => "DbEntity",
            Self::DbContext => "DbContext",
            Self::Area => "Area",
            Self::Filter => "Filter",
            Self::WebConfig => "WebConfig",
            Self::PartialView => "PartialView",
            Self::ScriptFile => "ScriptFile",
            Self::AjaxCall => "AjaxCall",
            Self::UiComponent => "UiComponent",
            Self::Service => "Service",
            Self::Repository => "Repository",
            Self::ExternalService => "ExternalService",
            Self::Document => "Document",
            Self::DocChunk => "DocChunk",
            Self::TodoMarker => "TodoMarker",
            Self::DbColumn => "DbColumn",
            Self::EnvVar => "EnvVar",
        }
    }

    /// Parse from string, matching TypeScript values.
    pub fn from_str_label(s: &str) -> Option<Self> {
        match s {
            "Project" => Some(Self::Project),
            "Package" => Some(Self::Package),
            "Module" => Some(Self::Module),
            "Folder" => Some(Self::Folder),
            "File" => Some(Self::File),
            "Class" => Some(Self::Class),
            "Function" => Some(Self::Function),
            "Method" => Some(Self::Method),
            "Variable" => Some(Self::Variable),
            "Interface" => Some(Self::Interface),
            "Enum" => Some(Self::Enum),
            "Decorator" => Some(Self::Decorator),
            "Import" => Some(Self::Import),
            "Type" => Some(Self::Type),
            "CodeElement" => Some(Self::CodeElement),
            "Community" => Some(Self::Community),
            "Process" => Some(Self::Process),
            "Struct" => Some(Self::Struct),
            "Macro" => Some(Self::Macro),
            "Typedef" => Some(Self::Typedef),
            "Union" => Some(Self::Union),
            "Namespace" => Some(Self::Namespace),
            "Trait" => Some(Self::Trait),
            "Impl" => Some(Self::Impl),
            "TypeAlias" => Some(Self::TypeAlias),
            "Const" => Some(Self::Const),
            "Static" => Some(Self::Static),
            "Property" => Some(Self::Property),
            "Record" => Some(Self::Record),
            "Delegate" => Some(Self::Delegate),
            "Annotation" => Some(Self::Annotation),
            "Constructor" => Some(Self::Constructor),
            "Template" => Some(Self::Template),
            "Section" => Some(Self::Section),
            "Route" => Some(Self::Route),
            "Tool" => Some(Self::Tool),
            "Library" => Some(Self::Library),
            "Controller" => Some(Self::Controller),
            "ControllerAction" => Some(Self::ControllerAction),
            "ApiEndpoint" => Some(Self::ApiEndpoint),
            "View" => Some(Self::View),
            "ViewModel" => Some(Self::ViewModel),
            "DbEntity" => Some(Self::DbEntity),
            "DbContext" => Some(Self::DbContext),
            "Area" => Some(Self::Area),
            "Filter" => Some(Self::Filter),
            "WebConfig" => Some(Self::WebConfig),
            "PartialView" => Some(Self::PartialView),
            "ScriptFile" => Some(Self::ScriptFile),
            "AjaxCall" => Some(Self::AjaxCall),
            "UiComponent" => Some(Self::UiComponent),
            "Service" => Some(Self::Service),
            "Repository" => Some(Self::Repository),
            "ExternalService" => Some(Self::ExternalService),
            "Document" => Some(Self::Document),
            "DocChunk" => Some(Self::DocChunk),
            "TodoMarker" => Some(Self::TodoMarker),
            "DbColumn" => Some(Self::DbColumn),
            "EnvVar" => Some(Self::EnvVar),
            _ => None,
        }
    }
}

impl std::fmt::Display for NodeLabel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

// ─── Relationship Types ──────────────────────────────────────────────────

/// All possible relationship types in the knowledge graph.
/// Matches the TypeScript `RelationshipType` union type exactly (20 variants).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RelationshipType {
    Contains,
    Calls,
    Inherits,
    Overrides,
    Imports,
    Uses,
    Defines,
    Decorates,
    Implements,
    Extends,
    HasMethod,
    HasProperty,
    Accesses,
    MemberOf,
    StepInProcess,
    /// Function/File -> Route (handler serves this endpoint)
    HandlesRoute,
    /// Function/File -> Route (consumer calls this endpoint)
    Fetches,
    /// Function/File -> Tool (handler implements this tool)
    HandlesTool,
    /// Route/Tool -> Process (this endpoint starts this execution flow)
    EntryPointOf,
    /// Function -> Function (middleware wrapper chain) — Reserved: future
    Wraps,
    // ── ASP.NET MVC 5 / EF6 relationship types ──────────────────
    /// Controller/ControllerAction -> View (renders this Razor view)
    RendersView,
    /// Controller -> Area (belongs to this MVC area)
    BelongsToArea,
    /// DbContext -> DbEntity (exposes this entity set)
    MapsToEntity,
    /// Controller -> ControllerAction (has this action method)
    HasAction,
    /// ControllerAction -> ViewModel/DbEntity (binds this model type)
    BindsModel,
    /// DbEntity -> DbEntity (navigation property / FK association)
    AssociatesWith,
    /// Controller/ControllerAction -> Filter (has this attribute filter)
    HasFilter,
    /// View -> PartialView (renders this partial view)
    UsesPartial,
    /// Controller/Area -> WebConfig (configured by this web.config)
    ConfiguredBy,
    /// AJAX/Script calls a controller action
    CallsAction,
    /// View/Layout includes a script file
    IncludesScript,
    /// View renders a UI component
    RendersComponent,
    /// Controller/Service depends on another service/repository (DI)
    DependsOn,
    /// Code calls an external service (WebAPI, WCF, REST)
    CallsService,
    // ── GraphRAG relationship types ─────────────────────────────
    /// Chunk belongs to a Document
    BelongsTo,
    /// Chunk mentions a specific code symbol
    Mentions,
    // ── Schema & API Inventory (Theme D) ────────────────────────
    /// DbEntity -> DbColumn (entity owns this column)
    HasColumn,
    /// DbEntity -> DbEntity (foreign-key reference between tables)
    ReferencesTable,
    /// Method/File -> EnvVar (this code reads the environment variable)
    UsesEnvVar,
    /// Class -> DbEntity (ORM class represents this entity/table)
    RepresentedBy,
    /// ApiEndpoint -> Method (endpoint is implemented by this handler method)
    HandledBy,
}

impl RelationshipType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Contains => "CONTAINS",
            Self::Calls => "CALLS",
            Self::Inherits => "INHERITS",
            Self::Overrides => "OVERRIDES",
            Self::Imports => "IMPORTS",
            Self::Uses => "USES",
            Self::Defines => "DEFINES",
            Self::Decorates => "DECORATES",
            Self::Implements => "IMPLEMENTS",
            Self::Extends => "EXTENDS",
            Self::HasMethod => "HAS_METHOD",
            Self::HasProperty => "HAS_PROPERTY",
            Self::Accesses => "ACCESSES",
            Self::MemberOf => "MEMBER_OF",
            Self::StepInProcess => "STEP_IN_PROCESS",
            Self::HandlesRoute => "HANDLES_ROUTE",
            Self::Fetches => "FETCHES",
            Self::HandlesTool => "HANDLES_TOOL",
            Self::EntryPointOf => "ENTRY_POINT_OF",
            Self::Wraps => "WRAPS",
            Self::RendersView => "RENDERS_VIEW",
            Self::BelongsToArea => "BELONGS_TO_AREA",
            Self::MapsToEntity => "MAPS_TO_ENTITY",
            Self::HasAction => "HAS_ACTION",
            Self::BindsModel => "BINDS_MODEL",
            Self::AssociatesWith => "ASSOCIATES_WITH",
            Self::HasFilter => "HAS_FILTER",
            Self::UsesPartial => "USES_PARTIAL",
            Self::ConfiguredBy => "CONFIGURED_BY",
            Self::CallsAction => "CALLS_ACTION",
            Self::IncludesScript => "INCLUDES_SCRIPT",
            Self::RendersComponent => "RENDERS_COMPONENT",
            Self::DependsOn => "DEPENDS_ON",
            Self::CallsService => "CALLS_SERVICE",
            Self::BelongsTo => "BELONGS_TO",
            Self::Mentions => "MENTIONS",
            Self::HasColumn => "HAS_COLUMN",
            Self::ReferencesTable => "REFERENCES_TABLE",
            Self::UsesEnvVar => "USES_ENV_VAR",
            Self::RepresentedBy => "REPRESENTED_BY",
            Self::HandledBy => "HANDLED_BY",
        }
    }

    pub fn from_str_type(s: &str) -> Option<Self> {
        match s {
            "CONTAINS" => Some(Self::Contains),
            "CALLS" => Some(Self::Calls),
            "INHERITS" => Some(Self::Inherits),
            "OVERRIDES" => Some(Self::Overrides),
            "IMPORTS" => Some(Self::Imports),
            "USES" => Some(Self::Uses),
            "DEFINES" => Some(Self::Defines),
            "DECORATES" => Some(Self::Decorates),
            "IMPLEMENTS" => Some(Self::Implements),
            "EXTENDS" => Some(Self::Extends),
            "HAS_METHOD" => Some(Self::HasMethod),
            "HAS_PROPERTY" => Some(Self::HasProperty),
            "ACCESSES" => Some(Self::Accesses),
            "MEMBER_OF" => Some(Self::MemberOf),
            "STEP_IN_PROCESS" => Some(Self::StepInProcess),
            "HANDLES_ROUTE" => Some(Self::HandlesRoute),
            "FETCHES" => Some(Self::Fetches),
            "HANDLES_TOOL" => Some(Self::HandlesTool),
            "ENTRY_POINT_OF" => Some(Self::EntryPointOf),
            "WRAPS" => Some(Self::Wraps),
            "RENDERS_VIEW" => Some(Self::RendersView),
            "BELONGS_TO_AREA" => Some(Self::BelongsToArea),
            "MAPS_TO_ENTITY" => Some(Self::MapsToEntity),
            "HAS_ACTION" => Some(Self::HasAction),
            "BINDS_MODEL" => Some(Self::BindsModel),
            "ASSOCIATES_WITH" => Some(Self::AssociatesWith),
            "HAS_FILTER" => Some(Self::HasFilter),
            "USES_PARTIAL" => Some(Self::UsesPartial),
            "CONFIGURED_BY" => Some(Self::ConfiguredBy),
            "CALLS_ACTION" => Some(Self::CallsAction),
            "INCLUDES_SCRIPT" => Some(Self::IncludesScript),
            "RENDERS_COMPONENT" => Some(Self::RendersComponent),
            "DEPENDS_ON" => Some(Self::DependsOn),
            "CALLS_SERVICE" => Some(Self::CallsService),
            "BELONGS_TO" => Some(Self::BelongsTo),
            "MENTIONS" => Some(Self::Mentions),
            "HAS_COLUMN" => Some(Self::HasColumn),
            "REFERENCES_TABLE" => Some(Self::ReferencesTable),
            "USES_ENV_VAR" => Some(Self::UsesEnvVar),
            "REPRESENTED_BY" => Some(Self::RepresentedBy),
            "HANDLED_BY" => Some(Self::HandledBy),
            _ => None,
        }
    }
}

impl std::fmt::Display for RelationshipType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

// ─── Enrichment Source ───────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EnrichedBy {
    Heuristic,
    Llm,
}

// ─── Process Type ────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProcessType {
    IntraCommunity,
    CrossCommunity,
}

// ─── Node Properties ─────────────────────────────────────────────────────

/// Properties attached to a graph node.
/// Matches the TypeScript `NodeProperties` type.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NodeProperties {
    pub name: String,
    pub file_path: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_line: Option<u32>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_line: Option<u32>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub language: Option<SupportedLanguage>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_exported: Option<bool>,

    // AST-derived framework hint
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ast_framework_multiplier: Option<f64>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub ast_framework_reason: Option<String>,

    // Community properties
    #[serde(skip_serializing_if = "Option::is_none")]
    pub heuristic_label: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub cohesion: Option<f64>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub symbol_count: Option<u32>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub keywords: Option<Vec<String>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub enriched_by: Option<EnrichedBy>,

    // Process properties
    #[serde(skip_serializing_if = "Option::is_none")]
    pub process_type: Option<ProcessType>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub step_count: Option<u32>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub communities: Option<Vec<String>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub entry_point_id: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub terminal_id: Option<String>,

    // Entry point scoring
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entry_point_score: Option<f64>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub entry_point_reason: Option<String>,

    // Method signature
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parameter_count: Option<u32>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub return_type: Option<String>,

    // Section-specific (markdown heading level, 1-6)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub level: Option<u8>,

    // Response shape
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_keys: Option<Vec<String>>,

    // Error response shape
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_keys: Option<Vec<String>>,

    // Middleware wrapper chain
    #[serde(skip_serializing_if = "Option::is_none")]
    pub middleware: Option<Vec<String>>,

    // ── ASP.NET MVC 5 / EF6 properties ──────────────────────────
    /// HTTP method for ControllerAction/ApiEndpoint (GET, POST, PUT, DELETE)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub http_method: Option<String>,

    /// Route template string, e.g. "api/products/{id}" or "{controller}/{action}/{id?}"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub route_template: Option<String>,

    /// MVC Area name for controllers/views
    #[serde(skip_serializing_if = "Option::is_none")]
    pub area_name: Option<String>,

    /// Database table name for DbEntity nodes (from .edmx EntitySet or [Table] attribute)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub db_table_name: Option<String>,

    /// EF association cardinality, e.g. "1:*", "1:1", "*:*"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ef_cardinality: Option<String>,

    /// View engine type: "razor", "aspx", "partial"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub view_engine: Option<String>,

    /// Layout/master page path for views
    #[serde(skip_serializing_if = "Option::is_none")]
    pub layout_path: Option<String>,

    /// Model type bound to a view (@model directive)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_type: Option<String>,

    /// Data annotations on entity properties, e.g. ["Required", "MaxLength(100)"]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data_annotations: Option<Vec<String>>,

    /// Connection string name for DbContext
    #[serde(skip_serializing_if = "Option::is_none")]
    pub connection_string_name: Option<String>,

    /// AJAX HTTP method (GET, POST, PUT, DELETE)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ajax_method: Option<String>,

    /// URL pattern in AJAX call
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ajax_url: Option<String>,

    /// UI component type (e.g., "Kendo.Grid", "Telerik.ComboBox")
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub component_type: Option<String>,

    /// Model type bound to a UI component
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bound_model: Option<String>,

    /// Service/Repository layer classification
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layer_type: Option<String>,

    /// Interface that a service/repository implements
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub implements_interface: Option<String>,

    /// Whether this code file/method is instrumented with tracing (e.g. StackLogger)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub is_traced: Option<bool>,

    /// Number of tracing/logging calls in this scope
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trace_call_count: Option<u32>,

    /// Whether this method has no incoming Calls edges (potential dead code)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub is_dead_candidate: Option<bool>,

    /// Cyclomatic complexity (CC) for Method/Function/Constructor nodes
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub complexity: Option<u32>,

    /// External service type (WebAPI, WCF, REST)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub service_type: Option<String>,

    // ── LLM enrichment properties ────────────────────────────────
    /// Detected code smells (e.g. "GodObject", "FeatureEnvy", "SrpViolation", "LongMethod")
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub llm_smells: Option<Vec<String>>,

    /// Detected design patterns (e.g. "Repository", "Factory", "Observer")
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub llm_patterns: Option<Vec<String>>,

    /// Risk score 0-100 assigned by LLM (composite of complexity + coupling + coverage)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub llm_risk_score: Option<u32>,

    /// One-line refactoring suggestion from LLM
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub llm_refactoring: Option<String>,

    /// SHA-256 hash of the source code at time of LLM enrichment (for incrementality)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub llm_source_hash: Option<String>,

    // ── GraphRAG properties ──────────────────────────────────────
    /// Document chunk text content
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,

    /// Document title
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,

    /// Page number or section index
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub page_number: Option<u32>,

    /// Semantic embedding vector for chunks
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub embedding: Option<Vec<f64>>,

    /// Source URL for externally-sourced documents
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_url: Option<String>,

    // ── Code Quality (TODO markers) ────────────────────────────
    /// Kind of TODO marker: "TODO", "FIXME", "HACK", "XXX".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub todo_kind: Option<String>,

    /// Full text of the TODO comment (after the marker).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub todo_text: Option<String>,

    // ── Schema & API Inventory (Theme D) ───────────────────────
    /// Route path for ApiEndpoint nodes (e.g. "/api/users/:id").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub route: Option<String>,

    /// Node ID of the handler Method for an ApiEndpoint.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub handler_id: Option<String>,

    /// Framework that produced the ApiEndpoint (e.g. "express", "fastapi",
    /// "spring", "nextjs"). Used in filters and badges in the UI.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub framework: Option<String>,

    /// SQL column type for DbColumn (e.g. "VARCHAR(255)", "INTEGER").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub column_type: Option<String>,

    /// Whether the DbColumn is marked as primary key.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub is_primary_key: Option<bool>,

    /// Whether the DbColumn is nullable.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub is_nullable: Option<bool>,

    /// File where an EnvVar was first declared (`.env`, `appsettings.json`,
    /// `application.yml`, etc.). None when the variable is only referenced.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub declared_in: Option<String>,

    /// Number of distinct code references to the EnvVar.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub used_in_count: Option<u32>,

    /// EnvVar declared in config files but never referenced in code.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unused: Option<bool>,

    /// EnvVar referenced in code but not declared in any config file.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub undeclared: Option<bool>,
}

// ─── Graph Node ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphNode {
    pub id: String,
    pub label: NodeLabel,
    pub properties: NodeProperties,
}

// ─── Graph Relationship ──────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphRelationship {
    pub id: String,
    pub source_id: String,
    pub target_id: String,
    #[serde(rename = "type")]
    pub rel_type: RelationshipType,
    /// Confidence score 0-1 (1.0 = certain, lower = uncertain resolution)
    pub confidence: f64,
    /// Semantics are edge-type-dependent:
    /// CALLS uses resolution tier, ACCESSES uses 'read'/'write', OVERRIDES uses MRO reason
    pub reason: String,
    /// Step number for STEP_IN_PROCESS relationships (1-indexed)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub step: Option<u32>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_node_label_roundtrip() {
        let label = NodeLabel::Function;
        let json = serde_json::to_string(&label).unwrap();
        assert_eq!(json, "\"Function\"");
        let parsed: NodeLabel = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, label);
    }

    #[test]
    fn test_relationship_type_roundtrip() {
        let rt = RelationshipType::StepInProcess;
        let json = serde_json::to_string(&rt).unwrap();
        assert_eq!(json, "\"STEP_IN_PROCESS\"");
        let parsed: RelationshipType = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, rt);
    }

    #[test]
    fn test_node_label_all_variants() {
        // Ensure all variants have a str representation
        let labels = [
            NodeLabel::Project,
            NodeLabel::Package,
            NodeLabel::Module,
            NodeLabel::Folder,
            NodeLabel::File,
            NodeLabel::Class,
            NodeLabel::Function,
            NodeLabel::Method,
            NodeLabel::Variable,
            NodeLabel::Interface,
            NodeLabel::Enum,
            NodeLabel::Decorator,
            NodeLabel::Import,
            NodeLabel::Type,
            NodeLabel::CodeElement,
            NodeLabel::Community,
            NodeLabel::Process,
            NodeLabel::Struct,
            NodeLabel::Macro,
            NodeLabel::Typedef,
            NodeLabel::Union,
            NodeLabel::Namespace,
            NodeLabel::Trait,
            NodeLabel::Impl,
            NodeLabel::TypeAlias,
            NodeLabel::Const,
            NodeLabel::Static,
            NodeLabel::Property,
            NodeLabel::Record,
            NodeLabel::Delegate,
            NodeLabel::Annotation,
            NodeLabel::Constructor,
            NodeLabel::Template,
            NodeLabel::Section,
            NodeLabel::Route,
            NodeLabel::Tool,
            NodeLabel::Library,
            // ASP.NET MVC 5 / EF6
            NodeLabel::Controller,
            NodeLabel::ControllerAction,
            NodeLabel::ApiEndpoint,
            NodeLabel::View,
            NodeLabel::ViewModel,
            NodeLabel::DbEntity,
            NodeLabel::DbContext,
            NodeLabel::Area,
            NodeLabel::Filter,
            NodeLabel::WebConfig,
            NodeLabel::PartialView,
            // Extended ASP.NET types
            NodeLabel::ScriptFile,
            NodeLabel::AjaxCall,
            NodeLabel::UiComponent,
            NodeLabel::Service,
            NodeLabel::Repository,
            NodeLabel::ExternalService,
            // GraphRAG types
            NodeLabel::Document,
            NodeLabel::DocChunk,
            // Code Quality types
            NodeLabel::TodoMarker,
            // Schema & API Inventory (Theme D)
            NodeLabel::DbColumn,
            NodeLabel::EnvVar,
        ];
        for label in &labels {
            let s = label.as_str();
            assert!(!s.is_empty());
            let parsed = NodeLabel::from_str_label(s).unwrap();
            assert_eq!(*label, parsed);
        }
    }

    #[test]
    fn test_graph_node_serialization() {
        let node = GraphNode {
            id: "Function:src/main.ts:handleLogin".to_string(),
            label: NodeLabel::Function,
            properties: NodeProperties {
                name: "handleLogin".to_string(),
                file_path: "src/main.ts".to_string(),
                start_line: Some(10),
                end_line: Some(25),
                is_exported: Some(true),
                ..Default::default()
            },
        };
        let json = serde_json::to_string_pretty(&node).unwrap();
        assert!(json.contains("\"handleLogin\""));
        assert!(json.contains("\"Function\""));

        // Round-trip
        let parsed: GraphNode = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.id, node.id);
        assert_eq!(parsed.label, node.label);
        assert_eq!(parsed.properties.name, "handleLogin");
    }

    #[test]
    fn test_graph_relationship_serialization() {
        let rel = GraphRelationship {
            id: "rel-1".to_string(),
            source_id: "Function:a".to_string(),
            target_id: "Function:b".to_string(),
            rel_type: RelationshipType::Calls,
            confidence: 0.95,
            reason: "exact".to_string(),
            step: None,
        };
        let json = serde_json::to_string(&rel).unwrap();
        assert!(json.contains("\"CALLS\""));
        let parsed: GraphRelationship = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.rel_type, RelationshipType::Calls);
    }

    #[test]
    fn test_new_relationship_types_roundtrip() {
        // Theme D new relationship types — verify SCREAMING_SNAKE_CASE
        // serialization and `from_str_type` round-trip.
        let cases = [
            (RelationshipType::HasColumn, "HAS_COLUMN"),
            (RelationshipType::ReferencesTable, "REFERENCES_TABLE"),
            (RelationshipType::UsesEnvVar, "USES_ENV_VAR"),
            (RelationshipType::RepresentedBy, "REPRESENTED_BY"),
            (RelationshipType::HandledBy, "HANDLED_BY"),
        ];
        for (rt, expected) in &cases {
            assert_eq!(rt.as_str(), *expected);
            let parsed = RelationshipType::from_str_type(expected).unwrap();
            assert_eq!(parsed, *rt);
        }
    }

    #[test]
    fn test_new_node_labels_roundtrip() {
        // Theme D new node labels.
        for (label, expected) in [
            (NodeLabel::DbColumn, "DbColumn"),
            (NodeLabel::EnvVar, "EnvVar"),
        ] {
            assert_eq!(label.as_str(), expected);
            assert_eq!(NodeLabel::from_str_label(expected).unwrap(), label);
        }
    }

    #[test]
    fn test_optional_fields_skipped() {
        let props = NodeProperties {
            name: "test".to_string(),
            file_path: "test.ts".to_string(),
            ..Default::default()
        };
        let json = serde_json::to_string(&props).unwrap();
        // Optional None fields should not appear in JSON
        assert!(!json.contains("startLine"));
        assert!(!json.contains("language"));
        assert!(!json.contains("cohesion"));
    }
}

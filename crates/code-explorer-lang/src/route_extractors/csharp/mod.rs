//! ASP.NET MVC 5 / Web API attribute extraction from C# source code.
//!
//! Extracts routing attributes, HTTP method decorators, data annotations,
//! controller/action detection, DbContext/Entity patterns, and Area associations
//! from C# source text using regex-based parsing (no tree-sitter dependency).
//!
//! This module provides the semantic layer needed to promote generic `Class`/`Method`
//! graph nodes into richer ASP.NET-specific types (`Controller`, `ControllerAction`,
//! `ApiEndpoint`, `View`, `ViewModel`, `DbEntity`, `DbContext`, `Area`).

mod ajax;
mod controllers;
mod database;
mod di;
mod external;
mod forms;
mod helpers;
mod services;
mod telerik;
mod tracing;
mod types;
mod views;

// Re-export all public types
pub use types::*;

// Re-export all public functions
pub use ajax::extract_ajax_calls;
pub use controllers::extract_controllers;
pub use database::{extract_db_contexts, extract_entities};
pub use di::extract_di_registrations;
pub use external::extract_external_service_calls;
pub use forms::{extract_form_actions, extract_partial_references};
pub use services::{extract_constructor_dependencies, extract_services_and_repositories};
pub use telerik::extract_telerik_components;
pub use tracing::extract_tracing_info;
pub use views::extract_view_info;

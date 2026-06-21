//! Razor (.cshtml / .razor) tree-sitter queries.
//!
//! Razor files are parsed using the C# tree-sitter grammar, so we reuse
//! C# query patterns for extracting classes, methods, properties, etc.
//! that appear inside `@code { }` blocks or inline `@{ }` blocks.
//!
//! Razor-specific directives (`@page`, `@model`, `@inject`, `@using`,
//! `@inherits`, `@implements`) are extracted via regex in the
//! `component_detection` module since they aren't valid C# syntax.

/// Tree-sitter queries for C# symbols found inside Razor files.
///
/// These are a subset of the full C# queries, focused on constructs
/// commonly found in Razor views and Blazor components:
/// - Classes, interfaces, records (from `@code` blocks)
/// - Methods, properties, constructors
/// - Using directives
/// - Calls (including dependency-injected service calls)
/// - Heritage (base classes, interfaces)
pub const QUERIES: &str = r#"
; ── Types (from @code blocks) ─────────────────────────────────────────
(class_declaration name: (identifier) @name) @definition.class
(interface_declaration name: (identifier) @name) @definition.interface
(struct_declaration name: (identifier) @name) @definition.struct
(enum_declaration name: (identifier) @name) @definition.enum
(record_declaration name: (identifier) @name) @definition.record

; ── Methods & Properties ──────────────────────────────────────────────
(method_declaration name: (identifier) @name) @definition.method
(local_function_statement name: (identifier) @name) @definition.function
(constructor_declaration name: (identifier) @name) @definition.constructor
(property_declaration name: (identifier) @name) @definition.property

; ── Using directives ──────────────────────────────────────────────────
(using_directive (qualified_name) @import.source) @import
(using_directive (identifier) @import.source) @import

; ── Calls ─────────────────────────────────────────────────────────────
(invocation_expression function: (identifier) @call.name) @call
(invocation_expression function: (member_access_expression name: (identifier) @call.name)) @call

; Null-conditional calls: service?.Method()
(invocation_expression
  function: (conditional_access_expression
    (member_binding_expression
      (identifier) @call.name))) @call

; Constructor calls: new Component()
(object_creation_expression type: (identifier) @call.name) @call

; ── Heritage ──────────────────────────────────────────────────────────
(class_declaration name: (identifier) @heritage.class
  (base_list (identifier) @heritage.extends)) @heritage
(class_declaration name: (identifier) @heritage.class
  (base_list (generic_name (identifier) @heritage.extends))) @heritage

; ── Assignments ───────────────────────────────────────────────────────
(assignment_expression
  left: (member_access_expression
    expression: (_) @assignment.receiver
    name: (identifier) @assignment.property)
  right: (_)) @assignment
"#;

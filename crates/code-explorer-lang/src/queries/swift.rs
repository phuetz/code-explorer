pub const QUERIES: &str = r#"
; Classes
(class_declaration "class" name: (type_identifier) @name) @definition.class

; Structs
(class_declaration "struct" name: (type_identifier) @name) @definition.struct

; Enums
(class_declaration "enum" name: (type_identifier) @name) @definition.enum

; Extensions (mapped to class — no dedicated label in schema)
(class_declaration "extension" name: (user_type (type_identifier) @name)) @definition.class

; Actors
(class_declaration "actor" name: (type_identifier) @name) @definition.class

; Protocols (mapped to interface)
(protocol_declaration name: (type_identifier) @name) @definition.interface

; Type aliases
(typealias_declaration name: (type_identifier) @name) @definition.type

; Functions (top-level and methods)
(function_declaration name: (simple_identifier) @name) @definition.function

; Protocol method declarations
(protocol_function_declaration name: (simple_identifier) @name) @definition.method

; Initializers
(init_declaration) @definition.constructor

; Properties (stored and computed)
(property_declaration (pattern (simple_identifier) @name)) @definition.property

; Enum cases
(enum_entry (simple_identifier) @name) @definition.property

; Imports
(import_declaration (identifier (simple_identifier) @import.source)) @import

; Calls - direct function calls
(call_expression (simple_identifier) @call.name) @call

; Calls - member/navigation calls (obj.method())
(call_expression (navigation_expression (navigation_suffix (simple_identifier) @call.name))) @call

; Heritage - class/struct/enum inheritance and protocol conformance
(class_declaration name: (type_identifier) @heritage.class
  (inheritance_specifier inherits_from: (user_type (type_identifier) @heritage.extends))) @heritage

; Heritage - protocol inheritance
(protocol_declaration name: (type_identifier) @heritage.class
  (inheritance_specifier inherits_from: (user_type (type_identifier) @heritage.extends))) @heritage

; Heritage - extension protocol conformance (e.g. extension Foo: SomeProtocol)
; Extensions wrap the name in user_type unlike class/struct/enum declarations
(class_declaration "extension" name: (user_type (type_identifier) @heritage.class)
  (inheritance_specifier inherits_from: (user_type (type_identifier) @heritage.extends))) @heritage

; Write access: obj.field = value (tree-sitter-swift 0.7.1 uses named fields)
(assignment
  target: (directly_assignable_expression
    (navigation_expression
      target: (_) @assignment.receiver
      suffix: (navigation_suffix
        suffix: (simple_identifier) @assignment.property)))
  result: (_)) @assignment
"#;

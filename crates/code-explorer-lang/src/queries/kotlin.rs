pub const QUERIES: &str = r#"
; Grammar: tree-sitter-kotlin-ng (1.x). Names use the `name:` field and `identifier`
; (NOT `type_identifier`/`simple_identifier`, which the older fwcd grammar used and which
; do not exist here — referencing them rejects the whole query).

; ── Classes / interfaces / objects ───────────────────────────────────────────
; class_declaration covers class, interface (anonymous `interface` keyword) and enum
; class; object_declaration covers singletons/companion objects.
(class_declaration
  name: (identifier) @name) @definition.class

(object_declaration
  name: (identifier) @name) @definition.class

; ── Functions / methods ──────────────────────────────────────────────────────
(function_declaration
  name: (identifier) @name) @definition.function

; ── Properties (val/var) ─────────────────────────────────────────────────────
(property_declaration
  (variable_declaration
    (identifier) @name)) @definition.property

; ── Enum entries ─────────────────────────────────────────────────────────────
(enum_entry
  (identifier) @name) @definition.enum

; ── Imports ──────────────────────────────────────────────────────────────────
(import
  (qualified_identifier) @import.source) @import

; ── Calls ────────────────────────────────────────────────────────────────────
; Direct: foo(...)   and constructor calls Foo(...)
(call_expression
  (identifier) @call.name) @call

; Member: receiver.method(...)  — capture the trailing identifier (the method name)
(call_expression
  (navigation_expression
    (_)
    (identifier) @call.name)) @call

; ── Heritage: class Foo : Bar  /  class Foo : Bar() ──────────────────────────
(class_declaration
  name: (identifier) @heritage.class
  (delegation_specifiers
    (delegation_specifier
      (user_type (identifier) @heritage.extends)))) @heritage

(class_declaration
  name: (identifier) @heritage.class
  (delegation_specifiers
    (delegation_specifier
      (constructor_invocation
        (user_type (identifier) @heritage.extends))))) @heritage
"#;

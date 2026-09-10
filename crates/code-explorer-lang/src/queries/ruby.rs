pub const QUERIES: &str = r#"
; ── Modules ──────────────────────────────────────────────────────────────────
(module
  name: (constant) @name) @definition.module

; ── Classes ──────────────────────────────────────────────────────────────────
(class
  name: (constant) @name) @definition.class

; ── Instance methods ─────────────────────────────────────────────────────────
(method
  name: (identifier) @name) @definition.method

; ── Singleton (class-level) methods ──────────────────────────────────────────
(singleton_method
  name: (identifier) @name) @definition.method

; ── All calls (require, include, attr_*, and regular calls routed in JS) ─────
(call
  method: (identifier) @call.name) @call

; ── Bare calls without parens (identifiers at statement level are method calls) ─
; NOTE: This may over-capture variable reads as calls (e.g. 'result' at
; statement level). Ruby's grammar makes bare identifiers ambiguous — they
; could be local variables or zero-arity method calls. Post-processing via
; isBuiltInOrNoise and symbol resolution filtering suppresses most false
; positives, but a variable name that coincidentally matches a method name
; elsewhere may produce a false CALLS edge.
(body_statement
  (identifier) @call.name @call)

; ── Heritage: class < SuperClass ─────────────────────────────────────────────
(class
  name: (constant) @heritage.class
  superclass: (superclass
    (constant) @heritage.extends)) @heritage

; Write access: obj.field = value (Ruby setter — syntactically a method call to field=)
(assignment
  left: (call
    receiver: (_) @assignment.receiver
    method: (identifier) @assignment.property)
  right: (_)) @assignment

; Write access: obj.field += value (compound assignment — operator_assignment node, not assignment)
(operator_assignment
  left: (call
    receiver: (_) @assignment.receiver
    method: (identifier) @assignment.property)
  right: (_)) @assignment
"#;

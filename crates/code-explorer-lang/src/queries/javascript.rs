pub const QUERIES: &str = r#"
(class_declaration
  name: (identifier) @name) @definition.class

(function_declaration
  name: (identifier) @name) @definition.function

(method_definition
  name: (property_identifier) @name) @definition.method

(lexical_declaration
  (variable_declarator
    name: (identifier) @name
    value: (arrow_function))) @definition.function

(lexical_declaration
  (variable_declarator
    name: (identifier) @name
    value: (function_expression))) @definition.function

; Object property functions: { run: () => ... } / { run: function () { ... } }
(pair
  key: (property_identifier) @name
  value: [(arrow_function) (function_expression)]) @definition.function

(pair
  key: (string (string_fragment) @name)
  value: [(arrow_function) (function_expression)]) @definition.function

; React.memo(function Component() { ... }) / forwardRef((props, ref) => ...)
(lexical_declaration
  (variable_declarator
    name: (identifier) @name
    value: (call_expression
      arguments: (arguments
        [(function_expression) (arrow_function)])))) @definition.function

; memo(forwardRef(function Component() { ... }))
(lexical_declaration
  (variable_declarator
    name: (identifier) @name
    value: (call_expression
      arguments: (arguments
        (call_expression
          arguments: (arguments
            [(function_expression) (arrow_function)])))))) @definition.function

; React.memo(Component) / React.forwardRef(Component)
(lexical_declaration
  (variable_declarator
    name: (identifier) @name
    value: (call_expression
      function: (member_expression
        object: (identifier) @_react_obj (#eq? @_react_obj "React")
        property: (property_identifier) @_react_wrapper (#eq? @_react_wrapper "memo"))
      arguments: (arguments (identifier))))) @definition.function

(lexical_declaration
  (variable_declarator
    name: (identifier) @name
    value: (call_expression
      function: (member_expression
        object: (identifier) @_react_obj (#eq? @_react_obj "React")
        property: (property_identifier) @_react_wrapper (#eq? @_react_wrapper "forwardRef"))
      arguments: (arguments (identifier))))) @definition.function

; memo(Component) / forwardRef(Component) when imported directly
(lexical_declaration
  (variable_declarator
    name: (identifier) @name
    value: (call_expression
      function: (identifier) @_react_wrapper (#eq? @_react_wrapper "memo")
      arguments: (arguments (identifier))))) @definition.function

(lexical_declaration
  (variable_declarator
    name: (identifier) @name
    value: (call_expression
      function: (identifier) @_react_wrapper (#eq? @_react_wrapper "forwardRef")
      arguments: (arguments (identifier))))) @definition.function

(export_statement
  declaration: (lexical_declaration
    (variable_declarator
      name: (identifier) @name
      value: (arrow_function)))) @definition.function

(export_statement
  declaration: (lexical_declaration
    (variable_declarator
      name: (identifier) @name
      value: (function_expression)))) @definition.function

(import_statement
  source: (string) @import.source) @import

; Re-export statements: export { X } from './y'
(export_statement
  source: (string) @import.source) @import

; Dynamic imports: import('./module') / await import('./module')
(call_expression
  function: (import)
  arguments: (arguments (string) @import.source)) @import

; CommonJS require: require('./module')
(call_expression
  function: (identifier) @_require_fn (#eq? @_require_fn "require")
  arguments: (arguments (string) @import.source)) @import

(call_expression
  function: (identifier) @call.name) @call

(call_expression
  function: (member_expression
    object: (_) @call.object
    property: (property_identifier) @call.name)) @call

; Constructor calls: new Foo()
(new_expression
  constructor: (identifier) @call.name) @call

; Class fields — field_definition captures JS class fields (class User { address = ... })
(field_definition
  property: (property_identifier) @name) @definition.property

; Heritage queries - class extends (JavaScript uses different AST than TypeScript)
; In tree-sitter-javascript, class_heritage directly contains the parent identifier
(class_declaration
  name: (identifier) @heritage.class
  (class_heritage
    (identifier) @heritage.extends)) @heritage

; Write access: obj.field = value
(assignment_expression
  left: (member_expression
    object: (_) @assignment.receiver
    property: (property_identifier) @assignment.property)
  right: (_)) @assignment

; Write access: obj.field += value (compound assignment)
(augmented_assignment_expression
  left: (member_expression
    object: (_) @assignment.receiver
    property: (property_identifier) @assignment.property)
  right: (_)) @assignment

; HTTP consumers: fetch('/path'), axios.get('/path'), $.get('/path'), etc.
(call_expression
  function: (identifier) @_fetch_fn (#eq? @_fetch_fn "fetch")
  arguments: (arguments
    [(string (string_fragment) @route.url)
     (template_string) @route.template_url])) @route.fetch

; axios.get/post, $.get/post/ajax
(call_expression
  function: (member_expression
    property: (property_identifier) @http_client.method)
  arguments: (arguments
    (string (string_fragment) @http_client.url))) @http_client

; Express/Hono route registration
(call_expression
  function: (member_expression
    property: (property_identifier) @express_route.method)
  arguments: (arguments
    (string (string_fragment) @express_route.path))) @express_route
"#;

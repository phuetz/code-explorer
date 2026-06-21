pub const QUERIES: &str = r#"
(class_declaration
  name: (type_identifier) @name) @definition.class

(interface_declaration
  name: (type_identifier) @name) @definition.interface

(type_alias_declaration
  name: (type_identifier) @name) @definition.type

(enum_declaration
  name: (identifier) @name) @definition.enum

(function_declaration
  name: (identifier) @name) @definition.function

; TypeScript overload signatures (function_signature is a separate node type from function_declaration)
(function_signature
  name: (identifier) @name) @definition.function

(method_definition
  name: (property_identifier) @name) @definition.method

; Class field functions: `run = () => ...` / `run = function () { ... }`
; They behave like callable instance methods for graph navigation.
(public_field_definition
  name: (property_identifier) @name
  value: [(arrow_function) (function_expression)]) @definition.method

(public_field_definition
  name: (private_property_identifier) @name
  value: [(arrow_function) (function_expression)]) @definition.method

; Interface members. Keep this scoped to interface bodies so inline object types
; in parameters do not explode the graph with thousands of unowned properties.
(interface_declaration
  (interface_body
    (method_signature
      name: (property_identifier) @name))) @definition.method

(interface_declaration
  (interface_body
    (property_signature
      name: (property_identifier) @name))) @definition.property

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

; Non-null assertion calls: cb!() / service.run!()
(call_expression
  function: (non_null_expression
    (identifier) @call.name)) @call

(call_expression
  function: (non_null_expression
    (member_expression
      object: (_) @call.object
      property: (property_identifier) @call.name))) @call

; Constructor calls: new Foo()
(new_expression
  constructor: (identifier) @call.name) @call

; Class properties — public_field_definition covers most TS class fields
(public_field_definition
  name: (property_identifier) @name) @definition.property

; Private class fields: #address: Address
(public_field_definition
  name: (private_property_identifier) @name) @definition.property

; Constructor parameter properties: constructor(public address: Address)
(required_parameter
  (accessibility_modifier)
  pattern: (identifier) @name) @definition.property

; Heritage queries - class extends
(class_declaration
  name: (type_identifier) @heritage.class
  (class_heritage
    (extends_clause
      value: (identifier) @heritage.extends))) @heritage

; Heritage queries - class implements interface
(class_declaration
  name: (type_identifier) @heritage.class
  (class_heritage
    (implements_clause
      (type_identifier) @heritage.implements))) @heritage.impl

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
; fetch() — global function
(call_expression
  function: (identifier) @_fetch_fn (#eq? @_fetch_fn "fetch")
  arguments: (arguments
    [(string (string_fragment) @route.url)
     (template_string) @route.template_url])) @route.fetch

; axios.get/post/put/delete/patch('/path'), $.get/post/ajax({url:'/path'})
(call_expression
  function: (member_expression
    property: (property_identifier) @http_client.method)
  arguments: (arguments
    (string (string_fragment) @http_client.url))) @http_client

; Decorators: @Controller, @Get, @Post, etc.
(decorator
  (call_expression
    function: (identifier) @decorator.name
    arguments: (arguments (string (string_fragment) @decorator.arg)?))) @decorator

; Express/Hono route registration: app.get('/path', handler), router.post('/path', fn)
(call_expression
  function: (member_expression
    property: (property_identifier) @express_route.method)
  arguments: (arguments
    (string (string_fragment) @express_route.path))) @express_route
"#;

; Function definitions
(function_declaration
  name: (identifier) @def.function)

(method_declaration
  name: (field_identifier) @def.method)

; Type definitions
(type_spec
  name: (type_identifier) @def.type)

; Call expressions
(call_expression
  function: (identifier) @ref.call)

(call_expression
  function: (selector_expression
    operand: (_) @ref.call.receiver
    field: (field_identifier) @ref.call))

; Import paths
(import_spec
  path: (interpreted_string_literal) @import)

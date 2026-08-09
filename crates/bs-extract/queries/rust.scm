; Function definitions
(function_item
  name: (identifier) @def.function)

; Method definitions (inside impl blocks)
(impl_item
  body: (declaration_list
    (function_item
      name: (identifier) @def.method)))

; Type definitions
(struct_item name: (type_identifier) @def.type)
(enum_item name: (type_identifier) @def.type)
(trait_item name: (type_identifier) @def.type)
(type_item name: (type_identifier) @def.type)

; Call expressions
(call_expression
  function: (identifier) @ref.call)

(call_expression
  function: (field_expression
    value: (_) @ref.call.receiver
    field: (field_identifier) @ref.call))

(call_expression
  function: (scoped_identifier
    name: (identifier) @ref.call))

; Use declarations
(use_declaration
  argument: (scoped_identifier
    path: (_) @import))

(use_declaration
  argument: (scoped_use_list
    path: (_) @import))

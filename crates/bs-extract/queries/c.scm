; Function definitions
(function_definition
  declarator: (function_declarator
    declarator: (identifier) @def.function))

; Function declarations (prototypes)
(declaration
  declarator: (function_declarator
    declarator: (identifier) @def.function))

; Struct/union/enum type definitions
(type_definition
  declarator: (type_identifier) @def.type)

(struct_specifier
  name: (type_identifier) @def.type)

; Call expressions
(call_expression
  function: (identifier) @ref.call)

(call_expression
  function: (field_expression
    value: (_) @ref.call.receiver
    field: (field_identifier) @ref.call))

; Include directives
(preproc_include
  path: [(string_literal) (system_lib_string)] @import)

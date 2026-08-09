; Function definitions
(function_definition
  declarator: (function_declarator
    declarator: (identifier) @def.function))

; Method definitions
(function_definition
  declarator: (function_declarator
    declarator: (qualified_identifier
      name: (identifier) @def.method)))

; Class declarations
(class_specifier
  name: (type_identifier) @def.type)

(struct_specifier
  name: (type_identifier) @def.type)

; Call expressions
(call_expression
  function: (identifier) @ref.call)

(call_expression
  function: (qualified_identifier
    name: (identifier) @ref.call))

(call_expression
  function: (field_expression
    argument: (_) @ref.call.receiver
    field: (field_identifier) @ref.call))

; Include directives
(preproc_include
  path: [(string_literal) (system_lib_string)] @import)

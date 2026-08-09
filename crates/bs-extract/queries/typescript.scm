; Function declarations
(function_declaration
  name: (identifier) @def.function)

; Arrow functions
(lexical_declaration
  (variable_declarator
    name: (identifier) @def.function
    value: [(arrow_function) (function_expression)]))

; Method definitions
(method_definition
  name: (property_identifier) @def.method)

; Class declarations
(class_declaration
  name: (type_identifier) @def.type)

; Interface declarations
(interface_declaration
  name: (type_identifier) @def.type)

; Type alias declarations
(type_alias_declaration
  name: (type_identifier) @def.type)

; Call expressions
(call_expression
  function: (identifier) @ref.call)

(call_expression
  function: (member_expression
    object: (_) @ref.call.receiver
    property: (property_identifier) @ref.call))

; Import declarations
(import_statement
  source: (string) @import)

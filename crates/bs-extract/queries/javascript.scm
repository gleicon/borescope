; Function declarations
(function_declaration
  name: (identifier) @def.function)

; Arrow functions assigned to variables
(lexical_declaration
  (variable_declarator
    name: (identifier) @def.function
    value: [(arrow_function) (function_expression)]))

; Method definitions in classes
(method_definition
  name: (property_identifier) @def.method)

; Class declarations
(class_declaration
  name: (identifier) @def.type)

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

(import_declaration
  source: (string) @import)

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

; --- Semantic pattern captures ---

(await_expression) @pattern.await
(new_expression) @pattern.alloc

(call_expression
  function: (identifier) @pattern.timer
  (#match? @pattern.timer "^(setTimeout|setInterval|queueMicrotask)$"))

(call_expression
  function: (member_expression
    property: (property_identifier) @pattern.spawn)
  (#match? @pattern.spawn "^(spawn|fork|exec|Worker)$"))

(for_statement) @pattern.loop
(for_in_statement) @pattern.loop
(while_statement) @pattern.loop

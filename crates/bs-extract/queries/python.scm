; Function definitions
(function_definition
  name: (identifier) @def.function)

; Method definitions (inside class body)
(class_definition
  body: (block
    (function_definition
      name: (identifier) @def.method)))

; Class definitions
(class_definition
  name: (identifier) @def.type)

; Call expressions
(call
  function: (identifier) @ref.call)

(call
  function: (attribute
    object: (_) @ref.call.receiver
    attribute: (identifier) @ref.call))

; Import statements
(import_statement
  name: (dotted_name) @import)

(import_from_statement
  module_name: (dotted_name) @import)

(import_from_statement
  module_name: (relative_import) @import)

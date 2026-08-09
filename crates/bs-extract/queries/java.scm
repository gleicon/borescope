; Method declarations
(method_declaration
  name: (identifier) @def.method)

; Constructor declarations
(constructor_declaration
  name: (identifier) @def.method)

; Class declarations
(class_declaration
  name: (identifier) @def.type)

; Interface declarations
(interface_declaration
  name: (identifier) @def.type)

; Call expressions
(method_invocation
  name: (identifier) @ref.call)

(method_invocation
  object: (_) @ref.call.receiver
  name: (identifier) @ref.call)

; Import declarations
(import_declaration
  (scoped_identifier) @import)

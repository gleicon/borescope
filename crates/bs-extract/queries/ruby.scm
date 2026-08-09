; Method definitions
(method
  name: (identifier) @def.method)

; Singleton method definitions
(singleton_method
  name: (identifier) @def.method)

; Class definitions
(class
  name: [(constant) (scope_resolution)] @def.type)

; Module definitions
(module
  name: [(constant) (scope_resolution)] @def.type)

; Call expressions
(call
  method: (identifier) @ref.call)

(call
  receiver: (_) @ref.call.receiver
  method: (identifier) @ref.call)

; Require statements
(call
  method: (identifier) @_require
  arguments: (argument_list
    (string
      (string_content) @import))
  (#match? @_require "^require"))

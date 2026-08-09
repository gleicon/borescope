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

; --- Semantic pattern captures ---

; Thread spawn
(call
  method: (identifier) @pattern.spawn
  (#match? @pattern.spawn "^(new|start)$")
  receiver: (constant) @_cls
  (#match? @_cls "^Thread$"))

; Mutex lock
(call
  method: (identifier) @pattern.lock
  (#match? @pattern.lock "^(lock|synchronize|mon_enter)$"))

; Allocating methods
(call
  method: (identifier) @pattern.alloc
  (#match? @pattern.alloc "^(dup|clone|flatten|map|select|reject|collect)$"))

; Loop constructs
(while_modifier) @pattern.loop
(until_modifier) @pattern.loop
(for) @pattern.loop

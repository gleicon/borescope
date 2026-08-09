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

; --- Semantic pattern captures ---

; Synchronized block (lock)
(synchronized_statement) @pattern.lock

; Lock method calls
(method_invocation
  name: (identifier) @pattern.lock
  (#match? @pattern.lock "^(lock|tryLock|acquire)$"))

; Thread/executor spawn
(object_creation_expression
  type: (type_identifier) @pattern.spawn
  (#match? @pattern.spawn "^(Thread|FutureTask|Callable|Runnable)$"))
(method_invocation
  name: (identifier) @pattern.spawn
  (#match? @pattern.spawn "^(submit|execute|fork|start)$"))

; Allocating calls
(method_invocation
  name: (identifier) @pattern.alloc
  (#match? @pattern.alloc "^(toString|clone|format|valueOf|copyOf)$"))
(object_creation_expression) @pattern.alloc

; Loop constructs
(for_statement) @pattern.loop
(enhanced_for_statement) @pattern.loop
(while_statement) @pattern.loop
(do_statement) @pattern.loop

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

; --- Semantic pattern captures ---

; Await expressions
(await) @pattern.await

; Async blocking / run-in-executor patterns
(call
  function: (attribute attribute: (identifier) @pattern.block_on)
  (#match? @pattern.block_on "^(run_until_complete|run|run_sync)$"))

; Thread/process/executor spawn
(call
  function: (attribute attribute: (identifier) @pattern.spawn)
  (#match? @pattern.spawn "^(Thread|Process|submit|apply_async|start|run_in_executor)$"))
(call
  function: (identifier) @pattern.spawn
  (#match? @pattern.spawn "^(Thread|Process)$"))

; Lock acquisition
(call
  function: (attribute attribute: (identifier) @pattern.lock)
  (#match? @pattern.lock "^(acquire|lock)$"))

; Allocating calls
(call
  function: (identifier) @pattern.alloc
  (#match? @pattern.alloc "^(list|dict|set|tuple|copy|deepcopy)$"))

; Loop constructs
(for_statement) @pattern.loop
(while_statement) @pattern.loop

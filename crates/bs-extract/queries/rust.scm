; Function definitions
(function_item
  name: (identifier) @def.function)

; Method definitions (inside impl blocks)
(impl_item
  body: (declaration_list
    (function_item
      name: (identifier) @def.method)))

; Type definitions
(struct_item name: (type_identifier) @def.type)
(enum_item name: (type_identifier) @def.type)
(trait_item name: (type_identifier) @def.type)
(type_item name: (type_identifier) @def.type)

; Call expressions
(call_expression
  function: (identifier) @ref.call)

(call_expression
  function: (field_expression
    value: (_) @ref.call.receiver
    field: (field_identifier) @ref.call))

(call_expression
  function: (scoped_identifier
    name: (identifier) @ref.call))

; Use declarations
(use_declaration
  argument: (scoped_identifier
    path: (_) @import))

(use_declaration
  argument: (scoped_use_list
    path: (_) @import))

; --- Semantic pattern captures ---

; Allocating method calls (.to_string(), .clone(), .collect(), etc.)
(call_expression
  function: (field_expression
    field: (field_identifier) @pattern.alloc)
  (#match? @pattern.alloc "^(to_string|to_owned|clone|collect|into_owned|to_vec|format)$"))

; Lock acquisition (Mutex, RwLock)
(call_expression
  function: (field_expression
    field: (field_identifier) @pattern.lock)
  (#match? @pattern.lock "^(lock|write|read|try_lock|try_write|try_read)$"))

; Await expressions — yield point
(await_expression) @pattern.await

; block_on — blocking inside async context
(call_expression
  function: (field_expression
    field: (field_identifier) @pattern.block_on)
  (#match? @pattern.block_on "^block_on$"))

; Task spawn
(call_expression
  function: (scoped_identifier
    name: (identifier) @pattern.spawn)
  (#match? @pattern.spawn "^(spawn|spawn_blocking|spawn_local)$"))

; Loop constructs
(loop_expression) @pattern.loop
(while_expression) @pattern.loop
(for_expression) @pattern.loop

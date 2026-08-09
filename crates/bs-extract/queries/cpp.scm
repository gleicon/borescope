; Function definitions
(function_definition
  declarator: (function_declarator
    declarator: (identifier) @def.function))

; Method definitions
(function_definition
  declarator: (function_declarator
    declarator: (qualified_identifier
      name: (identifier) @def.method)))

; Class declarations
(class_specifier
  name: (type_identifier) @def.type)

(struct_specifier
  name: (type_identifier) @def.type)

; Call expressions
(call_expression
  function: (identifier) @ref.call)

(call_expression
  function: (qualified_identifier
    name: (identifier) @ref.call))

(call_expression
  function: (field_expression
    argument: (_) @ref.call.receiver
    field: (field_identifier) @ref.call))

; Include directives
(preproc_include
  path: [(string_literal) (system_lib_string)] @import)

; --- Semantic pattern captures ---

; Mutex/lock_guard lock
(call_expression
  function: [(identifier)(field_expression field: _) @pattern.lock]
  (#match? @pattern.lock "^(lock|try_lock|acquire)$"))
(declaration
  type: (type_identifier) @pattern.lock
  (#match? @pattern.lock "^(lock_guard|unique_lock|scoped_lock|shared_lock)$"))

; Allocation (new expression)
(new_expression) @pattern.alloc
(call_expression
  function: (identifier) @pattern.alloc
  (#match? @pattern.alloc "^(malloc|calloc|realloc|make_shared|make_unique)$"))

; Thread spawn
(call_expression
  function: [(identifier)(qualified_identifier name: (identifier)) @pattern.spawn]
  (#match? @pattern.spawn "^(thread|async|spawn|CreateThread)$"))

; Await (co_await)
(co_await_expression) @pattern.await

; Loop constructs
(for_statement) @pattern.loop
(for_range_loop) @pattern.loop
(while_statement) @pattern.loop
(do_statement) @pattern.loop

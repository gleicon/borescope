; Function definitions
(function_definition
  declarator: (function_declarator
    declarator: (identifier) @def.function))

; Function declarations (prototypes)
(declaration
  declarator: (function_declarator
    declarator: (identifier) @def.function))

; Struct/union/enum type definitions
(type_definition
  declarator: (type_identifier) @def.type)

(struct_specifier
  name: (type_identifier) @def.type)

; Call expressions
(call_expression
  function: (identifier) @ref.call)

(call_expression
  function: (field_expression
    value: (_) @ref.call.receiver
    field: (field_identifier) @ref.call))

; Include directives
(preproc_include
  path: [(string_literal) (system_lib_string)] @import)

; --- Semantic pattern captures ---

; Mutex lock
(call_expression
  function: (identifier) @pattern.lock
  (#match? @pattern.lock "^(pthread_mutex_lock|pthread_rwlock_rdlock|pthread_rwlock_wrlock|sem_wait|WaitForSingleObject)$"))

; Allocation
(call_expression
  function: (identifier) @pattern.alloc
  (#match? @pattern.alloc "^(malloc|calloc|realloc|strdup|strndup)$"))

; Thread spawn
(call_expression
  function: (identifier) @pattern.spawn
  (#match? @pattern.spawn "^(pthread_create|CreateThread|fork)$"))

; Loop constructs
(for_statement) @pattern.loop
(while_statement) @pattern.loop
(do_statement) @pattern.loop

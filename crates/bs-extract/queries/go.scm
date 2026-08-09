; Function definitions
(function_declaration
  name: (identifier) @def.function)

(method_declaration
  name: (field_identifier) @def.method)

; Type definitions
(type_spec
  name: (type_identifier) @def.type)

; Call expressions
(call_expression
  function: (identifier) @ref.call)

(call_expression
  function: (selector_expression
    operand: (_) @ref.call.receiver
    field: (field_identifier) @ref.call))

; Import paths
(import_spec
  path: (interpreted_string_literal) @import)

; --- Semantic pattern captures ---

; Goroutine spawn
(go_statement) @pattern.spawn

; Mutex/RwMutex lock
(call_expression
  function: (selector_expression
    field: (field_identifier) @pattern.lock)
  (#match? @pattern.lock "^(Lock|RLock|TryLock|TryRLock)$"))

; Allocating builtins
(call_expression
  function: (identifier) @pattern.alloc
  (#match? @pattern.alloc "^(make|new|append)$"))

; Allocating method calls
(call_expression
  function: (selector_expression
    field: (field_identifier) @pattern.alloc)
  (#match? @pattern.alloc "^(Sprintf|Errorf|Marshal|Unmarshal|Clone)$"))

; Channel send/recv
(send_statement) @pattern.chan
(receive_statement) @pattern.chan

; Loop constructs
(for_statement) @pattern.loop

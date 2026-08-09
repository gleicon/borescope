; Function declarations
(function_declaration
  name: (identifier) @def.function)

; Arrow functions assigned to variables
(lexical_declaration
  (variable_declarator
    name: (identifier) @def.function
    value: [(arrow_function) (function_expression)]))

; Method definitions in classes
(method_definition
  name: (property_identifier) @def.method)

; Class declarations
(class_declaration
  name: (identifier) @def.type)

; Call expressions
(call_expression
  function: (identifier) @ref.call)

(call_expression
  function: (member_expression
    object: (_) @ref.call.receiver
    property: (property_identifier) @ref.call))

; Import declarations
(import_statement
  source: (string) @import)

(import_declaration
  source: (string) @import)

; --- Semantic pattern captures ---

; Await expression
(await_expression) @pattern.await

; new expressions (allocation)
(new_expression) @pattern.alloc

; Timers (potential ordering issues)
(call_expression
  function: (identifier) @pattern.timer
  (#match? @pattern.timer "^(setTimeout|setInterval|queueMicrotask)$"))

; Worker / child_process spawn
(call_expression
  function: (member_expression
    property: (property_identifier) @pattern.spawn)
  (#match? @pattern.spawn "^(spawn|fork|exec|execSync|Worker)$"))
(call_expression
  function: (identifier) @pattern.spawn
  (#match? @pattern.spawn "^(Worker)$"))

; Loop constructs
(for_statement) @pattern.loop
(for_in_statement) @pattern.loop
(while_statement) @pattern.loop
(do_statement) @pattern.loop

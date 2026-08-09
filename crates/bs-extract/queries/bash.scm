; Function definitions
(function_definition
  name: (word) @def.function)

; Command calls
(command
  name: (command_name
    (word) @ref.call))

; Source statements
(command
  name: (command_name
    (word) @_source)
  argument: (word) @import
  (#match? @_source "^(source|\\.)$"))

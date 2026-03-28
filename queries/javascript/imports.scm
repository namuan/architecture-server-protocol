(import_declaration
  source: (string) @source)

(call_expression
  function: (identifier) @func (#eq? @func "require")
  arguments: (arguments (string) @source))

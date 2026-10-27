; Syntax highlighting for Cadastre (.ca, .cav)

(comment) @comment

; The clauses of a check written as keys: before the keys of the registry, the first pattern wins
(check (attribute key: (identifier) @keyword))
(check (attribute value: (identifier) @constant.builtin))

; Registry
(resource kind: (identifier) @type)
(resource name: (identifier) @variable)
(attribute key: (identifier) @property)
(port_spec protocol: (identifier) @constant.builtin)
(port) @number
(layers (identifier) @label)

; Schema of the project
(kind_declaration name: (identifier) @type)
(group_declaration name: (identifier) @type)
(csv "csv" @function.builtin)
(group_declaration member: (identifier) @type)
(link_declaration layer: (identifier) @label)
(key_declaration key: (identifier) @property)
(type name: (identifier) @type)
(kinds (identifier) @type)
(choices (identifier) @constant)
(cardinality) @operator
(required) @keyword

; Values
(string) @string
(number) @number
(quantity) @number
(semver) @number
[(ipv4) (cidr4) (ipv6) (cidr6)] @string.special
(qualified_name) @variable

; Plans
(view name: (identifier) @function)
(check name: (identifier) @function)
(process name: (identifier) @function)
(participant name: (identifier) @variable)
(step_block flag: (identifier) @constant.builtin)
(process_selector name: (identifier) @variable)
(path_selector from: (_) @variable to: (_) @variable)
(aggregate function: (identifier) @function)
(call function: (identifier) @function)
[(none) (some)] @constant.builtin
(select_clause kind: (identifier) @type)
(group_by level: (identifier) @type)
(ancestor kind: (identifier) @type)
(right_path kind: (identifier) @type)
(layer_style layer: (identifier) @label)
(kind_style kind: (identifier) @type)
(lift_option key: (identifier) @property)
(follow_option key: (identifier) @property)
(edges_option key: (identifier) @property)
(render_clause format: (identifier) @constant)
(zone_clause name: (identifier) @type)
(optional_marker) @operator

[
  "for_each" "view" "select" "zone" "where" "group-by" "edges" "lift" "follow"
  "style" "render" "layer" "kind" "nodes" "ancestor" "exists"
  "with" "ends" "without" "isolated" "check" "expect" "count" "all" "ref" "pattern" "link" "process" "participant" "step" "loop" "virtual"
] @keyword

["and" "or" "not" "has" "in"] @keyword.operator

["->" "=" "!=" "<" "<=" ">" ">=" "/" "|"] @operator

["{" "}" "[" "]" "(" ")"] @punctuation.bracket
["," ":" "."] @punctuation.delimiter

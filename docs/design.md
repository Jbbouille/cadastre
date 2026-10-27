# Design

The decisions behind the language and the compiler, in short. The [guide](guide.md) details each of them, with the alternatives considered.

## Decisions

| Topic | Decision |
|---|---|
| Model | Typed graph with three relations: containment (nesting), references (typed attributes), links (`->`) |
| Separation | Registry (`.ca`, facts), plans (`.cav`, views and styles) and checks (`.cac`, the tests) are distinct files; the compiler rejects a block in the wrong file |
| Project | A directory: all `.ca` files merged form the registry, every `view` of the `.cav` files is evaluated against it |
| Authoring | By hand (human or agent); machine generation possible later, without changing the model |
| Syntax | Dedicated, brace-based, whitespace-insensitive, optional comma between elements, `//` and `/* */` comments |
| Resource kinds | Not keywords: generic `<kind> <name> { … }` rule, validated by a schema built into the compiler in v1 (kinds, allowed parents, unions, groups, cardinalities) |
| Schema of the project | `kind`, `link` and `layer` blocks type the keys of the project and declare kinds `like` a built-in one; a key enters the language only when the compiler uses it (placement, views, checks, renderings, exports), a descriptive key (`platform`, `technology`, `protocol`, `population`, `role`) belongs to the schema of the project |
| Attributes | Always named, no positional arguments; lists `[a, b]`; open schema: free keys accepted, typos reported, `--strict` to reject everything else |
| Literals | Identifiers (hyphens allowed), qualified names, numbers, semver, IPv4/IPv6, CIDR, `proto/port`, strings; raw text is kept |
| Hierarchy | A single parent; two trees: infrastructure (site → vpc / network → subnet → server) and functional (system → app, job, database, saas); technical placement goes through `on`; any second hierarchy (rack, interfaces, server membership in a system) goes through a reference |
| Links | Full form at the root, `-> target` form in a block; outgoing only (connection direction), one port per link; optional attributes (`description`, `carries`, `exchange`, `channel`, `through`…) |
| Link identity | (source, target, `proto/port`, channels); a duplicate is an error |
| Names | Any unambiguous suffix of the path (`web01`, `dmz.web01`, full path); lexical scope by system (`db` first designates the enclosing system's); candidates filtered by expected kind; ambiguity reported with the minimal suffix |
| Functional | Nestable systems, apps, scheduled jobs (`schedule`, `scheduler`), databases, SaaS, components |
| Data | `data` business objects, referenced by `carries` (flows, channels) and `stores` (databases, SaaS); data direction with `exchange: push / pull / both`; exchange channels (topics, queues, directories, buckets) |
| Infrastructure | Sites (`on-prem`, `colocation`, `cloud`, `office`), racks, Kubernetes clusters, workstation fleets, network devices with interfaces, connectors (VPN tunnels, peering, dedicated links), actors (populations, never individuals) |
| Responsibility | Teams (`team`, never people); `owner` (accountable) and `operator` (operations, providers included), inherited through containment; a missing `owner` is incomplete |
| Partial knowledge | An imprecise model is accepted, a wrong model is rejected: missing required attributes and approximate placements are reported as "incomplete"; `confidence` (`confirmed` by default, `assumed`, `doubtful`), `question`, `scope: external` |
| Traversals | Explicit `through` on flows (firewalls, tunnels, NAT, egress proxy); no routing-based deduction in v1 |
| Link layers | Free labels; `flow`, `auth` and `trigger` are standard |
| Views | `select`, `where`, `group-by` (nested), `zone`, `edges`, `lift`, `follow`, `through: show`, `style`, `render` (connection or data orientation, layout hints) |
| `lift` | Lifts any edge (flow, reference, connector) along a path (`on`, `on?.ancestor(subnet)`, `ancestor(system)`); loops dropped and parallel edges merged by default, provenance kept |
| `follow` | Follows a business object along the links that carry it (directly or through a channel), in the direction of the data, from a starting point; numbers the steps. End-to-end flows are derived, never declared |
| Rules (Datalog) | Left out of v1. `lift` and `where` cover the need; a simple `check` operator may come before any rule engine |
| Parsing | Tree-sitter: one grammar for both extensions, shared by the compiler, the formatter, syntax highlighting and the language server, with error recovery |
| Layout | No home-made engine: DOT generation (Graphviz), then SVG; default theme, cascading styles by kind, layer or filter, with a legend |
| Targets | The compiler core has no system access, so it runs natively (CLI) and in the browser (WebAssembly) |

## Architecture

```
.ca / .cav / .cac ──► Tree-sitter ──► syntax tree ──► registry ──► name resolution
                                                                  │
                                        ┌─────────────────────────┼──────────────────┐
                                 validation (schema,         projection per view    export
                                 consistency checks)        (select, where,         (JSON…)
                                                             group-by, edges, lift)
                                                                  │
                                                      renderings: DOT → SVG · text · Mermaid · ASCII art
```

The core (`cadastre-core`) has no system access: it takes texts and returns diagnostics and renderings, so that the command line, the language server and the WebAssembly module share it.

## Rejected options

- **HCL**: good for resources, verbose for relations, with no query language for views.
- **Datalog**: powerful for rules and projections, but unnecessary as long as `lift` and `where` are enough.
- **BPMN output**: BPMN models processes (tasks, events, gateways), which Cadastre does not describe.
- **Resource kinds as keywords**: every new kind would require changing the grammar.
- **Positional arguments** (`subnet dmz 10.0.1.0/24`): ambiguous without significant whitespace.
- **`.vue` extension**: already taken by Vue.js (editors, GitHub, front-end tools).
- **Several parents**: replaced by references (rack, interfaces), which cover the physical and network hierarchies without breaking the tree.
- **Reopening a resource from another file** (`extend dc-alpha.lan.servers { … }`, like open classes or partial classes): its definition would be scattered over files, and nothing in the file of the resource would tell that it is completed elsewhere. A resource names its parent instead (`parent:`), composition rather than reopening: everything about a resource stays in its declaration.
- **Name resolution by network proximity**: lexical scope by system and kind filtering are more predictable.
- **`#` comments and documentation comments**: a single comment syntax; a resource is documented with `description:`.
- **Object values** (`framework: { … }`): replaced by child resources.
- **Individuals in the registry**: actors are populations or roles.

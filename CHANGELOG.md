# Changelog

Every version of Cadastre, newest first. The [roadmap](ROADMAP.md) lists what each feature does in detail; this file says what changes for a project when it upgrades.

## Versions

Cadastre is in **0.x**. Until 1.0:

- a **minor** version (0.1 → 0.2) may change the language: a syntax, a key, a default. The old form is reported by the compiler with its replacement, for at least one minor version, so that a project is migrated by following its diagnostics;
- a **patch** version (0.1.0 → 0.1.1) fixes without changing the language or the renderings on purpose;
- what is **experimental** (see [Stability](README.md#stability)) may change or go in any version.

The compiler, the VS Code extension, the IntelliJ plugin and the WebAssembly module share one version number.

## Unreleased

## 0.1.1

Nothing changes for a project: the same language and renderings, published in more places.

- Releases publish the crates on crates.io (`cargo install cadastre`: the package of the command is now named `cadastre`, formerly `cadastre-cli`) and the VS Code extension on the Visual Studio Marketplace and Open VSX.

## 0.1.0

The first public version: the language and the compiler as described by the [guide](docs/guide.md) and the [reference](docs/reference.md).

- **The registry**: 35 kinds of resources (sites, networks, subnets, machines, clusters, pools, network devices, connectors, systems, apps, jobs, databases, SaaS, channels, data, teams, actors, processes), kinds within a resource (interfaces, components, channels, participants), typed keys, nesting, references, links with protocol, port, layers and data, `for_each` from lists and CSV files, partial knowledge (`confidence`, `question`, incomplete elements).
- **The schema of the project**: typed keys added to the built-in kinds, kinds of the project, groups, keys of the links.
- **Views** (`.cav`): selection, filters, grouping, zones, edges by layer, derived edges (`lift`, `follow`, processes, network paths), styles, `title` and `description` for their readers; renderings as SVG (Graphviz), Mermaid, sequence diagrams, traversals, DOT, text, ASCII art, tables (CSV, Markdown, HTML, Excel), draw.io.
- **Readable diagrams**: each view laid out several ways and the drawing that reads best kept (fewest crossings, short edges), the layout hints (`stack`, `first`, `last`, `cross`) respected; satellites (a node shared by several frames drawn in each); twins (nodes with the same exchanges drawn as one box); labels of dense diagrams left to the tooltips; frame titles kept clear of edges; dark mode. The `tangled` example, a real model anonymised, is their test bench.
- **Checks** (`.cac`): the tests of a project, run by `cadastre check` and shown by the editors, of severity error, warning or info.
- **Analyses**: impact of failures, alone (`failures`), in pairs (`failures(pairs)`: what a redundancy rests on) or together in a scenario (`failing: [a, b]`, `down`, drawn in red); egress and ingress; network paths deduced from the cabling.
- **The site** (`cadastre site`, `cadastre serve`): the project for its readers, a page per view and per resource, its diagrams leading to the resources, search, the checks, an API page and the JSON model (`docs/api.md`), the playground beside it.
- **Languages**: the site, the playground, the words of the diagrams and the diagnostics in the language of the project (`language = "fr"` in `cadastre.toml`, `--language`): English and French.
- **Project**: `cadastre.toml`, `cadastre init`, modules shared between projects, read only.
- **Exports and imports**: ArchiMate, GraphML, JSON; one-off imports of a Terraform state and of a CSV inventory (`--map`, `--kinds` for an export in its own words); `cadastre anonymize` to share a project without exposing it.
- **Tooling**: `cadastre` command line (`init`, `check`, `build`, `fmt`, `export`, `import`, `anonymize`, `serve`, `site`), language server (diagnostics, completion, navigation, references, renaming, hover, formatting), VS Code extension and IntelliJ plugin with live previews, browser playground (warning when it is of another build than the compiler), MCP server for AI agents.

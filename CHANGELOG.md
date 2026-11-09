# Changelog

Every version of Cadastre, newest first. The [roadmap](ROADMAP.md) lists what each feature does in detail; this file says what changes for a project when it upgrades.

## Versions

Cadastre is in **0.x**. Until 1.0:

- a **minor** version (0.1 → 0.2) may change the language: a syntax, a key, a default. The old form is reported by the compiler with its replacement, for at least one minor version, so that a project is migrated by following its diagnostics;
- a **patch** version (0.1.0 → 0.1.1) fixes without changing the language or the renderings on purpose;
- what is **experimental** (see [Stability](README.md#stability)) may change or go in any version.

The compiler, the VS Code extension, the IntelliJ plugin and the WebAssembly module share one version number.

## Unreleased

- **History from git**: with `[history] git = true` in `cadastre.toml`, each resource gets the day and the author of its last change, from `git blame` on its own lines: `changed` and `changed_by` in filters, checks and tables (`where changed < "2025-01-01"`), and on its page in the site. Days compare in their order (`"2026-11-09"`).
- **Link names**: `-> crm : https [flow] as F-042` gives a link a name, designated by a step of a process (`step F-042`), filtered (`edges [flow] where name = F-042`), listed in tables, shown in the tooltips, the site and the exports; its former names in `aliases`, still designating it. A name given twice is an error.
- **Computed keys**: a key of a kind computed from other keys of the resource, `max(…)` or `min(…)` of numbers or of values of a list ordered as written: `kind app { security: max(availability, integrity, confidentiality) }`. Read like a written key by filters, `group-by`, tables and the site; writing it by hand is an error.
- **`stack`**: the edges between the items of a level are laid out with a length of zero rather than drawn after the layout: they go straight from one item to the other, rather than around a frame to reach a node nested in it, and the info naming them is gone. `layout: free` on a style rule still leaves them out of the layout.
- **`merge: pair`**: the style rules apply to each direction of an edge going both ways, with its own source and target; the edge takes the most marked line of the two (dashed one way and solid the other: dashed), and its tooltip tells each direction apart.

## 0.3.0

Two changes may touch a project: `status` is now a standard key (`active`, `planned`, `decommissioned`), so a project that used it with other values gets an error for each; and `initiator` is on by default, so the views in data orientation gain a dot at the end opening the connection (`initiator: false` draws them as before). The rest is added: `status` and its lifecycle, pivot tables, links inside workbooks, `targets(…)` and `sources(…)`, zoom on the diagrams, a faster `cadastre serve`.

- **`targets(layer)`, `sources(layer)`**: the other ends of the links of a layer written from a node, or to it, in filters and tables: `count(sources(flow)) = 0` (the apps no flow reaches), `"count(targets(flow)) as Calls"`.
- **`status`**, a standard key of any resource and any link: `active` (by default), `planned` or `decommissioned`. Planned is drawn in blue, decommissioned in grey; what is decommissioned is left out of the impact analysis (it does not fail, nothing needs it), and what is in service but still placed in, hosted by or linked to something decommissioned is warned about. A project that used `status` as a free key with other values gets an error for each: rename the key, or map its values.
- **Pivot tables**: `render xlsx { group: status  count: [criticality] }` writes a row per value of a key or a path with the number of nodes (or edges) having it, a column per value of the keys counted, and a total: the applications by status and criticality, the flows by kind of exchange.
- **Workbooks**: a cell naming a resource listed by a sheet of the workbook (the server of an instance) links to its row, in the same sheet or in the first one listing it.
- **`merge: pair`**: an edge going both ways labels each direction apart, `→` the way of the drawing (`→ tcp/9392`, `← tcp/10006`), rather than both in one list.
- **Zoom and move around the diagrams** on the site, in the playground and in the VS Code preview: Ctrl + wheel (the wheel alone in the playground and the preview) zooms around the cursor, a drag moves the drawing, buttons zoom in, out, fit and show at 1:1; the links and the tooltips keep working, the zoom is kept while the view is drawn again, and a printed page gives the whole drawing. The *Fit to width* / *Actual size* button of the preview gives way to them.
- **`initiator`**: with `orientation: data`, a dot marks the end that opens the connection, the arrowhead still following the data: a pull reads as a pull (`files ──▶● reader`). On by default, so the views in data orientation change; `initiator: false` draws them as before. In the SVG (Graphviz and `layout: elk`), the DOT and the view to move by hand.
- **`cadastre serve`** generates the site again when the files of the project change, rather than when a request comes more than a second after the previous one, and lays out again only the views whose diagram changed: a page opens at once, an edit costs the layout of the views it touches. The new site is generated in the background, the previous one served meanwhile, and an open page reloads itself when it is there.
- **Move a view by hand**: on the page of a view in the site, in the *Movable* tab of the playground and with *Move by hand* in the VS Code preview, the diagram drawn again by Cytoscape.js, laid out by Graphviz like the SVG, its nodes and frames dragged and its edges bent, for a screen shared in a meeting; nothing goes back to the files. The boxes of twins and the nodes drawn as satellites are placed where the SVG draws them, so the routes of Graphviz fit them, and an edge Cytoscape cannot draw with its bends is drawn straight rather than lost. The draw.io export is now laid out by Graphviz for these views too, rather than by the compiler.
- **The internet as the other end of a cable** (`peer: internet`) and of a `wan_link`: the access of a site to its provider, without a site nor a router standing for it. Many interfaces reach it without contradiction, and network paths do not join two sites through it. A name written in full now designates its resource among others ending with it (`internet` at the root, beside interfaces named `internet`).

## 0.2.0

Two defaults change the drawings: `satellites` is `false`, and the items of a `stack` level are placed in the order written. To keep the drawings of 0.1, add `[render] satellites = true` to `cadastre.toml` and swap the items of the levels drawn in another order. The rest is added: tables for a CMDB, rack elevations, `merge: pair`, `referrers(…)`.

- **Faster builds**: the views are laid out by a few Graphviz processes, one per core, rather than one per variant of each view: where Graphviz takes a second to start (its fonts, on Windows), the `tangled` example builds in 3 seconds rather than 67, the drawings unchanged.
- **`render racks`**: rack elevations, a column per rack and a box per device at its height (`u`, its lowest unit, and `height`), the racks of a site side by side; two devices on one unit, or a device beyond the top of its rack, are warned about. In the site, the playground and the previews of the editors.
- **`referrers(key)`**: the resources whose key designates a node, the inverse of any reference key as `hosts` is of `on`. In filters (`count(referrers(backup)) = 0`, the backup policies no server uses) and in tables (`"referrers(backup) as Servers"`).
- **Workbooks**: `render xlsx { sheets: [servers, instances] }` writes the tables of other views as the following sheets of the workbook of a view, one file for a whole CMDB.
- **Tables**: a column can give its header (`"on.ancestor(site) as Site"`) and count the items of a path (`"count(on) as Servers"`), and `rows: "each(on.on)"` writes a row per item of a path, the columns starting with that path read from the item of the row: one row per app and physical server, as the general sheet of a CMDB.
- **`merge: pair`** in `lift(…)`: one edge per pair of nodes whatever the direction of its links, drawn with a head at each end when both directions are present.
- **`stack`**: an info names the edges between items of a level, which `stack` takes out of the layout (`view storage: 2 edges between items of a level of stack, drawn after the layout: …`).
- **`satellites` is `false` by default**: every node drawn once, the copies in each frame kept for the views that ask for them (`satellites: true`). A project sets its own default in `cadastre.toml`, `[render] satellites = true`, a view still deciding for itself. To keep the drawings of 0.1, add that line to `cadastre.toml`.
- **`stack`**: the items of a level are placed in the order they are written, across the direction, rather than where Graphviz puts them; `cross` still orders them. A view whose level was written in another order than drawn is drawn in the written order: swap its items to keep the drawing.
- **`cadastre anonymize`**: the copy builds when the original does. The names a `for_each` generates are renamed through their template (a reference to `srv-lil` no longer dangles); the networks are renumbered whole, a network still containing what it contained and none overlapping another unless they did; masks, `0.0.0.0` and the loopback are kept, as are the colors of the styles.

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

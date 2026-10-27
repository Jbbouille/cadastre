# The JSON model

One JSON model of a compiled project, served three ways:

| Where | How |
|---|---|
| `cadastre export json` | the whole model in one file, `out/model.json` |
| the site (`cadastre site`) | the same records as static files under `api/`, one per resource and per view, for scripts, dashboards and agents, without a server |
| the MCP server (`cadastre mcp`) | the same records in the results of `get_resource`, `eval_view`, `run_checks`, capped at 200 items |

The model is read only: writing is editing the `.ca` files and going through Git.

`version` is the version of this model (`2`); a field is added, never removed nor renamed, within a version. Every resource is designated by its **qualified name** (`dc1.lan.apps.web01`), unique in the project.

## The files of the site

```
api/
  project.json                  the project: counts, views, checks, modules, diagnostics
  resources.json                every resource, as its short record
  resources/<id>.json           the full record of a resource, by qualified name
  links.json                    every link, both ends named
  views/<name>.json             a view evaluated: nodes, frames, edges
  checks.json                   the checks and their results
  search.json                   the index of the search of the site
```

The site describes these files on its **API** page (`api.html`), with a live example of each.

## `project.json`

```json
{
  "format": "cadastre-model", "version": 2, "cadastre": "0.1.0",
  "name": "acme-is",
  "resources": 55, "links": 36,
  "kinds": { "app": 12, "database": 4, "server": 9, "site": 3 },
  "views": [{ "name": "flows", "title": "Application flows", "description": "…", "formats": ["svg"], "declared": "src/views/flows.cav:3:6" }],
  "checks": [{ "name": "owned", "severity": "error", "message": "…", "result": "passed", "summary": "no node selected, none expected", "selected": 0, "failing": [] }],
  "modules": [{ "name": "shared", "files": [12, 15] }],
  "files": ["src/apps.ca", "src/views/flows.cav"],
  "diagnostics": [{ "severity": "warning", "message": "…", "help": "…", "at": "src/apps.ca:12:3" }]
}
```

## A resource (`resources/<id>.json`)

The short record (what `resources.json` lists) then the detail:

| Field | |
|---|---|
| `id`, `kind`, `name`, `label` | the qualified name, the kind, the short name, the `label` if any |
| `owner`, `operator`, `site`, `system` | the responsible and operating teams (written or inherited), the site (by containment, or the site of what it runs on) and the system it is in |
| `incomplete`, `missing` | whether it lacks something for a complete model, and what: one line per missing key, placement or free-text schedule |
| `declared` | `path:line:column` of its declaration |
| `virtual`, `description`, `parent`, `path` | a virtual machine; the `description`; the parent; the ancestors from the root |
| `module` | the module it comes from, for a dependency |
| `attributes` | every key written, values as text, lists as arrays |
| `references` | each reference key with the qualified names it resolves to (`on`, `owner`, `rack`…) |
| `children` | what it contains |
| `hosts` | what runs on it (the inverse of `on`) |
| `members` | the members of a pool or a cluster (the resources whose `pool` or `cluster` designates it) |
| `runs_here` | what runs on what it contains or on its members, at any depth: the apps of a site, of a subnet, of a pool |
| `failures` | the resources whose failure alone makes it unavailable (impact analysis) |
| `failure_pairs` | the pairs of resources whose failure together, and not of either alone, makes it unavailable: what its redundancy rests on (`[["dc.s1", "dc.s2"]]`) |
| `questions` | its open questions |
| `outgoing`, `incoming` | its links: `with`, `port`, `layers`, `carries`, `through`, `through_steps` (the way as steps, each with its variants: `[["sw-01", "sw-02"], ["fw"]]`), `through_deduced`, `attributes`, `declared` |

## A view (`views/<name>.json`)

`name`, `title`, `description`, `formats`, `declared`, then:

- `nodes`: `id`, `kind`, `label`, `frame` (the id of its innermost frame);
- `frames`: `id`, `title`, `resource` (the qualified name the frame stands for, or null for a zone or a value), `in` (the frame it is in);
- `edges`: `source`, `target`, `relation` (null for a flow), `ports`, `layers`, `carries`, `through`, `through_steps` (the way as steps, each with its variants), `step`;
- `diagnostics`: the warnings of the evaluation.

A view is not evaluated while the project has errors: the file is then absent from the site, and `project.json` lists the errors.

## The checks (`checks.json`)

One record per check: `name`, `severity`, `message`, `declared`, `result` (`passed`, `failed`, `cannot be evaluated`, `nothing to run`, `not run` when the project has errors), `summary`, `selected`, `failing` (each with `item` and `declared`).

## Reading it

A static site: `fetch("api/resources/sales.shop.json")` from a page, `curl` from a script, or the files in the deployed folder. With `cadastre serve`, the same paths are served under `/api/`, computed at each request from the files on disk.

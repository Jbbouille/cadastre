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
  AGENTS.md                     how to read all this, for an AI agent or a script (docs/api-agents.md)
  index.json                    where to start: what the API holds, and a link to each file below
  project.json                  the project: counts, views, checks, modules, diagnostics
  schema.json                   the kinds, their parents and keys, the keys of the links and layers
  kinds.json                    every kind, with its number of resources
  kinds/<kind>.json             the resources of a kind, as short records with their keys
  resources.json                every resource, as its short record with its keys
  resources/<id>.json           the full record of a resource, by qualified name
  links.json                    every link, both ends named
  links/<id>.json               a link, by its identifier
  views.json                    every view, as declared
  views/<name>.json             a view evaluated: nodes, frames, edges
  checks.json                   the checks and their results
  search.json                   the index of the search of the site: resources, views, named links (`href`: their page)
```

The site describes these files on its **API** page (`api.html`), with a live example of each.

## Navigating

The API is read by following its links, from `api/index.json`: no path is to be built by hand. The records of the index, the project, the schema, the kinds, the resources, the links and the views carry a block `_links` beside their fields (not those of `checks.json` and `search.json`), its entries `{ "href": … }` relative to the file it is in, as the links of a web page (`../resources/sales.json` from `api/resources/sales.shop.json`):

| In | `_links` |
|---|---|
| `index.json` | `project`, `schema`, `kinds`, `resources`, `links`, `views`, `checks`, `search`, the pages of the site (`site`, `help`), the guide for agents (`agents`, `AGENTS.md`), and templates (`"templated": true`): `resource` (`resources/{id}.json`), `link`, `kind`, `view` |
| a resource | `self`, `page` (its HTML page), `kind`, the resources it names (`parent`, `owner`, `operator`, `site`, `system`, `path`, `children`, `hosts`, `members`, `runs_here`, `failures`), `references` by key (`on`, `pool`, the reference keys of the project), `outgoing` and `incoming` (each link, `with` its other end) |
| a link | `self`, `page`, `source`, `target`, `carries`, `through` |
| a view | `self`, `page`; each node to its `resource`, each frame of a resource to it, each edge to its `source`, `target` and `links` |
| a kind | `self`, `schema` (its entry in `schema.json`); its resources each with `self` |

In a template, `{id}` is a qualified name, `{name}` the name of a view, every character other than a letter, a digit, `.`, `-` and `_` written `_`. The fields of the records are those of the export and of the MCP server: `_links` is added in the files of the site only, and a client of version 2 reading the fields alone reads them as before.

## Using it

Three things to know, and a script is a few lines:

- **A list is enough to filter.** The records of `resources.json` and of `kinds/<kind>.json` carry, beside the short record, the `parent`, the `attributes` as written and the `references` resolved to qualified names: the servers on `debian12`, the apps of a team, the resources whose `status` is `planned` are found in one file, without reading a file per resource. The full record (`resources/<id>.json`) adds what is computed: links, children, what runs on it, failures.
- **An `href` is relative to the file it is read from**, as a link in a web page: resolve it against the URL of that file (`new URL(href, url)` in JavaScript, `urljoin(url, href)` in Python). The same record has other `href` in `resources.json` and in `kinds/<kind>.json`: keep the URL of the file with the record.
- **A template** (`resource`, `link`, `kind`, `view` of `index.json`) is filled with a name, every character other than a letter, a digit, `.`, `-` and `_` written `_`.

In JavaScript, from a page of the site or anywhere `fetch` reaches it:

```js
const api = new URL("https://cadastre-lang.org/reference/api/index.json");
async function get(href, from = api) {
  const url = new URL(href, from);
  return { url, data: await (await fetch(url)).json() };
}
const { data: index } = await get(api);
// The servers on debian12: the list of the servers, filtered.
const servers = await get(index._links.kind.href.replace("{kind}", "server"));
const debian = servers.data.resources.filter((s) => s.attributes.os === "debian12").map((s) => s.id);
// What runs on the first one, from its full record: its href read from the list's URL.
const first = await get(servers.data.resources[0]._links.self.href, servers.url);
console.log(debian, first.data.hosts);
```

In Python:

```python
import json, urllib.request
from urllib.parse import urljoin

API = "https://cadastre-lang.org/reference/api/index.json"

def get(href, base=API):
    url = urljoin(base, href)
    with urllib.request.urlopen(url) as response:
        return url, json.load(response)

_, index = get(API)
url, apps = get(index["_links"]["kind"]["href"].replace("{kind}", "app"))
for app in apps["resources"]:
    print(app["id"], app["owner"], app["references"].get("on", []))
```

The firewalls on a declared flow: its `through_steps` (the devices in order, each step with its variants, written or deduced from the network), their kinds in `resources.json`:

```js
const { data: links } = await get(index._links.links.href);
const { data: all } = await get(index._links.resources.href);
const kinds = new Map(all.map((r) => [r.id, r.kind]));
const flow = links.find((l) => l.source === "invoicing.billing" && l.target === "accounting.legacy-erp");
const firewalls = flow.through_steps.flat().filter((device) => kinds.get(device) === "firewall"); // ["paris-dc1.fw01"]
```

With `curl` and `jq`, a column for a spreadsheet:

```bash
curl -s https://cadastre-lang.org/reference/api/kinds/server.json \
  | jq -r '.resources[] | [.id, .site, .attributes.os // ""] | @tsv'
```

## The network path between two resources

The path between any two resources, a flow between them or not (the devices from one to the other, through the cabling, the gateways of the subnets, the VPN tunnels, the egress to the internet), is computed by the compiler, as `render traversal` draws it: every pair cannot be published in advance. A site published with its playground computes it in the page, with the compiler as WebAssembly (`playground/cadastre.js`, `playground/cadastre_wasm.wasm`, and the project, `playground/project.json`):

```js
const base = new URL("https://cadastre-lang.org/reference/playground/");
const { load } = await import(new URL("cadastre.js", base));
const cadastre = await load(new URL("cadastre_wasm.wasm", base));
const project = await (await fetch(new URL("project.json", base))).json();
const way = cadastre.path(project, "web01", "erp01");
// { from, to, found, why, steps: [["sw-01", "sw-02"], ["fw01"]], devices, cables, connectors, internet, resources }
```

`steps` are the devices between the two ends, in order, each with its variants (redundant switches); `resources` gives the kind and label of each, the firewalls being of kind `firewall`; when there is no path, `found` is `false` and `why` says what is missing (`no router nor firewall routes subnet aws-paris.prod.dmz (declare its gateway)`). The page `playground/path.html#from=web01&to=erp01` draws it for a person, and `&embed` leaves the drawing alone, for an `<iframe>`. The record in detail, and the other ways to it: [the guide for agents](api-agents.md), § 5.

## `schema.json`

The schema of the project, for a client to know the keys without reading the sources: `kinds` (each with `like` and `declared_by`, `language` or `project`, its `parents`, `within`, `connector`, and its `keys`), `every_resource` (the keys of every kind: `label`, `description`…), `links` and `layers` (the keys of the links, and of the links of each layer), `groups` (the unions of kinds). A key gives its `type` as the schema writes it (`ref(server | subnet)*`), `required`, `list`, `declared_by`, and `designates` (the kinds of a reference), `values` (a closed list) or `computed` when it has one.

## `project.json`

```json
{
  "format": "cadastre-model", "version": 2, "cadastre": "0.1.0",
  "name": "acme-is",
  "resources": 55, "links": 36,
  "kinds": { "app": 12, "database": 4, "server": 9, "site": 3 },
  "views": [{ "name": "flows", "title": "Application flows", "description": "…", "group": "Network", "formats": ["svg"], "declared": "src/views/flows.cav:3:6" }],
  "checks": [{ "name": "owned", "severity": "error", "message": "…", "result": "passed", "summary": "no node selected, none expected", "selected": 0, "failing": [] }],
  "modules": [{ "name": "shared", "files": [12, 15] }],
  "files": ["src/apps.ca", "src/views/flows.cav"],
  "diagnostics": [{ "severity": "warning", "message": "…", "help": "…", "at": "src/apps.ca:12:3" }]
}
```

## A resource (`resources/<id>.json`)

The short record (what `resources.json` and `kinds/<kind>.json` list, with `parent`, `attributes` and `references` besides) then the detail:

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
| `outgoing`, `incoming` | its links: `with`, `id` (the identifier of the link: its name, otherwise its ends and port, `shop--db--tcp-5432`; its page is `links/<id>.html`), `name`, `port`, `layers`, `carries`, `through`, `through_steps` (the way as steps, each with its variants: `[["sw-01", "sw-02"], ["fw"]]`), `through_deduced`, `attributes`, `declared` |

## A view (`views/<name>.json`)

`name`, `title`, `description`, `group` (as written, `null` without it), `formats`, `declared`, then:

- `nodes`: `id`, `kind`, `label`, `frame` (the id of its innermost frame);
- `frames`: `id`, `title`, `resource` (the qualified name the frame stands for, or null for a zone or a value), `in` (the frame it is in);
- `edges`: `source`, `target`, `relation` (null for a flow), `ports`, `layers`, `carries`, `through`, `through_steps` (the way as steps, each with its variants), `step`, `links` (the `id` of each link the edge draws), `exchange` (`push`, `pull`, `both`, or null), `reversed` (drawn from its target to its source: a `pull` with `orientation: data`);
- `diagnostics`: the warnings of the evaluation.

A view is not evaluated while the project has errors: the file is then absent from the site, and `project.json` lists the errors.

## The checks (`checks.json`)

One record per check: `name`, `severity`, `message`, `declared`, `result` (`passed`, `failed`, `cannot be evaluated`, `nothing to run`, `not run` when the project has errors), `summary`, `selected`, `failing` (each with `item` and `declared`).

## Reading it

A static site: `fetch("api/index.json")` from a page, then its links (an agent reads [`api/AGENTS.md`](api-agents.md) first); `curl` from a script, or the files in the deployed folder. With `cadastre serve`, the same paths are served under `/api/`, computed at each request from the files on disk.

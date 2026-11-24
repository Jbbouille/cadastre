# Reading a Cadastre site: a guide for agents

This guide is for a program, an AI agent most of all, that is given the address of a site published by [Cadastre](https://github.com/Jbbouille/cadastre) and must answer questions about the information system it describes: what exists, where it runs, who owns it, who talks to whom, what goes through what, what breaks when something fails. Every site holds a copy of this guide at `api/AGENTS.md`.

Read it whole once: the rules are few, and following them is what makes the answers right.

## 1. What a site is

A Cadastre project describes an information system in text files: sites, networks, subnets, servers, applications, databases, SaaS, data, teams, the links between them. `cadastre site` publishes it as a static site: HTML pages for people, and **an API**: JSON files under `api/`, the same facts as data. There is no server to query, nothing to authenticate: the API is files, read with HTTP `GET` (or from disk), a snapshot of the project at its last publication. It is read only.

The words:

- a **resource** is anything described: a `site`, a `server`, an `app`, a `database`, a `firewall`, a `team`, a piece of `data`… Its **kind** says which. Resources are nested: a server in a subnet, in a network, in a site; an app in a system.
- its **qualified name**, `id`, is the path of names from the root: `paris-dc1.lan.erp.erp01`. Its **name** is the last segment (`erp01`), its **label** a name for people (`Accounting ERP`), when written.
- a **link** joins two resources: a flow from an app to a database (`source`, `target`), on a `port` (`https/443`), in **layers** (`flow`, `auth`, `trigger`, `connector`…), **carrying** data, going **through** devices (a firewall, a NAT gateway, a VPN tunnel).
- a **reference** is a key naming other resources: `on` (what an app runs on), `owner` (a team), `rack`, `pool`…
- a **view** is a selection of the facts drawn as a diagram or a table; a **check** is a rule the facts must follow, run like a test.

## 2. Five rules

1. **Start from `api/index.json`, and follow its links.** Every file you need is reached from it by the `_links` blocks of the records: never build a path by hand when a link gives it.
2. **An `href` is relative to the file you read it from**, as a link in a web page. Resolve it against the URL of that file: `new URL(href, urlOfTheFile)` in JavaScript, `urljoin(url_of_the_file, href)` in Python. The same record has other `href` in `resources.json` (`resources/x.json`) and in `kinds/app.json` (`../resources/x.json`): keep the URL of the file with the record.
3. **A list is enough to filter.** `resources.json` (every resource) and `kinds/<kind>.json` (those of a kind) give each resource with its `owner`, `operator`, `site`, `system`, `parent`, `attributes` (every key as written) and `references` (resolved to qualified names). To find the servers on `debian12` or the apps of a team, read one list, not one file per resource.
4. **The full record says what is computed.** `resources/<id>.json` adds the links of the resource (`outgoing`, `incoming`), what it contains (`children`), what runs on it (`hosts`), what runs below it (`runs_here`), what it depends on (`failures`, `failure_pairs`). Read it for one resource, not for a whole list.
5. **Names in templates.** `index.json` gives templates (`"templated": true`): `resources/{id}.json`, `links/{id}.json`, `kinds/{kind}.json`, `views/{name}.json`. Fill them with a qualified name, a link identifier, a kind or a view name, every character other than a letter, a digit, `.`, `-` and `_` written `_`.

## 3. The files

| File | What it holds | Read it to |
|---|---|---|
| `index.json` | the counts, the links to every file, the templates | start |
| `project.json` | name, counts by kind, the views and checks with their results, the diagnostics (errors, warnings) | know the project, see whether it compiles |
| `schema.json` | every kind with its parents and keys (type, required, list, closed values, the kinds a reference designates), the keys of the links and of each layer | know which keys exist and what they hold, before filtering |
| `kinds.json` | every kind with its number of resources | see what the project describes |
| `kinds/<kind>.json` | the resources of a kind, each with its keys | filter a kind |
| `resources.json` | every resource, each with its keys | find a resource by name or label, filter across kinds |
| `resources/<id>.json` | one resource in full: its links, what it contains, runs, depends on | answer about one resource |
| `links.json` | every link: `source`, `target`, `port`, `layers`, `carries`, `through`, `through_steps`, `attributes` | flows between resources, what they carry and go through |
| `links/<id>.json` | one link | follow a link of a resource |
| `views.json`, `views/<name>.json` | the views, and what each draws: nodes, frames, edges | reuse a view the authors made |
| `checks.json` | the rules and their results, what fails them | quality, compliance |
| `search.json` | the index of the search of the site (resources, views, named links, with their page) | match free text to a resource |

`checks.json` and `search.json` have no `_links`; every other file has. A file absent while the project has errors: the views (`views/<name>.json`), `project.json` then lists the errors.

## 4. Doing things

The examples are in JavaScript; the same requests are a few lines in any language. A helper used below:

```js
const api = new URL("https://example.org/our-is/api/index.json"); // the address given
async function get(href, from = api) {
  const url = new URL(href, from);
  const response = await fetch(url);
  if (!response.ok) throw new Error(`${response.status} ${url}`);
  return { url, data: await response.json() };
}
const { data: index } = await get(api);
```

**Find a resource by name or label.** Read `resources.json`; match `id`, `name` or `label`. A short name can be shared by several resources (`web01` in two sites): ask, or take the one the context gives (its `site`, `system`).

```js
const all = await get(index._links.resources.href);
const found = all.data.filter((r) => r.name === "erp01" || r.label === "Accounting ERP");
```

**Filter a kind on a key.** `schema.json` tells the keys of a kind and their values; the list of the kind gives them.

```js
const servers = await get(index._links.kind.href.replace("{kind}", "server"));
const debian = servers.data.resources.filter((s) => s.attributes.os === "debian12");
```

**Where does an app run, what runs on a server.** An app's `references.on` (in the lists) gives its servers; a server's full record gives `hosts` (what runs on it) and `runs_here` (what runs on what it contains, for a site or a cluster).

**Who owns it, who operates it.** `owner` and `operator`, written or inherited from what contains it (an app from its system): a team, its own resource.

**The flows of an application.** Its full record: `outgoing` and `incoming`, each with the resource at the other end (`with`), `port`, `layers`, `carries`, `through`. Across the project: `links.json`, filtered on `source` or `target`. A flow is in the layer `flow`; `auth` is authentication, `trigger` a job started, `connector` a cable or a tunnel.

**What data goes where.** A link `carries` data resources; their `attributes` say what they are (`classification: personal`…). Personal data leaving for a SaaS: the links whose `target` is of kind `saas` and whose `carries` include a data classified `personal`.

**The firewalls on a flow.** A declared link gives the way it takes in `through_steps`: the devices in order, each step a list of variants (redundant devices, one of them taken), written by its authors or deduced from the network (`through_deduced: true`). Their kinds are in `resources.json`.

```js
const links = await get(index._links.links.href);
const kinds = new Map(all.data.map((r) => [r.id, r.kind]));
const flow = links.data.find((l) => l.source === "invoicing.billing" && l.target === "accounting.legacy-erp");
const firewalls = flow.through_steps.flat().filter((device) => kinds.get(device) === "firewall");
```

**The network path between any two resources**, flow or not: see section 5.

**What fails with it.** `failures` in the full record of a resource: the resources whose failure alone makes it unavailable, what it depends on (its servers, the switch before them, the firewall on the way…). `failure_pairs`: the pairs whose failure together, and not of either alone, makes it unavailable: what its redundancy rests on. To know what a failure takes down, read it the other way: the resources whose `failures` include it.

**What the authors drew.** `views.json` lists the views with their `title` and `description`; `views/<name>.json` gives what one draws: `nodes` (each a resource), `frames` (what groups them), `edges` (with the `links` they stand for). Each has its HTML page (`_links.page`), with its diagram, to show a person.

**What is wrong or unknown.** `checks.json`: each rule with its `result` and what fails it. A resource whose `incomplete` is `true` lacks something, listed in `missing`; `questions` holds the open questions of its authors. Say so when an answer rests on them.

## 5. The network path between two resources

The path between two resources (the switches, firewalls, routers and tunnels from one to the other, as the cabling, the subnets and their gateways, the VPN tunnels and the egress to the internet give it) is computed by the compiler. Every pair cannot be published in advance; there are three ways to it.

1. **A declared flow**: its `through_steps` (section 4), already in the API. No computation.
2. **Computed in a browser or Node.js**, by the compiler itself as WebAssembly. A site published with its playground holds it under `playground/` (the module `cadastre_wasm.wasm`, its wrapper `cadastre.js`, and the project, `project.json`):

   ```js
   const base = new URL("https://example.org/our-is/playground/");
   const { load } = await import(new URL("cadastre.js", base));
   const cadastre = await load(new URL("cadastre_wasm.wasm", base));
   const project = await (await fetch(new URL("project.json", base))).json();
   const way = cadastre.path(project, "web", "db");
   ```

   The answer, from an app `web` in a DMZ to a database `db` in the subnet of the servers, the firewall `fw` routing between them (its `resources` cut to four of its thirteen entries):

   ```json
   {
     "from": "web", "to": "db", "found": true, "why": null,
     "steps": [["gs.gs-sw-01"], ["gs.fw"], ["gs.gs-sw-02"]],
     "devices": ["gs.gs-sw-01", "gs.fw", "gs.gs-sw-02"],
     "cables": [["gs.gs-sw-01.fw-dmz", "gs.fw.dmz"], ["gs.web01.p1", "gs.gs-sw-01.eth1-20"],
                ["gs.gs-sw-02.eth1-21", "gs.db01.p1"], ["gs.fw.srv", "gs.gs-sw-02.fw-srv"]],
     "connectors": [],
     "internet": [],
     "resources": {
       "web": { "kind": "app", "label": null },
       "gs.gs-sw-01": { "kind": "switch", "label": null },
       "gs.fw": { "kind": "firewall", "label": null },
       "gs.fw.dmz": { "kind": "interface", "label": null }
     }
   }
   ```

   `steps` are the devices between the two ends, in order, each a list of variants (`["col-sw-01", "col-sw-02"]`: redundant switches, one of them taken); `devices` the same, each once; `cables` the cables taken (pairs of interfaces, in no particular order); `connectors` the `wan_link` and `vpn_tunnel` crossed; `internet` the way over the internet (the `egress` of one end to the other); `resources` the kind and label of every resource named, so the firewalls are those of kind `firewall`. `from` and `to` are qualified names, or names as written in a view (`web01`, when one resource has that name). When there is no path, `found` is `false` and `why` says why, in the words of the model: `the cables stop at gs.web01`, `no router nor firewall routes subnet aws-paris.prod.dmz (declare its gateway)`, `veeam runs on no machine`. The project is compiled once per load: keep `cadastre` and `project` to ask several paths.

3. **Shown to a person**: the page `playground/path.html#from=web01&to=erp01` draws the path (the boxes of the devices, in order) and lists the devices, firewalls marked; `&embed` leaves the drawing alone, for an `<iframe>`:

   ```html
   <iframe src="https://example.org/our-is/playground/path.html#from=web01&to=erp01&embed" width="100%" height="420" style="border:0"></iframe>
   ```

Without a JavaScript engine (a Python agent, a shell), the second way is not at hand: use the declared flows (way 1), or, with the sources of the project, the command line (`cadastre build` of a view `edges path(web01, erp01)` rendered as `text`) or the MCP server (`cadastre mcp`, [docs/mcp.md](mcp.md)).

## 6. Answering well

- **Say where the answer comes from**: the resource or the link, its page (`_links.page`), and when the facts were published (`index.json`, `cadastre`: the version of the compiler).
- **Do not guess what is not written.** An app without `on` runs nowhere known; a flow without `through` goes through nothing declared; a `found: false` path is not a path. Say what is missing, and what would be needed (the `why` of a path names it).
- **Names are qualified.** Answer with `label` for people and `id` to be exact: `Accounting ERP (accounting.legacy-erp)`.
- **Lists are complete.** `resources.json` lists every resource, `links.json` every link: a count from them is the count of the project.
- **The model is read only.** Changing it is editing its `.ca` files, in its Git repository, by its authors.

The detail of every record and field: [docs/api.md](api.md). The language itself: [docs/guide.md](guide.md).

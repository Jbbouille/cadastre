# Writing views

A cheat sheet for the part of the language that takes the most practice. The `.ca` files state facts; a view (`.cav`) chooses what to draw from them, and adds nothing to the model: to show something new, add it to a `.ca`. The examples describe a fictional logistics company: the systems `billing`, `orders`, `warehouse` and `crm`, two data centers `dc-alpha` and `dc-beta`, two colocation rooms `colo-1` and `colo-2`, offices, partner sites and a network operator, `netco`. The [guide](guide.md) (§ 6) explains each clause in full, the [reference](reference.md) lists every keyword, option and value.

## 1. How a view is evaluated

```
view network {
  select …          // 1. the nodes
  zone   …          // 4. hand-made frames, optional
  edges  …          // 2. the edges between them, 3. lifted if asked
  style  { … }      // 5. colors, lines, layout of this view, optional
  render svg { … }  // 6. formats and drawing options
}
```

1. **`select`** picks the nodes: kinds, then a `where` filter, then `with ends` / `without isolated`.
2. **`edges`** picks the edges among them: an edge is drawn only if **both its ends are nodes** of the view.
3. **`lift`** replaces the ends of edges by their parent, their site, their system… before that test.
4. **Frames**: `group-by` puts each node into the frame of its site, its system…; then `zone` takes some nodes or frames into frames of its own.
5. **`style`**: the default theme, then the project style, then the view style; the last rule wins.
6. **`render`**: the layout follows the edges; `stack`, `cross` and `orientation` give hints.

Most surprises come from step 2: a node that is not selected loses its edges.

## 2. `select`: the nodes

```
view devices {
  select switch, firewall, router
    where ancestor(site) in [dc-alpha, dc-beta] and kind in [switch, firewall]
       or ancestor(site) = colo-1
}
```

The kinds say which resources may enter, `where` filters them: here the switches and firewalls of the two data centers, plus everything (routers included) in `colo-1`.

| Clause | Effect | Example |
|---|---|---|
| `with ends` | adds the ends of the selected edges, even if not selected | `select app where ancestor(system) = warehouse with ends`: the apps warehouse talks to appear |
| `without isolated` | removes the nodes without any edge | jobs that only have triggers disappear from a flow view without `[trigger]` |
| `group-by K` | one frame per K | `group-by site` |
| `group-by K1 > K2` | nested frames, from the broadest | `group-by operator > site`, `group-by cluster > on` |

**The clauses apply to the whole selection, never to the last kind.** In `select app, job, saas, server, database without isolated`, the list of kinds only says what may enter; `where`, `with ends`, `without isolated` and `group-by` then act on every node, whatever its kind:

- `without isolated` removes any node, app or database alike, that has none of the edges drawn **in this view** (after `lift`); a node linked only by edges the view does not select is isolated here. It cannot be limited to one kind.
- `with ends` adds the ends of the edges of the plain selectors (layers, relations, connectors, `follow`), **of any kind, even one missing from the list**, and the `where` of `select` does not filter them. The edges of a `lift(…)` add nothing: their ends are lifted, not added. An end whose kind is a level of `group-by` is not added, and its edge is dropped with a warning.

**How a resource finds its frame**, for each level of `group-by`:

1. its **ancestor** of that kind: a switch is in its site;
2. otherwise a **reference** to that kind, or the key named: the `operator` of a site, the `cluster` of a server;
3. otherwise what **hosts** it (`on`): an app goes into the frame of its server;
4. otherwise what **it hosts** (`hosts`), when all of it is in one frame: a server without `system:` goes with the system of its apps; a server shared by several systems stays outside.

A **free key** or `component` as a level makes a frame per value: `group-by middleware` (a frame `JBoss 4.2`, a frame `WildFly`), `group-by site > component` (in each site, a frame per component of the apps, `jboss 4.2.0`).

A resource with **several values** at a level (an app `on` two servers) stays in the frame above. A selected node that **hosts** other nodes of the view is drawn **as a frame** (`group-by cluster > on`: the ESX server is the frame of its VMs).

### `zone`: frames that no key gives

```
view sites {
  select site without isolated group-by operator
  zone   partners "Partners" [partner-a, partner-b] { fill: "#eef6ff" }
  zone   internet where scope = external
}
```

- Members: names of nodes or frames of the view, or a filter. A frame comes with its content.
- A zone **wins over `group-by`**: `partner-a` and `partner-b` leave the frame of their operator.
- The **name** designates the zone in `stack` and `cross`; the **label** is its title.
- Drawn dashed and rounded; `{ color, fill, line }` after the members.
- **Nested**: `{ in: dc }` draws the zone in the frame of the zone `dc`.

## 3. `edges`: the edges

Several selectors, separated by commas, each with its own `where`:

| Selector | Takes | Example |
|---|---|---|
| `[flow]`, `[flow, auth]` | the `->` links of these layers | the flows and the authentications |
| `peer`, `on`, `rack`… | a relation, written with a reference key | `peer`: the cables between interfaces |
| `wan_link`, `vpn_tunnel`, `connector` | the connectors | the MPLS links, the VPN tunnels of the offices |
| `lift(<selector>, via: …)` | edges moved up to an ancestor | below |
| `follow(<data>, from: …)` | the path of a business object | below |

In a `where`, `source` and `target` are the two ends:

```
edges [flow] where source.ancestor(system) = billing or target.ancestor(system) = billing
```

### `lift`: moving edges up

A cable links two **interfaces**; to draw the **devices**, each end is replaced by its parent:

```
view cabling {
  select switch, firewall, router
  edges  lift(peer where source.parent.kind != server, via: parent), wan_link
}
```

| Parameter | Effect | Example |
|---|---|---|
| `via: parent` | an interface becomes its device | `lift(peer, via: parent)` |
| `via: ancestor(site)` | up to the site | 8 fibers between two rooms become `peer ×8` |
| `via: ancestor(system)?` | an app becomes its system; with `?`, a resource without system stays as it is | `lift([flow], via: ancestor(system)?)` |
| `via: system?.ancestor(system)` | a path: the `system` key, then its enclosing system | a gateway server, attached by `system: orders` |
| `merge: endpoints` (default) | one edge per pair of nodes, `×n` | |
| `merge: port` | one edge per pair and `protocol/port` | |
| `merge: none` | one edge per original edge | each fiber keeps its circuit |
| `loops: keep` | keeps the edges whose two ends become the same node | dropped by default |
| `via_<kind>: <path>` | the path for the ends of that kind, before `via` | `via: ancestor(system)?, via_job: on`: a job goes to its server, the rest to its system |

A target that is not selected is lifted in turn: a database `on` a shared database, lifted `via: on` with `select server`, reaches the server of the shared database.

Good to know:

- The `where` inside `lift(…)` filters the edges **before** they are lifted; a `where` after `lift(…)` filters them after.
- **An end that is already selected is not lifted.** To get one node per system, select the systems and what has no system, not their apps: `select system, app, saas where kind = system or not exists(ancestor(system))`.
- An edge whose two ends land on the same node is dropped (a flow internal to a system), unless `loops: keep`.

### `follow`: the path of a business object

```
view invoice-chain {
  select app, job, saas, server without isolated
  edges  follow(invoices, from: billing.portal)
  render svg { orientation: data, edge_labels: [step, port] }
}
```

With `relay: [database]`, the walk goes through databases: from a database, along its links that do not declare the data, to the apps reading it, then on; `relay: [stock.db]` through that one only, `relay_to: [app]` towards apps only. From `billing.portal`, follows **in the direction of the data** every link whose `carries` (or `channel`) holds `invoices`: along the link for `exchange: push`, backwards for `pull`, both ways for `both`. Each edge gets a **step** (1., 2., 3.), shown with `edge_labels: [step]`. The walk stops at a link that does not declare the data: a database whose app does not declare what it reads ends the path there.

## 4. Filters and paths

| Form | Example |
|---|---|
| `k = v`, `k != v` | `kind = system`, `exchange = pull` |
| `k in [a, b]` | `kind in [switch, firewall]`, `target in [broker, gateway]` |
| `k has v`: the list or the reference contains v | `carries has invoices` |
| `exists(k)` | `exists(serial)`: only physical servers have a serial number |
| `k` alone: a boolean | `incomplete`, `shared` |
| `and`, `or`, `not`, `( )` | `not (source.kind = database or target.kind = database)` |
| `<`, `<=`, `>`, `>=` | numbers and versions only: `version >= 2.0.0` |

**Paths** read from left to right, true if at least one target matches:

- `ancestor(site)`: the site containing the resource (the resource itself if it is a site);
- `source.parent.kind`: the kind of the device carrying the source interface of a cable;
- `on.ancestor(site)`: the site of the server hosting an app;
- `on?.ancestor(site)`: the same, the resource itself when it has no `on`.

**Pseudo-attributes**: `kind`, `name`, `source`, `target`, `proto`, `port`, `step`, `parent`, `hosts` (what a server hosts), `shared`, `incomplete` ([reference](reference.md#filters)).

### Pitfalls

- **`name` is the qualified name.** An app `tax-engine` inside the system `billing` is named `billing.tax-engine`: `target.name = tax-engine` finds nothing. Write `target in [tax-engine]`, which resolves the name as in a `.ca`, or the qualified name.
- **`carries` is a list**: `carries has invoices`, not `=`.
- **A missing attribute makes every comparison false, `!=` included.** `os != debian12` drops a server without `os`; to keep it: `not (os = debian12)`.
- **`and` binds tighter than `or`**: `A and B or C` reads `(A and B) or C`. Add parentheses when in doubt.
- **A filter that matches nothing is reported**: a style rule of the view applying to nothing, a `where` of `edges` keeping no edge, a zone filter matching nothing give a warning, with a hint when the value is a `label` or a short name compared to `name`.
- **A free key is compared on its raw text**, in order only with a number or a version (`criticality >= 2`) ([reference](reference.md): a key it does not list is free).
- **A key of the filter must exist for the selected kinds**: `select database where cidr = …` is an error, `cidr` being a key of subnets.

## 5. `style`

```
view wan {
  style {
    edges where kind = wan_link   { color: "#a16207", line: dashed, layout: free }
    edges where kind = vpn_tunnel { color: "#7c3aed", line: dashed, legend: "IPsec VPN" }
  }
}
```

| Target | Applies to | Properties |
|---|---|---|
| `kind <kind>`, `nodes where …` | nodes (and frames for `kind`) | `color`, `fill`, `shape`, `line`, `legend` |
| `edges where …`, `layer <layer>` | edges | `color`, `line`, `layout`, `legend` |

- **Cascade**: default theme, then the project `style` block (a single one, at the root of a `.cav`), then the `style` of the view; for each property, the last matching rule wins.
- **`legend: "…"`** puts the rule in the legend, only if its style shows in the view.
- **`layout: free`** draws an edge without letting it place the nodes: a mirror between two storage VMs that should stay side by side, the links of a view whose order is given by `stack`.

## 6. `render`

```
render svg {
  direction: tb
  stack: [[offices, partners], netco-mpls, [dc-alpha, dc-beta]]
  cross: [offices, partners, dc-alpha, dc-beta]
  node_labels: [label, description]
  edge_labels: [label, circuit_id]
  legend: true
}
```

| Option | Effect | Example |
|---|---|---|
| `direction` | `lr` (default), `rl`, `tb`, `bt` | `bt`: storage flows going up to the storage |
| `orientation: data` | arrows **and layout** along the data: a reader (`pull`) is placed after what it reads | extractors, then the integration that reads them, then its targets |
| `stack` | frames or nodes in this order **along** the direction; a nested list puts several on a level | `[[colo-1, colo-2], netco-mpls, [dc-alpha, dc-beta]]` |
| `cross` | frames or nodes in this order **across** the direction | the colocations above the data centers in `lr` |
| `first`, `last` | frames or nodes first or last **in their frame** | `first: [router-a, router-b]`: the routers above the switches of their room |
| `routing` | `curved` (default), `polyline`, `straight`, `orthogonal` | `orthogonal` for a network diagram |
| `edge_labels_layout` | `nodes` (default): labels laid out as nodes on the edges; `free`: placed after the layout, without moving anything | `free` when many labeled edges leave the same node and detour through their labels |
| `edge_labels_wrap` | labels on several lines: `40` breaks between values beyond 40 characters, `values` puts one value per line | a long `carries` list as a block |
| `spacing` | space between the nodes and between the ranks, as a factor: `1.5`, or `[nodes, ranks]` | `[1, 2]`: rooms further apart |
| `node_label_size`, `edge_label_size`, `frame_label_size` | sizes of the text, in points (`10`, `9`, `11`) | `edge_label_size: 7` on a crowded network view |
| `merge_label` | `count`: an edge merging links shows their protocols and number, `sftp ×9`, the detail in the tooltip; a number: from that many links | `merge_label: 5` on a view lifted to systems |
| `node_labels` | keys shown in the nodes | `[label, model, cpu, ram]` |
| `edge_labels` | keys shown on the edges; on a cable, those of its interfaces; an edge with none of them stays bare, `relation` names it anyway | `[label, circuit_id]` gives `Te1/0/1, C-1042` |
| `legend` | legend of the style rules carrying a `legend` | |

`stack`, `cross`, `first` and `last` are **hints**: edges going the other way prevail. The items of a level of `stack` stay side by side even when edges link them: `stack: [hq, [room-a, room-b]]` keeps the two rooms on a row, the links between them drawn across; `cross: [room-a, room-b]` puts them in this order. For straight links in a view of curved flows, a style rule: `kind wan_link { routing: straight }` (SVG). A cable or a connector has no direction: it is laid out in the order of `first` / `last`, then of the `stack` levels, then of the registry, whichever end declares it. To let the stack decide alone, set the edges concerned to `layout: free`. A frame is drawn in one piece: separating two of its items in `stack` cannot be honored, and the build warns about it.

## 7. Recipes

| Need | How |
|---|---|
| The devices linked by their cables | `select switch, firewall, router` + `lift(peer, via: parent)` |
| One edge per site rather than per cable | `select site` + `lift(peer, via: ancestor(site))` |
| One node per system | `select system, app, saas where kind = system or not exists(ancestor(system))` + `lift([flow], via: ancestor(system)?)` |
| The flows around a system | `edges [flow] where source.ancestor(system) = billing or target.ancestor(system) = billing`, with `with ends` on `select` |
| The path of a business object | `follow(<data>, from: <start>)` + `orientation: data` + `edge_labels: [step]` |
| The VMs inside their server | `select server, app group-by cluster > on` |
| The servers of a pool, and what runs on it | `select server, app group-by pool` |
| The apps by middleware | `select app group-by component`, or a free key: `group-by middleware` |
| The shared servers apart | `zone shared "Shared servers" where kind = server and shared` |
| An edge that must not move the nodes | `style { edges where … { layout: free } }` |
| Ordering frames | `stack` (along the direction, nested lists for a level), `cross` (across) |
| A frame no key gives | `zone <name> "<title>" [members]`, then its name in `stack` and `cross` |
| Placing along the data | `orientation: data` |
| A node at the top of its frame | `first: [<node>]` |
| Edges that detour through their labels | `edge_labels_layout: free`, possibly `routing: polyline` |
| Long labels | `edge_labels_wrap: 40` or `edge_labels_wrap: values` |
| An inventory | `select server` + `render csv, markdown { columns: [name, label, ip, "ancestor(site)", hosts] }` |
| A matrix of flows | `edges [flow]` + `render tsv { columns: [source, target, proto, port, exchange, carries] }` |

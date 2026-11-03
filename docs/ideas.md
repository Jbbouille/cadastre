# Ideas

Ideas met while writing a real map, not decided yet: what they would change, and what is open.

## `merge: pair`: one arrow for the two directions

`lift(…, merge: endpoints)` merges the edges going the same way between two nodes. A flow A → B
and a flow B → A stay two arrows: only symmetric relations (`peer`) are put back in one direction
before merging. On a view of a backup (a backup server and its proxy, a hypervisor manager and the
backup server), each pair talking both ways draws two arrows side by side, their labels mixed.

`merge: pair` would merge the edges of a pair of nodes whatever their direction:

- one arrow, with a head at each end when the two directions are present, a single head otherwise;
- the labels of each direction kept apart, `→ jobs, data` and
  `← disk-blocks`, or a single list when the direction does not matter;
- the tooltip listing the merged links, as `merge: endpoints` does;
- `orientation: data`: the layout follows the direction carrying the most links, or the first one
  written.

Open: the style of an edge merging links of two styles (solid one way, dashed the other); `pair`
on a `follow`, whose steps stay apart today.

## `satellites: false` by default

`satellites` is `true` by default: a node exchanging with two frames or more is drawn once in each
of them, in gray, with `×N`. On a small view grouped by site (two data centers, a few servers
each), the copies make the view harder to read than the edges they replace: the reader looks for
the node in several places, and has to write `satellites: false` on view after view.

Making it `false` by default would draw every node once, and keep the satellites for the dense
views that ask for them (`satellites: true`). Alternatives:

- a project default, in `cadastre.toml` (`[render] satellites = false`), the view still able to
  override it;
- `true` only from a threshold: a node drawn as satellites when it exchanges with more than N
  frames, or when the view has more than N edges.

Open: the views of the examples and of the tests that rely on the current default, to be given
`satellites: true`.

## Edges drawn after the layout: where they go

Two views of a real map, anonymized here, where an edge that does not take part in the layout ends
up in the wrong place. In both, the edge is drawn once the nodes are placed, as `layout: free`
edges are, and nothing decides on which side it passes nor what it may cross.

### A colocation linked by `layout: free` edges only

Two carrier sites wired to each other, two data centers wired to each other, a third colocation
with a single router linking a carrier switch to a data center switch. The links of the router,
and the one from a carrier to a data center, are `layout: free`, so as not to pull the four sites
out of their grid.

```
site carrier-north {
  switch north-sw-1 { interface p1 { peer: south-sw-1.p1 }  interface p2 { peer: south-sw-1.p2 } }
  switch north-sw-2 { interface p1 { peer: south-sw-2.p1 }  interface p2 { peer: south-sw-2.p2 }  interface dc { peer: dc-a-sw-2.up } }
  switch north-sw-3 { interface p1 { peer: south-sw-3.p1 } }
}
site carrier-south {
  switch south-sw-1 { interface p1  interface p2 }
  switch south-sw-2 { interface p1  interface p2  interface x { peer: colo-x-rt.south } }
  switch south-sw-3 { interface p1 }
}
site colo-x {
  router colo-x-rt { interface south  interface dc { peer: dc-b-sw-1.x } }
}
site dc-a {
  switch dc-a-sw-1 { interface p1 { peer: dc-b-sw-1.p1 } }
  switch dc-a-sw-2 { interface p1 { peer: dc-b-sw-2.p1 }  interface up }
}
site dc-b {
  switch dc-b-sw-1 { interface p1  interface x }
  switch dc-b-sw-2 { interface p1 }
}

view network {
  title: "Carriers and data centers"
  description: "Two carrier sites, two data centers, a colocation router linking a carrier to a data center."
  select switch, router group-by site
  edges  lift(peer, via: parent, merge: none)
  style {
    edges where source in [colo-x-rt, north-sw-2, dc-a-sw-2]
      and target in [colo-x-rt, north-sw-2, dc-a-sw-2, south-sw-2, dc-b-sw-1] { layout: free }
    edges where source in [colo-x-rt] or target in [colo-x-rt] { line: dashed }
  }
  render svg { satellites: false }
}
```

Wanted: `colo-x` on the side, its two links running along the outside of the grid, as the
`north-sw-2 — dc-a-sw-2` link happens to do on the left. Got: `colo-x` placed wherever Graphviz
puts a frame that no edge holds (on the real map between the two rows, here on the right), and its
links drawn straight to their ends, across the rows of cables between the two carriers and between
the two data centers.

Why: a `layout: free` edge is absent from the layout. The frame whose edges are all free is laid
out as an isolated node, and `cross` does not hold it either (the guide says so). The edge is then
drawn as a curve between its two ends, avoiding the nodes but not the other edges.

Ideas:

- a frame whose edges are all free placed next to its ends, on the side given by `first` / `last`
  or `cross`, rather than as an isolated node;
- free edges routed around the other edges (a channel along the outside of the frames they leave),
  or at least a `side: left | right | top | bottom` on the style rule of an edge;
- with `layout: elk` on the real map, the colocation lands at the end of the row it links and its
  links are routed orthogonally, without crossing the cables: to check on this example, and to
  say in the guide as the way out today if it holds.

### Links between the items of a `stack` level

Two data centers side by side (`stack: [[dc-a, dc-b], …]`), each with a storage VM pinned on a
host, and a host reaching the storage of both sites over iSCSI.

```
site dc-a {
  server host-a3 { serial: "A3" }
  server host-a1 { serial: "A1" }
  virtual server vm-a { on: host-a1 }
  server host-a2 {
    serial: "A2"
    -> store-a : iscsi [flow]
    -> store-b : iscsi [flow]
  }
}
site dc-b {
  server host-b3 { serial: "B3" }
  server host-b1 { serial: "B1" }
  virtual server vm-b { on: host-b1 }
  server host-b2 {
    serial: "B2"
    -> store-a : iscsi [flow]
    -> store-b : iscsi [flow]
  }
}
app store-a { on: vm-a  -> store-b [flow] }
app store-b { on: vm-b  -> store-a [flow] }
site dc-c { server host-c1 { serial: "C1" }  server host-c2 { serial: "C2" }  server host-c3 { serial: "C3" } }
site dc-d { server host-d1 { serial: "D1" }  server host-d2 { serial: "D2" }  server host-d3 { serial: "D3" } }

view storage {
  title: "Storage"
  description: "Two hosts reaching the storage VMs of both data centers over iSCSI, the VMs mirrored."
  select server, app where exists(serial) or name in [store-a, store-b] group-by site > on
  edges  [flow] where proto = iscsi or source.name in [store-a, store-b]
  style {
    edges where source.name in [store-a, store-b] and target.name in [store-a, store-b] { layout: free  routing: straight }
  }
  render svg { satellites: false  direction: bt  stack: [[dc-a, dc-b], [dc-c, dc-d]] }
}
```

Got: the edge from the host of one site to the storage of the other leaves towards its target,
passes it, goes around the frame of the host and comes back into the storage VM from the far side.
Without `stack`, the same edge goes straight to the VM.

Why: the items of a `stack` level stay on one rank, and the edges between them are turned into
`layout: free` edges (the guide says so). The iSCSI edges between `dc-a` and `dc-b` are therefore
drawn after the layout; to reach a node nested in two frames (the host, the VM), the curve looks
for a way in that does not cross the frames, and finds it on the far side.

Ideas:

- an edge that enters nested frames routed through their nearest side;
- the edges between the items of a level kept in the layout across the rank (as a flat edge in
  Graphviz) rather than freed, when they do not go against the order of the stack;
- a warning, or a line in the hover of the view, naming the edges that `stack` freed.

Also: `stack: [[dc-a, dc-b], …]` drew `dc-b` left of `dc-a` here: the order inside a level is
not kept without `cross`.

## `cadastre anonymize`: what got in the way

Making the two examples above from the real map, `cadastre anonymize` gave a copy that does not
build:

- a reference renamed on one side only (`dangling reference: server-407`, did you mean
  `server-207`?);
- addresses renumbered into subnets that overlap (`10.0.27.0/16` and `10.0.28.0/16`), errors that
  the original does not have;
- colors replaced by words (`color: "sed dox"`), where a color is not something that identifies.

A copy that builds, and draws the same views, would make it the way to report a layout problem.

## A view that gives back a whole CMDB

Two spreadsheets of the kind a company keeps beside its map: the CMDB kept by an outsourcer (data
centers, racks, physical servers, logical servers, instances, VLANs, backup policies) and an
application catalog (applications, flows between them, services exposed and consumed, lists of
values, pivot tables). The question: what would it take for views to write them, from the
`.ca` files, instead of keeping them by hand.

### What tables do already

`render csv, xlsx { columns: […] }`, a row per node or per edge, with keys, pseudo-attributes and
paths, gives most of the flat sheets:

- applications: `columns: [name, label, owner, "ancestor(system)", on, "on.on", "on.ancestor(site)", "on.ip"]`,
  the chain application → logical server → physical server through `on.on`;
- servers: `columns: [name, virtual, on, model, serial, rack, cpu, ram, os, ip, "ancestor(site)", hosts, shared]`;
- flows: `select app, saas, job` + `edges [flow]`, `columns: [source, "source.owner", target, "target.owner", proto, port, carries, exchange, description]`;
- the closed values of the schema give the lists of values a spreadsheet checks its cells against.

### What is missing

- **One workbook, several sheets.** An `xlsx` is one sheet named after its view. A CMDB is one
  file of ten sheets: a `workbook` grouping views (`render xlsx { sheets: [servers, instances, vlans] }`,
  or a project option) would write them together, with links between sheets on the names.
- **One row per pair.** A multi-valued path gives one cell, `"srv-a, srv-b"`. The sheet of the
  instances wants one row per instance *and* server, as the general view of a CMDB does
  (instance, logical server, physical server, building): `rows: each(on)`, or a path in `rows`.
- **Column headers.** A column is titled by its path (`on.ancestor(site)`). A sheet given to others
  wants its own words: `columns: ["on.ancestor(site)" as "Site", …]`.
- **Counts and pivots.** `count(path)` exists in filters, not in columns: the number of flows per
  application, of servers per application, of addresses used per subnet (free, allocated,
  reserved) are columns of these sheets. Then the pivot tables: the applications by status and
  criticality, the flows by kind of exchange, the services by consumer. `columns: ["count(on)"]`
  first, then rows grouped by a key with counts (`group: status`, `count: [criticality]`).
- **Computed keys.** A security criticality derived from four ratings (availability, integrity,
  confidentiality, traceability: the highest of them), a business criticality from the impacts
  (money, reputation, people, health): a key defined in the schema from other keys,
  `kind app { dict: max(availability, integrity, confidentiality, traceability) }`, checked and
  shown like a written one.
- **A lifecycle.** Applications, flows and services are active, planned, to be checked,
  abandoned, decommissioned; the sheets filter and count on it. A built-in `status` key with
  known values (and what `planned` or `decommissioned` changes: drawn dashed, left out of the
  impact analysis) rather than a free key each project names differently.
- **Identifiers of links.** A flow catalog numbers its flows and keeps their former numbers and
  the name of the program doing them: links have keys, but no name of their own to be designated
  by (in a process, a check, another sheet).
- **Services as resources.** Services exposed by an application and consumed by others, with
  their inputs and outputs: an `api` or `component` per service and a link per consumer work,
  but a table of consumptions would want the service as the row and the consumer as the column.
- **Policies referenced by servers.** Backup policies (retention per day, week, month, year, logs
  of databases), classes of service, disaster recovery levels: small shared records that many
  servers designate. A custom kind of the schema does it; what is missing is a table that writes
  the record with, for each, what refers to it (`"referrers(backup_policy)"`).
- **Rack elevations.** The building views of a CMDB draw each rack with its units: `rack` and `u`
  exist, there is no rendering of them (`render racks`, a column per rack, a box per device at its
  height).
- **History.** The follow-up sheets (who changed what, when) are the history of the files: a
  column with the date and author of the last change of a resource, from git, would replace them.

### Open

Whether the spreadsheet stays a source for some of it (`for_each … in csv(…)` reads a sheet
saved as CSV, not an `.xlsx`), and how an export from the CMDB of the outsourcer is compared with
the map (`cadastre import csv` bootstraps; a diff, what is in one and not the other, is the next
step).

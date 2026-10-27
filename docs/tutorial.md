# Getting started

Cadastre describes an information system as code: what exists (the **registry**, `.ca` files), and what to draw from it (the **plans**, `.cav` files) and what to check (the **checks**, `.cac` files). The language comes in three levels; most projects live at the first two. This tutorial walks through them with the project of [examples/tutorial](../examples/tutorial), which `cadastre check examples/tutorial` compiles and the playground opens (*tutorial*).

| Level | What you write | What you learn |
|---|---|---|
| 1. The registry | resources, keys, nesting, links | `kind name { key: value }`, `->`, references |
| 2. Views | what to select, how to group it, what to render | `select`, `where`, `group-by`, `edges [layer]`, `render` |
| 3. Further | derived edges, processes, checks, schema | `lift`, `follow`, `process`, `check`, `zone`, `style`, `kind` blocks |

The [guide](guide.md) explains every construct; the [reference](reference.md) lists the exact kinds, keys and values.

`cadastre init my-system` creates a project with this layout and a first registry to replace by yours.

## Level 1: the registry

A resource is a **kind**, a **name** and a block of **keys**. Nesting says what is inside what:

```
team ops

site dc1 {
  hosting: on-prem
  owner: ops
  network lan {
    subnet apps {
      cidr: 10.0.1.0/24
      server web01 { ip: 10.0.1.10 }
      server db01  { ip: 10.0.1.20 }
    }
  }
}
```

- The kinds are those of the language (`site`, `network`, `subnet`, `server`, `app`, `database`…): an unknown kind is an error, an unknown key is accepted, with a warning when it is close to a known one (`verison`).
- A key whose value names a resource is a **reference**: `owner: ops`. Names need only be unambiguous: `web01`, `apps.web01` and `dc1.lan.apps.web01` designate the same server.
- Some keys are inherited along the nesting: the servers of `dc1` are owned by `ops`.

Applications are grouped in **systems**, and run `on` servers. A **link** (`->`) says who calls whom, on which protocol and port, for which layer, carrying which data:

```
data order

system sales {
  owner: sales-team
  app shop {
    on: web01
    -> orders : https/443 [flow] { carries: [order] }
  }
  app orders {
    on: web01
    -> orders-db : tcp/5432 [flow] { carries: [order] }
  }
  database orders-db { on: db01 }
}
```

- A link written inside a resource starts from it; at the root, it names both ends: `shop -> orders : https/443`.
- `[flow]` is a **layer**: what the link is about (application flows, the network, triggers). Views select links by layer.
- What is not known yet can be written as such: `confidence: assumed`, `question: "which port?"`. The compiler lists the incomplete elements instead of refusing them.

`cadastre check` reports errors and warnings; `cadastre fmt` puts the files in their canonical form.

## Level 2: views

A view selects resources, groups them in frames, chooses the edges between them, and renders them:

```
view apps {
  select app, database group-by system
  edges  [flow]
  render svg, text
}
```

- `select` takes kinds, and a filter: `select app where on = web01`, `select app where owner = sales-team`, `select server where ancestor(site) = dc1`.
- `group-by` draws frames: by system, by site, or levels of them (`group-by site > subnet`).
- `edges [flow]` keeps the links of a layer whose two ends are selected.
- `render` names the formats: `svg`, `mermaid`, `drawio`, tables (`csv`, `markdown`, `html`, `xlsx`), `text`… `cadastre build` writes them to `out/`.

The [cheat sheet for views](views.md) gives recipes for the usual questions.

## Level 3: further

### Derived edges: `lift`, `follow`, `process`

Links are written between applications. Three selectors derive other edges from them, without writing anything more:

```
view servers {
  select server group-by site > subnet
  edges  lift([flow], via: on)               // the flows of the apps, between their servers
  render text
}

view order-path {
  select app, database without isolated
  edges  follow(order, from: shop)           // where the orders go, step by step
  render text { edge_labels: [step, port] }
}
```

A **process** names the steps of a business process, in order, from the links of the registry, and draws its sequence diagram:

```
process checkout {
  step shop -> orders
  step orders -> orders-db
  step orders -> shop { reply }
}

view checkout {
  select app, database
  edges  process(checkout)
  render sequence
}
```

The guide compares the three ([Choosing an edge selector](guide.md#choosing-an-edge-selector)).

### Checks: the tests of a project

A check reuses the selection of a view and states what is expected; it lives in a `.cac` file (`checks.cac` in the project of the tutorial), and `cadastre check` runs them:

```
check owned {
  select system where not exists(owner)
  expect none
  message: "every system has an owner"
}
```

### And then

- **Zones** gather frames that no key gives (`zone partners "Partners" [partner-a, partner-b]`), **styles** color what a filter matches.
- The **schema of the project** types its own keys and kinds: `kind server { inventory_id: pattern("INV-[0-9]+") }`, `kind printer { like: device }`, `group storage: database | saas`.
- **Impact analysis** (`where failures has fw01`), **egress** through firewalls and proxies, **exports** (ArchiMate, GraphML, JSON), a one-off **Terraform import**.

Each has its section in the [guide](guide.md).

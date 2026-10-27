# Cadastre guide

> Status: v1 draft. This guide explains the language and records its design decisions, with examples. The exact syntax, kinds, keys, types and values are in the [reference](reference.md), generated from the grammar and the compiler: a key it does not list is a free key.

New to Cadastre? Start with the [tutorial](tutorial.md). The language has three levels: **the registry** (§ 3: resources, keys, nesting, links), **views** (§ 6: `select`, `where`, `group-by`, `edges [layer]`, `render`), and **further** (derived edges with `lift`, `follow` and processes, zones, styles, checks, the schema of the project). The first two are enough for most maps. The README states what is [stable, advanced or experimental](../README.md#stability).

## 1. Files and project

| Extension | Role | Allowed content |
|---|---|---|
| `.ca` | **registry** | resources, attributes, links |
| `.cav` | **plans** | `view` and `style` declarations only |
| `.cac` | **checks** | `check` declarations only: the tests of the project |
| `.csv` | **data** | rows read by a `for_each` (§ 3, Series of resources) |
| `cadastre.toml` | **project file**, optional | its sources, its output directory, its modules |

- A single grammar (`tree-sitter-cadastre`) parses the three extensions. The **compiler** rejects a resource in a `.cav`, a `view` in a `.ca`, a `check` outside a `.cac`: the extension says what a file holds, before opening it, and a continuous integration can run the `.cac` files as the tests they are.
- **A project is a directory.** The compiler recursively loads every `.ca`, merges them into a single registry, then evaluates every `view` of the `.cav` files and runs every `check` of the `.cac` files against that registry.
- **No entry point, no imports between files.** There is no main file: every file of the directory is given to the compiler, and names resolve across the whole project (lexical scope by system, § 3). How files are split is free (by team, by site, by layer); [project-layout.md](project-layout.md) recommends a layout. Importing is reserved for sharing modules between projects (roadmap); project options, if needed later, will go in an optional configuration file, not in an entry point.
- The order of files and declarations does not matter: names are resolved after everything is parsed.
- **The project file**, `cadastre.toml`, is optional. The commands and the editors look for it from the directory given and up, as Cargo does; its directory is then the project, otherwise the directory given alone:

  ```toml
  [project]
  name = "acme-is"
  sources = ["src"]        # the directories read; the directory of the file by default
  out = "out"              # where cadastre build writes; out/ by default
  strict = false           # as --strict
  language = "fr"          # the language of the site and of the words of the diagrams: en (default), fr

  [dependencies]
  shared = { path = "../acme-shared" }
  ```

  An unknown table or key is an error, so that a typo does not go unnoticed.
- **Modules.** A dependency is another Cadastre project (a directory, with its own `cadastre.toml` or not): the registry shared by an organization, its teams, sites, schema and checks. Its resources join the registry under their own names: no prefix, no `import` in the files. What it brings: its schema, its registry, its styles and its **checks**, which then apply to the project (the rules of the organization); not its views, which are not built. It is **read only**: `cadastre fmt` and renaming leave it alone (renaming one of its resources is refused, with the reason); its errors are reported, with its path, not its warnings nor its incomplete elements. A name declared both in the project and in a module, or in two modules, is an error that names them. The modules of a module are read too, each once; a cycle of dependencies is an error. Dependencies are local paths for now.
- **Encoding: UTF-8.** A byte order mark is allowed and ignored. A file that is not valid UTF-8 is an error located at its first invalid byte, with a hint to convert it; the other files are still checked.

```
cadastre init [directory]      # a new project: cadastre.toml, src/ laid out as docs/project-layout.md recommends, a first site, system and view
cadastre check                 # diagnostics only, including the checks of the project (§ 6, Checks)
cadastre check --only owners   # the failures of one check only
cadastre build                 # every plan of the current directory
cadastre build --view flows    # a single plan
cadastre build --strict        # attributes outside the schema and incomplete elements are rejected (§ 4)
cadastre build --incomplete    # details of incomplete elements (§ 7)
cadastre build --out site/     # output directory (default: out/ in the project)
cadastre build --check         # lists the renderings of out/ that are missing or not up to date (exit code 1)
cadastre fmt                   # formats the .ca, .cav and .cac files in place
cadastre fmt --check           # lists the files that are not formatted (exit code 1)
cadastre export archimate      # the whole registry for other tools: archimate, graphml or json (out/model.<extension>)
cadastre anonymize             # a copy of the project with every name, text and address replaced, its shape kept: to share
cadastre site --out public     # the site of the project for its readers: pages, search, JSON model, the playground beside
cadastre serve                 # the same site, served and regenerated from the files as they change
cadastre import terraform terraform.tfstate   # bootstrap: .ca files from a Terraform state, in imported/, to review
cadastre import csv inventory.csv             # bootstrap: a .ca file from an inventory in CSV, in imported/, to review
```

Renderings are written to `out/<view>.svg` (`.dot` when Graphviz is not installed), `.dot`, `.txt`, `.mmd` (Mermaid), `.ascii.txt` (ASCII art), a table as `.csv`, `.tsv`, `.md`, `.html` or `.xlsx`, or `.drawio` (draw.io), depending on the formats of each view.

**Formatting.** `cadastre fmt` (and *Format Document* in the editor) writes the canonical form of a file, whatever its layout; only whitespace changes, and the commas between the items of a block, and comments stay in place.

- **Line breaks.** One statement per line. A block goes on one line when it holds three attributes at most, no nested resource, link nor comment, and its statement fits in 100 columns: `server web01 { os: debian12 }`, `-> db : jdbc [flow] { exchange: both }`; its items are then separated by commas. Otherwise it holds one item per line, without commas. The blocks of `view` and `style` always hold one item per line. A list value goes on one line when it fits, one item per line otherwise. A `select` too long puts `where`, `with ends`, `without isolated` and `group-by` on continuation lines; an `edges` too long puts each selector on its own line, and the options of a `lift` or a `follow` after its edges; a condition still too long is broken before its `and` and `or`. Blank lines between statements are kept, one at most.
- **Indentation and spacing.** Two spaces per open block or bracket, two more for the continuation of a statement; tokens are one space apart (`key: value`, `-> target : https/443 [flow]`, `{ … }`, `[a, b]`).
- **Alignment.** On consecutive statements of the same shape, columns are aligned: the target, `: port`, `[layers]` and `{` of links; the `{` of single-line declarations and style rules (and the target of a style rule); the arguments of the clauses of a view; trailing comments. Lines of a nested block, comment lines and declarations without block (`data articles`) do not break such a run; a blank line does, and so does a statement more than 20 columns longer or shorter than the others.

A file with syntax errors is left as it is.

## 2. Lexical structure

Whitespace and line breaks are **not significant**.

### Comments

```
// line comment
site paris-dc1 {
  hosting: on-prem   // until the end of the line
  /* block,
     over several lines */
}
```

- `//` until the end of the line, `/* … */` for a block. No `#`.
- Block comments do not nest: `/* a /* b */` ends at the first `*/`.
- A `//` inside a string (`"https://…"`) is not a comment.
- Comments are notes for authors, ignored by the compiler. There are no documentation comments: a resource is documented with `description:`.
- Tools (formatter, LSP) must keep comments in place.

### Tokens

| Token | Examples | Rule |
|---|---|---|
| `identifier` | `web01`, `debian12`, `web-01`, `on-prem` | `[a-zA-Z_][a-zA-Z0-9_]*(-[a-zA-Z0-9_]+)*` |
| `qualified_name` | `dmz.web01`, `aws-paris.prod.dmz.web01` | identifiers separated by `.`, no whitespace |
| `number` | `443`, `16.2` | `\d+(\.\d+)?` |
| `semver` | `1.2.3`, `1.2.3-rc.1`, `2.0.0+build.5`, `v1.4.0` | `v?` + `\d+.\d+.\d+` + optional `-prerelease` and `+build` (semver 2.0) |
| `ipv4` | `10.0.1.5` | 4 octets |
| `cidr4` | `10.0.1.0/24` | `ipv4/n` |
| `ipv6` | `fd00::1` | strict regex (`::` or 8 groups) |
| `cidr6` | `fd00::/8` | `ipv6/n` |
| `port_spec` | `https/443`, `tcp/8000-8100`, `icmp` | structured node: `proto` [`/` `port` or `port-port`], no whitespace |
| `string` | `"Customer billing"`, `"ge-0/0/1"` | double quotes, escapes `\"` `\\` `\n` `\t` |

Anything that fits no token is written as a `string`. This is the case for SPDX license identifiers (`"Apache-2.0"`, `"GPL-3.0-or-later"`: `Apache-2` would be read as an identifier, then `.0` would be an error), for values such as `"C++"` or `"C#"`, and for vendor interface names (`"ge-0/0/1"`).

### Disambiguation rules

- An identifier never starts with a digit: `10.0.1.0` is not a qualified name.
- `port_spec` starts with a letter; CIDR and IP start with a digit (IPv4) or a hexadecimal group (IPv6, in value position only).
- A hyphen inside an identifier must be followed by an alphanumeric character: `billing->crm` reads as `billing` `->` `crm`.
- The longest match wins: `10.0.1.5` is an `ipv4`, not a `semver` followed by `.5`; `v1.2.3` is a `semver`, not a qualified name.
- In attribute-key position, `ipv6` is not a valid token: `beef: 1` is an attribute.
- In plans, a path (`on?.ancestor(site)`) is a dedicated rule of the view grammar, not a `qualified_name` (§ 6).

### Raw text

The compiler **keeps the source text of every literal**. The typed value is derived according to the schema only. Hence `16.10` and `16.1` stay distinct as versions, and `0443` can be displayed as written.

## 3. The registry (`.ca`)

### Resources

```
<kind> <name> [ { <elements> } ]
```

- `kind` is **any identifier**, not a keyword. Valid kinds are defined by the schema (§ 4).
- The block is optional: `server web01` is a complete declaration.
- A block contains, in any order:

| Element | Form | Recognized by |
|---|---|---|
| attribute | `key: value` | `identifier :` |
| child resource | `kind name { … }` | `identifier identifier` |
| outgoing link | `-> target …` | `->` |

- Elements are separated by whitespace or by an **optional comma**: `{ os: debian12, arch: amd64 }`.

### Attributes

```
os: debian12
cidr: 10.0.1.0/24
version: 2.4.1
on: web01
on: [web01, web02]
```

- No positional arguments: every piece of data is a named attribute.
- Value: a token (§ 2) or a **list** `[v1, v2, …]`, commas mandatory inside, trailing comma allowed (`[web01, web02,]`) for cleaner diffs.

### Containment

Nesting expresses the hierarchy. **A resource has at most one parent.** Two trees coexist:

- the **infrastructure tree**: site → vpc / network → subnet → server (plus clusters, devices, workstation fleets);
- the **functional tree**: system → app, job, database, saas.

```
site aws-paris {
  hosting: cloud
  vpc prod {
    subnet dmz {
      cidr: 10.0.1.0/24
      server web01 { os: debian12 }
    }
  }
}

system invoicing {
  app billing { on: web01 }
}
```

A logical resource (app, job, database, SaaS) is never in the infrastructure tree: its technical placement goes through `on`. Any second hierarchy (physical location, network attachment of a device, membership of a server in a system) is expressed by a **reference**, never by a second parent.

**Placing by reference: `parent:`.** A resource declared at the root of a file can name its parent instead of being written in its block; it is then exactly as if it were nested. This lets a site be split by functional theme, or a large system over several files:

```
// src/sites/dc-alpha.ca
site dc-alpha { hosting: on-prem, network lan { subnet servers { cidr: 10.1.0.0/24 } } }

// src/infra/billing.ca: the servers of billing, in the subnet of dc-alpha
server billing-srv01 { parent: dc-alpha.lan.servers, ip: 10.1.0.11 }
server billing-srv02 { parent: servers, ip: 10.1.0.12 }

// src/apps/billing-reports.ca: an app of the system billing, written apart
app report { parent: billing, on: billing-srv01, -> db : tcp/5432 [flow] }
```

- **As if nested**: the qualified name (`dc-alpha.lan.servers.billing-srv01`), the lexical scope of the system (`db` above is billing's), inherited keys (`owner`, `operator`, `egress`), consistency checks (an IP outside the CIDR), views (`group-by`, `ancestor(…)`, `parent`) see the resource where it is placed.
- **The name of the parent** resolves like a reference written at the root: any unambiguous suffix, among the kinds the resource can be placed in. A precise parent is expected, as when nested: a server in a site is an approximate placement (incomplete element), a server in a system an error. A parent may itself be placed by `parent:`, in any order; a cycle is an error.
- **Nesting or `parent:`, never both**: `parent:` on a nested resource is an error, and so is `parent:` on a kind always at the root (`site`, `data`…). A resource keeps a single parent, and its keys stay in its own declaration: nothing else adds to a resource declared elsewhere.
- **In the editors**, the name after `parent:` leads to the parent, and the references of the parent list the resources placed in it from other files; completion proposes the precise parents of the kind.

### Attribute or child resource?

> **If it has its own attributes, or if there can be several of them, it is a child resource; otherwise it is an attribute.**

```
app billing {
  on: [web01, web02]
  version: 2.4.1
  language: java
  license: "Apache-2.0"

  component spring-boot { version: 3.2.1, license: "Apache-2.0" }
  component hibernate   { version: 6.4.0, license: "LGPL-2.1-only" }
}
```

- `language` and `license` describe the app itself with a simple value: they are attributes. `license` is an SPDX expression (`"MIT OR Apache-2.0"`, § Value types), checked against the SPDX License List, on an app, a job, a database, a SaaS or a component.
- A framework has its own version and license, and an app uses several of them: it is a `component` resource. It can then be selected in a view (`select component group-by app`) and checked by a future `check`.
- No object values (`framework: { name: …, version: … }`) and no numbered flat keys (`framework_version`): nesting covers the need without extra syntax.
- With a single parent, a component used by several apps is declared in each of them (`billing.spring-boot`, `crm.spring-boot`): these are two usages, possibly with different versions.

### Systems

A `system` groups the logical resources of a functional building block: apps, jobs, databases, SaaS.

```
system pricing {
  description: "Price management"
  owner: pricing-team

  app ui {
    label: "Pricing UI"
    on: pricing-srv01
    -> db : jdbc [flow] { exchange: both }          // pricing.db, by lexical scope
  }
  app article-extractor { -> db : jdbc [flow], -> esb : http [flow] }
  database db { engine: oracle, on: pricing-srv02 }
}

system catalog {
  app api { -> db : jdbc [flow] }                   // catalog.db
  database db { engine: oracle }
}
```

- Systems nest (`parents: none | system`) to express domains and sub-systems.
- Names are local to their system: `pricing.db` and `catalog.db` coexist. Inside a system, `db` first designates the system's own (**lexical scope**, § 5); from outside, write `pricing.db`.
- `owner` and `operator` designate the responsible teams (§ Teams and responsibilities); `criticality`… is free.
- Infrastructure resources (servers, clusters, devices) are attached to a system with the optional reference `system: ref(system)`: `server filehub { system: product }`.
- `group-by system` covers both cases (containment and reference); `lift([flow], via: ancestor(system))` gives the flows between systems (§ 6).

### Technical placement: `on`

`on` tells where a logical resource runs:

| Target of `on` | Case |
|---|---|
| `server` | app, job or database installed on a machine |
| `cluster` | app or job deployed on Kubernetes (§ Clusters) |
| `fleet` | application installed on workstations (§ Workstation fleets) |
| `subnet`, `vpc` | managed service (RDS, bucket…) deployed in that network |
| `database` | a database inside another one: an instance, a pluggable database or a schema of a shared database |

A machine runs `on` a server or a cluster too when it is virtual: `virtual server web01 { on: esx01 }` (§ Virtual machines).

```
system reporting {
  database analytics { managed: rds, on: db }      // db is a subnet
}
```

`on` accepts several values (`on: [web01, web02]`) for a resource spread over several machines.

**Databases in a database.** A shared database server (an Oracle container database, a SQL Server instance) holds the databases of several systems: each of them runs `on` it, and it runs on its machine. Its standby replicates it as a whole:

```
database shared-ora    { on: ora01, engine: oracle }
database shared-ora-dr { on: ora-dr01, engine: oracle }
shared-ora -> shared-ora-dr : oracle-dataguard [replication]

system billing { database invoices { on: shared-ora, label: "INVOICES" } }   // a PDB, a schema
system crm     { database contacts { on: shared-ora } }
```

The chain of `on` is followed to the machine: `group-by on` draws `invoices` and `contacts` in `shared-ora`, itself in `ora01`; `lift([flow], via: on)` takes a flow to `invoices` up to `ora01`; `failures` and `failures(hosting)` of `invoices` hold `shared-ora` and `ora01`. A cycle (`a` on `b`, `b` on `a`) is an error.

**Shared servers.** A server hosting the apps of several systems is not attached to any of them (no `system:`): what it hosts is known from the `on` of the apps, and views read it through the `hosts` and `shared` pseudo-attributes (§ 6, `where` filters):

```
server mutu01 { ip: 10.1.2.40 }                     // shared: no system
system pricing { app ui  { on: mutu01 } }
system catalog { app api { on: mutu01 } }
```

### Links

Two strictly equivalent forms:

**At the root level**, full form:
```
<source> -> <target> [ : <port_spec> ] [ [<layer>, …] ] [ { <attributes> } ]
```
```
billing -> crm : https/443 [flow]
```

**Inside a resource block**, with an implicit source (the enclosing resource):
```
app billing {
  on: web01
  -> crm : https/443 [flow]
}
```

Rules:
- The full form `a -> b` is **forbidden inside a block**.
- **Outgoing links only**: no `<-`. A flow points in the direction of the connection initiator; return traffic is implicit.
- **No chaining** (`a -> b -> c`) and **a single `port_spec` per link**: one line is one fact.
- Layers are free labels: `[flow]`, `[flow, network]`.
- Link ends must belong to the `flow_endpoint` group (§ 4).

### Link attributes

A link accepts an optional attribute block, subject to the same rules as a resource block (open and watchful schema, § 4):

```
app billing {
  -> crm : https/443 [flow] {
    description: "Fetches customer details before invoicing"
    carries: [customer]
  }
}
```

- A `{` that directly follows a link belongs to the link: no ambiguity with the other elements of the block.
- A link block only contains attributes: neither child resources nor links.
- Attributes are not part of the link identity (§ 5).

### Standard layers

Layers remain free labels, but three are standard: known to the default theme and recommended.

| Layer | Meaning |
|---|---|
| `flow` | exchange of business data |
| `auth` | authentication, directory |
| `trigger` | triggering, without business data |

They combine: `[flow, trigger]` for a call that triggers and transmits data.

### Data direction: `exchange`

The `->` arrow gives the direction of the **connection** (who initiates). The direction of the **data** is given by the standard key `exchange`:

| Value | Data goes… | Example |
|---|---|---|
| `push` | from the source to the target | a SaaS drops files over SFTP |
| `pull` | from the target to the source | a batch reads files over SFTP; a consumer reads a topic |
| `both` | both ways | an app reads and writes its database |

```
saas pim      { -> filehub : sftp/22 [flow] { exchange: push } }
app processor { -> filehub : sftp/22 [flow] { exchange: pull } }
```

No default value: an absent key means "direction not specified". Rendering in `data` orientation (§ 6) takes it into account.

### Business objects

What is transmitted or stored is described with `data` resources, usually at the root level, in a dedicated file:

```
data customer { description: "Customer details", classification: personal }
data invoice  { description: "Issued invoice",   classification: confidential }
```

They are referenced by the standard keys `carries` (on links) and `stores` (on databases and SaaS):

```
app crm {
  -> crmdb : tcp/5432 [flow] { carries: [customer, invoice] }
}

database crmdb { on: db01, stores: [customer, invoice] }
```

Unlike a `description`, these references are **queryable**: one can find where `customer` data flows and where it is stored.

### Exchange channels

A **channel** is an intermediary where a producer drops something and a consumer picks it up: topic, message queue, drop directory, bucket.

```
saas kafka-cloud {
  channel prices { type: topic, carries: [item-prices] }
}

site hoster-a-dc {
  server filehub {
    channel product-images { type: directory, path: "/data/images", carries: [product-files] }
  }
}

system product {
  app processor {
    -> kafka-cloud : kafka/9092 [flow] { exchange: push, channel: prices }
  }
}

system catalog {
  app api {
    -> kafka-cloud : kafka/9092 [flow] { exchange: pull, channel: prices }
  }
}
```

- `type` is a known key: `topic`, `queue`, `directory` or `bucket`. `path`, `partitions`, `retention`… are free.
- A channel is placed in its host: `saas`, `app` or `server`. `carries` describes its content.
- The connection goes to the host; the link designates the channel with `channel: ref(channel)*`. The channel must belong to the source or the target of the link, otherwise it is an error.
- Producer or consumer follows from `exchange`: `push` produces, `pull` consumes.

### Processes

A business process is a named resource listing its **steps** in their order: each one a link of the registry, the reply to one, or a processing within a resource; steps may be repeated in a loop. A link may serve several processes, each with its own order: the steps say which links make up a process, including those that do not carry its data (asking a directory where to send it).

```
system invoicing {
  process monthly-invoicing {
    description: "The invoices of the month, from the customer details to the accounting"
    step billing -> crm : https/443
    step crm -> crmdb { note: "customer details" }
    step crm -> billing { reply }
    step billing "generates the PDF invoices"
    loop "for each invoice" {
      step billing -> stripe
    }
    step billing -> legacy-erp { async }
  }
}
```

- **Steps.** `step a -> b` designates the link declared from `a` to `b`; its port (`: https/443`) and its layers (`[flow]`) choose among several. `{ reply }`: the answer to a call, the link declared from `b` to `a`. `{ async }`: the caller does not wait for an answer. `step a "…"`: a processing within `a`, without a link. `label: "…"` gives the text of the arrow, `note: "…"` a note beside it.
- **Order.** The order of the steps is the order they are written in, numbered automatically: no number to maintain when a step is inserted.
- **Checks.** A step designates one link: none, or several without a port or a layer to choose, is an error; so are an unknown flag or key of a step. A process is at the root or in a system; it is a resource (`select process`, hover, references).
- **Links serving a process** without being a step of it, in no order: `processes: [monthly-invoicing]` on the link.
- **Views.** `edges process(monthly-invoicing)` selects the links of the steps, numbered as the steps (`edge_labels: [step]`), and those serving the process; `render sequence` draws its sequence diagram, `render traversal` the devices each of its steps goes through (§ 6, Rendering).
- **Participants.** The sequence diagram places its participants in their order of appearance; `participants: [scheduler, billing]` on the process places these first, in this order (an end of a flow: an app, a job, a database…). A resource shows its `sequence_label` there, if it has one, in place of its label and the `node_labels` of the view: `sequence_label: "Directory\nldap01"` (`\n` breaks the line).
- **Groups of resources.** `participant esx "ESXi hosts" [prod, esx3]` declares, in a process, a participant standing for several resources, named in its steps like a resource, as their source or their target: `step vcenter -> esx`, `step esx -> proxy : nfc/902 { reply }`. Its members are ends of flows, or clusters standing for their servers (`cluster: prod`). The step designates the link of each member, every one checked (`no link vcenter -> dc.esx2 for this step`); the views draw them all, numbered as the step; the sequence diagram shows one participant, its label, otherwise its members as written (`prod, esx3`). The name belongs to the process: it takes precedence there over a resource of the same name, and may be listed in `participants`.

### Sites and hosting

Every place is a `site`, whether it hosts machines or people:

```
site paris-dc1 {
  hosting: on-prem
  address: "12 Example Street, Paris"
  network lan {
    subnet erp { cidr: 192.168.10.0/24 }
  }
}

site colo-pa3 {
  hosting: colocation
  operator: colo-provider            // external team (§ Teams)
}

site aws-paris {
  hosting: cloud
  provider: aws
  region: eu-west-3

  vpc prod    { account: "111111111111", subnet dmz { cidr: 10.0.1.0/24, az: eu-west-3a } }
  vpc staging { account: "222222222222" }
}
```

- `hosting` is a known key: `on-prem`, `colocation`, `cloud` or `office` (an office, where actors are).
- `provider`, `region`, `address` are free keys on the site; the provider operating it is its `operator` (§ Teams and responsibilities).
- The cloud account (`account`) is carried by the `vpc`: an account spans several regions, a region contains several accounts.
- The availability zone (`az`) is an attribute of the `subnet`, not a level of the tree.
- In the cloud, the structure is `vpc`; on premises and in colocation, `network`. An Azure VNet is described as a `vpc` (`provider: azure` on the site).

### Clusters

```
site aws-paris {
  vpc prod {
    cluster k8s-prod { platform: eks, version: "1.30" }
  }
}

system product {
  app processor { on: k8s-prod, namespace: product-prices, replicas: 2 }
  job purge     { on: k8s-prod, schedule: "0 3 * * *" }      // CronJob
}
```

- A `cluster` is placed in the infrastructure hosting it: `site`, `vpc`, `network` or `subnet`.
- `platform` is a key of the project, typed in its schema (§ 4, Schema of the project): `kind cluster { platform: kubernetes | openshift | eks | aks | gke }`, as the examples do.
- Apps and jobs are deployed on it with `on`. `namespace` and `replicas` are free keys. Pods are not modeled: the described unit is the app.
- The nodes of an on-premises cluster are servers referencing it: `server node01 { cluster: k8s-onprem }`.

### Virtual machines

A machine is physical, or **virtual**: it runs on a host, and says so in its declaration.

```
site dc1 {
  cluster prod
  network lan {
    subnet servers {
      cidr: 10.0.0.0/24
      server esx01 { ip: 10.0.0.10, rack: r12, cluster: prod }        // physical
      virtual server web01 { ip: 10.0.0.20, on: esx01 }               // on its host
      virtual server batch01 { ip: 10.0.0.21, on: prod }              // on any host of the cluster
    }
  }
  virtual firewall fw-dmz { on: prod }                                // a virtual appliance
}
```

- **`virtual`** comes before the kind of a machine: `server`, `router`, `switch`, `firewall`, `forward_proxy`, `device`, `load_balancer`, `proxy`. The kind does not change: `select server` takes both, `where virtual` the virtual ones only (`kind server where virtual { line: dashed }`).
- **`on`** gives its host: a server or a cluster (a virtual machine moving between its hosts). Without it, the host is unknown (a cloud instance). The machine stays in its network place (a subnet for a server), its address checked there.
- **Its host carries it**, as for an app (§ Technical placement): `group-by site > on` draws `web01` in the frame of `esx01`, and what runs on `web01` in its own frame; `failures` of `web01` holds `esx01`, and that of an app on `web01` both; `lift([flow], via: on)` takes the flows of `web01` up to `esx01`.
- **Checks.** `on` on a machine that is not `virtual` is an error (a warning for a `load_balancer` or a `proxy`, which took `on` before); `rack` on a virtual machine is an error, its rack being that of its host; `virtual` on what is not a machine is an error.
- The ArchiMate export makes a physical server a `Device` and a virtual machine a `Node`; the Terraform import declares the cloud instances `virtual server`.

### Pools of servers

A **pool** is a group of servers an app, a job or a database runs on, when it runs on all of them: a farm, the servers of the stores. Each server declares its pools, and the app designates the pool, not a list of servers to keep up to date:

```
pool store-servers { label: "Store servers" }

server srv-lil { pool: store-servers }      // in its site and subnet
app till { on: store-servers }
```

- `pool` is a root-level resource, or in a site or a system; `pool:` takes one or several pools; `on` accepts a pool for an app, a job or a database.
- In views, a pool is a resource like the others: `group-by pool` draws a frame per pool with its servers, `lift(…, via: on)` folds the flows of the app onto the pool, `hosts` of a pool lists what runs on it.
- A `cluster` is a platform (Kubernetes, vCenter) servers belong to; a pool only says that something runs on all of them.

The members of a pool can also be **selected** rather than declared one by one: a `select` in the pool, with the kinds and the filter of a view, takes every matching resource, present and future.

```
pool store-servers {
  label: "Store servers"
  select server where ancestor(site).role = store
}
```

- Only a pool selects; the kinds are those `on` designates (`server`, `cluster`, `pool`, `fleet`, `subnet`, `vpc`, and the kinds of the project like them). A selected resource is a member as if it declared `pool:`, both ways adding up.
- The filter is that of views (keys, paths, `ancestor(…)`, `has`, `count(…)`), apart from what depends on the pools themselves: `hosts`, `shared`, `failures`.
- A selection matching nothing is a warning. `select:` followed by a colon stays a key.

### Series of resources: `for_each`

A `for_each` declares resources and links once per item of a list, its variables replaced: a site and a server per store, from a list of codes.

```
for_each (code, ip) in [[lil, 10.1.1.10], [lyo, 10.1.2.10], [nte, 10.1.3.10]] {
  site store-$code {
    hosting: office
    label: "Store $code"
    network lan {
      subnet main {
        cidr: 10.1.0.0/16
        server srv-$code { ip: $ip, pool: store-servers }
      }
    }
  }
}
```

- One variable takes each item (`for_each code in [lil, lyo]`); several variables, in parentheses, take the values of each item, a list of as many values (`[[lil, 10.1.1.10], …]`).
- `$code` is replaced in names (`srv-$code` becomes `srv-lil`), in qualified names and references, and in strings (`"Store $code"`). A variable alone as a value takes the value of the item, its type included: `ip: $ip` is an address, checked against the subnet. A `$` followed by no variable stays, in a string (`"$Q scheduler"`); in a name, it is an error.
- A `for_each` stands at the root level or in a block, and may hold resources, links (`source -> target` anywhere, `-> target` in a block) and other `for_each`.
- The generated resources are ordinary resources: named, filtered, drawn like the others; a name generated twice is an error, as any duplicate. `cadastre fmt` breaks a long list one item per line.

**From a CSV file.** The items may come from a spreadsheet: `csv("stores.csv")` in place of the list, its path relative to the file writing it.

```
for_each (code, ip, label) in csv("../data/stores.csv") {
  site store-$code {
    hosting: office
    label: $label
    network lan { subnet main { cidr: 10.1.0.0/16, server srv-$code { ip: $ip } } }
  }
}
```

```
code;label;ip
lil;Store of Lille;10.1.1.10
lyo;;10.1.2.10
```

- Each variable takes the column of the same name, from the header line: a variable without a column is an error, listing the columns; the other columns are ignored.
- A cell is read as a value of the language when it is one token (`10.1.1.10` an address, checked against its subnet; `16` a number; `web01` a name), as a string otherwise. An **empty cell is absent**: the key it gives is not written (`label` of `store-lyo`).
- The file is UTF-8 (a byte order mark is allowed), its separator `,` or `;` (as Excel writes it in many languages), found on the header line; fields with a separator, a quote or a line break are quoted, as RFC 4180 says (`"Store; Lille"`, `""` for a quote). A wrong value is reported on its row of the CSV file.
- The `.csv` files of the sources are read with the project, as the `.ca` files.
- The file stays the source: a row added to it is a store more at the next compilation. For an inventory read once, its rows of every kind, its hierarchy, and then maintained in `.ca` files, `cadastre import csv` (§ 6, Bootstrapping a registry).

### Workstation fleets

A `fleet` describes a set of workstations, like a population:

```
site stores {
  hosting: office
  fleet store-workstations { size: 2000, os: windows11 }
}

system labels {
  app desktop { on: store-workstations, -> label-server : http [flow] }
}
```

- A fleet is placed in a `site`, a `network` or a `subnet`, or at the root.
- People (`actor`) use workstations (`fleet`), which host the installed applications.

### Jobs and scheduling

A `job` is a process that starts, runs, then stops, unlike an `app`, which runs continuously.

```
app central-scheduler { label: "Central scheduler" }

system pricing {
  job extractor {
    on: pricing-srv01
    schedule: "0 2 * * *"
    command: "java -jar main.jar"
    -> db : jdbc [flow] { exchange: pull }
    -> franchise-portal : ftp/21 [flow] { exchange: push, carries: [item-prices] }
  }

  job price-computation { on: pricing-srv01, scheduler: central-scheduler }

  job price-push {
    schedule: "0 6 * * *"
    -> ui : http [trigger] { description: "Triggers the price push" }
  }
}
```

- A job is placed in a system or at the root; `on: ref(server | cluster)`.
- `schedule` is a string: a **cron expression**, recognized by its shape (five fields, minute hour day-of-month month day-of-week, of numbers, `*`, `,`, `-`, `/` and the names `JAN`…`DEC`, `SUN`…`SAT`, or a macro: `@daily`, `@hourly`, `@weekly`, `@monthly`, `@yearly`), or **free text** ("10h, 12h10, 19h", "toutes les 15 min"). A text is taken for a cron expression when it has four to six fields, all of them but one at most made of cron values: `0 3 * FOO *` is a wrong cron expression, `toutes les 15 min` free text. A cron expression is checked (an hour of 25, four fields, a reversed range are errors) and shown in plain words in node labels (`node_labels: [schedule]`), tooltips and the editor's hover: `0 6,18 * * 1-5` reads "at 06:00 and 18:00 on Monday to Friday", `*/15 * * * *` "every 15 minutes". Free text is kept as written, and makes the job incomplete (§ 7): to be converted into a cron expression once known. `timezone` ("Europe/Paris"), on the job or on its scheduler, is shown after the words. `command`, `runtime` are free.
- `scheduler: ref(app)` designates a central scheduler. Without `scheduler`, a job with a `schedule` is scheduled locally (crontab, systemd, CronJob).
- Triggering through a network call is a link of the `trigger` layer.

### SaaS services

A SaaS service is a `saas` resource, at the root or in a system: its hosting is not managed by us, so it has no `on:`.

```
saas stripe {
  description: "Online invoice payment"
  vendor: "Stripe"
  url: "https://api.stripe.com"
  region: eu
  stores: [customer, invoice]
  -> billing : https/443 [flow] { description: "Payment webhooks", carries: [invoice] }
}
```

- `stores` tells which data is hosted by the vendor; `region` (free key) tells where.
- Several instances of the same service are distinct resources: `salesforce-prod`, `salesforce-sandbox`.
- Flows to and from the SaaS are ordinary links.
- In a view with `lift`, selecting `saas` is enough to keep the service as an end (§ 6).

### Actors

The humans using the information system are `actor` resources: **populations or roles, never individuals** (the registry is versioned and read by everyone; it must contain neither personal data nor changes at every arrival or departure).

```
actor customers {
  description: "End customers, through the web portal"
  population: external
  size: 50000
  -> lb01 : https/443 [flow] { carries: [customer] }
}

site paris-hq {
  hosting: office
  firewall fw-hq { … }
  actor hq-staff {
    population: internal
    -> legacy-erp : https/443 [flow] { through: [fw-hq] }
  }
}
```

- `population` is a key of the project, typed in its schema: `kind actor { population: internal | external | partner }`, as the reference example does. `size` is free.
- An actor is at the root (population without a place) or in a site (people in an office), which allows lifting its flows to the site level.
- `actor` belongs to `flow_endpoint`: it initiates its flows.
- **Client VPN**: an actor whose flows go through a VPN gateway, with no extra concept:
  ```
  actor remote-staff {
    population: internal
    -> legacy-erp : https/443 [flow] { through: [fw01.vpn, fw01] }
  }
  ```
- An **identity provider** (Okta, Entra ID, Keycloak) is an ordinary `saas` or `app`, with its flows; an app's authentication mode (`auth: sso`) is a free key.

### Teams and responsibilities

```
team pricing-team { label: "Pricing team", contact: "#team-pricing" }
team infra        { label: "Infrastructure team" }
team hoster-a     { label: "Managed hosting provider", scope: external }

system pricing {
  owner: pricing-team                // inherited by ui, extractor, db
  app ui { … }
  job extractor { … }
  database db { operator: infra }    // override: operated by infra
}

site hoster-a-dc {
  owner: infra
  operator: hoster-a                 // inherited by the whole site
  server filehub { owner: pricing-team }   // override
}
```

- A `team` is a **team, never a person** (same rule as actors). `contact` (free key) designates a channel or a shared mailbox. Teams nest (`parents: none | team`); a provider is a `scope: external` team.
- Two standard keys, `ref(team)`, on any resource:

| Key | Role | Question |
|---|---|---|
| `owner` | accountable: decides, arbitrates, holds the budget | "Who decides for this component?" |
| `operator` | operations: runs, patches, monitors | "Who do we call when it breaks?" |

- **Inheritance through containment**, in both trees: the children of a system inherit its `owner` and `operator`, the infrastructure of a site inherits the site's. A value written on a child overrides the inherited value. Inheritance does **not** follow references (a server attached with `system:` does not inherit the system's `owner`).
- Filters and `group-by` see the **effective value**, inheritance included: `where owner = pricing-team`, `group-by owner`.
- `owner` is required (§ 4) for `system`, `app`, `job`, `database`, `saas`, `server` and `cluster`: a component without an owner, after inheritance, is **incomplete**, unless `scope: external`. `operator` is optional.
- Other roles (development, support, security) are free keys in v1.

### Network devices

| Kind | Role | In a flow |
|---|---|---|
| `router` | routing between networks | traversed (`through`) |
| `switch` | layer-2 switching (layer 3 if it has IP interfaces) | traversed |
| `firewall` | filtering | traversed |
| `vpn_gateway` | VPN termination | traversed |
| `forward_proxy` | egress proxy | traversed |
| `load_balancer` | load balancing | **end** |
| `proxy` | reverse proxy, WAF, API gateway | **end** |
| `device` | anything else, specified by a key of the project (`role: "…"`: IPS probe, Wi-Fi access point…), or a kind of the project `like: device` (§ 4) | either |

- **Traversed or end.** A transparent device does not terminate the connection: the flow goes through it (`through`). A device that terminates the connection creates two flows, often on different ports, and is therefore an end:
  ```
  load_balancer lb01 {
    -> billing : http/8080 [flow]
  }
  saas stripe {
    -> lb01 : https/443 [flow]
  }
  ```
- **Multi-role devices.** Pick a main kind (what you buy, what you see in the rack). The VPN function, which has its own attributes, is a child resource: `firewall fw01 { vpn_gateway vpn { … } }`. Other roles go into a free key: `roles: [router]`.
- **`device`** is the escape hatch for any device without a dedicated kind, since an unknown `kind` is an error.
- **Hardware or software.** A network device is either an appliance (F5, ALB), placed in the infrastructure, or virtual, running on a host (§ Virtual machines): `virtual firewall fw-dmz { on: prod }`. A `proxy` or a `load_balancer` may also be software placed in a system: `system platform { virtual proxy ingress-nginx { on: k8s-prod } }`.
- **Firewall rules** and **security groups** are not modeled: they are projections of the registry flows.
- **Cloud gateways** (internet gateway, NAT gateway, Direct Connect gateway) and **transit gateways** are described as a `router` in the `vpc` or the site (§ Egress and ingress, § Connectors).

### Virtual IPs and DNS names

A **virtual IP** (`vip`) is an address and / or host names served by a load balancer, a proxy or a cluster (ingress). It is a child of the device that serves it, so one device carries as many as it serves:

```
load_balancer f5-prod {
  vip labels {
    host: [labels.example.com, labels-backup.example.com]
    ip: 10.1.2.30
    subnet: dmz
    -> label-server : http/8080 [flow]
  }
  vip intranet { host: "intranet.example.com" }
}

app desktop-label {
  -> labels.example.com : https/443 [flow]          // the vip labels, by its host name
}
```

- **Keys.** `host`: one or more DNS host names, written bare (`labels.example.com`) or as a string (a name whose labels start with a digit, such as `"3d.example.com"`, can only be written as a string); `ip`: its address; `subnet: ref(subnet)`: the subnet holding the address.
- **Flow target.** A `vip` belongs to `flow_endpoint`: flows towards it terminate at the device, which opens its own flows to the back ends, from the `vip` block. A link end is written with the name of the `vip` (`-> labels`, `-> f5-prod.labels`) or **with one of its host names** (`-> labels.example.com`): a name designating no resource is looked up among host names (§ 5, Names).
- **Views.** A `vip` is a node like any other (`select vip`, `group-by load_balancer`); without a `label`, it shows its first host name.
- **Checks.** The address is within the CIDR of `subnet` and unique in it, servers and interfaces included; outside any subnet, unique among virtual IPs. A host name is served by one virtual IP only. A `vip` with neither `ip` nor `host` is incomplete.

### URLs and access

A resource gives the address of its service with `url`, and the access for its administration with `console`: a web console (iDRAC, iLO, vCenter, the interface of a firewall), SSH, RDP.

```
server web01 {
  ip: 10.0.1.10
  console: ["https://idrac-web01.example.com", "ssh://admin@web01.example.com"]
}
app labels { on: web01, url: "https://labels.example.com" }
```

- **Keys.** `url` on `app`, `saas`, `component` and `vip`; `console` on `app`, `database`, `saas`, `server`, `cluster` and network devices. Both take one or more URLs, as strings. On another kind, `url` is a free key, not checked.
- **Checks.** A URL is `scheme://[user@]host[:port][/path][?query][#fragment]`, the host being a host name, an IPv4 address or a bracketed IPv6 address (`https://[fd00::1]:8443`). **No secret**: a password (`https://admin:secret@host`) or a query parameter holding one (`?token=`, `?apikey=`, `?password=`…) is an error, the registry being versioned and read by everyone; a user name is accepted (`ssh://admin@fw01`).
- **Renderings.** In SVG, a click on a resource opens its first `url`, otherwise its first `console`, in a new tab (an `ssh://` or `rdp://` link opens the client the system associates with it); Mermaid gets a `click … href`. HTML tables show the URLs of a column (`columns: [name, console]`) as links, and the hover in the editor too.

### Interfaces

Devices reach subnets through their **interfaces**, child resources that reference their subnet. A device can thus be attached to several subnets without having several parents. A `device` with a single attachment (a Wi-Fi access point in its VLAN) can also be placed in its subnet, like a server, with its `ip`.

```
firewall fw01 {
  interface wan     { ip: 203.0.113.2 }
  interface lan-dmz { subnet: dmz, ip: 10.0.1.1 }
  interface lan-db  { subnet: db,  ip: 10.0.2.1 }
}

switch sw01 {
  interface ge-0-0-1 { label: "ge-0/0/1", peer: web01 }
  interface ge-0-0-2 { label: "ge-0/0/2", peer: fw01.lan-dmz }
}
```

- `subnet` is optional: a switch port or a WAN interface has none.
- **Implicit server interface.** A server placed in a subnet has an implicit interface in that subnet; its address is written directly on the server (`server web01 { ip: 10.0.1.10 }`). Additional interfaces (multi-homing, management, backup) are declared explicitly.
- **`peer`** represents a cable. It takes a single value, is declared on one side only, and the compiler makes it symmetric. Declaring both sides consistently is accepted; a contradiction (A → B and B → C, or A → B and C → B) is an error.
- `peer: web01` (a server) designates its implicit interface. If the server has explicit interfaces, name the interface (`web01.eth1`).
- The Cadastre name of an interface is an identifier; the vendor name, which often contains `/` or `.`, goes into `label`.
- **VLANs.** A port carries one VLAN, untagged (`vlan: 65`, an access port), or several, tagged (`vlans: [64, 65, 3001]`, a trunk); a subnet has its VLAN (`subnet esx { cidr: 10.140.65.0/24, vlan: 65 }`). A network path in the subnet only takes the cables whose two ports carry its VLAN; a port without `vlan` nor `vlans` carries any (§ Network paths).
- **Aggregates.** `aggregate: "po31"` names the port-channel (or vPC) a port belongs to: its cables between two devices make one link in a path view.

### Racks

The physical location is a **reference** to a rack, the containment tree remaining logical:

```
site paris-dc1 {
  rack r12 { room: "Room 2", units: 42 }

  switch sw01 { rack: r12, u: 42 }

  network lan {
    subnet erp {
      server erp01 { rack: r12, u: 10 }
    }
  }
}
```

- `rack` is a resource whose parent is a `site`.
- Servers and network devices reference it with `rack: ref(rack)`. `u` and `room` are free keys.
- The rack of a resource must be in the **same site** as the resource, otherwise it is an error.

### VPNs and connectors

A **connector** is a resource with two symmetric ends, drawn as an edge in views (§ 4, § 6). Three kinds: `vpn_tunnel`, `vpc_peering`, `wan_link`. Their two ends must be distinct.

#### VPN tunnels

```
site paris-dc1 {
  firewall fw01 {
    vpn_gateway vpn { public_ip: 203.0.113.2 }
  }
}

site aws-paris {
  vpc prod {
    vpn_gateway vgw { label: "vgw-0a1b2c" }
  }
}

vpn_tunnel paris-aws-1 { ends: [fw01.vpn, vgw], protocol: ipsec }
vpn_tunnel paris-aws-2 { ends: [fw01.vpn, vgw], protocol: ipsec, description: "Backup tunnel" }
```

- `vpn_tunnel` is declared at the root: it belongs to both sides.
- `ends` has exactly two values, unordered: a tunnel is symmetric.
- `protocol` is a key of the project, typed in its schema: `kind vpn_tunnel { protocol: ipsec | wireguard | openvpn | gre }`, as the reference example does. The rest (encryption, IKE…) is free.
- One tunnel per resource: redundancy is written as several tunnels.

#### VPC peering

```
vpc_peering prod-shared {
  ends: [aws-paris.prod, aws-paris.shared]
  description: "Access to shared services"
}
```

`ends: ref(vpc){2}`, across accounts and regions.

#### Dedicated links

Direct Connect, ExpressRoute, Cloud Interconnect, MPLS, leased line, dark fiber: a single `wan_link` kind, specified by `technology`.

```
wan_link dx-paris {
  ends: [rt-dc1, dxgw]
  technology: direct_connect
  bandwidth: 10Gbps
  operator: colo-provider
  circuit_id: "dxcon-ffabc123"
}
```

- `technology` is a key of the project, typed in its schema: `kind wan_link { technology: direct_connect | expressroute | interconnect | mpls | leased_line | fiber }`, as the reference example does.
- `ends: ref(router | firewall | vpn_gateway){2}`. On the cloud side, the Direct Connect or ExpressRoute gateway is a `router` of the cloud site.
- `bandwidth` is typed in the schema of the project (`kind wan_link { bandwidth: bandwidth }`), then compared in filters: `select wan_link where bandwidth < 1Gbps`. `circuit_id` is a free key; the link carrier is its `operator`.

#### Transit gateway

A transit gateway (or an Azure hub) has N attachments: it is not a connector, but a `router` with one interface per attachment:

```
site aws-paris {
  router tgw {
    interface att-prod   { subnet: prod.dmz }
    interface att-shared { subnet: shared.tools }
  }
}
```

### Traversals

The standard key `through` lists, in order from the source to the target, the devices and tunnels a flow goes through:

```
app billing {
  -> legacy-erp : tcp/1521 [flow] {
    description: "Accounting export to the datacenter ERP"
    through: [paris-aws-1, fw01]
  }
}
```

- Items belong to the `transit` group (§ 4): traversed devices and connectors, but neither load balancers nor reverse proxies, which are ends.
- A **partial path** is allowed: list what matters, usually firewalls and tunnels. Its completeness is not checked; a device in none of the sites of the ends is reported (§ 5, Consistency checks).
- A flow without a written `through` gets the one of the cabling (§ Network paths), and the `egress` or `ingress` of the internet (§ Egress and ingress).

### Network paths

The path of a flow between two machines is deduced from the cabling (`peer`), the subnets and their gateways:

```
site dc1 {
  network adm {
    subnet esx { cidr: 10.0.65.0/24, vlan: 65 }
    subnet dmz { cidr: 10.0.40.0/24, vlan: 40, gateway: fw.dmz }
  }
  switch sw1 {
    interface eth1 { peer: esx01.p1, vlan: 65 }
    interface eth2 { peer: sw2.eth2, vlans: [40, 65] }      // a trunk
  }
  firewall fw {
    interface dmz { subnet: dmz, ip: 10.0.40.1 }            // a gateway: an interface in the subnet
    interface esx { subnet: esx, ip: 10.0.65.1 }
  }
  server esx01 { ip: 10.0.65.11, interface p1 }
}
```

- **Its ends** are machines: what runs on a machine (`on`) starts from it, a virtual machine from the cables of its host (§ Virtual machines), its host not shown in the path. A machine is in a subnet by its place, or by its `ip` (the narrowest `cidr` holding it).
- **In a subnet**, the path follows the cables through **switches** only, the two ports of each cable carrying the VLAN of the subnet. No firewall, no router.
- **Between subnets**, it goes up to a **gateway** of the first subnet, from router to router (through the subnets they share and the connectors between them, `wan_link`, `vpn_tunnel`), then down to the second. The gateway of a subnet is its `gateway` (an interface or its device), otherwise the routers and firewalls with an interface in it, by `subnet` or by address.
- **To and from the internet**, the path up to the device of the `egress` comes before it, the path from the device of the `ingress` after it. An end reached over the internet (a `saas`, the `internet`, `reach: internet`) has no machine: the path ends at the `egress` of the other end, or starts at its `ingress`.
- **Redundant ways** make one path whose steps have variants: two switches in vPC, two interconnections, `through: col-sw10-01 or col-sw10-02, gs-sw10-01 or gs-sw10-02`. `through has col-sw10-02` holds.
- **What it produces**: a flow without a written `through` gets the devices of the path, as for `egress`: filters, `through: show`, `lift`, checks, tooltips and text outputs, which say where it comes from (`(deduced: cabling)`). Only a complete path is kept: a flow whose ends are not cabled, or whose subnet has no known gateway, keeps no `through`, silently. A written `through` wins; one naming a switch, a router or a firewall off the path of the cabling is warned about.
- **Why there is no path** is said by a view drawing it, `edges path(a, b)` (§ 6, `edges`): the cables stop at a device (in a VLAN), a machine is in no subnet, a subnet has no gateway; and a subnet routed by several devices without a `gateway` is reported.
- **A traversal** draws paths as rows of boxes, `render traversal` (§ 6, Rendering): the path between two resources (`edges path(a, b)`), or that of each step of a process, a box at each device traversed, a switch crossed by the call and by its reply appearing in both rows.

### Egress and ingress

The devices a flow to the internet goes through are written once, with `egress`, rather than on each flow; `ingress` does the same for the flows coming from the internet:

```
site aws-paris {
  vpc prod {
    egress: [nat-gw]                        // every flow to the internet from this VPC
    router nat-gw { label: "nat-0a1b2c", public_ip: 15.188.1.1 }
  }
}

site paris-dc1 {
  egress: [fw01, px01]
  ingress: [waf01, fw01]                    // what comes from the internet
  forward_proxy px01 { public_ip: 203.0.113.10 }
  network lan { subnet erp { egress: [fw01] } }   // a subnet may be more precise
}

internet internet                           // the internet as an end, when the destination is not modeled

app billing    { on: web01, -> stripe : https/443 [flow] }      // through: [nat-gw], deduced
app legacy-erp { on: erp01, -> stripe : https/443 [flow] }      // through: [fw01], deduced
app updater    { on: erp01, -> internet : https/443 [flow] }    // through: [fw01], deduced
app exception  { on: erp01, -> stripe : https/443 [flow] { through: [fw01, px01] } }   // written: it wins
```

- **To the internet**: a flow whose target is reached over the internet, from a source that is not. **From the internet**: the other way around.
- **Reached over the internet**: a `saas` and the `internet`, and what is marked `reach: internet`, a partner's application for instance (`app carrier-api { scope: external, reach: internet }`). `reach` is inherited along containment, then `on`: `reach: internet` on a site or a network covers what it holds. `reach: internal` says the opposite, for a SaaS reached through a private link. `scope: external` says something else, **outside the perimeter**: a directory run by another team, or a hosting provider's site joined through the WAN, are external and internal to the network.
- **Where it is written**: `egress` and `ingress` go on a `site`, a `vpc`, a `network`, a `subnet`, a `system`, an `app`, a `job` or a `database`. The **nearest** counts: the resource's own, then its enclosing resources', then those of what it runs on (`on`): an app on a server of the `erp` subnet takes the `egress` of `erp` before the site's, and its own before both.
- **What it produces**: a flow without a written `through` gets `through` = the `egress` of its source (to the internet) or the `ingress` of its target (from the internet). This deduced `through` is a `through` like any other: filters (`through has px01`), `through: show`, `lift`, checks (`expect all through has fw01`); tooltips and text outputs say where it comes from: `through: fw01, px01 (deduced: egress of site paris-dc1)`. A written `through` always wins: it describes the exceptions.
- **Only traversed devices**: the items of `egress` and `ingress` are of the `transit` group, as for `through` (firewall, forward proxy, NAT router, tunnel); a reverse proxy or a load balancer remains an end of the flow.

- A **NAT** or **internet gateway** is a traversed `router`.
- An **egress proxy** is a traversed `forward_proxy`: the functional flow (`legacy-erp → stripe`) and what it carries are preserved, whereas as an end it would be split in two.
- **TLS inspection** does not change the model; it is noted with the free key `tls_inspection: true`.
- `public_ip` (free key) gives the egress IPs, often required by SaaS vendors for their allow-lists (`egress-ips` view, § 6).

### Partial knowledge and uncertainty

A registry is built progressively. **An imprecise model is accepted; a wrong model is rejected.**

**What we do not know: we do not write it.** An unknown protocol is a link without a `port_spec`, unknown hosting is an app without `on`. Missing required attributes make the element **incomplete** (§ 4, § 7), without an error.

**Approximate placement.** A resource can be placed in any possible ancestor of its normal parent, or at the root: a server whose subnet is unknown is placed directly in its site.

```
site hoster-a-dc {
  hosting: colocation
  operator: hoster-a                 // managed hosting provider, external team
  server filehub { os: redhat }      // subnet unknown
}
```

**What we doubt: `confidence`.** On any resource and any link:

| Value | Meaning |
|---|---|
| `confirmed` (default) | verified |
| `assumed` | assumed, to be verified |
| `doubtful` | doubtful, possibly wrong or obsolete |

Writing a fact asserts it: only doubts are annotated.

**What we wonder: `question`.** A string, or a list of strings, on any resource and any link:

```
app franchise-portal { question: "franchise-portal or franchise-portal-v2?" }

app catalog-api {
  -> logistics : http [flow] {
    confidence: assumed
    question: "Flow direction? Does the catalog API call logistics, or the other way round?"
  }
}
```

**Out of scope: `scope`.** `internal` (default) or `external`: an `external` resource exists and exchanges with the information system, but its inside is not described. It is never reported as incomplete. Not to be confused with `saas`, which is about hosting, not responsibility.

```
app esb { scope: external }
```

**Only what is asserted is checked**: without a known subnet, no IP / CIDR check; without a known site, no rack consistency check.

The `to-verify` and `to-complete` views (§ 8) list what remains to be validated and completed.

## 4. The schema

The schema defines resource kinds, their parents, their attributes and their constraints. In v1 it is **built into the compiler**; its internal format is designed to become declarable in a `.ca` later (`kind … { … }`).

Illustrative form:
```
kind subnet {
  parents: vpc | network
  attrs:  cidr: cidr (required)
}

kind app {
  parents: none | system
  attrs:  on: ref(server | cluster | fleet | subnet | vpc)+ (required)
          version: semver
}

kind vpn_tunnel {
  parents:    none
  connector: ends
  attrs:     ends: ref(vpn_gateway){2} (required)
             protocol: symbol
}

group connector = vpn_tunnel | vpc_peering | wan_link
```

### Value types

| Type | Accepts | Note |
|---|---|---|
| `symbol` | `identifier` | enums, booleans (`true`, `amd64`, `on-prem`) |
| `string` | `string` | |
| `number` | `number` | |
| `semver` | `semver` | strict, comparable (`>=`, `<`), `v` prefix ignored when comparing |
| `version` | `semver`, `number`, `string` | lenient: `16.2`, `"6.1.0-18-amd64"` |
| `ip` / `cidr` | `ipv4`, `ipv6` / `cidr4`, `cidr6` | |
| `host` | `identifier`, `qualified_name`, `string` | DNS host name (RFC 1123: labels of letters, digits and hyphens), compared in lowercase |
| `url` | `string` | `scheme://[user@]host[:port][/path]`, without a password nor a secret query parameter (§ 3, URLs and access) |
| `bandwidth` | `quantity`, `string` | a number and a decimal unit, `bps`, `kbps`, `Mbps`, `Gbps`, `Tbps` or `bit/s`, `kbit/s`…: `100Mbps` (no space), `"1.5 Gbit/s"`; compared in bits per second (`where bandwidth >= 1Gbps`) |
| `spdx` | `string` | an SPDX license expression: identifiers of the SPDX License List (any case), `+`, `AND`, `OR`, `WITH <exception>`, parentheses, `LicenseRef-<name>` for another license: `"MIT OR Apache-2.0"`; an unknown identifier is an error, with the closest one |
| `ref(K)` | `identifier`, `qualified_name` | reference to a resource of kind `K` |

### Reference cardinality

| Form | Meaning |
|---|---|
| `ref(server)` | a single value |
| `ref(server)+` | one or more (list accepted) |
| `ref(server)*` | zero or more |
| `ref(vpn_gateway){2}` | exactly n values |

Whether the attribute must be present is set separately, with `(required)`.

### Required attributes and default values

- `(required)` means "expected for a complete model". A **missing** required attribute makes the element **incomplete** (§ 7); a value that is **present but invalid** remains an error. With `--strict`, incomplete elements become errors.
- A standard key can have a **default value**, which filters see as if it were written: `confidence` is `confirmed`, `scope` is `internal`.
- A standard key can be **inherited** along the containment tree: `owner` and `operator`. The **effective value** of a resource is its written value, otherwise the value of its nearest parent that has one; filters, `group-by` and the required-attribute check use the effective value.

### Approximate placement

The schema declares the **precise** parent of a kind (`server { parents: subnet }`). The compiler also accepts any **possible ancestor** of that parent, derived from the declared parents of the kinds (`network`, `vpc`, `site`), as well as the root; such a placement makes the element incomplete. An impossible placement (a `server` in an `app` or a `rack`) remains an error.

### Unions, root and groups

- `K1 | K2` is a **union** of kinds. The same notation is used for parents (`parents: vpc | network`) and for references (`ref(interface | server)`).
- `none` designates the **root** and can be part of a union: `parents: none | subnet`.
- A **group** is a named union alias, usable wherever a kind is expected (schema, `select`, `ancestor(K)`): `network_device`, `connector`, `transit`, `flow_endpoint`, listed with their kinds in the [reference](reference.md#groups).

- An impossible parent is an error (see approximate placement above). Conditional constraints (a `vpc` only in a `cloud` site) cannot be expressed in v1.

### Connector kinds

A kind marked `connector: <attr>` has a `{2}` reference that defines its two ends, which must be distinct. In a view, it is selected as an (undirected) edge and lifted with `lift` (§ 6); the `connector` group is selected the same way. A reference attribute cannot be named after a connector kind or group.

### v1 standard kinds

The kinds, their parents and their keys are listed in the [reference](reference.md#kinds). Three families:

- **infrastructure**: `site`, `rack`, `vpc`, `network`, `subnet`, `server`, `cluster`, `fleet`, `interface`, network devices (`router`, `switch`, `firewall`, `forward_proxy`, `device`, `load_balancer`, `proxy`, `vpn_gateway`), `vip`, connectors (`vpn_tunnel`, `vpc_peering`, `wan_link`) and `actor`;
- **organization**: `team`;
- **functional**: `system`, `app`, `job`, `database`, `saas`, `component`, `channel`, `data`.

**Kinds within a resource.** Four kinds are details of another resource and are declared in its block only: `interface` of a network device, `component` of an app, `channel` of a broker (a SaaS, an app or a server), `participant` of a process. They are resources like the others, by a convention of the language: **anything with a name and a block is a resource**. It costs one line in the schema (a kind whose parent is imposed), and it gives them, without any code of their own, a resolved name (`sw01.ge-0-0-1`, `backup.esx`), typed references (`peer`, `members`) checked like any reference, the placement check, hover, go to definition, find references, renaming and formatting. The reference lists them [apart](reference.md#kinds-within-a-resource), completion proposes them in their resource only, and `select participant` in a view is warned about (a participant stands for its members: select them, or the process); `select interface` and `select component` are legitimate (a cabling view, the components of the apps).

`owner` (required, after inheritance) is added to the known keys of `system`, `app`, `job`, `database`, `saas`, `server` and `cluster`. A machine (`server`, network devices) is physical, or `virtual`, running `on` a server or a cluster (§ Virtual machines). A `load_balancer` or a `proxy` is an appliance in a site, a VPC or a network, or software at the root or in a system.

### Standard keys

Some keys mean the same everywhere and are used by renderings and views:

| Key | On | Type | Use |
|---|---|---|---|
| `label` | any resource | `string` | displayed name (default: the name); also a vendor interface name or a cloud object identifier |
| `description` | any resource, any link | `string` | what the resource is for, what the flow does; shown in SVG tooltips, text output and export |
| `confidence` | any resource, any link | `confirmed \| assumed \| doubtful`, default `confirmed` | degree of certainty |
| `question` | any resource, any link | `string` or list | open question to settle |
| `scope` | any resource | `internal \| external`, default `internal` | inside or outside the mapped perimeter |
| `system` | servers, clusters, devices | `ref(system)` | functional membership of an infrastructure resource |
| `carries` | link, `channel` | `ref(data)*` | business objects carried or contained |
| `stores` | `database`, `saas` | `ref(data)*` | business objects stored |
| `exchange` | link | `push \| pull \| both`, no default | data direction |
| `channel` | link | `ref(channel)*` | exchange channels used |
| `through` | link | `ref(transit)*` | devices and tunnels traversed, in order |
| `rack` | physical `server`, network devices | `ref(rack)` | physical location; a virtual machine has none, its host has one |
| `url` | `app`, `saas`, `component`, `vip` | `url*` | address of the service, opened by a click in SVG |
| `console` | `app`, `database`, `saas`, `server`, `cluster`, network devices | `url*` | access for its administration (web console, SSH, RDP), opened by a click without `url` |
| `owner` | any resource | `ref(team)`, inherited | accountable team (decides, arbitrates, budget) |
| `operator` | any resource | `ref(team)`, inherited | team or provider operating it |

### Free attributes

The schema is **open and watchful**:

| Key | Handling |
|---|---|
| known to the schema (`on`, `version`…) | typed and validated |
| unknown (`language`, `team`…) | accepted untyped, raw text kept, available for export and `check` |
| unknown but close to a known key of the same kind or to a standard key (`verison`) | accepted, with a **warning** and a suggestion (proximity rules in § 7) |

- The resource kind, on the other hand, must be known to the schema: an unknown `kind` remains an error (hence the `device` kind).
- With `--strict`, any unknown key is an **error**, for registries that must be locked down.
- The schema of the project types a free key, or declares a kind of its own (next section).

### Schema of the project

A project adds typed keys to the built-in kinds, and declares kinds of its own, with `kind` blocks in its `.ca` files (by convention a `schema.ca` at the root):

```
kind server {
  os: string
  ram: number
  hypervisor: vmware | hyperv
  vmware_cluster: ref(cluster)
  stretched_to: ref(site)*
  inventory_id: pattern("INV-[0-9]+") (required)
}

kind app { catalog_id: pattern("APP-\d{3}") }

kind printer {
  like: device
  parents: site | subnet
  serial: string
}
```

- **Typed keys.** A key gets a type of § Value types: `string`, `number`, `semver`, `version`, `ip`, `cidr`, `host`, `url`, `bandwidth`, `spdx`, `ref(K)` (kinds or groups, `ref(server | cluster)`), a closed list of values (`vmware | hyperv`), or `pattern("…")`, a string matching a regular expression as a whole (written as is between the quotes: `\d` is a digit). `+`, `*` and `{n}` give its cardinality (§ Reference cardinality); `(required)` makes a resource without it incomplete (§ Required attributes). Its values are then checked like those of a built-in key, and close keys are no longer warned about.
- **Keys of the links.** `link { … }` declares keys of every link, `layer flow { … }` keys of the links of a layer, with the same types: `link { volume: number }`, `layer flow { package: pattern("PKG-[0-9]+") (required) }`. A link without a required key of one of its layers is incomplete.
- **A list of one value** is written as the value alone: `hypervisor: vmware`. A name close to a type (`strng`) is taken for a typo of the type.
- **Kinds of the project.** A new kind is `like` a built-in one: it takes its parents (unless `parents` gives others), its keys, its icon and its shape, and it is in its groups and selections: `select device` and `ref(device)` take the printers. `kind = device` in a filter compares the name of the kind, and does not.
- **Groups of the project.** `group storage: database | saas` names a union of kinds, written at the root of a `.ca` file, usable wherever a built-in group is: `select storage`, `kind storage` in a style rule, `ref(storage)`, `group-by`, the kinds of a check. Its members are kinds (built in or of the project), built-in groups (`network_device`) or other groups of the project; the kinds of the project like a member are in it. A group named as a kind or a built-in group, declared twice, with an unknown member or whose members make a cycle is an error.
- **Several blocks.** The blocks of a kind merge, as those of the links or of a layer: the schema may be split by theme (`kind server` in the network schema and in the application one). A key declared in two of them is an error; `like` and `parent` are written once, in one of the blocks.
- **Checks.** A key declared twice, an unknown type or kind, a key the kind or the links already have (built in, or standard), `like` or `parents` on a built-in kind, a pattern that does not compile: errors, at the declaration. A `kind`, `link`, `layer` block or a `group` in a `.cav` file is an error.
- **What the language keeps.** A key is built into the language when the compiler uses it: placement, views, checks, renderings, exports (`on`, `ip`, `hosting`, `schedule`, `carries`, `through`, `url`…). A key that only describes (a platform, a technology, a protocol) belongs to the schema of the project: its values are the organization's, and the language does not have to follow them.
- **Conditional constraints** (`provider` required when `hosting: cloud`) are checks (§ Checks): `check cloud-provider { select site where hosting = cloud and not exists(provider) expect none }`.

## 5. Semantic model

### Three relations

| Relation | Syntax | Nature |
|---|---|---|
| **Containment** | nesting `{ }` | tree, at most one parent; named `parent` in views |
| **Reference** | attribute of type `ref(...)` | named after the attribute (`on`, `subnet`, `rack`, `stores`…), constrained by the schema |
| **Link** | `->` | carries an optional `port_spec`, layers and attributes (including `carries`, `through`) |

- A reference never replaces containment: `app billing { on: web01 }` does not make `web01` the parent of `billing`.
- References are directed, except for **symmetric relations**: `peer` and the end references of connectors.

### Names

- The **qualified name** of a resource is its full path in the containment tree: `aws-paris.prod.dmz.web01`. It is unique in the project.
- **Any suffix of the path** is accepted if unambiguous: `web01`, `dmz.web01`, `prod.dmz.web01`. The short name is the one-segment suffix.
- **Lexical scope by system.** A name written inside a system (at any depth: in an app, a job, a link) is first looked up among the descendants of the nearest enclosing system, then of the parent system, and so on, then in the whole project. The first level containing at least one candidate decides: one candidate resolves it; several are an ambiguity. A name written outside any system (root, infrastructure) is resolved in the whole project.
- Scope follows **the declared functional tree**, never network proximity: the same text always gives the same result. Outside systems, the location of a reference does not change what it designates.
- **Filtering by expected kind.** When the context expects a kind (`ref(K)` reference, `flow_endpoint` link end), only candidates of that kind are kept before checking for ambiguity:

| Candidates after filtering | Result |
|---|---|
| 1 | resolved |
| 0, no resource with that name | error: dangling reference |
| 0, but the name exists with another kind | error: wrong kind |
| several | error: ambiguity, with the minimal suffix that removes it |

Filtering by kind applies at each scope level. Hence adding a resource of another kind, or a resource with the same name in another system, never breaks an existing local reference.

- **Host names.** A name designating no resource at all is looked up among the host names of the virtual IPs (`-> labels.example.com`), in lowercase, then filtered by expected kind like any other name. A resource name always wins over a host name.

### Consistency checks

| Rule | Diagnostic |
|---|---|
| The rack of a resource is in the same site as the resource | error |
| The `ip` of an interface, a server or a virtual IP is within the CIDR of its subnet | error |
| An IP appears only once per subnet | error |
| The CIDRs of the subnets of a site do not overlap (sites may reuse the same private ranges) | error |
| A device of a written `through` is in the site of the source or of the target (a tunnel or a connector: one of its ends), as far as the sites are known | warning |
| The address of a virtual IP outside any subnet appears only once among virtual IPs | error |
| A host name is served by one virtual IP only | error |
| `peer` declarations do not contradict each other | error |
| The `channel` of a link belongs to its source or its target | error |

These checks only apply to what is asserted: missing information never triggers an error (§ 3, Partial knowledge).

### Link identity

- Key: **(source, target, `port_spec`, channels)**, computed after normalization:
  - names → full qualified form;
  - block form → full form;
  - `port_spec` → lowercase protocol, integer port (`HTTPS/0443` ≡ `https/443`).
- Protocols are not interpreted: `https/443` and `tcp/443` are two distinct keys.
- Layers and attributes are **not** part of the identity, except the channels: a pull on a topic and a push on another topic of the same broker, with the same protocol, are two links (`-> confluent : kafka { exchange: pull, channel: raw-prices }`, `-> confluent : kafka { exchange: push, channel: store-prices }`).
- Canonical identifier: `aws-paris.prod.dmz.billing->…:https/443`. It is used for provenance, diagnostics and export.

## 6. Plans (`.cav`, `.cac`)

A [cheat sheet](views.md) sums up how a view is evaluated, with recipes and pitfalls.

```
view <name> {
  title:       "<text>"                           // optional: the title for the readers of the view, its name otherwise
  description: "<text>"                           // optional: what the view shows, a sentence or two
  select   <kind>, … [ where <predicate> ] [ with ends ] [ without isolated ] [ group-by <kind> [ > <kind> … ] ]
  zone     <name> [ "<label>" ] [<member>, …] | where <predicate> [ { <properties> } ]   // zero or more
  edges    <selector> [ where <predicate> ], … [ , through: show ]
           // <selector>: [<layer>, …] | <relation> | lift(<selector>, …) | follow(<data>, from: <resource>) | process(<process>) | path(<resource>, <resource>)
  style    { … }                                   // optional
  render   <format>, … [ { <options> } ]
}
```

`failing: [a, b]` gives a view or a check a scenario of failures (§ Impact analysis). `title` and `description` are for the readers of a view: the site, the previews and the playground show them; the name stays what commands and links use. A view without a `title` is reported as an information (`view flows has no title: its readers see its name`): not wrong, but its readers see a technical name. At the root level of a `.cav`, there are `view` and `style` blocks (§ Rendering and styles); at the root level of a `.cac`, `check` blocks (§ Checks).

### `select`

`select` keeps the resources of the listed kinds. A group (`network_device`) can replace a list of kinds. `where` filters the kept resources (§ Filters).

By default, an edge is only rendered when both its ends are selected. Two modifiers let the nodes depend on the edges:

| Modifier | Effect |
|---|---|
| `with ends` | the edges of simple selectors (layers, relations, connectors) are no longer limited to the selected nodes: their ends are **added** to the nodes |
| `without isolated` | once edges are computed, nodes **without any edge** are removed |

```
view to-verify {
  select app, job, database, saas, server
    where confidence != confirmed or exists(question)
    with ends
  edges [flow, trigger] where confidence != confirmed or exists(question)
  render text
}

view gdpr {
  select app, database, saas without isolated
  edges  [flow] where carries.classification = personal
  render svg
}
```

- `with ends` does not affect `lift`, whose ends are selected nodes by definition.
- Added ends are ordinary nodes: grouped by `group-by` and styled by style rules.
- An end whose kind is used in `group-by` cannot become a node (a kind cannot be both a node and a frame): the edge is dropped with a warning.
- The two modifiers combine: `with ends without isolated` builds a view from its edges only.

### `group-by`

- `group-by K`: the group of a resource is its **nearest ancestor of kind K**; failing that, the target of a **single-valued reference** of kind K it carries (`rack`), inherited value included for `owner` and `operator`; failing that, the group of **what it runs on** (`on`), when all its hosts share it: with `group-by cluster`, an app on a server of cluster `vc` goes into `vc`'s frame. Hosts in different groups leave the resource outside any frame, without an error. One can also group by the name of a reference key: `group-by owner`.
- **Nesting** `K1 > K2 > …`, from broadest to finest. Each resource looks for its finest-level group; each **group** then looks for its own parent group, by the same rule applied to the group. Example: `site > rack` (the server finds its rack by reference, the rack finds its site by containment).
- A resource goes into the **finest level it can reach**: with `site > subnet`, a firewall placed in the site without a subnet appears in the site frame. With no reachable level (a `saas`), it is **outside any group**.
- **What a resource hosts.** Failing all that, a resource goes into the group of what it hosts (`hosts`, the inverse of `on`), when all of it shares one by containment or reference: with `group-by system`, a server without `system:` hosting the apps of one system goes into that system's frame. A server hosting several systems stays outside; a zone gathers such servers: `zone shared "Shared servers" where kind = server and shared`.
- **Frames of values.** A level that is a free key makes a frame per value of that key, within the frame of the outer level: `group-by middleware`, with `middleware: "JBoss 4.2"` on the apps, draws a frame `JBoss 4.2` and a frame `WildFly`. `group-by component` makes a frame per component of the apps, titled by its label or by its name and version (`jboss 4.2.0`), read from their `component` children. A resource without the value, or with several, goes into an outer level.
- A reference with **several values** gives no group at its level, without an error: with `group-by cluster > on`, an app on two servers of the cluster goes into the cluster frame, beside them; with `group-by rack`, a resource with `rack: [r1, r2]` stays outside any rack.
- Empty groups are not drawn.
- **A node that is also a frame.** With `group-by cluster > on`, a selected server hosting selected apps is both a node of the view and the frame of its apps: it is **drawn once, as the frame**, with the text, style (`nodes` rules included) and icon of the node, square corners and a black outline by default; edges to it stop at the frame border. A server hosting nothing in the view stays a node. ASCII art draws both the frame and the node.

  ```
  view storage {
    select server, app where exists(serial) or name in [datacore01, datacore02]
      group-by cluster > on      // ESX1 and ESX2, hosting the DataCore VMs, are frames
  }
  ```
- Errors: several possible references to K on the same resource; kind both selected and used in `group-by` (v1).

### `zone`

A **zone** is a named frame of the view, at the top level or in another zone, for what the registry does not group: the zones of a network page (Carrier, Hostco SAP, Cluster-A, Internet), the SAP sites of a provider apart from its other sites.

```
view wan {
  select site without isolated group-by operator
  zone hostco-sap "Hostco SAP" [hostco-site-east, hostco-site-south] { fill: "#eef6ff" }
  zone internet where scope = external
  render svg { direction: tb, stack: [[magasins, hostco-sap], telco-net-site, cluster-a-site-north] }
}
```

- **Members**: a list of names of nodes or frames of the view (a suffix of their qualified name, as in `stack`), or a filter, evaluated on the nodes and the frames.
- **Priority over `group-by`**: a member leaves its `group-by` frame for the zone; a frame comes with its content; a member inside another member stays where it is. Here, Site-East and Site-South leave the Hostco frame, which keeps the Cluster-A sites. A `group-by` frame left empty is not drawn.
- **Name and label**: the name designates the zone in `stack` and `cross`; the label, a string, is its title (the name by default).
- **Frame**: dashed and rounded by default; `color`, `fill` and `line` in the block after the members.
- **Nested zones**: `in: <zone>` draws the frame of a zone in the frame of another one (`zone sap "SAP" [hostco-site-east, hostco-site-south] { in: hostco }`); a zone may have no member of its own (`zone dc "Data centers" []`), only zones. A child zone keeps its name in `stack` and `cross`. A member is in one zone only: the zone that holds it, inside its parents.
- Errors: an `in` that is not a zone of the view, parents making a cycle, a zone declared twice in a view, a member in two zones, a name designating no resource; a name designating a resource that is not in the view is a warning.

### `edges`

Combinable selectors, separated by commas:

| Selector | Edges |
|---|---|
| `[flow]` | written links carrying one of the layers |
| `on`, `subnet`, `peer`, `stores`… | references of that relation |
| `vpn_tunnel`, `connector` | connectors of that kind or group, drawn as undirected edges |
| `parent` | containment |
| `lift(...)` | lifted edges (below) |
| `follow(...)` | links carrying a data object, walked from a starting point (below) |
| `path(a, b)` | the network path between two resources, deduced from the cabling, even without a flow between them (below) |

- Between brackets: layers. A bare identifier: a connector kind or group if one has that name, otherwise a relation.
- Each selector can be followed by `where` (§ Filters).
- Only edges whose two ends are in the selection are rendered.
- Merging applies within a selector; two selectors produce distinct edges.
- **`path(veeam.proxy, esx-02)`** draws the network path between two resources (§ Network paths): its cables between the devices, their interfaces as labels (`edge_labels: [label]`: `Eth1/47, Eth1/45`), the variants side by side, the cables of a port-channel as one; what runs on a machine linked to it (`on`), a virtual machine to its host; to a SaaS or the internet, the devices of the `egress` and the end beyond them (`path(veeam.serveur, impossible-cloud)`), from them those of the `ingress`. With `without isolated`, the view keeps only them: `select app, server, switch, firewall without isolated`. Without a path, a warning says why and where it stops; a subnet of an end routed by several devices without a `gateway` is reported.
- **`through: show`** splits every flow that goes through a device **present in the selection** into segments (`web01 → fw01 → db01`). Each segment keeps the flow's port and `carries`, then merging applies segment by segment. A device of the path that is not selected splits nothing; a tunnel appears in the segment label. By default, `through` is only visible in tooltips and text output.

### Paths

A path is a sequence of steps joined by a `.` without whitespace:

| Step | Leads to |
|---|---|
| reference name (`on`, `rack`…) | the targets of that reference |
| `parent` | the direct parent |
| `ancestor(K)` | the resource itself if it is of kind K, otherwise its nearest ancestor of kind K (union or group allowed), through containment only |

- Each step produces a **set** of targets: a multi-valued reference multiplies branches, and final targets are **deduplicated**.
- A step followed by **`?`** is optional: without a target, it is skipped and the resource continues along the path as is. Without `?`, a step without a target makes the path fail.

```
via: on                       # app → server (or cluster, fleet, subnet…)
via: on.parent                # app → server → subnet
via: on?.ancestor(subnet)     # app → server → subnet; managed database (on: subnet) → that subnet; server → its subnet
via: on?.ancestor(site)       # app, job, server or managed service → its site
via: ancestor(system)         # app or job → its system
via: rack.ancestor(site)      # server → rack → site
```

`ancestor(K)` includes the resource itself: this lets a single path handle different depths (an app on a server, a managed database sitting on a subnet).

### `where` filters

`where` attaches to a `select` (filters nodes) or to an edge selector, including inside a `lift`, where it filters **written edges, before lifting**, and after a `follow`, where it filters **the links before the walk**:

```
select server where os = debian12
edges  [flow] where carries has customer
edges  lift([flow] where through has fw01, via: on)
edges  follow(customer, from: crmdb) where confidence = confirmed
```

| Predicate | Meaning | Example |
|---|---|---|
| `k = v`, `k != v` | equality | `hosting = cloud` |
| `source.p = target.q`, `!=`, `<`… | two paths of the ends of an edge | `source.ancestor(system) != target.ancestor(system)` |
| `count(p) >= n`, `=`, `<`… | the number of distinct items a path gives | `count(on.ancestor(site)) >= 2` |
| `k < v`, `<=`, `>`, `>=` | comparison (`number`, `semver` only) | `version >= 2.0.0` |
| `k has v` | the list or reference contains v | `carries has customer` |
| `k in [a, b]` | the value is one of these | `protocol in [ipsec, wireguard]` |
| `exists(k)` | the attribute is present | `exists(description)` |
| `k` alone | boolean pseudo-attribute | `incomplete` |
| `and`, `or`, `not`, `( )` | combinations | `not exists(through) and port = 5432` |

- **Typed by the schema.** Values are compared according to the key's type; a reference (`through has fw01`) is resolved with filtering by expected kind (§ 5). Free keys are compared on their raw text, in order when the value is a number or a version (`criticality >= 2`).
- **Order comparisons** (`<`, `<=`, `>`, `>=`) need a number or a version on the right, and a key that can hold one: a key typed otherwise by the schema (`hosting`, a word of a closed list; `on`, a reference), `ancestor(…)` or a pseudo-attribute other than `port` and `step` is an error (`hosting > 3`, `version > latest`).
- **Keys of the filtered kinds.** In a `select` or a `kind` style rule, a filter starting with a key that none of the filtered kinds has (in its schema, or written on one of its resources) is an error: `select database where cidr = …` (`cidr` is a key of subnets).
- **Missing attribute: every comparison is false**, `!=` included. `os != debian12` does not keep a server without `os`; to include it: `not (os = debian12)`.
- **Paths on the left-hand side**, evaluated existentially (true if at least one target satisfies the predicate): `carries.classification = personal`, `ancestor(site).hosting = cloud`, `on.ancestor(site) = paris-dc1`.
- **Two paths on an edge.** On edges, the right-hand side may be a path too, starting with `source` or `target`: the two ends are compared to each other.

```
edges [flow] where source.ancestor(system) != target.ancestor(system)       // flows between systems
edges [flow] where source.owner = target.owner                              // flows inside a team
edges [flow] where source.on?.ancestor(site) != target.on?.ancestor(site)   // flows between sites
edges [flow] where source.version > target.version
```

  `=` holds when the two sides share a value (resources compared by identity, values by text); `!=` when both have values and share none (an app on `[web01, web02]` is not on another server than an app on `web01`); order comparisons when a pair of values satisfies them. **A side without value makes every comparison false**, `!=` included: an app outside any system is neither in the same system as another nor in a different one. A right-hand side starting with `source` or `target` is always a path, never the name of a resource (write `dc.source` for a resource named `source`); two paths are only compared on edges, an error on nodes.
- **Pseudo-attributes**:

| Pseudo-attribute | On | Meaning |
|---|---|---|
| `kind`, `name` | nodes and edges | kind, qualified name |
| `incomplete` | nodes and edges | true if the element is incomplete (§ 7): `where incomplete`; an edge is incomplete when one of its ends, or a connector it draws, is |
| `source`, `target` | edges | ends, usable at the head of a path: `target.kind = saas` |
| `proto`, `port` | edges | taken from the `port_spec`; `port = 8080` is true if 8080 falls within a range |
| `step` | edges | step of a `follow` edge (§ `follow`), absent otherwise: `edges where step = 1` |
| `hosts` | nodes | the resources whose `on` designates the node, usable at the head of a path: `hosts has catalog.api`, `hosts.kind = database` |
| `shared` | nodes | true if what the node hosts belongs to at least two systems (nearest enclosing system, otherwise `system:`): `select server where shared` |
| `virtual` | nodes | true if the node is a virtual machine, declared `virtual` (§ Virtual machines): `select server where virtual` |
| `failures` | nodes | the resources whose failure alone makes the node unavailable (§ Impact analysis), usable at the head of a path: `failures has fw01`, `failures.kind = server` |
| `failures(hosting)` | nodes | the same through hosting only, without the links: `failures(hosting).kind = server` |
| `failures(pairs)` | nodes | the resources of the pairs whose failure together, and not of either alone, makes the node unavailable: what its redundancy rests on, `failures(pairs) has web02` |
| `down` | nodes | true if the node is unavailable in the scenario of the view or the check (`failing: [fw01, fw02]`), failing itself or through what fails: `select app where down` |

- **Aggregates**: `count(path)` is the number of distinct items the path gives, compared to a number: `select app where count(on) < 2` (an app on a single server), `count(on.ancestor(site)) >= 2` (in two sites at least), `count(hosts) > 10`. `count` stays usable as a key (`where count = 2`): only `count(…)` is the function. No other aggregate (`sum`), sub-query or regular expression.

### Choosing an edge selector

Links are written once, between the resources that exchange; the selectors derive the edges a view needs from them. Which one answers which question:

| Question | Selector | Edges | Order |
|---|---|---|---|
| Who calls whom? | `[flow]` (a layer), `on`, `peer`, `connector`… | the links or references written, between selected ends | none |
| The same, at another level: between servers, sites, systems | `lift(<selector>, via: <path>)` | each end replaced along a path (`on`, `ancestor(system)`), then merged | none |
| Where does a data object go, from a starting point? | `follow(<data>, from: <resource>)` | the links carrying the object, walked in the direction of the data | `step`, the distance from the start, derived |
| How does a business process run? | `process(<process>)` | the steps a `process` lists, written in order, replies and processings included | `step`, the order written |

- `lift` changes **where** the edges are drawn, not which links: it combines with the others (`lift(follow(order, from: shop), via: on)`).
- `follow` and `process` both number edges, but `follow` **derives** the path from `carries` and `exchange`, while a `process` **states** it: use `follow` to discover where data goes, a process to document a sequence that the data alone does not give (a call to a directory, a reply, a loop).
- A link reached by `follow` or listed by a process is an ordinary edge: filters, styles and merging apply.

### `lift`

```
lift(<selector>, via: <path>, loops: drop | keep, merge: endpoints | port | none)
```

The first argument is any edge selector (layers, relation, connector, `parent`, `follow`), with its own `where`, which filters the links before lifting. A `where` written after `lift(…)` filters the lifted edges, whose ends are already folded: `lift([flow] where carries has customer, via: on) where target.ancestor(subnet) = dmz` keeps the flows carrying customer details, then those lifted onto a server of the DMZ. For each selected edge `a → b`:
1. each end is replaced by its targets along `via`; an end already in the selection is kept as is; a target that is not selected is lifted in turn, along the path for its kind: a database on a shared database, lifted `via: on`, reaches the server of the latter;
2. an edge is emitted for each combination (cartesian product, due to multi-valued references);
3. loops and merging are applied.

| Parameter | Values | Default |
|---|---|---|
| `via` | path | required |
| `via_<kind>` | path for the ends of that kind or group, before `via`: `lift([flow], via: ancestor(system)?, via_job: on)` takes a job to its server and anything else to its system | none |
| `loops` | `drop`: removes `x → x` edges; `keep`: keeps them | `drop` |
| `merge` | `endpoints`: one edge per (source, target); `port`: one per (source, target, `port_spec`); `none`: multigraph | `endpoints` |

Common rules:
- Edges remain **directed** (`a → b` and `b → a` do not merge), except for symmetric relations (`peer`, connectors), merged regardless of direction and drawn without arrows.
- Label after merging: for flows, the sorted union of `port_spec`s; for references and connectors, the relation name followed by `×n` when n > 1, unless `edge_labels` gives other labels (§ 6, `render`).
- A merged edge carries the union of the original layers, `carries` and `through` (unordered). `description`s and the order of `through` remain available through provenance.
- Each lifted edge keeps its **provenance** (identifiers of the original edges), used for SVG tooltips, text output and debugging.
- An end that is neither selected nor liftable: the edge is **dropped with a warning**.
- `confidence` of a merged edge: the **strongest** of the original edges (one confirmed flow is enough). Original `question`s are shown in the tooltip.
- `exchange` of a merged edge: `push` and `pull` give `both`; missing values are ignored when known values exist.

### `follow`

```
follow(<data>, from: <resource>)
```

`follow` follows a business object along the links that carry it, in the direction of the data. It answers "where do the item prices go, starting from the pricing UI?" without declaring anything more than the links already say: end-to-end flows are **derived**, never written.

```
view price-chain {
  select app, job, saas, server without isolated
  edges  follow(item-prices, from: pricing.price-push)
  render svg { orientation: data, edge_labels: [step, port] }
}
```

1. **Links carrying the object**: every link, whatever its layers, whose `carries` contains it, or that uses a `channel` whose `carries` contains it. The `where` of the selector applies to these links, before the walk.
2. **Direction of the data**: `exchange: push` along the link, `pull` against it, `both` both ways; without `exchange`, the direction of the connection.
3. **Walk from `from`**, breadth first: a link is reached when the data leaves one of its ends already reached; it gets as **step** the distance from `from` (1 for the links leaving `from`). The other end is reached in turn, unless it already was: the walk stops on loops.

| Parameter | Values | Default |
|---|---|---|
| data object | reference to a `data` | required |
| `from` | reference to a `flow_endpoint` | none: every link carrying the object, without step |

- Parallel branches share a step: a topic consumed by three apps gives three edges at the same step.
- A link reached from both ends (`exchange: both`) keeps its smallest step.
- Upstream links (bringing the object to `from` or to a reached end without leaving a reached end) are not reached.
- To show where a chain starts, the triggering link carries the object too: `-> ui : http [trigger] { carries: [item-prices] }`.
- Edges are ordinary edges: the selection, `with ends`, `without isolated`, merging (by port) and styles apply. A rule can target a step: `edges where step = 1 { color: red }`.
- `follow` inside `lift` is accepted: the step of each lifted edge is kept, the smallest one after merging.
- A `from` that reaches no link carrying the object: **warning** when the view is evaluated.

- **Relays.** `relay: [database]` lets a resource of these kinds pass the data on along its links that do not declare it: the prices written to a database go on through the API reading it, and from it along the links that carry them. The direction is that of `exchange`, from the relay towards the other end without one. `relay` takes kinds, groups or resources: `relay: [stock.db]` relays through that database only, not through every database. `relay_to: [app]` (kinds or resources) keeps only the relayed links leading there, against the noise of a shared database read by many jobs. Without `from`, `relay` changes nothing.
- **Side calls.** `side: [stores]` also draws the links leaving the resources the data goes through and carrying these data (directly or through a channel), without a step: the calls serving the processing, such as asking a directory where to send the data. `side: all` draws all of them, whatever they carry. They may serve other processings too: a named process (§ Processes) says which belong to one.
- **Channels.** The data reaching, through a channel, the resource holding it (a topic of a broker) leaves it through that channel only: a broker is crossed once per channel, in the order of the data (raw prices to a topic, a stream reading it and writing another topic, the sinks of that one: steps 1 to 4, not all the consumers at step 2). Two links between the same ends at two steps stay two edges.

### Rendering and styles

#### `render`

```
render svg, text {
  direction: lr
  edge_labels: [port, carries]
  legend: true
}
```

Each format produces its own file: `out/<view>.svg`, `out/<view>.dot`, `out/<view>.txt`, `out/<view>.mmd`, `out/<view>.ascii.txt`, for tables `out/<view>.csv`, `.tsv`, `.md`, `.html`, `.xlsx`, and `out/<view>.drawio`. SVG is produced from DOT by Graphviz (`dot -Tsvg` natively, a WebAssembly build of Graphviz in the browser); the other formats are produced by the compiler alone.

| Format | Output |
|---|---|
| `svg` | diagram, through Graphviz |
| `dot` | Graphviz source |
| `text` | nodes by frame, then edges with their provenance |
| `mermaid` | Mermaid flowchart |
| `sequence` | Mermaid sequence diagram of the processes the view selects (`edges process(…)`, § 3, Processes): a participant per resource, those of `participants` first, then in their order of appearance; a numbered arrow per step (`->>` a call, `-)` an asynchronous one, `-->>` a reply, an arrow on itself for a processing), its `label` followed by the protocol of its link, an acronym in capitals, a name as usually written, with its port unless it is the usual one (`reads the invoices (JDBC 5432)`, `calls the CRM (HTTPS)`, `writes (OpenEdge)`), unless `edge_labels` leaves out `port`; loops and notes; the label of the process as title, unless `title: false`; `out/<view>.sequence.mmd` |
| `traversal` | SVG of the paths the view selects, drawn by the compiler: a row per path between two resources (`edges path(a, b)`, headed `a → b`, or why there is none), from the first to the second through the devices of the cabling; a row per step between two participants of the processes it selects (`edges process(…)`), before the paths, numbered as in the sequence diagram (a processing has a number, but no row), its text as there; from its source to its target, a box at each device its flow goes through, those of the `through` of its link (written, or deduced from the cabling, § 3, Network paths), the other way for a reply; redundant devices in one box (`col-sw10-01`, `or col-sw10-02`), an end with the machines it runs on (`on proxy01`, `on esx01`); a dashed arrow for a reply, a dotted one over the internet; `edge_labels: [interface]` writes the interfaces of the cables between two boxes, redundant ones together (`p1, p2 → eth1-11`); `out/<view>.traversal.svg` |
| `ascii` | the diagram drawn with Unicode box-drawing characters, for terminals, commit messages and Markdown code blocks |
| `csv`, `tsv`, `markdown`, `html` | a table: a row per node, or per edge when the view has an `edges` clause (`rows`), the `columns` chosen; CSV and TSV start with a byte order mark, for Excel to read them as UTF-8, and TSV is what Excel pastes; `html` is a whole page, to open in a browser (`out/<view>.html`), its header kept in sight while scrolling |
| `xlsx` | the same table as an Excel workbook, `out/<view>.xlsx`: a sheet named after the view, the header in bold and frozen, an automatic filter, columns as wide as their content, URLs (`url`, `console`) as links; the cells are text, except ports and steps, numbers: Excel would otherwise read a version (`16.2`) as a number or a range (`1-2`) as a date |
| `drawio` | an editable diagram for draw.io (diagrams.net, its desktop and VS Code apps), `out/<view>.drawio`: laid out by Graphviz like the SVG (`dot -Tjson`; the compiler's own layout, that of the ASCII art, without Graphviz); frames are containers holding their nodes, edges are attached to their ends and routed by draw.io, so that moving a node or a frame keeps the diagram connected; shapes, colors, lines and icons are those of the SVG, and each node and edge carries its data (qualified name, kind, attributes) as draw.io properties, shown by *Edit Data* |

| Option | Values | Default |
|---|---|---|
| `direction` | `lr` (left → right), `rl` (right → left), `tb` (top → bottom), `bt` (bottom → top): the direction of the edges; frame titles stay at the top | `lr` |
| `orientation` | `connection`: arrow in the initiator's direction; `data`: according to `exchange` (`push` along the link, `pull` reversed, `both` with two heads, unspecified without a head). The layout follows the arrows: with `data`, a reader `pull`ing from a source is placed after it | `connection` |
| `node_labels` | list of attributes shown in the node, under the label, and in group frames, under the frame name: `[label, description]` | `[label]` |
| `edge_labels` | **Unless written, a diagram of more than 20 edges draws no label on its edges**: they stay in the tooltips, the text and the tables, and the diagram reads by its shapes and colors; write the option to draw them whatever the number of edges. List among `port`, `carries`, `through`, `description`, `step` (step number of a `follow`, written `2.` before the rest of the label), any key written on the links or the connectors of the edge (`circuit_id` on a `wan_link`) or on the two ends of the references it draws, the end carrying the reference first (the interfaces of a `peer` cable, even after `lift`), `relation` (the relation name of a reference or connector edge, `peer ×2`), `interface` (the interfaces of the cables between two devices, `render traversal`), or `none`. `label` gives the label of those ends, the name of the carrying end when it has none: `edge_labels: [label, circuit_id]` writes `Ten2/0/1, 22061212` on a fiber. A relation edge (`peer`, a connector) shows its relation name when no other label applies, unless the list holds keys of its own (`label`, `circuit_id`): a cable without them stays bare | `[port]` |
| `legend` | `true`, `false`: legend of the style rules carrying a `legend` property | `false` |
| `icons` | `true`, `false`: the icon of its kind left of the text of each node and frame (SVG) | `true` |
| `satellites` | `true`, `false`: a node exchanging with two frames or more (a directory, a file hub, a broker, a scheduler) is drawn once **in each of them**, in gray, and the edges of the frame go to that copy, rather than once with every edge of the view pulled to it across the frames; every copy, and the original, carries `×N`, the number of places the node is drawn in; its own frame keeps it in black, a node at the root of the view with no other edge is not drawn a second time. The tooltip of a copy says where the node is. Each frame then reads by itself (SVG; the DOT, and the draw.io export laid out from it) | `true` |
| `twins` | `true`, `false`: in a frame, the nodes of one kind with exactly the same exchanges (same other ends, directions, ports, labels, styles) are drawn as **one box listing them**, with one edge per shared exchange, rather than each with its own edges: a database read by twenty apps, a hub feeding thirty servers. The box says how many of what kind, then the names in columns of 12; each name keeps its link, the tooltip of the box lists the resources, the tooltip of an edge says it stands for them all. A node drawn as a frame, as a satellite, named by a layout hint, with a loop or without edges is never in a box. The text and the tables keep every node and every edge (SVG) | `true` |
| `merge_label` | label of an edge merging several links (after `lift`): `list`, their labels; `count`, a summary from 2 links, their protocols and their number (`sftp ×9`, `https, sftp ×4`, `×12` beyond three protocols), the detail being in the tooltip; a number, the summary from that many links on | `list` |
| `satellites` | ✔ | ✘ | ✘ | ✔ (laid out by Graphviz) |
| `twins` | ✔ | ✘ | ✘ | ✔ (laid out by Graphviz) |
| `columns` | columns of a table: keys, pseudo-attributes or paths, as in filters; a path holding parentheses is quoted: `columns: [name, ip, "ancestor(site)", hosts]`, `columns: [source, target, proto, port, carries]`. A resource shows its short name, several values are joined by `, ` | `[name, kind, label]` for nodes, `[source, target, port, carries]` for edges |
| `rows` | `nodes` or `edges`: the rows of a table | `edges` when the view has an `edges` clause |

#### Layout hints

Graphviz places the nodes and routes the edges by itself, the ranks computed over the whole view (its `newrank` mode: on a dense view with frames, fewer crossings and shorter edges than frame by frame), and most views need nothing more. When a diagram does not read well (a frame far from the one it exchanges with, a device drawn below its switches, labels on top of each other), these options steer it. They are **hints**: the engine keeps the last word, edges going the other way prevail over `stack`, and a hint a format cannot follow is ignored (§ Support per format). Reach for them last, after `group-by`, `zone` and the selection itself.

| Option | Values | Default |
|---|---|---|
| `layout` | engine laying out the SVG: `graphviz`, or `elk` (Eclipse ELK, its layered algorithm ported to Rust: orthogonal edges, labels on them, nested frames); see below | `graphviz` |
| `stack` | list of frames or nodes (names of resources), placed in this order top to bottom (`tb`, `bt`) or left to right (`lr`, `rl`): a hint, edges going the other way prevail. A nested list puts several on the same level: `stack: [[carrier-pa2, carrier-pa7], colo-pa3, [cluster-a-site-north, cluster-a-site-west]]` (the only place where a list may contain a list). The items of a level stay side by side, on one rank, even when edges link them: these edges leave the ranks alone, as `layout: free` edges, their labels on them; Graphviz orders the items of a level to avoid crossings, `cross` orders them. A frame is drawn in one piece: two of its items on levels separated by an item outside it cannot be honored, and a warning says so (`group-by operator` puts Site-East and Site-North in the same frame) | none |
| `cross` | list of frames or nodes (names of resources), placed in this order **across** the direction: top to bottom in `lr` / `rl`, left to right in `tb` / `bt`, among the frames and nodes sharing a column (a row); the others after them. A hint, through the order frames and nodes are written in, the crossing minimization of Graphviz being cut short: frames whose edges are all `layout: free` may still not follow it. `cross: [carrier-pa2, cluster-a-site-north]` puts a site above the other in a left-to-right view | none |
| `first`, `last` | lists of frames or nodes placed first (at the top in `tb` / `bt`, at the left in `lr` / `rl`) or last **in their frame**, before or after its other items: `first: [pa2-router, pa7-router]` puts the routers above the switches of their room; a cable touching them starts from them | none |
| `routing` | shape of the edges: `curved`, `polyline` (broken lines), `straight`, `orthogonal` (horizontal and vertical segments, labels less well placed) | `curved` |
| `edge_labels_layout` | `nodes`: the labels are laid out as small nodes on the edges, which reserves room for them but makes edges detour through their labels when many leave the same node; `free`: centered on the edges after the layout (SVG), without moving anything, at the risk of overlaps | `nodes` |
| `edge_labels_wrap` | labels on several lines: a number of characters, a line break between values once a line would exceed it (a value is never cut), or `values`, one value per line: `edge_labels_wrap: 40` | none: one line |
| `spacing` | space between the nodes and between the ranks, as a factor of the defaults: `1.5` for both, `[1, 2]` for the nodes, then the ranks (the frames of a level of `stack`, the rows of a top-to-bottom view) | `1` |
| `node_label_size`, `edge_label_size`, `frame_label_size` | size of the text of the nodes, of the labels of the edges, of the titles of the frames, in points | `10`, `9`, `11` |

- **`layout: elk`** lays out the SVG of the view with Eclipse ELK rather than Graphviz, view by view: `render svg { layout: elk }`. The SVG is then drawn by the compiler, `out/<view>.svg` as before, without Graphviz. It suits flow views without many edges between frames, where Graphviz crosses or bends them; Graphviz stays the default. The drawing is that of Graphviz: the shapes and icons of the kinds, the styles, the links and tooltips, the legend. Experimental: the compiler includes ELK only when built with the `elk` feature (`cargo install --path crates/cadastre-cli --features elk`; ELK is under the EPL-2.0), otherwise the view is laid out by Graphviz, with a warning. `cross` orders the items of a layer, `first` and `last` put a node in the first or the last layer of its frame, `stack` orders items by invisible edges as for Graphviz, more loosely across frames; `routing` and `edge_labels_layout` do not apply (a warning): ELK routes the edges orthogonally, their labels on them; the edges reaching a same side of a node are spread along it, a few points apart, rather than converging. Edges between nodes of different frames often go around them: views with many such edges stay better with Graphviz.

#### `style`

A `style` block at the **root level of a `.cav`** applies to the whole project (**only one** in the project); a `style` block **inside a view** extends and overrides the project style for that view.

```
style {
  edges where proto in [http, https]       { color: "#1d4ed8", legend: "HTTP / HTTPS" }
  edges where proto in [sftp, ftp, azcopy] { color: "#c2410c", legend: "SFTP / FTP / azcopy" }
  edges where proto = jdbc                 { color: "#4b5563", legend: "JDBC" }
  layer auth                               { color: "#be185d", legend: "Authentication" }
  layer trigger                            { color: "#9ca3af", legend: "Triggering" }
  kind  saas                               { shape: rounded, color: purple }
  kind  connector                          { line: dotted }
  nodes where scope = external             { line: dashed }
}
```

| Target | Applies to | Properties |
|---|---|---|
| `kind <kind or group> [where …]` | nodes and group frames of that kind (edges for a connector kind) | `color`, `fill`, `shape` (`box`, `rounded`, `ellipse`, `cylinder`, `hexagon`, `note`), `line`, `legend` |
| `layer <layer> [where …]` | edges of that layer | `color`, `line` (`solid`, `dashed`, `dotted`), `layout` (`free`), `routing` (`straight`), `legend` |
| `nodes [where …]` | all nodes, not the group frames | as `kind` |
| `edges [where …]` | all edges | as `layer` |

- Colors: CSS names (`red`, `steelblue`) or a quoted hexadecimal code (`"#3366cc"`).
- `color` is the outline of a node, the border of a frame, the line of an edge; `fill` the background of a node or a frame. On a dark `fill`, the text (and the icon) turns white.
- `layout: free` on edges: the edge is drawn but places no node, as if it were absent for the layout; its label is put on the drawn edge after the layout (SVG), its letters outlined by a white halo, and no longer lines up with the labels of the other edges: at the middle of the edge, or at the position along it closest to the middle where it covers no node and no other label; when there is none, where it covers the least. For an edge between two nodes that should stay side by side, such as a mirror between two storage VMs. A link written in both directions is two edges: the rule must match both, `edges where source.name in [datacore01, datacore02] and target.name in [datacore01, datacore02] { layout: free }`.
- `routing: straight` on edges: the edge is drawn as a straight line, from the border of a node to the other, whatever the `routing` of the view: straight links and curved flows in the same view, `kind wan_link { routing: straight }`. Graphviz routes all the edges of a graph alike: the edge is redrawn after the layout, in SVG (command line, playground, VS Code), its label on it; the DOT, Mermaid and draw.io outputs keep the routing of the view. A straight line goes over what lies between its ends: order them with `stack`, `cross`, `first` and `last`.

Layout hints together, for a storage view where the servers of a cluster use the storage VMs of another one:

```
view storage {
  select server, app group-by cluster > on
  edges  [flow] where proto = iscsi or source.name = datacore01
  style {
    // The mirror, written in both directions.
    edges where source.name in [datacore01, datacore02] and target.name in [datacore01, datacore02] { layout: free }
  }
  render svg {
    direction: bt                       // the iSCSI flows go up to the storage
    stack: [cluster-a, cluster-b, veeam]     // and the backup server, without flows, at the bottom
  }
}
```
- `legend: "…"` gives the rule's label in the legend; a rule without `legend` does not appear in it. The legend only lists the rules whose style **shows in the view**: the rule applies to at least one of its nodes, frames or edges, and no later rule overrides its color, fill or line there. Each entry has a sample: a filled square for node and frame rules, a line (solid, dashed or dotted) for edge rules. The legend also explains the lines of the default theme drawn in the view (§ Default theme): `Assumed link`, `Doubtful link`, `Trigger`, and the borders of `External`, `Assumed resource` and `Doubtful resource` nodes, unless a rule with a `legend` is about the same thing (it sets the `line`, its filter reads `confidence` or `scope`, or it styles the `trigger` layer): `edges where confidence = assumed { legend: "Supposé" }` names the entry in the language of the project.
- **Cascade**: default theme, then project style, then view style; within a block, declaration order. For each property, **the last matching rule wins**.
- Two project `style` blocks: **error** (since file order is meaningless, the cascade would be undefined).

#### Default theme

- `database` as a cylinder, `saas` rounded, `actor` as an ellipse, `job` as a hexagon, connectors dotted, everything else as a box.
- Standard layers: `flow` solid, `auth` thin, `trigger` dashed with a hollow arrowhead.
- `confidence`: `assumed` dashed, `doubtful` dotted with a `?` added to the label.
- `scope: external`: dashed border.
- Group frames show the name and the key attribute: `dmz (10.0.1.0/24)`, `aws-paris (cloud)`, then the `node_labels` attributes.
- Nodes show `label` (or the name); the SVG tooltip shows the `description`, attributes, `question`s and provenance.
- **Satellites** (SVG): a node that several frames exchange with is drawn once in each of them, in gray and with `×N` (`satellites`, above): a dense view with frames reads frame by frame, without the edges pulled across it to a few shared nodes.
- **Several layouts tried** (SVG): Graphviz orders the frames from the order they are written in, and computes the ranks over the whole view or frame by frame; each choice wins on some views. A view is laid out fourteen ways (the frames of the root as written, reversed, by connectivity and that reversed; each of the last three applied inside the frames too; each with both rankings) and the drawing that reads best is kept: no frames overlapping, then the fewest crossings, each worth three average edges, plus the length of the edges. With `stack`, `first` or `last`, which fix the ranks, only the seven orders are tried, the ranks frame by frame; with `cross`, which fixes the order, the view is laid out as written. A layout takes milliseconds. The CLI, the site and the playground do it; an editor drawing without Graphviz installed keeps the first.
- **Twins** (SVG): the nodes of a frame with exactly the same exchanges are drawn as one box listing their names, with one edge per exchange (`twins`, above): a fan of thirty identical edges becomes one edge to a list, and the view reads at first sight.
- **Titles kept clear** (SVG): the title of a frame that an edge would cross is moved along the top of its frame to where no edge passes; the labels placed on edges get a white halo the edge passes under. No text is crossed by an edge on the views of the examples.
- **Dark mode** (SVG): the drawing follows the color scheme of where it is shown (`prefers-color-scheme`): on a dark page or in a dark editor, the background, the borders, the arrowheads and the default texts turn light, the colors of the layers and of the style rules stay; a text on a light `fill` of a style rule stays dark. Nothing to configure: a single file for both.
- **Icons** (SVG): each node and each group frame shows the icon of its kind left of its text, in the color of the node: `server`, `cluster`, `system`, `app`, `job`, `database`, `saas`, `firewall`… one per kind. The icons are those of [Lucide](https://lucide.dev) (ISC license) and are embedded in the file: an SVG rendering stays a single file. `icons: false` removes them.

#### ASCII art

```
┌─────┐              ╭───────╮
│ api ├───tcp/5432──▶│ store │
└─────┘              ╰───────╯
```

- Nodes are placed in ranks along the edges (left to right or top to bottom, after `direction`); group frames are nested bands, drawn with `┄` and `┆` and their title in the top border.
- Edges are routed orthogonally around the boxes; two edges never run along each other, they only cross (`┼`). Arrowheads follow `orientation`: `▶` (`▷` for a hollow arrowhead), no head for undirected edges.
- A label is written on a straight stretch of its edge, otherwise beside it, otherwise as a numbered note under the drawing: `(1) web01 → stripe: https/443`.
- Nodes without edges are spread over a grid.
- Lines follow the style: `─` solid, `╌` dashed, `┈` dotted; `rounded`, `ellipse` and `cylinder` shapes get rounded corners.
- Large views can be wide: `direction: tb` and filters (`without isolated`, `where`) keep them readable.

#### Support per format

| Option | `svg` / `dot` | `mermaid` | `text` | `ascii` |
|---|---|---|---|---|
| direction, labels | ✔ | ✔ | labels only | ✔ (labels on the edges or as notes) |
| colors | ✔ | ✔ | ✘ | ✘ |
| fill, frame colors | ✔ | ✔ | ✘ | ✘ |
| lines | ✔ | ✔ | ✘ | ✔ |
| shapes | ✔ | partial (cylinder, rounded, ellipse) | ✘ | partial (rounded corners) |
| tooltips, provenance | ✔ (`<title>`) | ✘ | ✔ (plain text) | ✘ |
| legend | ✔ (swatches) | ✔ (a `Legend` subgraph) | ✔ (in words) | ✔ (line samples, in words) |
| icons | ✔ (SVG) | ✘ | ✘ | ✘ |
| `stack` | ✔ | ✔ (invisible links) | ✘ | ✘ |
| `cross` | ✔ | ✔ (declaration order) | ✘ | ✘ |
| `first`, `last` | ✔ | ✔ (invisible links) | ✘ | ✘ |
| `routing` | ✔ | ✔ (`curve`) | ✘ | always orthogonal |
| `edge_labels_layout` | ✔ | ✘ | ✘ | ✘ |
| `edge_labels_wrap` | ✔ | ✔ (`<br/>`) | ✘ (one line) | ✘ (one line) |
| `spacing` | ✔ | ✔ (`nodeSpacing`, `rankSpacing`) | ✘ | ✘ |
| text sizes | ✔ | ✘ | ✘ | ✘ |
| `routing: straight` on edges | ✔ (SVG) | ✘ | ✘ | ✘ |
| `merge_label` | ✔ | ✔ | ✔ | ✔ |
| `layout: free` | ✔ | ✘ | ✘ | ✔ |

An option with no equivalent in a format is ignored without a warning.

### Impact analysis

`failures` gives, for each node, the resources whose failure **alone** makes it unavailable. A failure spreads:

- to what a resource contains (a site, a network, a subnet, a server with its interfaces) and to what is in a rack (`rack:`);
- to what runs on it (`on`), once **every** host is down: an app `on: [web01, web02]` survives the loss of one;
- to a cluster or a pool, once all its members are down (or when it fails as a whole);
- to a tunnel or a connector, when one of its ends is down;
- along the links: the source of a link depends on its target and on what the link goes through (`through`, written or deduced from `egress`).

Systems, teams, actors and data do not fail by themselves. `failures(hosting)` leaves the links out: what a node needs to **run** (containment, racks, `on`, clusters, pools, connectors), not what it calls. A shared database reached by many apps is then not counted for each of them, and a single point of failure of the hosting stands out: `failures(hosting).kind = server`.

```
view fw01-failure {                                  // what the failure of fw01 takes down
  select app, database, saas where failures has fw01
}

check no-single-server {                             // no app depends on one server alone
  select app where criticality = high and failures.kind = server
  expect none
}

check redundant-hosting {                            // nor runs on one server alone
  select app where criticality = high and failures(hosting).kind = server
  expect none
}

check two-sites {                                    // nor on one site
  select app where criticality = high and failures.kind = site
  expect none
}
```

In the reference example, `billing` runs on `web01` and `web02`, but depends on `crm`, which runs on `web01` alone: `web01` is a single point of failure of `billing`, through `crm`.

**The pairs a redundancy rests on.** `failures(pairs)` gives the resources of the pairs whose failure together, and not of either alone, makes a node unavailable: an app on `[s1, s2]` falls with `s1 + s2`. Only the most precise pairs are kept: `s1 + s2`, not the rack of `s1` with `s2`, the rack taking `s1` down. The site shows them on the page of each resource (*Redundancy*), the API in `failure_pairs`.

```
view rests-on-fw02 {                                 // the apps whose redundancy needs fw02
  select app where failures(pairs) has fw02
}
```

**Several failures together.** `failures` looks at one failure at a time. A scenario takes down several resources at once (both firewalls, a site and its link to the other one, both web servers): `failing: [fw01, fw02]` in a view or a check, and `down` is true for the resources unavailable then, those failing included. The failure spreads as above, links included. A system fails as what it contains: `failing: [invoicing]`, a system, takes down its apps, databases and jobs.

```
view web-servers-failure {                           // what remains when web01 and web02 fail
  failing: [web01, web02]
  select app, database, saas, server
  edges  [flow], on
  render svg { legend: true }
}

check survives-paris {                               // the critical apps survive the loss of a site
  failing: [paris-dc1]
  select app where criticality = high and down
  expect none
}
```

In a view with a scenario, what fails is filled in red, what is unavailable through it in pale red (*Failing*, *Unavailable* in the legend); a style rule `nodes where down { … }` draws them otherwise. In the reference example, both web servers failing take down `billing` and `crm`, not `crmdb`, which runs on `db01`.

### Checks

A check is a rule on the model, the test of a project: `cadastre check` runs them, and a failing check is an error, so a CI fails on it like on a failing test.

```
check <name> {
  select <kind>, … [ where <predicate> ]           // nodes, as in a view
  edges  <selector> [ where <predicate> ], …       // or edges, as in a view
  expect none | some | count <op> N | all <predicate>
  message:  "<why the rule exists>"                 // optional
  severity: error | warning | info                  // optional, error by default
}
```

```
check every-app-has-an-owner {
  select app where not exists(owner)
  expect none
  message: "an app has an owner team, written or inherited from its system"
}

check no-personal-data-to-saas {
  edges  [flow] where carries has customer and target.kind = saas
  expect none
}

check erp-behind-fw01 {
  select app
  edges  [flow] where target = legacy-erp
  expect all through has fw01
}

check redundant-front {
  select server where ancestor(subnet) = dmz
  expect count >= 2
}
```

- **The selection is a view's.** `select` and `edges` are those of a view (§ `select`, § `edges`, `lift`, `follow`), evaluated the same way; `group-by`, `zone`, `style` and `render` have no meaning in a check. Without `select`, every kind is selected: the edges are then only filtered by their `where` (with `lift`, write the `select` the edges are lifted onto).
- **The expectation is on the edges** when the check has an `edges` clause, **on the nodes** otherwise:

| Expectation | Holds when |
|---|---|
| `expect none` | nothing is selected: the selection describes what must not exist |
| `expect some` | at least one node or edge is selected |
| `expect count <op> N` | the number of selected nodes or edges compared to N (`=`, `!=`, `<`, `<=`, `>`, `>=`) |
| `expect all <predicate>` | every selected node or edge satisfies the predicate, written as in `where` |

- **A failure says what is wrong.** It names the offending nodes or edges (the selected ones for `expect none`, those that do not satisfy the predicate for `expect all`) and gives their positions, after the position of the check; `message` is shown as its help:

```
error: check every-app-has-an-owner failed: 1 node selected, none expected: orphan
  --> checks.cac:3:7
  --> apps.ca:52:5
  help: an app has an owner team, written or inherited from its system
```

- **Severity.** `severity: warning` turns a failure into a warning, for a rule introduced gradually while the model is being completed; `severity: info` into an information, for what is worth knowing without being wrong (the servers hosting a single app, the flows without a known path). Neither fails `cadastre check`; an information shows in editors as such, and in the command line with its count (`1 info(s)`).
- **Checks run on a valid model.** As long as the project has errors, its checks are not run: their failures would be noise.
- **Where.** Checks are written in `.cac` files, apart from the views, as the tests of a project are apart from its code; by convention, a `checks.cac` at the root of the project, or `src/checks/<theme>.cac`. A `check` in a `.cav` or a `.ca` file is an error, and so is a `view` in a `.cac`. Their names are unique among checks; a check and a view may share a name.
- **Resilience** rules count: `check critical-in-two-sites { select app where criticality = high and count(on.ancestor(site)) < 2 expect none }`.
- **What they cover.** Invariants (security, ownership, architecture rules such as no direct access to another system's database: `edges [flow] where target.kind = database and source.ancestor(system) != target.ancestor(system)`, end-of-life operating systems and versions, data hosting), expectations on precise cases (the stores reach the ERP only through the VPN), the quality of the model (at most N `confidence: assumed`, no open `question` on a critical app).
- **Renderings.** Committed renderings are tested by `cadastre build --check`, which fails when a file of `out/` is missing or differs from what the views give now.

### Bootstrapping a registry: Terraform, CSV

A registry need not be typed from scratch: `cadastre import terraform <state> [--out <directory>]` reads a Terraform state, the JSON of `terraform show -json` or a `.tfstate` file (in a directory, its `terraform.tfstate`), and writes `.ca` files to `imported/`, one per site, formatted. It is a **one-off bootstrap**: the files are to be reviewed, completed (owners, operators, flows, which a state does not hold) and moved into the project; an existing file is never overwritten. The playground does the same in the browser (*Import Terraform state…*), the files added under `imported/`.

| Terraform | Cadastre |
|---|---|
| AWS: a VPC (`aws_vpc`), per region (per account and region when the state spans several accounts) | a `site` `aws-<region>` (`hosting: cloud`, `provider`, `region`), the `vpc` in it |
| `aws_subnet`, `azurerm_subnet`, `google_compute_subnetwork` | a `subnet` with its `cidr` (and `az`, `region`) |
| `aws_instance`, `azurerm_linux_virtual_machine`, `azurerm_windows_virtual_machine`, `google_compute_instance` | a `virtual server` in its subnet (its host unknown), with its `ip` and its type (`instance_type`, `size`, `machine_type`) |
| `aws_internet_gateway`, `aws_nat_gateway` | a `router` in the VPC (`role`, `public_ip`) |
| `aws_lb`, `aws_alb`, `aws_elb` | a `load_balancer` in the VPC (`dns_name`) |
| `aws_eks_cluster` | a `cluster` (`platform: eks`) |
| `aws_db_instance`, `aws_rds_cluster` | a `database` `on` the VPC of its subnet group (`engine`, `version`, `managed: rds`) |
| `azurerm_virtual_network` (per location), `google_compute_network` (per project) | a `site` `azure-<location>` or `gcp-<project>`, the network as its `vpc` |

Names come from the `Name` tag or the name of the cloud resource, as identifiers (`web-1`); the identifier of the cloud resource is kept in `cloud_id`, and its Terraform address in a comment above it (`// aws_instance.web[0]`). The other resources of the state are listed in `not-imported.ca`.

**From an inventory in CSV.** `cadastre import csv <file> [--kind <kind>] [--out <directory>]` reads a spreadsheet or an export of a CMDB, a resource per row, and writes `imported/<file>.ca`, formatted (*Import CSV inventory…* in the playground). As the Terraform import, a one-off bootstrap: the file is reviewed, completed and moved into the project, and the CSV left behind. A `for_each … in csv(…)` keeps instead the file as the source of a homogeneous series (§ 3, Series of resources).

```
kind;name;parent;ip;owner;description
site;Paris DC;;;;
server;web01;Paris DC;10.0.0.5;ops;Front server
virtual server;vm 02;Paris DC;10.0.0.6;ops;
app;shop;;;sales;"The shop, online"
```

```
site paris-dc {
  label: "Paris DC"
  server web01         { ip: 10.0.0.5, owner: ops, description: "Front server" }
  virtual server vm-02 { label: "vm 02", ip: 10.0.0.6, owner: ops }
}

app shop { owner: sales, description: "The shop, online" }
```

- **Columns**, by their header, in any case: `kind` (a kind of the language or of the project, `virtual server` for a virtual machine; without the column, `--kind` gives the kind of every row), `name`, `parent` (optional: the name of the row it is nested in), and any key. An empty cell gives no key.
- **Names** become identifiers (`Paris DC` → `paris-dc`), the name as written kept as the `label`; a name taken already among its siblings gets a number (`web01-2`). A `parent` is found by its name as written or as an identifier; a parent not in the file leaves the row at the root.
- **Values** are written as the schema types their key: a text quoted, a reference by its name as an identifier, a list split on its commas (`url`, `console`), a number, an address or a name bare; a key the schema does not know, as the value looks.
- **What to review** is listed at the top of the file: the kinds the language does not know (to declare in the schema of the project), the keys it does not know (to declare, or to rename: a column `Vendor ID` gives `vendor_id`), the rows without a name or a kind.
- **An export in its own words** (a CMDB: `Hostname`, `Type CI`, `Adresse IP`): `--map` reads its columns as the words of the language, `--kinds` its kinds, whatever the case; a column mapped to nothing is left out. The playground asks for the columns when the file has no `name` column.

  ```
  cadastre import csv export.csv --map "Hostname=name, Type CI=kind, Adresse IP=ip, Comment=" --kinds "Physical server=server, VM=virtual server"
  ```

- The format of the CSV is that of `for_each`: UTF-8, `,` or `;`, quoted fields as RFC 4180 says.

### Sharing a project without exposing it

`cadastre anonymize [directory] [--out <directory>]` writes a copy of the project (`anonymized/` beside it by default) where everything that identifies is replaced and everything that shapes the diagrams is kept, to report a bug or an unreadable view, or to hand a project to someone outside.

- **Kept**: the kinds, the nesting, the links with their ports, protocols and standard layers, the references, the views and the checks, the schema of the project.
- **Replaced, consistently across the files**: the names of the resources by their kind and a number (`app-7`, `server-12`), wherever they are written; the names of the views, checks, zones and project layers, the items of `for_each` lists, the host names; every string by a text of the same length (the width of a box counts for the layout); the addresses and networks, renumbered in `10.0.0.0/8` and `fd00::/8`; the comments, dropped.
- The copy compiles and renders the same views, to the names near; a cron expression in a string becomes free text (the job incomplete), a CSV file is kept as it is and reported.
- **Read the copy before sharing it**: a key of the project (`vendor_id`) and the words of the language stay, and a value the tool takes for a word of the language stays too.

### The site

`cadastre site --out <directory>` generates the **site of the project for its readers**: people who need the map without ever opening a `.ca` file. Static files, served by any web server, deployed by the continuous integration (GitHub Pages, an intranet, a shared folder), with nothing to install for the readers.

- **Home**: the project, its counts (the sites by `hosting` when there are several: `103 sites (93 office, 6 colocation, …)`), its checks (what fails, and what is to know: a check of severity `info`), its views as cards with their diagram, title and description (§ 6, `title` and `description`), grouped by file of views when there are several, the systems and the sites.
- **A page per view**: the diagram, whose nodes and frames lead to the pages of their resources, then the nodes and the edges as tables (the variants of a redundant way written `sw-01 or sw-02`). A view rendered as a `sequence` or a `traversal` shows that drawing first (the sequence diagram drawn by Mermaid from a CDN), the graph after it. *Open in the playground* opens the same view in the playground, for those who want to see how it is written.
- **A page per resource**: its path from the root as a breadcrumb, its kind, label and description, what it misses when it is incomplete, its owner, operator, site (through `on` for what runs on a machine) and system, every key written (references as links), what it contains, its members (a pool, a cluster), what runs on it, what runs here (on what it contains or on its members: the apps of a site), its incoming and outgoing links with their ports and data, what it depends on (the impact analysis), its open questions, the views it appears in.
- **Resources**: every resource by kind, except what is within another one (an interface, a component), on its page; **Checks**: the rules of the project and their results, what fails them linked; a **search** by name or label on every page.
- **The JSON model** under `api/` ([docs/api.md](api.md)): `project.json`, a record per resource, the views evaluated, the checks, for scripts, dashboards and agents; the **API** page of the site describes them, with a live example of each.
- **The playground** beside it, under `playground/`, read-only on the project, with a link back to the site (to the page of the view shown), when the command finds one (`--playground <directory>`, `CADASTRE_PLAYGROUND`, or `playground/` beside the executable, as the release archives ship it).

The diagrams are laid out by Graphviz when `dot` is installed; otherwise the pages draw them in the browser, with Graphviz for the browser taken from the playground or, failing that, from a CDN. Dark mode and printing follow the page.

`cadastre serve` serves the same site on `127.0.0.1:8080` (`--host`, `--port`), generated again from the files on disk when they change: the site of the project as it is being written. `--title <name>` names the site; otherwise the directory of the project.

**Languages.** The site, the playground published with it and the words the diagrams add (the legend, the tooltips of the satellites and of the twins) are written in the language of the project: `language = "fr"` in `cadastre.toml`, or `--language fr` on `site`, `serve` and `build`. English by default; the playground alone follows the language of the browser. The language itself stays in English, as every language does (`site`, `server`, `on`), and the contents of the project (labels, descriptions, titles) are those written by its authors. The diagnostics of the compiler follow too, in the command line, the editors (the language server reads the `cadastre.toml` of the project), the playground and the site. Languages known: `en`, `fr`.

### Exports

Views draw parts of the registry; an export hands **the whole registry** to another tool: every resource with its attributes, containment, references (`on`, `owner`, `rack`…) and links. `cadastre export <format> [directory] [--out <file>]` writes it to `out/model.<extension>`; the playground (*Export model…*) and VS Code (*Cadastre: Export model…*) generate it too. A project with errors is not exported.

| Format | File | For |
|---|---|---|
| `archimate` | `model.archimate.xml` | the ArchiMate Open Exchange Format, imported by Archi, Sparx EA and other modeling tools |
| `graphml` | `model.graphml` | a graph: yEd, Gephi, Cytoscape, NetworkX |
| `json` | `model.json` | the model for scripts: `{ "format": "cadastre-model", "version": 1, "resources": […], "links": […] }` |

- **ArchiMate.** Each resource is an element of the type closest to its kind: `site` a Location, `vpc`, `network`, `subnet` and `internet` Communication Networks, `server`, `cluster`, `pool` and `fleet` Nodes, network devices Devices, `vip` and `interface` Technology Interfaces, connectors Paths, `rack` Equipment, `system` a Grouping, `app`, `saas` and `component` Application Components, `job` an Application Process, `database` System Software, `channel` an Application Interface, `data` a Data Object, `team` and `actor` Business Actors. Relationships follow what ArchiMate allows between the two types: containment is an Aggregation or a Composition, `on` a Serving (an app served by its server) or an Assignment (a database on its server), a link a Flow between applications or between nodes; any other pair is an Association, named by its key or its port. The label is the name of the element; the description its documentation; the kind, the qualified name and every key are properties.
- **GraphML.** One node per resource (`kind`, `name`, `label`, `description`), one edge per relation, its `relation` the containment (`parent`, from the child), a reference key (`on`, `owner`…) or `link` (with `port`, `layers`, `carries`).
- **JSON.** One object per resource (`id` its qualified name, `kind`, `name`, `parent`, `attributes` as written, `references` resolved to qualified names, `incomplete`) and per link (`source`, `target`, `port`, `layers`, `attributes`, `carries`, `through`, `through_deduced` when it comes from an `egress` or an `ingress`).

## 7. Diagnostics

### Errors

| Case | Example |
|---|---|
| Resource in a `.cav`, `view` in a `.ca`, `check` outside a `.cac` | `server tmp01` in `network.cav` |
| Full link inside a block | `app x { a -> b }` |
| Qualified name declared twice | two `aws-paris.prod.dmz.web01` |
| Ambiguous name after filtering by kind | `web01` (server) exists in `dmz` and `erp` |
| Dangling reference | `on: web99` |
| Wrong target kind | `on: crmdb` while a `server` is expected |
| Link end outside `flow_endpoint` | `-> customer`, `-> dmz` |
| Unknown resource kind | `serveur web01` |
| Impossible placement | `server` in an `app`, `app` in a `subnet` |
| Value outside the schema for a known key | `subnet dmz { cidr: debian12 }`, `hosting: cloude` |
| Unknown attribute, in `--strict` mode only | `app billing { language: java }` |
| Cardinality violated | list given to `ref(server)`, three `ends` |
| Attribute written twice in a block | two `on:` in the same app |
| Link declared twice (same key) | the same link in `apps.ca` and `flows.ca`, different layers included |
| Rack in another site than the resource | `web01` in `paris-dc1`, `rack: r40` in `colo-pa3` |
| IP outside the subnet CIDR, or duplicated | `subnet: dmz, ip: 10.0.2.1` |
| Contradictory `peer` | `ge1 { peer: web01 }` and `ge2 { peer: web01 }` |
| Unknown kind, relation or option in a plan | `select serveur`, `via: hosted_on`, `merge: foo`, `follow(prices, to: x)` |
| Name of a plan that designates no resource of the expected kind | `follow(item-price)` (dangling), `follow(pricing-ui)` (an `app`, `data` expected) |
| Impossible `group-by` | several keys referencing K, kind both selected and grouping |
| Identical connector ends | `ends: [vgw, vgw]` |
| `channel` foreign to the link | `-> filehub { channel: prices }` while `prices` is in `kafka-cloud` |
| Invalid filter | `where language < java` (order comparison with a word), `where hosting > 3` (a key without order), `select database where cidr = …` (a key of no selected kind), `where verison = 2.0.0` |
| Several project `style` blocks | a root `style` block in `views.cav` and another in `theme.cav` |
| Incomplete element, in `--strict` mode only | `subnet dmz { }` without `cidr` |

Messages give every location involved and, when possible, help:
```
error: link declared twice: invoicing.billing -> invoicing.crm : https/443
  --> apps.ca:12:3
  --> flows.ca:4:1
  help: declare the link once, with all its layers

error: ambiguous name: web01 (server or cluster or fleet or subnet or vpc expected)
  --> apps.ca:3:7
  help: candidates: paris-dc1.lan.erp.web01, aws-paris.prod.dmz.web01; use a more qualified name, for instance erp.web01

error: server web01 cannot be placed in app billing
  --> infra.ca:4:10
  help: parent of server: subnet
```

### Warnings

| Case | Example |
|---|---|
| Link without a port, redundant with a link with a port | `a -> b [flow]` and `a -> b : https/443` |
| Unknown attribute close to a known key | `verison: 2.4.1` → "did you mean `version`?" |
| Layer absent from the registry in a plan | `edges [flwo]` |
| Edge dropped by `lift` (end without a target) | `billing -> stripe` in a `select subnet` view |
| Edge dropped by `with ends` (end of a kind used in `group-by`) | a flow to a `subnet` in a view grouped by `subnet` |
| Filter of a view that matches nothing: a style rule of the view applying to no node, frame or edge, the `where` of an edge selector keeping none of its edges, a zone filter matching nothing; with a hint when a compared value is a `label` or a short name compared to `name` | `edges where source.name = file-hub { … }`, `file-hub` being a label |
| `follow` reaching no link from its starting point | `follow(item-prices, from: catalog)` where `catalog` only receives prices |

### Incomplete elements

Third category: what is missing for a complete model, without being wrong.

| Case | Example |
|---|---|
| Missing required attribute | `subnet dmz { }` without `cidr`, `app logistics { }` without `on`, an app without an `owner` nor an owning system |
| Approximate placement | `server filehub` directly in a `site` |
| Virtual IP with neither address nor host name | `vip labels { }` |
| Schedule in free text | `job report { schedule: "10h, 12h10, 19h" }` |

- `scope: external` resources are never incomplete.
- By default, incomplete elements are **summarized in one line**; `--incomplete` gives the details, with what is missing for each element; `--strict` turns them into errors.

```
cadastre check
  0 error(s), 2 warning(s)
  37 incomplete element(s) (cadastre check --incomplete for details)
```

### Suggestions

All suggestions ("did you mean …?") follow the same rule.

**Distance**: Damerau-Levenshtein (a transposition counts as 1: `verison` → `version`).

| Length of the written name | Maximum distance |
|---|---|
| 1 to 3 | no suggestion (`os` does not suggest `on`) |
| 4 to 6 | 1 |
| 7 and more | 2 |

Always reported, whatever the length: a case difference (`Version`) and `_` versus `-` (`on_prem`).

**Candidates**: only names valid in the context (known keys of the kind and standard keys; kinds; symbol values; resources of the expected kind; relations and options of plans; existing layers).

| Case | Severity |
|---|---|
| free key close to a known key | warning |
| unknown kind | error |
| invalid symbol value (list of possible values) | error |
| dangling reference | error |
| unknown relation, option or key in a plan | error |
| layer absent from the registry | warning |

A free key warned about as close to a known key is silenced by declaring it in the schema of the project (§ 4, Schema of the project).

## 8. Complete example

This example lives in [`examples/reference/`](../examples/reference/).

`teams.ca`:
```
team infra           { label: "Infrastructure team" }
team invoicing-team  { label: "Invoicing team", contact: "#invoicing" }
team accounting-team { label: "Accounting team" }
```

`infra.ca`:
```
site aws-paris {
  hosting: cloud
  owner: infra
  provider: aws
  region: eu-west-3

  vpc prod {
    account: "111111111111"
    vpn_gateway vgw { label: "vgw-0a1b2c" }

    subnet dmz {
      cidr: 10.0.1.0/24
      az: eu-west-3a
      server web01 { os: debian12, ip: 10.0.1.10 }
      server web02 { os: debian12, ip: 10.0.1.11 }
    }
    subnet db {
      cidr: 10.0.2.0/24
      az: eu-west-3a
      server db01 { os: debian12, kernel: "6.1.0-18-amd64", ip: 10.0.2.10 }
    }
  }
}

site paris-dc1 {
  hosting: on-prem
  owner: infra
  rack r12 { room: "Room 2", units: 42 }

  firewall fw01 {
    rack: r12
    interface wan     { ip: 203.0.113.2 }
    interface lan-erp { subnet: erp, ip: 192.168.10.1 }
    vpn_gateway vpn   { public_ip: 203.0.113.2 }
  }

  switch sw01 {
    rack: r12
    interface ge-0-0-1 { label: "ge-0/0/1", peer: erp01 }
    interface ge-0-0-2 { label: "ge-0/0/2", peer: fw01.lan-erp }
  }

  network lan {
    subnet erp {
      cidr: 192.168.10.0/24
      server erp01 { rack: r12, ip: 192.168.10.20 }
    }
  }
}

vpn_tunnel paris-aws-1 { ends: [fw01.vpn, vgw], protocol: ipsec }
vpn_tunnel paris-aws-2 { ends: [fw01.vpn, vgw], protocol: ipsec, description: "Backup tunnel" }
```

`data.ca`:
```
data customer { description: "Customer details", classification: personal }
data invoice  { description: "Issued invoice",   classification: confidential }
```

`apps.ca`:
```
system invoicing {
  description: "Invoicing and customer relationship"
  owner: invoicing-team
  operator: infra

  app billing {
    description: "Monthly customer invoicing, PDF generation"
    on: [web01, web02]
    version: 2.4.1
    language: java
    license: "Apache-2.0"
    -> crm : https/443 [flow] {
      description: "Fetches customer details before invoicing"
      exchange: pull
      carries: [customer]
    }
    -> stripe     : https/443 [flow] { exchange: push, carries: [invoice] }
    -> analytics  : tcp/5432  [flow] { exchange: push, carries: [invoice] }
    -> legacy-erp : tcp/1521  [flow] {
      description: "Accounting export to the datacenter ERP"
      exchange: push
      carries: [invoice]
      through: [paris-aws-1, fw01]
    }

    component spring-boot { version: 3.2.1, license: "Apache-2.0" }
  }

  app crm {
    description: "Customer relationship management"
    on: web01
    version: v1.8.0
    -> crmdb : tcp/5432 [flow] { exchange: both, carries: [customer, invoice] }
  }

  database crmdb {
    on: db01
    engine: postgres
    version: 16.2
    stores: [customer, invoice]
  }

  database analytics {
    engine: postgres
    managed: rds
    on: db                                   // managed service, deployed in the db subnet
    stores: [invoice]
    question: "Is this database still used?"
  }
}

system accounting {
  owner: accounting-team
  app legacy-erp {
    label: "Accounting ERP"
    on: erp01
  }
}
```

`saas.ca`:
```
saas stripe {
  description: "Online invoice payment"
  owner: invoicing-team
  vendor: "Stripe"
  region: eu
  stores: [invoice]
}
```

`views.cav`:
```
style {
  layer flow      { color: steelblue }
  kind  connector { color: gray, line: dotted }
}

view flows {
  select app, database, saas
  edges  [flow]
  render svg
}

view network {
  select server, saas group-by site > subnet
  edges  lift([flow], via: on)
  render svg
}

view macro {
  select subnet
  edges  lift([flow], via: on?.ancestor(subnet), merge: port)
  render svg
}

view landscape {
  select system, saas
  edges  lift([flow], via: ancestor(system))
  render svg { orientation: data }
}

view to-verify {
  select app, database, saas, server where confidence != confirmed or exists(question) with ends
  edges  [flow] where confidence != confirmed or exists(question)
  render text
}

view ownership {
  select app, database, saas group-by owner
  render svg { node_labels: [label, operator] }
}

view to-complete {
  select server, app, database, subnet where incomplete
  render text
}

view inter-sites {
  select site, saas
  edges  lift([flow], via: on?.ancestor(site)), lift(connector, via: ancestor(site))
  render svg { legend: true }
}

view gdpr {
  select app, database, saas without isolated
  edges  [flow] where carries.classification = personal
  style  { layer flow { color: red } }
  render svg { edge_labels: [port, carries] }
}

view customer-trail {
  select app, database without isolated
  edges  follow(customer, from: crmdb)
  render svg { orientation: data, edge_labels: [step, port] }
}

view fw01-matrix {
  select server, saas
  edges  lift([flow] where through has fw01, via: on, merge: port)
  render text
}

view security {
  select server, firewall group-by site > subnet
  edges  lift([flow], via: on), through: show
  render svg
}

view physical {
  select server, switch, firewall group-by site > rack
  edges  lift(peer, via: parent)
  render svg
}

view deployment {
  select app, database, server
  edges  [flow], on
  render svg
}
```

Expected results:
- **flows**: `billing → crm` (https/443), `billing → stripe` (https/443), `billing → analytics` (tcp/5432), `billing → legacy-erp` (tcp/1521), `crm → crmdb` (tcp/5432).
- **network**: frames `aws-paris` > `dmz` (web01, web02), `db` (db01) and `paris-dc1` > `erp` (erp01); `stripe` outside any frame. Edges: `web02 → web01` (https/443, `customer`); `web01 → stripe` and `web02 → stripe` (https/443, `invoice`); `web01 → db01` (tcp/5432, `customer`, `invoice`); `web01 → erp01` and `web02 → erp01` (tcp/1521, `invoice`). `web01 → web01` is dropped (loop). `billing → analytics` is dropped **with a warning**: `analytics` is deployed on a subnet, not on a server, and is not selected.
- **macro**: `dmz → db` (tcp/5432, `customer`, `invoice`, provenance: `crm → crmdb` and `billing → analytics`, `analytics` sitting on the `db` subnet itself), `dmz → erp` (tcp/1521). `dmz → dmz` is dropped (loop). `billing → stripe` is dropped **with a warning**: `stripe` has neither `on` nor a subnet.
- **landscape** (data orientation): `invoicing → stripe` (https/443, `push`), `invoicing → accounting` (tcp/1521, `push`). Flows internal to `invoicing` are loops.
- **to-verify** (text): `analytics`, with its question "Is this database still used?". No flow is assumed, doubtful or questioned, so `with ends` adds nothing here (see `examples/retail` for a registry with uncertain flows).
- **ownership**: frames `invoicing-team` (billing, crm, crmdb, analytics, stripe) and `accounting-team` (legacy-erp); under billing, crm, crmdb and analytics, the operator `infra` inherited from the system (stripe has no operator).
- **to-complete** (text): empty, the example registry is complete (every component has an owner, inherited from its site or system).
- **inter-sites**: `aws-paris → stripe` (https/443), `aws-paris → paris-dc1` (tcp/1521), and the undirected edge `aws-paris — paris-dc1` (`vpn_tunnel ×2`). The other flows are loops inside `aws-paris`.
- **security**: `fw01` in the `paris-dc1` frame, outside subnets. The flow to the ERP is split: `web01 → fw01` and `web02 → fw01` (tcp/1521, with `paris-aws-1` in the label), `fw01 → erp01` (tcp/1521). Plus `web02 → web01` and `web01 → db01`. `billing → stripe` and `billing → analytics` are dropped with a warning.
- **physical**: frame `paris-dc1` > `r12` (fw01, sw01, erp01); web01, web02, db01 in the `aws-paris` frame (no rack, finest reachable level). Undirected edges `sw01 — erp01` and `sw01 — fw01`.
- **gdpr**: only flows carrying `personal` data: `billing → crm` (https/443, `customer`) and `crm → crmdb` (tcp/5432, `customer`, `invoice`), in red (overriding the project style). Flows carrying only `invoice` (`confidential`) are excluded, and so are the nodes left without an edge (`without isolated`): only `billing`, `crm` and `crmdb` remain.
- **fw01-failure**: `billing`, whose flow to `legacy-erp` goes through `fw01`; `legacy-erp` itself does not depend on the firewall.
- **customer-trail** (data orientation): the customer records followed from `crmdb`. Step 1, `crm — crmdb` (tcp/5432, `exchange: both`, the data leaving `crmdb` towards `crm`); step 2, `billing → crm` (https/443, `pull`: billing fetches the customers from the CRM). `billing → stripe`, `billing → analytics` and `billing → legacy-erp` only carry `invoice`: they are not followed, and `stripe`, `analytics` and `legacy-erp` are left without an edge.
- **fw01-matrix** (text): `web01 → erp01` and `web02 → erp01` (tcp/1521), with the path `paris-aws-1, fw01` and the provenance `billing → legacy-erp`.
- **deployment**: flows between apps and databases (`billing → crm`, `billing → analytics`, `billing → legacy-erp`, `crm → crmdb`) and `on` edges (`billing → web01`, `billing → web02`, `crm → web01`, `crmdb → db01`, `legacy-erp → erp01`). `billing → stripe` does not appear, `stripe` not being selected.
- **egress**: `billing → nat-gw → stripe` (https/443, `invoice`): the flow writes no `through`, the NAT gateway is deduced from the `egress` of the VPC `prod` and drawn by `through: show`; the apps without a flow to a SaaS are left out (`without isolated`).

## 9. Beyond v1

No syntax question is open for v1. What was deliberately postponed is listed in the [roadmap](../ROADMAP.md), in particular:

- the declarative schema (`kind`, `group`) and its conditional constraints;
- modules, `import`, `for_each`;
- the `check` operator (`through` consistency, CIDR overlap, compliance rules);
- deducing traversals from the topology;
- typed VLANs;
- aggregates and regular expressions in filters;
- logical links through a channel (`bridge`): "B consumes what A produces";
- a data direction per carried object;
- typed cron expressions;
- serverless and PaaS, network policies and service mesh;
- icons, named themes, manual placement and PNG / PDF exports;
- ArchiMate, draw.io, GraphML and JSON exports;
- importing from external sources (Terraform, cloud APIs, Kubernetes, CMDB, CSV).

# Project layout

A recommendation for organizing the files of a project, not a rule of the language: the compiler reads every `.ca`, `.cav` and `.cac` of the directory recursively, and folders and file names mean nothing to it ([guide](guide.md), § 1). Names resolve across the whole project, so files can be moved and split freely; this layout only makes a large registry easy to find one's way in, and easy to review.

`cadastre init` creates this layout, with a first site, system, view and check to replace.

## The rule

**Sources in `src/`; inside, technical theme first, functional theme second.** One folder per technical theme (sites, network, cloud, applications…); inside it, one file per functional theme (billing, orders, warehouse…) wherever the theme can be split that way, with the same file names from one folder to another.

**Business processes the same way, business domain first, functional theme second.** In `src/processes/`, one folder per business domain (finance, sales, IT operations…); inside it, one file per functional theme, named as in `apps/` and `views/`.

```
my-project/
├── cadastre.toml             # the project: sources = ["src"], its modules
├── README.md                 # what the registry covers, how to build it
├── docs/                     # notes, conventions of the team
├── out/                      # renderings: cadastre build src --out out
└── src/
    ├── schema.ca             # the schema of the project: typed keys, kinds of its own
    ├── teams.ca              # teams and actors
    ├── data.ca               # business objects (data)
    ├── data/                 # CSV files read by for_each: the list of the stores…
    ├── sites/                # one file per site: its networks, subnets, devices and servers
    │   ├── dc-alpha.ca
    │   ├── dc-beta.ca
    │   └── offices.ca
    ├── network/              # what links the sites
    │   ├── wan.ca            # WAN links, operators
    │   └── vpn.ca            # VPN tunnels, VPC peerings
    ├── cloud/
    │   ├── aws.ca            # cloud sites, VPCs, managed services
    │   └── saas.ca           # SaaS used by several functional themes
    ├── apps/                 # one file per functional theme
    │   ├── billing.ca        # system billing: its apps, jobs, databases and their flows
    │   ├── orders.ca
    │   ├── warehouse.ca
    │   └── shared.ca         # what several themes use: identity provider, scheduler, message broker
    ├── processes/            # one folder per business domain, one file per functional theme
    │   ├── finance/
    │   │   └── billing.ca    # the processes of billing: monthly invoicing, dunning
    │   ├── sales/
    │   │   └── orders.ca     # order to delivery
    │   └── it-operations/
    │       └── backup.ca     # the backup of the virtual machines
    ├── views/
    │   ├── style.cav         # the project style block
    │   ├── network.cav       # network views
    │   ├── billing.cav       # the views of each functional theme
    │   └── orders.cav
    └── checks/
        ├── ownership.cac     # the rules of the project, by theme: the tests
        └── security.cac
```

Looking for "the billing flows" means opening `src/apps/billing.ca`; for "what runs in dc-alpha", `src/sites/dc-alpha.ca`; for "how the invoices of the month are produced", `src/processes/finance/billing.ca`. A functional theme reads across the folders under the same file name (`apps/billing.ca`, `views/billing.cav`); a technical theme reads in its folder.

Keeping the sources in `src/` leaves room beside them for the documentation and the renderings, and lets the commands target the registry alone:

```bash
cadastre check src
cadastre fmt src --check
cadastre build src --out out
```

## Technical themes

| Folder | Contains |
|---|---|
| `sites/` | on-premises and hosting sites, each with its networks, subnets, network devices, interfaces and cables (`peer`), servers, clusters, racks, virtual IPs |
| `network/` | connectors between sites (`wan_link`, `vpn_tunnel`, `vpc_peering`) and the sites of network operators |
| `cloud/` | cloud sites and VPCs, managed services, SaaS |
| `apps/` | systems, apps, jobs, databases, components, channels, and the flows they initiate |
| `processes/` | business processes (`process`), their participants and steps: one folder per business domain, one file per functional theme |
| `views/` | `.cav` files: views and the project style |
| `checks/` | `.cac` files: the checks, one file per theme of rules (ownership, security, resilience…) |
| `src/` itself | `schema.ca`, `teams.ca`, `data.ca`: what every theme references |

A small project can keep one file per technical theme directly in `src/` (`infra.ca`, `apps.ca`, `views.cav`…), as the [examples](../examples) do; folders come when a theme grows beyond one file.

## What can be split by functional theme

- **Applications: yes.** A system and its apps, jobs and databases form one block, `system billing { … }`, which goes into `apps/billing.ca`. Their infrastructure is designated by name (`on: billing-srv01`), wherever it is written.
- **Infrastructure: the site in its file, the servers by functional theme if wanted.** The site file holds the structure (site → network → subnet, the network devices); the servers of a functional theme can be written in a file of their own, placed by reference: `server billing-srv01 { parent: dc-alpha.lan.servers }` in `infra/billing.ca` ([guide](guide.md), § Containment). A server is then found by its theme, and the site file stays short. The functional theme of a server is also carried by `system: billing` (or derived from the `on` of what it hosts), which views select (`select server where system = billing`).
- **Flows: with the resource that initiates them**, in its block (`-> target : https/443 [flow]`): `apps/billing.ca` holds every flow initiated by billing, towards any theme; a flow between two themes is found in the file of its source.
- **What is shared goes into a file of its own**: `apps/shared.ca` for the apps several themes rely on, `cloud/saas.ca` for a SaaS used by several themes. Do not attach it to one of the themes arbitrarily.
- **Business processes: in `processes/<domain>/<theme>.ca`, not with the resources they go through.** A process names resources of several themes and sites, as steps; written in the file of one of them (an app, a site), it would be hard to find and would hide the others. It goes at the root of its file: a `system` block is written once, in `apps/`. A process serving several functional themes goes into the file of the theme it is for (the invoicing of the orders, into `finance/billing.ca`); its views into `views/<theme>.cav` (`edges process(…)`, `render sequence`).
- **Business objects** are referenced from every theme (`carries`, `stores`): they stay together in `data.ca`, or in `data/<theme>.ca` when there are many.
- **Views** follow the functional split: `views/<theme>.cav` for the views of a theme, `views/network.cav` for the network pages, the single project `style` block in `views/style.cav`. **Checks** go in `checks/<theme>.cac`: a file per family of rules rather than per functional theme, since a rule usually spans the themes.

## Names

- Folder and file names in lowercase, with hyphens, in the language of the team; the same functional names everywhere (`billing` in `apps/`, `processes/finance/` and `views/`, and as the name of the system).
- File names are not namespaces: two resources with the same name in two files are resolved like any homonyms (by suffix, by scope of system, by expected kind), and a duplicated qualified name is an error wherever the files are.
- Run `cadastre fmt src` before committing, so that diffs only show what changed.

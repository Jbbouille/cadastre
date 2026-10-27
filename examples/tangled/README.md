# Tangled: a real model, anonymised, with views that are still spaghetti

The other examples are written to read well. This one is not: it is the map of a real information system (about 1,000 resources and 440 links), anonymised, kept as it was while it was being completed. Its views are the ones that are still hard to read, to have real cases when working on the layout (Graphviz and ELK), the labels, satellites, lifting or new renderings.

## What was anonymised

- Every resource is renamed after its kind: `app-001`, `srv-012`, `db-03`, `sys-04`, `site-05`, `sw-02`… The structure is kept: nesting, `on`, `peer` cables, links, `carries`, processes.
- Labels, descriptions, questions, fully qualified names, serial numbers, URLs, repositories and catalogue identifiers are removed. A node shows its name.
- IP ranges are shifted, `/16` by `/16`, so that subnets and the addresses in them stay consistent.
- The stores keep their number, their table (`for_each`) and their addresses, shifted; their names and codes are replaced by `Store 001`, `s001`…
- What is left as text is generic: hardware models, operating systems, sizes, cron schedules.

## The views

| View | What makes it tangled | Edges |
|---|---|---|
| `tangled-databases` | 124 database flows of one integration platform, grouped by instance | 124 |
| `tangled-hub` | one hub and its batch, 84 flows, a very tall column of the stores' servers | 84 |
| `tangled-lifted` | flows lifted to their servers, jobs and file drops across seven systems | 82 |
| `tangled-systems` | four systems and everything around them (`with ends`) | 77 |
| `tangled-directory` | many callers of two identity services, right to left, styled edges | 63 |
| `tangled-backup` | a backup platform across two sites, control flows crossing the data flows | 50 |
| `tangled-zones` | a legacy system drawn in zones, its neighbours across them | 50 |
| `tangled-network` | the cabling of every site, lifted to the devices, with WAN and VPN | 50 |

## Measured

Crossings and drawn edges of each view, laid out by Graphviz, before and after the changes made on them: the twins (`twins`: the nodes of a frame with exactly the same exchanges drawn as one box) and several layouts tried (the frames in several orders, at the root and inside the frames, each with the ranks over the whole view or frame by frame, the drawing that reads best kept); the satellites were already in place.

| View | Edges | Crossings |
|---|---|---|
| `tangled-databases` | 124 → 17 | 512 → 0 |
| `tangled-hub` | 84 → 45 | 38 → 0 |
| `tangled-systems` | 77 → 53 | 45 → 3 |
| `tangled-lifted` | 82 → 79 | 34 → 15 |
| `tangled-backup` | 50 → 50 | 25 → 1 |
| `tangled-directory` | 64 → 52 | 2 → 0 |
| `tangled-network` | 50 → 45 | 6 → 4 |
| `tangled-zones` | 50 → 45 | 3 → 2 |

`tangled-lifted` keeps 15 crossings in every order tried: they come from a cycle, `app-160` feeding jobs that feed `saas-06`, which feeds jobs that feed `app-160` back; drawn in the direction of the data, part of the cycle runs backwards and crosses the fan of jobs. Drawn by connection instead (`orientation: connection`), it has 59.

```
cadastre build examples/tangled --out out
```

Six warnings are expected in `tangled-network`: cables to servers that the view does not select (servers without a `model`), as in the original.

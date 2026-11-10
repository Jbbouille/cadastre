//! Layout hints (spec § 6, `render`): `direction: rl | bt`, `stack`, and `layout: free` edges.

use std::collections::HashMap;
use std::io::Write;
use std::process::{Command, Stdio};

use cadastre_core::{Compilation, Severity, SourceFile, build, compile};

/// Two clusters: iSCSI flows go from the second one to the storage VMs of the first one,
/// mirrored to each other; a backup server has no flow.
const REGISTRY: &str = r#"
site dc {
  hosting: on-prem
  network lan {
    subnet servers {
      cidr: 10.0.0.0/24
      cluster cluster-a
      cluster cluster-b
      server esx1 { cluster: cluster-a }
      server esx2 { cluster: cluster-a }
      server esx4 { cluster: cluster-b, -> datacore01 : iscsi [flow], -> datacore02 : iscsi [flow] }
      server veeam
    }
  }
}
app datacore01 { on: esx1, -> datacore02 : mirror [flow] }
app datacore02 { on: esx2 }
"#;

fn compilation(render: &str, style: &str) -> Compilation {
    let view = format!(
        "view storage {{\n  select server, app group-by cluster\n  edges [flow]\n  style {{ {style} }}\n  render {render}\n}}"
    );
    compile(&[
        SourceFile { path: "a.ca".into(), text: REGISTRY.into() },
        SourceFile { path: "v.cav".into(), text: view },
    ])
}

fn errors(c: &Compilation) -> Vec<String> {
    c.diagnostics.iter().filter(|d| d.severity == Severity::Error).map(|d| d.message.clone()).collect()
}

fn output(c: &Compilation, format: &str) -> String {
    let (outputs, _) = build(c, None);
    outputs.into_iter().find(|o| o.format == format).unwrap().content
}

/// The DOT identifier of a node, from its tooltip (`n12`).
fn id(dot: &str, qualified: &str) -> String {
    // Tooltips are escaped in DOT: a line break is written `\n`.
    let line = dot
        .lines()
        .find(|l| l.contains(&format!("tooltip=\"{qualified}\\n")) || l.contains(&format!("tooltip=\"{qualified}\"")))
        .unwrap();
    line.split_whitespace().next().unwrap().to_owned()
}

#[test]
fn directions() {
    let c = compilation("dot, mermaid { direction: bt }", "");
    assert!(errors(&c).is_empty(), "{:?}", errors(&c));
    let dot = output(&c, "dot");
    assert!(dot.contains("rankdir=BT"), "{dot}");
    // Graphviz draws frame titles at the bottom when ranks go up: brought back to the top.
    assert!(dot.contains("labelloc=\"b\";"), "{dot}");
    assert!(output(&c, "mermaid").contains("flowchart BT"));

    let c = compilation("dot { direction: rl }", "");
    let dot = output(&c, "dot");
    assert!(dot.contains("rankdir=RL") && !dot.contains("labelloc"), "{dot}");

    let c = compilation("dot { direction: up }", "");
    assert!(errors(&c).iter().any(|m| m == "invalid value for direction: up"), "{:?}", errors(&c));
}

#[test]
fn stack_orders_frames_and_nodes() {
    let c = compilation("dot, mermaid { direction: bt, stack: [cluster-a, cluster-b, veeam] }", "");
    assert!(errors(&c).is_empty(), "{:?}", errors(&c));
    let dot = output(&c, "dot");
    let hints: Vec<&str> = dot.lines().filter(|l| l.contains("style=invis")).map(str::trim).collect();
    // Bottom to top: the edges go from the lower item to the upper one: from the last nodes of a
    // frame (no edge inside it leaves them) to the first nodes of the next one (none reaches them).
    let node = |name: &str| id(&dot, &format!("server dc.lan.servers.{name}"));
    let (esx1, esx2, esx4, veeam) = (node("esx1"), node("esx2"), node("esx4"), node("veeam"));
    let datacore01 = id(&dot, "app datacore01");
    let upward = "[style=invis, ltail=cluster_1, lhead=cluster_0];";
    assert_eq!(
        hints,
        [
            format!("{esx4} -> {esx1} {upward}"),
            format!("{esx4} -> {esx2} {upward}"),
            format!("{esx4} -> {datacore01} {upward}"),
            format!("{veeam} -> {esx4} [style=invis, lhead=cluster_1];"),
        ],
        "{dot}"
    );

    // Mermaid: invisible links after the drawn ones.
    let mermaid = output(&c, "mermaid");
    let lines: Vec<&str> = mermaid.lines().collect();
    let last_drawn = lines.iter().rposition(|l| l.contains("-->")).unwrap();
    let first_hint = lines.iter().position(|l| l.contains("~~~")).unwrap();
    assert!(first_hint > last_drawn, "{mermaid}");
    assert!(mermaid.contains("  g1 ~~~ g0") && mermaid.contains(&format!("  {veeam} ~~~ g1")), "{mermaid}");

    let c = compilation("dot { stack: [cluster-a, nowhere] }", "");
    assert!(errors(&c).iter().any(|m| m == "dangling reference: nowhere"), "{:?}", errors(&c));
}

#[test]
fn stack_levels_hold_several_items() {
    // The two clusters on the same level, above the backup server.
    let c = compilation("dot, mermaid { direction: tb, stack: [[cluster-a, cluster-b], veeam] }", "");
    assert!(errors(&c).is_empty(), "{:?}", errors(&c));
    let dot = output(&c, "dot");
    let hints: Vec<&str> = dot
        .lines()
        .filter(|l| l.contains("style=invis") && !l.trim_start().starts_with("level") && !l.contains("minlen=0"))
        .map(str::trim)
        .collect();
    let node = |name: &str| id(&dot, &format!("server dc.lan.servers.{name}"));
    let (esx1, esx2, esx4, veeam) = (node("esx1"), node("esx2"), node("esx4"), node("veeam"));
    let datacore02 = id(&dot, "app datacore02");
    // From the last nodes of each cluster of the first level to the backup server; none between
    // the two clusters.
    assert_eq!(
        hints,
        [
            format!("{esx1} -> {veeam} [style=invis, ltail=cluster_0];"),
            format!("{esx2} -> {veeam} [style=invis, ltail=cluster_0];"),
            format!("{datacore02} -> {veeam} [style=invis, ltail=cluster_0];"),
            format!("{esx4} -> {veeam} [style=invis, ltail=cluster_1];"),
        ],
        "{dot}"
    );
    let mermaid = output(&c, "mermaid");
    assert!(
        mermaid.contains(&format!("  g0 ~~~ {veeam}")) && mermaid.contains(&format!("  g1 ~~~ {veeam}")),
        "{mermaid}"
    );

    // A frame is drawn in one piece: separating its clusters with the backup server is a
    // contradiction, reported as a warning.
    let c = compilation("dot { stack: [esx1, veeam, esx2] }", "");
    let (_, diagnostics) = build(&c, None);
    let warnings: Vec<&str> = diagnostics.iter().map(|d| d.message.as_str()).collect();
    assert_eq!(
        warnings,
        [
            "view storage: stack separates esx1 and esx2 with veeam, but they are in the frame cluster-a, drawn in one piece"
        ]
    );
    for stack in ["[[cluster-a, cluster-b], veeam]", "[esx1, esx2, veeam]", "[esx1, [esx2, veeam]]"] {
        let (_, diagnostics) = build(&compilation(&format!("dot {{ stack: {stack} }}"), ""), None);
        // No warning; the edges between the clusters of a level, an info (`cross_orders_a_level_of_stack`).
        assert!(diagnostics.iter().all(|d| d.severity == Severity::Info), "{stack}: {diagnostics:?}");
    }

    // Elsewhere, a list cannot contain a list.
    let c = compilation("dot { cross: [[cluster-a, cluster-b], veeam] }", "");
    assert_eq!(errors(&c), ["cross: a list cannot contain a list"]);
    let c = compile(&[SourceFile { path: "a.ca".into(), text: "server s { tags: [[a, b], c] }".into() }]);
    assert_eq!(errors(&c), ["tags: a list cannot contain a list"]);
}

#[test]
fn free_edges_place_no_node() {
    let c = compilation("dot", "edges where proto = mirror { layout: free }");
    assert!(errors(&c).is_empty(), "{:?}", errors(&c));
    let dot = output(&c, "dot");
    let mirror = dot.lines().find(|l| l.contains("constraint=false")).unwrap();
    // Drawn with its label placed against the drawn edge (`xlabel`), without a text node.
    assert!(mirror.contains("xlabel=\"mirror\"") && mirror.contains("dir=forward"), "{mirror}");
    assert!(dot.contains("forcelabels=true"), "{dot}");
    assert!(!dot.lines().any(|l| l.contains("shape=none") && l.contains("label=\"mirror\"")), "{dot}");
    // The other edges keep their text node.
    assert!(dot.lines().any(|l| l.contains("shape=none") && l.contains("label=\"iscsi\"")), "{dot}");

    let c = compilation("dot", "edges { layout: fixed }");
    assert!(errors(&c).iter().any(|m| m == "invalid value for layout: fixed"), "{:?}", errors(&c));
}

#[test]
fn the_retail_example_stacks_its_systems() {
    let files: Vec<SourceFile> =
        ["teams.ca", "infra.ca", "product.ca", "data.ca", "saas.ca", "systems.ca", "views.cav"]
            .iter()
            .map(|f| SourceFile {
                path: (*f).to_owned(),
                text: std::fs::read_to_string(format!("{}/../../../examples/retail/{f}", env!("CARGO_MANIFEST_DIR")))
                    .unwrap(),
            })
            .collect();
    let c = compile(&files);
    assert!(errors(&c).is_empty(), "{:?}", errors(&c));
    let (outputs, diagnostics) = build(&c, Some("systems-stacked"));
    assert!(diagnostics.iter().all(|d| d.severity != Severity::Error), "{diagnostics:?}");
    let dot = &outputs[0].content;
    // Three pairs of frames, each tied from the last nodes of one to the first nodes of the next.
    let hints = dot.lines().filter(|l| l.contains("style=invis")).count();
    assert!(hints >= 3, "{dot}");
    assert!(dot.contains("rankdir=TB") && dot.contains("constraint=false"), "{dot}");
}

#[test]
fn cross_orders_across_the_direction() {
    // Where a frame is written in the DOT, by its title.
    let at = |dot: &str, title: &str| dot.find(&format!("<td>{title}</td>")).unwrap();

    // Left to right: Graphviz draws the first frame written in a rank at the bottom, so the
    // frame to show on top is written last.
    let c = compilation("dot { cross: [cluster-b, cluster-a] }", "");
    assert!(errors(&c).is_empty(), "{:?}", errors(&c));
    let dot = output(&c, "dot");
    assert!(at(&dot, "cluster-a") < at(&dot, "cluster-b"), "{dot}");
    let c = compilation("dot { cross: [cluster-a, cluster-b] }", "");
    let dot = output(&c, "dot");
    assert!(at(&dot, "cluster-b") < at(&dot, "cluster-a"), "{dot}");

    // Top to bottom: left to right, written in the given order; Mermaid in the given order too.
    let c = compilation("dot, mermaid { direction: tb, cross: [cluster-b, cluster-a] }", "");
    let dot = output(&c, "dot");
    assert!(at(&dot, "cluster-b") < at(&dot, "cluster-a"), "{dot}");
    let mermaid = output(&c, "mermaid");
    assert!(mermaid.find("[\"cluster-b\"]").unwrap() < mermaid.find("[\"cluster-a\"]").unwrap(), "{mermaid}");

    let c = compilation("dot { cross: [nowhere] }", "");
    assert!(errors(&c).iter().any(|m| m == "dangling reference: nowhere"), "{:?}", errors(&c));
}

#[test]
fn the_data_orientation_also_orients_the_layout() {
    // An ESB reads the extractors (`pull`) and pushes to a portal: with `orientation: data`, the
    // extractors come before the ESB, as the arrows say.
    let registry = "app esb {\n  -> extractor : sftp [flow] { exchange: pull }\n  -> portal : https [flow] { exchange: push }\n}\n\
                    app extractor\napp portal";
    let view = "view data {\n  select app\n  edges [flow]\n  render dot, mermaid, ascii { orientation: data, edge_labels: [none] }\n}";
    let c = compile(&[
        SourceFile { path: "a.ca".into(), text: registry.into() },
        SourceFile { path: "v.cav".into(), text: view.into() },
    ]);
    assert!(errors(&c).is_empty(), "{:?}", errors(&c));
    let dot = output(&c, "dot");
    let (esb, extractor, portal) = (id(&dot, "app esb"), id(&dot, "app extractor"), id(&dot, "app portal"));
    // Drawn from the data source, the dot of the initiator at the ESB in both (`initiator`).
    assert!(dot.contains(&format!("  {extractor} -> {esb} [dir=both")), "{dot}");
    assert!(dot.contains(&format!("  {esb} -> {portal} [dir=both")), "{dot}");
    let mermaid = output(&c, "mermaid");
    assert!(mermaid.contains(&format!("{extractor} --> {esb}")), "{mermaid}");
    // Left to right in ASCII art: extractor, then esb, then portal.
    let ascii = output(&c, "ascii");
    let column = |name: &str| ascii.lines().find_map(|l| l.find(name)).unwrap();
    assert!(column("extractor") < column("esb") && column("esb") < column("portal"), "{ascii}");
}

#[test]
fn routing_and_labels_placed_after_the_layout() {
    let c = compilation("dot, mermaid { routing: orthogonal, edge_labels_layout: free }", "");
    assert!(errors(&c).is_empty(), "{:?}", errors(&c));
    let dot = output(&c, "dot");
    assert!(dot.contains("splines=ortho") && dot.contains("forcelabels=true"), "{dot}");
    // Labels on the edges, not as nodes laid out with the others.
    assert!(dot.contains("xlabel=\"iscsi\"") && !dot.contains("shape=none"), "{dot}");
    assert!(output(&c, "mermaid").contains("    curve: step\n"));
    // By default, curved edges and labels as nodes.
    let dot = output(&compilation("dot", ""), "dot");
    assert!(!dot.contains("splines") && dot.contains("shape=none"), "{dot}");
    let c = compilation("dot { routing: zigzag }", "");
    assert_eq!(errors(&c), ["invalid value for routing: zigzag"]);
}

#[test]
fn first_and_last_in_their_frame() {
    // The backup server first at the top level, before the two cluster frames; esx2 last in its frame.
    let c = compilation("dot { direction: tb, first: [veeam], last: [esx2] }", "");
    assert!(errors(&c).is_empty(), "{:?}", errors(&c));
    let dot = output(&c, "dot");
    let hints: Vec<&str> = dot.lines().filter(|l| l.contains("style=invis")).map(str::trim).collect();
    let node = |name: &str| id(&dot, &format!("server dc.lan.servers.{name}"));
    let (esx1, esx2, veeam) = (node("esx1"), node("esx2"), node("veeam"));
    assert!(hints.contains(&format!("{veeam} -> {esx1} [style=invis, lhead=cluster_0];").as_str()), "{dot}");
    assert!(hints.contains(&format!("{esx1} -> {esx2} [style=invis];").as_str()), "{dot}");
}

#[test]
fn undirected_edges_ignore_the_side_declaring_them() {
    // The same cable, declared by either switch: drawn in the order of the registry.
    let dot_of = |registry: &str| {
        let view = "view cables {\n  select switch\n  edges  lift(peer, via: parent)\n  render dot\n}";
        let c = compile(&[
            SourceFile { path: "a.ca".into(), text: registry.into() },
            SourceFile { path: "v.cav".into(), text: view.into() },
        ]);
        output(&c, "dot")
    };
    let declared_by_first = dot_of("switch sw1 { interface p1 { peer: sw2.p1 } }\nswitch sw2 { interface p1 }");
    let declared_by_second = dot_of("switch sw1 { interface p1 }\nswitch sw2 { interface p1 { peer: sw1.p1 } }");
    let edge = |dot: &str| dot.lines().find(|l| l.contains("dir=none")).unwrap().trim().to_owned();
    let (a, b) = (edge(&declared_by_first), edge(&declared_by_second));
    assert_eq!(a.split(" [").next(), b.split(" [").next(), "{a}\n{b}");
    // A node listed in `first` starts the cable.
    let c = compile(&[
        SourceFile {
            path: "a.ca".into(),
            text: "switch sw1 { interface p1 { peer: sw2.p1 } }\nswitch sw2 { interface p1 }".into(),
        },
        SourceFile {
            path: "v.cav".into(),
            text: "view cables {\n  select switch\n  edges  lift(peer, via: parent)\n  render dot { first: [sw2] }\n}"
                .into(),
        },
    ]);
    let dot = output(&c, "dot");
    let (sw1, sw2) = (id(&dot, "switch sw1"), id(&dot, "switch sw2"));
    assert!(
        dot.contains(&format!("  {sw2} -> e0 [dir=none")) && dot.contains(&format!("  e0 -> {sw1} [dir=none")),
        "{dot}"
    );
}

#[test]
fn cross_cuts_the_crossing_minimization_short() {
    let dot = output(&compilation("dot { cross: [cluster-b, cluster-a] }", ""), "dot");
    assert!(dot.contains("mclimit=0.01"), "{dot}");
    assert!(!output(&compilation("dot", ""), "dot").contains("mclimit"));
}

#[test]
fn spacing_and_sizes() {
    let c = compilation(
        "dot, mermaid { spacing: [2, 1.5], node_label_size: 12, edge_label_size: 8, frame_label_size: 14 }",
        "",
    );
    assert!(errors(&c).is_empty(), "{:?}", errors(&c));
    let dot = output(&c, "dot");
    assert!(dot.contains("fontsize=14, compound=true") && dot.contains("nodesep=0.5, ranksep=0.75"), "{dot}");
    assert!(dot.contains("node [fontname=\"Helvetica\", fontsize=12]"), "{dot}");
    assert!(dot.contains("edge [fontname=\"Helvetica\", fontsize=8]"), "{dot}");
    let mermaid = output(&c, "mermaid");
    assert!(
        mermaid.contains(
            "  flowchart:
    nodeSpacing: 100
    rankSpacing: 75
"
        ),
        "{mermaid}"
    );
    // One factor for both; by default, Graphviz's own spacing.
    assert!(output(&compilation("dot { spacing: 1.5 }", ""), "dot").contains("nodesep=0.375, ranksep=0.75"));
    assert!(!output(&compilation("dot", ""), "dot").contains("nodesep"));

    let c = compilation("dot { spacing: [1, 2, 3], node_label_size: big }", "");
    assert_eq!(errors(&c), ["invalid value for spacing: [1, 2, 3]", "invalid value for node_label_size: big"]);
}

#[test]
fn a_level_of_stack_side_by_side() {
    // The two clusters on a level: the flows between them have a length of zero, so that the
    // second one stays beside the first one; the flow within a cluster keeps its rank.
    let c = compilation("dot { stack: [[cluster-a, cluster-b]] }", "");
    assert!(errors(&c).is_empty(), "{:?}", errors(&c));
    let dot = output(&c, "dot");
    let (esx4, datacore01, datacore02) =
        (id(&dot, "server dc.lan.servers.esx4"), id(&dot, "app datacore01"), id(&dot, "app datacore02"));
    let edge = |from: &str, to: &str| {
        dot.lines().find(|l| l.trim_start().starts_with(&format!("{from} -> {to} "))).unwrap_or_default().to_owned()
    };
    // Written from cluster-a, the first of the level, their arrows turned: Graphviz puts the tail
    // of a flat edge first, on top in `lr`.
    let across = |a: &str, b: &str| format!("{}{}", edge(a, b), edge(b, a));
    assert!(across(&esx4, &datacore01).contains("minlen=0"), "{dot}");
    assert!(across(&esx4, &datacore02).contains("minlen=0"), "{dot}");
    assert!(edge(&datacore01, &esx4).contains("[dir=back, minlen=0"), "{dot}");
    assert!(!dot.contains("constraint=false"), "{dot}");
    assert!(!edge(&datacore01, &datacore02).contains("minlen=0"), "{dot}");
    // An invisible point before each cluster, all on one rank, pulling the clusters to the same
    // rank; the point and the nodes of its cluster in a group, the points in the order of the level.
    assert!(dot.contains("  { rank=same; level0_0; level0_1; }"), "{dot}");
    assert!(dot.contains("  level0_1 [shape=point, style=invis"), "{dot}");
    assert!(dot.contains(&format!("  level0_1 -> {esx4} [style=invis, lhead=cluster_")), "{dot}");
    assert!(dot.contains(&format!("  {esx4} [group=\"level0_1\"];")), "{dot}");
    assert!(dot.contains("  level0_0 -> level0_1 [style=invis];"), "{dot}");
}

#[test]
fn a_straight_edge() {
    let c = compilation("dot", "edges where proto = mirror { routing: straight }");
    assert!(errors(&c).is_empty(), "{:?}", errors(&c));
    let dot = output(&c, "dot");
    let (datacore01, datacore02) = (id(&dot, "app datacore01"), id(&dot, "app datacore02"));
    // In one piece, its label on it: redrawn as a straight line in SVG.
    let line = dot.lines().find(|l| l.trim_start().starts_with(&format!("{datacore01} -> {datacore02} "))).unwrap();
    assert!(line.contains("xlabel=\"mirror\"") && line.contains("class=\"straight on_edge\""), "{line}");
    let c = compilation("dot", "edges where proto = mirror { routing: orthogonal }");
    assert_eq!(errors(&c), ["invalid value for routing: orthogonal"]);
}

#[test]
fn cross_orders_a_level_of_stack() {
    // The two clusters on a level, cluster-a first across the direction (the order of `cross`,
    // or the order written): the iSCSI flows from
    // cluster-b to cluster-a are written from cluster-a, their arrows turned, so that Graphviz puts
    // cluster-a first; without its second crossing minimization, which would undo it.
    let c = compilation("dot { direction: tb, stack: [[cluster-a, cluster-b]], cross: [cluster-a, cluster-b] }", "");
    assert!(errors(&c).is_empty(), "{:?}", errors(&c));
    let dot = output(&c, "dot");
    let (esx4, datacore01) = (id(&dot, "server dc.lan.servers.esx4"), id(&dot, "app datacore01"));
    assert!(dot.contains("remincross=false"), "{dot}");
    assert!(dot.contains(&format!("  {datacore01} -> {esx4} [dir=back,")), "{dot}");
    assert!(dot.contains("  level0_0 -> level0_1 [style=invis];"), "{dot}");
    // `cross` the other way: the points of the level the other way.
    let other = output(
        &compilation("dot { direction: tb, stack: [[cluster-a, cluster-b]], cross: [cluster-b, cluster-a] }", ""),
        "dot",
    );
    assert!(other.contains("  level0_1 -> level0_0 [style=invis];"), "{other}");
    // Without `cross`, the order the level is written in: the same drawing.
    let same = output(&compilation("dot { direction: tb, stack: [[cluster-a, cluster-b]] }", ""), "dot");
    assert!(
        same.contains("remincross=false") && same.contains(&format!("  {datacore01} -> {esx4} [dir=back,")),
        "{same}"
    );
    // The iSCSI flows between the two clusters stay in the layout, with a length of zero: the
    // clusters keep their rank, the flows go straight from one to the other.
    let edge = same.lines().find(|l| l.starts_with(&format!("  {datacore01} -> {esx4} "))).unwrap();
    assert!(edge.contains("minlen=0") && !edge.contains("constraint=false"), "{edge}");
    // Unless a rule sets them free.
    let free = output(
        &compilation(
            "dot { direction: tb, stack: [[cluster-a, cluster-b]] }",
            "edges where proto = iscsi { layout: free }",
        ),
        "dot",
    );
    let edge = free.lines().find(|l| l.starts_with(&format!("  {datacore01} -> {esx4} "))).unwrap();
    assert!(edge.contains("constraint=false") && !edge.contains("minlen=0"), "{edge}");
    // Written the other way, cluster-b first.
    let dot = output(&compilation("dot { direction: tb, stack: [[cluster-b, cluster-a]] }", ""), "dot");
    assert!(dot.contains(&format!("  {esx4} -> {datacore01} [dir=forward,")), "{dot}");
}

/// Two data centers on each of two levels: hosts reaching the storage VMs of both over iSCSI in
/// the first ones, three servers in each of the others.
const SITES: &str = r#"
site dc-a {
  server host-a3 { serial: "A3" }
  server host-a1 { serial: "A1" }
  virtual server vm-a { on: host-a1 }
  server host-a2 { serial: "A2", -> store-a : iscsi [flow], -> store-b : iscsi [flow] }
}
site dc-b {
  server host-b3 { serial: "B3" }
  server host-b1 { serial: "B1" }
  virtual server vm-b { on: host-b1 }
  server host-b2 { serial: "B2", -> store-a : iscsi [flow], -> store-b : iscsi [flow] }
}
app store-a { on: vm-a, -> store-b [flow] }
app store-b { on: vm-b, -> store-a [flow] }
site dc-c { server host-c1 { serial: "C1" }, server host-c2 { serial: "C2" }, server host-c3 { serial: "C3" } }
site dc-d { server host-d1 { serial: "D1" }, server host-d2 { serial: "D2" }, server host-d3 { serial: "D3" } }
"#;

/// The DOT of the storage view of [`SITES`], with these render options.
fn sites(render: &str) -> String {
    let view = format!(
        "view storage {{\n  select server, app where exists(serial) or name in [store-a, store-b] group-by site > on\n  \
         edges [flow] where proto = iscsi or source.name in [store-a, store-b]\n  \
         style {{ edges where source.name in [store-a, store-b] and target.name in [store-a, store-b] {{ layout: free, routing: straight }} }}\n  \
         render dot {{ satellites: false, {render} }}\n}}"
    );
    let c = compile(&[
        SourceFile { path: "a.ca".into(), text: SITES.into() },
        SourceFile { path: "v.cav".into(), text: view },
    ]);
    assert!(errors(&c).is_empty(), "{:?}", errors(&c));
    output(&c, "dot")
}

/// The boxes of the frames of DOT graphs laid out by Graphviz, all in one process (it takes
/// about a second to start on some machines), by their tooltip (`site dc-a`): `[x0, y0, x1, y1]`
/// in points, the origin at the bottom left; `None` when Graphviz is not installed.
fn frame_boxes(dots: &[String]) -> Option<Vec<HashMap<String, [f64; 4]>>> {
    let mut child = Command::new("dot")
        .arg("-Tjson")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    // Written while Graphviz writes its layouts: a full pipe would block both.
    let mut stdin = child.stdin.take()?;
    let input = dots.concat();
    let writer = std::thread::spawn(move || stdin.write_all(input.as_bytes()));
    let output = child.wait_with_output().ok()?;
    writer.join().ok()?.ok()?;
    let layouts: Vec<serde_json::Value> =
        serde_json::Deserializer::from_slice(&output.stdout).into_iter().collect::<Result<_, _>>().unwrap();
    assert_eq!(layouts.len(), dots.len(), "a layout per graph");
    let boxes = layouts.iter().map(|layout| {
        let objects = layout["objects"].as_array().map(Vec::as_slice).unwrap_or_default();
        objects
            .iter()
            .filter_map(|object| {
                let bb: Vec<f64> = object["bb"].as_str()?.split(',').filter_map(|v| v.parse().ok()).collect();
                Some((object["tooltip"].as_str()?.to_owned(), bb.try_into().ok()?))
            })
            .collect()
    });
    Some(boxes.collect())
}

#[test]
fn the_items_of_a_level_of_stack_in_their_order_in_every_direction() {
    // A flat edge between two frames does not order them: Graphviz orders the frames of a rank
    // as blocks. The points before the items of a level are ordered instead, each in a group with
    // the nodes of its item, which follows it.
    let mut cases = Vec::new();
    for direction in ["tb", "bt", "lr", "rl"] {
        // As written; and the other way, by `cross`.
        for (cross, levels) in [
            ("", [["dc-a", "dc-b"], ["dc-c", "dc-d"]]),
            (", cross: [dc-b, dc-a, dc-d, dc-c]", [["dc-b", "dc-a"], ["dc-d", "dc-c"]]),
        ] {
            let dot = sites(&format!("direction: {direction}, stack: [[dc-a, dc-b], [dc-c, dc-d]]{cross}"));
            // In every direction, the points in the order across it: Graphviz puts the tail of a
            // flat edge first, left to right (`tb`, `bt`) or top to bottom (`lr`, `rl`).
            let (first, second) = if cross.is_empty() { ("0", "1") } else { ("1", "0") };
            for level in 0..2 {
                assert!(dot.contains(&format!("  {{ rank=same; level{level}_0; level{level}_1; }}")), "{dot}");
                assert!(
                    dot.contains(&format!("  level{level}_{first} -> level{level}_{second} [style=invis];")),
                    "{direction}{cross}: {dot}"
                );
            }
            // The nodes of a site in the group of its point.
            for host in ["host-c1", "host-c2", "host-c3"] {
                let node = id(&dot, &format!("server dc-c.{host}"));
                assert!(dot.contains(&format!("  {node} [group=\"level1_0\"];")), "{dot}");
            }
            // The level ranked first reaches the points of the other one, not its sites: the
            // points have a rank of their own (`bt`, `rl`: the second level is ranked first).
            let later = if ["tb", "lr"].contains(&direction) { 1 } else { 0 };
            assert!(dot.contains(&format!(" -> level{later}_0 [style=invis, ltail=cluster_")), "{dot}");
            cases.push((direction, cross, levels, dot));
        }
    }
    // Laid out by Graphviz: the sites of each level in that order, those of the second level (no
    // edge between them) side by side, not on either side of the first level.
    let dots: Vec<String> = cases.iter().map(|(.., dot)| dot.clone()).collect();
    let Some(layouts) = frame_boxes(&dots) else {
        eprintln!("Graphviz is not installed: layout not checked");
        return;
    };
    for ((direction, cross, levels, dot), boxes) in cases.iter().zip(layouts) {
        // Where a site starts and ends across the direction, in its order.
        let span = |site: &str| {
            let [x0, y0, x1, y1] = boxes[&format!("site {site}")];
            if ["tb", "bt"].contains(direction) { (x0, x1) } else { (-y1, -y0) }
        };
        for (level, [a, b]) in levels.iter().enumerate() {
            let gap = span(b).0 - span(a).1;
            assert!(gap >= 0.0, "{direction}{cross}: {a} not before {b}: {boxes:?}\n{dot}");
            assert!(level == 0 || gap < 36.0, "{direction}{cross}: {a} and {b} apart: {boxes:?}\n{dot}");
        }
    }
}

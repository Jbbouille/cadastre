//! The expected results of the complete example (spec § 8), checked against
//! the evaluation of `examples/reference`.

use cadastre_core::view::{self, Exchange, Graph};
use cadastre_core::{Compilation, Diagnostic, Severity, SourceFile, compile};

macro_rules! example {
    ($($file:literal),*) => {
        [$(SourceFile {
            path: $file.to_owned(),
            text: include_str!(concat!("../../../../examples/reference/", $file)).to_owned(),
        }),*]
    };
}

fn compilation() -> Compilation {
    let c = compile(&example!("teams.ca", "infra.ca", "data.ca", "apps.ca", "saas.ca", "views.cav"));
    let errors: Vec<_> = c.diagnostics.iter().filter(|d| d.severity == Severity::Error).collect();
    assert!(errors.is_empty(), "{errors:?}");
    c
}

fn evaluate(c: &Compilation, name: &str) -> (Graph, Vec<Diagnostic>) {
    let view = c.plans.views.iter().find(|v| v.name == name).expect("view exists");
    let mut diagnostics = Vec::new();
    let graph = view::evaluate(view, &c.registry, &c.model, &mut diagnostics);
    (graph, diagnostics)
}

/// Edges as `a -> b [ports]`, `a -- b [relation xN]` for symmetric edges.
fn edges(c: &Compilation, graph: &Graph) -> Vec<String> {
    let name = |id| c.registry.resource(id).name.clone();
    let mut out: Vec<String> = graph
        .edges
        .iter()
        .map(|e| {
            let arrow = if e.symmetric { "--" } else { "->" };
            let label = match &e.relation {
                Some(r) if e.provenance.len() > 1 => format!("{r} x{}", e.provenance.len()),
                Some(r) => r.clone(),
                None => e.ports.iter().cloned().collect::<Vec<_>>().join(", "),
            };
            format!("{} {arrow} {} [{label}]", name(e.source), name(e.target))
        })
        .collect();
    out.sort();
    out
}

fn nodes(c: &Compilation, graph: &Graph) -> Vec<String> {
    graph.nodes.iter().map(|&id| c.registry.resource(id).name.clone()).collect()
}

/// `node: frame > frame` for every grouped node.
fn frames(c: &Compilation, graph: &Graph) -> Vec<String> {
    let mut out: Vec<String> = graph
        .node_group
        .iter()
        .map(|(&node, &group)| {
            let mut chain = Vec::new();
            let mut current = Some(group);
            while let Some(g) = current {
                chain.push(c.registry.resource(graph.groups[g].resource.unwrap()).name.clone());
                current = graph.groups[g].parent;
            }
            chain.reverse();
            format!("{}: {}", c.registry.resource(node).name, chain.join(" > "))
        })
        .collect();
    out.sort();
    out
}

fn warnings(diagnostics: &[Diagnostic]) -> Vec<String> {
    diagnostics.iter().filter(|d| d.severity == Severity::Warning).map(|d| d.message.clone()).collect()
}

#[test]
fn flows() {
    let c = compilation();
    let (g, _) = evaluate(&c, "flows");
    assert_eq!(
        edges(&c, &g),
        [
            "billing -> analytics [tcp/5432]",
            "billing -> crm [https/443]",
            "billing -> legacy-erp [tcp/1521]",
            "billing -> stripe [https/443]",
            "crm -> crmdb [tcp/5432]",
        ]
    );
}

#[test]
fn network() {
    let c = compilation();
    let (g, d) = evaluate(&c, "network");
    assert_eq!(
        edges(&c, &g),
        [
            "web01 -> db01 [tcp/5432]",
            "web01 -> erp01 [tcp/1521]",
            "web01 -> stripe [https/443]",
            "web02 -> erp01 [tcp/1521]",
            "web02 -> stripe [https/443]",
            "web02 -> web01 [https/443]",
        ]
    );
    assert_eq!(
        frames(&c, &g),
        ["db01: aws-paris > db", "erp01: paris-dc1 > erp", "web01: aws-paris > dmz", "web02: aws-paris > dmz",]
    );
    let w = warnings(&d);
    assert_eq!(w.len(), 1, "{w:?}");
    assert!(w[0].contains("analytics"), "{w:?}");
}

#[test]
fn macro_view() {
    let c = compilation();
    let (g, d) = evaluate(&c, "macro");
    assert_eq!(edges(&c, &g), ["dmz -> db [tcp/5432]", "dmz -> erp [tcp/1521]"]);
    let to_db = g.edges.iter().find(|e| c.registry.resource(e.target).name == "db").unwrap();
    assert_eq!(to_db.provenance.len(), 2, "{:?}", to_db.provenance);
    let w = warnings(&d);
    assert_eq!(w.len(), 1, "{w:?}");
    assert!(w[0].contains("stripe"), "{w:?}");
}

#[test]
fn landscape() {
    let c = compilation();
    let (g, _) = evaluate(&c, "landscape");
    assert_eq!(edges(&c, &g), ["invoicing -> accounting [tcp/1521]", "invoicing -> stripe [https/443]"]);
    assert!(g.edges.iter().all(|e| e.exchange == Some(Exchange::Push)));
}

#[test]
fn to_verify_and_to_complete() {
    let c = compilation();
    let (g, _) = evaluate(&c, "to-verify");
    assert_eq!(nodes(&c, &g), ["analytics"]);
    assert!(g.edges.is_empty());
    let (g, _) = evaluate(&c, "to-complete");
    assert!(g.nodes.is_empty(), "{:?}", nodes(&c, &g));
}

#[test]
fn ownership() {
    let c = compilation();
    let (g, _) = evaluate(&c, "ownership");
    assert_eq!(
        frames(&c, &g),
        [
            "analytics: invoicing-team",
            "billing: invoicing-team",
            "crm: invoicing-team",
            "crmdb: invoicing-team",
            "legacy-erp: accounting-team",
            "stripe: invoicing-team",
        ]
    );
}

#[test]
fn inter_sites() {
    let c = compilation();
    let (g, _) = evaluate(&c, "inter-sites");
    assert_eq!(
        edges(&c, &g),
        [
            "aws-paris -- paris-dc1 [vpn_tunnel x2]",
            "aws-paris -> paris-dc1 [tcp/1521]",
            "aws-paris -> stripe [https/443]",
        ]
    );
}

#[test]
fn gdpr() {
    let c = compilation();
    let (g, _) = evaluate(&c, "gdpr");
    assert_eq!(edges(&c, &g), ["billing -> crm [https/443]", "crm -> crmdb [tcp/5432]"]);
    // `without isolated`: stripe, analytics and legacy-erp carry no personal data.
    assert_eq!(nodes(&c, &g), ["billing", "crm", "crmdb"]);
}

#[test]
fn with_ends_adds_the_ends_of_uncertain_flows() {
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
    let (g, _) = evaluate(&c, "to-verify");
    let e = edges(&c, &g);
    assert!(e.contains(&"api -> logistics [http]".to_owned()), "{e:?}");
    assert!(e.contains(&"central-scheduler -> ui []".to_owned()), "{e:?}");
    // The sure end of an assumed flow is added as a node.
    assert!(nodes(&c, &g).contains(&"api".to_owned()), "{:?}", nodes(&c, &g));
}

#[test]
fn with_ends_cannot_add_a_grouping_kind() {
    let c = compile(&[
        SourceFile {
            path: "a.ca".into(),
            text: "team t
site s { hosting: cloud, owner: t, vpc v { subnet n { cidr: 10.0.0.0/8, server x } } }
app a { owner: t, on: x, -> b : https [flow] { confidence: assumed } }
app b { owner: t, on: n }"
                .into(),
        },
        SourceFile {
            path: "v.cav".into(),
            text: "view v { select server where confidence = confirmed with ends group-by app edges on render text }"
                .into(),
        },
    ]);
    let (g, d) = evaluate(&c, "v");
    assert!(g.edges.is_empty(), "{:?}", edges(&c, &g));
    let w = warnings(&d);
    assert_eq!(w.len(), 2, "{w:?}");
    assert!(w[0].contains("is of kind app, which frames the view"), "{w:?}");
}

#[test]
fn fw01_matrix() {
    let c = compilation();
    let (g, _) = evaluate(&c, "fw01-matrix");
    assert_eq!(edges(&c, &g), ["web01 -> erp01 [tcp/1521]", "web02 -> erp01 [tcp/1521]"]);
}

#[test]
fn security() {
    let c = compilation();
    let (g, d) = evaluate(&c, "security");
    assert_eq!(
        edges(&c, &g),
        [
            "fw01 -> erp01 [tcp/1521]",
            "web01 -> db01 [tcp/5432]",
            "web01 -> fw01 [tcp/1521]",
            "web02 -> fw01 [tcp/1521]",
            "web02 -> web01 [https/443]",
        ]
    );
    let to_fw = g.edges.iter().find(|e| c.registry.resource(e.target).name == "fw01").unwrap();
    let via: Vec<&str> = to_fw.via.iter().map(|&v| c.registry.resource(v).name.as_str()).collect();
    assert_eq!(via, ["paris-aws-1"]);
    assert!(frames(&c, &g).contains(&"fw01: paris-dc1".to_owned()), "{:?}", frames(&c, &g));
    assert_eq!(warnings(&d).len(), 2, "{:?}", warnings(&d));
}

#[test]
fn physical() {
    let c = compilation();
    let (g, _) = evaluate(&c, "physical");
    assert_eq!(edges(&c, &g), ["fw01 -- sw01 [peer]", "sw01 -- erp01 [peer]"]);
    assert_eq!(
        frames(&c, &g),
        [
            "db01: aws-paris",
            "erp01: paris-dc1 > r12",
            "fw01: paris-dc1 > r12",
            "sw01: paris-dc1 > r12",
            "web01: aws-paris",
            "web02: aws-paris",
        ]
    );
}

#[test]
fn deployment() {
    let c = compilation();
    let (g, _) = evaluate(&c, "deployment");
    assert_eq!(
        edges(&c, &g),
        [
            "billing -> analytics [tcp/5432]",
            "billing -> crm [https/443]",
            "billing -> legacy-erp [tcp/1521]",
            "billing -> web01 [on]",
            "billing -> web02 [on]",
            "crm -> crmdb [tcp/5432]",
            "crm -> web01 [on]",
            "crmdb -> db01 [on]",
            "legacy-erp -> erp01 [on]",
        ]
    );
}

#[test]
fn renderings() {
    let c = compilation();
    let (outputs, _) = cadastre_core::build(&c, Some("network"));
    assert_eq!(outputs.len(), 1);
    let dot = &outputs[0].content;
    assert!(dot.starts_with("digraph \"network\""));
    // Frames and nodes carry the icon of their kind: an HTML-like label with a square for it.
    assert!(dot.contains("subgraph cluster_0") && dot.contains("<td>dmz (10.0.1.0/24)</td>"), "{dot}");
    assert!(dot.contains("class=\"kind_subnet\"") && dot.contains("class=\"kind_server\""), "{dot}");

    let (outputs, _) = cadastre_core::build(&c, Some("fw01-matrix"));
    assert_eq!(outputs[0].format, "text");
    assert!(outputs[0].content.contains("through: paris-aws-1, fw01"), "{}", outputs[0].content);

    let (outputs, _) = cadastre_core::build(&c, Some("inter-sites"));
    let dot = &outputs[0].content;
    assert!(dot.contains("label=\"vpn_tunnel ×2\""), "{dot}");
    assert!(dot.contains("dir=none"), "{dot}");
}

#[test]
fn customer_trail() {
    let c = compilation();
    let (g, d) = evaluate(&c, "customer-trail");
    assert!(d.is_empty(), "{d:?}");
    assert_eq!(edges(&c, &g), ["billing -> crm [https/443]", "crm -> crmdb [tcp/5432]"]);
    let step =
        |target: &str| g.edges.iter().find(|e| c.registry.resource(e.target).name == target).and_then(|e| e.step);
    assert_eq!((step("crmdb"), step("crm")), (Some(1), Some(2)));
    assert_eq!(nodes(&c, &g), ["billing", "crm", "crmdb"]);
}

#[test]
fn node_labels_apply_to_nodes_and_frames() {
    let c = compile(&[
        SourceFile {
            path: "a.ca".into(),
            text: "system sales { description: \"Pricing\", app ihm { description: \"Pricing UI\" } }".into(),
        },
        SourceFile {
            path: "v.cav".into(),
            text: "view v {\n  select app group-by system\n  render dot, text, mermaid { node_labels: [label, description] }\n}".into(),
        },
    ]);
    let (outputs, _) = cadastre_core::build(&c, None);
    let content = |format: &str| outputs.iter().find(|o| o.format == format).unwrap().content.clone();
    assert!(content("dot").contains("<td>sales<br/>Pricing</td>"), "{}", content("dot"));
    assert!(content("dot").contains("<td>ihm<br/>Pricing UI</td>"), "{}", content("dot"));
    assert!(content("text").contains("sales / Pricing"), "{}", content("text"));
    assert!(content("mermaid").contains("[\"sales<br/>Pricing\"]"), "{}", content("mermaid"));
}

#[test]
fn dot_edge_labels_are_nodes_on_the_edge() {
    let c = compile(&[
        SourceFile {
            path: "a.ca".into(),
            text: "system s { app a { -> db : jdbc [flow] } database db }\napp b { -> a : https/443 [flow] }".into(),
        },
        SourceFile {
            path: "v.cav".into(),
            text: "view v {\n  select app, database group-by system\n  edges [flow]\n  render dot { icons: false }\n}"
                .into(),
        },
    ]);
    let (outputs, _) = cadastre_core::build(&c, None);
    let dot = &outputs[0].content;
    let lines: Vec<&str> = dot.lines().map(str::trim).collect();
    let node = |name: &str| {
        lines.iter().find(|l| l.contains(&format!("label=\"{name}\""))).unwrap().split(' ').next().unwrap()
    };
    let (a, db, b) = (node("a"), node("db"), node("b"));
    // Each labeled edge goes through its text node: the arrowhead is on the second segment.
    let text = |label: &str| {
        *lines.iter().find(|l| l.contains(&format!("label=\"{label}\"")) && l.contains("shape=none")).unwrap()
    };
    let (jdbc, https) = (text("jdbc").split(' ').next().unwrap(), text("https/443").split(' ').next().unwrap());
    assert!(lines.iter().any(|l| l.starts_with(&format!("{a} -> {jdbc} [dir=none"))), "{dot}");
    assert!(lines.iter().any(|l| l.starts_with(&format!("{jdbc} -> {db} [dir=forward"))), "{dot}");
    assert!(lines.iter().any(|l| l.starts_with(&format!("{b} -> {https} [dir=none"))), "{dot}");
    // The text of an edge inside the system lies in its frame; the other one at the root.
    let in_cluster = |id: &str| {
        let start = dot.find("subgraph cluster_").unwrap();
        let end = start + dot[start..].find("\n  }").unwrap();
        dot[start..end].contains(&format!("\n    {id} ["))
    };
    assert!(in_cluster(jdbc) && !in_cluster(https), "{dot}");
}

#[test]
fn mermaid_merges_parallel_links_and_asks_for_elk() {
    let c = compile(&[
        SourceFile {
            path: "a.ca".into(),
            text: "app shop { -> hub : sftp/22 [flow], -> hub : azcopy [flow], -> api : https [flow] }\napp hub\napp api".into(),
        },
        SourceFile {
            path: "v.cav".into(),
            text: "style { edges where proto = https { color: \"#1d4ed8\" } }\nview v {\n  select app\n  edges [flow]\n  render mermaid\n}"
                .into(),
        },
    ]);
    let (outputs, _) = cadastre_core::build(&c, None);
    let mermaid = &outputs[0].content;
    assert!(mermaid.starts_with("---\nconfig:\n  layout: elk\n---\nflowchart LR\n"), "{mermaid}");
    let links: Vec<&str> = mermaid.lines().filter(|l| l.contains("-->")).collect();
    assert_eq!(links.len(), 2, "{mermaid}");
    assert!(
        links.iter().any(|l| l.contains("|\"sftp/22, azcopy\"|") || l.contains("|\"azcopy, sftp/22\"|")),
        "{mermaid}"
    );
    // `linkStyle` counts the links as written.
    let https = links.iter().position(|l| l.contains("https")).unwrap();
    assert!(mermaid.contains(&format!("linkStyle {https} stroke:#1d4ed8")), "{mermaid}");
}

#[test]
fn group_by_follows_on() {
    let c = compile(&[
        SourceFile {
            path: "a.ca".into(),
            text: "cluster vc
site p { hosting: on-prem, server esx1 { cluster: vc }, server web1 }
site q { hosting: on-prem, server esx2 { cluster: vc }, server web2 }
app vm { on: esx1 }
app spread { on: [web1, web2] }
app pair { on: [esx1, esx2] }"
                .into(),
        },
        SourceFile {
            path: "v.cav".into(),
            text: "view by_cluster {\n  select app group-by cluster\n}\nview by_site {\n  select app group-by site\n}"
                .into(),
        },
    ]);
    let (g, d) = evaluate(&c, "by_cluster");
    assert!(d.is_empty(), "{d:?}");
    // vm runs on a server of vc; pair runs on two servers of vc; spread runs on servers without a cluster.
    assert_eq!(frames(&c, &g), ["pair: vc", "vm: vc"]);
    let (g, d) = evaluate(&c, "by_site");
    assert!(d.is_empty(), "{d:?}");
    // spread and pair run on servers of two sites: no frame, no error.
    assert_eq!(frames(&c, &g), ["vm: p"]);
}

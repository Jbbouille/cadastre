//! Impact analysis: `failures`, the resources whose failure alone makes a node unavailable.

use cadastre_core::{Compilation, Severity, SourceFile, compile};

/// The plan file a text goes in: a `.cac` for checks, a `.cav` for views.
fn plan_path(text: &str) -> &'static str {
    if text.trim_start().starts_with("check") { "c.cac" } else { "v.cav" }
}

const REGISTRY: &str = "team t
site dc {
  hosting: on-prem
  rack r1
  firewall fw01 { rack: r1 }
  network n { subnet a { cidr: 10.0.0.0/24, server s1 { rack: r1, cluster: k }, server s2 { cluster: k }, server s3 { } } }
}
cluster k
system shop {
  owner: t
  app front { on: [s1, s2], -> back : https/443 [flow] { through: [fw01] } }
  app back { on: k, -> store : tcp/5432 [flow] }
  database store { on: s3 }
  app lonely { on: s1 }
}
";

fn compilation(plans: &str) -> Compilation {
    compile(&[
        SourceFile { path: "a.ca".into(), text: REGISTRY.into() },
        SourceFile { path: plan_path(plans).into(), text: plans.into() },
    ])
}

fn failures(name: &str) -> Vec<String> {
    let c = compilation("");
    let id = c.registry.ids().find(|&i| c.registry.qualified_name(i) == name).unwrap();
    c.model.failures_of(&c.registry, id).iter().map(|&f| c.registry.qualified_name(f)).collect()
}

#[test]
fn what_takes_a_resource_down() {
    // Two hosts: neither server alone; the firewall, its rack, the dependencies of its flow and
    // what they run on (the cluster `k` as a whole).
    assert_eq!(
        failures("shop.front"),
        ["dc", "dc.r1", "dc.fw01", "dc.n", "dc.n.a", "dc.n.a.s3", "k", "shop.back", "shop.store"]
    );
    // A cluster survives one of its members, not its own failure.
    assert_eq!(failures("shop.back"), ["dc", "dc.n", "dc.n.a", "dc.n.a.s3", "k", "shop.store"]);
    // A single host, and its rack.
    assert_eq!(failures("shop.lonely"), ["dc", "dc.r1", "dc.n", "dc.n.a", "dc.n.a.s1"]);
}

#[test]
fn the_impact_of_a_failure_in_a_view() {
    let c = compilation("view fw01 { select app, database where failures has fw01 }");
    assert!(c.diagnostics.iter().all(|d| d.severity != Severity::Error), "{:?}", c.diagnostics);
    let graph = cadastre_core::view::evaluate(&c.plans.views[0], &c.registry, &c.model, &mut Vec::new());
    let names: Vec<&str> = graph.nodes.iter().map(|&id| c.registry.resource(id).name.as_str()).collect();
    assert_eq!(names, ["front"]);
}

#[test]
fn a_single_point_of_failure_check() {
    let c = compilation(
        "check no-server-alone {
           select app where failures.kind = server
           expect none
           severity: warning
         }",
    );
    let failure = c.diagnostics.iter().find(|d| d.message.starts_with("check ")).unwrap();
    assert_eq!(
        failure.message,
        "check no-server-alone failed: 3 nodes selected, none expected: shop.front, shop.back, shop.lonely"
    );
}

#[test]
fn failures_through_hosting_only() {
    let c = compilation("view v { select app where failures(hosting).kind = server }");
    assert!(c.diagnostics.iter().all(|d| d.severity != Severity::Error), "{:?}", c.diagnostics);
    let hosting = |name: &str| -> Vec<String> {
        let id = c.registry.ids().find(|&i| c.registry.qualified_name(i) == name).unwrap();
        c.model.hosting_failures_of(&c.registry, id).iter().map(|&f| c.registry.qualified_name(f)).collect()
    };
    // Two hosts, and neither the firewall of its flow nor what it calls.
    assert_eq!(hosting("shop.front"), ["dc", "dc.n", "dc.n.a"]);
    assert_eq!(hosting("shop.back"), ["dc", "dc.n", "dc.n.a", "k"]);
    // `back` depends on `store`, which runs on `s3` alone: a failure, not of its hosting.
    let graph = cadastre_core::view::evaluate(&c.plans.views[0], &c.registry, &c.model, &mut Vec::new());
    let names: Vec<&str> = graph.nodes.iter().map(|&id| c.registry.resource(id).name.as_str()).collect();
    assert_eq!(names, ["lonely"]);
}

/// The names of the nodes of the first view, and of those `down` among them.
fn scenario(plans: &str) -> (Compilation, Vec<String>, Vec<String>) {
    let c = compilation(plans);
    let graph = cadastre_core::view::evaluate(&c.plans.views[0], &c.registry, &c.model, &mut Vec::new());
    let name = |id| c.registry.qualified_name(id);
    let mut nodes: Vec<String> = graph.nodes.iter().map(|&id| name(id)).collect();
    let mut down: Vec<String> = graph.nodes.iter().filter(|id| graph.down.contains(id)).map(|&id| name(id)).collect();
    nodes.sort();
    down.sort();
    (c, nodes, down)
}

#[test]
fn several_failures_together() {
    // One server: what runs on it alone; the cluster survives on the other member.
    let (c, _, down) = scenario("view v { failing: [s1]\n select app, database where down }");
    assert!(c.diagnostics.iter().all(|d| d.severity != Severity::Error), "{:?}", c.diagnostics);
    assert_eq!(down, ["shop.lonely"]);
    // Both servers: the cluster, the app on both, what calls them.
    let (_, nodes, down) = scenario("view v { failing: [s1, s2]\n select app, database }");
    assert_eq!(nodes, ["shop.back", "shop.front", "shop.lonely", "shop.store"]);
    assert_eq!(down, ["shop.back", "shop.front", "shop.lonely"]);
    // A system fails as what it contains.
    let (_, _, down) = scenario("view v { failing: [shop]\n select app, database }");
    assert_eq!(down, ["shop.back", "shop.front", "shop.lonely", "shop.store"]);
}

#[test]
fn a_check_on_a_scenario() {
    let c = compilation("check survives-s1 { failing: [s1]\n select app where down\n expect none }");
    let failures: Vec<String> = cadastre_core::rules::run(&c).iter().map(|d| d.message.clone()).collect();
    assert_eq!(failures, ["check survives-s1 failed: 1 node selected, none expected: shop.lonely"]);
    let c = compilation("check survives-s3 { failing: [s1]\n select app\n expect all not down }");
    assert_eq!(cadastre_core::rules::run(&c).len(), 1);
}

#[test]
fn what_a_scenario_cannot_fail() {
    let c = compilation("view v { failing: [t, nowhere, \"x\"]\n select app }");
    let messages: Vec<&str> = c.diagnostics.iter().map(|d| d.message.as_str()).collect();
    assert!(messages.contains(&"t is a team and contains nothing that fails"), "{messages:?}");
    assert!(messages.contains(&"dangling reference: nowhere"), "{messages:?}");
    assert!(messages.contains(&"failing lists names of resources, not \"x\""), "{messages:?}");
}

#[test]
fn a_scenario_drawn_in_red_with_its_legend() {
    let c = compilation("view v { failing: [s3]\n select app, database, server\n render svg }");
    let graph = cadastre_core::view::evaluate(&c.plans.views[0], &c.registry, &c.model, &mut Vec::new());
    let scene = cadastre_core::render::Scene::new(&graph, &c.plans, &c.registry, &c.model);
    let dot = cadastre_core::render::dot(&scene);
    assert!(dot.contains("#fca5a5") && dot.contains("#fee2e2"), "{dot}");
    let legend: Vec<String> = scene.legend().into_iter().map(|e| e.label).collect();
    assert!(legend.contains(&"Failing".to_owned()) && legend.contains(&"Unavailable".to_owned()), "{legend:?}");
}

fn pairs(name: &str) -> Vec<String> {
    let c = compilation("");
    let id = c.registry.ids().find(|&i| c.registry.qualified_name(i) == name).unwrap();
    let name = |id| c.registry.qualified_name(id);
    c.model.failure_pairs_of(&c.registry, id).iter().map(|&(a, b)| format!("{} + {}", name(a), name(b))).collect()
}

#[test]
fn the_pairs_of_failures_a_redundancy_rests_on() {
    // On both servers: their pair; the rack of s1 takes the firewall down, so front alone.
    assert_eq!(pairs("shop.front"), ["dc.n.a.s1 + dc.n.a.s2"]);
    // A cluster of s1 and s2: their pair; the rack of s1 with s2 too, left out, the rack taking
    // s1 down.
    assert_eq!(pairs("shop.back"), ["dc.n.a.s1 + dc.n.a.s2"]);
    // A single host: no pair, its failure alone is enough.
    assert!(pairs("shop.lonely").is_empty());
    // In a filter: the resources of the pairs.
    let c = compilation("view v { select app where failures(pairs) has s2 }");
    assert!(c.diagnostics.iter().all(|d| d.severity != Severity::Error), "{:?}", c.diagnostics);
    let graph = cadastre_core::view::evaluate(&c.plans.views[0], &c.registry, &c.model, &mut Vec::new());
    let mut names: Vec<String> = graph.nodes.iter().map(|&id| c.registry.qualified_name(id)).collect();
    names.sort();
    assert_eq!(names, ["shop.back", "shop.front"]);
}

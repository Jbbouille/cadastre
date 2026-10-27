//! Aggregates in filters: `count(path)`, the number of distinct items a path gives.

use cadastre_core::{Compilation, Severity, SourceFile, compile};

/// The plan file a text goes in: a `.cac` for checks, a `.cav` for views.
fn plan_path(text: &str) -> &'static str {
    if text.trim_start().starts_with("check") { "c.cac" } else { "v.cav" }
}

const REGISTRY: &str = "team t
site paris { hosting: on-prem, network n { subnet a { cidr: 10.0.1.0/24, server p1 { }, server p2 { } } } }
site lyon { hosting: on-prem, network n { subnet a { cidr: 10.0.2.0/24, server l1 { } } } }
system shop {
  owner: t
  criticality: high
  app front { on: [p1, l1] }
  app back { on: [p1, p2] }
  app solo { on: p1 }
}
";

fn compilation(plans: &str) -> Compilation {
    compile(&[
        SourceFile { path: "a.ca".into(), text: REGISTRY.into() },
        SourceFile { path: plan_path(plans).into(), text: plans.into() },
    ])
}

fn nodes(plans: &str) -> Vec<String> {
    let c = compilation(plans);
    let errors: Vec<_> = c.diagnostics.iter().filter(|d| d.severity == Severity::Error).collect();
    assert!(errors.is_empty(), "{errors:?}");
    let graph = cadastre_core::view::evaluate(&c.plans.views[0], &c.registry, &c.model, &mut Vec::new());
    graph.nodes.iter().map(|&id| c.registry.resource(id).name.clone()).collect()
}

#[test]
fn counting_items() {
    assert_eq!(nodes("view v { select app where count(on) >= 2 }"), ["front", "back"]);
    assert_eq!(nodes("view v { select app where count(on) < 2 }"), ["solo"]);
    // Distinct items: `back` runs on two servers of one site.
    assert_eq!(nodes("view v { select app where count(on.ancestor(site)) >= 2 }"), ["front"]);
    assert_eq!(nodes("view v { select app where count(on.ancestor(site)) = 1 }"), ["back", "solo"]);
}

#[test]
fn a_resilience_check() {
    let c = compilation(
        "check redundant-in-two-sites {
           select app where count(on.ancestor(site)) < 2
           expect none
           message: \"a critical app runs in two sites\"
         }",
    );
    let failure = c.diagnostics.iter().find(|d| d.message.starts_with("check ")).unwrap();
    assert_eq!(
        failure.message,
        "check redundant-in-two-sites failed: 2 nodes selected, none expected: shop.back, shop.solo"
    );
}

#[test]
fn invalid_aggregates() {
    let c = compilation(
        "view a { select app where sum(on) > 1 }
         view b { select app where count(on) > many }",
    );
    let messages: Vec<&str> = c.diagnostics.iter().map(|d| d.message.as_str()).collect();
    assert!(messages.contains(&"unknown function: sum"), "{messages:?}");
    assert!(messages.contains(&"count compares a count to many, not a number"), "{messages:?}");
}

//! Comparisons between two paths of the ends of an edge.

use cadastre_core::{Compilation, SourceFile, compile};

/// The plan file a text goes in: a `.cac` for checks, a `.cav` for views.
fn plan_path(text: &str) -> &'static str {
    if text.trim_start().starts_with("check") { "c.cac" } else { "v.cav" }
}

const REGISTRY: &str = "team t
team u
site paris { hosting: on-prem, network n { subnet lan { cidr: 10.0.2.0/24, server s1 { }, server s2 { } } } }
system shop {
  owner: t
  app front { on: s1, version: 2.1.0, -> store : tcp/5432 [flow], -> billing.api : https/443 [flow] }
  database store { on: s2 }
}
system billing {
  owner: u
  app api { on: s2, version: 1.4.0, -> store : tcp/5432 [flow] }
  database store { on: s2 }
}
app loose { on: s1, -> shop.store : tcp/5432 [flow] }
";

fn compilation(plans: &str) -> Compilation {
    compile(&[
        SourceFile { path: "a.ca".into(), text: REGISTRY.into() },
        SourceFile { path: plan_path(plans).into(), text: plans.into() },
    ])
}

/// The edges of a view, as `source -> target`.
fn edges(plans: &str) -> Vec<String> {
    let c = compilation(plans);
    let errors: Vec<_> = c.diagnostics.iter().filter(|d| d.severity == cadastre_core::Severity::Error).collect();
    assert!(errors.is_empty(), "{errors:?}");
    let view = &c.plans.views[0];
    let graph = cadastre_core::view::evaluate(view, &c.registry, &c.model, &mut Vec::new());
    let mut out: Vec<String> = graph
        .edges
        .iter()
        .map(|e| format!("{} -> {}", c.registry.qualified_name(e.source), c.registry.qualified_name(e.target)))
        .collect();
    out.sort();
    out
}

#[test]
fn different_systems() {
    // `loose` belongs to no system: a side without value is never compared.
    assert_eq!(
        edges("view v { select app, database edges [flow] where source.ancestor(system) != target.ancestor(system) }"),
        ["shop.front -> billing.api"]
    );
    assert_eq!(
        edges("view v { select app, database edges [flow] where source.ancestor(system) = target.ancestor(system) }"),
        ["billing.api -> billing.store", "shop.front -> shop.store"]
    );
}

#[test]
fn qualified_names_and_optional_steps() {
    // `target.owner` is written like a qualified name; `on?.ancestor(site)` keeps the end without `on`.
    assert_eq!(
        edges("view v { select app, database edges [flow] where source.owner != target.owner }"),
        ["shop.front -> billing.api"]
    );
    assert_eq!(
        edges("view v { select app, database edges [flow] where source.on = target.on }"),
        ["billing.api -> billing.store"]
    );
    assert_eq!(
        edges(
            "view v { select app, database edges [flow] where source.on?.ancestor(site) = target.on?.ancestor(site) and target.kind = app }"
        ),
        ["shop.front -> billing.api"]
    );
}

#[test]
fn order_between_versions() {
    assert_eq!(
        edges("view v { select app edges [flow] where source.version > target.version }"),
        ["shop.front -> billing.api"]
    );
}

#[test]
fn checks_with_two_paths() {
    let c = compilation(
        "check no-direct-db-access {
           edges [flow] where target.kind = database and source.ancestor(system) != target.ancestor(system)
           expect none
         }
         check same-owner {
           select app, database
           edges [flow]
           expect all source.owner = target.owner
         }",
    );
    let failures: Vec<&str> =
        c.diagnostics.iter().map(|d| d.message.as_str()).filter(|m| m.starts_with("check ")).collect();
    // `loose` has no owner: the comparison is false, so `expect all` reports it.
    assert_eq!(
        failures,
        [
            "check same-owner failed: 2 of 4 edges do not meet the expectation: shop.front -> billing.api, loose -> shop.store"
        ]
    );
}

#[test]
fn invalid_path_comparisons() {
    let c = compilation(
        "view a { select app where on = target.on }
         view b { select app edges [flow] where source.ancestor(system) = on.ancestor(site) }
         view c { select app edges [flow] where source.owner = target.ownr }",
    );
    let messages: Vec<&str> = c.diagnostics.iter().map(|d| d.message.as_str()).collect();
    for expected in [
        "two paths are compared on an edge only",
        "a path on the right of a comparison starts with source or target",
        "unknown key: ownr",
    ] {
        assert!(messages.contains(&expected), "{expected} not in {messages:?}");
    }
}

#[test]
fn values_stay_values() {
    // A name that does not start with an end is a value, qualified or not.
    let c = compilation("view v { select app where on = paris.n.lan.s1 or owner = t }");
    assert!(c.diagnostics.iter().all(|d| d.severity != cadastre_core::Severity::Error), "{:?}", c.diagnostics);
}

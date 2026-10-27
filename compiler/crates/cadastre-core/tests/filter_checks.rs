//! What filters are checked against: order comparisons, keys of the filtered kinds, and the
//! `incomplete` pseudo-attribute on edges.

use cadastre_core::{Compilation, Severity, SourceFile, compile};

const REGISTRY: &str = "team t
site paris { hosting: on-prem, network n { subnet lan { cidr: 10.0.2.0/24, server s1 { os: debian12 } } } }
system shop {
  owner: t
  app front { on: s1, version: 2.1.0, criticality: 3, -> store : tcp/5432 [flow], -> pending [flow] }
  database store { on: s1 }
}
app pending { }
";

fn compilation(plans: &str) -> Compilation {
    compile(&[
        SourceFile { path: "a.ca".into(), text: REGISTRY.into() },
        SourceFile { path: "v.cav".into(), text: plans.into() },
    ])
}

fn errors(plans: &str) -> Vec<String> {
    compilation(plans).diagnostics.into_iter().filter(|d| d.severity == Severity::Error).map(|d| d.message).collect()
}

#[test]
fn order_comparisons_need_numbers_or_versions() {
    // Versions, numbers on a free key (`criticality`, `os`), ports and steps are ordered.
    assert!(
        errors(
            "view a { select app where version >= 2.0.0 and criticality > 2 }
             view c { select server where os > 3 }
             view b { select app edges [flow] where port < 1024 }"
        )
        .is_empty()
    );
    let messages = errors(
        "view a { select site where hosting > 3 }
         view b { select app where on > 2 }
         view c { select app where version > latest }
         view d { select app where ancestor(system) >= 1 }
         view e { select app where kind < 2 }",
    );
    assert_eq!(
        messages,
        [
            "hosting cannot be compared in order: `hosting`, a word of a closed list",
            "on cannot be compared in order: `on`, a reference",
            "latest is neither a number nor a version: it cannot be compared in order",
            "ancestor(system) cannot be compared in order: a resource",
            "kind cannot be compared in order: `kind`, which is not a number",
        ]
    );
}

#[test]
fn keys_of_the_filtered_kinds() {
    let messages = errors("view a { select database where cidr = 10.0.2.0/24 }");
    assert_eq!(messages, ["cidr is not a key of database"]);
    // A key written on a resource of the kind, a key of the schema of one of the kinds, a common key.
    assert!(
        errors("view a { select app, database where criticality = 3 or stores has x or exists(label) }").is_empty()
    );
    // A style rule on a kind is checked the same way.
    assert_eq!(errors("style { kind database where cidr = 1 { color: red } }"), ["cidr is not a key of database"]);
}

#[test]
fn incomplete_edges() {
    // `pending` has no `on`: incomplete, and so is the edge towards it.
    let c = compilation("view v { select app, database edges [flow] where incomplete }");
    let view = &c.plans.views[0];
    let graph = cadastre_core::view::evaluate(view, &c.registry, &c.model, &mut Vec::new());
    let edges: Vec<String> = graph
        .edges
        .iter()
        .map(|e| format!("{} -> {}", c.registry.qualified_name(e.source), c.registry.qualified_name(e.target)))
        .collect();
    assert_eq!(edges, ["shop.front -> pending"]);
}

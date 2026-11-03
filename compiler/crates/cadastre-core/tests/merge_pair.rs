//! `merge: pair`: one edge per pair of nodes, whatever the direction of its links.

use cadastre_core::{Compilation, Severity, SourceFile, compile, view};

const REGISTRY: &str = r#"
site dc {
  hosting: on-prem
  server backup
  server proxy
  server vcenter
}
app jobs { on: backup, -> agent : tcp/9392 [flow], -> agent : tcp/2500 [flow] }
app agent { on: proxy, -> jobs : tcp/10006 [flow] }
app inventory { on: vcenter, -> jobs : https/443 [flow] }
"#;

fn compilation(merge: &str) -> Compilation {
    let views =
        format!("view v {{\n  select server\n  edges lift([flow], via: on, merge: {merge})\n  render text\n}}\n");
    let c = compile(&[
        SourceFile { path: "a.ca".into(), text: REGISTRY.into() },
        SourceFile { path: "v.cav".into(), text: views },
    ]);
    let errors: Vec<_> = c.diagnostics.iter().filter(|d| d.severity == Severity::Error).collect();
    assert!(errors.is_empty(), "{errors:?}");
    c
}

/// The edges of the view: `source -> target`, and whether they go both ways.
fn edges(c: &Compilation) -> Vec<(String, String, bool)> {
    let graph = view::evaluate(&c.plans.views[0], &c.registry, &c.model, &mut Vec::new());
    let name = |id| c.registry.resource(id).name.clone();
    let mut edges: Vec<_> = graph.edges.iter().map(|e| (name(e.source), name(e.target), e.both_ways)).collect();
    edges.sort();
    edges
}

#[test]
fn one_edge_per_pair() {
    // `endpoints`: backup → proxy and proxy → backup are two edges.
    assert_eq!(
        edges(&compilation("endpoints")),
        [
            ("backup".into(), "proxy".into(), false),
            ("proxy".into(), "backup".into(), false),
            ("vcenter".into(), "backup".into(), false)
        ]
    );
    // `pair`: one, both ways, laid out from backup (two links that way, one the other); the edge
    // from vcenter keeps its single head.
    let c = compilation("pair");
    assert_eq!(edges(&c), [("backup".into(), "proxy".into(), true), ("vcenter".into(), "backup".into(), false)]);
    let text = cadastre_core::build(&c, None).0.into_iter().find(|o| o.format == "text").unwrap().content;
    assert!(text.contains("<->"), "{text}");
}

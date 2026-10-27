//! A dense diagram draws no label on its edges unless the view writes `edge_labels`.

use cadastre_core::{Compilation, Severity, SourceFile, build, compile};

/// One app calling 21 SaaS: more than the 20 edges a diagram labels by default.
const REGISTRY: &str = r#"
team t
site dc { hosting: on-prem, network lan { subnet s { cidr: 10.0.0.0/24, server srv { owner: t } } } }
for_each n in [a, b, c, d, e, f, g, h, i, j, k, l, m, o, p, q, r, u, v, w, x] {
  saas svc-$n { owner: t }
}
app hub {
  owner: t
  on: srv
  for_each n in [a, b, c, d, e, f, g, h, i, j, k, l, m, o, p, q, r, u, v, w, x] {
    -> svc-$n : https/443 [flow]
  }
}
"#;

fn outputs(views: &str) -> Vec<cadastre_core::Output> {
    let c: Compilation = compile(&[
        SourceFile { path: "a.ca".into(), text: REGISTRY.into() },
        SourceFile { path: "v.cav".into(), text: views.into() },
    ]);
    assert!(c.diagnostics.iter().all(|d| d.severity != Severity::Error), "{:?}", c.diagnostics);
    build(&c, None).0
}

#[test]
fn a_dense_diagram_leaves_its_labels_to_the_tooltips() {
    let outputs = outputs("view v { select app, saas\n  edges [flow]\n  render svg, mermaid, text }");
    let dot = &outputs.iter().find(|o| o.format == "svg").unwrap().content;
    assert!(!dot.contains("label=\"https/443\""), "{dot}");
    assert!(dot.contains("tooltip=\"from: hub -> svc-a : https/443"), "{dot}");
    let mermaid = &outputs.iter().find(|o| o.format == "mermaid").unwrap().content;
    assert!(!mermaid.contains("https/443"), "{mermaid}");
    // The text keeps them.
    let text = &outputs.iter().find(|o| o.format == "text").unwrap().content;
    assert!(text.contains("[https/443]"), "{text}");
}

#[test]
fn edge_labels_written_are_drawn() {
    let outputs = outputs("view v { select app, saas\n  edges [flow]\n  render svg { edge_labels: [port] } }");
    let dot = &outputs.iter().find(|o| o.format == "svg").unwrap().content;
    assert!(dot.contains("label=\"https/443\""), "{dot}");
}

#[test]
fn a_small_diagram_keeps_its_labels() {
    let outputs =
        outputs("view v { select app, saas where name in [hub, svc-a, svc-b]\n  edges [flow]\n  render svg }");
    let dot = &outputs.iter().find(|o| o.format == "svg").unwrap().content;
    assert!(dot.contains("label=\"https/443\""), "{dot}");
}
